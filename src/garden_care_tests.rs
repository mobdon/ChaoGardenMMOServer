//! Fruit, seed, tree and animal-part tests with made-up item rows only.
//! Tree timers and capture rules are the native constants from `garden.rs`
//! and `care.rs`; the random sequence is scripted.
use crate::{
    care::{self, CareState, SCRIPTED},
    care_catalog::CareCatalog,
    game::{Game, Player},
    garden::{self, FRAME_MS, GardenState, Scripted},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const FRUIT_RIPE_MS: f64 = 18_100. * FRAME_MS;

fn fixture() -> (Game, String) {
    let mut game = Game::new(vec![], BTreeMap::new(), false, 1000.).unwrap();
    game.care_catalog = CareCatalog::synthetic();
    script(&[0.]);
    let session = game
        .create_session(&json!({"name":"Synthetic garden test"}))
        .unwrap();
    let id = session["player"]["id"].as_str().unwrap().to_owned();
    game.connect(&id);
    (game, id)
}
fn script(values: &[f32]) {
    SCRIPTED.with(|s| *s.borrow_mut() = Some(Scripted(values.to_vec(), 0)));
}
fn player<'a>(game: &'a mut Game, id: &str) -> &'a mut Player {
    game.players.get_mut(id).unwrap()
}
fn go(game: &mut Game, id: &str, place: &str) {
    let (zone, x, z) = garden::position(place).unwrap();
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
fn native_fruit_rows_add_skills_only_and_leave_needs_and_body_to_their_modules() {
    let (mut game, id) = fixture();
    go(&mut game, &id, "garden-1");
    {
        let p = player(&mut game, &id);
        p.care.inventory.insert("sa2-fruit:5".into(), 1);
        p.hunger = 10.;
        p.alignment = 0.;
    }
    let before = game.players[&id].clone();
    let r = send(
        &mut game,
        &id,
        1,
        json!({"kind":"consume","itemId":"sa2-fruit:5"}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    // Four native bites through the life module: Exp, hunger and like.
    assert_eq!(r["effects"]["exp"]["stamina"], 5);
    assert_eq!(r["effects"]["exp"]["intelligence"], 0, "Exp minimum 0");
    assert_eq!(r["effects"]["bites"], 4);
    assert_eq!(r["effects"]["body"], "per-bite-life-event");
    let p = &game.players[&id];
    assert!(p.hunger > before.hunger, "each bite lowers hunger by 2400");
    assert_eq!(
        p.life.like,
        before.life.like + 1,
        "handed over by the owner"
    );
    assert_eq!(p.happiness, before.happiness);
    assert_eq!(p.alignment, 0.);
    assert_eq!(
        send(
            &mut game,
            &id,
            2,
            json!({"kind":"consume","itemId":"sa2-fruit:24"})
        )["code"],
        "invalid-request"
    );
}

#[test]
fn black_market_restocks_on_entry_and_sells_at_native_prices() {
    let (mut game, id) = fixture();
    {
        let p = player(&mut game, &id);
        p.zone = "hub".into();
        (p.x, p.z, p.rings) = (0., 0., 1_000);
    }
    let market = json!({"kind":"market"});
    assert_eq!(
        send(&mut game, &id, 5, market.clone())["code"],
        "out-of-range"
    );
    let buy = |item: &str| json!({"kind":"purchase","itemId":item});
    {
        let p = player(&mut game, &id);
        (p.x, p.z) = (-4., 8.);
    }
    assert_eq!(
        send(&mut game, &id, 6, buy("seed:strong"))["code"],
        "not-in-stock"
    );
    // Draws: eggs, Hero, Dark, Heart, Chao, two-of (0, 1), seed gate, Tasty,
    // two-of (0, 1), alternate mushroom, add it, timer.
    script(&[0., 0., 0., 0., 0., 0., 0.5, 0., 0., 0., 0.5, 0., 0., 0.]);
    let r = send(&mut game, &id, 7, market.clone());
    assert_eq!(r["effects"]["restocked"], true, "{r}");
    let stock: Vec<_> = r["effects"]["market"]["stock"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["itemId"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        stock,
        [
            "sa2-fruit:5",
            "sa2-fruit:6",
            "sa2-fruit:10",
            "sa2-fruit:11",
            "sa2-fruit:7",
            "sa2-fruit:8",
            "seed:strong",
            "seed:tasty",
            "seed:round",
            "seed:triangle",
            "sa2-fruit:21",
        ]
    );
    // Re-entering before the timer runs out keeps the stock.
    assert_eq!(
        send(&mut game, &id, 8, market.clone())["effects"]["restocked"],
        false
    );
    assert_eq!(
        send(&mut game, &id, 9, buy("seed:square"))["code"],
        "not-in-stock"
    );
    // Seeds stay in stock after a purchase; mushrooms are removed.
    for seq in [10, 11] {
        assert_eq!(
            send(&mut game, &id, seq, buy("seed:strong"))["accepted"],
            true
        );
    }
    assert_eq!(game.players[&id].rings, 0);
    assert_eq!(
        send(&mut game, &id, 12, buy("sa2-fruit:21"))["code"],
        "insufficient-rings"
    );
    let sell = |item: &str| json!({"kind":"sell","itemId":item});
    let r = send(&mut game, &id, 13, sell("seed:strong"));
    assert_eq!(r["effects"]["rings"], 150, "{r}");
    player(&mut game, &id).rings = 300;
    assert_eq!(
        send(&mut game, &id, 14, buy("sa2-fruit:21"))["accepted"],
        true
    );
    assert!(
        !game.players[&id]
            .care
            .market
            .stock
            .contains(&"sa2-fruit:21".to_owned())
    );
    assert_eq!(
        send(&mut game, &id, 15, sell("sa2-fruit:21"))["effects"]["rings"],
        50
    );
    assert_eq!(
        send(&mut game, &id, 16, sell("sa2-fruit:21"))["code"],
        "item-not-owned"
    );
    // Restock is due 10,800 frames later at the earliest (timer draw 0).
    game.time += 10_800. * FRAME_MS;
    assert_eq!(
        send(&mut game, &id, 17, market)["effects"]["restocked"],
        true
    );
}

#[test]
fn planted_seed_sprouts_when_watered_grows_and_bears_its_native_fruit() {
    let (mut game, id) = fixture();
    player(&mut game, &id)
        .care
        .inventory
        .insert("seed:hero".into(), 2);
    let plant = json!({"kind":"plant","itemId":"seed:hero","plotId":"garden-1"});
    go(&mut game, &id, "garden-2");
    assert_eq!(
        send(&mut game, &id, 10, plant.clone())["code"],
        "out-of-range"
    );
    go(&mut game, &id, "garden-1");
    let r = send(&mut game, &id, 11, plant.clone());
    assert_eq!(r["effects"]["kind"], 3, "{r}");
    assert_eq!(send(&mut game, &id, 12, plant)["code"], "plot-occupied");
    let water = json!({"kind":"water","plotId":"garden-1"});
    assert_eq!(send(&mut game, &id, 13, water.clone())["accepted"], true);
    // Sprouting 380 frames, then young; keep it watered to grow at 5e-5/frame.
    let mut seq = 14;
    // Stop once adult: watering an adult tree ripens its fruit (+0.6 each).
    while game.players[&id].garden.trees["garden-1"].phase != garden::Phase::Adult {
        game.time += 2_000. * FRAME_MS;
        assert_eq!(send(&mut game, &id, seq, water.clone())["accepted"], true);
        seq += 1;
        assert!(seq < 30);
    }
    let tree = &game.players[&id].garden.trees["garden-1"];
    assert_eq!(tree.phase, garden::Phase::Adult, "{tree:?}");
    // Life starts at 6,000 and drains for the frames past adulthood.
    assert!(tree.life <= 6_000 && tree.life > 5_900, "{}", tree.life);
    assert_eq!(tree.fruit, 5);
    let harvest = json!({"kind":"harvest","plotId":"garden-1"});
    assert_eq!(
        send(&mut game, &id, 40, harvest.clone())["code"],
        "no-fruit"
    );
    game.time += FRUIT_RIPE_MS;
    // Shake counters start at 11 + rand * 14 = 11 with rand 0.
    let mut dropped = 0;
    for n in 0..11 {
        let r = send(&mut game, &id, 41 + n, harvest.clone());
        assert_eq!(r["accepted"], true, "{r}");
        dropped += r["effects"]["dropped"].as_u64().unwrap();
    }
    assert_eq!(dropped, 3);
    assert_eq!(game.players[&id].care.inventory["sa2-fruit:5"], 3);
    assert_eq!(send(&mut game, &id, 60, harvest)["code"], "no-fruit");
}

#[test]
fn permanent_garden_trees_bear_the_gardens_own_fruit() {
    let (mut game, id) = fixture();
    go(&mut game, &id, "dark-grove-2");
    let harvest = json!({"kind":"harvest","plotId":"dark-grove-2"});
    assert_eq!(
        send(&mut game, &id, 70, harvest.clone())["code"],
        "no-fruit"
    );
    game.time += FRUIT_RIPE_MS;
    let mut last = Value::Null;
    for n in 0..11 {
        last = send(&mut game, &id, 71 + n, harvest.clone());
    }
    assert_eq!(last["accepted"], true, "{last}");
    assert_eq!(game.players[&id].care.inventory["sa2-fruit:2"], 3);
    let grove = &game.players[&id].garden.groves["dark-grove-2"];
    assert_eq!(grove.kind, 8);
    assert_eq!(grove.harvested, 3);
}

#[test]
fn fruit_and_tree_actions_fail_closed_without_verified_rows() {
    let (mut game, id) = fixture();
    game.care_catalog = CareCatalog::default();
    go(&mut game, &id, "garden-grove-1");
    game.time += FRUIT_RIPE_MS;
    assert_eq!(
        send(
            &mut game,
            &id,
            90,
            json!({"kind":"harvest","plotId":"garden-grove-1"})
        )["code"],
        "native-data-unavailable"
    );
}

#[test]
fn capture_follows_the_native_part_and_flag_rules() {
    let (mut game, id) = fixture();
    go(&mut game, &id, "garden-1");
    {
        let p = player(&mut game, &id);
        for item in ["sa2:0", "sa2:3", "sa2:15", "sa2:17", "sa2:22"] {
            p.care.inventory.insert(item.into(), 1);
        }
    }
    // rand 0: every 50% take and every 20% clear happens.
    let r = send(
        &mut game,
        &id,
        100,
        json!({"kind":"consume","itemId":"sa2:0"}),
    );
    assert_eq!(r["effects"]["parts"], json!({"ears":0,"tail":0}), "{r}");
    assert_eq!(r["effects"]["historyBit"], true);
    // rand 0.9: no take, no clear; Bat sets flag D6 and fails the 50% history roll.
    game.time += 1500.;
    script(&[0.9]);
    let r = send(
        &mut game,
        &id,
        101,
        json!({"kind":"consume","itemId":"sa2:17"}),
    );
    assert_eq!(r["effects"]["parts"], json!({}));
    assert_eq!(r["effects"]["flagD6"], true);
    assert_eq!(r["effects"]["historyBit"], false);
    assert_eq!(game.players[&id].care.evidence.species_history, 1);
    // Half Fish (empty lists) with rand 0 clears every shown kind and sets D3.
    game.time += 1500.;
    script(&[0.]);
    let r = send(
        &mut game,
        &id,
        102,
        json!({"kind":"consume","itemId":"sa2:15"}),
    );
    assert_eq!(r["effects"]["parts"], json!({"ears":null,"tail":null}));
    assert_eq!(r["effects"]["flagD3"], true);
    assert_eq!(r["effects"]["flagD6"], false, "legs cleared by a non-Bat");
    // Synthetic Rabbit list is unresolved: no part change at all.
    game.time += 1500.;
    let r = send(
        &mut game,
        &id,
        103,
        json!({"kind":"consume","itemId":"sa2:3"}),
    );
    assert_eq!(r["effects"]["partsApplied"], false);
    // Drives never change parts or species history.
    game.time += 1500.;
    let history = game.players[&id].care.evidence.species_history;
    let r = send(
        &mut game,
        &id,
        104,
        json!({"kind":"consume","itemId":"sa2:22"}),
    );
    assert!(r["effects"].get("parts").is_none());
    assert_eq!(game.players[&id].care.evidence.species_history, history);
    // Rebirth: history clears; each part is lost when rand < 0.3.
    let p = player(&mut game, &id);
    p.care.animal_parts.insert("ears".into(), 0);
    p.care.animal_parts.insert("tail".into(), 0);
    script(&[0.5, 0.1]);
    care::rebirth(p);
    assert_eq!(p.care.evidence.species_history, 0);
    assert_eq!(p.care.animal_parts.len(), 1);
    assert!(p.care.animal_parts.contains_key("ears"));
    care::reset_animal_parts(p);
    assert!(p.care.animal_parts.is_empty());
}

#[test]
fn version_one_care_saves_migrate_and_garden_round_trips_through_profile() {
    let mut legacy = serde_json::to_value(CareState::default()).unwrap();
    legacy["version"] = json!(1);
    for key in ["animalParts", "flagD3", "flagD6"] {
        legacy.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(CareState::restore(Some(&legacy)).unwrap().version, 2);
    let (mut game, id) = fixture();
    go(&mut game, &id, "garden-1");
    player(&mut game, &id)
        .care
        .inventory
        .insert("seed:tasty".into(), 1);
    send(
        &mut game,
        &id,
        110,
        json!({"kind":"plant","itemId":"seed:tasty","plotId":"garden-1"}),
    );
    let profile = game.players[&id].profile();
    assert_eq!(
        GardenState::restore(profile.get("garden")).unwrap(),
        game.players[&id].garden
    );
    assert_eq!(profile["garden"]["trees"]["garden-1"]["kind"], 2);
}

#[test]
fn behaviour_hook_shakes_and_eats_from_a_nearby_tree() {
    let (mut game, id) = fixture();
    go(&mut game, &id, "garden-1");
    assert_eq!(
        care::eat_garden_fruit(&mut game, &id, "garden-grove-1"),
        Err("out-of-range")
    );
    // Groves were planted when the Chao connected.
    game.time += FRUIT_RIPE_MS;
    let (place, _, _) = garden::nearest_fruit(&game.players[&id], game.time).unwrap();
    go(&mut game, &id, &place);
    let mut eaten = 0;
    for _ in 0..11 {
        let r = care::eat_garden_fruit(&mut game, &id, &place).unwrap();
        eaten += u32::from(r["eaten"] == true);
        assert_eq!(
            care::eat_garden_fruit(&mut game, &id, &place),
            Err("cooldown")
        );
        game.time += 1500.;
    }
    assert_eq!(eaten, 1);
    assert_eq!(game.players[&id].care.inventory["sa2-fruit:0"], 2);
}

#[test]
fn black_market_eggs_hats_and_themes_follow_the_native_rules() {
    let (mut game, id) = fixture();
    {
        let p = player(&mut game, &id);
        p.zone = "hub".into();
        (p.x, p.z, p.rings) = (-4., 8., 50_000);
        p.care.market.stock = vec![
            "egg:2".into(),
            "egg:36".into(),
            "hat:3".into(),
            "theme:omochao".into(),
        ];
        p.care.market.restock_at = Some(u64::MAX);
    }
    let buy = |item: &str| json!({"kind":"purchase","itemId":item});
    assert_eq!(send(&mut game, &id, 1, buy("egg:2"))["accepted"], true);
    assert_eq!(
        send(&mut game, &id, 2, buy("egg:2"))["code"],
        "not-in-stock",
        "eggs are removed"
    );
    assert_eq!(send(&mut game, &id, 3, buy("egg:36"))["accepted"], true);
    assert_eq!(game.players[&id].rings, 50_000 - 400 - 5_000);
    assert_eq!(
        send(&mut game, &id, 4, json!({"kind":"sell","itemId":"egg:2"}))["code"],
        "not-sellable"
    );
    let theme = send(&mut game, &id, 5, buy("theme:omochao"));
    assert_eq!(theme["accepted"], true, "{theme}");
    assert!(game.players[&id].care.market.themes[0]);
    assert!(
        !game.players[&id]
            .care
            .inventory
            .contains_key("theme:omochao")
    );
    // Eggs go down in a garden, into the nest, with the native egg-colour genes.
    let place = |item: &str| json!({"kind":"place-egg","itemId":item});
    assert_eq!(
        send(&mut game, &id, 6, place("egg:2"))["code"],
        "out-of-range"
    );
    go(&mut game, &id, "garden-1");
    assert_eq!(send(&mut game, &id, 7, place("egg:2"))["accepted"], true);
    assert_eq!(send(&mut game, &id, 8, place("egg:36"))["accepted"], true);
    let nest = &game.players[&id].life.nest;
    let looks: Vec<_> = nest.iter().map(|e| e.expressed.look).collect();
    assert_eq!(
        (looks[0].color, looks[0].shiny, looks[0].monotone),
        (2, false, true)
    );
    assert_eq!(
        (looks[1].color, looks[1].shiny, looks[1].monotone),
        (9, true, true)
    );
    assert_eq!(nest[1].gene.color, [9, 9]);
    // Hats: the Chao only puts one on with MinimalFlag bit 16 (Skeleton Dog).
    {
        let p = player(&mut game, &id);
        p.zone = "hub".into();
        (p.x, p.z) = (-4., 8.);
    }
    assert_eq!(send(&mut game, &id, 9, buy("hat:3"))["accepted"], true);
    let give = json!({"kind":"give-hat","itemId":"hat:3"});
    game.time += 1500.;
    assert_eq!(
        send(&mut game, &id, 10, give.clone())["code"],
        "chao-wont-wear"
    );
    player(&mut game, &id).care.evidence.species_history |= 1 << 16;
    assert_eq!(send(&mut game, &id, 11, give.clone())["accepted"], true);
    assert_eq!(game.players[&id].care.hat, Some(3));
    assert_eq!(game.players[&id].public()["hat"], 3);
    assert_eq!(
        send(&mut game, &id, 12, json!({"kind":"sell","itemId":"hat:3"}))["code"],
        "item-not-owned"
    );
}

#[test]
fn restock_reads_the_native_emblem_count() {
    let (mut game, id) = fixture();
    {
        let p = player(&mut game, &id);
        p.zone = "hub".into();
        (p.x, p.z) = (-4., 8.);
        // Prison Lane (slot 9) all five missions: 5 emblems, the White Egg row.
        for m in 0..5 {
            assert!(crate::emblems::record_mission(&mut p.care.emblems, 9, m, 5));
        }
    }
    script(&[0., 0., 0., 0., 0., 0., 0.5, 0., 0., 0., 0.5, 0., 0., 0.]);
    let r = send(&mut game, &id, 1, json!({"kind":"market"}));
    assert_eq!(r["accepted"], true, "{r}");
    let p = &game.players[&id];
    assert_eq!(p.care.emblems.count(), 5);
    assert!(p.care.market.garden_id.is_some());
    assert_eq!(
        p.care.market.stock[0], "egg:2",
        "White Egg is the only row at 5 emblems"
    );
    assert_eq!(care::public(&game, &id)["emblems"]["count"], 5);
}
