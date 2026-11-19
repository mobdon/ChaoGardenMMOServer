//! Opt-in, bounded away competitions. Native tables are references for an
//! explicitly adapted simulation, never a claim that native routes are playable.
use crate::activities::ActivityCatalog;
use crate::competition_sim::{self, Competitor, Simulation, SimulationKind};
use crate::game::{ALL_STATS, Event, Game, Input, Player};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

pub const VERSION: u8 = 1;
pub const MAX_MATCHES: u8 = 3;
pub const GAP_MS: u32 = 60_000;
pub const HISTORY_LIMIT: usize = 10;

#[derive(Clone, Debug, Default)]
pub struct CompetitionCatalog {
    race: Option<(String, Value, Vec<Competitor>)>,
    karate: Option<Competitor>,
}

impl CompetitionCatalog {
    pub fn from_values(races: &Value, karate: &Value) -> Result<Self, String> {
        // Reuse the existing authoritative catalog validation. Only the first
        // audited references are offered until this vertical slice is accepted.
        let catalog = ActivityCatalog::from_values(races.clone(), karate.clone())?;
        let race = catalog
            .courses
            .iter()
            .find(|c| c.id == "beginner-0-1")
            .and_then(|course| {
                let bots = races.get("raceBots")?.as_array()?;
                let rivals: Vec<_> = bots
                    .iter()
                    .enumerate()
                    .skip(1)
                    .take(3)
                    .filter_map(|(i, bot)| competition_sim::rival_from_native(bot, i))
                    .collect();
                (rivals.len() == 3).then(|| (course.id.clone(), course.source.clone(), rivals))
            });
        let karate = catalog
            .karate
            .iter()
            .find(|t| t.id == "karate-beginner")
            .and_then(|t| {
                t.opponents
                    .iter()
                    .find(|o| o.id == "native-karate-opponent-2")
            })
            .map(competition_sim::competitor_from_karate);
        Ok(Self { race, karate })
    }

    pub fn public(&self) -> Value {
        json!({"version":VERSION,"rules":"adapted-ai-v1","maxMatches":MAX_MATCHES,
            "gapMs":GAP_MS,"race":self.race.is_some(),"karate":self.karate.is_some(),
            "raceCourseId":"beginner-0-1","karateOpponentId":"native-karate-opponent-2"})
    }

