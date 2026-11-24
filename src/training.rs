//! Durable host adapter for the existing pure, explicitly adapted training core.
use crate::{
    care::{Intent, MAX_RECEIPTS},
    game::{Game, Input, Player},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiny_garden_core::training::{
    Stat, TrainingError, TrainingLedger, TrainingPolicy, ring_quote, ring_rate,
};

pub const RULES: &str = "adapted-afk-training-v2";
const LOCK_PREFIX: &str = "training:afk:";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrainingResult {
    pub job_id: u64,
    pub stat: Stat,
    pub outcome: Outcome,
    pub awarded_points: u32,
    pub applied_points: u32,
    pub completed_server_ms: u64,
    pub receipt_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Settled,
    Canceled,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrainingState {
    version: u32,
    ledger: Value,
    pub last_result: Option<TrainingResult>,
}

fn code(error: TrainingError) -> &'static str {
    match error {
        TrainingError::InvalidDuration | TrainingError::InvalidPolicy => {
            "invalid-training-duration"
        }
        TrainingError::ActiveJob => "training-active",
        TrainingError::MissingJob => "training-unavailable",
        TrainingError::WrongJob => "training-job-mismatch",
        TrainingError::JobClosed => "training-closed",
        TrainingError::NotReady { .. } => "training-not-ready",
        TrainingError::ReadyToSettle => "training-ready-settle",
        TrainingError::ClockMovedBackwards => "training-clock-error",
        _ => "training-state-invalid",
    }
}
fn save_ledger(ledger: &TrainingLedger) -> Result<Value, &'static str> {
    serde_json::from_str(&ledger.to_stored_json().map_err(code)?)
        .map_err(|_| "training-state-invalid")
}
impl TrainingState {
    fn core(&self, id: &str) -> Result<TrainingLedger, &'static str> {
        if self.version != 1 {
            return Err("training-state-invalid");
        }
        TrainingLedger::from_trusted_json(&self.ledger.to_string(), id).map_err(code)
    }
    pub fn validate(&self, expected_id: Option<&str>) -> Result<(), &'static str> {
        let id = expected_id
            .or_else(|| self.ledger["targetChaoId"].as_str())
            .ok_or("training-state-invalid")?;
        let core = self.core(id)?;
        if self.ledger["job"].is_object()
            && self.ledger["job"]["policy"]
                != serde_json::to_value(TrainingPolicy::default())
                    .map_err(|_| "training-state-invalid")?
        {
            return Err("training-state-invalid");
        }
        let at = self.ledger["lastServerMs"]
            .as_u64()
            .ok_or("training-state-invalid")?;
        let view = core.view(at).map_err(code)?;
        match (&view, &self.last_result) {
            (Some(job), Some(result))
                if matches!(job.phase, "settled" | "canceled")
                    && result.job_id == job.job_id
                    && result.stat == job.stat
                    && result.applied_points <= result.awarded_points
                    && result.awarded_points <= tiny_garden_core::training::MAX_AWARD_POINTS
                    && result.completed_server_ms == at
                    && self.ledger["job"]["status"]["server_ms"] == json!(at)
                    && uuid::Uuid::parse_str(&result.receipt_id)
                        .is_ok_and(|id| id.hyphenated().to_string() == result.receipt_id)
                    && match result.outcome {
                        Outcome::Settled => {
                            job.phase == "settled" && result.awarded_points == job.projected_points
                        }
                        Outcome::Canceled => {
                            job.phase == "canceled"
                                && result.awarded_points == 0
                                && result.applied_points == 0
                        }
                    } =>
            {
                Ok(())
            }
            (None, None) => Ok(()),
            (Some(job), None) if matches!(job.phase, "training" | "ready") => Ok(()),
            _ => Err("training-state-invalid"),
        }
    }
}

