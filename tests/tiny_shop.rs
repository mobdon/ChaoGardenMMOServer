//! Tiny shop host tests use disposable profiles and adapted garden fruit only.
use axum::{body::Body, http::Request};
use chao_game_server::{
    activities::ActivityCatalog,
    game::{Game, Race, hash_token},
    service::{AppState, Config, router},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{
    fs,
    sync::{Arc, Barrier},
};
use tower::ServiceExt;
use uuid::Uuid;

fn fixture() -> (tempfile::TempDir, AppState, String) {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let id = {
        let mut game = state.game.lock().unwrap();
        let session = game
            .create_session(&json!({"name":"Tiny shop fixture"}))
            .unwrap();
        let id = session["player"]["id"].as_str().unwrap().to_owned();
        game.connect(&id);
        away_from_desktop_shops(&mut game, &id, "garden");
        state.persist(&mut game).unwrap();
        id
    };
    (directory, state, id)
}

fn away_from_desktop_shops(game: &mut Game, id: &str, zone: &str) {
    let p = game.players.get_mut(id).unwrap();
    p.zone = zone.into();
    p.x = 15.;
    p.z = 15.;
    p.y = 0.;
    p.ground_y = 0.;
    p.jump_offset = 0.;
}

fn request(revision: u64, intent: Value) -> Value {
    json!({"type":"care","requestId":Uuid::new_v4().to_string(),"expectedRevision":revision,"intent":intent})
}
fn buy(revision: u64) -> Value {
    request(
        revision,
        json!({"kind":"tiny_purchase","itemId":"garden-fruit"}),
    )
}
fn consume(revision: u64) -> Value {
    request(revision, json!({"kind":"consume","itemId":"garden-fruit"}))
}
fn events(game: &Game) -> Value {
    json!(
        game.events
            .iter()
            .map(|event| json!({"target":event.target,"message":event.message}))
            .collect::<Vec<_>>()
    )
}
fn disk_profile(directory: &std::path::Path, id: &str) -> Value {
    let profiles: Value =
        serde_json::from_slice(&fs::read(directory.join("profiles.json")).unwrap()).unwrap();
    profiles
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap()
        .clone()
}
fn catalog() -> Value {
    json!({"rulesVersion":"adapted-tiny-shop-v1","enabled":true,"items":[{"itemId":"garden-fruit","name":"Garden Fruit","price":3,"enabled":true,"maxQuantity":99}],"nativeCatalog":{"enabled":false,"reason":"native-data-unavailable"}})
}

#[test]
fn tiny_shop_buys_one_at_the_same_price_in_every_safe_area_without_changing_desktop_proximity() {
    for zone in ["hub", "garden", "hero", "dark"] {
        let (_directory, state, id) = fixture();
        let mut game = state.game.lock().unwrap();
        away_from_desktop_shops(&mut game, &id, zone);
        let before = game.players[&id].profile();
        let desktop = request(0, json!({"kind":"purchase","itemId":"garden-fruit"}));
        let denied = state.care_action(&mut game, &id, &desktop).unwrap();
        assert_eq!(denied["code"], "out-of-range", "{zone}");
        assert_eq!(game.players[&id].profile(), before, "{zone}");
        let result = state.care_action(&mut game, &id, &buy(0)).unwrap();
        assert_eq!(result["accepted"], true, "{zone}");
        assert_eq!(result["effects"]["rings"], -3);
        assert_eq!(result["effects"]["inventory"]["garden-fruit"], 1);
        let p = &game.players[&id];
        assert_eq!(p.rings, 17);
        assert_eq!(p.care.inventory["garden-fruit"], 1);
        assert_eq!(p.care.revision, 1);
        assert_eq!(p.care.receipts.len(), 1);
        assert_eq!(serde_json::to_value(&p.stats).unwrap(), before["stats"]);
        let shop = game.snapshot(&id)["care"]["tinyShop"].clone();
        assert_eq!(shop["rulesVersion"], catalog()["rulesVersion"]);
        assert_eq!(shop["items"], catalog()["items"]);
        assert_eq!(shop["eligibility"]["canPurchase"], true);
        assert!(shop["eligibility"]["reason"].is_null());
    }
}

#[test]
fn buy_feed_and_uuid_replays_survive_two_reloads_without_losing_unknown_save_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4().to_string();
    let future = json!({"version":42,"opaque":["retained",{"enabled":true}],"nullable":null});
    let run_metadata = json!({"futureRule":"opaque","samples":[5,8,13]});
    let gear_metadata = json!({"format":7,"calibration":[1.5,-2.]});
    fs::write(directory.path().join("profiles.json"), serde_json::to_vec_pretty(&json!([{
        "id":id,"tokenHash":hash_token("tiny-metadata-fixture"),"name":"Future Tiny fields",
        "rings":100,"form":"chaos","alignment":-80,"happiness":50,"energy":40,"hunger":30,
        "futureNamespace":future,"stats":{"run":{"grade":3,"level":8,"points":99,"futureMetadata":run_metadata}},
        "gear":{"futureMetadata":gear_metadata}
    }])).unwrap()).unwrap();
    let purchase = buy(0);
    let feed = consume(1);
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let expected_stats;
    {
        let mut game = state.game.lock().unwrap();
        game.connect(&id);
        away_from_desktop_shops(&mut game, &id, "dark");
        let before = game.players[&id].stats.clone();
        let result = state.care_action(&mut game, &id, &purchase).unwrap();
        assert_eq!(result["accepted"], true);
        assert_eq!(game.players[&id].rings, 97);
        assert_eq!(
            game.players[&id].stats, before,
            "buying alone cannot grant effects"
        );
        expected_stats = before;
    }
    let bought = disk_profile(directory.path(), &id);
    assert_eq!(bought["futureNamespace"], future);
    assert_eq!(bought["stats"]["run"]["futureMetadata"], run_metadata);
    assert_eq!(bought["gear"]["futureMetadata"], gear_metadata);
    let restored = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    {
        let mut game = restored.game.lock().unwrap();
        game.connect(&id);
        let replay = restored.care_action(&mut game, &id, &purchase).unwrap();
        assert_eq!(replay["replayed"], true);
        assert_eq!(game.players[&id].rings, 97);
        let result = restored.care_action(&mut game, &id, &feed).unwrap();
        assert_eq!(result["accepted"], true);
        assert_eq!(result["effects"]["inventory"]["garden-fruit"], -1);
        assert_eq!(result["effects"]["points"]["stamina"], 25);
        assert_eq!(result["effects"]["levelsChanged"], false);
        assert_eq!(result["effects"]["gradesChanged"], false);
        let p = &game.players[&id];
        assert!(p.care.inventory.is_empty());
        assert_eq!(p.care.revision, 2);
        assert_eq!(p.care.receipts.len(), 2);
        assert_eq!(p.rings, 97);
        for (name, stat) in &expected_stats {
            assert_eq!(p.stats[name].level, stat.level);
            assert_eq!(p.stats[name].grade, stat.grade);
            assert_eq!(
                p.stats[name].points,
                stat.points + if name == "stamina" { 25 } else { 0 }
            );
        }
    }
    let committed = fs::read(directory.path().join("profiles.json")).unwrap();
    let final_state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let mut game = final_state.game.lock().unwrap();
    assert_eq!(game.players[&id].connections, 0);
    for message in [&purchase, &feed] {
        let replay = final_state.care_action(&mut game, &id, message).unwrap();
        assert_eq!(replay["accepted"], true);
        assert_eq!(replay["replayed"], true);
    }
    let p = &game.players[&id];
    assert_eq!(p.rings, 97);
    assert!(p.care.inventory.is_empty());
    assert_eq!(p.care.receipts.len(), 2);
    assert_eq!(p.form, "chaos");
    assert_eq!(p.alignment, -80.);
    let profile = p.profile();
    assert_eq!(profile["futureNamespace"], future);
    assert_eq!(profile["stats"]["run"]["futureMetadata"], run_metadata);
    assert_eq!(profile["gear"]["futureMetadata"], gear_metadata);
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        committed
    );
}

