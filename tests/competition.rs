use chao_game_server::{
    competition,
    game::Game,
    service::{AppState, Config},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

// Synthetic catalogs exercise the locally extracted schema. No native assets,
// original data rows, credentials, or live profiles are included in this test.
fn race_catalog() -> Value {
    json!({
        "courses":[{
            "id":"beginner-0-1", "name":"Synthetic swim reference", "group":"beginner", "level":1,
            "available":false, "geometryValidated":false,
            "source":{"courseIndex":0,"tableIndex":0,"rivalSetIndex":-1,"raw":[-1,0,0,0,-1]}
        },{
            "id":"beginner-0-2", "name":"Unproved next course", "group":"beginner", "level":2,
            "available":false, "geometryValidated":false,
            "source":{"courseIndex":0,"tableIndex":1,"rivalSetIndex":-1,"raw":[-1,0,0,0,-1]}
        }],
        "raceBots":(0..4).map(|index|json!({
            "index":index,"address":format!("synthetic-{index}"),"statMode":0,"sharedStat":1,
            "perStatPairs":{"swim":[1,1],"fly":[1,1],"run":[1,1],"power":[1,1],
                "stamina":[1,1],"luck":[1,1],"intelligence":[1,1]},
            "appearanceBytes":vec![0;16],"morphParameters":vec![0;8]
        })).collect::<Vec<_>>()
    })
}

fn karate_catalog() -> Value {
    json!({"tiers":[{
        "id":"karate-beginner","name":"Synthetic Beginner",
        "source":{"nativeTablesJoined":true,"rosterStartDescriptor":2,"tierSelectionCallsiteValidated":false},
        "opponents":[{
            "id":"native-karate-opponent-2","name":"Synthetic CPU","nativeIndex":2,
            "skills":[1,1,1,1,1,1,1,0],"appearance":{"synthetic":true}
        },{
            "id":"native-karate-opponent-3","name":"Unproved next CPU","nativeIndex":3,
            "skills":[10,10,10,10,10,10,10,0],"appearance":{"synthetic":true}
        }]
    }]})
}

fn write_catalogs(path: &Path) {
    fs::create_dir_all(path.join("activities")).unwrap();
    fs::write(
        path.join("activities/races.json"),
        race_catalog().to_string(),
    )
    .unwrap();
    fs::write(
        path.join("activities/karate.json"),
        karate_catalog().to_string(),
    )
    .unwrap();
}

fn fixture() -> (tempfile::TempDir, AppState, String, String) {
    let dir = tempfile::tempdir().unwrap();
    write_catalogs(dir.path());
    let state = AppState::load(Config::local(dir.path().to_owned(), true)).unwrap();
    let (id, token) = {
        let mut game = state.game.lock().unwrap();
        let session = game
            .create_session(&json!({"name":"Competition test"}))
            .unwrap();
        let id = session["player"]["id"].as_str().unwrap().to_owned();
        let token = session["token"].as_str().unwrap().to_owned();
        game.connect(&id);
        state.persist(&mut game).unwrap();
        (id, token)
    };
    (dir, state, id, token)
}

fn advance(game: &mut Game, ms: usize) {
    for _ in 0..ms.div_ceil(50) {
        game.tick(0.05);
    }
}

fn choice(state: &AppState, game: &mut Game, id: &str, message: Value) -> Value {
    advance(game, 100);
    state.competition_action(game, id, &message).unwrap()
}

fn public(game: &Game, id: &str) -> Value {
    competition::public(&game.players[id])
}

fn start(state: &AppState, game: &mut Game, id: &str, kind: &str, matches: u32) -> Value {
    let revision = public(game, id)["revision"].clone();
    choice(
        state,
        game,
        id,
        json!({"type":"action","action":"competition-start",
        "expectedRevision":revision,"kind":kind,"matches":matches}),
    )
}

fn tick_and_save(state: &AppState, game: &mut Game, ms: usize) {
    for _ in 0..ms.div_ceil(50) {
        game.tick(0.05);
        if competition::has_pending(game) {
            state.persist(game).unwrap();
        }
    }
}

fn finish_one(state: &AppState, game: &mut Game, id: &str, count: usize) -> Value {
    for _ in 0..5000 {
        if public(game, id)["results"].as_array().unwrap().len() >= count {
            return public(game, id)["results"][count - 1].clone();
        }
        tick_and_save(state, game, 50);
    }
    panic!("server competition did not produce a durable result");
}

#[test]
fn explicit_bounded_opt_in_runs_one_race_and_one_karate_while_owner_is_away() {
    for kind in ["race", "karate"] {
        let (_dir, state, id, token) = fixture();
        let mut game = state.game.lock().unwrap();
        let before = game.players[&id].rings;
        let stats = game.players[&id].stats.clone();
        tick_and_save(&state, &mut game, 10_000);
        assert!(public(&game, &id)["plan"].is_null());
        assert!(public(&game, &id)["results"].as_array().unwrap().is_empty());
        assert_eq!(game.players[&id].rings, before);
        assert_eq!(start(&state, &mut game, &id, kind, 1)["accepted"], true);
        let match_id = public(&game, &id)["active"]["id"].clone();
        assert!(match_id.is_string());
        game.disconnect(&id);
        let result = finish_one(&state, &mut game, &id, 1);
        assert_eq!(result["id"], match_id);
        assert_eq!(result["kind"], kind);
        assert_eq!(result["saved"], true);
        assert!(result["rank"].as_u64().unwrap() >= 1);
        assert!(result["rings"].as_u64().unwrap() > 0);
        assert_eq!(
            game.players[&id].rings,
            before + result["rings"].as_u64().unwrap() as u32
        );
        assert_eq!(game.players[&id].stats, stats);
        assert!(public(&game, &id)["plan"].is_null());
        tick_and_save(&state, &mut game, 90_000);
        assert_eq!(public(&game, &id)["results"].as_array().unwrap().len(), 1);
        assert_eq!(
            game.players
                .values()
                .filter(|p| p.token_hash == game.players[&id].token_hash)
                .count(),
            1
        );
        game.connect(&id);
        assert_eq!(game.player_for_token(&token).unwrap().id, id);
        assert_eq!(public(&game, &id)["results"][0], result);
    }
}

#[test]
fn strict_intents_reject_claimed_results_unknown_kinds_and_excess_budget() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    for extra in [
        json!({"kind":"race","matches":0}),
        json!({"kind":"race","matches":4}),
        json!({"kind":"race","matches":1.5}),
        json!({"kind":"race","matches":1,"rings":999999}),
        json!({"kind":"race","matches":1,"seed":0}),
        json!({"kind":"race","matches":1,"elapsedMs":999999}),
        json!({"kind":"race","matches":1,"courseId":"beginner-0-2"}),
        json!({"kind":"karate","matches":1,"opponentId":"native-karate-opponent-3"}),
        json!({"kind":"unknown","matches":1}),
    ] {
        let mut message = extra;
        message["type"] = json!("action");
        message["action"] = json!("competition-start");
        message["expectedRevision"] = json!(0);
        assert_eq!(choice(&state, &mut game, &id, message)["accepted"], false);
        assert!(public(&game, &id)["plan"].is_null());
    }
    assert_eq!(game.players[&id].rings, 20);
    assert_eq!(start(&state, &mut game, &id, "race", 1)["accepted"], true);
    let active = public(&game, &id)["active"]["id"].clone();
    for action in [
        "competition-result",
        "competition-finish",
        "competition-claim",
    ] {
        assert_eq!(
            choice(
                &state,
                &mut game,
                &id,
                json!({"type":"action","action":action,"matchId":active,"rank":1,"rings":999999})
            )["accepted"],
            false
        );
    }
    assert_eq!(public(&game, &id)["active"]["id"], active);
    assert_eq!(game.players[&id].rings, 20);
}