fn lock(job_id: u64) -> String {
    format!("{LOCK_PREFIX}{job_id}")
}
fn core(player: &Player) -> Result<TrainingLedger, &'static str> {
    match &player.care.training {
        Some(state) => state.core(&player.id),
        None => TrainingLedger::new(&player.id).map_err(code),
    }
}
pub fn validate_player(player: &Player) -> Result<(), String> {
    if let Some(state) = &player.care.training {
        state.validate(Some(&player.id)).map_err(str::to_owned)?;
        let ledger = state.core(&player.id).map_err(str::to_owned)?;
        let at = state.ledger["lastServerMs"]
            .as_u64()
            .ok_or("Invalid training clock")?;
        let active = ledger
            .view(at)
            .map_err(|_| "Invalid training view")?
            .filter(|job| matches!(job.phase, "training" | "ready"));
        if let Some(job) = active {
            if !player.care.tiny_resident {
                return Err("Training job lost its Tiny Garden opt-in".into());
            }
            if player.care.activity_lock.as_deref() != Some(lock(job.job_id).as_str()) {
                return Err("Training job is missing its activity lock".into());
            }
            if player.vehicle.is_some() || !capacity(player, 1) {
                return Err(
                    "Training job conflicts with its saved activity or closing capacity".into(),
                );
            }
        } else if player
            .care
            .activity_lock
            .as_ref()
            .is_some_and(|value| value.starts_with(LOCK_PREFIX))
        {
            return Err("Closed training job retained an activity lock".into());
        }
        let job = ledger
            .view(at)
            .map_err(|_| "Invalid training view")?
            .ok_or("Saved training state is missing its job")?;
        let duration = job.deadline_server_ms - job.started_server_ms;
        if !player.care.receipts.values().any(|receipt| {
            let Intent::TrainingStart {stat,duration_ms,quoted_cost} = &receipt.request.intent else {return false};
            if *stat != job.stat || *duration_ms != duration {return false;}
            match quoted_cost {
                None => receipt.effects == json!({"training":{"jobId":job.job_id,"outcome":"started","costRings":0}}),
                Some(cost) => (1..=4).any(|rate| ring_quote(duration,rate)==Some(*cost) && receipt.effects == start_effect(job.job_id,rate,*cost))
                    || fruit_receipt(&receipt.effects, job.job_id, duration, *cost)
            }
        }) {
            return Err("Training job is missing its durable start receipt".into());
        }
        if let Some(result) = &state.last_result {
            let receipt = player
                .care
                .receipts
                .get(&result.receipt_id)
                .ok_or("Training result is missing its durable receipt")?;
            let matches_intent = match result.outcome {
                Outcome::Settled => {
                    matches!(receipt.request.intent, Intent::TrainingSettle { job_id } if job_id == result.job_id)
                }
                Outcome::Canceled => {
                    matches!(receipt.request.intent, Intent::TrainingCancel { job_id } if job_id == result.job_id)
                }
            };
            if !matches_intent
                || receipt.effects
                    != json!({"training":result,"points":{stat_name(result.stat):result.applied_points},"levelsChanged":false,"gradesChanged":false})
            {
                return Err("Training result does not match its durable receipt".into());
            }
        }
    } else if player
        .care
        .activity_lock
        .as_ref()
        .is_some_and(|value| value.starts_with(LOCK_PREFIX))
    {
        return Err("Training activity lock has no durable job".into());
    }
    Ok(())
}
fn unsafe_activity(player: &Player) -> bool {
    player.tiny_game.is_some()
        || player.race.is_some()
        || player.karate.is_some()
        || player.vehicle.is_some()
        || !["hub", "garden", "hero", "dark"].contains(&player.zone.as_str())
}
fn capacity(player: &Player, reserve: usize) -> bool {
    let reserve = reserve
        + usize::from(
            player
                .tiny_collection
                .as_ref()
                .is_some_and(|state| state.pending_egg.is_some()),
        );
    player.care.receipts.len() <= MAX_RECEIPTS.saturating_sub(reserve)
        && player.care.revision <= u64::MAX - reserve as u64
}
fn stat_name(stat: Stat) -> &'static str {
    match stat {
        Stat::Swim => "swim",
        Stat::Fly => "fly",
        Stat::Run => "run",
        Stat::Power => "power",
        Stat::Stamina => "stamina",
    }
}
fn start_effect(job: u64, rate: u32, cost: u32) -> Value {
    json!({"rings":-i64::from(cost),"training":{"jobId":job,"outcome":"started","costRings":cost,"ratePerMinute":rate,"tariff":"skill-band-v1"}})
}
/// MMO price (mob 2026-11-22): a trained point costs half of what the same
/// point costs as Tiny Garden fruit, because training takes time. The fruit is
/// the cheapest one per point for that skill; a fruit gives its byte x 10 points
/// (care.rs, tiny fruit). None when the fruit data is not loaded or no fruit
/// grows the skill; the skill band price is used then.
fn fruit(game: &Game, stat: Stat) -> Option<(u32, u32)> {
    let column = 2 + crate::game::MAIN_STATS
        .iter()
        .position(|name| *name == stat_name(stat))?;
    game.tiny_shop_catalog
        .as_ref()?
        .fruits
        .iter()
        .filter(|fruit| fruit.raw[column] > 0)
        .map(|fruit| (fruit.price, fruit.raw[column] as u32 * 10))
        .min_by_key(|(price, points)| (u64::from(*price) * 1000) / u64::from(*points))
}
/// Rings for `duration_ms` of training at one point a minute: half the fruit's
/// price for those points, rounded up, never free.
fn fruit_quote(duration_ms: u64, (price, points): (u32, u32)) -> Option<u32> {
    ring_quote(duration_ms, 1)?;
    let minutes = duration_ms / 60_000;
    u32::try_from(
        (minutes * u64::from(price))
            .div_ceil(2 * u64::from(points))
            .max(1),
    )
    .ok()
}
fn fruit_effect(job: u64, cost: u32, (price, points): (u32, u32)) -> Value {
    json!({"rings":-i64::from(cost),"training":{"jobId":job,"outcome":"started","costRings":cost,"fruitPrice":price,"fruitPoints":points,"tariff":"half-fruit-v1"}})
}
/// True when a saved start receipt is a half-fruit one whose cost follows from
/// the fruit numbers it recorded.
fn fruit_receipt(effects: &Value, job: u64, duration_ms: u64, cost: u32) -> bool {
    let number = |key| {
        effects["training"][key]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0)
    };
    let (Some(price), Some(points)) = (number("fruitPrice"), number("fruitPoints")) else {
        return false;
    };
    fruit_quote(duration_ms, (price, points)) == Some(cost)
        && *effects == fruit_effect(job, cost, (price, points))
}
/// The cost of a session and the receipt effect that records how it was priced.
fn price(
    game: &Game,
    player: &Player,
    stat: Stat,
    duration_ms: u64,
    job: u64,
) -> Option<(u32, Value)> {
    match fruit(game, stat) {
        Some(fruit) => {
            let cost = fruit_quote(duration_ms, fruit)?;
            Some((cost, fruit_effect(job, cost, fruit)))
        }
        None => {
            let rate = ring_rate(player.stats[stat_name(stat)].level);
            let cost = ring_quote(duration_ms, rate)?;
            Some((cost, start_effect(job, rate, cost)))
        }
    }
}
const MENU_MINUTES: [u64; 4] = [1, 15, 60, 480];
pub fn capability() -> Value {
    json!({"enabled":true,"rulesVersion":RULES,"policy":{"tickMs":60000,"pointsPerTick":1,"maxDurationMs":28800000,"maxPoints":480,"tariff":"skill-band-v1","cancelRefund":false,"menuMinutes":MENU_MINUTES},
        "stats":["swim","fly","run","power","stamina"]})
}
pub fn public(game: &Game, player: &Player, now: u64) -> Value {
    let mut result = capability();
    let view = core(player).and_then(|ledger| ledger.view(now).map_err(code));
    let (job, error) = match view {
        Ok(job) => (job, None),
        Err(code) => (None, Some(code)),
    };
    let active = job
        .as_ref()
        .is_some_and(|job| matches!(job.phase, "training" | "ready"));
    let owned = job
        .as_ref()
        .is_some_and(|job| player.care.activity_lock.as_deref() == Some(lock(job.job_id).as_str()));
    let common = player.connections > 0 && !unsafe_activity(player);
    let can_start = common
        && player.care.tiny_resident
        && player.care.activity_lock.is_none()
        && !active
        && capacity(player, 2)
        && error.is_none();
    let can_close =
        common && owned && player.watch.is_none() && capacity(player, 1) && error.is_none();
    let reason = error.or_else(|| {
        if !common {
            Some("activity-conflict")
        } else if !player.care.tiny_resident {
            Some("tiny-entry-required")
        } else if !capacity(player, if active { 1 } else { 2 }) {
            Some("receipt-capacity")
        } else if active {
            Some("training-active")
        } else if player.care.activity_lock.is_some() {
            Some("activity-conflict")
        } else {
            None
        }
    });
    let mut quotes = serde_json::Map::new();
    for stat in [Stat::Swim, Stat::Fly, Stat::Run, Stat::Power, Stat::Stamina] {
        quotes.insert(
            stat_name(stat).into(),
            json!(MENU_MINUTES.map(|minutes| {
                price(game, player, stat, minutes * 60000, 0).map(|(cost, _)| cost)
            })),
        );
    }
    result["quotes"] = Value::Object(quotes);
    result["serverNowMs"] = json!(now);
    result["job"] = json!(job);
    result["lastResult"] = json!(
        player
            .care
            .training
            .as_ref()
            .and_then(|state| state.last_result.as_ref())
    );
    result["eligibility"] = json!({"canStart":can_start,"canCancel":can_close&&job.as_ref().is_some_and(|job|job.phase=="training"),
        "canSettle":can_close&&job.as_ref().is_some_and(|job|job.phase=="ready"),"reason":reason});
    result
}