#[test]
fn failed_save_rolls_back_wallet_inventory_revision_receipt_events_and_dirty_then_same_uuid_commits()
 {
    let (directory, state, id) = fixture();
    let purchase = buy(0);
    let mut game = state.game.lock().unwrap();
    game.dirty = true;
    let before = game.players[&id].profile();
    let before_events = events(&game);
    let before_disk = fs::read(directory.path().join("profiles.json")).unwrap();
    let blocked = directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocked).unwrap();
    assert!(state.care_action(&mut game, &id, &purchase).is_err());
    assert_eq!(game.players[&id].profile(), before);
    assert!(game.dirty);
    assert_eq!(events(&game), before_events);
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        before_disk
    );
    fs::remove_dir(&blocked).unwrap();
    let committed = state.care_action(&mut game, &id, &purchase).unwrap();
    assert_eq!(committed["accepted"], true);
    assert_eq!(committed["replayed"], false);
    let replay = state.care_action(&mut game, &id, &purchase).unwrap();
    assert_eq!(replay["replayed"], true);
    assert_eq!(game.players[&id].rings, 17);
    assert_eq!(game.players[&id].care.inventory["garden-fruit"], 1);
    assert_eq!(game.players[&id].care.receipts.len(), 1);
    assert_eq!(disk_profile(directory.path(), &id)["rings"], 17);
}