#[test]
fn duplicate_sockets_stale_revisions_and_other_profiles_cannot_replace_an_entry() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    game.connect(&id);
    assert_eq!(start(&state, &mut game, &id, "race", 1)["accepted"], true);
    let plan = public(&game, &id)["plan"]["id"].clone();
    let active = public(&game, &id)["active"]["id"].clone();
    for _ in 0..3 {
        assert_eq!(
            choice(
                &state,
                &mut game,
                &id,
                json!({"type":"action","action":"competition-start","expectedRevision":0,"kind":"karate","matches":3})
            )["accepted"],
            false
        );
        assert_eq!(
            choice(
                &state,
                &mut game,
                &id,
                json!({"type":"action","action":"competition-stop","expectedRevision":0,"planId":plan})
            )["accepted"],
            false
        );
    }
    assert_eq!(public(&game, &id)["active"]["id"], active);
    let other = game
        .create_session(&json!({"name":"Another profile"}))
        .unwrap()["player"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    game.connect(&other);
    let revision = public(&game, &id)["revision"].clone();
    assert_eq!(
        choice(
            &state,
            &mut game,
            &other,
            json!({"type":"action","action":"competition-stop","expectedRevision":revision,"planId":plan})
        )["accepted"],
        false
    );
    game.disconnect(&id);
    assert_eq!(game.players[&id].connections, 1);
    assert_eq!(public(&game, &id)["active"]["id"], active);
    game.disconnect(&id);
    let result = finish_one(&state, &mut game, &id, 1);
    assert_eq!(result["id"], active);
    assert_eq!(game.players[&other].rings, 20);
}

