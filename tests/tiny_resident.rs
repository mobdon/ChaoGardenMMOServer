//! Automatic Tiny residency is an explicit durable choice, never a page-load side effect.
use chao_game_server::{
    activities::ActivityCatalog,
    game::{Game, Race},
    phone,
    service::{AppState, Config},
    watch,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};
use tiny_garden_games::{MemoryView, TinyView};
use uuid::Uuid;

fn fixture() -> (tempfile::TempDir, AppState, String) {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let id = {
        let mut game = state.game.lock().unwrap();
        let session = game
            .create_session(&json!({"name":"Tiny resident"}))
            .unwrap();
        let id = session["player"]["id"].as_str().unwrap().to_owned();
        game.connect(&id);
        state.persist(&mut game).unwrap();
        id
    };
    (directory, state, id)
}
fn enter(revision: u64) -> Value {
    json!({"type":"care","requestId":Uuid::new_v4().to_string(),"expectedRevision":revision,"intent":{"kind":"tiny_enter"}})
}
fn advance(game: &mut Game, milliseconds: usize) {
    for _ in 0..milliseconds.div_ceil(50) {
        game.tick(0.05);
    }
}
fn action(game: &mut Game, id: &str, action: &str, mut fields: Value) {
    advance(game, 100);
    fields["action"] = json!(action);
    game.action(id, &fields);
}
fn run_id(game: &Game, id: &str) -> String {
    game.players[id].tiny_game.as_ref().unwrap().id.clone()
}
fn memory_view(game: &Game, id: &str) -> MemoryView {
    match game.players[id]
        .tiny_game
        .as_ref()
        .unwrap()
        .adapted()
        .unwrap()
        .view()
    {
        TinyView::Memory(view) => view,
        _ => panic!("expected public Memory view"),
    }
}
// Observe only faces/occupancy sent to clients, including the visible shuffle.
fn finish_memory(game: &mut Game, id: &str) {
    action(game, id, "tiny-start", json!({"gameId":"memory"}));
    let mut known: Vec<_> = memory_view(game, id)
        .cards
        .into_iter()
        .map(|card| card.face)
        .collect();
    for _ in 0..120 {
        if memory_view(game, id).phase == "choose" {
            break;
        }
        advance(game, 50);
        let view = memory_view(game, id);
        let removed: Vec<_> = known
            .iter()
            .enumerate()
            .filter(|(i, face)| face.is_some() && !view.cards[*i].occupied)
            .map(|(i, _)| i)
            .collect();
        let added: Vec<_> = known
            .iter()
            .enumerate()
            .filter(|(i, face)| face.is_none() && view.cards[*i].occupied)
            .map(|(i, _)| i)
            .collect();
        if !removed.is_empty() {
            assert_eq!(removed.len(), 1);
            assert_eq!(added.len(), 1);
            known.swap(removed[0], added[0]);
        }
    }
    assert_eq!(memory_view(game, id).phase, "choose");
    for face in 0..7 {
        let pair: Vec<_> = known
            .iter()
            .enumerate()
            .filter(|(_, value)| **value == Some(face))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(pair.len(), 2);
        for cell in pair {
            advance(game, 100);
            game.tiny_input(
                id,
                &json!({"sessionId":run_id(game,id),"input":{"kind":"memory_flip","cell":cell}}),
            );
        }
    }
    assert!(phone::has_pending(game));
}

