use chao_game_server::game::Game;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture(owner: bool) -> (Game, String) {
    let mut game = Game::new(vec![], BTreeMap::new(), owner, 1000.).unwrap();
    let session = game.create_session(&json!({"name":"Mech test"})).unwrap();
    let id = session["player"]["id"].as_str().unwrap().to_owned();
    game.connect(&id);
    (game, id)
}
fn action(game: &mut Game, id: &str, name: &str, mut fields: Value, elapsed: f64) {
    game.time += elapsed;
    fields["action"] = json!(name);
    game.action(id, &fields);
}
fn arena() -> (Game, String) {
    let (mut game, id) = fixture(true);
    action(&mut game, &id, "mount-dark-walker", json!({}), 100.);
    action(&mut game, &id, "mission", json!({}), 100.);
    let player = game.players.get_mut(&id).unwrap();
    player.x = 0.;
    player.y = 0.;
    player.z = 0.;
    player.ground_y = 0.;
    player.jump_offset = 0.;
    player.facing = 0.;
    for enemy in &mut game.mission.as_mut().unwrap().enemies {
        enemy.x = 20.;
        enemy.z = 20.;
    }
    (game, id)
}

#[test]
fn cannon_server_aim_fixed_damage_shared_cooldown_and_real_kill_rewards() {
    let (mut game, id) = arena();
    let enemies = &mut game.mission.as_mut().unwrap().enemies;
    for (index, x, z) in [(0, 0., 5.), (1, 0., -1.), (2, 0., 10.), (3, 5., 1.)] {
        enemies[index].x = x;
        enemies[index].z = z;
    }
    let target_id = enemies[0].id.clone();
    let far_id = enemies[2].id.clone();
    action(
        &mut game,
        &id,
        "mech-fire",
        json!({"targetId":far_id,"damage":999999,"range":999}),
        100.,
    );
    let enemies = &game.mission.as_ref().unwrap().enemies;
    assert_eq!(enemies[0].hp, 40.);
    for enemy in &enemies[1..] {
        assert_eq!(enemy.hp, 100.);
    }
    let shot = &game
        .events
        .iter()
        .find(|event| event.message["kind"] == "mech-shot")
        .unwrap()
        .message;
    assert_eq!(shot["targetId"], target_id);
    assert_eq!(shot["hit"], true);
    assert_eq!(shot["hitZ"], 5.);
    assert_eq!(shot["y"], 0.65);
    action(&mut game, &id, "mech-fire", json!({}), 0.);
    action(&mut game, &id, "attack", json!({}), 100.);
    assert_eq!(game.mission.as_ref().unwrap().enemies[0].hp, 40.);
    action(&mut game, &id, "attack", json!({}), 250.);
    assert_eq!(game.mission.as_ref().unwrap().enemies[0].hp, 0.);
    assert_eq!(game.players[&id].rings, 25);
    assert_eq!(
        game.events
            .iter()
            .filter(|event| event.message["kind"] == "mech-shot")
            .count(),
        2
    );
}

#[test]
fn cannon_three_dimensional_range_and_zone_protect_other_targets() {
    let (mut game, id) = arena();
    game.mission.as_mut().unwrap().enemies[0].x = 0.;
    game.mission.as_mut().unwrap().enemies[0].z = 1.;
    game.players.get_mut(&id).unwrap().y = 20.;
    action(&mut game, &id, "mech-fire", json!({}), 100.);
    assert_eq!(game.mission.as_ref().unwrap().enemies[0].hp, 100.);
    assert_eq!(game.events.last().unwrap().message["hit"], false);
    action(&mut game, &id, "hub", json!({}), 100.);
    action(&mut game, &id, "mech-fire", json!({}), 400.);
    let shot = &game.events.last().unwrap().message;
    assert_eq!(shot["kind"], "mech-shot");
    assert_eq!(shot["hit"], false);
    assert_eq!(shot["targetId"], Value::Null);
    assert_eq!(game.mission.as_ref().unwrap().enemies[0].hp, 100.);
}

