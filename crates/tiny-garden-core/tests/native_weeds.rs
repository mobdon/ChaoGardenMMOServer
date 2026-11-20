use std::num::NonZeroU16;
use tiny_garden_core::{native_growth::NativeRng, native_weeds::*};

fn budget(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n).unwrap()
}
fn assets() -> NativeWeedAssets {
    NativeWeedAssets {
        map: [9; 440],
        trig: [0; 256],
        hitbox: [-8, 16, -14, 15],
    }
}
fn garden(timer: u32) -> NativeWeeds {
    NativeWeeds::restore(NativeWeedFields {
        spawn_ticks: timer,
        ..NativeWeedFields::default()
    })
}
fn occupied() -> NativeWeeds {
    let mut g = garden(100);
    g.fields.positions[0] = NativeWeedPosition { x: 80, y: 100 };
    NativeWeeds::restore(g.fields)
}
fn pull_input() -> NativePullInput {
    NativePullInput {
        cursor_x: 88,
        cursor_y: 85,
        shop_boundary: 59,
        pressed_keys: 1,
        ..NativePullInput::default()
    }
}

#[test]
fn native_spawn_clock_wraps_and_all_pause_or_full_capacity_gates_preserve_rng() {
    let mut g = garden(0);
    let mut rng = NativeRng::new(99);
    assert_eq!(
        g.spawn_tick(0, &assets(), &mut rng, budget(1))
            .unwrap()
            .event,
        SpawnEvent::Waiting
    );
    assert_eq!(g.fields.spawn_ticks, u32::MAX);
    assert_eq!(rng.state(), 99);
    for flag in [1, 2, 0x10, 0x20, 0x40, 0x80] {
        let mut g = garden(1);
        let before = g.clone();
        assert_eq!(
            g.spawn_tick(flag, &assets(), &mut rng, budget(1))
                .unwrap()
                .event,
            SpawnEvent::Paused
        );
        assert_eq!(g, before);
    }
    let mut g = garden(1);
    g.fields.positions.fill(NativeWeedPosition { x: 8, y: 24 });
    let before = g.clone();
    assert_eq!(
        g.spawn_tick(0, &assets(), &mut rng, budget(1))
            .unwrap()
            .event,
        SpawnEvent::Paused
    );
    assert_eq!(g, before);
    assert_eq!(rng.state(), 99);
}

#[test]
fn spawn_uses_exact_bounded_draws_first_free_slot_and_native_emergence_fields() {
    let mut g = garden(1);
    g.fields.positions[0] = NativeWeedPosition { x: 9, y: 24 };
    let mut rng = NativeRng::new(123);
    let mut expected = rng;
    let timer = (u32::from(expected.bounded(budget(30))) + 60) * 60;
    let p = NativeWeedPosition {
        x: expected.bounded(budget(160)) as u8 + 8,
        y: expected.bounded(budget(136)) as u8 + 24,
    };
    let result = g.spawn_tick(0x0c, &assets(), &mut rng, budget(1)).unwrap();
    assert_eq!(
        result,
        SpawnUpdate {
            event: SpawnEvent::Spawned {
                slot: 1,
                position: p
            },
            candidates: 1,
            random_calls: 3
        }
    );
    assert_eq!(rng.state(), expected.state());
    assert_eq!(g.fields.spawn_ticks, timer);
    assert_eq!(g.fields.count(), 2);
    assert!(g.weeds[1].present && g.weeds[1].emerging);
    assert_eq!(g.weeds[1].y, 0);
    assert_eq!((NATIVE_WEED_TILE, NATIVE_EFFECT_TILE), (0x109, 0x18e));
}

