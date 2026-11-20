use chao_game_server::{
    game::Game,
    phone,
    service::{AppState, Config},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};
use tiny_garden_games::{MemoryView, TinyView};

fn fixture(owner: bool) -> (Game, String, String) {
    let mut g = Game::new(vec![], BTreeMap::new(), owner, 1000.).unwrap();
    let s = g.create_session(&json!({"name":"Phone test"})).unwrap();
    let id = s["player"]["id"].as_str().unwrap().to_owned();
    let token = s["token"].as_str().unwrap().to_owned();
    g.connect(&id);
    (g, id, token)
}
fn advance(g: &mut Game, ms: usize) {
    for _ in 0..ms.div_ceil(50) {
        g.tick(0.05);
    }
}
fn act(g: &mut Game, id: &str, action: &str, mut fields: Value) {
    advance(g, 100);
    fields["action"] = json!(action);
    g.action(id, &fields);
}
fn run_id(g: &Game, id: &str) -> String {
    g.players[id].tiny_game.as_ref().unwrap().id.clone()
}
fn input(g: &mut Game, id: &str, intent: Value) {
    advance(g, 100);
    g.tiny_input(id,&json!({"sessionId":run_id(g,id),"input":intent,"score":999999,"rings":999999,"elapsedMs":999999}));
}
fn memory_view(g: &Game, id: &str) -> MemoryView {
    match g.players[id].tiny_game.as_ref().unwrap().core.view() {
        TinyView::Memory(view) => view,
        _ => panic!("expected Memory"),
    }
}

// Solve using only the same faces and occupied/moving cells sent to a browser.
// No private board, seed, result, score or reward is injected into the core.
fn earn_memory_result(g: &mut Game, id: &str) {
    act(g, id, "tiny-start", json!({"gameId":"memory"}));
    let mut known: Vec<_> = memory_view(g, id)
        .cards
        .into_iter()
        .map(|c| c.face)
        .collect();
    for _ in 0..120 {
        if memory_view(g, id).phase == "choose" {
            break;
        }
        advance(g, 50);
        let view = memory_view(g, id);
        let removed: Vec<_> = known
            .iter()
            .enumerate()
            .filter(|(i, v)| v.is_some() && !view.cards[*i].occupied)
            .map(|(i, _)| i)
            .collect();
        let added: Vec<_> = known
            .iter()
            .enumerate()
            .filter(|(i, v)| v.is_none() && view.cards[*i].occupied)
            .map(|(i, _)| i)
            .collect();
        if !removed.is_empty() {
            assert_eq!(removed.len(), 1);
            assert_eq!(added.len(), 1);
            known.swap(removed[0], added[0]);
        }
    }
    assert_eq!(memory_view(g, id).phase, "choose");
    for face in 0..7 {
        let pair: Vec<_> = known
            .iter()
            .enumerate()
            .filter(|(_, value)| **value == Some(face))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(pair.len(), 2);
        for cell in pair {
            input(g, id, json!({"kind":"memory_flip","cell":cell}));
        }
    }
    assert!(phone::has_pending(g));
    assert_eq!(g.snapshot(id)["tinyGame"]["phase"], "saving");
}

#[test]
fn all_five_games_are_host_owned_and_main_reward_actions_cannot_run_in_parallel() {
    let (mut g, id, _) = fixture(true);
    for game_id in ["memory", "rock_paper_scissors", "bounce", "ball", "search"] {
        act(
            &mut g,
            &id,
            "tiny-start",
            json!({"gameId":game_id,"seed":42,"rings":999999}),
        );
        let session = run_id(&g, &id);
        let before = g.players[&id].public();
        let view = g.snapshot(&id);
        assert_eq!(view["tinyGame"]["gameId"], game_id);
        assert_eq!(view["tinyGame"]["view"]["game"], game_id);
        assert!(!view.to_string().contains("tokenHash"));
        assert!(!view["tinyGame"].to_string().contains("seed"));
        for action in [
            "mission",
            "race",
            "karate-enter",
            "mount-dark-walker",
            "train-run",
            "buy",
            "evolve",
        ] {
            act(
                &mut g,
                &id,
                action,
                json!({"tier":"test","item":"garden-fruit"}),
            );
        }
        g.set_input(&id, &json!({"dx":1,"dz":1,"jump":true,"boost":true}));
        advance(&mut g, 200);
        assert_eq!(g.players[&id].zone, "hub");
        assert_eq!(
            g.players[&id].rings,
            before["rings"].as_u64().unwrap() as u32
        );
        assert!(g.players[&id].vehicle.is_none());
        assert_eq!(g.players[&id].x, before["x"].as_f64().unwrap());
        act(&mut g, &id, "tiny-stop", json!({"sessionId":session}));
        assert!(g.players[&id].tiny_game.is_none());
    }
}