#[test]
fn running_entry_restarts_with_frozen_seed_roster_clock_and_no_downtime_catchup() {
    let (dir, state, id, token) = fixture();
    let before;
    {
        let mut game = state.game.lock().unwrap();
        start(&state, &mut game, &id, "karate", 1);
        tick_and_save(&state, &mut game, 1500);
        game.dirty = true;
        state.persist(&mut game).unwrap();
        before = public(&game, &id);
        assert!(before["active"]["elapsedMs"].as_u64().unwrap() > 0);
        assert!(before["results"].as_array().unwrap().is_empty());
        let restored = Game::new(
            game.export_profiles(),
            BTreeMap::new(),
            false,
            9_999_999_999_999.,
        )
        .unwrap();
        assert_eq!(public(&restored, &id)["active"], before["active"]);
        assert_eq!(restored.players[&id].rings, 20);
    }
    let restarted = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let mut game = restarted.game.lock().unwrap();
    assert_eq!(game.player_for_token(&token).unwrap().id, id);
    assert_eq!(public(&game, &id)["active"], before["active"]);
    let result = finish_one(&restarted, &mut game, &id, 1);
    assert_eq!(result["id"], before["active"]["id"]);
    let rings = game.players[&id].rings;
    state.persist(&mut game).unwrap();
    drop(game);
    let final_restart = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let mut game = final_restart.game.lock().unwrap();
    tick_and_save(&final_restart, &mut game, 100_000);
    assert_eq!(game.players[&id].rings, rings);
    assert_eq!(public(&game, &id)["results"].as_array().unwrap().len(), 1);
    assert_eq!(public(&game, &id)["results"][0], result);
}

#[test]
fn start_and_stop_rollback_the_lease_revision_and_budget_when_disk_save_fails() {
    let (dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    let bytes = fs::read(dir.path().join("profiles.json")).unwrap();
    let before = public(&game, &id);
    fs::create_dir(dir.path().join("profiles.json.tmp")).unwrap();
    advance(&mut game, 100);
    assert!(state.competition_action(&mut game, &id,
        &json!({"type":"action","action":"competition-start","expectedRevision":0,"kind":"race","matches":3})).is_err());
    assert_eq!(public(&game, &id), before);
    assert!(game.players[&id].activity_lease.is_none());
    assert_eq!(fs::read(dir.path().join("profiles.json")).unwrap(), bytes);
    fs::remove_dir(dir.path().join("profiles.json.tmp")).unwrap();
    assert_eq!(start(&state, &mut game, &id, "race", 3)["accepted"], true);
    let before = public(&game, &id);
    let lease = game.players[&id].activity_lease.clone();
    let persisted = fs::read(dir.path().join("profiles.json")).unwrap();
    fs::create_dir(dir.path().join("profiles.json.tmp")).unwrap();
    advance(&mut game, 100);
    let before_action = public(&game, &id);
    assert!(state.competition_action(&mut game, &id,
        &json!({"type":"action","action":"competition-stop","expectedRevision":before["revision"],"planId":before["plan"]["id"]})).is_err());
    assert_eq!(public(&game, &id), before_action);
    assert_eq!(game.players[&id].activity_lease, lease);
    assert_eq!(
        fs::read(dir.path().join("profiles.json")).unwrap(),
        persisted
    );
    fs::remove_dir(dir.path().join("profiles.json.tmp")).unwrap();
    let stopped = choice(
        &state,
        &mut game,
        &id,
        json!({"type":"action","action":"competition-stop","expectedRevision":before["revision"],"planId":before["plan"]["id"]}),
    );
    assert_eq!(stopped["accepted"], true);
    assert!(game.players[&id].activity_lease.is_none());
    tick_and_save(&state, &mut game, 200_000);
    assert_eq!(game.players[&id].rings, 20);
    assert!(public(&game, &id)["results"].as_array().unwrap().is_empty());
}

#[test]
fn a_foreign_training_lease_survives_restart_and_blocks_competition() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    let lease = json!({"kind":"training","id":"synthetic-job","revision":7});
    game.players.get_mut(&id).unwrap().activity_lease = Some(lease.clone());
    assert_eq!(start(&state, &mut game, &id, "race", 1)["accepted"], false);
    assert_eq!(game.players[&id].activity_lease, Some(lease.clone()));
    let mut restored = Game::new(game.export_profiles(), BTreeMap::new(), false, 0.).unwrap();
    assert_eq!(restored.players[&id].activity_lease, Some(lease));
    assert!(public(&restored, &id)["plan"].is_null());
    restored.connect(&id);
    let stats = restored.players[&id].stats.clone();
    let (x, z) = (restored.players[&id].x, restored.players[&id].z);
    restored.set_input(&id, &json!({"dx":1,"dz":1,"jump":true,"boost":true}));
    for action in ["mission", "tiny-start", "watch-start", "train-run"] {
        advance(&mut restored, 100);
        restored.action(
            &id,
            &json!({"type":"action","action":action,"gameId":"memory"}),
        );
    }
    advance(&mut restored, 1000);
    assert_eq!((restored.players[&id].x, restored.players[&id].z), (x, z));
    assert_eq!(restored.players[&id].stats, stats);
    assert!(restored.players[&id].tiny_game.is_none());
    assert!(restored.players[&id].watch.is_none());
}

