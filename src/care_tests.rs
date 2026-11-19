//! Transaction tests use made-up signed rows, never a decoded retail table.
use crate::{
    activities::ActivityCatalog,
    care::{self, CareState, MAX_QUANTITY, MAX_RECEIPTS, Receipt, Request},
    care_catalog::CareCatalog,
    game::{ALL_STATS, Game, Player, Race},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture(owner: bool) -> (Game, String) {
    let mut game = Game::new(vec![], BTreeMap::new(), owner, 1000.).unwrap();
    game.care_catalog = CareCatalog::synthetic();
    let session = game
        .create_session(&json!({"name":"Synthetic care test"}))
        .unwrap();
    let id = session["player"]["id"].as_str().unwrap().to_owned();
    game.connect(&id);
    (game, id)
}
fn request(sequence: u32, revision: u64, intent: Value) -> Value {
    json!({"type":"care","requestId":format!("{sequence:08x}-0000-4000-8000-000000000001"),
        "expectedRevision":revision,"intent":intent})
}
fn state(p: &Player) -> Value {
    json!({"care":p.care,"stats":p.stats,"rings":p.rings,"energy":p.energy,
        "satiety":p.hunger,"happiness":p.happiness,"form":p.form,"alignment":p.alignment,"admin":p.admin})
}
fn consume(sequence: u32, revision: u64, item: &str) -> Value {
    request(sequence, revision, json!({"kind":"consume","itemId":item}))
}

#[test]
fn all_seven_signed_channels_clamp_without_changing_levels_grades_or_form() {
    let (mut game, id) = fixture(false);
    let before = [9995, 5, 100, 10, 9990, 5, 9980];
    let after = [9999, 0, 117, 0, 9999, 0, 9999];
    let changes = [4, -5, 17, -10, 9, -5, 19];
    {
        let p = game.players.get_mut(&id).unwrap();
        p.care.inventory.insert("sa2:0".into(), 1);
        p.happiness = 1.;
        p.hunger = 98.;
        p.energy = 40.;
        for (name, points) in ALL_STATS.into_iter().zip(before) {
            let stat = p.stats.get_mut(name).unwrap();
            stat.points = points;
            stat.level = 7;
            stat.grade = 2;
        }
    }
    let response = care::action(&mut game, &id, &consume(1, 0, "sa2:0"));
    assert_eq!(response["accepted"], true);
    let p = &game.players[&id];
    for ((name, points), delta) in ALL_STATS.into_iter().zip(after).zip(changes) {
        assert_eq!(p.stats[name].points, points, "{name}");
        assert_eq!(p.stats[name].level, 7, "{name}");
        assert_eq!(p.stats[name].grade, 2, "{name}");
        assert_eq!(response["effects"]["points"][name], delta, "{name}");
    }
    assert_eq!(p.happiness, 0.);
    assert_eq!(p.hunger, 100.);
    assert_eq!(p.energy, 40.);
    assert_eq!(response["effects"]["happiness"], -1.);
    assert_eq!(response["effects"]["satiety"], 2.);
    assert_eq!(p.care.raw_totals["power"], -19);
    assert_eq!(p.care.raw_totals["unknown18"], -37);
    assert_eq!(p.care.evidence.species_history, 1);
    assert_eq!(p.care.evidence.animal_uses, 1);
    assert_eq!(p.care.evidence.public()["nativeEvolution"]["ready"], false);
    assert_eq!(p.form, "normal");
    assert!(!p.care.inventory.contains_key("sa2:0"));
    assert_eq!(response["effects"]["unknown18Applied"], false);
}

#[test]
fn owner_perfect_stats_and_privileges_survive_negative_native_effects() {
    let (mut game, id) = fixture(true);
    let before = game.players[&id].stats.clone();
    game.players
        .get_mut(&id)
        .unwrap()
        .care
        .inventory
        .insert("sa2:0".into(), 1);
    let response = care::action(&mut game, &id, &consume(2, 0, "sa2:0"));
    assert_eq!(response["accepted"], true);
    assert_eq!(game.players[&id].stats, before);
    assert!(game.players[&id].admin);
    for name in ALL_STATS {
        assert_eq!(response["effects"]["points"][name], 0);
    }
    assert_eq!(game.players[&id].care.evidence.animal_uses, 1);
}

#[test]
fn drives_use_separate_history_and_every_principal_channel() {
    let (mut game, id) = fixture(false);
    for (offset, (native, skill)) in [(21, "swim"), (22, "run"), (23, "power"), (24, "fly")]
        .into_iter()
        .enumerate()
    {
        game.players
            .get_mut(&id)
            .unwrap()
            .care
            .inventory
            .insert(format!("sa2:{native}"), 1);
        game.time += 1500.;
        let response = care::action(
            &mut game,
            &id,
            &consume(10 + offset as u32, offset as u64, &format!("sa2:{native}")),
        );
        assert_eq!(response["accepted"], true);
        assert_eq!(response["effects"]["points"][skill], 11);
    }
    let p = &game.players[&id];
    assert_eq!(p.care.evidence.species_history, 0);
    assert_eq!(p.care.evidence.animal_uses, 0);
    assert_eq!(p.care.evidence.drive_uses, 4);
    assert_eq!(
        p.care.evidence.public()["learnedBehavior"]["unlocked"],
        json!([])
    );
}

#[test]
fn consumption_requires_owned_quantity_and_removes_empty_stacks() {
    let (mut game, id) = fixture(false);
    let before = state(&game.players[&id]);
    assert_eq!(
        care::action(&mut game, &id, &consume(20, 0, "garden-fruit"))["code"],
        "item-not-owned"
    );
    assert_eq!(state(&game.players[&id]), before);
    game.players
        .get_mut(&id)
        .unwrap()
        .care
        .inventory
        .insert("garden-fruit".into(), 2);
    assert_eq!(
        care::action(&mut game, &id, &consume(21, 0, "garden-fruit"))["accepted"],
        true
    );
    assert_eq!(game.players[&id].care.inventory["garden-fruit"], 1);
    game.time += 1500.;
    assert_eq!(
        care::action(&mut game, &id, &consume(22, 1, "garden-fruit"))["accepted"],
        true
    );
    assert!(
        !game.players[&id]
            .care
            .inventory
            .contains_key("garden-fruit")
    );
    game.time += 1500.;
    let before = state(&game.players[&id]);
    assert_eq!(
        care::action(&mut game, &id, &consume(23, 2, "garden-fruit"))["code"],
        "item-not-owned"
    );
    assert_eq!(state(&game.players[&id]), before);
}

#[test]
fn missing_native_data_never_consumes_an_owned_item_or_charges_a_purchase() {
    let (mut game, id) = fixture(false);
    game.care_catalog = CareCatalog::default();
    let p = game.players.get_mut(&id).unwrap();
    p.care.inventory.insert("sa2:22".into(), 1);
    p.x = -7.;
    p.z = 0.;
    let before = state(p);
    assert_eq!(
        care::action(&mut game, &id, &consume(30, 0, "sa2:22"))["code"],
        "native-data-unavailable"
    );
    assert_eq!(
        care::action(
            &mut game,
            &id,
            &request(31, 0, json!({"kind":"purchase","itemId":"sa2:22"}))
        )["code"],
        "native-data-unavailable"
    );
    assert_eq!(state(&game.players[&id]), before);
    assert_eq!(care::public(&game, &id)["catalog"]["enabled"], false);
}

#[test]
fn forged_fields_and_invalid_identities_are_rejected_without_any_mutation() {
    let (mut game, id) = fixture(false);
    game.players
        .get_mut(&id)
        .unwrap()
        .care
        .inventory
        .insert("garden-fruit".into(), 1);
    let base = consume(40, 0, "garden-fruit");
    let mut forged = Vec::new();
    for (field, value) in [
        ("playerId", json!(id)),
        ("stats", json!({"run":9999})),
        ("timestamp", json!(0)),
        ("quantity", json!(99)),
    ] {
        let mut candidate = base.clone();
        candidate[field] = value;
        forged.push(candidate);
    }
    for (field, value) in [
        ("gain", json!(9999)),
        ("quantity", json!(2)),
        ("needs", json!({"energy":100})),
        ("history", json!(0xffffff)),
    ] {
        let mut candidate = base.clone();
        candidate["intent"][field] = value;
        forged.push(candidate);
    }
    for item in ["sa2:25", "sa2:255", "sa2:00", "sa2:-1", "not-an-item", ""] {
        forged.push(consume(40, 0, item));
    }
    let mut wrong_type = base.clone();
    wrong_type["type"] = json!("action");
    forged.push(wrong_type);
    let mut invalid_uuid = base.clone();
    invalid_uuid["requestId"] = json!("client-counter");
    forged.push(invalid_uuid);
    let mut negative_revision = base;
    negative_revision["expectedRevision"] = json!(-1);
    forged.push(negative_revision);
    let before = state(&game.players[&id]);
    for candidate in forged {
        let response = care::action(&mut game, &id, &candidate);
        assert_eq!(response["code"], "invalid-request", "{candidate}");
        assert_eq!(state(&game.players[&id]), before);
    }
    assert!(game.care_catalog.item(25).is_none());
}

#[test]
fn committed_receipt_matches_the_complete_payload_and_survives_activity_changes() {
    let (mut game, id) = fixture(false);
    game.players
        .get_mut(&id)
        .unwrap()
        .care
        .inventory
        .insert("garden-fruit".into(), 2);
    let original = consume(50, 0, "garden-fruit");
    let committed = care::action(&mut game, &id, &original);
    assert_eq!(committed["accepted"], true);
    let p = game.players.get_mut(&id).unwrap();
    p.care.activity_lock = Some("server-training-job".into());
    p.zone = "mission".into();
    p.connections = 0;
    let before = state(p);
    let replay = care::action(&mut game, &id, &original);
    assert_eq!(replay["accepted"], true);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["effects"], committed["effects"]);
    assert_eq!(replay["revision"], committed["revision"]);
    assert_eq!(state(&game.players[&id]), before);
    let mut changed_revision = original.clone();
    changed_revision["expectedRevision"] = json!(1);
    let mut changed_intent = original;
    changed_intent["intent"] = json!({"kind":"pet"});
    for changed in [changed_revision, changed_intent] {
        assert_eq!(
            care::action(&mut game, &id, &changed)["code"],
            "request-id-conflict"
        );
        assert_eq!(state(&game.players[&id]), before);
    }
    assert_eq!(
        care::action(&mut game, &id, &consume(51, 1, "garden-fruit"))["code"],
        "activity-conflict"
    );
    assert_eq!(state(&game.players[&id]), before);
}