#[test]
fn cannon_mission_clear_rewards_real_kills_and_each_member_once() {
    let (mut game, id) = arena();
    let guest = game.create_session(&json!({"name":"Coop guest"})).unwrap();
    let guest_id = guest["player"]["id"].as_str().unwrap().to_owned();
    game.connect(&guest_id);
    action(&mut game, &guest_id, "mission", json!({}), 100.);
    for (index, enemy) in game
        .mission
        .as_mut()
        .unwrap()
        .enemies
        .iter_mut()
        .enumerate()
    {
        enemy.x = 0.;
        enemy.z = 3. + index as f64 * 0.1;
    }
    for _ in 0..10 {
        action(&mut game, &id, "mech-fire", json!({}), 400.);
    }
    assert!(game.mission.as_ref().unwrap().completed);
    assert_eq!(game.players[&id].rings, 95);
    assert_eq!(game.players[&guest_id].rings, 70);
    action(&mut game, &id, "mech-fire", json!({}), 400.);
    assert_eq!(game.players[&id].rings, 95);
}

#[test]
fn boost_has_finite_fuel_height_landing_and_ground_only_recovery() {
    let (mut game, id) = fixture(true);
    action(&mut game, &id, "mount-dark-walker", json!({}), 100.);
    let stats = game.players[&id].stats.clone();
    let mut highest: f64 = 0.;
    let mut boost_ticks = 0;
    for _ in 0..150 {
        game.set_input(
            &id,
            &json!({"boost":true,"boostEnergy":999999,"y":9999,"vy":9999,"mech":{"boosting":true}}),
        );
        game.tick(0.05);
        let player = &game.players[&id];
        highest = highest.max(player.jump_offset);
        if player.boosting {
            boost_ticks += 1;
        }
        assert!((0. ..=100.).contains(&player.boost_energy));
    }
    assert!(highest > 2.9 && highest <= 3.);
    assert!(boost_ticks <= 50);
    let player = &game.players[&id];
    assert_eq!(player.boost_energy, 0.);
    assert_eq!(player.jump_offset, 0.);
    assert!(!player.boosting);
    assert_eq!(player.public()["mech"]["grounded"], true);
    assert_eq!(player.stats, stats);
    game.set_input(&id, &json!({"boost":false}));
    for _ in 0..40 {
        game.tick(0.05);
    }
    assert_eq!(game.players[&id].boost_energy, 50.);
}

#[test]
fn stale_boost_falls_mount_toggles_do_not_refill_and_air_does_not_recharge() {
    let (mut game, id) = fixture(true);
    action(&mut game, &id, "mount-dark-walker", json!({}), 100.);
    for _ in 0..10 {
        game.set_input(&id, &json!({"boost":true}));
        game.tick(0.05);
    }
    assert_eq!(game.players[&id].boost_energy, 80.);
    assert!(game.players[&id].jump_offset > 0.);
    action(&mut game, &id, "dismount-vehicle", json!({}), 100.);
    action(&mut game, &id, "mount-dark-walker", json!({}), 100.);
    assert_eq!(game.players[&id].boost_energy, 80.);
    game.set_input(&id, &json!({"boost":false}));
    for _ in 0..3 {
        game.tick(0.05);
    }
    assert_eq!(game.players[&id].boost_energy, 80.);
    assert!(game.players[&id].jump_offset > 0.);
    game.set_input(&id, &json!({"boost":true}));
    for _ in 0..30 {
        game.tick(0.05);
    }
    assert!(!game.players[&id].boosting);
    assert_eq!(game.players[&id].jump_offset, 0.);
    assert!(game.export_profiles()[0].get("boostEnergy").is_none());
}

#[test]
fn unmounted_and_regular_players_cannot_forge_weapon_mount_thrust_or_fuel() {
    for owner in [true, false] {
        let (mut game, id) = fixture(owner);
        action(
            &mut game,
            &id,
            "mech-fire",
            json!({"vehicle":"dark-chao-walker","admin":true,"damage":999999}),
            100.,
        );
        game.set_input(
            &id,
            &json!({"boost":true,"vehicle":"dark-chao-walker","admin":true,"boostEnergy":999999}),
        );
        game.tick(0.05);
        let player = &game.players[&id];
        assert_eq!(player.jump_offset, 0.);
        assert!(!player.boosting);
        assert_eq!(player.public()["mech"], Value::Null);
        assert!(
            !game
                .events
                .iter()
                .any(|event| event.message["kind"] == "mech-shot")
        );
    }
}