#[test]
fn rejected_candidate_discards_extra_rng_and_budget_resume_never_redraws_timer() {
    let mut rng = NativeRng::new(0);
    let mut expected = rng;
    let timer = (u32::from(expected.bounded(budget(30))) + 60) * 60;
    let x1 = expected.bounded(budget(160)) as u8 + 8;
    let y1 = expected.bounded(budget(136)) as u8 + 24;
    expected.next_u15();
    let after_reject = expected.state();
    let x2 = expected.bounded(budget(160)) as u8 + 8;
    let y2 = expected.bounded(budget(136)) as u8 + 24;
    let mut a = assets();
    a.map.fill(0);
    let first = usize::from(y1 / 8) * 22 + usize::from(x1 / 8);
    let second = usize::from(y2 / 8) * 22 + usize::from(x2 / 8);
    assert_ne!(first, second);
    a.map[second] = 13;
    let mut g = garden(1);
    let first = g.spawn_tick(0, &a, &mut rng, budget(1)).unwrap();
    assert_eq!((first.event, first.random_calls), (SpawnEvent::Pending, 4));
    assert_eq!(rng.state(), after_reject);
    assert!(g.pending_spawn());
    let next = g.continue_spawn(&a, &mut rng, budget(1)).unwrap();
    assert_eq!(
        next.event,
        SpawnEvent::Spawned {
            slot: 0,
            position: NativeWeedPosition { x: x2, y: y2 }
        }
    );
    assert_eq!(next.random_calls, 2);
    assert_eq!(g.fields.spawn_ticks, timer);
    assert_eq!(rng.state(), expected.state());
    assert!(!g.pending_spawn());
    assert_eq!(
        g.continue_spawn(&a, &mut rng, budget(1)),
        Err(Error::NoPendingSpawn)
    );
}

#[test]
fn source_map_requires_exact_tile_words_and_candidates_stay_in_native_bounds() {
    let mut a = assets();
    assert!(a.permits_spawn(8, 24));
    a.map.fill(13);
    assert!(a.permits_spawn(167, 159));
    for tile in [0, 8, 10, 0x4009, 0x100d] {
        a.map.fill(tile);
        assert!(!a.permits_spawn(8, 24));
    }
}

#[test]
fn emergence_takes_64_eligible_updates_with_original_affine_parameters_and_pause_gates() {
    let mut g = garden(1);
    g.spawn_tick(0, &assets(), &mut NativeRng::new(123), budget(1))
        .unwrap();
    let base = g.weeds[0].base_y;
    let before = g.clone();
    assert!(!g.update_weed(0, 0x10).unwrap().rendered);
    assert_eq!(g, before);
    assert!(g.update_weed(0, 0x80).unwrap().rendered);
    assert_eq!(g, before);
    for frame in 1..=63 {
        let result = g.update_weed(0, 0).unwrap();
        assert_eq!(result.affine_parameter, Some(0x4000 / frame));
        assert_eq!(g.weeds[0].y, i16::from(base) + ((64 - frame) >> 3));
        assert!(g.weeds[0].emerging);
    }
    let result = g.update_weed(0, 0).unwrap();
    assert_eq!(result.affine_parameter, None);
    assert!(!g.weeds[0].emerging);
    assert_eq!(g.weeds[0].y, i16::from(base));
}

#[test]
fn pull_sequence_uses_75_updates_effect_transitions_and_clears_saved_x_only() {
    let a = assets();
    let mut g = occupied();
    let start = g.begin_pull(0, pull_input(), &a).unwrap();
    assert_eq!(
        (
            start.cursor.cursor_x,
            start.cursor.cursor_y,
            start.cursor.cursor_layer,
            start.sound
        ),
        (80, 84, 101, Some(0x15))
    );
    let mut created = 0;
    let mut released = 0;
    let mut effect = NativeWeedEffect::default();
    let mut finish = 0;
    for tick in 1..=75 {
        let p = g.update_pull(0, &a).unwrap();
        let w = g.update_weed(0, 0).unwrap();
        if w.effect == Some(EffectCommand::Create) {
            created += 1;
            assert_eq!(tick, 8);
        }
        if w.effect == Some(EffectCommand::Release) {
            released += 1;
            effect.releasing = true;
            assert_eq!(tick, 50);
        }
        effect.update(0);
        if p.sound == Some(0xd) {
            assert_eq!(tick, 50);
        }
        if p.finished {
            finish = tick;
            assert!(w.removed);
        }
        if tick < 75 {
            assert!(!w.removed);
        }
    }
    assert_eq!((created, released, finish), (1, 1, 75));
    assert_eq!(g.fields.positions[0], NativeWeedPosition { x: 0, y: 100 });
    assert!(!g.weeds[0].present);
    assert_eq!(g.fields.count(), 0);
    assert!(g.pull.is_none());
    assert!(!effect.disposed);
    for _ in 0..4 {
        effect.update(0);
    }
    assert!(effect.disposed);
}