#[test]
fn owner_phone_transfer_dismounts_and_returns_home_but_cannot_skip_a_mission() {
    let (mut g, id, _) = fixture(true);
    act(&mut g, &id, "mount-dark-walker", json!({}));
    act(&mut g, &id, "garden", json!({}));
    act(&mut g, &id, "tiny-start", json!({"gameId":"memory"}));
    assert!(g.players[&id].vehicle.is_none());
    assert_eq!(g.players[&id].zone, "hub");
    let session = run_id(&g, &id);
    act(&mut g, &id, "tiny-stop", json!({"sessionId":session}));
    act(&mut g, &id, "mission", json!({}));
    assert_eq!(g.players[&id].zone, "mission");
    act(&mut g, &id, "tiny-start", json!({"gameId":"search"}));
    assert!(g.players[&id].tiny_game.is_none());
}

#[test]
fn bad_game_input_and_stale_session_ids_cannot_inject_state_time_or_rewards() {
    let (mut g, id, _) = fixture(false);
    act(&mut g, &id, "tiny-start", json!({"gameId":"unknown"}));
    assert!(g.players[&id].tiny_game.is_none());
    act(
        &mut g,
        &id,
        "tiny-start",
        json!({"gameId":"rock_paper_scissors"}),
    );
    let session = run_id(&g, &id);
    let before = g.snapshot(&id)["tinyGame"]["view"].clone();
    for intent in [
        json!({"kind":"finish","rings":999999}),
        json!({"kind":"move","x":999,"y":0}),
        json!({"kind":"rps_shoot","score":9999}),
    ] {
        g.tiny_input(
            &id,
            &json!({"sessionId":session,"input":intent,"elapsedMs":999999}),
        );
    }
    assert_eq!(g.snapshot(&id)["tinyGame"]["view"], before);
    assert_eq!(g.players[&id].rings, 20);
    g.tiny_input(
        &id,
        &json!({"sessionId":"old-session","input":{"kind":"rps_shoot"}}),
    );
    assert_eq!(g.snapshot(&id)["tinyGame"]["view"], before);
    act(&mut g, &id, "tiny-stop", json!({"sessionId":"old-session"}));
    assert!(g.players[&id].tiny_game.is_some());
    act(&mut g, &id, "tiny-start", json!({"gameId":"ball"}));
    assert_eq!(run_id(&g, &id), session);
}

#[test]
fn duplicate_sockets_share_one_clock_and_unfinished_final_disconnect_pays_nothing() {
    let (mut g, id, token) = fixture(false);
    act(
        &mut g,
        &id,
        "tiny-start",
        json!({"gameId":"rock_paper_scissors"}),
    );
    g.connect(&id);
    advance(&mut g, 1000);
    assert_eq!(
        g.snapshot(&id)["tinyGame"]["view"]["state"]["remaining_ms"],
        29000
    );
    g.disconnect(&id);
    assert!(g.players[&id].tiny_game.is_some());
    g.disconnect(&id);
    assert!(g.players[&id].tiny_game.is_none());
    assert_eq!(g.players[&id].rings, 20);
    let restored = Game::new(g.export_profiles(), BTreeMap::new(), false, 1000.).unwrap();
    assert!(
        restored
            .player_for_token(&token)
            .unwrap()
            .tiny_game
            .is_none()
    );
}

