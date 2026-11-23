//! Deterministic load shape; microbenchmark only, not a network capacity claim.
//! cargo run --release --locked --example capacity_core > measurements/core.json
use chao_game_server::game::{Game, Player, SHOP_CATALOG};
use serde_json::{Value, json};
use std::{collections::BTreeMap, hint::black_box, time::Instant};

fn distribution(mut samples: Vec<f64>) -> Value {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    json!({"count":n,"p50":samples[n/2],"p95":samples[(n*95/100).min(n-1)],"max":samples[n-1]})
}

// Original 282f6ab snapshot shape for this fixture (no mission/race/Tiny game).
// Used for payload comparison only; no claim of measuring an unmodified service.
fn global_baseline(game: &Game, id: &str) -> Value {
    let rings: Vec<_> = game
        .rings
        .iter()
        .filter(|r| r.available_at <= game.time)
        .map(|r| json!({"id":r.id,"x":r.x,"z":r.z,"zone":r.zone}))
        .collect();
    json!({"type":"snapshot","selfId":id,"serverTime":game.time,
        "players":game.players.values().filter(|p|p.connections>0||p.watch.is_some()).map(Player::public).collect::<Vec<_>>(),
        "rings":rings,"enemies":[],"mission":null,"race":null,"shop":{"catalog":SHOP_CATALOG},
        "activities":game.activities.public(),"karate":null,"activityResult":null,"tinyGame":null,"tinyResult":null})
}

fn main() {
    let mut results = vec![];
    for count in [100usize, 250, 500] {
        for resident_count in [0usize, count] {
            let mut game = Game::new(vec![], BTreeMap::new(), false, 1_800_000_000_000.).unwrap();
            let mut connected = vec![];
            for i in 0..count + resident_count {
                let session = game
                    .create_session(&json!({"name":format!("Load {i}")}))
                    .unwrap();
                let id = session["player"]["id"].as_str().unwrap().to_owned();
                if i < count {
                    game.connect(&id);
                    let player = game.players.get_mut(&id).unwrap();
                    player.zone = ["hub", "garden", "hero", "dark"][i % 4].into();
                    player.alignment = match player.zone.as_str() {
                        "hero" => 100.,
                        "dark" => -100.,
                        _ => 0.,
                    };
                    connected.push(id);
                } else {
                    game.connect(&id);
                    let player = game.players.get_mut(&id).unwrap();
                    player.zone = ["garden", "hero", "dark"][(i - count) % 3].into();
                    player.alignment = match player.zone.as_str() {
                        "hero" => 100.,
                        "dark" => -100.,
                        _ => 0.,
                    };
                    game.action(&id, &json!({"type":"action","action":"watch-start"}));
                    assert!(game.players[&id].watch.is_some());
                    game.disconnect(&id);
                }
            }
            game.drain_events();
            for _ in 0..40 {
                game.tick(0.05);
                game.drain_events();
            }
            let mut tick = vec![];
            let mut resident = vec![];
            for _ in 0..400 {
                let started = Instant::now();
                game.tick(0.05);
                tick.push(started.elapsed().as_secs_f64() * 1000.);
                resident.push(game.last_resident_tick.as_secs_f64() * 1000.);
                game.drain_events();
            }
            let baseline_bytes = serde_json::to_vec(&global_baseline(&game, &connected[0]))
                .unwrap()
                .len();
            let mut snapshot = vec![];
            let mut room_bytes = BTreeMap::new();
            for n in 0..200 {
                let id = &connected[n % count];
                let started = Instant::now();
                let data = serde_json::to_vec(&game.snapshot(id)).unwrap();
                snapshot.push(started.elapsed().as_secs_f64() * 1000.);
                room_bytes.insert(game.players[id].zone.clone(), data.len());
                black_box(data);
            }
            results.push(
                json!({"connected":count,"residents":resident_count,"rooms":4,
                "tickMs":distribution(tick),"residentMs":distribution(resident),
                "snapshotBuildSerializeMs":distribution(snapshot),"snapshotBytesByRoom":room_bytes,
                "unfilteredBaselineSnapshotBytes":baseline_bytes}),
            );
        }
    }
    println!("{}",serde_json::to_string_pretty(&json!({"schemaVersion":1,"kind":"core microbenchmark; synthetic flat terrain; no network, disk, native assets or active competitions","results":results})).unwrap());
}