#[test]
fn explicit_tiny_entry_is_durable_visible_offline_and_restores_even_when_legacy_resident_is_null() {
    let (directory, state, id) = fixture();
    let request = enter(0);
    let original_watch;
    {
        let mut game = state.game.lock().unwrap();
        let before = game.players[&id].clone();
        let response = state.care_action(&mut game, &id, &request).unwrap();
        assert_eq!(response["accepted"], true);
        let p = &game.players[&id];
        assert!(p.care.tiny_resident);
        assert!(p.watch.is_some());
        assert_eq!(p.zone, "garden");
        assert_eq!(p.rings, before.rings);
        assert_eq!(p.stats, before.stats);
        assert_eq!(p.token_hash, before.token_hash);
        assert_eq!(p.form, before.form);
        assert_eq!(p.gear, before.gear);
        assert_eq!(p.care.revision, 1);
        original_watch = p.watch.as_ref().unwrap().id.clone();
        assert_eq!(
            game.snapshot(&id)["care"]["tinyResident"],
            json!({"enabled":true,"active":true,"mode":"resident"})
        );
    }
    let path = directory.path().join("profiles.json");
    let mut profiles: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(profiles[0]["care"]["tinyResident"], true);
    // A save taken during a minigame has no legacy watch, but its opt-in remains.
    profiles[0]["resident"] = Value::Null;
    fs::write(&path, serde_json::to_vec_pretty(&profiles).unwrap()).unwrap();
    let restored = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let mut game = restored.game.lock().unwrap();
    let p = &game.players[&id];
    assert!(p.care.tiny_resident);
    assert!(p.watch.is_some());
    assert_eq!(p.connections, 0);
    assert_ne!(p.watch.as_ref().unwrap().id, original_watch);
    let rings = p.rings;
    let stats = p.stats.clone();
    let start = (p.x, p.z);
    let mut moved = false;
    for _ in 0..500 {
        game.tick(0.05);
        let p = &game.players[&id];
        moved |= (p.x - start.0).hypot(p.z - start.1) > 0.1;
    }
    assert!(moved);
    assert_eq!(game.players[&id].rings, rings);
    assert_eq!(game.players[&id].stats, stats);
    let view = game.snapshot("spectator");
    assert_eq!(view["players"].as_array().unwrap().len(), 1);
    assert_eq!(view["players"][0]["connected"], false);
    assert_eq!(
        restored.care_action(&mut game, &id, &request).unwrap()["replayed"],
        true
    );
}

#[test]
fn tiny_entry_failure_rolls_back_the_choice_pose_receipt_and_disk_then_same_uuid_can_commit() {
    let (directory, state, id) = fixture();
    let request = enter(0);
    let mut game = state.game.lock().unwrap();
    let before = game.players[&id].profile();
    let disk = fs::read(directory.path().join("profiles.json")).unwrap();
    fs::create_dir(directory.path().join("profiles.json.tmp")).unwrap();
    assert!(state.care_action(&mut game, &id, &request).is_err());
    assert_eq!(game.players[&id].profile(), before);
    assert!(!game.players[&id].care.tiny_resident);
    assert!(game.players[&id].watch.is_none());
    assert_eq!(game.players[&id].zone, "hub");
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        disk
    );
    fs::remove_dir(directory.path().join("profiles.json.tmp")).unwrap();
    assert_eq!(
        state.care_action(&mut game, &id, &request).unwrap()["accepted"],
        true
    );
    assert_eq!(
        state.care_action(&mut game, &id, &request).unwrap()["replayed"],
        true
    );
    assert_eq!(game.players[&id].care.receipts.len(), 1);
}