#[test]
fn actual_earned_reward_and_receipt_commit_together_only_after_atomic_save_and_never_replay() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let mut g = state.game.lock().unwrap();
    let s = g.create_session(&json!({"name":"Atomic phone"})).unwrap();
    let id = s["player"]["id"].as_str().unwrap().to_owned();
    let token = s["token"].as_str().unwrap();
    g.connect(&id);
    state.persist(&mut g).unwrap();
    let before = fs::read(dir.path().join("profiles.json")).unwrap();
    earn_memory_result(&mut g, &id);
    assert_eq!(g.players[&id].rings, 20);
    assert!(g.players[&id].tiny_records.is_empty());
    let result_id = run_id(&g, &id);
    let projected = g.export_profiles();
    let projected = projected.iter().find(|p| p["id"] == id).unwrap();
    let credit = projected["rings"].as_u64().unwrap() - 20;
    assert!(credit > 0 && credit <= 100);
    assert_eq!(
        projected["tinyRecords"]["memory"]["lastResultId"],
        result_id
    );
    fs::create_dir(dir.path().join("profiles.json.tmp")).unwrap();
    assert!(state.persist(&mut g).is_err());
    assert!(state.persist(&mut g).is_err());
    assert_eq!(fs::read(dir.path().join("profiles.json")).unwrap(), before);
    assert_eq!(g.players[&id].rings, 20);
    assert!(phone::has_pending(&g));
    assert!(
        !g.drain_events()
            .iter()
            .any(|e| e.message["kind"] == "tiny-game-result")
    );
    act(&mut g, &id, "tiny-start", json!({"gameId":"ball"}));
    assert_eq!(run_id(&g, &id), result_id);
    act(&mut g, &id, "tiny-stop", json!({"sessionId":result_id}));
    assert!(phone::has_pending(&g));
    fs::remove_dir(dir.path().join("profiles.json.tmp")).unwrap();
    state.persist(&mut g).unwrap();
    assert_eq!(g.players[&id].rings, 20 + credit as u32);
    assert_eq!(g.players[&id].tiny_records["memory"].plays, 1);
    assert!(!phone::has_pending(&g));
    let events = g.drain_events();
    let results: Vec<_> = events
        .iter()
        .filter(|e| e.message["kind"] == "tiny-game-result")
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].message["saved"], true);
    assert_eq!(results[0].message["rings"], credit);
    state.persist(&mut g).unwrap();
    advance(&mut g, 1000);
    assert_eq!(g.players[&id].rings, 20 + credit as u32);
    assert!(
        !g.drain_events()
            .iter()
            .any(|e| e.message["kind"] == "tiny-game-result")
    );
    drop(g);
    let restarted = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let loaded = restarted.game.lock().unwrap();
    let p = loaded.player_for_token(token).unwrap();
    assert_eq!(p.rings, 20 + credit as u32);
    assert_eq!(
        p.tiny_records["memory"].last_result_id.as_deref(),
        Some(result_id.as_str())
    );
    assert!(p.tiny_game.is_none());
}

#[test]
fn pending_reward_survives_final_disconnect_until_saved_and_then_clears_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let mut g = state.game.lock().unwrap();
    let s = g.create_session(&json!({})).unwrap();
    let id = s["player"]["id"].as_str().unwrap().to_owned();
    g.connect(&id);
    earn_memory_result(&mut g, &id);
    g.disconnect(&id);
    assert!(phone::has_pending(&g));
    state.persist(&mut g).unwrap();
    assert!(g.players[&id].tiny_game.is_none());
    assert_eq!(g.players[&id].tiny_records["memory"].plays, 1);
    assert!(g.players[&id].rings > 20);
}

#[test]
fn restoring_phone_records_bounds_numbers_and_rejects_unknown_games_and_receipts() {
    let records = phone::restore_records(
        &json!({"memory":{"plays":999999999,"completions":-1,"ringsEarned":999999999,"lastResultId":"forged"},"unknown":{"plays":99}}),
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records["memory"].plays, 99999);
    assert_eq!(records["memory"].completions, 0);
    assert_eq!(records["memory"].rings_earned, 999999);
    assert!(records["memory"].last_result_id.is_none());
}