#[test]
fn stale_revision_and_second_socket_cannot_double_spend() {
    let (mut game, id) = fixture(false);
    game.connect(&id);
    game.players
        .get_mut(&id)
        .unwrap()
        .care
        .inventory
        .insert("garden-fruit".into(), 1);
    assert_eq!(game.players[&id].connections, 2);
    assert_eq!(
        care::action(&mut game, &id, &consume(60, 0, "garden-fruit"))["accepted"],
        true
    );
    let before = state(&game.players[&id]);
    assert_eq!(
        care::action(&mut game, &id, &consume(61, 0, "garden-fruit"))["code"],
        "stale-revision"
    );
    assert_eq!(state(&game.players[&id]), before);
    assert!(
        !game.players[&id].care.receipts.contains_key(
            request(61, 0, json!({"kind":"pet"}))["requestId"]
                .as_str()
                .unwrap()
        )
    );
}

#[test]
fn full_receipt_ledger_fails_closed_and_retains_old_retries() {
    let (mut game, id) = fixture(false);
    let p = game.players.get_mut(&id).unwrap();
    for index in 0..MAX_RECEIPTS {
        let request: Request = serde_json::from_value(request(
            100 + index as u32,
            index as u64,
            json!({"kind":"pet"}),
        ))
        .unwrap();
        p.care.receipts.insert(
            request.request_id.clone(),
            Receipt {
                request,
                revision: index as u64 + 1,
                effects: json!({"happiness":2}),
            },
        );
    }
    p.care.revision = MAX_RECEIPTS as u64;
    p.care.inventory.insert("garden-fruit".into(), 1);
    let before = state(p);
    let full = care::action(
        &mut game,
        &id,
        &consume(9000, MAX_RECEIPTS as u64, "garden-fruit"),
    );
    assert_eq!(full["code"], "receipt-capacity");
    assert_eq!(state(&game.players[&id]), before);
    let replay = care::action(&mut game, &id, &request(100, 0, json!({"kind":"pet"})));
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["revision"], 1);
    assert_eq!(game.players[&id].care.receipts.len(), MAX_RECEIPTS);
    assert_eq!(state(&game.players[&id]), before);
}

