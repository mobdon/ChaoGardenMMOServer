//! Differential runner. Built privately by verify_native_ball.py.
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, BufRead},
};
use tiny_garden_core::native_ball::*;
use tiny_garden_core::native_growth;

fn main() {
    let payload = fs::read(env::args_os().nth(1).expect("private decoded payload")).unwrap();
    let src = BallSource::from_verified_payload(&payload).expect("A3VE source mismatch");
    for line in io::stdin().lock().lines() {
        let v: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let n = |k: usize| v[k].as_i64().unwrap();
        let result = match v[0].as_str().unwrap() {
            "stage" => {
                let mut rng = native_growth::NativeRng::new(n(2) as u32);
                let s = src.create_stage(n(1) as u8, &mut rng).unwrap();
                json!([
                    s.generated_feed(),
                    rng.state(),
                    s.remaining,
                    s.period,
                    s.timer
                ])
            }
            "flip" => {
                let mut board = serde_json::from_value(v[3].clone()).unwrap();
                let effects = src.flip(&mut board, n(1) as usize, n(2) as usize).unwrap();
                json!([board, effects])
            }
            "ring" => {
                let mut ring = FallingRing {
                    x: n(1) as i16,
                    y: n(2) as i32,
                    velocity: n(3) as i16,
                    delay: n(4) as u8,
                };
                let result = ring.step(n(5) as i16, n(6) as u8, n(7) as u8);
                json!([
                    ring.y,
                    ring.velocity,
                    ring.delay,
                    match result {
                        RingStep::Falling => 0,
                        RingStep::Caught => 1,
                        RingStep::Missed => 2,
                    }
                ])
            }
            "shot" => {
                let board = serde_json::from_value(v[6].clone()).unwrap();
                let mut shot = Shot {
                    column: n(1) as u8,
                    color: n(2) as u8,
                    y: n(3) as i16,
                };
                let result = shot.step(&board, n(4) as i16, n(5) as u8).unwrap();
                json!([
                    shot.y,
                    match result {
                        ShotStep::Flying => 0,
                        ShotStep::Landed { row, .. } => row,
                    }
                ])
            }
            "drop" => {
                let mut rng = native_growth::NativeRng::new(n(2) as u32);
                let mut s = src.create_stage(n(1) as u8, &mut rng).unwrap();
                s.board = serde_json::from_value(v[3].clone()).unwrap();
                s.remaining = n(4) as u8;
                s.falling = n(5) != 0;
                s.flags = n(6) as u8;
                s.scroll = n(7) as i16;
                s.timer = n(8) as u8;
                let drops = s.descend(n(9) as u8);
                json!([
                    s.board,
                    s.remaining,
                    u8::from(s.falling),
                    s.flags,
                    s.scroll,
                    s.timer,
                    drops
                        .iter()
                        .map(|d| [
                            i32::from(d.column),
                            i32::from(d.x),
                            i32::from(d.y),
                            i32::from(d.delay)
                        ])
                        .collect::<Vec<_>>()
                ])
            }
            "control" => {
                let board = serde_json::from_value(v[12].clone()).unwrap();
                let mut p = Launcher {
                    x: n(1) as i16,
                    column: n(2) as u8,
                    phase: n(3) as u8,
                    facing_right: n(4) != 0,
                    timer: n(5) as u8,
                    anim_frame: n(6) as u8,
                    anim_timer: n(7) as u8,
                };
                let fire = p.control(n(8) as u16, n(9) as u16, n(10) as u8, n(11) as u8, &board);
                json!([
                    p.x,
                    p.column,
                    p.phase,
                    u8::from(p.facing_right),
                    p.timer,
                    p.anim_frame,
                    p.anim_timer,
                    fire.map(|s| s.color).unwrap_or(0)
                ])
            }
            "resolve" => {
                use sha2::{Digest, Sha256};
                let mut s = src
                    .create_stage(0, &mut native_growth::NativeRng::new(0))
                    .unwrap();
                s.board = serde_json::from_value(v[4].clone()).unwrap();
                s.remaining = n(5) as u8;
                let mut op = Resolution::new(n(1) as u8, n(2) as u8, n(3) as u8).unwrap();
                let mut digest = Sha256::new();
                let mut ticks = 0u16;
                loop {
                    let end = op.step(&src, &mut s, 0);
                    digest.update([op.phase, op.timer, s.flags, s.remaining]);
                    for row in s.board {
                        digest.update(row);
                    }
                    ticks += 1;
                    if let Some(rings) = end {
                        for d in rings {
                            digest.update([d.column, d.delay]);
                            digest.update(d.x.to_le_bytes());
                            digest.update(d.y.to_le_bytes());
                        }
                        break;
                    }
                    assert!(ticks < 2048);
                }
                json!([ticks, format!("{:x}", digest.finalize())])
            }
            "play" => {
                use sha2::{Digest, Sha256};
                let mut round = BallPlay::begin(
                    &src,
                    n(1) as u8,
                    &mut native_growth::NativeRng::new(n(2) as u32),
                    0,
                )
                .unwrap();
                let mut digests = Vec::new();
                for held in v[3].as_array().unwrap() {
                    let out = round.step(&src, held.as_u64().unwrap() as u16);
                    let mut d = Sha256::new();
                    for row in round.stage.board {
                        d.update(row);
                    }
                    let p = &round.player;
                    digests.push(json!([
                        format!("{:x}", d.finalize()),
                        round.stage.flags,
                        round.stage.remaining,
                        round.stage.timer,
                        round.stage.scroll,
                        p.x,
                        p.column,
                        p.phase,
                        round.rings
                    ]));
                    if out != PlayOutcome::Playing {
                        break;
                    }
                }
                json!(digests)
            }
            "session" => {
                let mut session = BallSession::new(src, n(1) as u32);
                use sha2::{Digest, Sha256};
                let mut full = Sha256::new();
                let mut last = None;
                let mut board_changes = Vec::new();
                let mut transitions = Vec::new();
                for (i, held) in v[2].as_array().unwrap().iter().enumerate() {
                    let stage = session.stage_number();
                    let rings = session.play.rings;
                    let result = session.step(held.as_u64().unwrap() as u16);
                    full.update([session.stage_number()]);
                    full.update(session.play.rings.to_le_bytes());
                    for row in session.play.stage.board {
                        full.update(row);
                    }
                    let b = session.play.stage.board;
                    if last != Some(b) {
                        let mut d = Sha256::new();
                        for row in b {
                            d.update(row);
                        }
                        board_changes.push(json!([i + 1, format!("{:x}", d.finalize())]));
                        last = Some(b);
                    }
                    if session.stage_number() != stage
                        || session.play.rings != rings
                        || result.is_some()
                    {
                        transitions.push(json!([
                            i + 1,
                            session.stage_number(),
                            session.play.rings,
                            result
                                .map(|(e, _)| match e {
                                    Finish::Quit => 1,
                                    Finish::Lost => 2,
                                    Finish::Cleared => 3,
                                })
                                .unwrap_or(0)
                        ]));
                    }
                    if result.is_some() {
                        assert!(session.step(0).is_none());
                    }
                }
                json!([transitions, format!("{:x}", full.finalize()), board_changes])
            }
            "wallet" => json!(cartridge_wallet(n(1) as u32, n(2) as u16)),
            _ => panic!("unknown operation"),
        };
        println!("{result}");
    }
}
