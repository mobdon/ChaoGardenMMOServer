use chao_game_server::{
    activities::ActivityCatalog,
    game::{Game, Terrain},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn race() -> Value {
    json!({"id":"synthetic-course","name":"Synthetic validated course","group":"test","model":"race-normal","worldId":"test-race","available":true,"geometryValidated":true,"bounds":{"minX":30,"maxX":35,"minZ":0,"maxZ":1},"spawn":{"x":32,"z":0.5},"checkpoints":[{"x":30.5,"z":0.5,"radius":0.2},{"x":33.5,"z":0.5,"radius":0.2},{"x":31.5,"z":0.5,"radius":0.2}],"rewardRings":25})
}
fn tier() -> Value {
    json!({"id":"test-tier","name":"Synthetic two-opponent tier","opponents":(0..2).map(|i|json!({"id":format!("cpu-{i}"),"name":format!("Opponent {}",i+1),"nativeIndex":i,"skills":[1,1,1,1,1,1,1,0],"appearance":{"type":2}})).collect::<Vec<_>>(),"rewardRings":50})
}
fn fixture(terrain: bool, owner: bool) -> (Game, String, String) {
    let mut game = Game::new(vec![], BTreeMap::new(), owner, 1000.).unwrap();
    game.activities =
        ActivityCatalog::from_values(json!({"courses":[race()]}), json!({"tiers":[tier()]}))
            .unwrap();
    if terrain {
        game.activities.terrains.insert(
            "test-race".into(),
            Terrain {
                min_x: 30.,
                min_z: 0.,
                step: 0.25,
                width: 21,
                depth: 5,
                heights: vec![Some(2.); 105],
            },
        );
    }
    let session = game
        .create_session(&json!({"name":"Activity test"}))
        .unwrap();
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
fn moving(game: &mut Game, id: &str, dx: f64, ms: usize) {
    for _ in 0..ms.div_ceil(50) {
        game.set_input(id, &json!({"dx":dx,"dz":0}));
        game.tick(0.05);
    }
}
fn move_until(game: &mut Game, id: &str, dx: f64, checkpoint: Option<usize>) {
    for _ in 0..200 {
        if game.players[id].race.as_ref().map(|r| r.next_checkpoint) == checkpoint {
            return;
        }
        moving(game, id, dx, 50);
    }
    panic!("physical movement did not reach expected gate");
}

#[test]
fn unavailable_or_missing_collision_courses_never_enter_from_client_claims() {
    let mut row = race();
    row["geometryValidated"] = json!(false);
    assert!(ActivityCatalog::from_values(json!({"courses":[row]}), json!({})).is_err());
    let (mut game, id, _) = fixture(false, false);
    assert_eq!(
        game.snapshot(&id)["activities"]["raceCourses"][0]["available"],
        false
    );
    act(
        &mut game,
        &id,
        "race-enter",
        json!({"courseId":"synthetic-course","available":true,"geometryValidated":true}),
    );
    assert_eq!(game.players[&id].zone, "hub");
    assert!(game.players[&id].race.is_none());
    game.activities.courses[0].available = false;
    assert_eq!(
        game.activities.public()["raceCourses"][0]["available"],
        false
    );
}

#[test]
fn native_races_count_down_use_native_bounds_and_physical_order_and_persist_once() {
    let (mut game, id, token) = fixture(true, false);
    let rings = game.players[&id].rings;
    act(
        &mut game,
        &id,
        "race-enter",
        json!({"courseId":"synthetic-course"}),
    );
    assert_eq!(game.players[&id].x, 32.);
    assert_eq!(game.players[&id].y, 2.);
    assert_eq!(game.snapshot(&id)["race"]["phase"], "countdown");
    moving(&mut game, &id, 1., 1000);
    assert_eq!(game.players[&id].x, 32.);
    advance(&mut game, 2000);
    moving(&mut game, &id, 1., 1000);
    assert!(game.players[&id].x > 33.);
    assert_eq!(game.players[&id].race.as_ref().unwrap().next_checkpoint, 0);
    act(
        &mut game,
        &id,
        "race-result",
        json!({"elapsedMs":1,"rings":9999}),
    );
    assert_eq!(game.players[&id].rings, rings);
    move_until(&mut game, &id, -1., Some(1));
    move_until(&mut game, &id, 1., Some(2));
    move_until(&mut game, &id, -1., None);
    let p = &game.players[&id];
    assert_eq!(p.zone, "hub");
    assert_eq!(p.rings, rings + 25);
    assert_eq!(p.race_records["synthetic-course"].completions, 1);
    assert!(p.race_records["synthetic-course"].best_ms.unwrap() > 5000.);
    assert_eq!(p.activity_result.as_ref().unwrap()["kind"], "race");
    advance(&mut game, 2000);
    assert_eq!(game.players[&id].rings, rings + 25);
    let restored = Game::new(game.export_profiles(), BTreeMap::new(), false, 1000.).unwrap();
    let p = restored.player_for_token(&token).unwrap();
    assert_eq!(p.race_records["synthetic-course"].completions, 1);
    assert!(p.race.is_none());
}

#[test]
fn activities_require_dismount_and_cancel_only_after_final_connection_leaves() {
    let (mut game, id, _) = fixture(true, true);
    act(&mut game, &id, "mount-dark-walker", json!({}));
    act(
        &mut game,
        &id,
        "race-enter",
        json!({"courseId":"synthetic-course"}),
    );
    assert!(game.players[&id].race.is_none());
    act(&mut game, &id, "karate-enter", json!({"tier":"test-tier"}));
    assert!(game.players[&id].karate.is_none());
    act(&mut game, &id, "dismount-vehicle", json!({}));
    act(
        &mut game,
        &id,
        "race-enter",
        json!({"courseId":"synthetic-course"}),
    );
    act(&mut game, &id, "mount-dark-walker", json!({}));
    assert!(game.players[&id].vehicle.is_none());
    game.connect(&id);
    game.disconnect(&id);
    assert!(game.players[&id].race.is_some());
    game.disconnect(&id);
    assert!(game.players[&id].race.is_none());
    assert_eq!(game.players[&id].zone, "hub");
    game.connect(&id);
    act(&mut game, &id, "karate-enter", json!({"tier":"test-tier"}));
    game.disconnect(&id);
    assert!(game.players[&id].karate.is_none());
}

#[test]
fn karate_uses_native_floor_and_server_windup_range_recovery_and_damage() {
    let (mut game, id, _) = fixture(true, false);
    act(&mut game, &id, "karate-enter", json!({"tier":"test-tier"}));
    assert_eq!(game.players[&id].y, 0.75);
    assert_eq!(game.players[&id].z, 0.8);
    assert_eq!(game.snapshot(&id)["karate"]["opponent"]["y"], 0.75);
    let now = game.time;
    let bout = game.players.get_mut(&id).unwrap().karate.as_mut().unwrap();
    let initial = bout.opponent_hp;
    bout.opponent_action = "recover".into();
    bout.opponent_until = now + 10000.;
    act(
        &mut game,
        &id,
        "karate-attack",
        json!({"move":"punch","damage":9999}),
    );
    advance(&mut game, 150);
    assert_eq!(
        game.players[&id].karate.as_ref().unwrap().opponent_hp,
        initial
    );
    advance(&mut game, 100);
    assert_eq!(
        game.players[&id].karate.as_ref().unwrap().opponent_hp,
        initial
    );
    advance(&mut game, 400);
    game.players.get_mut(&id).unwrap().z = 1.5;
    act(&mut game, &id, "karate-attack", json!({"move":"punch"}));
    advance(&mut game, 150);
    assert_eq!(
        game.players[&id].karate.as_ref().unwrap().opponent_hp,
        initial
    );
    advance(&mut game, 50);
    let hp = game.players[&id].karate.as_ref().unwrap().opponent_hp;
    assert!(hp < initial && hp > initial - 11.);
    act(&mut game, &id, "karate-attack", json!({"move":"kick"}));
    assert_eq!(
        game.players[&id].karate.as_ref().unwrap().player_action,
        "recover"
    );
    act(&mut game, &id, "karate-next", json!({}));
    assert_eq!(game.players[&id].karate.as_ref().unwrap().opponent_index, 0);
    act(&mut game, &id, "mount-dark-walker", json!({}));
    assert!(game.players[&id].vehicle.is_none());
    moving(&mut game, &id, 1., 2000);
    assert!(game.players[&id].x <= 1.35);
}

#[test]
fn karate_guard_dodge_and_focus_use_server_stamina_and_telegraph_timing() {
    for defense in ["karate-block", "karate-dodge"] {
        let (mut game, id, _) = fixture(true, false);
        act(&mut game, &id, "karate-enter", json!({"tier":"test-tier"}));
        let now = game.time;
        let p = game.players.get_mut(&id).unwrap();
        p.z = 1.5;
        let b = p.karate.as_mut().unwrap();
        let hp = b.player_hp;
        b.opponent_action = "punch".into();
        b.opponent_hit_at = Some(now + 300.);
        b.opponent_until = now + 1000.;
        act(&mut game, &id, defense, json!({"direction":"right"}));
        advance(&mut game, 300);
        let b = game.players[&id].karate.as_ref().unwrap();
        let damage = hp - b.player_hp;
        if defense == "karate-block" {
            assert!(damage > 2. && damage < 3.);
        } else {
            assert_eq!(damage, 0.);
        }
        assert!(b.player_stamina < 100.);
    }
    let (mut game, id, _) = fixture(true, false);
    act(&mut game, &id, "karate-enter", json!({"tier":"test-tier"}));
    game.players
        .get_mut(&id)
        .unwrap()
        .karate
        .as_mut()
        .unwrap()
        .player_stamina = 25.;
    act(&mut game, &id, "karate-focus", json!({}));
    let before = game.players[&id].karate.as_ref().unwrap().player_stamina;
    advance(&mut game, 500);
    assert!(game.players[&id].karate.as_ref().unwrap().player_stamina >= before + 15.);
}

#[test]
fn karate_requires_real_round_wins_and_persists_final_reward_once() {
    let (mut game, id, token) = fixture(true, false);
    let rings = game.players[&id].rings;
    act(&mut game, &id, "karate-enter", json!({"tier":"test-tier"}));
    act(
        &mut game,
        &id,
        "karate-result",
        json!({"won":true,"damage":9999,"rings":9999}),
    );
    assert_eq!(game.players[&id].rings, rings);
    for round in 0..2 {
        advance(&mut game, 1500);
        for _ in 0..30 {
            if game.players[&id].karate.as_ref().unwrap().phase != "fighting" {
                break;
            }
            act(&mut game, &id, "karate-attack", json!({"move":"punch"}));
            advance(&mut game, 550);
        }
        assert_eq!(
            game.players[&id].karate.as_ref().unwrap().phase,
            if round == 0 { "round-won" } else { "won" }
        );
        if round == 0 {
            assert_eq!(game.players[&id].rings, rings);
            act(&mut game, &id, "karate-next", json!({}));
            assert_eq!(game.players[&id].karate.as_ref().unwrap().opponent_index, 1);
            assert_eq!(game.players[&id].y, 0.75);
        }
    }
    assert_eq!(game.players[&id].rings, rings + 50);
    assert_eq!(game.players[&id].karate_records["test-tier"].completions, 1);
    assert_eq!(
        game.players[&id].activity_result.as_ref().unwrap()["won"],
        true
    );
    for _ in 0..10 {
        act(&mut game, &id, "karate-next", json!({}));
    }
    assert_eq!(game.players[&id].rings, rings + 50);
    let restored = Game::new(game.export_profiles(), BTreeMap::new(), false, 1000.).unwrap();
    let p = restored.player_for_token(&token).unwrap();
    assert_eq!(p.karate_records["test-tier"].completions, 1);
    assert!(p.karate.is_none());
    act(&mut game, &id, "karate-leave", json!({}));
    assert_eq!(game.players[&id].zone, "hub");
    assert!(game.players[&id].karate.is_none());
}

#[test]
fn karate_timeout_and_depleted_stamina_cannot_be_forged_into_wins() {
    let (mut game, id, _) = fixture(true, false);
    act(&mut game, &id, "karate-enter", json!({"tier":"test-tier"}));
    game.players
        .get_mut(&id)
        .unwrap()
        .karate
        .as_mut()
        .unwrap()
        .player_stamina = 0.;
    act(
        &mut game,
        &id,
        "karate-attack",
        json!({"move":"kick","stamina":100}),
    );
    assert_eq!(
        game.players[&id].karate.as_ref().unwrap().player_action,
        "idle"
    );
    let now = game.time;
    let b = game.players.get_mut(&id).unwrap().karate.as_mut().unwrap();
    b.player_hp = 1.;
    b.ends_at = now + 50.;
    advance(&mut game, 50);
    assert_eq!(game.players[&id].karate.as_ref().unwrap().phase, "lost");
    assert_eq!(game.players[&id].rings, 20);
    assert!(game.players[&id].karate_records.is_empty());
}
