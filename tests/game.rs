use chao_game_server::game::{ALL_STATS, Game, MAIN_STATS, RACE_CHECKPOINTS, Terrain, hash_token};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture(owner: bool) -> (Game, String, String) {
    fixture_terrain(owner, BTreeMap::new())
}
fn fixture_terrain(owner: bool, terrains: BTreeMap<String, Terrain>) -> (Game, String, String) {
    let mut game = Game::new(vec![], terrains, owner, 1000.).unwrap();
    let session = game.create_session(&json!({"name": "Tester"})).unwrap();
    let id = session["player"]["id"].as_str().unwrap().to_owned();
    let token = session["token"].as_str().unwrap().to_owned();
    game.connect(&id);
    (game, id, token)
}
fn advance(game: &mut Game, ms: usize) {
    for _ in 0..ms.div_ceil(50) {
        game.tick(0.05);
    }
}
fn act(game: &mut Game, id: &str, action: &str, mut fields: Value) {
    advance(game, 100);
    fields["action"] = json!(action);
    game.action(id, &fields);
}
fn position(game: &mut Game, id: &str, x: f64, z: f64) {
    let p = game.players.get_mut(id).unwrap();
    p.x = x;
    p.z = z;
}
fn terrain(rows: &[&[Option<f64>]], min_z: f64) -> Terrain {
    Terrain {
        min_x: 0.,
        min_z,
        step: 1.,
        width: rows[0].len(),
        depth: rows.len(),
        heights: rows.iter().flat_map(|row| row.iter().copied()).collect(),
    }
}

#[test]
fn bootstrap_is_explicit_and_only_first_profile_owns_perfect_stats() {
    let (mut g, id, _) = fixture(true);
    let p = &g.players[&id];
    assert!(p.admin);
    for stat in MAIN_STATS {
        assert_eq!(p.stats[stat].grade, 5);
    }
    for stat in ALL_STATS {
        assert_eq!(p.stats[stat].level, 99);
    }
    assert_eq!(p.stats["luck"].grade, 255);
    assert_eq!(p.stats["intelligence"].grade, 255);
    let second = g
        .create_session(&json!({"owner":true,"admin":true,"stats":{"run":{"grade":5,"level":99}}}))
        .unwrap();
    assert_eq!(second["player"]["admin"], false);
    assert_eq!(second["player"]["stats"]["run"]["level"], 1);
    let (regular, regular_id, _) = fixture(false);
    assert!(!regular.players[&regular_id].admin);
}

