//! Small custom resident routine; this does not execute retail Chao AI.
//! Behaviour selection lives in `crate::behaviour` (placeholder tuning until native values are decoded).
use crate::behaviour::{self, Behaviour, Choice, Kind, NeedsView};
use crate::game::{Game, Input, Player};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct WatchState {
    pub id: String,
    pub activity: &'static str,
    pub started_at: f64,
    pub activity_started_at: f64,
    pub activity_until: f64,
    pub behaviour: Behaviour,
    target: Option<(f64, f64)>,
    anchor: (f64, f64),
    decision: u32,
    salt: u64,
    last_care: Option<(&'static str, f64)>,
    next_checkpoint_at: f64,
}
impl WatchState {
    pub fn public(&self, owner_online: bool) -> Value {
        json!({"id":self.id,"mode":"watch","activity":self.activity,"startedAt":self.started_at,
            "activityStartedAt":self.activity_started_at,"activityUntil":self.activity_until,
            "target":self.target.map(|(x,z)|json!({"x":x,"z":z})),
            "lastCare":self.last_care.map(|(kind,at)|json!({"kind":kind,"at":at})),
            "behaviour":self.behaviour.public(),"resident":true,"ownerOnline":owner_online,"rules":"adapted-resident"})
    }
    fn state(&mut self, kind: Kind, now: f64, variant: u8) {
        self.behaviour = Behaviour::new(kind, variant, now, behaviour::duration(kind));
        self.sync(now);
        if kind != Kind::Wander {
            self.target = None;
        }
    }
    /// Keep the legacy `activity` fields in step with the behaviour.
    fn sync(&mut self, now: f64) {
        let activity = self.behaviour.kind.legacy();
        if activity != self.activity {
            self.activity = activity;
            self.activity_started_at = now;
        }
        self.activity_until = self.behaviour.until;
    }
    fn new(player: &Player, now: f64) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            activity: "idle",
            started_at: now,
            activity_started_at: now,
            activity_until: now + behaviour::tuning::IDLE_MS,
            behaviour: Behaviour::new(Kind::Idle, 0, now, behaviour::tuning::IDLE_MS),
            target: None,
            anchor: (player.x, player.z),
            decision: 0,
            salt: Uuid::new_v4().as_u128() as u64,
            last_care: None,
            next_checkpoint_at: now + 15000.,
        }
    }
}

pub fn restore(game: &Game, player: &mut Player, value: &Value) {
    if value["enabled"] != true {
        return;
    }
    let zone = match value["zone"].as_str() {
        Some("hero") if player.alignment >= 60. => "hero",
        Some("dark") if player.alignment <= -60. => "dark",
        _ => "garden",
    };
    let x = value["x"]
        .as_f64()
        .filter(|v| v.is_finite() && v.abs() <= 22.)
        .unwrap_or(0.);
    let z = value["z"]
        .as_f64()
        .filter(|v| v.is_finite() && v.abs() <= 22.)
        .unwrap_or(6.);
    if game.teleport(player, zone, x, z).is_err() {
        return;
    }
    player.vehicle = None;
    player.watch = Some(WatchState::new(player, game.time));
}

fn valid_intent(message: &Value, allowed: &[&str]) -> bool {
    message
        .as_object()
        .is_some_and(|object| object.keys().all(|key| allowed.contains(&key.as_str())))
}

