//! Care host integration tests use adapted fruit and synthetic profile data only.
use chao_game_server::{
    activities::ActivityCatalog,
    care::CareState,
    game::{ALL_STATS, Event, Game, Race, Terrain, hash_token},
    service::{AppState, Config},
};
use serde_json::{Value, json};
use std::{
    fs,
    sync::{Arc, Barrier},
};
use uuid::Uuid;

fn fixture() -> (tempfile::TempDir, AppState, String) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let id = {
        let mut game = state.game.lock().unwrap();
        let session = game
            .create_session(&json!({"name":"Care fixture"}))
            .unwrap();
        let id = session["player"]["id"].as_str().unwrap().to_owned();
        game.connect(&id);
        at_shop(&mut game, &id);
        state.persist(&mut game).unwrap();
        id
    };
    (dir, state, id)
}

fn at_shop(game: &mut Game, id: &str) {
    let player = game.players.get_mut(id).unwrap();
    player.zone = "hub".into();
    player.x = -4.;
    player.z = 8.;
    player.y = 0.;
    player.ground_y = 0.;
    player.jump_offset = 0.;
}

fn request(revision: u64, intent: Value) -> Value {
    json!({"type":"care","requestId":Uuid::new_v4().to_string(),
        "expectedRevision":revision,"intent":intent})
}

fn purchase(revision: u64) -> Value {
    request(revision, json!({"kind":"purchase","itemId":"garden-fruit"}))
}

fn consume(revision: u64) -> Value {
    request(revision, json!({"kind":"consume","itemId":"garden-fruit"}))
}

fn event_state(game: &Game) -> Vec<Value> {
    game.events
        .iter()
        .map(|event| json!({"target":event.target,"message":event.message}))
        .collect()
}

#[test]
fn inventory_needs_effects_and_committed_retries_survive_reload() {
    let (dir, state, id) = fixture();
    let buy = purchase(0);
    let eat = consume(1);
    let (buy_result, eat_result, expected_stats, expected_care) = {
        let mut game = state.game.lock().unwrap();
        let player = game.players.get_mut(&id).unwrap();
        player.energy = 55.;
        player.hunger = 45.;
        player.happiness = 50.;
        let original_stats = player.stats.clone();
        let buy_result = state.care_action(&mut game, &id, &buy).unwrap();
        assert_eq!(buy_result["accepted"], true);
        assert_eq!(game.players[&id].rings, 17);
        assert_eq!(game.players[&id].care.inventory["garden-fruit"], 1);
        assert_eq!(game.players[&id].stats, original_stats);
        let eat_result = state.care_action(&mut game, &id, &eat).unwrap();
        assert_eq!(eat_result["accepted"], true);
        assert_eq!(eat_result["effects"]["points"]["stamina"], 25);
        assert_eq!(eat_result["effects"]["levelsChanged"], false);
        assert_eq!(eat_result["effects"]["gradesChanged"], false);
        let player = &game.players[&id];
        assert!(player.care.inventory.is_empty());
        assert_eq!(
            (player.energy, player.hunger, player.happiness),
            (90., 75., 58.)
        );
        assert_eq!(player.care.revision, 2);
        for name in ALL_STATS {
            assert_eq!(player.stats[name].level, original_stats[name].level);
            assert_eq!(player.stats[name].grade, original_stats[name].grade);
            assert_eq!(
                player.stats[name].points,
                original_stats[name].points + if name == "stamina" { 25 } else { 0 }
            );
        }
        (
            buy_result,
            eat_result,
            player.stats.clone(),
            player.care.clone(),
        )
    };
    let persisted = fs::read(dir.path().join("profiles.json")).unwrap();
    let restored = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let mut game = restored.game.lock().unwrap();
    // Replays resolve before connection, activity, revision and cooldown checks.
    assert_eq!(game.players[&id].connections, 0);
    for (message, prior) in [(&buy, buy_result), (&eat, eat_result)] {
        let response = restored.care_action(&mut game, &id, message).unwrap();
        assert_eq!(response["accepted"], true);
        assert_eq!(response["replayed"], true);
        assert_eq!(response["revision"], prior["revision"]);
        assert_eq!(response["effects"], prior["effects"]);
    }
    assert_eq!(game.players[&id].care, expected_care);
    assert_eq!(game.players[&id].stats, expected_stats);
    assert_eq!(game.players[&id].rings, 17);
    assert_eq!(
        (
            game.players[&id].energy,
            game.players[&id].hunger,
            game.players[&id].happiness
        ),
        (90., 75., 58.)
    );
    assert_eq!(
        fs::read(dir.path().join("profiles.json")).unwrap(),
        persisted
    );
}

