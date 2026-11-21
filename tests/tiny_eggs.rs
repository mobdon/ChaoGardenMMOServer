//! Real egg/ownership transactions. Positive native cases require the user's
//! explicitly configured local payload; no copied native template is embedded.
use chao_game_server::{
    collection::{self, MAX_CHAO, ROTATION_MS},
    game::{Event, Game},
    service::{AppState, Config},
};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Barrier},
};
use uuid::Uuid;

const NOW: u64 = 1_800_000_000_000;
fn request(revision: u64, intent: Value) -> Value {
    json!({"type":"care","requestId":Uuid::new_v4().to_string(),"expectedRevision":revision,"intent":intent})
}
fn fixture(native: bool, owner: bool) -> (tempfile::TempDir, Config, AppState, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::local(dir.path().to_owned(), owner);
    if native {
        config.tiny_garden_payload = Some(PathBuf::from(
            env::var_os("CHAO_TINY_GARDEN_PAYLOAD")
                .expect("Set explicit owned payload for native egg acceptance"),
        ));
    }
    let state = AppState::load(config.clone()).unwrap();
    let id = {
        let mut game = state.game.lock().unwrap();
        let session = game
            .create_session(&json!({"name":"Egg ownership fixture"}))
            .unwrap();
        let id = session["player"]["id"].as_str().unwrap().to_owned();
        game.connect(&id);
        game.players.get_mut(&id).unwrap().rings = 999_999;
        assert_eq!(
            state
                .care_action_at(
                    &mut game,
                    &id,
                    &request(0, json!({"kind":"tiny_enter"})),
                    NOW
                )
                .unwrap()["accepted"],
            true
        );
        id
    };
    (dir, config, state, id)
}
fn offer(game: &Game, id: &str, now: u64) -> Value {
    collection::shop_public(game, &game.players[id], now)
}
fn buy_request(game: &Game, id: &str, now: u64) -> Value {
    let shop = offer(game, id, now);
    request(
        game.players[id].care.revision,
        json!({"kind":"tiny_egg_purchase","eggId":shop["offers"][0]["eggId"],"stockEpoch":shop["epoch"]}),
    )
}
fn hatch_request(game: &Game, id: &str) -> Value {
    let egg = game.players[id]
        .tiny_collection
        .as_ref()
        .unwrap()
        .pending_egg
        .as_ref()
        .unwrap();
    request(
        game.players[id].care.revision,
        json!({"kind":"tiny_egg_hatch","ownedEggId":egg.id}),
    )
}
fn disk(dir: &Path, id: &str) -> Value {
    let profiles: Value =
        serde_json::from_slice(&fs::read(dir.join("profiles.json")).unwrap()).unwrap();
    profiles
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap()
        .clone()
}
fn events(game: &Game) -> Value {
    json!(
        game.events
            .iter()
            .map(|e| json!({"target":e.target,"message":e.message}))
            .collect::<Vec<_>>()
    )
}
fn reject(state: &AppState, game: &mut Game, id: &str, message: &Value, now: u64, code: &str) {
    let profile = game.players[id].profile();
    let dirty = game.dirty;
    let oldevents = events(game);
    let result = state.care_action_at(game, id, message, now).unwrap();
    assert_eq!(result["accepted"], false, "{message}");
    assert_eq!(result["code"], code, "{message}");
    assert_eq!(game.players[id].profile(), profile);
    assert_eq!(game.dirty, dirty);
    assert_eq!(events(game), oldevents);
}

#[test]
fn unavailable_native_source_exposes_no_fake_stock_and_cannot_charge_or_hatch() {
    let (dir, _, state, id) = fixture(false, false);
    let mut game = state.game.lock().unwrap();
    let shop = offer(&game, &id, NOW);
    assert_eq!(shop["enabled"], false);
    assert_eq!(shop["offers"], json!([]));
    assert_eq!(shop["eligibility"]["reason"], "native-data-unavailable");
    let collection = collection::public(&game, &game.players[&id], NOW);
    assert_eq!(collection["activeChaoId"], id);
    assert_eq!(collection["chao"].as_array().unwrap().len(), 1);
    assert_eq!(collection["selection"]["enabled"], false);
    let bytes = fs::read(dir.path().join("profiles.json")).unwrap();
    for intent in [
        json!({"kind":"tiny_egg_purchase","eggId":0,"stockEpoch":NOW/ROTATION_MS}),
        json!({"kind":"tiny_egg_hatch","ownedEggId":Uuid::new_v4().to_string()}),
    ] {
        reject(
            &state,
            &mut game,
            &id,
            &request(1, intent),
            NOW,
            "native-data-unavailable",
        );
    }
    assert!(game.players[&id].tiny_collection.is_none());
    assert_eq!(fs::read(dir.path().join("profiles.json")).unwrap(), bytes);
}