pub fn action(game: &mut Game, player: &mut Player, message: &Value) {
    let action = message["action"].as_str().unwrap_or_default();
    let allowed = if action == "watch-start" {
        &["type", "action"][..]
    } else if action == "watch-stop" {
        &["type", "action", "sessionId"][..]
    } else {
        &["type", "action", "sessionId", "kind"][..]
    };
    if !valid_intent(message, allowed) {
        game.notice(&player.id, "Invalid watch intent.", "error");
        return;
    }
    if action == "watch-start" {
        if player.watch.is_some() {
            return;
        }
        if player.tiny_game.is_some()
            || player.race.is_some()
            || player.karate.is_some()
            || !["hub", "garden", "hero", "dark"].contains(&player.zone.as_str())
        {
            game.notice(
                &player.id,
                "Finish your current activity before watching your Chao.",
                "error",
            );
            return;
        }
        if player.vehicle.take().is_some() {
            game.dirty = true;
        }
        if player.zone == "hub" && game.teleport(player, "garden", 0., 6.).is_err() {
            return;
        }
        player.input = Input::default();
        player.vy = 0.;
        player.jump_offset = 0.;
        player.y = player.ground_y;
        player.boosting = false;
        player.watch = Some(WatchState::new(player, game.time));
        game.dirty = true;
        return;
    }
    let Some(watch) = &mut player.watch else {
        return;
    };
    if message["sessionId"] != watch.id {
        return;
    }
    if action == "watch-stop" {
        player.watch = None;
        player.input = Input::default();
        game.dirty = true;
        return;
    }
    if action != "watch-care" {
        return;
    }
    let Some(kind) = message["kind"]
        .as_str()
        .filter(|kind| ["pet", "rest", "wake"].contains(kind))
    else {
        game.notice(&player.id, "Unknown care interaction.", "error");
        return;
    };
    if !player.cooldown("watch-care", game.time, 1500.) {
        return;
    }
    let watch = player.watch.as_mut().unwrap();
    let now = game.time;
    let asleep = watch.behaviour.kind == Kind::Sleep;
    match kind {
        "pet" => {
            player.happiness = (player.happiness + 2.).min(100.);
            game.dirty = true;
            watch.last_care = Some(("pet", now));
            // Petting a sleeping Chao wakes it with a start; otherwise it is happy.
            watch.state(if asleep { Kind::Startled } else { Kind::Happy }, now, 0);
        }
        "rest" => {
            watch.last_care = Some(("rest", now));
            watch.state(Kind::Sleep, now, 0);
        }
        "wake" => {
            watch.last_care = Some(("wake", now));
            watch.state(if asleep { Kind::Startled } else { Kind::Idle }, now, 0);
        }
        _ => {}
    }
    player.input = Input::default();
}

fn target(game: &Game, player: &Player, watch: &WatchState) -> Option<(f64, f64)> {
    for attempt in 0..8 {
        let step = u64::from(watch.decision) + attempt;
        let angle =
            (watch.salt % 4096) as f64 / 4096. * std::f64::consts::TAU + step as f64 * 2.3999632297;
        let radius = 1. + ((watch.salt.wrapping_add(step)) % 5) as f64 * 0.36;
        let x = (watch.anchor.0 + angle.sin() * radius).clamp(-21., 21.);
        let z = (watch.anchor.1 + angle.cos() * radius).clamp(-21., 21.);
        let count = ((player.x - x).hypot(player.z - z) / 0.2).ceil().max(1.) as usize;
        let mut previous = game.player_ground(player, player.x, player.z)?;
        let safe = (1..=count).all(|index| {
            let t = index as f64 / count as f64;
            let Some(y) = game.player_ground(
                player,
                player.x + (x - player.x) * t,
                player.z + (z - player.z) * t,
            ) else {
                return false;
            };
            let valid = (y - previous).abs() <= 0.15;
            previous = y;
            valid
        });
        if safe {
            return Some((x, z));
        }
    }
    None
}

/// Feed a fresh server intent into the existing authoritative terrain/movement
/// path. Called after stale client input is cleared, once per canonical profile.
pub fn tick(game: &mut Game, player: &mut Player) {
    let Some(mut watch) = player.watch.take() else {
        return;
    };
    if game.time >= watch.next_checkpoint_at {
        game.dirty = true;
        watch.next_checkpoint_at = game.time + 15000.;
    }
    if watch.behaviour.advance(game.time) {
        if watch.behaviour.kind == Kind::Idle {
            watch.decision = watch.decision.wrapping_add(1);
            let needs = NeedsView::from_prototype(player.energy, player.happiness);
            let (choice, variant) = behaviour::choose(needs, watch.salt, watch.decision);
            match choice {
                Choice::Rest(kind) => watch.state(kind, game.time, variant),
                Choice::Play => watch.state(Kind::Play, game.time, variant),
                Choice::Wander => {
                    if let Some(destination) = target(game, player, &watch) {
                        watch.target = Some(destination);
                        watch.state(Kind::Wander, game.time, variant);
                    } else {
                        watch.state(Kind::Idle, game.time, variant);
                    }
                }
                Choice::Idle => watch.state(Kind::Idle, game.time, variant),
            }
        } else {
            let variant = (behaviour::mix(watch.salt, watch.decision) >> 48) as u8;
            watch.state(Kind::Idle, game.time, variant);
        }
    }
    watch.sync(game.time);
    player.input = Input::default();
    if let Some((x, z)) = watch.target {
        let distance = (x - player.x).hypot(z - player.z);
        if distance <= 0.15 {
            watch.state(Kind::Idle, game.time, 0);
        } else {
            player.input.dx = (x - player.x) / distance * 0.28;
            player.input.dz = (z - player.z) / distance * 0.28;
        }
    }
    player.watch = Some(watch);
}