#[test]
fn disk_failure_rolls_back_consumption_effects_cooldown_receipt_dirty_and_events() {
    let (dir, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    assert_eq!(
        state.care_action(&mut game, &id, &purchase(0)).unwrap()["accepted"],
        true
    );
    let player = game.players.get_mut(&id).unwrap();
    player.energy = 9.;
    player.hunger = 12.;
    player.happiness = 17.;
    game.dirty = true;
    state.persist(&mut game).unwrap();
    game.events.push(Event {
        target: Some(id.clone()),
        message: json!({"kind":"existing-event"}),
    });
    let before_profile = game.players[&id].profile();
    let before_care = game.players[&id].care.clone();
    let before_cooldowns = game.players[&id].cooldowns.clone();
    let before_events = event_state(&game);
    let before_dirty = game.dirty;
    let before_disk = fs::read(dir.path().join("profiles.json")).unwrap();
    fs::create_dir(dir.path().join("profiles.json.tmp")).unwrap();
    let eat = consume(1);
    assert!(state.care_action(&mut game, &id, &eat).is_err());
    assert_eq!(game.players[&id].profile(), before_profile);
    assert_eq!(game.players[&id].care, before_care);
    assert_eq!(game.players[&id].cooldowns, before_cooldowns);
    assert_eq!(event_state(&game), before_events);
    assert_eq!(game.dirty, before_dirty);
    assert_eq!(
        fs::read(dir.path().join("profiles.json")).unwrap(),
        before_disk
    );
    fs::remove_dir(dir.path().join("profiles.json.tmp")).unwrap();
    let saved = state.care_action(&mut game, &id, &eat).unwrap();
    assert_eq!(saved["accepted"], true);
    assert_eq!(saved["replayed"], false);
    assert_eq!(game.players[&id].care.receipts.len(), 2);
    assert!(game.players[&id].care.inventory.is_empty());
    assert_eq!(game.players[&id].stats["stamina"].points, 25);
    assert_eq!(
        state.care_action(&mut game, &id, &eat).unwrap()["replayed"],
        true
    );
    assert_eq!(game.players[&id].stats["stamina"].points, 25);
}

#[test]
fn simultaneous_devices_with_one_revision_commit_only_one_purchase() {
    let (_dir, state, id) = fixture();
    {
        let mut game = state.game.lock().unwrap();
        game.connect(&id);
        assert_eq!(game.players[&id].connections, 2);
    }
    let barrier = Arc::new(Barrier::new(3));
    let requests = [purchase(0), purchase(0)];
    let responses = std::thread::scope(|scope| {
        let jobs: Vec<_> = requests
            .iter()
            .map(|message| {
                let barrier = barrier.clone();
                let state = &state;
                let id = &id;
                scope.spawn(move || {
                    barrier.wait();
                    let mut game = state.game.lock().unwrap();
                    state.care_action(&mut game, id, message).unwrap()
                })
            })
            .collect();
        barrier.wait();
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        responses
            .iter()
            .filter(|value| value["accepted"] == true)
            .count(),
        1
    );
    assert_eq!(
        responses
            .iter()
            .filter(|value| value["code"] == "stale-revision")
            .count(),
        1
    );
    let game = state.game.lock().unwrap();
    assert_eq!(game.players[&id].rings, 17);
    assert_eq!(game.players[&id].care.revision, 1);
    assert_eq!(game.players[&id].care.receipts.len(), 1);
    assert_eq!(game.players[&id].care.inventory["garden-fruit"], 1);
}

#[test]
fn reused_request_identity_cannot_change_intent_or_original_revision() {
    let (dir, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    let first = purchase(0);
    assert_eq!(
        state.care_action(&mut game, &id, &first).unwrap()["accepted"],
        true
    );
    let persisted = fs::read(dir.path().join("profiles.json")).unwrap();
    for field in ["intent", "expectedRevision"] {
        let mut changed = first.clone();
        if field == "intent" {
            changed["intent"] = json!({"kind":"consume","itemId":"garden-fruit"});
        } else {
            changed["expectedRevision"] = json!(1);
        }
        let response = state.care_action(&mut game, &id, &changed).unwrap();
        assert_eq!(response["accepted"], false);
        assert_eq!(response["code"], "request-id-conflict");
    }
    assert_eq!(game.players[&id].care.inventory["garden-fruit"], 1);
    assert_eq!(game.players[&id].care.revision, 1);
    assert_eq!(
        fs::read(dir.path().join("profiles.json")).unwrap(),
        persisted
    );
}

#[test]
fn forged_fields_remote_shop_missing_items_and_insufficient_rings_never_mutate() {
    let (dir, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    let missing = state.care_action(&mut game, &id, &consume(0)).unwrap();
    assert_eq!(missing["code"], "item-not-owned");
    for extra in ["targetId", "stats", "time", "quantity"] {
        let mut forged = purchase(0);
        forged[extra] = json!(1000);
        assert_eq!(
            state.care_action(&mut game, &id, &forged).unwrap()["code"],
            "invalid-request"
        );
    }
    let mut forged = purchase(0);
    forged["intent"]["price"] = json!(0);
    assert_eq!(
        state.care_action(&mut game, &id, &forged).unwrap()["code"],
        "invalid-request"
    );
    game.players.get_mut(&id).unwrap().x = 20.;
    assert_eq!(
        state.care_action(&mut game, &id, &purchase(0)).unwrap()["code"],
        "out-of-range"
    );
    at_shop(&mut game, &id);
    game.players.get_mut(&id).unwrap().rings = 2;
    game.dirty = true;
    state.persist(&mut game).unwrap();
    let before = game.players[&id].profile();
    let persisted = fs::read(dir.path().join("profiles.json")).unwrap();
    assert_eq!(
        state.care_action(&mut game, &id, &purchase(0)).unwrap()["code"],
        "insufficient-rings"
    );
    assert_eq!(game.players[&id].profile(), before);
    assert!(game.players[&id].care.inventory.is_empty());
    assert!(game.players[&id].care.receipts.is_empty());
    assert_eq!(
        fs::read(dir.path().join("profiles.json")).unwrap(),
        persisted
    );
}

#[test]
fn active_activities_reject_new_care_but_allow_committed_replay() {
    for activity in [
        "watch",
        "tiny",
        "race",
        "mission",
        "karate",
        "vehicle",
        "future-training",
        "disconnected",
    ] {
        let (_dir, state, id) = fixture();
        let mut game = state.game.lock().unwrap();
        let committed = purchase(0);
        assert_eq!(
            state.care_action(&mut game, &id, &committed).unwrap()["accepted"],
            true
        );
        match activity {
            "watch" => {
                game.action(&id, &json!({"action":"watch-start"}));
                assert!(game.players[&id].watch.is_some());
            }
            "tiny" => {
                game.action(&id, &json!({"action":"tiny-start","gameId":"search"}));
                assert!(game.players[&id].tiny_game.is_some());
            }
            "race" => {
                game.players.get_mut(&id).unwrap().race = Some(Race {
                    started_at: game.time,
                    next_checkpoint: 0,
                    course_id: None,
                })
            }
            "mission" => game.players.get_mut(&id).unwrap().zone = "mission".into(),
            "karate" => {
                game.activities = ActivityCatalog::from_values(json!({}), json!({"tiers":[{"id":"synthetic-tier","name":"Synthetic test tier","opponents":[{"id":"synthetic","name":"Synthetic","skills":[10,10,10,10,10,10,10]}]}]})).unwrap();
                game.action(
                    &id,
                    &json!({"action":"karate-enter","tier":"synthetic-tier"}),
                );
                assert!(game.players[&id].karate.is_some());
            }
            "vehicle" => {
                game.players.get_mut(&id).unwrap().vehicle = Some("dark-chao-walker".into())
            }
            "future-training" => {
                game.players.get_mut(&id).unwrap().care.activity_lock =
                    Some("training:test-job".into())
            }
            "disconnected" => game.players.get_mut(&id).unwrap().connections = 0,
            _ => unreachable!(),
        }
        let before = game.players[&id].care.clone();
        for intent in [
            json!({"kind":"pet"}),
            json!({"kind":"consume","itemId":"garden-fruit"}),
            json!({"kind":"purchase","itemId":"garden-fruit"}),
        ] {
            let response = state
                .care_action(&mut game, &id, &request(1, intent))
                .unwrap();
            assert_eq!(response["code"], "activity-conflict", "{activity}");
            assert_eq!(response["accepted"], false, "{activity}");
            assert_eq!(game.players[&id].care, before, "{activity}");
        }
        let replay = state.care_action(&mut game, &id, &committed).unwrap();
        assert_eq!(replay["replayed"], true, "{activity}");
        assert_eq!(game.players[&id].care, before, "{activity}");
    }
}

#[test]
fn legacy_profiles_preserve_earned_state_and_owner_privilege() {
    let dir = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4().to_string();
    let owner_id = Uuid::new_v4().to_string();
    let stats: serde_json::Map<String, Value> = ALL_STATS
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            (
                name.to_owned(),
                json!({"grade":3,"level":20+i,"points":123+i}),
            )
        })
        .collect();
    let legacy = json!({"id":id,"tokenHash":hash_token("legacy-token"),"name":"Legacy Chao",
        "stats":stats,"rings":54321,"alignment":-75,"form":"chaos","happiness":42,
        "ownedCosmetics":["eggshell"],"equipped":"eggshell","missionsCompleted":7,"racesCompleted":4,"bestRaceMs":1234});
    let owner = json!({"id":owner_id,"tokenHash":hash_token("legacy-owner"),"name":"Owner","admin":true,
        "form":"chaos","vehicle":"dark-chao-walker","gear":{"footwear":"shadow-air-shoes","hands":"knuckles-hammer-gloves"},"rings":76543});
    let source = serde_json::to_vec_pretty(&json!([legacy, owner])).unwrap();
    fs::write(dir.path().join("profiles.json"), &source).unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let mut game = state.game.lock().unwrap();
    let player = &game.players[&id];
    assert!(player.care.inventory.is_empty());
    assert!(player.care.receipts.is_empty());
    assert_eq!(serde_json::to_value(&player.stats).unwrap(), json!(stats));
    assert_eq!(
        (player.rings, player.alignment, player.form.as_str()),
        (54321, -75., "chaos")
    );
    assert_eq!((player.energy, player.hunger), (100., 100.));
    assert_eq!(player.equipped.as_deref(), Some("eggshell"));
    assert_eq!(player.missions_completed, 7);
    assert_eq!(player.races_completed, 4);
    assert_eq!(player.best_race_ms, Some(1234.));
    let owner = &game.players[&owner_id];
    assert!(owner.admin);
    assert_eq!(owner.form, "chaos");
    assert_eq!(owner.vehicle.as_deref(), Some("dark-chao-walker"));
    assert_eq!(owner.gear.footwear.as_deref(), Some("shadow-air-shoes"));
    assert_eq!(owner.gear.hands.as_deref(), Some("knuckles-hammer-gloves"));
    assert!(
        owner
            .stats
            .values()
            .all(|stat| stat.level == 99 && stat.points == 9999)
    );
    assert_eq!(fs::read(dir.path().join("profiles.json")).unwrap(), source);
    game.connect(&id);
    at_shop(&mut game, &id);
    assert_eq!(
        state.care_action(&mut game, &id, &purchase(0)).unwrap()["accepted"],
        true
    );
    let migrated: Value =
        serde_json::from_slice(&fs::read(dir.path().join("profiles.json")).unwrap()).unwrap();
    let record = migrated
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["id"] == id)
        .unwrap();
    assert_eq!(record["stats"], json!(stats));
    assert_eq!(record["rings"], 54318);
    assert_eq!(record["form"], "chaos");
    assert_eq!(record["care"]["inventory"]["garden-fruit"], 1);
}

#[test]
fn malformed_or_newer_care_data_fails_load_without_rewriting_save() {
    let mut unsupported = serde_json::to_value(CareState::default()).unwrap();
    unsupported["version"] = json!(999);
    let mut negative_inventory = serde_json::to_value(CareState::default()).unwrap();
    negative_inventory["inventory"] = json!({"garden-fruit":-1});
    let mut empty_nested_evidence = serde_json::to_value(CareState::default()).unwrap();
    empty_nested_evidence["evidence"] = json!({});
    for care in [
        Value::Null,
        json!({}),
        unsupported,
        negative_inventory,
        empty_nested_evidence,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = serde_json::to_vec_pretty(&json!([{"id":Uuid::new_v4().to_string(),"tokenHash":hash_token("unchanged"),"rings":4321,"form":"chaos","care":care}])).unwrap();
        fs::write(dir.path().join("profiles.json"), &source).unwrap();
        assert!(AppState::load(Config::local(dir.path().to_owned(), false)).is_err());
        assert_eq!(fs::read(dir.path().join("profiles.json")).unwrap(), source);
    }
}

#[test]
fn duplicate_receipt_revisions_and_mismatched_original_revision_fail_load() {
    let (dir, state, id) = fixture();
    {
        let mut game = state.game.lock().unwrap();
        assert_eq!(
            state.care_action(&mut game, &id, &purchase(0)).unwrap()["accepted"],
            true
        );
    }
    let original: Value =
        serde_json::from_slice(&fs::read(dir.path().join("profiles.json")).unwrap()).unwrap();
    for corruption in ["duplicate-revision", "mismatched-original-revision"] {
        let mut profiles = original.clone();
        let record = profiles
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|record| record["id"] == id)
            .unwrap();
        let receipts = record["care"]["receipts"].as_object_mut().unwrap();
        let first_id = receipts.keys().next().unwrap().clone();
        if corruption == "duplicate-revision" {
            let duplicate_id = Uuid::new_v4().to_string();
            let mut duplicate = receipts[&first_id].clone();
            duplicate["request"]["requestId"] = json!(duplicate_id);
            receipts.insert(duplicate_id, duplicate);
        } else {
            receipts.get_mut(&first_id).unwrap()["request"]["expectedRevision"] = json!(17);
        }
        let source = serde_json::to_vec_pretty(&profiles).unwrap();
        fs::write(dir.path().join("profiles.json"), &source).unwrap();
        assert!(
            AppState::load(Config::local(dir.path().to_owned(), false)).is_err(),
            "{corruption}"
        );
        assert_eq!(fs::read(dir.path().join("profiles.json")).unwrap(), source);
    }
}