#[test]
fn quantity_and_wallet_limits_are_authoritative_and_fail_without_mutation() {
    let (_directory, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    let p = game.players.get_mut(&id).unwrap();
    p.rings = 100;
    p.care.inventory.insert("garden-fruit".into(), 98);
    assert_eq!(
        state.care_action(&mut game, &id, &buy(0)).unwrap()["accepted"],
        true
    );
    assert_eq!(game.players[&id].care.inventory["garden-fruit"], 99);
    assert_eq!(game.players[&id].rings, 97);
    let before = game.players[&id].profile();
    assert_eq!(
        state.care_action(&mut game, &id, &buy(1)).unwrap()["code"],
        "inventory-full"
    );
    assert_eq!(game.players[&id].profile(), before);
    assert_eq!(
        game.snapshot(&id)["care"]["tinyShop"]["eligibility"]["reason"],
        "inventory-full"
    );
    let p = game.players.get_mut(&id).unwrap();
    p.care.inventory.clear();
    p.rings = 2;
    let before = p.profile();
    assert_eq!(
        state.care_action(&mut game, &id, &buy(1)).unwrap()["code"],
        "insufficient-rings"
    );
    assert_eq!(game.players[&id].profile(), before);
    assert_eq!(
        game.snapshot(&id)["care"]["tinyShop"]["eligibility"]["reason"],
        "insufficient-rings"
    );
    let p = game.players.get_mut(&id).unwrap();
    p.rings = 100;
    p.care.revision = u64::MAX;
    let before = p.profile();
    assert_eq!(
        state.care_action(&mut game, &id, &buy(u64::MAX)).unwrap()["code"],
        "receipt-capacity"
    );
    assert_eq!(game.players[&id].profile(), before);
    assert_eq!(
        game.snapshot(&id)["care"]["tinyShop"]["eligibility"]["reason"],
        "receipt-capacity"
    );
}

#[test]
fn tiny_purchase_rejects_claimed_fields_wrong_intent_spelling_and_other_items() {
    let (_directory, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    let before = game.players[&id].profile();
    for (field, value) in [
        ("price", json!(3)),
        ("quantity", json!(1)),
        ("targetId", json!(id)),
        ("serverTime", json!(0)),
    ] {
        let mut envelope = buy(0);
        envelope[field] = value.clone();
        let mut intent = buy(0);
        intent["intent"][field] = value;
        for forged in [envelope, intent] {
            assert_eq!(
                state.care_action(&mut game, &id, &forged).unwrap()["code"],
                "invalid-request"
            );
            assert_eq!(game.players[&id].profile(), before);
        }
    }
    for kind in ["tinypurchase", "tiny-purchase", "TinyPurchase"] {
        let wrong = request(0, json!({"kind":kind,"itemId":"garden-fruit"}));
        assert_eq!(
            state.care_action(&mut game, &id, &wrong).unwrap()["code"],
            "invalid-request"
        );
    }
    for item in [
        "sa2:0",
        "sa2:22",
        "sa2:23",
        "sa2:25",
        "garden-fruit ",
        "future-fruit",
        "",
    ] {
        let unavailable = request(0, json!({"kind":"tiny_purchase","itemId":item}));
        assert_eq!(
            state.care_action(&mut game, &id, &unavailable).unwrap()["accepted"],
            false,
            "{item}"
        );
        assert_eq!(game.players[&id].profile(), before, "{item}");
    }
}

#[test]
fn tiny_purchase_rejects_every_activity_lock_but_keeps_committed_replays_available() {
    for activity in [
        "watch",
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
        let committed = buy(0);
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
                });
            }
            "karate" => {
                game.activities = ActivityCatalog::from_values(json!({}),json!({"tiers":[{"id":"synthetic-tier","name":"Synthetic","opponents":[{"id":"synthetic","name":"Synthetic","skills":[10,10,10,10,10,10,10]}]}]})).unwrap();
                game.action(
                    &id,
                    &json!({"action":"karate-enter","tier":"synthetic-tier"}),
                );
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
            "disconnected" => game.players.get_mut(&id).unwrap().connections = 0,
            _ => unreachable!(),
        }
        let before = game.players[&id].profile();
        assert_eq!(
            state.care_action(&mut game, &id, &buy(1)).unwrap()["code"],
            "activity-conflict",
            "{activity}"
        );
        let shop = game.snapshot(&id)["care"]["tinyShop"].clone();
        assert_eq!(shop["eligibility"]["canPurchase"], false, "{activity}");
        assert_eq!(
            shop["eligibility"]["reason"], "activity-conflict",
            "{activity}"
        );
        assert_eq!(
            state.care_action(&mut game, &id, &committed).unwrap()["replayed"],
            true,
            "{activity}"
        );
        assert_eq!(game.players[&id].profile(), before, "{activity}");
    }
}

