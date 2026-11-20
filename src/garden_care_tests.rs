//! Fruit, seed, tree, animal-part and drop-table tests with made-up rows only.
use crate::{
    care::{self, CareState, DROP_WEIGHTS},
    care_catalog::CareCatalog,
    game::{Game, Player},
    garden::{self, FRUIT_MS, GardenState, MATURE_MS, MAX_FRUIT, WATER_BONUS_MS},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture() -> (Game, String) {
    let mut game = Game::new(vec![], BTreeMap::new(), false, 1000.).unwrap();
    game.care_catalog = CareCatalog::synthetic();
    let session = game
        .create_session(&json!({"name":"Synthetic garden test"}))
        .unwrap();
    let id = session["player"]["id"].as_str().unwrap().to_owned();
    game.connect(&id);
    (game, id)
}
fn player<'a>(game: &'a mut Game, id: &str) -> &'a mut Player {
    game.players.get_mut(id).unwrap()
}
fn go(game: &mut Game, id: &str, zone: &str, x: f64, z: f64) {
    let p = player(game, id);
    p.zone = zone.into();
    (p.x, p.y, p.z) = (x, 0., z);
}
fn send(game: &mut Game, id: &str, seq: u32, intent: Value) -> Value {
    let revision = game.players[id].care.revision;
    care::action(
        game,
        id,
        &json!({"type":"care","requestId":format!("{seq:08x}-0000-4000-8000-00000000abcd"),
            "expectedRevision":revision,"intent":intent}),
    )
}

