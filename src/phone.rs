//! Adapted Tiny Garden games: input-only control, one run per profile and a
//! staged reward whose ring credit and receipt share one atomic profile write.
use crate::game::{Event, Game, Input, Player};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tiny_garden_games::{TinyGame, TinyInput, TinySession};
use uuid::Uuid;

const GAME_IDS: [&str; 5] = ["memory", "rock_paper_scissors", "bounce", "ball", "search"];
pub const MAX_RING_REWARD: u32 = 100;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TinyRecord {
    pub plays: u32,
    pub completions: u32,
    pub rings_earned: u32,
    pub last_result_id: Option<String>,
}

pub fn restore_records(value: &Value) -> BTreeMap<String, TinyRecord> {
    GAME_IDS
        .into_iter()
        .filter_map(|id| {
            let row = value.get(id)?.as_object()?;
            Some((
                id.to_owned(),
                TinyRecord {
                    plays: row
                        .get("plays")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                        .min(99999) as u32,
                    completions: row
                        .get("completions")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                        .min(99999) as u32,
                    rings_earned: row
                        .get("ringsEarned")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                        .min(999999) as u32,
                    last_result_id: row
                        .get("lastResultId")
                        .and_then(Value::as_str)
                        .filter(|id| Uuid::parse_str(id).is_ok())
                        .map(str::to_owned),
                },
            ))
        })
        .collect()
}

#[derive(Clone, Debug)]
struct PendingReward {
    rings_after: u32,
    record: TinyRecord,
    result: Value,
}

#[derive(Clone, Debug)]
pub struct TinyRun {
    pub id: String,
    pub game_id: String,
    pub started_at: f64,
    pub core: TinySession,
    last_move_at: f64,
    pending: Option<PendingReward>,
    result: Option<Value>,
    save_failed: bool,
}

impl TinyRun {
    pub fn saving(&self) -> bool {
        self.pending.is_some()
    }
    pub fn public(&self) -> Value {
        json!({"id":self.id,"gameId":self.game_id,"startedAt":self.started_at,
            "phase":if self.pending.is_some(){"saving"}else if self.result.is_some(){"finished"}else{"playing"},
            "view":self.core.view(),"result":self.pending.as_ref().map(|p|&p.result).or(self.result.as_ref())})
    }
}

pub fn action(game: &mut Game, player: &mut Player, message: &Value) {
    match message["action"].as_str().unwrap_or_default() {
        "tiny-start" => {
            if player
                .tiny_game
                .as_ref()
                .is_some_and(|run| run.pending.is_some() || run.result.is_none())
            {
                game.notice(
                    &player.id,
                    "Finish or stop your current Tiny Garden game first.",
                    "error",
                );
                return;
            }
            if player.race.is_some()
                || player.karate.is_some()
                || !["hub", "garden", "hero", "dark"].contains(&player.zone.as_str())
            {
                game.notice(&player.id, "Finish your current activity and return to a garden before starting a Tiny Garden game.", "error");
                return;
            }
            let Some(id) = message["gameId"]
                .as_str()
                .filter(|id| GAME_IDS.contains(id))
            else {
                game.notice(&player.id, "Unknown Tiny Garden game.", "error");
                return;
            };
            let game_id: TinyGame = serde_json::from_value(json!(id)).expect("fixed game ID");
            if game.teleport(player, "hub", 0., 6.).is_err() {
                game.notice(
                    &player.id,
                    "A safe garden transfer is not available.",
                    "error",
                );
                return;
            }
            if player.vehicle.take().is_some() {
                game.dirty = true;
            }
            player.tiny_game = Some(TinyRun {
                id: Uuid::new_v4().to_string(),
                game_id: id.to_owned(),
                started_at: game.time,
                core: TinySession::new(game_id, Uuid::new_v4().as_u128() as u64),
                last_move_at: game.time,
                pending: None,
                result: None,
                save_failed: false,
            });
            player.tiny_result = None;
            player.input = Input::default();
            player.vy = 0.;
            player.jump_offset = 0.;
            player.y = player.ground_y;
            player.boosting = false;
        }
        "tiny-stop" => {
            let Some(run) = &player.tiny_game else {
                return;
            };
            if message["sessionId"] != run.id {
                return;
            }
            if run.saving() {
                game.notice(
                    &player.id,
                    "Your earned result is waiting to save. Please retry Save.",
                    "error",
                );
                return;
            }
            player.tiny_game = None;
            player.input = Input::default();
        }
        _ => {}
    }
}

