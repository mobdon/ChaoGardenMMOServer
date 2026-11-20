use chao_game_server::game::Game;
use serde_json::json;
use std::collections::BTreeMap;
// Uses the owner's locally extracted tables. No retail bytes or profile required in Git.
#[test]
#[ignore = "requires CHAO_ROSTER_TABLE and CHAO_SKY_RAILS"]
fn mission_character_moves() {
    let mut g = Game::new(vec![], BTreeMap::new(), true, 0.).unwrap();
    let s = g.create_session(&json!({})).unwrap();
    let id = s["player"]["id"].as_str().unwrap().to_owned();
    g.connect(&id);
    let act = |g: &mut Game, a: &str, k: &str| {
        g.tick(0.1);
        g.action(&id, &json!({"action":a,"character":k}));
    };
    let step = |g: &mut Game, x: f64, z: f64, q: bool, j: bool| {
        g.set_input(&id, &json!({"dx":x,"dz":z,"tech":q,"jump":j}));
        g.tick(0.05);
    };
    let place = |g: &mut Game, x: f64, y: f64, z: f64, heading: f64| {
        let p = g.players.get_mut(&id).unwrap();
        p.x = x;
        p.y = y;
        p.z = z;
        p.jump_offset = y;
        p.ground_y = 0.;
        p.vy = 0.;
        p.facing = heading;
        p.roster_velocity = (0., 0.);
    };
    act(&mut g, "mission", "");
    act(&mut g, "dev-actor", "terios");
    assert_eq!(g.players[&id].dev_actor.as_deref(), Some("terios"));
    place(&mut g, 0., 1., 0., std::f64::consts::PI);
    act(&mut g, "attack", "");
    for _ in 0..10 {
        step(&mut g, 0., 0., false, false);
    }
    assert!(g.events.iter().any(|e| e.message["kind"] == "homing-hit"));
    place(&mut g, -2., 0., 12., 1.5);
    step(&mut g, 0., 0., true, false);
    assert_eq!(g.players[&id].tech.mode, "grind");
    let initial = g.players[&id].x;
    for _ in 0..6 {
        step(&mut g, 0., 0., false, false);
    }
    assert!((g.players[&id].x - initial).abs() > 1.);
    step(&mut g, 0., 0., false, true);
    assert!(g.players[&id].vy > 0. && g.players[&id].tech.mode != "grind");
    act(&mut g, "dev-actor", "knuck");
    place(&mut g, 0., 2., 0., 0.);
    for _ in 0..5 {
        step(&mut g, 0., 1., true, false);
    }
    assert_eq!(g.players[&id].tech.mode, "glide");
    assert!(g.players[&id].y > 1.7);
    place(&mut g, -15.4, 0., 12., -std::f64::consts::FRAC_PI_2);
    for _ in 0..8 {
        step(&mut g, -1., 0., false, false);
    }
    assert!(g.players[&id].x > -15.45);
    for _ in 0..27 {
        step(&mut g, -1., 0., true, false);
    }
    assert!(g.players[&id].y > 3.7);
    place(&mut g, 16., 0., 6., 0.);
    let before = g.players[&id].rings;
    for _ in 0..2 {
        act(&mut g, "roster-dig", "");
        for _ in 0..23 {
            step(&mut g, 0., 0., false, false);
        }
    }
    assert_eq!(g.players[&id].rings, before + 1);
    act(&mut g, "dev-actor", "dwalk");
    place(&mut g, 0., 0., -4., std::f64::consts::PI);
    for _ in 0..16 {
        step(&mut g, 0., 0., true, false);
    }
    assert!(!g.players[&id].tech.locks().is_empty());
    step(&mut g, 0., 0., false, false);
    for _ in 0..25 {
        step(&mut g, 0., 0., false, false);
    }
    assert!(g.events.iter().any(|e| e.message["kind"] == "missile-hit"));
    act(&mut g, "dev-actor", "sshadow");
    place(&mut g, 0., 0., 14., 0.);
    let before = g.players[&id].rings;
    for _ in 0..25 {
        step(&mut g, 0., 0., true, false);
    }
    assert!(g.players[&id].y > 3. && g.players[&id].rings == before - 1);
    let h = g.players[&id].y;
    for _ in 0..5 {
        step(&mut g, 0., 0., false, false);
    }
    assert!((g.players[&id].y - h).abs() < 0.001);
    step(&mut g, 0., 0., false, true);
    assert!(g.players[&id].y < h);
    act(&mut g, "dev-actor", "");
    assert!(g.players[&id].tech.mode.is_empty());
}