#[test]
fn strict_egg_intents_reject_fake_price_rng_record_owner_gain_and_selection() {
    let (_dir, _config, state, id) = fixture(false, false);
    let mut game = state.game.lock().unwrap();
    for intent in [
        json!({"kind":"tiny_egg_purchase","eggId":12,"stockEpoch":0}),
        json!({"kind":"tiny_egg_purchase","eggId":0,"stockEpoch":0.5}),
        json!({"kind":"tiny_egg_hatch","ownedEggId":"not-uuid"}),
        json!({"kind":"tiny_chao_select","chaoId":id}),
    ] {
        reject(
            &state,
            &mut game,
            &id,
            &request(1, intent),
            NOW,
            "invalid-request",
        );
    }
    for (field, value) in [
        ("price", json!(0)),
        ("rings", json!(99999)),
        ("seed", json!(1)),
        ("nativeRecord", json!([0, 0])),
        ("ownerId", json!(id)),
        ("stats", json!({"run":9999})),
        ("serverNowMs", json!(NOW)),
    ] {
        let mut intent = json!({"kind":"tiny_egg_purchase","eggId":0,"stockEpoch":NOW/ROTATION_MS});
        intent[field] = value;
        reject(
            &state,
            &mut game,
            &id,
            &request(1, intent),
            NOW,
            "invalid-request",
        );
    }
}