pub fn input(game: &mut Game, player: &mut Player, message: &Value) {
    let Some(run) = &mut player.tiny_game else {
        return;
    };
    if message["sessionId"] != run.id || run.pending.is_some() || run.result.is_some() {
        return;
    }
    let intent: TinyInput = match serde_json::from_value(message["input"].clone()) {
        Ok(input) => input,
        Err(_) => {
            game.notice(&player.id, "Invalid Tiny Garden input.", "error");
            return;
        }
    };
    let moving = message["input"]["kind"] == "move";
    let cooldown = if moving { "tiny-move" } else { "tiny-tap" };
    if !player.cooldown(cooldown, game.time, if moving { 40. } else { 80. }) {
        return;
    }
    let run = player.tiny_game.as_mut().unwrap();
    if run.core.input(intent).is_err() {
        return;
    }
    if moving {
        run.last_move_at = game.time;
    }
    prepare_reward(game, player);
}

pub fn tick(game: &mut Game, player: &mut Player, dt: f64) {
    let Some(run) = &mut player.tiny_game else {
        return;
    };
    if run.pending.is_some() || run.result.is_some() {
        return;
    }
    if game.time - run.last_move_at > 350. {
        let _ = run.core.input(TinyInput::Move { x: 0, y: 0 });
    }
    let _ = run
        .core
        .advance((dt * 1000.).round().clamp(1., 100.) as u32);
    prepare_reward(game, player);
}

fn prepare_reward(game: &mut Game, player: &mut Player) {
    let Some(run) = &mut player.tiny_game else {
        return;
    };
    if run.pending.is_some() || run.result.is_some() || run.core.outcome().is_none() {
        return;
    }
    let Ok(outcome) = run.core.claim_result() else {
        return;
    };
    let value = serde_json::to_value(outcome).expect("core result serialization");
    let earned = value["rings"]
        .as_u64()
        .unwrap_or(0)
        .min(u64::from(u32::MAX)) as u32;
    let credited = earned
        .min(MAX_RING_REWARD)
        .min(999999_u32.saturating_sub(player.rings));
    let mut record = player
        .tiny_records
        .get(&run.game_id)
        .cloned()
        .unwrap_or_default();
    if record.last_result_id.as_deref() == Some(&run.id) {
        return;
    }
    record.plays = (record.plays + 1).min(99999);
    record.completions = (record.completions + u32::from(value["completed"] == true)).min(99999);
    record.rings_earned = record.rings_earned.saturating_add(credited).min(999999);
    record.last_result_id = Some(run.id.clone());
    let result = json!({"id":run.id,"gameId":run.game_id,"rings":credited,"earnedRings":earned,
        "reason":value["reason"],"completed":value["completed"],"elapsedMs":value["elapsed_ms"],"saved":false});
    run.pending = Some(PendingReward {
        rings_after: player.rings + credited,
        record,
        result,
    });
    game.dirty = true;
}

/// A pending reward is projected into the same file as its receipt. Live rings
/// and final events stay unchanged until AppState's fsync/rename succeeds.
pub fn project_profile(player: &Player, profile: &mut Value) {
    let Some(run) = &player.tiny_game else {
        return;
    };
    let Some(pending) = &run.pending else {
        return;
    };
    profile["rings"] = json!(pending.rings_after);
    profile["tinyRecords"][&run.game_id] = json!(pending.record);
}

pub fn has_pending(game: &Game) -> bool {
    game.players
        .values()
        .any(|p| p.tiny_game.as_ref().is_some_and(TinyRun::saving))
}

pub fn save_failed(game: &mut Game) {
    for player in game.players.values_mut() {
        let Some(run) = &mut player.tiny_game else {
            continue;
        };
        if run.pending.is_some() && !run.save_failed {
            run.save_failed = true;
            game.events.push(Event {target:Some(player.id.clone()),message:json!({"type":"notice","kind":"error","text":"Your Tiny Garden reward could not save yet. It remains pending; please retry Save."})});
        }
    }
}

pub fn commit_rewards(game: &mut Game) {
    for player in game.players.values_mut() {
        let Some(run) = &mut player.tiny_game else {
            continue;
        };
        let Some(mut pending) = run.pending.take() else {
            continue;
        };
        player.rings = pending.rings_after;
        player
            .tiny_records
            .insert(run.game_id.clone(), pending.record);
        pending.result["saved"] = json!(true);
        run.result = Some(pending.result.clone());
        player.tiny_result = Some(pending.result.clone());
        let mut message = pending.result;
        message["type"] = json!("event");
        message["kind"] = json!("tiny-game-result");
        game.events.push(Event {
            target: Some(player.id.clone()),
            message,
        });
        if player.connections == 0 {
            player.tiny_game = None;
        }
    }
}