#[test]
fn foreign_training_and_future_profile_fields_round_trip_without_being_reinterpreted() {
    let (_dir, state, id, _) = fixture();
    let game = state.game.lock().unwrap();
    let mut profiles = game.export_profiles();
    let training = json!({"jobId":"synthetic-training","status":"running","futureField":[1,2,3]});
    let future = json!({"version":19,"unknown":{"mustSurvive":true},"nullable":null});
    profiles[0]["trainingJob"] = training.clone();
    profiles[0]["futureFeature"] = future.clone();
    let mut restored = Game::new(profiles, BTreeMap::new(), false, 1234.).unwrap();
    assert_eq!(restored.export_profiles()[0]["trainingJob"], training);
    assert_eq!(restored.export_profiles()[0]["futureFeature"], future);
    assert!(competition::training_conflict(&restored.players[&id]));
    restored.connect(&id);
    advance(&mut restored, 100);
    restored.action(&id, &json!({"type":"action","action":"competition-start","expectedRevision":0,"kind":"race","matches":1}));
    assert!(public(&restored, &id)["plan"].is_null());
    assert_eq!(restored.export_profiles()[0]["trainingJob"], training);
    assert_eq!(restored.export_profiles()[0]["futureFeature"], future);
    let snapshot = restored.snapshot(&id).to_string();
    assert!(!snapshot.contains("futureFeature"));
    assert!(!snapshot.contains("synthetic-training"));
}

#[test]
fn competition_lease_blocks_manual_games_training_and_reward_pickups() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    start(&state, &mut game, &id, "race", 1);
    let x = game.players[&id].x;
    let z = game.players[&id].z;
    let zone = game.players[&id].zone.clone();
    game.rings[0].x = x;
    game.rings[0].z = z;
    game.rings[0].zone = zone;
    let stats = game.players[&id].stats.clone();
    for action in [
        "mission",
        "race",
        "race-enter",
        "karate-enter",
        "tiny-start",
        "train-run",
        "train-swim",
        "mount-dark-walker",
        "feed",
        "buy",
    ] {
        advance(&mut game, 100);
        game.action(
            &id,
            &json!({"type":"action","action":action,"courseId":"beginner-0-1",
            "tier":"karate-beginner","gameId":"memory","item":"garden-fruit"}),
        );
    }
    game.set_input(&id, &json!({"dx":1,"dz":1,"jump":true,"boost":true}));
    tick_and_save(&state, &mut game, 1000);
    assert_eq!(game.players[&id].x, x);
    assert_eq!(game.players[&id].z, z);
    assert_eq!(game.players[&id].rings, 20);
    assert_eq!(game.players[&id].stats, stats);
    assert_eq!(game.rings[0].available_at, 0.);
    assert!(game.players[&id].race.is_none());
    assert!(game.players[&id].karate.is_none());
    assert!(game.players[&id].tiny_game.is_none());
    assert!(game.players[&id].vehicle.is_none());
}

