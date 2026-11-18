use chao_game_server::{
    game::{Game, Terrain},
    service::{AppState, Config},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
fn fixture(owner: bool) -> (Game, String, String) {
    let mut g = Game::new(vec![], BTreeMap::new(), owner, 1000.).unwrap();
    let s = g.create_session(&json!({"name":"Resident"})).unwrap();
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
fn watch_id(g: &Game, id: &str) -> String {
    g.players[id].watch.as_ref().unwrap().id.clone()
}

#[test]
fn resident_routine_is_visible_and_active_while_away_without_passive_rewards() {
    let (mut g, id, _) = fixture(false);
    act(&mut g, &id, "watch-start", json!({}));
    assert_eq!(g.players[&id].zone, "garden");
    let stats = g.players[&id].stats.clone();
    let happiness = g.players[&id].happiness;
    let rings = g.players[&id].rings;
    g.rings[0].x = g.players[&id].x;
    g.rings[0].z = g.players[&id].z;
    g.disconnect(&id);
    assert!(g.players[&id].watch.is_some());
    let mut states = BTreeSet::new();
    let mut moved = false;
    for _ in 0..500 {
        g.tick(0.05);
        let p = &g.players[&id];
        states.insert(p.watch.as_ref().unwrap().activity);
        moved |= p.x.hypot(p.z - 6.) > 0.1;
        let visible = g.snapshot("spectator");
        assert_eq!(visible["players"].as_array().unwrap().len(), 1);
        assert_eq!(visible["players"][0]["connected"], false);
        assert_eq!(visible["players"][0]["watch"]["ownerOnline"], false);
    }
    assert!(moved);
    assert!(states.contains("idle") && states.contains("wander") && states.contains("rest"));
    assert_eq!(g.players[&id].rings, rings);
    assert_eq!(g.players[&id].stats, stats);
    assert_eq!(g.players[&id].happiness, happiness);
    assert_eq!(g.rings[0].available_at, 0.);
    g.connect(&id);
    g.connect(&id);
    assert_eq!(g.snapshot(&id)["players"].as_array().unwrap().len(), 1);
    g.disconnect(&id);
    assert!(g.players[&id].watch.is_some());
}

#[test]
fn residency_restarts_once_at_safe_garden_position_without_wall_clock_catchup() {
    let (mut g, id, token) = fixture(false);
    act(&mut g, &id, "watch-start", json!({}));
    advance(&mut g, 4000);
    let old = watch_id(&g, &id);
    let profiles = g.export_profiles();
    assert_eq!(profiles[0]["resident"]["enabled"], true);
    let mut restored = Game::new(profiles, BTreeMap::new(), false, 9_999_999_999.).unwrap();
    let p = restored.player_for_token(&token).unwrap();
    assert_eq!(p.zone, "garden");
    assert_eq!(p.rings, 20);
    assert_eq!(p.watch.as_ref().unwrap().activity, "idle");
    assert_ne!(p.watch.as_ref().unwrap().id, old);
    assert_eq!(
        restored.snapshot(&id)["players"].as_array().unwrap().len(),
        1
    );
    restored.connect(&id);
    act(&mut restored, &id, "watch-stop", json!({"sessionId":old}));
    assert!(restored.players[&id].watch.is_some());
    let current = watch_id(&restored, &id);
    act(
        &mut restored,
        &id,
        "watch-stop",
        json!({"sessionId":current}),
    );
    assert!(restored.players[&id].watch.is_none());
    let direct = Game::new(restored.export_profiles(), BTreeMap::new(), false, 0.).unwrap();
    assert!(direct.players[&id].watch.is_none());
}

#[test]
fn strict_watch_intents_isolate_profiles_and_stop_resumes_direct_movement() {
    let (mut g, id, _) = fixture(false);
    act(&mut g, &id, "watch-start", json!({"x":999,"rings":9999}));
    assert!(g.players[&id].watch.is_none());
    act(&mut g, &id, "watch-start", json!({}));
    let first = watch_id(&g, &id);
    let other = g.create_session(&json!({"name":"Other"})).unwrap()["player"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    g.connect(&other);
    act(&mut g, &other, "watch-start", json!({}));
    let second = watch_id(&g, &other);
    act(
        &mut g,
        &other,
        "watch-care",
        json!({"sessionId":first,"kind":"pet"}),
    );
    act(&mut g, &other, "watch-stop", json!({"sessionId":first}));
    assert_eq!(watch_id(&g, &other), second);
    assert_eq!(g.players[&other].happiness, 65.);
    act(
        &mut g,
        &id,
        "watch-care",
        json!({"sessionId":first,"kind":"pet","happiness":100}),
    );
    assert_eq!(g.players[&id].happiness, 65.);
    act(&mut g, &id, "watch-stop", json!({"sessionId":first}));
    assert!(g.players[&id].watch.is_none());
    let x = g.players[&id].x;
    for _ in 0..10 {
        g.set_input(&id, &json!({"dx":1,"dz":0}));
        g.tick(0.05);
    }
    assert!(g.players[&id].x > x + 0.4);
    act(&mut g, &id, "watch-start", json!({}));
    let new = watch_id(&g, &id);
    assert_ne!(new, first);
    act(&mut g, &id, "watch-stop", json!({"sessionId":first}));
    assert_eq!(watch_id(&g, &id), new);
}

#[test]
fn watch_excludes_manual_inputs_conflicting_activities_and_tiny_game_completion() {
    let (mut g, id, _) = fixture(true);
    act(&mut g, &id, "mount-dark-walker", json!({}));
    act(&mut g, &id, "watch-start", json!({}));
    assert!(g.players[&id].vehicle.is_none());
    let session = watch_id(&g, &id);
    let x = g.players[&id].x;
    g.set_input(&id, &json!({"dx":1,"dz":1,"jump":true,"boost":true}));
    advance(&mut g, 500);
    assert_eq!(g.players[&id].x, x);
    for action in [
        "mission",
        "race",
        "race-enter",
        "karate-enter",
        "mount-dark-walker",
        "feed",
        "train-run",
        "buy",
        "tiny-start",
    ] {
        act(
            &mut g,
            &id,
            action,
            json!({"gameId":"memory","courseId":"forged","tier":"forged","item":"garden-fruit"}),
        );
    }
    assert_eq!(g.players[&id].zone, "garden");
    assert!(g.players[&id].race.is_none());
    assert!(g.players[&id].karate.is_none());
    assert!(g.players[&id].tiny_game.is_none());
    assert_eq!(g.players[&id].rings, 20);
    act(&mut g, &id, "watch-stop", json!({"sessionId":session}));
    act(&mut g, &id, "tiny-start", json!({"gameId":"memory"}));
    assert!(g.players[&id].tiny_game.is_some());
    act(&mut g, &id, "watch-start", json!({}));
    assert!(g.players[&id].watch.is_none());
}

#[test]
fn deliberate_care_has_fixed_cooldown_and_no_currency_or_skill_effects() {
    let (mut g, id, _) = fixture(false);
    act(&mut g, &id, "watch-start", json!({}));
    let session = watch_id(&g, &id);
    let stats = g.players[&id].stats.clone();
    for _ in 0..5 {
        act(
            &mut g,
            &id,
            "watch-care",
            json!({"sessionId":session,"kind":"pet"}),
        );
    }
    assert_eq!(g.players[&id].happiness, 67.);
    advance(&mut g, 1500);
    act(
        &mut g,
        &id,
        "watch-care",
        json!({"sessionId":session,"kind":"rest"}),
    );
    assert_eq!(g.players[&id].watch.as_ref().unwrap().activity, "rest");
    advance(&mut g, 1500);
    act(
        &mut g,
        &id,
        "watch-care",
        json!({"sessionId":session,"kind":"wake"}),
    );
    assert_eq!(g.players[&id].watch.as_ref().unwrap().activity, "idle");
    assert_eq!(g.players[&id].stats, stats);
    assert_eq!(g.players[&id].rings, 20);
}

#[test]
fn resident_targets_use_authoritative_ground_and_do_not_cross_holes() {
    let field = Terrain {
        min_x: -2.,
        min_z: 4.,
        step: 0.5,
        width: 9,
        depth: 9,
        heights: (0..81)
            .map(|i| if i % 9 <= 4 { Some(0.) } else { None })
            .collect(),
    };
    let mut fields = BTreeMap::new();
    fields.insert("garden".into(), field);
    let mut g = Game::new(vec![], fields, false, 0.).unwrap();
    let id = g.create_session(&json!({})).unwrap()["player"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    g.connect(&id);
    act(&mut g, &id, "watch-start", json!({}));
    g.disconnect(&id);
    for _ in 0..500 {
        g.tick(0.05);
        let p = &g.players[&id];
        assert!(g.player_ground(p, p.x, p.z).is_some());
        assert!(p.x <= 0.25);
        assert_eq!(p.y, 0.);
    }
}

#[test]
fn resident_opt_in_and_stop_rollback_on_save_failure_and_commit_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), true)).unwrap();
    let mut g = state.game.lock().unwrap();
    let id = g.create_session(&json!({})).unwrap()["player"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    g.connect(&id);
    act(&mut g, &id, "mount-dark-walker", json!({}));
    state.persist(&mut g).unwrap();
    let before = fs::read(dir.path().join("profiles.json")).unwrap();
    fs::create_dir(dir.path().join("profiles.json.tmp")).unwrap();
    advance(&mut g, 100);
    assert!(
        state
            .watch_action(&mut g, &id, &json!({"action":"watch-start"}))
            .is_err()
    );
    assert!(g.players[&id].watch.is_none());
    assert_eq!(g.players[&id].vehicle.as_deref(), Some("dark-chao-walker"));
    assert_eq!(fs::read(dir.path().join("profiles.json")).unwrap(), before);
    fs::remove_dir(dir.path().join("profiles.json.tmp")).unwrap();
    advance(&mut g, 100);
    let ack = state
        .watch_action(&mut g, &id, &json!({"action":"watch-start"}))
        .unwrap();
    assert_eq!(ack["kind"], "watch-saved");
    let session = watch_id(&g, &id);
    let persisted = fs::read(dir.path().join("profiles.json")).unwrap();
    let data: Value = serde_json::from_slice(&persisted).unwrap();
    assert_eq!(data[0]["resident"]["enabled"], true);
    fs::create_dir(dir.path().join("profiles.json.tmp")).unwrap();
    advance(&mut g, 100);
    assert!(
        state
            .watch_action(
                &mut g,
                &id,
                &json!({"action":"watch-stop","sessionId":session})
            )
            .is_err()
    );
    assert_eq!(watch_id(&g, &id), session);
    assert_eq!(
        fs::read(dir.path().join("profiles.json")).unwrap(),
        persisted
    );
    fs::remove_dir(dir.path().join("profiles.json.tmp")).unwrap();
    advance(&mut g, 100);
    state
        .watch_action(
            &mut g,
            &id,
            &json!({"action":"watch-stop","sessionId":session}),
        )
        .unwrap();
    assert!(g.players[&id].watch.is_none());
    let data: Value =
        serde_json::from_slice(&fs::read(dir.path().join("profiles.json")).unwrap()).unwrap();
    assert!(data[0]["resident"].is_null());
    assert_eq!(data[0]["rings"], 20);
}
