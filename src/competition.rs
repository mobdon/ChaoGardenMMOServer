//! Away Chao Races and Karate bouts: native only, so currently unavailable.
//!
//! The native rules are ported in `competition_native` (GameCube SA2B, with
//! addresses). A native race or bout result also needs routines that are not
//! ported yet: the race course geometry, path following and movement
//! integrator (main.dol MOV_Control), and the karate motion lengths, fighter
//! movement and ring-out. Nothing is substituted for them, so starting a
//! competition fails closed with that reason. No non-native outcome, timing or
//! ring payout exists in this module.
//!
//! Saved profiles from the earlier adapted simulation still load: their
//! history is kept as data, and an unfinished plan is cancelled without a
//! payout on the next tick, releasing its activity lease.
use crate::competition_native::NativeTables;
use crate::game::{Event, Game, Player};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

pub const VERSION: u8 = 1;
pub const HISTORY_LIMIT: usize = 10;
/// The earlier adapted simulation's rules label; only its saved data loads.
pub const LEGACY_RULES_VERSION: &str = "adapted-ai-competition-v1";
/// Legacy save bounds (the old plan budget, gap and per-result ring cap), used
/// only to validate saved data.
const LEGACY_MAX_MATCHES: u8 = 3;
const LEGACY_GAP_MS: u32 = 60_000;
const LEGACY_MAX_RINGS: u32 = 50;

/// Why no competition can start.
pub const UNAVAILABLE: &str = "Native Chao Race and Karate results need the course geometry, \
path following and movement integrator (race) and the motion lengths, fighter movement and \
ring-out (karate), which are not ported yet. No substitute is used.";

#[derive(Clone, Debug, Default)]
pub struct CompetitionCatalog {
    tables: Option<NativeTables>,
}

impl CompetitionCatalog {
    /// `native` is CHAO_DATA_DIR/activities/competition-native.json (client CLI
    /// `chao-competition-tables` on the owner's sonic2app.exe).
    pub fn from_values(native: &Value) -> Result<Self, String> {
        let tables = if native.is_null() || native.as_object().is_some_and(|o| o.is_empty()) {
            None
        } else {
            Some(NativeTables::from_value(native)?)
        };
        Ok(Self { tables })
    }

    pub fn tables(&self) -> Option<&NativeTables> {
        self.tables.as_ref()
    }

    pub fn public(&self) -> Value {
        json!({"version":VERSION,"rules":"native-gsne8p","race":false,"karate":false,
            "nativeTables":self.tables.is_some(),"reason":UNAVAILABLE})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SimulationKind {
    Race,
    Karate,
}

/// Saved-data shapes of the earlier adapted simulation (kept so old profiles load).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Competitor {
    pub id: String,
    pub name: String,
    pub stats: [u16; 7],
    pub source: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub id: String,
    pub rank: u8,
    pub finish_ms: Option<u32>,
    pub remaining_hp: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrameCompetitor {
    pub id: String,
    pub progress: u32,
    pub hp: u32,
    pub stamina: u32,
    pub action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SimFrame {
    pub elapsed_ms: u32,
    pub competitors: Vec<FrameCompetitor>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Simulation {
    pub kind: SimulationKind,
    pub rules_version: String,
    pub duration_ms: u32,
    pub competitors: Vec<Competitor>,
    pub frames: Vec<SimFrame>,
    pub placements: Vec<Placement>,
    pub reason: String,
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
                || p.remaining >= LEGACY_MAX_MATCHES
                || p.wait_remaining_ms > LEGACY_GAP_MS
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
                || !valid_simulation(&a.simulation, owner)
        })
        || state.results.iter().any(|r| {
            Uuid::parse_str(&r.id).is_err()
                || r.rings > LEGACY_MAX_RINGS
                || r.earned_rings > LEGACY_MAX_RINGS
                || r.rings > r.earned_rings
                || r.kind != r.simulation.kind
                || r.simulation
                    .placements
                    .iter()
                    .find(|p| p.id == owner)
                    .is_none_or(|p| p.rank != r.rank)
                || !valid_simulation(&r.simulation, owner)
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

/// Structural bounds for saved legacy data (nothing is replayed or re-run).
fn valid_simulation(simulation: &Simulation, owner: &str) -> bool {
    let ids: std::collections::BTreeSet<_> = simulation
        .competitors
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    simulation.rules_version == LEGACY_RULES_VERSION
        && (1..=120_000).contains(&simulation.duration_ms)
        && (2..=4).contains(&simulation.competitors.len())
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
        && simulation.frames.len() <= 1000
        && simulation.frames.iter().all(|f| {
            f.elapsed_ms <= simulation.duration_ms
                && f.competitors.len() == simulation.competitors.len()
                && f.competitors
                    .iter()
                    .zip(&simulation.competitors)
                    .all(|(c, who)| {
                        c.id == who.id && c.progress <= 10_000 && c.hp <= 199 && c.stamina <= 150
                    })
        })
        && simulation.placements.len() == simulation.competitors.len()
        && simulation
            .placements
            .iter()
            .all(|p| ids.contains(p.id.as_str()) && (1..=4).contains(&p.rank))
        && (simulation.kind != SimulationKind::Karate || simulation.competitors.len() == 2)
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
    json!({"version":VERSION,"revision":state.revision,"plan":visible.plan,"active":Value::Null,
        "saving":state.pending.is_some(),"completedTotal":state.completed_total,
        "available":false,"reason":UNAVAILABLE,
        "results":state.results.iter().map(MatchResult::public).collect::<Vec<_>>()})
}

fn stage(game: &mut Game, player: &mut Player, mut next: CompetitionState) {
    next.revision += 1;
    next.pending = None;
    next.save_failed = false;
    player.competition.pending = Some(Pending {
        next: Box::new(next),
    });
    game.dirty = true;
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
    if action == "competition-start" {
        game.notice(&player.id, UNAVAILABLE, "error");
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
    stage(game, player, next);
}

/// A saved legacy plan cannot run natively: cancel it without a payout and
/// release its lease (only while this authority holds that lease).
pub fn tick(game: &mut Game, player: &mut Player, _dt: f64) {
    if player.competition.pending.is_some() {
        return;
    }
    let Some(plan) = &player.competition.plan else {
        return;
    };
    if player.activity_lease.as_ref() != Some(&lease(plan)) {
        return;
    }
    let mut next = player.competition.clone();
    next.plan = None;
    next.active = None;
    stage(game, player, next);
    game.notice(&player.id, UNAVAILABLE, "info");
}

pub fn project_profile(player: &Player, profile: &mut Value) {
    let state = &player.competition;
    let next = state.pending.as_ref().map_or(state, |p| &p.next);
    profile["competition"] = json!(next);
    profile["activityLease"] = projected_lease(player, next).unwrap_or(Value::Null);
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
            game.events.push(Event {
                target: Some(player.id.clone()),
                message: json!({"type":"notice","kind":"error",
                "text":"Your competition change is waiting to save. It will retry."}),
            });
        }
    }
}

pub fn commit(game: &mut Game) {
    for player in game.players.values_mut() {
        let Some(pending) = player.competition.pending.take() else {
            continue;
        };
        let next_lease = projected_lease(player, &pending.next);
        player.competition = *pending.next;
        player.activity_lease = next_lease;
    }
}