#[test]
fn reward_and_receipt_are_atomic_retryable_and_not_replayed_by_disconnect_or_restart() {
    for kind in ["race", "karate"] {
        let (dir, state, id, token) = fixture();
        let mut game = state.game.lock().unwrap();
        start(&state, &mut game, &id, kind, 1);
        game.disconnect(&id);
        let match_id = public(&game, &id)["active"]["id"].clone();
        let baseline = game.players[&id].rings;
        let mut staged_credit = None;
        for _ in 0..4000 {
            game.tick(0.05);
            if competition::has_pending(&game) {
                let profiles = game.export_profiles();
                let projected = profiles.iter().find(|p| p["id"] == id).unwrap();
                if projected["rings"].as_u64().unwrap() > u64::from(baseline) {
                    staged_credit = Some(projected["rings"].as_u64().unwrap() as u32 - baseline);
                    break;
                }
                state.persist(&mut game).unwrap();
            }
        }
        let credit =
            staged_credit.expect("winning server simulation should stage a positive reward");
        assert_eq!(game.players[&id].rings, baseline);
        assert!(public(&game, &id)["results"].as_array().unwrap().is_empty());
        let saved_before = fs::read(dir.path().join("profiles.json")).unwrap();
        fs::create_dir(dir.path().join("profiles.json.tmp")).unwrap();
        for _ in 0..3 {
            assert!(state.persist(&mut game).is_err());
            competition::save_failed(&mut game);
            advance(&mut game, 500);
            assert_eq!(game.players[&id].rings, baseline);
            assert!(competition::has_pending(&game));
        }
        assert_eq!(
            fs::read(dir.path().join("profiles.json")).unwrap(),
            saved_before
        );
        assert!(
            !game
                .drain_events()
                .iter()
                .any(|event| event.message["kind"] == "competition-result")
        );
        fs::remove_dir(dir.path().join("profiles.json.tmp")).unwrap();
        state.persist(&mut game).unwrap();
        assert_eq!(game.players[&id].rings, baseline + credit);
        let result = public(&game, &id)["results"][0].clone();
        assert_eq!(result["id"], match_id);
        assert_eq!(result["rings"], credit);
        assert_eq!(result["saved"], true);
        let emitted = game
            .drain_events()
            .into_iter()
            .filter(|event| event.message["kind"] == "competition-result")
            .count();
        assert_eq!(emitted, 1);
        state.persist(&mut game).unwrap();
        tick_and_save(&state, &mut game, 100_000);
        assert_eq!(game.players[&id].rings, baseline + credit);
        assert!(
            !game
                .drain_events()
                .iter()
                .any(|event| event.message["kind"] == "competition-result")
        );
        drop(game);
        let restarted = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
        let mut game = restarted.game.lock().unwrap();
        assert_eq!(
            game.player_for_token(&token).unwrap().rings,
            baseline + credit
        );
        tick_and_save(&restarted, &mut game, 100_000);
        assert_eq!(public(&game, &id)["results"][0], result);
        assert_eq!(game.players[&id].rings, baseline + credit);
    }
}

#[test]
fn spectator_replay_returns_only_saved_server_results_without_reward_side_effects() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    start(&state, &mut game, &id, "race", 1);
    let match_id = public(&game, &id)["active"]["id"].clone();
    game.drain_events();
    choice(
        &state,
        &mut game,
        &id,
        json!({"type":"action","action":"competition-replay","matchId":match_id}),
    );
    assert!(
        !game
            .drain_events()
            .iter()
            .any(|event| event.message["kind"] == "competition-replay")
    );
    let result = finish_one(&state, &mut game, &id, 1);
    let spectator = game.create_session(&json!({"name":"Spectator"})).unwrap()["player"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    game.connect(&spectator);
    game.disconnect(&id);
    game.drain_events();
    let rings = game.players[&id].rings;
    let results = public(&game, &id)["results"].clone();
    for _ in 0..3 {
        choice(
            &state,
            &mut game,
            &spectator,
            json!({"type":"action","action":"competition-replay","profileId":id,"matchId":result["id"]}),
        );
        let events = game.drain_events();
        let replay = events
            .iter()
            .find(|event| event.message["kind"] == "competition-replay")
            .expect("spectator receives saved replay");
        assert_eq!(replay.target.as_deref(), Some(spectator.as_str()));
        assert!(!replay.message.to_string().contains("tokenHash"));
        assert!(replay.message.to_string().contains("placements"));
        assert!(replay.message.to_string().contains("frames"));
    }
    assert_eq!(game.players[&id].rings, rings);
    assert_eq!(game.players[&spectator].rings, 20);
    assert_eq!(public(&game, &id)["results"], results);
}

