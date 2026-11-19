//! Tiny Garden games: input-only control, one run per profile and a
//! staged reward whose ring credit and receipt share one atomic profile write.
use crate::native_tiny::NativeRun;
use crate::{
    game::{Event, Game, Input, Player},
    watch,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tiny_garden_games::{TinyGame, TinyInput, TinySession};
use uuid::Uuid;

pub(crate) const GAME_IDS: [&str; 5] =
    ["memory", "rock_paper_scissors", "bounce", "ball", "search"];
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
    core: RunCore,
    last_move_at: f64,
    pending: Option<PendingReward>,
    result: Option<Value>,
    save_failed: bool,
}

#[derive(Clone, Debug)]
enum RunCore {
    Adapted(Box<TinySession>),
    Native(Box<NativeRun>),
}
impl TinyRun {
    pub fn adapted(&self) -> Option<&TinySession> {
        if let RunCore::Adapted(core) = &self.core {
            Some(core)
        } else {
            None
        }
    }
    pub(crate) fn release_controls(&mut self) {
        if let RunCore::Native(core) = &mut self.core {
            core.release();
        }
    }
    fn rules(&self) -> &'static str {
        match &self.core {
            RunCore::Adapted(_) => "adapted-v1",
            RunCore::Native(core) => core.rules(),
        }
    }
    fn view(&self) -> Value {
        match &self.core {
            RunCore::Adapted(core) => json!(core.view()),
            RunCore::Native(core) => json!({"game":self.game_id,"state":core.view()}),
        }
    }
    pub fn saving(&self) -> bool {
        self.pending.is_some()
    }
    pub fn public(&self) -> Value {
        json!({"id":self.id,"gameId":self.game_id,"startedAt":self.started_at,
            "phase":if self.pending.is_some(){"saving"}else if self.result.is_some(){"finished"}else if matches!(&self.core, RunCore::Native(core) if core.halted()){"error"}else{"playing"},
            "rulesVersion":self.rules(),"view":self.view(),"result":self.pending.as_ref().map(|p|&p.result).or(self.result.as_ref())})
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
                || player.care.activity_lock.is_some()
                || (player.care.tiny_resident && player.vehicle.is_some())
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
            let seed = Uuid::new_v4().as_u128() as u64;
            let core = match game.native_tiny.begin(game_id, seed as u32) {
                Ok(Some(run)) => RunCore::Native(Box::new(run)),
                Ok(None) => RunCore::Adapted(Box::new(TinySession::new(game_id, seed))),
                Err(_) => {
                    game.notice(&player.id, "The native round could not start.", "error");
                    return;
                }
            };
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
            if player.care.tiny_resident {
                player.watch = None;
            }
            player.tiny_game = Some(TinyRun {
                id: Uuid::new_v4().to_string(),
                game_id: id.to_owned(),
                started_at: game.time,
                core,
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
            watch::maintain_tiny(game, player);
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
    if let RunCore::Native(core) = &mut run.core {
        let envelope_ok = message.as_object().is_some_and(|m| {
            m.keys()
                .all(|k| matches!(k.as_str(), "type" | "input" | "sessionId"))
        });
        if !envelope_ok || core.buttons(&message["input"], game.time).is_err() {
            game.notice(&player.id, "Invalid Tiny Garden input.", "error");
        }
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
    let RunCore::Adapted(core) = &mut run.core else {
        return;
    };
    if core.input(intent).is_err() {
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
    match &mut run.core {
        RunCore::Native(core) => core.advance(dt, game.time),
        RunCore::Adapted(core) => {
            if game.time - run.last_move_at > 350. {
                let _ = core.input(TinyInput::Move { x: 0, y: 0 });
            }
            let _ = core.advance((dt * 1000.).round().clamp(1., 100.) as u32);
        }
    }
    prepare_reward(game, player);
}

fn prepare_reward(game: &mut Game, player: &mut Player) {
    let Some(run) = &mut player.tiny_game else {
        return;
    };
    if run.pending.is_some() || run.result.is_some() {
        return;
    }
    let rules = run.rules();
    let (value, limit) = match &mut run.core {
        RunCore::Adapted(core) => {
            let Ok(outcome) = core.claim_result() else {
                return;
            };
            (outcome, MAX_RING_REWARD)
        }
        RunCore::Native(core) => {
            let Some(outcome) = core.take_outcome() else {
                return;
            };
            (outcome, core.max_ring_reward())
        }
    };
    let earned = value.rings;
    let credited = earned
        .min(limit)
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
    record.completions = (record.completions + u32::from(value.completed)).min(99999);
    record.rings_earned = record.rings_earned.saturating_add(credited).min(999999);
    record.last_result_id = Some(run.id.clone());
    let result = json!({"id":run.id,"gameId":run.game_id,"rings":credited,"earnedRings":earned,
        "rulesVersion":rules,"reason":value.reason,"completed":value.completed,"elapsedMs":value.elapsed_ms,"saved":false});
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
        if player.connections == 0 || player.care.tiny_resident {
            player.tiny_game = None;
        }
    }
    watch::maintain_all(game);
}

#[cfg(test)]
mod native_receipt {
    use super::*;
    use crate::service::{AppState, Config};

    #[test]
    #[ignore = "requires the private verified A3VE payload"]
    fn native_ball_pause_quit_saves_exact_original_partial_award() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::local(dir.path().to_owned(), true);
        cfg.tiny_a3ve_payload = Some(
            std::env::var_os("CHAO_TINY_A3VE_PAYLOAD")
                .expect("explicit private A3VE payload required")
                .into(),
        );
        let state = AppState::load(cfg.clone()).unwrap();
        let mut g = state.game.lock().unwrap();
        let session = g
            .create_session(&json!({"name":"Original Ball receipt"}))
            .unwrap();
        let id = session["player"]["id"].as_str().unwrap();
        g.connect(id);
        g.action(id, &json!({"action":"tiny-start","gameId":"ball"}));
        // Only this test fixes the host seed. The socket has no seed or score field.
        let round = g.native_tiny.begin(TinyGame::Ball, 26).unwrap().unwrap();
        let run = g.players.get_mut(id).unwrap().tiny_game.as_mut().unwrap();
        run.core = RunCore::Native(Box::new(round));
        let session_id = run.id.clone();
        // This short, deterministic input trace is independently observed on the
        // A3VE CPU: first catch on frame 854; Start, Down, A quit with one ring
        // and the garden return on frame 877. No scores are submitted.
        let mut previous = 0;
        for frame in 1..=877 {
            let held = match frame {
                855 => 8,
                859 => 128,
                861 => 1,
                n if n <= 854 && n % 20 == 0 => 2,
                _ => 0,
            };
            if held != previous {
                g.tiny_input(id,&json!({"type":"tiny-input","sessionId":session_id,"input":{"kind":"native_buttons","held":held}}));
                previous = held;
            }
            // One cartridge frame, rounded upward one nanosecond.
            g.tick(4389. / 262144. + 1e-9);
            let p = &g.players[id];
            assert_eq!(p.rings, 20);
            let r = p.tiny_game.as_ref().unwrap();
            assert_eq!(r.saving(), frame == 877);
            if frame == 854 {
                assert_eq!(r.public()["view"]["state"]["rings"], 1);
            }
        }
        let pending = g.players[id].tiny_game.as_ref().unwrap().public();
        assert_eq!(pending["result"]["rings"], 1);
        assert_eq!(pending["result"]["reason"], "quit");
        assert_eq!(pending["result"]["completed"], false);
        state.persist(&mut g).unwrap();
        state.persist(&mut g).unwrap();
        assert_eq!(g.players[id].rings, 21);
        drop(g);
        let reload = AppState::load(cfg).unwrap();
        let game = reload.game.lock().unwrap();
        let p = game
            .player_for_token(session["token"].as_str().unwrap())
            .unwrap();
        assert_eq!(p.rings, 21);
        assert_eq!(p.tiny_records["ball"].plays, 1);
        assert_eq!(
            p.tiny_records["ball"].last_result_id.as_deref(),
            Some(session_id.as_str())
        );
    }
}
