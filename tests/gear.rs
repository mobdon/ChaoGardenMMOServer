use chao_game_server::game::Game;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture(owner: bool) -> (Game, String, String) {
    let mut game = Game::new(vec![], BTreeMap::new(), owner, 1000.).unwrap();
    let session = game.create_session(&json!({"name":"Gear test", "admin":true, "gear":{"footwear":"shadow-air-shoes","hands":"knuckles-hammer-gloves"}})).unwrap();
    let id = session["player"]["id"].as_str().unwrap().to_owned();
    let token = session["token"].as_str().unwrap().to_owned();
    game.connect(&id);
    (game, id, token)
}
fn action(game: &mut Game, id: &str, name: &str, mut fields: Value) {
    game.time += 100.;
    fields["action"] = json!(name);
    game.action(id, &fields);
}

#[test]
fn owner_gear_slots_persist_without_changing_chao_stats_or_active_mech_state() {
    let (mut game, id, token) = fixture(true);
    assert_eq!(
        game.players[&id].public()["gear"],
        json!({"footwear":null,"hands":null})
    );
    action(&mut game, &id, "mount-dark-walker", json!({}));
    game.set_input(&id, &json!({"boost":true}));
    game.tick(0.05);
    let before = game.players[&id].public();
    action(
        &mut game,
        &id,
        "equip-owner-gear",
        json!({"item":"shadow-air-shoes"}),
    );
    action(
        &mut game,
        &id,
        "equip-owner-gear",
        json!({"item":"knuckles-hammer-gloves"}),
    );
    let after = game.players[&id].public();
    assert_eq!(
        after["gear"],
        json!({"footwear":"shadow-air-shoes","hands":"knuckles-hammer-gloves"})
    );
    action(
        &mut game,
        &id,
        "equip-owner-gear",
        json!({"item":"sonic-light-shoes"}),
    );
    assert_eq!(
        game.players[&id].public()["gear"]["footwear"],
        "sonic-light-shoes"
    );
    action(
        &mut game,
        &id,
        "equip-owner-gear",
        json!({"item":"shadow-air-shoes"}),
    );
    for key in [
        "stats",
        "rings",
        "form",
        "alignment",
        "vehicle",
        "mech",
        "x",
        "y",
        "z",
    ] {
        assert_eq!(after[key], before[key]);
    }
    let restored = Game::new(game.export_profiles(), BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(
        restored.player_for_token(&token).unwrap().gear,
        game.players[&id].gear
    );
    action(
        &mut game,
        &id,
        "unequip-owner-gear",
        json!({"slot":"footwear"}),
    );
    assert_eq!(
        game.players[&id].public()["gear"],
        json!({"footwear":null,"hands":"knuckles-hammer-gloves"})
    );
}

#[test]
fn gear_rejects_forged_owner_unknown_items_slots_and_corrupt_persisted_choices() {
    let (mut regular, id, _) = fixture(false);
    action(
        &mut regular,
        &id,
        "equip-owner-gear",
        json!({"item":"shadow-air-shoes","admin":true}),
    );
    action(
        &mut regular,
        &id,
        "unequip-owner-gear",
        json!({"slot":"admin","admin":true}),
    );
    assert_eq!(
        regular.players[&id].public()["gear"],
        json!({"footwear":null,"hands":null})
    );
    let mut profiles = regular.export_profiles();
    profiles[0]["gear"] = json!({"footwear":"shadow-air-shoes","hands":"knuckles-hammer-gloves"});
    let restored = Game::new(profiles, BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(
        restored.players[&id].public()["gear"],
        json!({"footwear":null,"hands":null})
    );
    let (mut owner, id, _) = fixture(true);
    for item in [
        json!("unknown"),
        json!("__proto__"),
        json!("footwear"),
        Value::Null,
    ] {
        action(&mut owner, &id, "equip-owner-gear", json!({"item":item}));
    }
    for slot in [
        json!("admin"),
        json!("__proto__"),
        json!("unknown"),
        Value::Null,
    ] {
        action(&mut owner, &id, "unequip-owner-gear", json!({"slot":slot}));
    }
    assert_eq!(
        owner.players[&id].public()["gear"],
        json!({"footwear":null,"hands":null})
    );
    let mut profiles = owner.export_profiles();
    profiles[0]["gear"] =
        json!({"footwear":"knuckles-hammer-gloves","hands":"shadow-air-shoes","admin":true});
    let restored = Game::new(profiles, BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(
        restored.players[&id].public()["gear"],
        json!({"footwear":null,"hands":null})
    );
}

#[test]
fn metal_sonic_chao_look_is_owner_only_and_persists() {
    let (mut owner, id, token) = fixture(true);
    action(
        &mut owner,
        &id,
        "equip-owner-gear",
        json!({"item":"metal-sonic-chao"}),
    );
    assert_eq!(
        owner.players[&id].public()["gear"]["look"],
        "metal-sonic-chao"
    );
    let restored = Game::new(owner.export_profiles(), BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(
        restored
            .player_for_token(&token)
            .unwrap()
            .gear
            .look
            .as_deref(),
        Some("metal-sonic-chao")
    );
    action(
        &mut owner,
        &id,
        "unequip-owner-gear",
        json!({"slot":"look"}),
    );
    assert_eq!(
        owner.players[&id].public()["gear"],
        json!({"footwear":null,"hands":null})
    );

    let (mut regular, id, _) = fixture(false);
    action(
        &mut regular,
        &id,
        "equip-owner-gear",
        json!({"item":"metal-sonic-chao","admin":true}),
    );
    assert_eq!(
        regular.players[&id].public()["gear"],
        json!({"footwear":null,"hands":null})
    );
    let mut profiles = regular.export_profiles();
    profiles[0]["gear"] = json!({"look":"metal-sonic-chao"});
    let restored = Game::new(profiles, BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(
        restored.players[&id].public()["gear"],
        json!({"footwear":null,"hands":null})
    );
}

#[test]
fn metal_sonic_chao_look_runs_at_his_own_top_speed() {
    let travel = |metal: bool| {
        let (mut game, id, _) = fixture(true);
        if metal {
            action(
                &mut game,
                &id,
                "equip-owner-gear",
                json!({"item":"metal-sonic-chao"}),
            );
        }
        let start = game.players[&id].x;
        for _ in 0..20 {
            game.set_input(&id, &json!({"dx":1,"dz":0}));
            game.tick(0.05);
        }
        game.players[&id].x - start
    };
    let (chao, metal) = (travel(false), travel(true));
    assert!(chao > 0.1 && metal > chao * 3., "chao {chao} metal {metal}");
}

#[test]
fn metal_sonic_chao_look_jumps_like_metal_sonic() {
    let peak = |metal: bool| {
        let (mut game, id, _) = fixture(true);
        if metal {
            action(
                &mut game,
                &id,
                "equip-owner-gear",
                json!({"item":"metal-sonic-chao"}),
            );
        }
        game.set_input(&id, &json!({"dx":0,"dz":0,"jump":true}));
        let mut top: f64 = 0.;
        for _ in 0..40 {
            game.tick(0.02);
            top = top.max(game.players[&id].jump_offset);
        }
        top
    };
    let (chao, metal) = (peak(false), peak(true));
    assert!(chao > 0. && metal > chao * 3., "chao {chao} metal {metal}");
}
