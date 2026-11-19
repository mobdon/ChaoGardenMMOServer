#![allow(dead_code)]
use native_bounce::*;
use std::{
    env, fs,
    io::{self, Read, Write},
};
use tiny_garden_core::{native_bios_math, native_bounce, native_memory};

fn main() {
    let path = env::args_os().nth(1).expect("private payload");
    let payload = fs::read(path).unwrap();
    let source = A2neBounceSource::from_verified_payload(&payload).unwrap();
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input).unwrap();
    assert_eq!(input.len() % 4, 0);
    let mut output = Vec::new();
    for case in input.as_chunks::<4>().0 {
        let stage = case[0];
        let mode = case[1];
        let frames = u16::from_le_bytes(case[2..].try_into().unwrap());
        let mut round = source.begin_round();
        round.stage = stage;
        if stage > 2 {
            round.target_kind[6] = 2;
        }
        for (i, bumper) in round.bumpers.iter_mut().enumerate() {
            if stage > 1 && (i == 0 || stage > 2) {
                let mut a = NativeBounceActor::default();
                a.raw[..4].copy_from_slice(&0x0200_f4c0_u32.to_le_bytes());
                a.raw[0x1c] = i as u8;
                *bumper = Some(a);
            }
        }
        let mut trace = Vec::new();
        let mut count = 0_u32;
        let mut previous_held = 0;
        let mut cleared = [false; 4];
        for frame in 0..frames {
            if round.finished().is_some() {
                break;
            }
            if mode == 7
                && !cleared[usize::from(round.stage)]
                && u32::from_le_bytes(round.chao.raw[..4].try_into().unwrap()) == CHAO_ADDRESS
            {
                // Controlled empty-target setup tests original clear/next-stage
                // branches. Ordinary traces separately prove collection.
                cleared[usize::from(round.stage)] = true;
                round.remaining = 0;
                round.target_kind = [0; TARGETS];
                round.targets = [None; TARGETS];
            }
            let mut held = match mode {
                0 => {
                    if frame % 60 == 0 {
                        1
                    } else {
                        0
                    }
                }
                1 => 17,
                2 => 35,
                3 => {
                    if frame % 240 < 120 {
                        17
                    } else {
                        33
                    }
                }
                4 => {
                    if round.chao.raw[0xa] as i16 > round.spring.raw[0xa] as i16 {
                        17
                    } else {
                        33
                    }
                }
                5 => match frame {
                    40 => 8,
                    45 => 128,
                    50 => 1,
                    _ => 0,
                },
                6 => match frame {
                    40 | 50 => 8,
                    _ => {
                        if frame % 120 < 60 {
                            17
                        } else {
                            33
                        }
                    }
                },
                7 => u16::from(frame % 120 == 0),
                _ => 0,
            };
            if frame == 1 {
                held |= 1;
            }
            let pressed = held & !previous_held;
            previous_held = held;
            round
                .step(
                    native_memory::NativeButtons { pressed, held },
                    &native_bios_math::ReferenceBiosMath,
                )
                .unwrap();
            trace.extend_from_slice(&pressed.to_le_bytes());
            trace.extend_from_slice(&held.to_le_bytes());
            trace.extend_from_slice(&[round.stage, round.lives]);
            trace.extend_from_slice(&round.rings.to_le_bytes());
            trace.extend_from_slice(&round.scroll.to_le_bytes());
            trace.extend_from_slice(&[round.flags, round.remaining, round.target_sound]);
            trace.extend_from_slice(&round.vy.to_le_bytes());
            trace.extend_from_slice(&round.vx.to_le_bytes());
            for h in round.hitbox {
                trace.extend_from_slice(&h.to_le_bytes());
            }
            trace.extend_from_slice(&round.chao.raw);
            trace.extend_from_slice(&round.spring.raw);
            trace.extend_from_slice(&round.target_master.raw);
            for a in round.targets {
                trace.push(u8::from(a.is_some()));
                trace.extend_from_slice(&a.unwrap_or_default().raw);
            }
            for a in round.clouds {
                trace.extend_from_slice(&a.raw);
            }
            for a in round.bumpers {
                trace.push(u8::from(a.is_some()));
                trace.extend_from_slice(&a.unwrap_or_default().raw);
            }
            trace.extend_from_slice(&round.target_kind);
            trace.push(u8::from(round.finished().is_some()));
            count += 1;
        }
        output.extend_from_slice(&count.to_le_bytes());
        output.extend_from_slice(
            &(if count == 0 {
                0
            } else {
                trace.len() / count as usize
            } as u16)
                .to_le_bytes(),
        );
        output.extend_from_slice(&trace);
    }
    io::stdout().write_all(&output).unwrap();
}