#[test]
fn shop_purchase_requires_vertical_proximity_and_ground_at_target() {
    let (dir, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    let before_disk = fs::read(dir.path().join("profiles.json")).unwrap();
    let before_care = game.players[&id].care.clone();
    game.players.get_mut(&id).unwrap().y = 100.;
    let airborne = state.care_action(&mut game, &id, &purchase(0)).unwrap();
    assert_eq!(airborne["code"], "out-of-range");
    assert_eq!(airborne["accepted"], false);
    game.players.get_mut(&id).unwrap().y = 0.;
    let mut heights = vec![Some(0.); 9];
    heights[4] = None;
    game.terrains.insert(
        "hub".into(),
        Terrain {
            min_x: -5.,
            min_z: 7.,
            step: 1.,
            width: 3,
            depth: 3,
            heights,
        },
    );
    assert!(game.sample_ground("hub", -4., 8.).is_none());
    let unsupported = state.care_action(&mut game, &id, &purchase(0)).unwrap();
    assert_eq!(unsupported["code"], "out-of-range");
    assert_eq!(unsupported["accepted"], false);
    assert_eq!(game.players[&id].care, before_care);
    assert_eq!(game.players[&id].rings, 20);
    assert_eq!(
        fs::read(dir.path().join("profiles.json")).unwrap(),
        before_disk
    );
}

#[test]
fn activity_lock_blocks_new_movement_and_clears_preexisting_input_on_tick() {
    let (_dir, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    game.set_input(&id, &json!({"dx":1,"dz":0,"jump":true,"boost":true}));
    assert_eq!(game.players[&id].input.dx, 1.);
    game.players.get_mut(&id).unwrap().care.activity_lock = Some("training:synthetic-job".into());
    let (x, z) = (game.players[&id].x, game.players[&id].z);
    game.set_input(&id, &json!({"dx":-1,"dz":1,"jump":true,"boost":true}));
    assert_eq!(game.players[&id].input.dx, 1.);
    game.tick(0.05);
    let player = &game.players[&id];
    assert_eq!((player.x, player.z), (x, z));
    assert_eq!((player.input.dx, player.input.dz), (0., 0.));
    assert!(!player.input.jump);
    assert!(!player.input.boost);
    assert_eq!(
        player.care.activity_lock.as_deref(),
        Some("training:synthetic-job")
    );
}

#[test]
fn disabled_native_catalog_keeps_owned_items_and_marks_native_shop_offers_disabled() {
    let (dir, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    assert!(!game.care_catalog.enabled());
    // Synthetic ownership checks preservation; no native stat rows are supplied.
    game.players
        .get_mut(&id)
        .unwrap()
        .care
        .inventory
        .insert("sa2:0".into(), 1);
    game.dirty = true;
    state.persist(&mut game).unwrap();
    let before = game.players[&id].profile();
    let before_disk = fs::read(dir.path().join("profiles.json")).unwrap();
    let response = state
        .care_action(
            &mut game,
            &id,
            &request(0, json!({"kind":"consume","itemId":"sa2:0"})),
        )
        .unwrap();
    assert_eq!(response["accepted"], false);
    assert_eq!(response["code"], "native-data-unavailable");
    assert_eq!(game.players[&id].profile(), before);
    assert_eq!(
        fs::read(dir.path().join("profiles.json")).unwrap(),
        before_disk
    );
    let snapshot = game.snapshot(&id);
    for offer in snapshot["care"]["shop"].as_array().unwrap() {
        assert_eq!(offer["enabled"], offer["itemId"] == "garden-fruit");
    }
}
