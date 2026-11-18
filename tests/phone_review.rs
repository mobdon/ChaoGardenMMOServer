//! Independent host-boundary regression checks for Tiny Garden integration.
use chao_game_server::game::Game;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn player(game: &mut Game, name: &str) -> String {
    let session = game.create_session(&json!({"name":name})).unwrap();
    let id = session["player"]["id"].as_str().unwrap().to_owned();
    game.connect(&id);
    id
}
fn advance(game: &mut Game, milliseconds: usize) {
    for _ in 0..milliseconds.div_ceil(50) {
        game.tick(0.05);
    }
}
fn action(game: &mut Game, id: &str, fields: Value) {
    advance(game, 100);
    game.action(id, &fields);
}
fn session_id(game: &Game, player: &str) -> String {
    game.snapshot(player)["tinyGame"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn two_profiles_cannot_control_or_observe_each_others_private_rounds() {
    let mut game = Game::new(vec![], BTreeMap::new(), false, 1000.).unwrap();
    let a = player(&mut game, "A");
    let b = player(&mut game, "B");
    for id in [&a, &b] {
        action(
            &mut game,
            id,
            json!({"action":"tiny-start","gameId":"search"}),
        );
    }
    let a_id = session_id(&game, &a);
    let b_id = session_id(&game, &b);
    assert_ne!(a_id, b_id);
    let before_a = game.snapshot(&a)["tinyGame"].clone();
    let before_b = game.snapshot(&b)["tinyGame"].clone();
    game.tiny_input(
        &a,
        &json!({"sessionId":b_id,"input":{"kind":"move","x":1,"y":0}}),
    );
    assert_eq!(game.snapshot(&a)["tinyGame"], before_a);
    assert_eq!(game.snapshot(&b)["tinyGame"], before_b);
    assert!(!game.snapshot(&a)["tinyGame"].to_string().contains(&b_id));
    assert!(!game.snapshot(&b)["tinyGame"].to_string().contains(&a_id));
    action(
        &mut game,
        &a,
        json!({"action":"tiny-stop","sessionId":b_id}),
    );
    assert_eq!(session_id(&game, &a), a_id);
    assert_eq!(session_id(&game, &b), b_id);
}

#[test]
fn a_replaced_session_rejects_old_actions_and_stale_movement_expires() {
    let mut game = Game::new(vec![], BTreeMap::new(), false, 1000.).unwrap();
    let id = player(&mut game, "Moving");
    action(
        &mut game,
        &id,
        json!({"action":"tiny-start","gameId":"search"}),
    );
    let old = session_id(&game, &id);
    action(
        &mut game,
        &id,
        json!({"action":"tiny-stop","sessionId":old}),
    );
    action(
        &mut game,
        &id,
        json!({"action":"tiny-start","gameId":"search"}),
    );
    let current = session_id(&game, &id);
    assert_ne!(old, current);
    let start = game.snapshot(&id)["tinyGame"]["view"]["state"]["player_x"]
        .as_f64()
        .unwrap();
    game.tiny_input(
        &id,
        &json!({"sessionId":old,"input":{"kind":"move","x":1,"y":0}}),
    );
    advance(&mut game, 200);
    assert_eq!(
        game.snapshot(&id)["tinyGame"]["view"]["state"]["player_x"],
        start
    );
    game.tiny_input(
        &id,
        &json!({"sessionId":current,"input":{"kind":"move","x":1,"y":0}}),
    );
    advance(&mut game, 200);
    let moving = game.snapshot(&id)["tinyGame"]["view"]["state"]["player_x"]
        .as_f64()
        .unwrap();
    assert!(moving > start);
    advance(&mut game, 300);
    let stopped = game.snapshot(&id)["tinyGame"]["view"]["state"]["player_x"]
        .as_f64()
        .unwrap();
    advance(&mut game, 1000);
    assert_eq!(
        game.snapshot(&id)["tinyGame"]["view"]["state"]["player_x"],
        stopped
    );
    assert_eq!(game.players[&id].rings, 20);
}