#[test]
fn resident_wandering_without_competition_opt_in_never_grants_competition_rewards() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    advance(&mut game, 100);
    state
        .watch_action(
            &mut game,
            &id,
            &json!({"type":"action","action":"watch-start"}),
        )
        .unwrap();
    let before = game.players[&id].rings;
    let stats = game.players[&id].stats.clone();
    game.disconnect(&id);
    tick_and_save(&state, &mut game, 180_000);
    assert_eq!(game.players[&id].rings, before);
    assert_eq!(game.players[&id].stats, stats);
    assert!(public(&game, &id)["plan"].is_null());
    assert!(public(&game, &id)["results"].as_array().unwrap().is_empty());
    assert!(game.players[&id].race.is_none());
    assert!(game.players[&id].karate.is_none());
    assert!(game.players[&id].tiny_game.is_none());
}

#[test]
fn an_opted_in_resident_pauses_for_competition_and_resumes_without_passive_payouts() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    advance(&mut game, 100);
    state
        .watch_action(
            &mut game,
            &id,
            &json!({"type":"action","action":"watch-start"}),
        )
        .unwrap();
    let watch = game.players[&id].watch.as_ref().unwrap().id.clone();
    let position = (game.players[&id].x, game.players[&id].z);
    let baseline = game.players[&id].rings;
    assert_eq!(start(&state, &mut game, &id, "race", 1)["accepted"], true);
    game.disconnect(&id);
    tick_and_save(&state, &mut game, 1000);
    assert_eq!((game.players[&id].x, game.players[&id].z), position);
    assert_eq!(game.players[&id].rings, baseline);
    let result = finish_one(&state, &mut game, &id, 1);
    assert_eq!(game.players[&id].watch.as_ref().unwrap().id, watch);
    tick_and_save(&state, &mut game, 30_000);
    assert_eq!(
        game.players[&id].rings,
        baseline + result["rings"].as_u64().unwrap() as u32
    );
    assert_eq!(public(&game, &id)["results"].as_array().unwrap().len(), 1);
    assert!(
        game.snapshot("spectator")["players"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == id && p["watch"]["resident"] == true)
    );
}

#[test]
fn exactly_the_requested_budget_is_scheduled_with_a_saved_gap_between_matches() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    start(&state, &mut game, &id, "race", 2);
    let first = finish_one(&state, &mut game, &id, 1);
    let waiting = public(&game, &id);
    assert!(waiting["active"].is_null());
    assert!(waiting["plan"]["waitRemainingMs"].as_u64().unwrap() > 0);
    let remaining = waiting["plan"]["waitRemainingMs"].as_u64().unwrap();
    assert!(remaining <= 60_000);
    tick_and_save(&state, &mut game, remaining.saturating_sub(100) as usize);
    assert!(public(&game, &id)["active"].is_null());
    let second = finish_one(&state, &mut game, &id, 2);
    assert_ne!(first["id"], second["id"]);
    assert!(public(&game, &id)["plan"].is_null());
    tick_and_save(&state, &mut game, 180_000);
    assert_eq!(public(&game, &id)["results"].as_array().unwrap().len(), 2);
}

#[test]
fn malformed_saved_ledgers_fail_closed_before_scheduling_or_crediting() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    start(&state, &mut game, &id, "race", 1);
    let valid = game.export_profiles();
    let mut exhausted_wait = valid.clone();
    exhausted_wait[0]["competition"]["active"] = Value::Null;
    exhausted_wait[0]["competition"]["plan"]["remaining"] = json!(0);
    assert!(Game::new(exhausted_wait, BTreeMap::new(), false, 0.).is_err());
    let mut wrong_kind = valid.clone();
    wrong_kind[0]["competition"]["active"]["kind"] = json!("karate");
    assert!(Game::new(wrong_kind, BTreeMap::new(), false, 0.).is_err());
    let mut wrong_lease = valid;
    wrong_lease[0]["activityLease"] = json!({"kind":"training","id":"conflicting-job"});
    assert!(Game::new(wrong_lease, BTreeMap::new(), false, 0.).is_err());
    finish_one(&state, &mut game, &id, 1);
    let valid = game.export_profiles();
    let mut duplicate = valid.clone();
    let receipt = duplicate[0]["competition"]["results"][0].clone();
    duplicate[0]["competition"]["results"]
        .as_array_mut()
        .unwrap()
        .push(receipt);
    assert!(Game::new(duplicate, BTreeMap::new(), false, 0.).is_err());
    let mut wrong_rank = valid;
    wrong_rank[0]["competition"]["results"][0]["rank"] = json!(99);
    assert!(Game::new(wrong_rank, BTreeMap::new(), false, 0.).is_err());
}