#[test]
fn concurrent_devices_charge_once_for_same_uuid_and_serialize_different_revision_claims() {
    for duplicate in [true, false] {
        let (_directory, state, id) = fixture();
        let state = Arc::new(state);
        let barrier = Arc::new(Barrier::new(3));
        let first = buy(0);
        let second = if duplicate { first.clone() } else { buy(0) };
        let workers = [first, second]
            .into_iter()
            .map(|message| {
                let state = state.clone();
                let barrier = barrier.clone();
                let id = id.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let mut game = state.game.lock().unwrap();
                    state.care_action(&mut game, &id, &message).unwrap()
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            results.iter().filter(|r| r["accepted"] == true).count(),
            if duplicate { 2 } else { 1 }
        );
        if duplicate {
            assert_eq!(results.iter().filter(|r| r["replayed"] == true).count(), 1);
        } else {
            assert_eq!(
                results
                    .iter()
                    .filter(|r| r["code"] == "stale-revision")
                    .count(),
                1
            );
        }
        let game = state.game.lock().unwrap();
        let p = &game.players[&id];
        assert_eq!(p.rings, 17);
        assert_eq!(p.care.inventory["garden-fruit"], 1);
        assert_eq!(p.care.revision, 1);
        assert_eq!(p.care.receipts.len(), 1);
    }
}

#[test]
fn a_committed_uuid_cannot_be_repurposed_as_a_desktop_purchase_or_another_item() {
    let (_directory, state, id) = fixture();
    let mut game = state.game.lock().unwrap();
    let committed = buy(0);
    assert_eq!(
        state.care_action(&mut game, &id, &committed).unwrap()["accepted"],
        true
    );
    let before = game.players[&id].profile();
    for intent in [
        json!({"kind":"purchase","itemId":"garden-fruit"}),
        json!({"kind":"tiny_purchase","itemId":"sa2:22"}),
    ] {
        let mut changed = committed.clone();
        changed["intent"] = intent;
        assert_eq!(
            state.care_action(&mut game, &id, &changed).unwrap()["code"],
            "request-id-conflict"
        );
        assert_eq!(game.players[&id].profile(), before);
    }
}

#[tokio::test]
async fn status_advertises_the_actual_server_catalog_and_care_mutation_capability() {
    let (_directory, state, _id) = fixture();
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/api/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let status: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status["capabilities"]["careMutations"], true);
    assert_eq!(status["tinyGarden"]["shop"], catalog());
}