#[test]
fn session_resumes_without_renaming_and_hashes_match_legacy_node_sha256() {
    let (mut g, id, token) = fixture(true);
    assert_eq!(
        hash_token("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let resume = g
        .create_session(&json!({"token":token,"name":"Ignored"}))
        .unwrap();
    assert_eq!(resume["player"]["id"], id);
    assert_eq!(resume["player"]["name"], "Tester");
    assert!(
        !serde_json::to_string(&g.export_profiles())
            .unwrap()
            .contains(&token)
    );
    assert!(!g.snapshot(&id).to_string().contains(&token));
    assert!(
        !g.snapshot(&id)
            .to_string()
            .contains(&g.players[&id].token_hash)
    );
}

#[test]
fn resume_only_rejects_missing_or_unknown_tokens_without_creating_a_profile() {
    let (mut g, id, token) = fixture(false);
    let count = g.players.len();
    for invalid in [
        Value::Null,
        json!(""),
        json!("unknown"),
        json!("x".repeat(257)),
    ] {
        assert!(
            g.create_session(&json!({"token":invalid,"resumeOnly":true}))
                .is_err()
        );
        assert_eq!(g.players.len(), count);
    }
    let resumed = g
        .create_session(&json!({"token":token,"resumeOnly":true}))
        .unwrap();
    assert_eq!(resumed["player"]["id"], id);
    assert!(g.create_session(&json!({"token":"legacy-unknown"})).is_ok());
}

#[test]
fn legacy_profile_shape_preserves_currency_progression_owner_and_equipment() {
    let (mut g, id, token) = fixture(true);
    let p = g.players.get_mut(&id).unwrap();
    p.rings = 123;
    p.missions_completed = 2;
    p.owned_cosmetics = vec!["eggshell".into()];
    p.equipped = Some("eggshell".into());
    let restored = Game::new(g.export_profiles(), BTreeMap::new(), true, 1000.).unwrap();
    let p = restored.player_for_token(&token).unwrap();
    assert!(p.admin);
    assert_eq!(p.rings, 123);
    assert_eq!(p.missions_completed, 2);
    assert_eq!(p.equipped.as_deref(), Some("eggshell"));
    assert_eq!(p.zone, "hub");
}

#[test]
fn restored_alignment_sets_dark_and_hero_forms() {
    let (mut g, id, token) = fixture(false);
    g.players.get_mut(&id).unwrap().alignment = -80.;
    let restored = Game::new(g.export_profiles(), BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(restored.player_for_token(&token).unwrap().form, "dark");
}

#[test]
fn walker_mount_is_admin_owned_persistent_and_does_not_change_stats_or_movement() {
    let (mut g, id, token) = fixture(true);
    let before = g.players[&id].public();
    assert_eq!(before["vehicle"], Value::Null);
    act(
        &mut g,
        &id,
        "mount-dark-walker",
        json!({"vehicle":"other", "run":9999}),
    );
    let mounted = g.players[&id].public();
    assert_eq!(mounted["vehicle"], "dark-chao-walker");
    for key in ["stats", "rings", "form", "alignment", "x", "y", "z"] {
        assert_eq!(mounted[key], before[key]);
    }
    let mut restored = Game::new(g.export_profiles(), BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(
        restored
            .player_for_token(&token)
            .unwrap()
            .vehicle
            .as_deref(),
        Some("dark-chao-walker")
    );
    restored.connect(&id);
    act(&mut restored, &id, "dismount-vehicle", json!({}));
    assert_eq!(restored.players[&id].vehicle, None);
    let (mut regular, regular_id, _) = fixture(false);
    act(
        &mut regular,
        &regular_id,
        "mount-dark-walker",
        json!({"admin":true}),
    );
    assert_eq!(regular.players[&regular_id].vehicle, None);
    let mut profiles = regular.export_profiles();
    profiles[0]["vehicle"] = json!("dark-chao-walker");
    let clean = Game::new(profiles, BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(clean.players[&regular_id].vehicle, None);
    let mut profiles = g.export_profiles();
    profiles[0]["vehicle"] = json!("unknown-vehicle");
    let clean = Game::new(profiles, BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(clean.players[&id].vehicle, None);
}

#[test]
fn forged_coordinates_admin_rewards_stats_and_jump_state_are_ignored() {
    let (mut g, id, _) = fixture(false);
    g.set_input(&id, &json!({"dx":0,"dz":0,"x":9999,"z":9999,"y":9999,"vy":9999,"jumpOffset":9999,"admin":true,"rings":999999}));
    for action in [
        "reward",
        "set-stats",
        "mission-complete",
        "race-complete",
        "admin-dark-chaos",
    ] {
        act(
            &mut g,
            &id,
            action,
            json!({"admin":true,"rings":999999,"levels":99}),
        );
    }
    let p = &g.players[&id];
    assert!(!p.admin);
    assert_eq!(p.rings, 20);
    assert_eq!(p.stats["run"].level, 1);
    assert_eq!((p.x, p.z, p.y), (0., 6., 0.));
    assert_eq!(p.form, "normal");
}

#[test]
fn normalized_input_has_native_speed_and_stale_input_stops() {
    for owner in [false, true] {
        let (mut g, id, _) = fixture(owner);
        position(&mut g, &id, 0., 0.);
        for _ in 0..20 {
            g.set_input(&id, &json!({"dx":99999,"dz":99999}));
            g.tick(0.05);
        }
        let p = &g.players[&id];
        let traveled = p.x.hypot(p.z);
        assert!(traveled > if owner { 1.98 } else { 1.39 });
        assert!(traveled < if owner { 2.01 } else { 1.42 });
        advance(&mut g, 1000);
        let stopped = g.players[&id].x;
        advance(&mut g, 1000);
        assert_eq!(g.players[&id].x, stopped);
    }
}

#[test]
fn bounds_and_invalid_numeric_types_do_not_teleport() {
    let (mut g, id, _) = fixture(false);
    position(&mut g, &id, 21.99, 21.99);
    g.set_input(&id, &json!({"dx":1,"dz":1}));
    g.tick(0.1);
    assert_eq!((g.players[&id].x, g.players[&id].z), (22., 22.));
    g.set_input(&id, &json!({"dx":"Infinity","dz":null,"dt":999}));
    g.tick(0.05);
    assert_eq!(g.players[&id].x, 22.);
}

#[test]
fn jump_has_real_quarter_unit_apex_and_lands() {
    let (mut g, id, _) = fixture(false);
    let mut highest: f64 = 0.;
    for i in 0..30 {
        g.set_input(&id, &json!({"dx":0,"dz":0,"jump":i==0}));
        g.tick(0.05);
        highest = highest.max(g.players[&id].jump_offset);
    }
    assert!((0.235..=0.245).contains(&highest));
    assert_eq!(g.players[&id].jump_offset, 0.);
}

#[test]
fn ring_pickups_require_proximity_and_respawn_once() {
    let (mut g, id, _) = fixture(false);
    act(&mut g, &id, "garden", json!({}));
    let ring = g.rings[0].clone();
    act(&mut g, &id, "collect", json!({"id":ring.id}));
    assert_eq!(g.players[&id].rings, 20);
    position(&mut g, &id, ring.x, ring.z);
    g.tick(0.05);
    assert_eq!(g.players[&id].rings, 21);
    g.tick(0.05);
    assert_eq!(g.players[&id].rings, 21);
    advance(&mut g, 20100);
    assert_eq!(g.players[&id].rings, 22);
}

#[test]
fn alignment_gates_and_choice_cooldown_are_authoritative() {
    let (mut g, id, _) = fixture(false);
    act(&mut g, &id, "hero", json!({}));
    assert_eq!(g.players[&id].zone, "hub");
    act(&mut g, &id, "align-good", json!({}));
    act(&mut g, &id, "align-good", json!({}));
    assert_eq!(g.players[&id].alignment, 15.);
    for _ in 0..3 {
        advance(&mut g, 1600);
        act(&mut g, &id, "align-good", json!({}));
    }
    assert_eq!(g.players[&id].alignment, 60.);
    assert_eq!(g.players[&id].form, "hero");
    act(&mut g, &id, "hero", json!({}));
    assert_eq!(g.players[&id].zone, "hero");
    for _ in 0..20 {
        advance(&mut g, 1600);
        act(&mut g, &id, "align-dark", json!({}));
    }
    assert_eq!(g.players[&id].alignment, -100.);
    act(&mut g, &id, "dark", json!({}));
    assert_eq!(g.players[&id].zone, "dark");
}

#[test]
fn training_requires_knuckles_or_pond_and_consumes_energy_with_cooldown() {
    let (mut g, id, _) = fixture(false);
    act(&mut g, &id, "train-run", json!({"x":7,"z":2}));
    assert_eq!(g.players[&id].stats["run"].level, 1);
    position(&mut g, &id, 7., 2.);
    act(&mut g, &id, "train-run", json!({}));
    assert_eq!(g.players[&id].stats["run"].level, 3);
    assert!(g.players[&id].energy < 100.);
    act(&mut g, &id, "train-run", json!({}));
    assert_eq!(g.players[&id].stats["run"].level, 3);
    advance(&mut g, 1000);
    act(&mut g, &id, "garden", json!({}));
    act(&mut g, &id, "train-swim", json!({}));
    assert_eq!(g.players[&id].stats["swim"].level, 1);
    position(&mut g, &id, -8., 0.);
    act(&mut g, &id, "train-swim", json!({}));
    assert_eq!(g.players[&id].stats["swim"].level, 3);
}

#[test]
fn legacy_feed_cannot_bypass_canonical_inventory_even_at_the_fruit_grove() {
    let (mut g, id, _) = fixture(false);
    act(&mut g, &id, "feed", json!({}));
    assert_eq!(g.players[&id].rings, 20);
    act(&mut g, &id, "garden", json!({}));
    position(&mut g, &id, -7., -5.);
    g.players.get_mut(&id).unwrap().energy = 10.;
    let stats = g.players[&id].stats.clone();
    let happiness = g.players[&id].happiness;
    act(&mut g, &id, "feed", json!({"price":-999}));
    assert_eq!(g.players[&id].rings, 20);
    assert!(g.players[&id].energy < 11.);
    assert_eq!(g.players[&id].happiness, happiness);
    assert_eq!(g.players[&id].stats, stats);
    assert!(g.players[&id].care.inventory.is_empty());
    assert!(g.players[&id].care.receipts.is_empty());
}

#[test]
fn attack_enforces_three_dimensional_reach_and_cooldown() {
    let (mut g, id, _) = fixture(true);
    act(&mut g, &id, "mission", json!({}));
    advance(&mut g, 600);
    position(&mut g, &id, 0., 0.);
    {
        let e = &mut g.mission.as_mut().unwrap().enemies[0];
        e.x = 0.81;
        e.z = 0.;
    }
    g.action(&id, &json!({"action":"attack","range":99999}));
    assert_eq!(g.mission.as_ref().unwrap().enemies[0].hp, 100.);
    advance(&mut g, 600);
    {
        let e = &mut g.mission.as_mut().unwrap().enemies[0];
        e.x = 0.79;
        e.z = 0.;
    }
    g.players.get_mut(&id).unwrap().y = 5.;
    g.action(&id, &json!({"action":"attack","y":0}));
    assert_eq!(g.mission.as_ref().unwrap().enemies[0].hp, 100.);
    advance(&mut g, 600);
    position(&mut g, &id, 0., 0.);
    {
        let e = &mut g.mission.as_mut().unwrap().enemies[0];
        e.x = 0.79;
        e.z = 0.;
    }
    g.action(&id, &json!({"action":"attack"}));
    assert_eq!(g.mission.as_ref().unwrap().enemies[0].hp, 0.);
    {
        let e = &mut g.mission.as_mut().unwrap().enemies[1];
        e.x = 0.2;
        e.z = 0.;
    }
    act(&mut g, &id, "attack", json!({}));
    assert_eq!(g.mission.as_ref().unwrap().enemies[1].hp, 100.);
}

#[test]
fn cooperative_mission_pays_each_participant_once_then_can_start_new_instance() {
    let (mut g, id, _) = fixture(true);
    let second = g.create_session(&json!({"name":"Friend"})).unwrap();
    let other = second["player"]["id"].as_str().unwrap().to_owned();
    g.connect(&other);
    act(&mut g, &id, "mission", json!({}));
    act(&mut g, &other, "mission", json!({}));
    let instance = g.mission.as_ref().unwrap().id.clone();
    for index in 0..5 {
        advance(&mut g, 600);
        let e = g.mission.as_ref().unwrap().enemies[index].clone();
        position(&mut g, &id, e.x, e.z);
        act(&mut g, &id, "attack", json!({}));
    }
    assert!(g.mission.as_ref().unwrap().completed);
    assert_eq!(g.players[&id].missions_completed, 1);
    assert_eq!(g.players[&other].missions_completed, 1);
    assert!(g.players[&id].rings >= 95);
    let paid = g.players[&id].rings;
    g.finish_mission();
    act(&mut g, &id, "mission-complete", json!({"reward":99999}));
    assert_eq!(g.players[&id].rings, paid);
    act(&mut g, &id, "hub", json!({}));
    act(&mut g, &other, "hub", json!({}));
    act(&mut g, &id, "mission", json!({}));
    assert_ne!(g.mission.as_ref().unwrap().id, instance);
}

#[test]
fn race_requires_ordered_traversal_takes_real_time_and_pays_once() {
    let (mut g, id, _) = fixture(false);
    act(&mut g, &id, "race", json!({}));
    act(
        &mut g,
        &id,
        "race-complete",
        json!({"time":1,"reward":99999}),
    );
    assert_eq!(g.players[&id].races_completed, 0);
    g.tick(0.05);
    assert_eq!(g.players[&id].race.as_ref().unwrap().next_checkpoint, 1);
    for (x, z) in RACE_CHECKPOINTS.into_iter().skip(1) {
        for _ in 0..1000 {
            let p = &g.players[&id];
            let Some(race) = &p.race else {
                break;
            };
            let previous = race.next_checkpoint;
            let dx = x - p.x;
            let dz = z - p.z;
            let length = dx.hypot(dz);
            g.set_input(&id, &json!({"dx":dx/length,"dz":dz/length}));
            g.tick(0.05);
            if g.players[&id]
                .race
                .as_ref()
                .is_none_or(|r| r.next_checkpoint != previous)
            {
                break;
            }
        }
    }
    let p = &g.players[&id];
    assert_eq!(p.races_completed, 1);
    assert_eq!(p.zone, "hub");
    assert_eq!(p.rings, 45);
    assert!(p.best_race_ms.unwrap() > 30000.);
    advance(&mut g, 1000);
    assert_eq!(g.players[&id].rings, 45);
}

#[test]
fn normal_chaos_evolution_requires_earned_milestones_even_with_perfect_stats() {
    let (mut g, id, _) = fixture(true);
    act(
        &mut g,
        &id,
        "evolve",
        json!({"missionsCompleted":3,"racesCompleted":1}),
    );
    assert_ne!(g.players[&id].form, "chaos");
    {
        let p = g.players.get_mut(&id).unwrap();
        p.missions_completed = 3;
        p.races_completed = 1;
        p.happiness = 80.;
    }
    act(&mut g, &id, "evolve", json!({}));
    assert_eq!(g.players[&id].form, "chaos");
    act(&mut g, &id, "align-dark", json!({}));
    assert_eq!(g.players[&id].form, "chaos");
}

#[test]
fn authorized_owner_dark_chaos_override_preserves_stats_currency_and_persists() {
    let (mut g, id, token) = fixture(true);
    let stats = g.players[&id].stats.clone();
    let rings = g.players[&id].rings;
    act(&mut g, &id, "admin-dark-chaos", json!({}));
    assert_eq!(g.players[&id].alignment, -100.);
    assert_eq!(g.players[&id].form, "chaos");
    assert_eq!(g.players[&id].rings, rings);
    assert_eq!(g.players[&id].stats, stats);
    let restored = Game::new(g.export_profiles(), BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(restored.player_for_token(&token).unwrap().form, "chaos");
}

#[test]
fn legacy_shop_consumables_and_unknown_items_cannot_spend_or_grant_effects() {
    let (mut g, id, _) = fixture(false);
    position(&mut g, &id, -4., 8.);
    {
        let p = g.players.get_mut(&id).unwrap();
        p.energy = 20.;
        p.hunger = 30.;
        p.happiness = 40.;
    }
    act(
        &mut g,
        &id,
        "buy",
        json!({"item":"garden-fruit","price":-999,"rings":99999,"energy":99999}),
    );
    let p = &g.players[&id];
    assert_eq!(p.rings, 20);
    assert!(p.energy < 21.);
    assert_eq!(p.hunger, 30.);
    assert_eq!(p.happiness, 40.);
    act(
        &mut g,
        &id,
        "buy",
        json!({"item":"run-drive","price":0,"levels":99}),
    );
    assert_eq!(g.players[&id].rings, 20);
    assert_eq!(g.players[&id].stats["run"].level, 1);
    for item in [
        json!("power-drive"),
        json!("../profiles.json"),
        json!({"id":"garden-fruit","price":0}),
    ] {
        act(&mut g, &id, "buy", json!({"item":item}));
    }
    assert_eq!(g.players[&id].rings, 20);
    assert_eq!(g.players[&id].stats["power"].level, 1);
    assert!(g.players[&id].care.inventory.is_empty());
    assert!(g.players[&id].care.receipts.is_empty());
}

#[test]
fn shop_proximity_is_server_owned_with_boundary_checks() {
    let (mut g, id, _) = fixture(false);
    g.players.get_mut(&id).unwrap().rings = 100;
    act(
        &mut g,
        &id,
        "buy",
        json!({"item":"eggshell-cap","x":-4,"z":8}),
    );
    assert_eq!(g.players[&id].rings, 100);
    position(&mut g, &id, -1.99, 8.);
    act(&mut g, &id, "buy", json!({"item":"eggshell-cap"}));
    assert_eq!(g.players[&id].rings, 100);
    position(&mut g, &id, -2.01, 8.);
    act(&mut g, &id, "buy", json!({"item":"eggshell-cap"}));
    assert_eq!(g.players[&id].rings, 75);
    assert_eq!(g.players[&id].equipped.as_deref(), Some("eggshell"));
    act(&mut g, &id, "garden", json!({}));
    position(&mut g, &id, -4., 8.);
    act(&mut g, &id, "buy", json!({"item":"eggshell-cap"}));
    assert_eq!(g.players[&id].rings, 75);
}

#[test]
fn cosmetic_is_one_time_owned_equipped_and_round_trips() {
    let (mut g, id, token) = fixture(false);
    position(&mut g, &id, -4., 8.);
    g.players.get_mut(&id).unwrap().rings = 50;
    act(&mut g, &id, "buy", json!({"item":"eggshell-cap"}));
    act(&mut g, &id, "buy", json!({"item":"eggshell-cap"}));
    let p = &g.players[&id];
    assert_eq!(p.rings, 25);
    assert_eq!(p.owned_cosmetics, vec!["eggshell"]);
    assert_eq!(p.equipped.as_deref(), Some("eggshell"));
    let restored = Game::new(g.export_profiles(), BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(
        restored
            .player_for_token(&token)
            .unwrap()
            .equipped
            .as_deref(),
        Some("eggshell")
    );
}

#[test]
fn corrupt_cosmetics_and_unowned_equipment_are_filtered() {
    let (g, _, token) = fixture(false);
    let mut profiles = g.export_profiles();
    profiles[0]["ownedCosmetics"] = json!(["unknown", "eggshell", "eggshell"]);
    profiles[0]["equipped"] = json!("unknown");
    let restored = Game::new(profiles, BTreeMap::new(), false, 1000.).unwrap();
    let p = restored.player_for_token(&token).unwrap();
    assert_eq!(p.owned_cosmetics, vec!["eggshell"]);
    assert_eq!(p.equipped, None);
}

#[test]
fn terrain_interpolation_holes_and_validation() {
    let t = terrain(
        &[
            &[Some(0.), Some(0.4), Some(0.8)],
            &[Some(0.), Some(0.4), Some(0.8)],
        ],
        0.,
    );
    assert!((t.sample(0.5, 0.5).unwrap() - 0.2).abs() < 1e-9);
    assert_eq!(t.sample(-1., 0.), None);
    let hole = terrain(
        &[&[Some(2.), None, Some(2.)], &[Some(2.), None, Some(2.)]],
        0.,
    );
    assert_eq!(hole.sample(1., 0.5), None);
    assert_eq!(hole.sample(0.1, 0.5), Some(2.));
    let mut bad = t.clone();
    bad.heights = vec![None; 6];
    assert!(bad.validate().is_err());
    bad.heights = vec![Some(f64::NAN); 6];
    assert!(bad.validate().is_err());
}

#[test]
fn terrain_safe_spawn_absolute_height_and_forged_vertical_input() {
    let t = terrain(
        &[
            &[None, None, Some(7.)],
            &[None, None, None],
            &[None, None, None],
        ],
        0.,
    );
    let (mut g, id, _) = fixture_terrain(false, BTreeMap::from([("hub".into(), t)]));
    assert_eq!(
        (g.players[&id].x, g.players[&id].z, g.players[&id].y),
        (2., 0., 7.)
    );
    g.set_input(
        &id,
        &json!({"dx":0,"dz":0,"y":99999,"vy":99999,"groundY":-99999,"jumpOffset":99999}),
    );
    g.tick(0.05);
    assert_eq!(g.players[&id].y, 7.);
    g.set_input(&id, &json!({"dx":0,"dz":0,"jump":true}));
    g.tick(0.05);
    assert!(g.players[&id].y > 7.);
    advance(&mut g, 1500);
    assert_eq!(g.players[&id].y, 7.);
}

#[test]
fn native_navigation_blocks_voids_and_high_cliffs_with_real_jump_limit() {
    for row in [
        vec![Some(0.), None, Some(0.), Some(0.)],
        vec![Some(0.), Some(4.), Some(4.), Some(4.)],
    ] {
        let t = terrain(&[&row, &row, &row], 5.);
        let (mut g, id, _) = fixture_terrain(true, BTreeMap::from([("hub".into(), t)]));
        for i in 0..80 {
            g.set_input(&id, &json!({"dx":99999,"dz":0,"jump":i==20,"y":99999}));
            g.tick(0.05);
        }
        assert!(g.players[&id].x < 0.5);
        assert_eq!(g.players[&id].y, 0.);
    }
}

#[test]
fn real_jump_can_clear_low_ledge_while_walking_cannot() {
    let t = terrain(
        &[
            &[None, None, None, None],
            &[Some(0.), Some(0.2), Some(0.2), Some(0.2)],
            &[None, None, None, None],
        ],
        5.,
    );
    let (mut g, id, _) = fixture_terrain(false, BTreeMap::from([("hub".into(), t)]));
    for _ in 0..15 {
        g.set_input(&id, &json!({"dx":1,"dz":0}));
        g.tick(0.05);
    }
    assert!(g.players[&id].x < 0.5);
    for i in 0..24 {
        g.set_input(&id, &json!({"dx":1,"dz":0,"jump":i==0}));
        g.tick(0.05);
    }
    assert!(g.players[&id].x > 1.5);
    advance(&mut g, 1000);
    assert_eq!(g.players[&id].y, 0.2);
}

#[test]
fn disconnect_cancels_race_and_returns_to_separate_hub() {
    let (mut g, id, _) = fixture(false);
    act(&mut g, &id, "garden", json!({}));
    assert_eq!(g.players[&id].zone, "garden");
    act(&mut g, &id, "race", json!({}));
    g.disconnect(&id);
    assert_eq!(g.players[&id].zone, "hub");
    assert!(g.players[&id].race.is_none());
    let snapshot = g.snapshot(&id);
    assert_eq!(snapshot["players"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["players"][0]["id"], id);
    assert_eq!(snapshot["players"][0]["connected"], false);
}

#[test]
fn body_reshapes_from_stats_and_survives_restore() {
    let (mut g, id, token) = fixture(false);
    let p = g.players.get_mut(&id).unwrap();
    chao_game_server::body::apply_capture(&mut p.body, 0);
    assert!(p.body.v_pos < 0.);
    let aim = p.body.v_pos;
    assert_eq!(p.public()["body"]["typeName"], "child");
    let restored = Game::new(g.export_profiles(), BTreeMap::new(), false, 1000.).unwrap();
    let p = restored.player_for_token(&token).unwrap();
    assert_eq!(p.body.v_pos, aim);
    assert_eq!(
        p.public()["body"]["morph"]["targets"]
            .as_array()
            .unwrap()
            .len(),
        17
    );
}

#[test]
fn one_memory_card_holds_up_to_eight_chao() {
    let (mut g, first_id, first_token) = fixture(false);
    assert!(g.card_for(&first_token).is_some());
    let made = g
        .create_session(&json!({"name":"Pebble the long name"}))
        .unwrap();
    let card = made["card"].as_str().unwrap().to_owned();
    assert_eq!(
        made["player"]["name"], "Pebble",
        "names fit the game's 7 bytes"
    );
    for i in 1..8 {
        let more = g
            .create_session(&json!({"name":format!("C{i}"),"card":card}))
            .unwrap();
        assert_eq!(more["card"], card);
    }
    assert!(
        g.create_session(&json!({"name":"Nine","card":card}))
            .is_err()
    );
    assert!(g.create_session(&json!({"card":"not-a-card"})).is_err());
    let sessions = g.card_sessions(&card);
    assert_eq!(sessions.len(), 8);
    // Each token the card hands out opens its own Chao and leads back to the card.
    for session in &sessions {
        let token = session["token"].as_str().unwrap();
        let p = g.player_for_token(token).unwrap();
        assert_eq!(p.id, session["player"]["id"]);
        assert_eq!(g.card_for(token), g.card_for(&card));
    }
    assert!(!sessions.iter().any(|s| s["player"]["id"] == first_id));
    // The code never lands in the stored profiles.
    assert!(
        !serde_json::to_string(&g.export_profiles())
            .unwrap()
            .contains(&card)
    );
}

#[test]
fn a_carried_egg_hatches_into_a_new_slot_on_the_same_card() {
    let mut g = Game::new(vec![], BTreeMap::new(), false, 1000.).unwrap();
    let made = g.create_session(&json!({"name":"Mother"})).unwrap();
    let (card, token) = (
        made["card"].as_str().unwrap().to_owned(),
        made["token"].as_str().unwrap().to_owned(),
    );
    let parent = made["player"]["id"].as_str().unwrap().to_owned();
    g.players
        .get_mut(&parent)
        .unwrap()
        .care
        .inventory
        .insert("egg:2".into(), 1);
    let revision = g.players[&parent].care.revision;
    let egg = |item: &str| json!({"name":"Pip","card":card,"egg":{"from":token,"itemId":item}});
    assert!(
        g.create_session(&egg("egg:3")).is_err(),
        "only a carried egg"
    );
    let hatched = g.create_session(&egg("egg:2")).unwrap();
    assert_eq!(hatched["card"], card.as_str());
    assert_eq!(hatched["player"]["name"], "Pip");
    let child = &g.players[hatched["player"]["id"].as_str().unwrap()];
    assert_eq!(
        child.life.gene.color,
        [2, 2],
        "a White egg hatches a white Chao"
    );
    assert!(
        !g.players[&parent].care.inventory.contains_key("egg:2"),
        "the egg is used up"
    );
    assert_eq!(g.players[&parent].care.revision, revision + 1);
    assert!(
        g.create_session(&egg("egg:2")).is_err(),
        "and can't hatch twice"
    );
    let other = g.create_session(&json!({"name":"Else"})).unwrap();
    let stranger = json!({"name":"Pip","card":other["card"],"egg":{"from":token,"itemId":"egg:2"}});
    g.players
        .get_mut(&parent)
        .unwrap()
        .care
        .inventory
        .insert("egg:2".into(), 1);
    assert!(
        g.create_session(&stranger).is_err(),
        "an egg stays on its own card"
    );
}

#[test]
fn only_the_selected_chao_of_a_card_is_out() {
    let mut g = Game::new(vec![], BTreeMap::new(), false, 1000.).unwrap();
    let first = g.create_session(&json!({"name":"One"})).unwrap();
    let card = first["card"].as_str().unwrap().to_owned();
    let second = g
        .create_session(&json!({"name":"Two","card":card}))
        .unwrap();
    let stranger = g.create_session(&json!({"name":"Else"})).unwrap();
    let id = |v: &Value| v["player"]["id"].as_str().unwrap().to_owned();
    let (one, two, other) = (id(&first), id(&second), id(&stranger));
    g.connect(&one);
    g.connect(&other);
    assert!(g.benched.is_empty());
    g.connect(&two);
    assert!(
        g.benched.contains(&one),
        "selecting another Chao sends the first back to the card"
    );
    assert!(!g.benched.contains(&other), "other players' Chao stay");
    g.connect(&one);
    assert!(
        g.benched.contains(&two) && !g.benched.contains(&one),
        "and swapping back works"
    );
}