#[test]
fn a_full_wallet_still_commits_one_zero_credit_receipt() {
    let (dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    game.players.get_mut(&id).unwrap().rings = 999999;
    start(&state, &mut game, &id, "karate", 1);
    let result = finish_one(&state, &mut game, &id, 1);
    assert_eq!(result["rank"], 1);
    assert_eq!(result["earnedRings"], 50);
    assert_eq!(result["rings"], 0);
    assert_eq!(public(&game, &id)["completedTotal"], 1);
    assert_eq!(game.players[&id].rings, 999999);
    drop(game);
    let restarted = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let mut game = restarted.game.lock().unwrap();
    tick_and_save(&restarted, &mut game, 120_000);
    assert_eq!(public(&game, &id)["results"][0], result);
    assert_eq!(public(&game, &id)["completedTotal"], 1);
    assert_eq!(game.players[&id].rings, 999999);
}

#[test]
#[ignore = "requires locally extracted catalogs; run with CHAO_COMPETITION_REFERENCE_DIR"]
fn locally_extracted_rosters_complete_both_away_slices_and_restore_saved_replays() {
    let references = std::env::var_os("CHAO_COMPETITION_REFERENCE_DIR")
        .map(std::path::PathBuf::from)
        .expect("set CHAO_COMPETITION_REFERENCE_DIR to the local .local/activities directory before running this ignored proof");
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("activities")).unwrap();
    // Copy the two read-only reference catalogs into an isolated temporary
    // authority. Never open or copy the live service's profiles or terrain.
    for name in ["races.json", "karate.json"] {
        fs::copy(
            references.join(name),
            dir.path().join("activities").join(name),
        )
        .unwrap_or_else(|error| panic!("could not copy required local {name}: {error}"));
    }
    let state = AppState::load(Config::local(dir.path().to_owned(), true)).unwrap();
    let mut game = state.game.lock().unwrap();
    let catalog = game.competition_catalog.public();
    assert_eq!(catalog["race"], true);
    assert_eq!(catalog["karate"], true);
    assert!(
        !game
            .activities
            .courses
            .iter()
            .find(|c| c.id == "beginner-0-1")
            .unwrap()
            .available
    );
    let session = game
        .create_session(&json!({"name":"Local roster proof"}))
        .unwrap();
    let id = session["player"]["id"].as_str().unwrap().to_owned();
    let token = session["token"].as_str().unwrap().to_owned();
    let baseline = game.players[&id].rings;
    let mut summaries = Vec::new();
    for (index, kind) in ["race", "karate"].into_iter().enumerate() {
        game.connect(&id);
        assert_eq!(start(&state, &mut game, &id, kind, 1)["accepted"], true);
        let match_id = public(&game, &id)["active"]["id"].clone();
        game.disconnect(&id);
        assert_eq!(game.players[&id].connections, 0);
        let result = finish_one(&state, &mut game, &id, index + 1);
        assert_eq!(result["id"], match_id);
        assert_eq!(result["kind"], kind);
        assert_eq!(result["saved"], true);
        assert_eq!(result["rank"], 1);
        summaries.push(result);
    }
    assert!(
        !game
            .activities
            .courses
            .iter()
            .find(|c| c.id == "beginner-0-1")
            .unwrap()
            .available
    );
    let credited: u32 = summaries
        .iter()
        .map(|result| result["rings"].as_u64().unwrap() as u32)
        .sum();
    assert_eq!(game.players[&id].rings, baseline + credited);
    assert_eq!(credited, 75);
    state.persist(&mut game).unwrap();
    drop(game);
    let restarted = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let mut game = restarted.game.lock().unwrap();
    assert_eq!(game.player_for_token(&token).unwrap().id, id);
    assert_eq!(game.players[&id].rings, baseline + credited);
    assert_eq!(public(&game, &id)["completedTotal"], 2);
    game.connect(&id);
    game.drain_events();
    for result in summaries {
        choice(
            &restarted,
            &mut game,
            &id,
            json!({"type":"action","action":"competition-replay","matchId":result["id"]}),
        );
        let events = game.drain_events();
        let replay = events
            .iter()
            .find(|event| event.message["kind"] == "competition-replay")
            .expect("saved local-reference replay survives restart");
        assert_eq!(replay.message["result"], result);
        assert_eq!(
            replay.message["simulation"]["placements"],
            result["placements"]
        );
        eprintln!(
            "local roster proof: kind={} rank={} credited={} durable=true",
            result["kind"].as_str().unwrap(),
            result["rank"],
            result["rings"]
        );
    }
    tick_and_save(&restarted, &mut game, 180_000);
    assert_eq!(game.players[&id].rings, baseline + credited);
    assert_eq!(public(&game, &id)["completedTotal"], 2);
}