#[test]
fn native_fruit_rows_apply_through_hooks_without_invented_alignment() {
    let (mut game, id) = fixture();
    go(&mut game, &id, "garden", 0., 0.);
    {
        let p = player(&mut game, &id);
        p.care.inventory.insert("sa2-fruit:5".into(), 1);
        p.care.inventory.insert("sa2-fruit:6".into(), 1);
        p.hunger = 10.;
        p.alignment = 0.;
    }
    let stamina = game.players[&id].stats["stamina"].points;
    let r = send(
        &mut game,
        &id,
        1,
        json!({"kind":"consume","itemId":"sa2-fruit:5"}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    assert_eq!(r["effects"]["points"]["stamina"], 5);
    assert_eq!(r["effects"]["satiety"], 40.);
    assert_eq!(r["effects"]["body"], "pending-body-module");
    assert_eq!(r["effects"]["specialEffect"], "unknown-not-applied");
    let p = &game.players[&id];
    assert_eq!(p.stats["stamina"].points, stamina + 5);
    assert_eq!(p.alignment, 0.);
    assert!(!p.care.inventory.contains_key("sa2-fruit:5"));
    game.time += 1500.;
    let r = send(
        &mut game,
        &id,
        2,
        json!({"kind":"consume","itemId":"sa2-fruit:6"}),
    );
    assert_eq!(r["accepted"], true);
    assert_eq!(game.players[&id].alignment, 0.);
    assert_eq!(
        send(
            &mut game,
            &id,
            3,
            json!({"kind":"consume","itemId":"sa2-fruit:24"})
        )["code"],
        "invalid-request"
    );
}

#[test]
fn seeds_plant_water_grow_and_harvest_once_per_fruit() {
    let (mut game, id) = fixture();
    go(&mut game, &id, "hub", -4., 8.);
    player(&mut game, &id).rings = 100;
    let r = send(
        &mut game,
        &id,
        10,
        json!({"kind":"purchase","itemId":"seed:hero"}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    assert_eq!(game.players[&id].rings, 85);
    let plant = json!({"kind":"plant","itemId":"seed:hero","plotId":"garden-1"});
    assert_eq!(
        send(&mut game, &id, 11, plant.clone())["code"],
        "out-of-range"
    );
    go(&mut game, &id, "garden", 10., 2.);
    let r = send(&mut game, &id, 12, plant.clone());
    assert_eq!(r["accepted"], true, "{r}");
    assert!(!game.players[&id].care.inventory.contains_key("seed:hero"));
    // Replaying the same request returns the old outcome without planting again.
    let revision = game.players[&id].care.revision;
    let replay = care::action(
        &mut game,
        &id,
        &json!({"type":"care","requestId":"0000000c-0000-4000-8000-00000000abcd",
            "expectedRevision":revision-1,"intent":plant}),
    );
    assert_eq!(replay["replayed"], true);
    player(&mut game, &id)
        .care
        .inventory
        .insert("seed:round".into(), 1);
    assert_eq!(
        send(
            &mut game,
            &id,
            13,
            json!({"kind":"plant","itemId":"seed:round","plotId":"garden-1"})
        )["code"],
        "plot-occupied"
    );
    let water = json!({"kind":"water","plotId":"garden-1"});
    let r = send(&mut game, &id, 14, water.clone());
    assert_eq!(r["effects"]["growthMs"], WATER_BONUS_MS);
    assert_eq!(send(&mut game, &id, 15, water.clone())["code"], "cooldown");
    let harvest = json!({"kind":"harvest","plotId":"garden-1"});
    assert_eq!(
        send(&mut game, &id, 16, harvest.clone())["code"],
        "no-fruit"
    );
    game.time += MATURE_MS - WATER_BONUS_MS + 2. * FRUIT_MS;
    assert_eq!(send(&mut game, &id, 17, water)["code"], "not-growing");
    let view = care::public(&game, &id)["garden"]["plots"][0].clone();
    assert_eq!(view["tree"]["stage"], "mature");
    assert_eq!(view["tree"]["fruitReady"], 2);
    for seq in [18, 19] {
        let r = send(&mut game, &id, seq, harvest.clone());
        assert_eq!(r["effects"]["inventory"]["sa2-fruit:5"], 1, "{r}");
    }
    assert_eq!(send(&mut game, &id, 20, harvest)["code"], "no-fruit");
    assert_eq!(game.players[&id].care.inventory["sa2-fruit:5"], 2);
}

#[test]
fn garden_groves_bear_native_garden_fruit_and_regrow() {
    let (mut game, id) = fixture();
    go(&mut game, &id, "garden", -7., -5.);
    let harvest = json!({"kind":"harvest","plotId":"garden-grove"});
    for seq in 0..u32::from(MAX_FRUIT) {
        let r = send(&mut game, &id, 30 + seq, harvest.clone());
        assert_eq!(r["effects"]["inventory"]["sa2-fruit:0"], 1, "{r}");
    }
    assert_eq!(
        send(&mut game, &id, 40, harvest.clone())["code"],
        "no-fruit"
    );
    game.time += FRUIT_MS;
    assert_eq!(send(&mut game, &id, 41, harvest)["accepted"], true);
    go(&mut game, &id, "dark", -6., -6.);
    let r = send(
        &mut game,
        &id,
        42,
        json!({"kind":"harvest","plotId":"dark-grove"}),
    );
    assert_eq!(r["effects"]["inventory"]["sa2-fruit:2"], 1);
}

#[test]
fn native_fruit_and_seed_actions_fail_closed_without_verified_rows() {
    let (mut game, id) = fixture();
    game.care_catalog = CareCatalog::default();
    go(&mut game, &id, "hub", -4., 8.);
    player(&mut game, &id).rings = 100;
    assert_eq!(
        send(
            &mut game,
            &id,
            50,
            json!({"kind":"purchase","itemId":"seed:strong"})
        )["code"],
        "native-data-unavailable"
    );
    go(&mut game, &id, "garden", -7., -5.);
    assert_eq!(
        send(
            &mut game,
            &id,
            51,
            json!({"kind":"harvest","plotId":"garden-grove"})
        )["code"],
        "native-data-unavailable"
    );
    assert_eq!(game.players[&id].rings, 100);
}

#[test]
fn animals_show_only_verified_part_categories_and_drives_show_none() {
    let (mut game, id) = fixture();
    go(&mut game, &id, "garden", 0., 0.);
    {
        let p = player(&mut game, &id);
        for item in ["sa2:0", "sa2:3", "sa2:22"] {
            p.care.inventory.insert(item.into(), 1);
        }
    }
    let r = send(
        &mut game,
        &id,
        60,
        json!({"kind":"consume","itemId":"sa2:0"}),
    );
    assert_eq!(r["effects"]["parts"], json!({"ears":0,"tail":0}));
    assert_eq!(r["effects"]["body"], "pending-body-module");
    game.time += 1500.;
    // Synthetic Rabbit has no verified categories: existing parts stay.
    let r = send(
        &mut game,
        &id,
        61,
        json!({"kind":"consume","itemId":"sa2:3"}),
    );
    assert_eq!(r["effects"]["parts"], json!({}));
    game.time += 1500.;
    let r = send(
        &mut game,
        &id,
        62,
        json!({"kind":"consume","itemId":"sa2:22"}),
    );
    assert_eq!(r["effects"]["parts"], json!({}));

    let shown = &game.players[&id].care.animal_parts;
    assert_eq!(shown.len(), 2);
    assert_eq!(shown["ears"], 0);
    let p = player(&mut game, &id);
    care::reset_animal_parts(p);
    care::reset_species_history(p);
    assert!(p.care.animal_parts.is_empty());
    assert_eq!(p.care.evidence.species_history, 0);
}

#[test]
fn version_one_care_saves_migrate_and_garden_round_trips_through_profile() {
    let mut legacy = serde_json::to_value(CareState::default()).unwrap();
    legacy["version"] = json!(1);
    legacy.as_object_mut().unwrap().remove("animalParts");
    let restored = CareState::restore(Some(&legacy)).unwrap();
    assert_eq!(restored.version, 2);
    let mut bad = legacy.clone();
    bad["animalParts"] = json!({"ears":1});
    assert!(CareState::restore(Some(&bad)).is_err());

    let (mut game, id) = fixture();
    go(&mut game, &id, "garden", 10., 2.);
    player(&mut game, &id)
        .care
        .inventory
        .insert("seed:tasty".into(), 1);
    send(
        &mut game,
        &id,
        70,
        json!({"kind":"plant","itemId":"seed:tasty","plotId":"garden-1"}),
    );
    let profile = game.players[&id].profile();
    assert_eq!(
        GardenState::restore(profile.get("garden")).unwrap(),
        game.players[&id].garden
    );
    assert_eq!(profile["garden"]["trees"]["garden-1"]["family"], "tasty");
}

#[test]
fn drop_table_weights_reach_every_item_and_keep_rares_uncommon() {
    let total: u32 = DROP_WEIGHTS.iter().map(|w| u32::from(*w)).sum();
    let mut seen = [0u32; 25];
    for roll in 0..total {
        seen[usize::from(care::drop_choice(roll as u16))] += 1;
    }
    assert_eq!(seen.map(u32::from), DROP_WEIGHTS.map(u32::from));
    assert!(DROP_WEIGHTS[15..21].iter().all(|w| *w < DROP_WEIGHTS[0]));
}

#[test]
fn behaviour_hook_eats_nearby_fruit_atomically() {
    let (mut game, id) = fixture();
    go(&mut game, &id, "garden", 0., 0.);
    assert_eq!(
        care::eat_garden_fruit(&mut game, &id, "garden-grove"),
        Err("out-of-range")
    );
    let (place, x, z) = garden::nearest_fruit(&game.players[&id], game.time).unwrap();
    assert_eq!(place, "garden-grove");
    go(&mut game, &id, "garden", x, z);
    let revision = game.players[&id].care.revision;
    let r = care::eat_garden_fruit(&mut game, &id, &place).unwrap();
    assert_eq!(r["fruitId"], "sa2-fruit:0");
    assert_eq!(game.players[&id].care.revision, revision + 1);
    assert_eq!(game.players[&id].garden.groves[&place].fruit_ready, 2);
    assert_eq!(
        care::eat_garden_fruit(&mut game, &id, &place),
        Err("cooldown")
    );
}
