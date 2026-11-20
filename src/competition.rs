//! Opt-in, bounded away competitions. Outcomes come from the native Chao Race
//! and Karate rules (`competition_native`), driven by the MMO layer
//! (`competition_mmo`) for everything the server does not run natively.
use crate::competition_mmo::{self as mmo, Competitor, MmoConfig, Simulation, SimulationKind};
use crate::competition_native::{MslRand, NativeTables, karate, race};
use crate::emblems::{self, Emblem};
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
    tables: Option<NativeTables>,
    /// MsgChaoKarateName (from the local karate catalog), by name index.
    names: Vec<String>,
    config: MmoConfig,
}

impl CompetitionCatalog {
    /// `native` is CHAO_DATA_DIR/activities/competition-native.json (client CLI
    /// `chao-competition-tables` on the owner's sonic2app.exe); `config` is the
    /// optional MMO tuning file. Without the native tables nothing is offered.
    pub fn from_values(karate: &Value, native: &Value, config: &Value) -> Result<Self, String> {
        let tables = if native.is_null() || native.as_object().is_some_and(|o| o.is_empty()) {
            None
        } else {
            Some(NativeTables::from_value(native)?)
        };
        let names = karate
            .get("names")
            .and_then(Value::as_array)
            .map(|n| {
                n.iter()
                    .take(64)
                    .map(|v| v.as_str().unwrap_or_default().chars().take(32).collect())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            tables,
            names,
            config: MmoConfig::from_value(config)?,
        })
    }

    pub fn public(&self) -> Value {
        let c = &self.config;
        let class = match c.race_class {
            race::Class::Beginner => "beginner",
            race::Class::Jewel => "jewel",
            race::Class::Challenge => "challenge",
            race::Class::Hero => "hero",
            race::Class::Dark => "dark",
        };
        json!({"version":VERSION,"rules":mmo::RULES_VERSION,"maxMatches":MAX_MATCHES,
            "gapMs":GAP_MS,"race":self.tables.is_some(),"karate":self.tables.is_some(),
            "raceCourseId":format!("{class}-{}-{}", c.race_course, c.race_level + 1),
            "karateClass":c.karate_class,"karateRounds":karate::ROUNDS})
    }

    pub fn config(&self) -> &MmoConfig {
        &self.config
    }