#[test]
fn corrupted_saved_replay_frames_are_rejected_before_a_restart_can_resume_them() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    start(&state, &mut game, &id, "race", 1);
    let mut active = game.export_profiles();
    active[0]["competition"]["active"]["simulation"]["frames"][0]["competitors"][0]["progress"] =
        json!(999999);
    assert!(Game::new(active, BTreeMap::new(), false, 0.).is_err());
    finish_one(&state, &mut game, &id, 1);
    let mut completed = game.export_profiles();
    completed[0]["competition"]["results"][0]["simulation"]["frames"][0]["competitors"][0]["stamina"] =
        json!(999999);
    assert!(Game::new(completed, BTreeMap::new(), false, 0.).is_err());
}

#[test]
fn a_replaced_runtime_lease_pauses_competition_and_survives_safe_cancellation() {
    let (dir, state, id, token) = fixture();
    let mut game = state.game.lock().unwrap();
    assert_eq!(start(&state, &mut game, &id, "race", 2)["accepted"], true);
    let before = public(&game, &id);
    let baseline = game.players[&id].rings;
    let saved_before = fs::read(dir.path().join("profiles.json")).unwrap();
    let foreign = json!({"kind":"training","id":"runtime-training-job","revision":21});
    game.players.get_mut(&id).unwrap().activity_lease = Some(foreign.clone());
    advance(&mut game, 10_000);
    assert_eq!(public(&game, &id)["active"], before["active"]);
    assert_eq!(public(&game, &id)["plan"], before["plan"]);
    assert_eq!(game.players[&id].rings, baseline);
    assert_eq!(game.players[&id].activity_lease, Some(foreign.clone()));
    game.dirty = true;
    assert!(state.persist(&mut game).is_err());
    assert_eq!(
        fs::read(dir.path().join("profiles.json")).unwrap(),
        saved_before
    );
    assert_eq!(game.players[&id].activity_lease, Some(foreign.clone()));
    assert_eq!(
        choice(
            &state,
            &mut game,
            &id,
            json!({
                "type":"action","action":"competition-stop",
                "expectedRevision":before["revision"],"planId":before["plan"]["id"]
            })
        )["accepted"],
        true
    );
    assert!(public(&game, &id)["plan"].is_null());
    assert!(public(&game, &id)["active"].is_null());
    assert_eq!(game.players[&id].activity_lease, Some(foreign.clone()));
    assert_eq!(game.players[&id].rings, baseline);
    assert!(public(&game, &id)["results"].as_array().unwrap().is_empty());
    let persisted: Value =
        serde_json::from_slice(&fs::read(dir.path().join("profiles.json")).unwrap()).unwrap();
    let saved = persisted
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap();
    assert_eq!(saved["activityLease"], foreign);
    assert!(saved["competition"]["plan"].is_null());
    drop(game);
    let restarted = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let mut game = restarted.game.lock().unwrap();
    assert_eq!(
        game.player_for_token(&token).unwrap().activity_lease,
        Some(foreign.clone())
    );
    advance(&mut game, 180_000);
    assert_eq!(game.players[&id].activity_lease, Some(foreign));
    assert!(public(&game, &id)["plan"].is_null());
    assert!(public(&game, &id)["results"].as_array().unwrap().is_empty());
    assert_eq!(game.players[&id].rings, baseline);
}