#[test]
fn malformed_collection_fails_load_without_touching_legacy_profile() {
    let (dir, config, _, id) = fixture(false, false);
    let mut p = disk(dir.path(), &id);
    p["tinyCollection"] = json!({"version":999,"ownerId":id});
    let bytes = serde_json::to_vec(&json!([p])).unwrap();
    fs::write(dir.path().join("profiles.json"), &bytes).unwrap();
    assert!(AppState::load(config).is_err());
    assert_eq!(fs::read(dir.path().join("profiles.json")).unwrap(), bytes);
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn native_purchase_hatch_adds_owned_chao_and_preserves_active_owner_credentials_stats_gear_and_replays()
 {
    let (dir, config, state, id) = fixture(true, true);
    let (buy, hatch, expected, active) = {
        let mut game = state.game.lock().unwrap();
        let active = game.players[&id].profile();
        let shop = offer(&game, &id, NOW);
        assert_eq!(shop["enabled"], true);
        let price = shop["offers"][0]["price"].as_u64().unwrap();
        let buy = buy_request(&game, &id, NOW);
        let result = state.care_action_at(&mut game, &id, &buy, NOW).unwrap();
        assert_eq!(result["accepted"], true);
        assert_eq!(result["effects"]["rings"], -(price as i64));
        assert_eq!(game.players[&id].rings, 999_999 - price as u32);
        let hatch = hatch_request(&game, &id);
        let result = state
            .care_action_at(&mut game, &id, &hatch, NOW + 1)
            .unwrap();
        assert_eq!(result["accepted"], true);
        assert_eq!(result["effects"]["chao"]["active"], false);
        let new_id = result["effects"]["chao"]["id"].as_str().unwrap();
        assert_ne!(new_id, id);
        assert_eq!(
            game.players.len(),
            1,
            "a newborn is not a new credential/player"
        );
        let public = collection::public(&game, &game.players[&id], NOW + 1);
        assert_eq!(public["chao"].as_array().unwrap().len(), 2);
        assert_eq!(public["activeChaoId"], id);
        assert!(public["pendingEgg"].is_null());
        assert_eq!(public["chao"][1]["stats"].as_object().unwrap().len(), 5);
        for stat in public["chao"][1]["stats"].as_object().unwrap().values() {
            assert!(stat["grade"].as_u64().unwrap() <= 5);
            assert_eq!(stat["level"], 1);
            assert_eq!(stat["points"], 0);
            assert_eq!(stat["experience"], 0);
        }
        assert_eq!(public["chao"][1]["nativeNeeds"]["mood"], 100);
        assert_eq!(public["selection"]["enabled"], false);
        let text = public.to_string();
        assert!(
            !text.contains("nativeRecord")
                && !text.contains("hatchRng")
                && !text.contains("stockSeed")
                && !text.contains("tokenHash")
        );
        let profile = game.players[&id].profile();
        for field in [
            "id",
            "name",
            "tokenHash",
            "admin",
            "stats",
            "gear",
            "form",
            "alignment",
            "ownedCosmetics",
        ] {
            assert_eq!(profile[field], active[field], "{field}");
        }
        (buy, hatch, profile, active)
    };
    for _ in 0..2 {
        let next = AppState::load(config.clone()).unwrap();
        let mut game = next.game.lock().unwrap();
        assert_eq!(
            game.players[&id].profile()["tokenHash"],
            active["tokenHash"]
        );
        for command in [&buy, &hatch] {
            assert_eq!(
                next.care_action_at(&mut game, &id, command, NOW + ROTATION_MS)
                    .unwrap()["replayed"],
                true
            );
        }
        assert_eq!(
            game.players[&id].profile()["tinyCollection"],
            expected["tinyCollection"]
        );
        assert_eq!(
            disk(dir.path(), &id)["tinyCollection"],
            expected["tinyCollection"]
        );
        assert_eq!(game.players[&id].care.history.len(), 3);
    }
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn one_held_egg_survives_hour_changes_disconnect_and_restart_until_explicit_online_hatch() {
    let (dir, config, state, id) = fixture(true, false);
    let egg = {
        let mut game = state.game.lock().unwrap();
        let buy = buy_request(&game, &id, NOW);
        assert_eq!(
            state.care_action_at(&mut game, &id, &buy, NOW).unwrap()["accepted"],
            true
        );
        let egg = game.players[&id]
            .tiny_collection
            .as_ref()
            .unwrap()
            .pending_egg
            .clone();
        let extra = buy_request(&game, &id, NOW + ROTATION_MS);
        reject(
            &state,
            &mut game,
            &id,
            &extra,
            NOW + ROTATION_MS,
            "egg-slot-full",
        );
        game.disconnect(&id);
        game.tick(0.05);
        assert_eq!(
            state
                .settle_ready_training_at(&mut game, NOW + ROTATION_MS * 3)
                .unwrap(),
            0
        );
        assert!(
            game.players[&id]
                .tiny_collection
                .as_ref()
                .unwrap()
                .chao
                .is_empty()
        );
        egg
    };
    let next = AppState::load(config).unwrap();
    let mut game = next.game.lock().unwrap();
    assert_eq!(
        game.players[&id]
            .tiny_collection
            .as_ref()
            .unwrap()
            .pending_egg,
        egg
    );
    let hatch = hatch_request(&game, &id);
    reject(
        &next,
        &mut game,
        &id,
        &hatch,
        NOW + ROTATION_MS * 3,
        "activity-conflict",
    );
    game.connect(&id);
    assert_eq!(
        next.care_action_at(&mut game, &id, &hatch, NOW + ROTATION_MS * 3)
            .unwrap()["accepted"],
        true
    );
    assert!(
        game.players[&id]
            .tiny_collection
            .as_ref()
            .unwrap()
            .pending_egg
            .is_none()
    );
    let buy = buy_request(&game, &id, NOW + ROTATION_MS * 3);
    assert_eq!(
        next.care_action_at(&mut game, &id, &buy, NOW + ROTATION_MS * 3)
            .unwrap()["accepted"],
        true
    );
    assert_eq!(
        disk(dir.path(), &id)["tinyCollection"]["chao"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn source_native_weighted_hourly_stock_is_stable_after_reconnect_with_all_twelve_exact_prices() {
    let (_dir, _config, state, id) = fixture(true, false);
    let mut game = state.game.lock().unwrap();
    let first = offer(&game, &id, NOW);
    game.disconnect(&id);
    game.connect(&id);
    assert_eq!(offer(&game, &id, NOW)["offers"], first["offers"]);
    let prices = [
        0, 1000, 500, 5000, 7000, 10000, 8000, 14000, 12000, 20000, 16000, 18000,
    ];
    let mut seen = [false; 12];
    for n in 0..1000 {
        let shop = offer(&game, &id, NOW + n * ROTATION_MS);
        let row = &shop["offers"][0];
        let egg = row["eggId"].as_u64().unwrap() as usize;
        seen[egg] = true;
        assert_eq!(row["price"], prices[egg]);
        assert_eq!(shop["epoch"], (NOW + n * ROTATION_MS) / ROTATION_MS);
    }
    assert!(seen.into_iter().all(|present| present));
}

fn rollback(dir: &Path, state: &AppState, game: &mut Game, id: &str, message: &Value, now: u64) {
    let before = game.players[id].profile();
    let before_events = events(game);
    let dirty = game.dirty;
    let before_disk = fs::read(dir.join("profiles.json")).unwrap();
    let blocked = dir.join("profiles.json.tmp");
    fs::create_dir(&blocked).unwrap();
    assert!(state.care_action_at(game, id, message, now).is_err());
    assert_eq!(game.players[id].profile(), before);
    assert_eq!(events(game), before_events);
    assert_eq!(game.dirty, dirty);
    assert_eq!(fs::read(dir.join("profiles.json")).unwrap(), before_disk);
    fs::remove_dir(blocked).unwrap();
    assert_eq!(
        state.care_action_at(game, id, message, now).unwrap()["accepted"],
        true
    );
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn failed_purchase_and_hatch_rollback_wallet_egg_rng_record_receipts_history_events_and_disk() {
    let (dir, _, state, id) = fixture(true, false);
    let mut game = state.game.lock().unwrap();
    game.events.push(Event {
        target: Some(id.clone()),
        message: json!({"kind":"existing"}),
    });
    let buy = buy_request(&game, &id, NOW);
    rollback(dir.path(), &state, &mut game, &id, &buy, NOW);
    let hatch = hatch_request(&game, &id);
    rollback(dir.path(), &state, &mut game, &id, &hatch, NOW + 1);
    assert_eq!(
        game.players[&id]
            .tiny_collection
            .as_ref()
            .unwrap()
            .chao
            .len(),
        1
    );
    assert_eq!(game.players[&id].care.history.len(), 3);
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn duplicate_devices_purchase_and_hatch_once_and_different_operations_share_the_revision() {
    let (_dir, _config, state, id) = fixture(true, false);
    for hatching in [false, true] {
        let command = {
            let game = state.game.lock().unwrap();
            if hatching {
                hatch_request(&game, &id)
            } else {
                buy_request(&game, &id, NOW)
            }
        };
        let barrier = Arc::new(Barrier::new(3));
        let results = std::thread::scope(|scope| {
            let jobs: Vec<_> = (0..2)
                .map(|_| {
                    let barrier = barrier.clone();
                    let state = &state;
                    let command = &command;
                    let id = &id;
                    scope.spawn(move || {
                        barrier.wait();
                        let mut game = state.game.lock().unwrap();
                        state
                            .care_action_at(&mut game, id, command, NOW + 1)
                            .unwrap()
                    })
                })
                .collect();
            barrier.wait();
            jobs.into_iter()
                .map(|job| job.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            results
                .iter()
                .filter(|r| r["accepted"] == true && r["replayed"] == false)
                .count(),
            1
        );
        assert_eq!(results.iter().filter(|r| r["replayed"] == true).count(), 1);
        let mut game = state.game.lock().unwrap();
        let mut different = command.clone();
        different["requestId"] = json!(Uuid::new_v4().to_string());
        reject(
            &state,
            &mut game,
            &id,
            &different,
            NOW + 1,
            "stale-revision",
        );
    }
    let game = state.game.lock().unwrap();
    assert_eq!(
        game.players[&id]
            .tiny_collection
            .as_ref()
            .unwrap()
            .chao
            .len(),
        1
    );
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn collection_capacity_is_eight_total_and_hatching_frees_the_single_slot_within_the_same_hour() {
    let (_dir, _config, state, id) = fixture(true, false);
    let mut game = state.game.lock().unwrap();
    let active_stats = game.players[&id].stats.clone();
    for _ in 1..MAX_CHAO {
        let buy = buy_request(&game, &id, NOW);
        assert_eq!(
            state.care_action_at(&mut game, &id, &buy, NOW).unwrap()["accepted"],
            true
        );
        let hatch = hatch_request(&game, &id);
        assert_eq!(
            state.care_action_at(&mut game, &id, &hatch, NOW).unwrap()["accepted"],
            true
        );
    }
    let buy = buy_request(&game, &id, NOW);
    reject(&state, &mut game, &id, &buy, NOW, "collection-full");
    let public = collection::public(&game, &game.players[&id], NOW);
    assert_eq!(public["chao"].as_array().unwrap().len(), MAX_CHAO);
    assert_eq!(public["activeChaoId"], id);
    assert_eq!(game.players[&id].stats, active_stats);
    assert_eq!(game.players.len(), 1);
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn stale_stock_unoffered_eggs_wrong_hatch_identity_training_and_clock_do_not_spend_or_replace_chao()
{
    let (_dir, _config, state, id) = fixture(true, false);
    let mut game = state.game.lock().unwrap();
    let mut buy = buy_request(&game, &id, NOW);
    buy["intent"]["stockEpoch"] = json!(NOW / ROTATION_MS - 1);
    reject(&state, &mut game, &id, &buy, NOW, "stale-stock");
    let mut buy = buy_request(&game, &id, NOW);
    buy["intent"]["eggId"] = json!((buy["intent"]["eggId"].as_u64().unwrap() + 1) % 12);
    reject(&state, &mut game, &id, &buy, NOW, "egg-not-offered");
    let begin = request(
        1,
        json!({"kind":"training_start","stat":"run","durationMs":60_000,"quotedCostRings":1}),
    );
    assert_eq!(
        state.care_action_at(&mut game, &id, &begin, NOW).unwrap()["accepted"],
        true
    );
    let buy = buy_request(&game, &id, NOW);
    reject(&state, &mut game, &id, &buy, NOW, "activity-conflict");
    assert_eq!(
        state
            .settle_ready_training_at(&mut game, NOW + 60_000)
            .unwrap(),
        1
    );
    let buy = buy_request(&game, &id, NOW + 60_000);
    assert_eq!(
        state
            .care_action_at(&mut game, &id, &buy, NOW + 60_000)
            .unwrap()["accepted"],
        true
    );
    let wrong = request(
        game.players[&id].care.revision,
        json!({"kind":"tiny_egg_hatch","ownedEggId":Uuid::new_v4().to_string()}),
    );
    reject(
        &state,
        &mut game,
        &id,
        &wrong,
        NOW + 60_000,
        "egg-id-mismatch",
    );
    let hatch = hatch_request(&game, &id);
    reject(
        &state,
        &mut game,
        &id,
        &hatch,
        NOW + 59_999,
        "collection-clock-error",
    );
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn temporarily_missing_source_preserves_owned_egg_and_reenabling_source_hatches_the_same_identity()
{
    let (dir, config, state, id) = fixture(true, false);
    let owned;
    {
        let mut game = state.game.lock().unwrap();
        let buy = buy_request(&game, &id, NOW);
        assert_eq!(
            state.care_action_at(&mut game, &id, &buy, NOW).unwrap()["accepted"],
            true
        );
        owned = game.players[&id].tiny_collection.clone();
    }
    let unavailable = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    {
        let mut game = unavailable.game.lock().unwrap();
        game.connect(&id);
        assert_eq!(game.players[&id].tiny_collection, owned);
        let hatch = hatch_request(&game, &id);
        reject(
            &unavailable,
            &mut game,
            &id,
            &hatch,
            NOW + 1,
            "native-data-unavailable",
        );
    }
    let restored = AppState::load(config).unwrap();
    let mut game = restored.game.lock().unwrap();
    game.connect(&id);
    let hatch = hatch_request(&game, &id);
    assert_eq!(
        restored
            .care_action_at(&mut game, &id, &hatch, NOW + 1)
            .unwrap()["accepted"],
        true
    );
    assert_eq!(
        game.players[&id].tiny_collection.as_ref().unwrap().chao[0].origin,
        owned.unwrap().pending_egg.unwrap()
    );
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn corrupted_native_body_owner_ids_or_missing_receipts_fail_restore_without_rewriting_save() {
    let (dir, config, state, id) = fixture(true, false);
    {
        let mut game = state.game.lock().unwrap();
        let buy = buy_request(&game, &id, NOW);
        assert_eq!(
            state.care_action_at(&mut game, &id, &buy, NOW).unwrap()["accepted"],
            true
        );
        let hatch = hatch_request(&game, &id);
        assert_eq!(
            state.care_action_at(&mut game, &id, &hatch, NOW).unwrap()["accepted"],
            true
        );
    }
    let original = disk(dir.path(), &id);
    let mut cases = Vec::new();
    let mut p = original.clone();
    p["tinyCollection"]["ownerId"] = json!(Uuid::new_v4().to_string());
    cases.push(p);
    let mut p = original.clone();
    p["tinyCollection"]["chao"][0]["nativeRecord"][0x80] = json!(255);
    cases.push(p);
    let mut p = original.clone();
    p["tinyCollection"]["chao"][0]["id"] = json!(id);
    cases.push(p);
    let mut p = original.clone();
    p["care"]["history"] = json!([]);
    p["care"]["receipts"] = json!({});
    cases.push(p);
    for p in cases {
        let bytes = serde_json::to_vec(&json!([p])).unwrap();
        fs::write(dir.path().join("profiles.json"), &bytes).unwrap();
        assert!(AppState::load(config.clone()).is_err());
        assert_eq!(fs::read(dir.path().join("profiles.json")).unwrap(), bytes);
    }
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn unknown_profile_stat_gear_and_owner_state_survive_purchase_hatch_and_source_restart() {
    let (dir, config, state, id) = fixture(true, true);
    let mut profile = disk(dir.path(), &id);
    profile["futureNamespace"] = json!({"nested":[1,{"opaque":true}]});
    profile["stats"]["run"]["futureMetadata"] = json!("retained");
    profile["gear"]["futureMetadata"] = json!({"calibration":42});
    profile["gear"]["footwear"] = json!("shadow-air-shoes");
    profile["gear"]["hands"] = json!("knuckles-hammer-gloves");
    let original = profile.clone();
    fs::write(
        dir.path().join("profiles.json"),
        serde_json::to_vec(&json!([profile])).unwrap(),
    )
    .unwrap();
    drop(state);
    let next = AppState::load(config.clone()).unwrap();
    {
        let mut game = next.game.lock().unwrap();
        game.connect(&id);
        let buy = buy_request(&game, &id, NOW);
        assert_eq!(
            next.care_action_at(&mut game, &id, &buy, NOW).unwrap()["accepted"],
            true
        );
        let hatch = hatch_request(&game, &id);
        assert_eq!(
            next.care_action_at(&mut game, &id, &hatch, NOW).unwrap()["accepted"],
            true
        );
    }
    let restored = AppState::load(config).unwrap();
    let game = restored.game.lock().unwrap();
    let current = game.players[&id].profile();
    for field in [
        "id",
        "name",
        "tokenHash",
        "admin",
        "stats",
        "gear",
        "form",
        "alignment",
        "futureNamespace",
    ] {
        assert_eq!(current[field], original[field], "{field}");
    }
    assert_eq!(
        current["tinyCollection"]["chao"].as_array().unwrap().len(),
        1
    );
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn insufficient_wallet_and_distinct_simultaneous_purchase_ids_cannot_overdraw_or_add_two_eggs() {
    let (_dir, _config, state, id) = fixture(true, false);
    let mut now = NOW;
    {
        let mut game = state.game.lock().unwrap();
        while offer(&game, &id, now)["offers"][0]["price"] == 0 {
            now += ROTATION_MS;
        }
        game.players.get_mut(&id).unwrap().rings = 0;
        let buy = buy_request(&game, &id, now);
        reject(&state, &mut game, &id, &buy, now, "insufficient-rings");
        game.players.get_mut(&id).unwrap().rings = 999_999;
    }
    let commands = {
        let game = state.game.lock().unwrap();
        [buy_request(&game, &id, now), buy_request(&game, &id, now)]
    };
    let barrier = Arc::new(Barrier::new(3));
    let results = std::thread::scope(|scope| {
        let jobs: Vec<_> = commands
            .iter()
            .map(|command| {
                let barrier = barrier.clone();
                let state = &state;
                let id = &id;
                scope.spawn(move || {
                    barrier.wait();
                    let mut game = state.game.lock().unwrap();
                    state.care_action_at(&mut game, id, command, now).unwrap()
                })
            })
            .collect();
        barrier.wait();
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|r| r["accepted"] == true).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| r["code"] == "stale-revision")
            .count(),
        1
    );
    let game = state.game.lock().unwrap();
    assert_eq!(game.players[&id].care.revision, 2);
    assert!(
        game.players[&id]
            .tiny_collection
            .as_ref()
            .unwrap()
            .pending_egg
            .is_some()
    );
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn purchase_and_later_training_keep_the_hatch_revision_reserved() {
    let (_dir, _config, state, id) = fixture(true, false);
    let mut game = state.game.lock().unwrap();
    game.players.get_mut(&id).unwrap().care.revision = u64::MAX - 1;
    let buy = buy_request(&game, &id, NOW);
    reject(&state, &mut game, &id, &buy, NOW, "receipt-capacity");
    game.players.get_mut(&id).unwrap().care.revision = 1;
    let buy = buy_request(&game, &id, NOW);
    assert_eq!(
        state.care_action_at(&mut game, &id, &buy, NOW).unwrap()["accepted"],
        true
    );
    game.players.get_mut(&id).unwrap().care.revision = u64::MAX - 2;
    let begin = request(
        u64::MAX - 2,
        json!({"kind":"training_start","stat":"run","durationMs":60_000,"quotedCostRings":1}),
    );
    reject(&state, &mut game, &id, &begin, NOW, "receipt-capacity");
    game.players.get_mut(&id).unwrap().care.revision = u64::MAX - 3;
    let begin = request(
        u64::MAX - 3,
        json!({"kind":"training_start","stat":"run","durationMs":60_000,"quotedCostRings":1}),
    );
    assert_eq!(
        state.care_action_at(&mut game, &id, &begin, NOW).unwrap()["accepted"],
        true
    );
    assert_eq!(
        state
            .settle_ready_training_at(&mut game, NOW + 60_000)
            .unwrap(),
        1
    );
    assert_eq!(game.players[&id].care.revision, u64::MAX - 1);
    let other = request(u64::MAX - 1, json!({"kind":"pet"}));
    reject(
        &state,
        &mut game,
        &id,
        &other,
        NOW + 60_000,
        "receipt-capacity",
    );
    let hatch = hatch_request(&game, &id);
    assert_eq!(
        state
            .care_action_at(&mut game, &id, &hatch, NOW + 60_000)
            .unwrap()["accepted"],
        true
    );
    assert_eq!(game.players[&id].care.revision, u64::MAX);
    assert!(
        game.players[&id]
            .tiny_collection
            .as_ref()
            .unwrap()
            .pending_egg
            .is_none()
    );
}

#[test]
#[ignore = "Requires explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn back_online_from_tiny_garden_the_held_egg_moves_into_the_nest_as_a_gba_egg() {
    let (_dir, _config, state, id) = fixture(true, false);
    let mut game = state.game.lock().unwrap();
    let buy = buy_request(&game, &id, NOW);
    assert_eq!(
        state.care_action_at(&mut game, &id, &buy, NOW).unwrap()["accepted"],
        true
    );
    let v = game.players[&id]
        .tiny_collection
        .as_ref()
        .unwrap()
        .pending_egg
        .as_ref()
        .unwrap()
        .native_egg_id;
    assert!(game.bring_tiny_egg(&id).is_some());
    let p = &game.players[&id];
    assert!(p.tiny_collection.as_ref().unwrap().pending_egg.is_none());
    let egg = p.life.nest.last().unwrap();
    assert_eq!(egg.gene.jewel, [v; 2]);
    assert_eq!(egg.egg_color, Some(if v == 0 { 0 } else { v + 0x35 }));
    assert!(game.bring_tiny_egg(&id).is_none(), "only once");
}