#[test]
fn actual_tiny_games_suspend_residency_and_stopping_resumes_without_manual_watch_steps() {
    let (_directory, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    assert_eq!(
        state.care_action(&mut game, &id, &enter(0)).unwrap()["accepted"],
        true
    );
    let watch = game.players[&id].watch.as_ref().unwrap().id.clone();
    action(
        &mut game,
        &id,
        "tiny-start",
        json!({"gameId":"unavailable-game"}),
    );
    assert_eq!(game.players[&id].watch.as_ref().unwrap().id, watch);
    assert!(game.players[&id].tiny_game.is_none());
    for game_id in ["memory", "rock_paper_scissors", "bounce", "ball", "search"] {
        action(&mut game, &id, "tiny-start", json!({"gameId":game_id}));
        let session = run_id(&game, &id);
        assert!(game.players[&id].watch.is_none());
        assert!(game.players[&id].care.tiny_resident);
        assert_eq!(watch::tiny_public(&game.players[&id])["mode"], "minigame");
        game.set_input(&id, &json!({"dx":1,"dz":1,"jump":true,"boost":true}));
        assert_eq!(game.players[&id].input.dx, 0.);
        assert_eq!(game.players[&id].input.dz, 0.);
        action(
            &mut game,
            &id,
            "tiny-stop",
            json!({"sessionId":"wrong-session"}),
        );
        assert!(game.players[&id].tiny_game.is_some());
        assert!(game.players[&id].watch.is_none());
        action(&mut game, &id, "tiny-stop", json!({"sessionId":session}));
        assert!(game.players[&id].tiny_game.is_none());
        assert!(game.players[&id].watch.is_some());
        assert_eq!(game.players[&id].zone, "garden");
        assert_eq!(watch::tiny_public(&game.players[&id])["mode"], "resident");
    }
}

#[test]
fn terminal_reward_stays_paused_through_save_failure_and_disconnect_then_resumes_after_commit_once()
{
    let (directory, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    state.care_action(&mut game, &id, &enter(0)).unwrap();
    let rings = game.players[&id].rings;
    finish_memory(&mut game, &id);
    let session = run_id(&game, &id);
    let earned = game.snapshot(&id)["tinyGame"]["result"]["rings"]
        .as_u64()
        .unwrap() as u32;
    assert!(earned > 0);
    assert_eq!(game.players[&id].rings, rings);
    assert!(game.players[&id].watch.is_none());
    assert_eq!(watch::tiny_public(&game.players[&id])["mode"], "saving");
    fs::create_dir(directory.path().join("profiles.json.tmp")).unwrap();
    assert!(state.persist(&mut game).is_err());
    action(&mut game, &id, "tiny-stop", json!({"sessionId":session}));
    assert!(game.players[&id].tiny_game.as_ref().unwrap().saving());
    game.disconnect(&id);
    advance(&mut game, 1000);
    assert!(game.players[&id].tiny_game.as_ref().unwrap().saving());
    assert!(game.players[&id].watch.is_none());
    assert_eq!(game.players[&id].rings, rings);
    fs::remove_dir(directory.path().join("profiles.json.tmp")).unwrap();
    state.persist(&mut game).unwrap();
    let p = &game.players[&id];
    assert_eq!(p.rings, rings + earned);
    assert!(p.tiny_game.is_none());
    assert!(p.watch.is_some());
    assert_eq!(p.tiny_result.as_ref().unwrap()["saved"], true);
    assert_eq!(p.tiny_records["memory"].plays, 1);
    assert_eq!(
        p.tiny_records["memory"].last_result_id.as_deref(),
        Some(session.as_str())
    );
    assert_eq!(watch::tiny_public(p)["mode"], "resident");
    state.persist(&mut game).unwrap();
    advance(&mut game, 2000);
    assert_eq!(game.players[&id].rings, rings + earned);
    let restored = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let game = restored.game.lock().unwrap();
    assert!(game.players[&id].watch.is_some());
    assert_eq!(game.players[&id].rings, rings + earned);
    assert_eq!(game.players[&id].tiny_records["memory"].plays, 1);
}

#[test]
fn last_device_disconnect_cancels_only_the_unfinished_run_and_keeps_one_shared_resident() {
    let (_directory, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    state.care_action(&mut game, &id, &enter(0)).unwrap();
    game.connect(&id);
    action(&mut game, &id, "tiny-start", json!({"gameId":"search"}));
    game.disconnect(&id);
    assert_eq!(game.players[&id].connections, 1);
    assert!(game.players[&id].tiny_game.is_some());
    assert!(game.players[&id].watch.is_none());
    game.disconnect(&id);
    assert_eq!(game.players[&id].connections, 0);
    assert!(game.players[&id].tiny_game.is_none());
    assert!(game.players[&id].watch.is_some());
    assert!(game.players[&id].tiny_result.is_none());
    assert!(game.players[&id].tiny_records.is_empty());
    assert_eq!(game.players[&id].rings, 20);
    assert_eq!(
        game.snapshot("spectator")["players"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn new_entry_rejects_conflicting_activities_and_training_pauses_ai_until_its_lock_is_cleared() {
    for scenario in [
        "tiny",
        "race",
        "karate",
        "vehicle",
        "training",
        "mission",
        "disconnected",
    ] {
        let (_directory, state, id) = fixture();
        let mut game = state.game.lock().unwrap();
        match scenario {
            "tiny" => action(&mut game, &id, "tiny-start", json!({"gameId":"memory"})),
            "race" => {
                game.players.get_mut(&id).unwrap().race = Some(Race {
                    started_at: game.time,
                    next_checkpoint: 0,
                    course_id: None,
                })
            }
            "karate" => {
                game.activities=ActivityCatalog::from_values(json!({}),json!({"tiers":[{"id":"synthetic","name":"Synthetic","opponents":[{"id":"test","name":"Test","skills":[10,10,10,10,10,10,10]}]}]})).unwrap();
                action(&mut game, &id, "karate-enter", json!({"tier":"synthetic"}));
                assert!(game.players[&id].karate.is_some());
            }
            "vehicle" => {
                game.players.get_mut(&id).unwrap().vehicle = Some("dark-chao-walker".into())
            }
            "training" => {
                game.players.get_mut(&id).unwrap().care.activity_lock =
                    Some("training:synthetic-job".into())
            }
            "mission" => game.players.get_mut(&id).unwrap().zone = "mission".into(),
            "disconnected" => game.disconnect(&id),
            _ => unreachable!(),
        }
        let before = game.players[&id].profile();
        assert_eq!(
            state.care_action(&mut game, &id, &enter(0)).unwrap()["code"],
            "activity-conflict",
            "{scenario}"
        );
        assert_eq!(game.players[&id].profile(), before, "{scenario}");
    }
    let (_directory, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    state.care_action(&mut game, &id, &enter(0)).unwrap();
    game.players.get_mut(&id).unwrap().care.activity_lock = Some("training:synthetic-job".into());
    let position = (game.players[&id].x, game.players[&id].z);
    let stats = game.players[&id].stats.clone();
    advance(&mut game, 4000);
    let p = &game.players[&id];
    assert!(p.watch.is_none());
    assert_eq!((p.x, p.z), position);
    assert_eq!(p.stats, stats);
    assert_eq!(watch::tiny_public(p)["mode"], "paused");
    assert!(p.care.tiny_resident);
    game.players.get_mut(&id).unwrap().care.activity_lock = None;
    game.tick(0.05);
    assert!(game.players[&id].watch.is_some());
    assert_eq!(watch::tiny_public(&game.players[&id])["mode"], "resident");
}

#[test]
fn existing_manual_watch_can_become_tiny_and_explicit_desktop_stop_durably_restores_manual_control()
{
    let (directory, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    action(&mut game, &id, "watch-start", json!({}));
    let session = game.players[&id].watch.as_ref().unwrap().id.clone();
    assert!(!game.players[&id].care.tiny_resident);
    state.care_action(&mut game, &id, &enter(0)).unwrap();
    assert!(game.players[&id].care.tiny_resident);
    assert_eq!(game.players[&id].watch.as_ref().unwrap().id, session);
    advance(&mut game, 100);
    let response = state
        .watch_action(
            &mut game,
            &id,
            &json!({"action":"watch-stop","sessionId":session}),
        )
        .unwrap();
    assert_eq!(response["accepted"], true);
    assert!(!game.players[&id].care.tiny_resident);
    assert!(game.players[&id].watch.is_none());
    let x = game.players[&id].x;
    game.set_input(&id, &json!({"dx":1,"dz":0}));
    game.tick(0.1);
    assert!(game.players[&id].x > x);
    assert_eq!(watch::tiny_public(&game.players[&id])["mode"], "manual");
    let restored = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let game = restored.game.lock().unwrap();
    assert!(!game.players[&id].care.tiny_resident);
    assert!(game.players[&id].watch.is_none());
}

#[test]
fn legacy_care_defaults_and_passive_connections_never_opt_in_or_bypass_manual_watch() {
    let (_directory, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    game.connect(&id);
    advance(&mut game, 500);
    assert!(!game.players[&id].care.tiny_resident);
    assert!(game.players[&id].watch.is_none());
    let mut profiles = game.export_profiles();
    profiles[0]["care"]
        .as_object_mut()
        .unwrap()
        .remove("tinyResident");
    let mut restored = Game::new(profiles, BTreeMap::new(), false, 1000.).unwrap();
    restored.connect(&id);
    assert!(!restored.players[&id].care.tiny_resident);
    assert!(restored.players[&id].watch.is_none());
    action(&mut restored, &id, "watch-start", json!({}));
    let watch = restored.players[&id].watch.as_ref().unwrap().id.clone();
    action(&mut restored, &id, "tiny-start", json!({"gameId":"memory"}));
    assert!(restored.players[&id].tiny_game.is_none());
    assert_eq!(restored.players[&id].watch.as_ref().unwrap().id, watch);
    assert_eq!(watch::tiny_public(&restored.players[&id])["mode"], "manual");
}