#[test]
fn native_pull_pause_differs_from_weed_task_and_hit_rectangle_keeps_inclusive_edges() {
    let a = assets();
    let mut g = occupied();
    g.begin_pull(0, pull_input(), &a).unwrap();
    let before = g.clone();
    let paused = g.update_pull(0x80, &a).unwrap();
    assert!(paused.native_dirty);
    assert_eq!(g, before);
    g.update_pull(0x10, &a).unwrap();
    assert_eq!(g.pull.unwrap().counter, 1);
    for (x, y) in [(80, 78), (96, 93)] {
        let mut g = occupied();
        g.begin_pull(
            0,
            NativePullInput {
                cursor_x: x,
                cursor_y: y,
                ..pull_input()
            },
            &a,
        )
        .unwrap();
    }
    for (x, y) in [(79, 78), (97, 93), (80, 77), (96, 94)] {
        let mut g = occupied();
        let before = g.clone();
        assert_eq!(
            g.begin_pull(
                0,
                NativePullInput {
                    cursor_x: x,
                    cursor_y: y,
                    ..pull_input()
                },
                &a
            ),
            Err(Error::PullBlocked)
        );
        assert_eq!(g, before);
    }
}

#[test]
fn native_effect_has_three_ten_update_frames_and_independent_freeze_render_gates() {
    let mut e = NativeWeedEffect::default();
    for _ in 0..15 {
        assert!(e.update(0));
    }
    assert_eq!(e.tile, 0x18e);
    e.releasing = true;
    let before = e;
    assert!(!e.update(0x10));
    assert_eq!(e, before);
    assert!(e.update(0x80));
    assert_eq!(e, before);
    for tick in 1..=30 {
        let rendered = e.update(0);
        if tick < 30 {
            assert!(rendered);
        } else {
            assert!(!rendered);
        }
    }
    assert_eq!(e.tile, 0x194);
    assert!(e.disposed);
}

#[test]
fn raw_weed_fields_preserve_unknown_bytes_and_fail_before_partial_truncated_writes() {
    let fields = NativeWeedFields {
        spawn_ticks: 0x12345678,
        positions: std::array::from_fn(|i| NativeWeedPosition {
            x: i as u8,
            y: 100 + i as u8,
        }),
    };
    let mut record = [0xa5; 0x80];
    fields.write_raw_fields(&mut record).unwrap();
    assert_eq!(NativeWeedFields::read_raw_fields(&record).unwrap(), fields);
    for (i, &b) in record.iter().enumerate() {
        if !(0x10..0x14).contains(&i) && !(0x37..0x57).contains(&i) {
            assert_eq!(b, 0xa5);
        }
    }
    let mut short = [0xa5; 0x56];
    let before = short;
    assert!(fields.write_raw_fields(&mut short).is_err());
    assert_eq!(short, before);
    assert!(NativeWeedFields::read_raw_fields(&short).is_err());
}

#[test]
#[ignore = "requires CHAO_TINY_GARDEN_PAYLOAD, owner's exact decoded A2NE payload"]
fn actual_native_map_trig_hitbox_and_long_seed_stream_stay_source_bound() {
    let bytes = std::fs::read(
        std::env::var_os("CHAO_TINY_GARDEN_PAYLOAD").expect("Set CHAO_TINY_GARDEN_PAYLOAD"),
    )
    .unwrap();
    let a = NativeWeedAssets::from_verified_payload(&bytes).unwrap();
    assert_eq!(a.hitbox, [-8, 16, -14, 15]);
    assert_eq!(a.trig[0], 256);
    assert_eq!(a.trig[192], 0);
    for seed in 0..1000 {
        let mut g = garden(1);
        let mut rng = NativeRng::new(seed);
        let mut result = g.spawn_tick(0, &a, &mut rng, budget(1)).unwrap();
        while result.event == SpawnEvent::Pending {
            result = g.continue_spawn(&a, &mut rng, budget(1)).unwrap();
        }
        let SpawnEvent::Spawned { position, .. } = result.event else {
            panic!("no spawn")
        };
        assert!((8..=167).contains(&position.x));
        assert!((24..=159).contains(&position.y));
        assert!(a.permits_spawn(position.x, position.y));
    }
    let mut changed = bytes.clone();
    changed[MAP_OFFSET] ^= 1;
    assert!(NativeWeedAssets::from_verified_payload(&changed).is_err());
    assert!(NativeWeedAssets::from_verified_payload(&bytes[..bytes.len() - 1]).is_err());
}