pub fn ready_job(player: &Player, now: u64) -> Option<u64> {
    let job = core(player).ok()?.view(now).ok()??;
    (job.phase == "ready"
        && !unsafe_activity(player)
        && player.watch.is_none()
        && capacity(player, 1)
        && player.care.activity_lock.as_deref() == Some(lock(job.job_id).as_str()))
    .then_some(job.job_id)
}

pub fn apply(
    game: &Game,
    player: &mut Player,
    intent: &Intent,
    now: u64,
    receipt_id: &str,
    automatic: bool,
) -> Result<Value, &'static str> {
    let mut ledger = core(player)?;
    let (last_result, effects) = match intent {
        Intent::TrainingStart {
            stat,
            duration_ms,
            quoted_cost,
        } => {
            if player.connections == 0
                || !player.care.tiny_resident
                || unsafe_activity(player)
                || player.care.activity_lock.is_some()
            {
                return Err("activity-conflict");
            }
            if !capacity(player, 2) {
                return Err("receipt-capacity");
            }
            let id = ledger
                .start(*stat, *duration_ms, now, TrainingPolicy::default())
                .map_err(code)?;
            let (cost, effect) =
                price(game, player, *stat, *duration_ms, id).ok_or("invalid-training-duration")?;
            if *quoted_cost != Some(cost) {
                return Err("training-price-changed");
            }
            player.rings = player.rings.checked_sub(cost).ok_or("insufficient-rings")?;
            player.care.activity_lock = Some(lock(id));
            player.watch = None;
            player.input = Input::default();
            (None, effect)
        }
        Intent::TrainingCancel { job_id } | Intent::TrainingSettle { job_id } => {
            if (!automatic && player.connections == 0)
                || unsafe_activity(player)
                || player.watch.is_some()
                || player.care.activity_lock.as_deref() != Some(lock(*job_id).as_str())
            {
                return Err("activity-conflict");
            }
            let job = ledger
                .view(now)
                .map_err(code)?
                .ok_or("training-unavailable")?;
            let (outcome, awarded, applied) = if matches!(intent, Intent::TrainingCancel { .. }) {
                ledger.cancel(*job_id, now).map_err(code)?;
                (Outcome::Canceled, 0, 0)
            } else {
                let award = ledger.settle(*job_id, now).map_err(code)?;
                let stat = player
                    .stats
                    .get_mut(stat_name(award.stat))
                    .ok_or("training-state-invalid")?;
                let before = stat.points;
                stat.points = stat.points.saturating_add(award.points).min(9999);
                (Outcome::Settled, award.points, stat.points - before)
            };
            let result = TrainingResult {
                job_id: *job_id,
                stat: job.stat,
                outcome,
                awarded_points: awarded,
                applied_points: applied,
                completed_server_ms: now,
                receipt_id: receipt_id.into(),
            };
            let effects = json!({"training":result,"points":{stat_name(job.stat):applied},"levelsChanged":false,"gradesChanged":false});
            player.care.activity_lock = None;
            (Some(result), effects)
        }
        _ => return Err("invalid-request"),
    };
    player.care.training = Some(TrainingState {
        version: 1,
        ledger: save_ledger(&ledger)?,
        last_result,
    });
    crate::watch::maintain_tiny(game, player);
    Ok(effects)
}