    fn simulation(
        &self,
        player: &Player,
        others: &[Competitor],
        kind: SimulationKind,
        seed: u64,
    ) -> Option<(Simulation, Value)> {
        let tables = self.tables.as_ref()?;
        let cfg = &self.config;
        let owner = Competitor {
            id: player.id.clone(),
            name: player.name.clone(),
            stats: ALL_STATS.map(|s| player.stats[s].level),
            skills: ALL_STATS.map(|s| mmo::skill_points(player.stats[s].points)),
            source: json!({"kind":"canonical-chao"}),
        };
        match kind {
            SimulationKind::Race => {
                let mut rng = MslRand { next: seed as u32 };
                let mut field = vec![owner];
                let mut rivals = mmo::native_rivals(tables, cfg, &mut rng)?;
                // MMO: other players' Chao take the first lanes when configured.
                for (lane, other) in others
                    .iter()
                    .take(usize::from(cfg.race_player_rivals))
                    .enumerate()
                {
                    rivals[lane] = other.clone();
                }
                field.extend(rivals);
                let sim = mmo::simulate_race(u64::from(rng.next), tables, cfg, field)?;
                Some((
                    sim,
                    json!({"class":cfg.race_class,"course":cfg.race_course,"level":cfg.race_level}),
                ))
            }
            SimulationKind::Karate => {
                let round = player.competition.karate_round.min(karate::ROUNDS - 1);
                let mut rng = MslRand { next: seed as u32 };
                let (opponent, d) =
                    mmo::karate_opponent(tables, &self.names, cfg.karate_class, round, &mut rng)?;
                let native = json!({"class":cfg.karate_class,"round":round,
                    "descriptor":opponent.source["descriptor"]});
                let child = player.body.is_child();
                let sim = mmo::simulate_karate(
                    u64::from(rng.next),
                    cfg,
                    owner,
                    child,
                    opponent,
                    d.kind,
                    native.clone(),
                );
                Some((sim, native))
            }
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
    /// Native award: race progress/medal/unlock/emblems or karate round/emblem.
    #[serde(default)]
    pub award: Value,
}

impl MatchResult {
    fn public(&self) -> Value {
        json!({"id":self.id,"kind":self.kind,"rank":self.rank,"rings":self.rings,
            "earnedRings":self.earned_rings,"durationMs":self.simulation.duration_ms,
            "placements":self.simulation.placements,"reason":self.simulation.reason,
            "saved":true,"replayAvailable":true,"rulesVersion":self.simulation.rules_version,
            "award":self.award})
    }
}

#[derive(Clone, Debug)]
struct Pending {
    next: Box<CompetitionState>,
    reward: u32,
    result_id: Option<String>,
    emblems: Vec<Emblem>,
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
    /// Karate rounds won in a row in the configured class (fn_72_54A4 +7).
    #[serde(default)]
    pub karate_round: u8,
    /// Native race progress bytes (CM fn_60_24ABC).
    #[serde(default)]
    pub race_progress: race::Progress,
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
            karate_round: 0,
            race_progress: race::Progress::default(),
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
                || r.rings > 1000
                || r.earned_rings > 1000
                || r.kind != r.simulation.kind
                || r.simulation
                    .placements
                    .iter()
                    .find(|p| p.id == owner)
                    .is_none_or(|p| p.rank != r.rank)
                || r.earned_rings != payout(&r.simulation, r.rank)
                || r.rings > r.earned_rings
                || !valid_simulation(&r.simulation, owner, r.seed)
        })
    {
        return Err("Invalid competition ledger bounds".into());
    }
    if state.karate_round >= karate::ROUNDS {
        return Err("Invalid karate round".into());
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

/// Structural checks. Results are not re-run on load (the native tables live
/// outside the save), so a saved result is bounded, not replayed.
fn valid_simulation(simulation: &Simulation, owner: &str, _seed: u64) -> bool {
    let current = simulation.rules_version == mmo::RULES_VERSION;
    let legacy = simulation.rules_version == mmo::LEGACY_RULES_VERSION;
    let lanes = if current { 2..=race::LANES } else { 2..=4 };
    let ids: std::collections::BTreeSet<_> = simulation
        .competitors
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    (current || legacy)
        && (1..=3_600_000).contains(&simulation.duration_ms)
        && lanes.contains(&simulation.competitors.len())
        && ids.len() == simulation.competitors.len()
        && simulation
            .competitors
            .first()
            .is_some_and(|c| c.id == owner)
        && simulation
            .competitors
            .iter()
            .all(|c| c.stats.iter().all(|s| *s <= 99))
        && !simulation.frames.is_empty()
        && simulation.frames.len() <= 20_000
        && simulation.frames.iter().all(|f| {
            f.elapsed_ms <= simulation.duration_ms
                && f.competitors.len() == simulation.competitors.len()
                && f.competitors
                    .iter()
                    .zip(&simulation.competitors)
                    .all(|(c, who)| {
                        c.progress <= 10_000
                            && c.id == who.id
                            && match simulation.kind {
                                // Zeal never exceeds 1000; HP is at most 1000 + stamina skill.
                                SimulationKind::Karate => {
                                    c.stamina <= 1000 && c.hp <= 1000 + 65_535
                                }
                                // Race stamina starts at fn_76_1764's maximum and only falls.
                                SimulationKind::Race => {
                                    c.hp == 0
                                        && i64::from(c.stamina)
                                            <= i64::from(race::start_stamina(race::race_skill(
                                                u32::from(who.skills[4]),
                                            )))
                                            .max(i64::from(legacy) * 100)
                                }
                            }
                    })
        })
        && simulation.placements.len() == simulation.competitors.len()
        && simulation
            .placements
            .iter()
            .all(|p| ids.contains(p.id.as_str()) && (1..=8).contains(&p.rank))
        && (simulation.kind != SimulationKind::Karate || simulation.competitors.len() == 2)
        && simulation.config.as_ref().is_none_or(MmoConfig::valid)
        && (legacy || simulation.config.is_some())
}

/// MMO payout from the config the result was made with (legacy results use
/// the default table, which equals the old one).
fn payout(simulation: &Simulation, rank: u8) -> u32 {
    let legacy = MmoConfig::default();
    let cfg = simulation.config.as_ref().unwrap_or(&legacy);
    match simulation.kind {
        SimulationKind::Race => cfg
            .race_rings
            .get(usize::from(rank).wrapping_sub(1))
            .copied()
            .unwrap_or(0),
        SimulationKind::Karate => {
            if rank == 1 {
                cfg.karate_rings
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
        emblems: vec![],
    });
    game.dirty = true;
}

fn make_entry(game: &Game, player: &Player, kind: SimulationKind) -> Option<Entry> {
    let seed = Uuid::new_v4().as_u128() as u64;
    // MMO: other players' Chao (by id order) for the configured player lanes.
    let others: Vec<Competitor> = game
        .players
        .values()
        .filter(|p| p.id != player.id)
        .take(usize::from(
            game.competition_catalog.config().race_player_rivals,
        ))
        .map(|p| Competitor {
            id: p.id.clone(),
            name: p.name.clone(),
            stats: ALL_STATS.map(|s| p.stats[s].level),
            skills: ALL_STATS.map(|s| mmo::skill_points(p.stats[s].points)),
            source: json!({"kind":"player-chao","mmo":true}),
        })
        .collect();
    let (simulation, reference) =
        game.competition_catalog
            .simulation(player, &others, kind.clone(), seed)?;
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
        let earned = payout(&entry.simulation, rank);
        let rings = earned.min(999999_u32.saturating_sub(player.rings));
        let mut next = player.competition.clone();
        next.active = None;
        let (award, new_emblems) = native_award(game, player, &mut next, &entry, rank);
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
            award,
        });
        if next.results.len() > HISTORY_LIMIT {
            next.results.remove(0);
        }
        stage(game, player, next, rings, Some(entry.id));
        if let Some(p) = &mut player.competition.pending {
            p.emblems = new_emblems;
        }
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

/// Native results of a finished match: race wins record progress, medals,
/// unlocks and emblems (fn_76_698); karate counts rounds and five wins in a
/// row earn the class emblem (fn_72_5D48). Losing a round restarts the class
/// (MMO: the native loss path is not traced).
fn native_award(
    game: &Game,
    player: &Player,
    next: &mut CompetitionState,
    entry: &Entry,
    rank: u8,
) -> (Value, Vec<Emblem>) {
    let Some(cfg) = entry.simulation.config.as_ref() else {
        return (Value::Null, vec![]);
    };
    match entry.kind {
        SimulationKind::Race => {
            if rank != 1 {
                return (json!({"win":false}), vec![]);
            }
            let row = cfg.race_class.row(cfg.race_course, cfg.race_level);
            let Some(row) = game
                .competition_catalog
                .tables
                .as_ref()
                .zip(row)
                .and_then(|(t, row)| t.race_courses.get(row))
            else {
                return (Value::Null, vec![]);
            };
            let have = player.care.emblems.race;
            let award = race::win(
                &mut next.race_progress,
                &have,
                cfg.race_class,
                cfg.race_course,
                cfg.race_level,
                row,
            );
            let list = award.emblems.iter().map(|i| Emblem::ChaoRace(*i)).collect();
            (
                json!({"win":true,"award":award,"progress":next.race_progress}),
                list,
            )
        }
        SimulationKind::Karate => {
            if rank != 1 {
                next.karate_round = 0;
                return (json!({"win":false,"round":0}), vec![]);
            }
            next.karate_round += 1;
            if next.karate_round < karate::ROUNDS {
                return (json!({"win":true,"round":next.karate_round}), vec![]);
            }
            next.karate_round = 0;
            let class = usize::from(cfg.karate_class);
            let fresh = !player.care.emblems.karate[class];
            (
                json!({"win":true,"classCleared":cfg.karate_class,"emblem":fresh}),
                if fresh {
                    vec![Emblem::ChaoKarate(class)]
                } else {
                    vec![]
                },
            )
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
        for emblem in &pending.emblems {
            emblems::award(&mut player.care.emblems, *emblem);
        }
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
