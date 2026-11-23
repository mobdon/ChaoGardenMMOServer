//! Screenshot setup only (untracked): two adult, breed-ready test Chao in the garden of a private data dir,
//! the same way tests/life_protocol.rs prepares them. Prints their tokens and cards as JSON.
use chao_game_server::life::{Stage, native::BREED_INVITE};
use chao_game_server::service::{AppState, Config};
use serde_json::json;

fn main() {
    let dir = std::env::args().nth(1).expect("data dir");
    let state = AppState::load(Config::local(dir.into(), false)).unwrap();
    let mut game = state.game.lock().unwrap();
    let mut out = vec![];
    for (name, x) in [("Momo", -2.), ("Dadu", -1.)] {
        let session = game.create_session(&json!({ "name": name })).unwrap();
        let id = session["player"]["id"].as_str().unwrap().to_owned();
        let mut player = game.players.remove(&id).unwrap();
        game.teleport(&mut player, "garden", x, 2.).unwrap();
        player.life.stage = Stage::Adult;
        player.life.emotion.state[3] = BREED_INVITE;
        game.players.insert(id, player);
        out.push(session);
    }
    game.dirty = true;
    state.persist(&mut game).unwrap();
    println!("{}", serde_json::to_string(&out).unwrap());
}