    fn simulation(
        &self,
        player: &Player,
        kind: SimulationKind,
        seed: u64,
    ) -> Option<(Simulation, Value)> {
        let owner = Competitor {
            id: player.id.clone(),
            name: player.name.clone(),
            stats: ALL_STATS.map(|s| player.stats[s].level),
            source: json!({"kind":"canonical-chao"}),
        };
        match kind {
            SimulationKind::Race => {
                let (id, source, rivals) = self.race.as_ref()?;
                Some((
                    competition_sim::simulate_race(seed, owner, rivals.clone()),
                    json!({"courseId":id,"source":source,"geometryUsed":false}),
                ))
            }
            SimulationKind::Karate => Some((
                competition_sim::simulate_karate(seed, owner, self.karate.clone()?),
                json!({"tierId":"karate-beginner","opponentId":"native-karate-opponent-2"}),
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub id: String,
    pub kind: SimulationKind,
    pub remaining: u8,
    pub wait_remaining_ms: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub kind: SimulationKind,
    pub elapsed_ms: u32,
    seed: u64,
    pub simulation: Simulation,
    reference: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MatchResult {
    pub id: String,
    pub kind: SimulationKind,
    pub rank: u8,
    pub rings: u32,
    pub earned_rings: u32,
    pub simulation: Simulation,
    reference: Value,
    seed: u64,
}

impl MatchResult {
    fn public(&self) -> Value {
        json!({"id":self.id,"kind":self.kind,"rank":self.rank,"rings":self.rings,
            "earnedRings":self.earned_rings,"durationMs":self.simulation.duration_ms,
            "placements":self.simulation.placements,"reason":self.simulation.reason,
            "saved":true,"replayAvailable":true,"rulesVersion":self.simulation.rules_version})
    }
}

#[derive(Clone, Debug)]
struct Pending {
    next: Box<CompetitionState>,
    reward: u32,
    result_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompetitionState {
    pub version: u8,
    pub revision: u64,
    pub plan: Option<Plan>,
    pub active: Option<Entry>,
    pub results: Vec<MatchResult>,
    pub completed_total: u64,
    #[serde(skip)]
    pending: Option<Pending>,
    #[serde(skip)]
    save_failed: bool,
}

impl Default for CompetitionState {
    fn default() -> Self {
        Self {
            version: VERSION,
            revision: 0,
            plan: None,
            active: None,
            results: vec![],
            completed_total: 0,
            pending: None,
            save_failed: false,
        }
    }
}

pub fn restore(value: &Value, owner: &str) -> Result<CompetitionState, String> {
    if value.is_null() {
        return Ok(CompetitionState::default());
    }
    let state: CompetitionState = serde_json::from_value(value.clone())
        .map_err(|e| format!("Invalid competition save: {e}"))?;
    if state.version != VERSION
        || state.revision >= u64::MAX - 10
        || state.results.len() > HISTORY_LIMIT
        || state.completed_total < state.results.len() as u64
        || state.plan.as_ref().is_some_and(|p| {
            Uuid::parse_str(&p.id).is_err()
                || p.remaining >= MAX_MATCHES
                || p.wait_remaining_ms > GAP_MS
                || (state.active.is_none() && p.remaining == 0)
        })
        || state.active.as_ref().is_some_and(|a| {
            Uuid::parse_str(&a.id).is_err()
                || a.elapsed_ms > a.simulation.duration_ms
                || state.plan.is_none()
                || a.kind != a.simulation.kind
                || state
                    .plan
                    .as_ref()
                    .is_some_and(|p| p.kind != a.kind || p.wait_remaining_ms != 0)
                || !valid_simulation(&a.simulation, owner, a.seed)
        })
        || state.results.iter().any(|r| {
            Uuid::parse_str(&r.id).is_err()
                || r.rings > 50
                || r.earned_rings > 50
                || r.kind != r.simulation.kind
                || r.simulation
                    .placements
                    .iter()
                    .find(|p| p.id == owner)
                    .is_none_or(|p| p.rank != r.rank)
                || r.earned_rings != payout(&r.kind, r.rank)
                || r.rings > r.earned_rings
                || !valid_simulation(&r.simulation, owner, r.seed)
        })
    {
        return Err("Invalid competition ledger bounds".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    if state.results.iter().any(|r| !ids.insert(r.id.as_str()))
        || state
            .active
            .as_ref()
            .is_some_and(|a| ids.contains(a.id.as_str()))
    {
        return Err("Duplicate competition receipt".into());
    }
    Ok(state)
}

fn valid_simulation(simulation: &Simulation, owner: &str, seed: u64) -> bool {
    let bounded = (1..=120_000).contains(&simulation.duration_ms)
        && (2..=4).contains(&simulation.competitors.len())
        && simulation
            .competitors
            .first()
            .is_some_and(|c| c.id == owner)
        && simulation
            .competitors
            .iter()
            .all(|c| c.stats.iter().all(|s| *s <= 99))
        && !simulation.frames.is_empty()
        && simulation.frames.len() <= 1000
        && simulation
            .frames
            .iter()
            .all(|f| f.elapsed_ms <= simulation.duration_ms)
        && simulation.placements.len() == simulation.competitors.len();
    if !bounded || simulation.rules_version != competition_sim::RULES_VERSION {
        return false;
    }
    let owner = simulation.competitors[0].clone();
    let expected = match simulation.kind {
        SimulationKind::Race => {
            competition_sim::simulate_race(seed, owner, simulation.competitors[1..].to_vec())
        }
        SimulationKind::Karate if simulation.competitors.len() == 2 => {
            competition_sim::simulate_karate(seed, owner, simulation.competitors[1].clone())
        }
        _ => return false,
    };
    expected == *simulation
}

fn payout(kind: &SimulationKind, rank: u8) -> u32 {
    match kind {
        SimulationKind::Race => match rank {
            1 => 25,
            2 => 15,
            3 => 5,
            _ => 0,
        },
        SimulationKind::Karate => {
            if rank == 1 {
                50
            } else {
                0
            }
        }
    }
}

pub fn reserved(player: &Player) -> bool {
    player.competition.plan.is_some() || player.competition.pending.is_some()
}

pub fn training_conflict(player: &Player) -> bool {
    player.care.activity_lock.is_some()
        || player.activity_lease.is_some()
        || ["training", "trainingJob", "trainingLedger"]
            .iter()
            .any(|key| {
                player
                    .profile_extra
                    .get(*key)
                    .is_some_and(|v| !v.is_null() && *v != false)
            })
}

pub fn lease(plan: &Plan) -> Value {
    json!({"kind":"competition","id":plan.id})
}

pub fn public(player: &Player) -> Value {
    let state = &player.competition;
    let visible = state.pending.as_ref().map_or(state, |p| &p.next);
    let active = visible.active.as_ref().map(|entry| {
        let frame = entry.simulation.frames.iter().rev().find(|f| f.elapsed_ms <= entry.elapsed_ms);
        json!({"id":entry.id,"kind":entry.kind,"elapsedMs":entry.elapsed_ms,
            "durationMs":entry.simulation.duration_ms,"phase":if state.pending.is_some(){"saving"}else{"running"},
            "competitors":entry.simulation.competitors.iter().map(|c| json!({"id":c.id,"name":c.name,"stats":c.stats})).collect::<Vec<_>>(),
            "currentFrame":frame,"rulesVersion":entry.simulation.rules_version})
    });
    json!({"version":VERSION,"revision":state.revision,"plan":visible.plan,"active":active,
        "saving":state.pending.is_some(),"completedTotal":state.completed_total,
        "results":state.results.iter().map(MatchResult::public).collect::<Vec<_>>()})
}

fn stage(
    game: &mut Game,
    player: &mut Player,
    mut next: CompetitionState,
    reward: u32,
    result_id: Option<String>,
) {
    next.revision += 1;
    next.pending = None;
    next.save_failed = false;
    player.competition.pending = Some(Pending {
        next: Box::new(next),
        reward,
        result_id,
    });
    game.dirty = true;
}

fn make_entry(game: &Game, player: &Player, kind: SimulationKind) -> Option<Entry> {
    let seed = Uuid::new_v4().as_u128() as u64;
    let (simulation, reference) =
        game.competition_catalog
            .simulation(player, kind.clone(), seed)?;
    Some(Entry {
        id: Uuid::new_v4().to_string(),
        kind,
        elapsed_ms: 0,
        seed,
        simulation,
        reference,
    })
}

pub fn action(game: &mut Game, player: &mut Player, message: &Value) {
    let action = message["action"].as_str().unwrap_or_default();
    let allowed: &[&str] = match action {
        "competition-start" => &["type", "action", "expectedRevision", "kind", "matches"],
        "competition-stop" => &["type", "action", "expectedRevision", "planId"],
        "competition-replay" => &["type", "action", "matchId", "profileId"],
        _ => {
            game.notice(&player.id, "Unknown competition intent.", "error");
            return;
        }
    };
    if !message
        .as_object()
        .is_some_and(|o| o.keys().all(|k| allowed.contains(&k.as_str())))
    {
        game.notice(&player.id, "Invalid competition intent.", "error");
        return;
    }
    if action == "competition-replay" {
        let target = message["profileId"].as_str().unwrap_or(&player.id);
        let other = if target == player.id {
            Some(&*player)
        } else {
            game.players.get(target)
        };
        if let Some(result) = other.and_then(|p| {
            p.competition
                .results
                .iter()
                .find(|r| message["matchId"] == r.id)
        }) {
            game.events.push(Event {target:Some(player.id.clone()),message:json!({"type":"event","kind":"competition-replay",
                "profileId":target,"result":result.public(),"simulation":result.simulation,"seed":result.seed,"reference":result.reference})});
        }
        return;
    }
    if player.competition.pending.is_some()
        || message["expectedRevision"].as_u64() != Some(player.competition.revision)
    {
        game.notice(
            &player.id,
            "Competition state changed or is waiting to save. Refresh before retrying.",
            "error",
        );
        return;
    }
    if action == "competition-stop" {
        if player
            .competition
            .plan
            .as_ref()
            .is_none_or(|p| message["planId"] != p.id)
        {
            return;
        }
        let mut next = player.competition.clone();
        next.plan = None;
        next.active = None;
        stage(game, player, next, 0, None);
        return;
    }
    if reserved(player)
        || training_conflict(player)
        || player.tiny_game.is_some()
        || player.race.is_some()
        || player.karate.is_some()
        || player.vehicle.is_some()
        || !["hub", "garden", "hero", "dark"].contains(&player.zone.as_str())
    {
        game.notice(
            &player.id,
            "Finish your current activity or training before opting into competitions.",
            "error",
        );
        return;
    }
    let Some(matches) = message["matches"]
        .as_u64()
        .filter(|n| (1..=u64::from(MAX_MATCHES)).contains(n))
    else {
        game.notice(&player.id, "Choose one to three matches.", "error");
        return;
    };
    let Ok(kind) = serde_json::from_value::<SimulationKind>(message["kind"].clone()) else {
        return;
    };
    let Some(entry) = make_entry(game, player, kind.clone()) else {
        game.notice(
            &player.id,
            "The audited competition reference is unavailable on this server.",
            "error",
        );
        return;
    };
    let mut next = player.competition.clone();
    next.plan = Some(Plan {
        id: Uuid::new_v4().to_string(),
        kind,
        remaining: matches as u8 - 1,
        wait_remaining_ms: 0,
    });
    next.active = Some(entry);
    player.activity_lease = next.plan.as_ref().map(lease);
    player.input = Input::default();
    player.boosting = false;
    stage(game, player, next, 0, None);
}

/// Simulation time advances only while this authority is running. Persisted
/// elapsed time resumes; process downtime grants no catch-up or rewards.
pub fn tick(game: &mut Game, player: &mut Player, dt: f64) {
    if player.competition.pending.is_some() || player.competition.plan.is_none() {
        return;
    }
    if player
        .competition
        .plan
        .as_ref()
        .is_some_and(|p| player.activity_lease.as_ref() != Some(&lease(p)))
    {
        return;
    }
    let delta = (dt * 1000.).round() as u32;
    if let Some(entry) = &mut player.competition.active {
        entry.elapsed_ms = entry
            .elapsed_ms
            .saturating_add(delta)
            .min(entry.simulation.duration_ms);
        game.dirty = true;
        if entry.elapsed_ms < entry.simulation.duration_ms {
            return;
        }
        let entry = entry.clone();
        let rank = entry
            .simulation
            .placements
            .iter()
            .find(|p| p.id == player.id)
            .map_or(4, |p| p.rank);
        let earned = payout(&entry.kind, rank);
        let rings = earned.min(999999_u32.saturating_sub(player.rings));
        let mut next = player.competition.clone();
        next.active = None;
        if next.plan.as_ref().is_some_and(|p| p.remaining == 0) {
            next.plan = None;
        } else if let Some(plan) = &mut next.plan {
            plan.wait_remaining_ms = GAP_MS;
        }
        next.completed_total = next.completed_total.saturating_add(1);
        next.results.push(MatchResult {
            id: entry.id.clone(),
            kind: entry.kind,
            rank,
            rings,
            earned_rings: earned,
            simulation: entry.simulation,
            reference: entry.reference,
            seed: entry.seed,
        });
        if next.results.len() > HISTORY_LIMIT {
            next.results.remove(0);
        }
        stage(game, player, next, rings, Some(entry.id));
    } else {
        let plan = player.competition.plan.as_mut().unwrap();
        plan.wait_remaining_ms = plan.wait_remaining_ms.saturating_sub(delta);
        game.dirty = true;
        if plan.wait_remaining_ms > 0 {
            return;
        }
        let kind = plan.kind.clone();
        if let Some(entry) = make_entry(game, player, kind) {
            let mut next = player.competition.clone();
            next.plan.as_mut().unwrap().remaining -= 1;
            next.active = Some(entry);
            stage(game, player, next, 0, None);
        }
    }
}

pub fn project_profile(player: &Player, profile: &mut Value) {
    let state = &player.competition;
    let next = state.pending.as_ref().map_or(state, |p| &p.next);
    profile["competition"] = json!(next);
    profile["activityLease"] = projected_lease(player, next).unwrap_or(Value::Null);
    if let Some(pending) = &state.pending {
        profile["rings"] = json!(player.rings.saturating_add(pending.reward).min(999999));
    }
}

fn projected_lease(player: &Player, next: &CompetitionState) -> Option<Value> {
    if let Some(plan) = &next.plan {
        return Some(lease(plan));
    }
    let owned = player.competition.plan.as_ref().map(lease);
    if owned.is_some() && player.activity_lease == owned {
        None
    } else {
        player.activity_lease.clone()
    }
}

/// A conflicting subsystem must cancel/release a plan before acquiring its
/// canonical lease. Never silently reclaim a foreign lease during a save.
pub fn validate_reservations(game: &Game) -> Result<(), String> {
    for player in game.players.values() {
        let state = &player.competition;
        let next = state.pending.as_ref().map_or(state, |p| &p.next);
        if next
            .plan
            .as_ref()
            .is_some_and(|plan| player.activity_lease.as_ref() != Some(&lease(plan)))
        {
            return Err("Competition conflicts with current activity lease".into());
        }
    }
    Ok(())
}

pub fn has_pending(game: &Game) -> bool {
    game.players
        .values()
        .any(|p| p.competition.pending.is_some())
}

pub fn save_failed(game: &mut Game) {
    for player in game.players.values_mut() {
        if player.competition.pending.is_some() && !player.competition.save_failed {
            player.competition.save_failed = true;
            game.events.push(Event {target:Some(player.id.clone()),message:json!({"type":"notice","kind":"error",
                "text":"Your competition is waiting to save. It will retry without granting duplicate rewards."})});
        }
    }
}

pub fn commit(game: &mut Game) {
    for player in game.players.values_mut() {
        let Some(pending) = player.competition.pending.take() else {
            continue;
        };
        let next_lease = projected_lease(player, &pending.next);
        player.rings = player.rings.saturating_add(pending.reward).min(999999);
        player.competition = *pending.next;
        player.activity_lease = next_lease;
        if let Some(id) = pending.result_id {
            let result = player
                .competition
                .results
                .iter()
                .find(|r| r.id == id)
                .unwrap();
            game.events.push(Event {
                target: None,
                message: json!({"type":"event","kind":"competition-result",
                "profileId":player.id,"result":result.public()}),
            });
        }
    }
}