#[test]
fn pending_drops_are_private_nearby_once_only_and_cannot_respawn_after_claim() {
    let (mut game, id) = fixture(false);
    let other = game.create_session(&json!({"name":"Other Chao"})).unwrap()["player"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    game.connect(&other);
    for who in [&id, &other] {
        game.players.get_mut(who).unwrap().zone = "mission".into();
    }
    let catalog = game.care_catalog.clone();
    care::mission_drop(
        game.players.get_mut(&id).unwrap(),
        &catalog,
        "mission-test",
        "robot-test",
        2.,
        3.,
    );
    let spawn_revision = game.players[&id].care.revision;
    care::mission_drop(
        game.players.get_mut(&id).unwrap(),
        &catalog,
        "mission-test",
        "robot-test",
        2.,
        3.,
    );
    assert_eq!(game.players[&id].care.revision, spawn_revision);
    assert_eq!(game.players[&id].care.pickups.len(), 1);
    let pickup = game.players[&id]
        .care
        .pickups
        .values()
        .next()
        .unwrap()
        .clone();
    let native = pickup.native_id;
    let intent = json!({"kind":"pickup","pickupId":pickup.id});
    assert_eq!(
        care::action(&mut game, &other, &request(70, 0, intent.clone()))["code"],
        "pickup-unavailable"
    );
    let before = state(&game.players[&id]);
    assert_eq!(
        care::action(&mut game, &id, &request(71, spawn_revision, intent.clone()))["code"],
        "out-of-range"
    );
    assert_eq!(state(&game.players[&id]), before);
    let p = game.players.get_mut(&id).unwrap();
    p.x = 2.;
    p.z = 3.;
    let claim = request(72, spawn_revision, intent.clone());
    assert_eq!(care::action(&mut game, &id, &claim)["accepted"], true);
    assert_eq!(
        game.players[&id].care.inventory[&format!("sa2:{native}")],
        1
    );
    assert_eq!(game.players[&id].care.evidence.species_history, 0);
    assert!(game.players[&id].care.pickups.is_empty());
    assert_eq!(care::action(&mut game, &id, &claim)["replayed"], true);
    let after_claim = game.players[&id].care.revision;
    assert_eq!(
        care::action(&mut game, &id, &request(73, after_claim, intent))["code"],
        "pickup-unavailable"
    );
    care::mission_drop(
        game.players.get_mut(&id).unwrap(),
        &catalog,
        "mission-test",
        "robot-test",
        2.,
        3.,
    );
    assert!(game.players[&id].care.pickups.is_empty());
    assert_eq!(game.players[&id].care.revision, after_claim);
    assert!(game.players[&other].care.inventory.is_empty());
}

#[test]
fn inventory_limits_keep_currency_and_pending_drop_intact() {
    let (mut game, id) = fixture(false);
    let p = game.players.get_mut(&id).unwrap();
    p.x = -4.;
    p.z = 8.;
    p.care.inventory.insert("garden-fruit".into(), MAX_QUANTITY);
    let before = state(p);
    assert_eq!(
        care::action(
            &mut game,
            &id,
            &request(80, 0, json!({"kind":"purchase","itemId":"garden-fruit"}))
        )["code"],
        "inventory-full"
    );
    assert_eq!(state(&game.players[&id]), before);
    let catalog = game.care_catalog.clone();
    let p = game.players.get_mut(&id).unwrap();
    p.zone = "mission".into();
    p.x = 1.;
    p.z = 1.;
    care::mission_drop(p, &catalog, "mission-capacity", "robot-capacity", 1., 1.);
    let drop = p.care.pickups.values().next().unwrap().clone();
    p.care
        .inventory
        .insert(format!("sa2:{}", drop.native_id), MAX_QUANTITY);
    let revision = p.care.revision;
    let before = state(p);
    assert_eq!(
        care::action(
            &mut game,
            &id,
            &request(81, revision, json!({"kind":"pickup","pickupId":drop.id}))
        )["code"],
        "inventory-full"
    );
    assert_eq!(state(&game.players[&id]), before);
}

#[test]
fn new_care_conflicts_with_every_existing_activity_and_future_server_lock() {
    for scenario in [
        "disconnected",
        "mission",
        "vehicle",
        "race",
        "watch",
        "tiny",
        "karate",
        "training",
    ] {
        let (mut game, id) = fixture(false);
        game.players
            .get_mut(&id)
            .unwrap()
            .care
            .inventory
            .insert("garden-fruit".into(), 1);
        match scenario {
            "disconnected" => game.players.get_mut(&id).unwrap().connections = 0,
            "mission" => game.players.get_mut(&id).unwrap().zone = "mission".into(),
            "vehicle" => {
                game.players.get_mut(&id).unwrap().vehicle = Some("dark-chao-walker".into())
            }
            "race" => {
                game.players.get_mut(&id).unwrap().race = Some(Race {
                    started_at: game.time,
                    next_checkpoint: 0,
                    course_id: None,
                })
            }
            "watch" => {
                game.action(&id, &json!({"type":"action","action":"watch-start"}));
                assert!(game.players[&id].watch.is_some());
            }
            "tiny" => {
                game.action(
                    &id,
                    &json!({"type":"action","action":"tiny-start","gameId":"memory"}),
                );
                assert!(game.players[&id].tiny_game.is_some());
            }
            "karate" => {
                game.activities=ActivityCatalog::from_values(json!({}),json!({"tiers":[{"id":"test-tier","name":"Synthetic tier","opponents":[{"id":"test-opponent","name":"Synthetic opponent","nativeIndex":0,"skills":[1,1,1,1,1,1,1,0]}]}]})).unwrap();
                game.action(
                    &id,
                    &json!({"type":"action","action":"karate-enter","tier":"test-tier"}),
                );
                assert!(game.players[&id].karate.is_some());
            }
            "training" => {
                game.players.get_mut(&id).unwrap().care.activity_lock =
                    Some("training:synthetic-job".into())
            }
            _ => unreachable!(),
        }
        let before = state(&game.players[&id]);
        assert_eq!(
            care::action(&mut game, &id, &consume(90, 0, "garden-fruit"))["code"],
            "activity-conflict",
            "{scenario}"
        );
        assert_eq!(state(&game.players[&id]), before, "{scenario}");
        assert_eq!(
            care::public(&game, &id)["eligibility"]["canCare"],
            false,
            "{scenario}"
        );
    }
}

#[test]
fn saved_care_round_trip_keeps_pending_claims_receipts_and_signed_evidence() {
    let (mut game, id) = fixture(false);
    game.players
        .get_mut(&id)
        .unwrap()
        .care
        .inventory
        .insert("sa2:0".into(), 2);
    let original = consume(95, 0, "sa2:0");
    assert_eq!(care::action(&mut game, &id, &original)["accepted"], true);
    let catalog = game.care_catalog.clone();
    care::mission_drop(
        game.players.get_mut(&id).unwrap(),
        &catalog,
        "persisted-mission",
        "persisted-robot",
        1.,
        2.,
    );
    let saved = serde_json::to_value(&game.players[&id].care).unwrap();
    assert_eq!(
        CareState::restore(Some(&saved)).unwrap(),
        game.players[&id].care
    );
    assert_eq!(CareState::restore(None).unwrap(), CareState::default());
    assert!(CareState::restore(Some(&Value::Null)).is_err());
    for (field, value) in [
        ("version", json!(2)),
        ("inventory", json!({"sa2:25":1})),
        ("inventory", json!({"garden-fruit":0})),
        (
            "evidence",
            json!({"version":1,"speciesHistory":1<<25,"animalUses":1,"driveUses":0}),
        ),
    ] {
        let mut invalid = saved.clone();
        invalid[field] = value;
        assert!(CareState::restore(Some(&invalid)).is_err(), "{field}");
    }
    let profile = game.players.get_mut(&id).unwrap();
    profile.care = CareState::restore(Some(&saved)).unwrap();
    assert_eq!(care::action(&mut game, &id, &original)["replayed"], true);
}

#[test]
fn revision_overflow_rejects_new_effects_without_receipt_or_item_loss() {
    let (mut game, id) = fixture(false);
    let p = game.players.get_mut(&id).unwrap();
    p.care.revision = u64::MAX;
    p.care.inventory.insert("garden-fruit".into(), 1);
    let before = state(p);
    assert_eq!(
        care::action(&mut game, &id, &consume(99, u64::MAX, "garden-fruit"))["code"],
        "receipt-capacity"
    );
    assert_eq!(state(&game.players[&id]), before);
}

#[test]
fn seven_tiny_fruit_rows_use_signed_adapted_effects_and_keep_levels_grades_hidden_stats_and_energy()
{
    let (mut game, id) = fixture(false);
    // Made-up rows exercise the adapter; no decoded original table is embedded.
    game.tiny_shop_catalog = Some(tiny_garden_core::shop::TinyShopCatalog {
        fruits: std::array::from_fn(|id| tiny_garden_core::shop::TinyFruit {
            id: id as u8,
            price: 40 + id as u32,
            raw: [-1, 2, -3, 4, -5, 6, -7],
        }),
    });
    game.players.get_mut(&id).unwrap().rings = 1000;
    for native_id in 0..7 {
        game.time += 2000.;
        let item = format!("tiny-fruit:{native_id}");
        let (before, rings) = {
            let p = game.players.get_mut(&id).unwrap();
            p.happiness = 50.;
            p.hunger = 50.;
            p.energy = 31.;
            for stat in p.stats.values_mut() {
                stat.points = 100;
                stat.level = 7;
                stat.grade = 2;
            }
            (p.stats.clone(), p.rings)
        };
        let revision = game.players[&id].care.revision;
        let buy = request(
            200 + native_id * 2,
            revision,
            json!({"kind":"tiny_purchase","itemId":item}),
        );
        let bought = care::action(&mut game, &id, &buy);
        assert_eq!(bought["accepted"], true);
        assert_eq!(game.players[&id].rings, rings - 40 - native_id);
        assert_eq!(game.players[&id].care.inventory[&item], 1);
        let feed = consume(201 + native_id * 2, revision + 1, &item);
        let applied = care::action(&mut game, &id, &feed);
        assert_eq!(applied["accepted"], true);
        assert_eq!(
            applied["effects"]["raw"]["applicationRules"],
            care::TINY_FRUIT_RULES
        );
        assert_eq!(applied["effects"]["raw"]["nativeLevelUpPorted"], false);
        assert_eq!(
            applied["effects"]["points"],
            json!({"swim":-30,"fly":40,"run":-50,"power":60,"stamina":-70})
        );
        let p = &game.players[&id];
        assert!(!p.care.inventory.contains_key(&item));
        assert_eq!((p.happiness, p.hunger, p.energy), (40., 70., 31.));
        assert_eq!(p.stats["luck"], before["luck"]);
        assert_eq!(p.stats["intelligence"], before["intelligence"]);
        assert!(
            p.care.raw_totals.is_empty(),
            "Tiny byte units must not mix with SA2 animal totals"
        );
        assert_eq!(p.care.evidence.animal_uses, 0);
        for name in ALL_STATS {
            assert_eq!(p.stats[name].level, before[name].level);
            assert_eq!(p.stats[name].grade, before[name].grade);
        }
        assert_eq!(care::action(&mut game, &id, &buy)["replayed"], true);
        assert_eq!(care::action(&mut game, &id, &feed)["replayed"], true);
        assert_eq!(game.players[&id].rings, rings - 40 - native_id);
    }
    assert_eq!(
        care::public(&game, &id)["tinyShop"]["items"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
}

#[test]
fn unavailable_tiny_source_preserves_owned_fruit_and_rejects_noncanonical_ids() {
    let (mut game, id) = fixture(false);
    game.players
        .get_mut(&id)
        .unwrap()
        .care
        .inventory
        .insert("tiny-fruit:0".into(), 1);
    let before = state(&game.players[&id]);
    for intent in [
        json!({"kind":"tiny_purchase","itemId":"tiny-fruit:0"}),
        json!({"kind":"consume","itemId":"tiny-fruit:0"}),
    ] {
        assert_eq!(
            care::action(&mut game, &id, &request(230, 0, intent))["code"],
            "native-data-unavailable"
        );
        assert_eq!(state(&game.players[&id]), before);
    }
    for item in [
        "tiny-fruit:00",
        "tiny-fruit:7",
        "tiny-fruit:-1",
        "tiny-fruit:",
        "tiny-fruit:1.0",
    ] {
        assert_eq!(
            care::action(&mut game, &id, &consume(231, 0, item))["code"],
            "invalid-request"
        );
        assert_eq!(state(&game.players[&id]), before);
    }
    let saved = serde_json::to_value(&game.players[&id].care).unwrap();
    assert_eq!(
        CareState::restore(Some(&saved)).unwrap(),
        game.players[&id].care
    );
    assert_eq!(
        care::public(&game, &id)["tinyShop"]["nativeCatalog"]["enabled"],
        false
    );
}
