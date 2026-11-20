use tiny_garden_core::{
    native_growth::NativeRng,
    native_rps::{NativeRpsClock, NativeRpsTarget, NativeRpsWaves, TARGET_COUNT},
};

fn target(state: u8, counter: u8, route: i16, x: i16, y: i16) -> NativeRpsTarget {
    let mut raw = [0xa5; 12];
    raw[0] = state;
    raw[1] = counter;
    raw[4..6].copy_from_slice(&route.to_le_bytes());
    raw[6..8].copy_from_slice(&x.to_le_bytes());
    raw[8..10].copy_from_slice(&y.to_le_bytes());
    NativeRpsTarget { raw }
}

fn one_target(value: NativeRpsTarget) -> NativeRpsWaves {
    let mut state = NativeRpsWaves {
        targets: [target(4, 0, 0, 0, 0); TARGET_COUNT],
        active_targets: 1,
        unhit_targets: 2,
        route_speed: 2,
        ..NativeRpsWaves::default()
    };
    state.targets[0] = value;
    state
}

#[test]
fn countdown_preserves_native_zero_expiry_boundary() {
    let mut clock = NativeRpsClock::default();
    clock.set(30);
    assert!(!clock.tick());
    assert_eq!(
        clock,
        NativeRpsClock {
            seconds: 29,
            subframes: 59
        }
    );
    for _ in 1..1800 {
        assert!(!clock.tick());
    }
    assert_eq!(clock, NativeRpsClock::default());
    assert!(clock.tick());
    assert_eq!(clock, NativeRpsClock::default());
}

#[test]
fn time_bonus_caps_and_preserves_subframes_exactly() {
    let mut clock = NativeRpsClock {
        seconds: 88,
        subframes: 27,
    };
    clock.add_seconds(10);
    assert_eq!(
        clock,
        NativeRpsClock {
            seconds: 98,
            subframes: 27
        }
    );
    clock.add_seconds(1);
    assert_eq!(
        clock,
        NativeRpsClock {
            seconds: 99,
            subframes: 0
        }
    );
    clock = NativeRpsClock {
        seconds: 0xff,
        subframes: 7,
    };
    clock.add_seconds(10);
    assert_eq!(
        clock,
        NativeRpsClock {
            seconds: 9,
            subframes: 7
        }
    );
    clock.set(255);
    assert_eq!(clock.subframes, 0);
}

#[test]
fn first_wave_consumes_ten_draws_and_preserves_unwritten_record_bytes() {
    let mut state = NativeRpsWaves {
        targets: [NativeRpsTarget { raw: [0xa5; 12] }; TARGET_COUNT],
        round_rings: 71,
        ..NativeRpsWaves::default()
    };
    let mut rng = NativeRng::new(123);
    let mut reference = rng;
    let expected: Vec<_> = (0..TARGET_COUNT)
        .map(|_| (reference.next_u15() / 10923) as u8)
        .collect();
    state.initialize(&mut rng);
    assert_eq!(rng, reference);
    assert_eq!(
        (
            state.wave,
            state.route_speed,
            state.active_targets,
            state.unhit_targets
        ),
        (0, 2, 10, 10)
    );
    assert_eq!(
        state.clock,
        NativeRpsClock {
            seconds: 30,
            subframes: 0
        }
    );
    assert_eq!(state.round_rings, 71);
    for (index, row) in state.targets.iter().enumerate() {
        assert_eq!(
            (row.state(), row.gesture(), row.route()),
            (0, expected[index], -(index as i16) * 32 - 2)
        );
        for offset in [1, 3, 6, 7, 8, 9, 10, 11] {
            assert_eq!(row.raw[offset], 0xa5);
        }
    }
}

#[test]
fn route_uses_original_corners_and_one_subtraction_wrap() {
    for (route, expected_route, position) in [
        (-1, 1, (10, 9)),
        (61, 63, (10, 71)),
        (62, 64, (10, 72)),
        (193, 195, (141, 72)),
        (194, 196, (142, 72)),
        (257, 259, (142, 9)),
        (258, 260, (142, 8)),
        (390, 392, (10, 8)),
        (391, 1, (10, 9)),
        (999, 609, (-207, 8)),
    ] {
        let mut state = one_target(target(0, 255, route, 1000, 1000));
        let update = state.tick_targets(&mut NativeRng::new(1));
        assert_eq!(
            (state.targets[0].route(), state.targets[0].position()),
            (expected_route, position)
        );
        assert_eq!(state.targets[0].raw[1], 0);
        assert_eq!(update.appeared, u16::from(route == -1));
        assert_eq!(update.random_calls, 0);
    }
}

#[test]
fn negative_route_retains_xy_and_signed_route_wrap_does_not_create_appearance() {
    let mut state = one_target(target(0, 0, -3, 47, 61));
    let update = state.tick_targets(&mut NativeRng::new(1));
    assert_eq!(state.targets[0].position(), (47, 61));
    assert_eq!((update.hidden, update.appeared), (1, 0));
    state.targets[0] = target(0, 0, i16::MAX, 47, 61);
    let update = state.tick_targets(&mut NativeRng::new(1));
    assert_eq!(state.targets[0].route(), -32767);
    assert_eq!(state.targets[0].position(), (47, 61));
    assert_eq!((update.hidden, update.appeared), (1, 0));
}

#[test]
fn defeated_and_tied_targets_remove_after_old_counter_thirty() {
    let mut state = one_target(target(2, 29, 0, 100, 50));
    state.active_targets = 2;
    state.wave = 3;
    state.targets[0].raw[10] = 0xfe;
    state.targets[0].raw[11] = 3;
    let mut rng = NativeRng::new(1);
    assert_eq!(state.tick_targets(&mut rng).defeated_removed, 0);
    assert_eq!(state.targets[0].position(), (98, 53));
    assert_eq!(state.tick_targets(&mut rng).defeated_removed, 1);
    assert_eq!(state.targets[0].state(), 4);
    assert_eq!(state.targets[0].position(), (96, 56));
    assert_eq!((state.active_targets, state.round_rings), (1, 2));
    state.targets[0] = target(3, 30, 0, 100, 50);
    state.active_targets = 2;
    assert_eq!(state.tick_targets(&mut rng).tie_removed, 1);
    assert_eq!(state.targets[0].position(), (100, 50));
    assert_eq!((state.active_targets, state.round_rings), (1, 2));
}

#[test]
fn flying_target_window_is_inclusive_and_uses_signed_velocity() {
    for (x, y, moves) in [
        (-33, 0, false),
        (-32, -32, true),
        (239, 159, true),
        (240, 0, false),
        (0, -33, false),
        (0, 160, false),
    ] {
        let mut state = one_target(target(2, 0, 0, x, y));
        state.targets[0].raw[10] = 0xff;
        state.targets[0].raw[11] = 2;
        state.tick_targets(&mut NativeRng::new(1));
        assert_eq!(
            state.targets[0].position(),
            if moves { (x - 1, y + 2) } else { (x, y) }
        );
    }
}

#[test]
fn last_removal_advances_wave_with_original_bonus_and_later_speed_reset() {
    let mut state = one_target(target(2, 30, 0, 0, 0));
    state.wave = 8;
    state.unhit_targets = 0;
    state.route_speed = 4;
    state.clock = NativeRpsClock {
        seconds: 89,
        subframes: 42,
    };
    let mut rng = NativeRng::new(1);
    let update = state.tick_targets(&mut rng);
    assert_eq!(
        (state.round_rings, state.wave, state.route_speed),
        (5, 9, 2)
    );
    assert_eq!(state.targets[0].route(), -4); // old speed used by A800 first.
    assert_eq!((state.active_targets, state.unhit_targets), (10, 10));
    assert_eq!(
        state.clock,
        NativeRpsClock {
            seconds: 99,
            subframes: 0
        }
    );
    assert!(update.wave_started);
    assert_eq!(update.random_calls, 10);
}

#[test]
fn signed_counter_and_round_score_byte_behavior_are_not_new_clamps() {
    let mut state = one_target(target(2, 0xff, 0, 0, 0));
    state.tick_targets(&mut NativeRng::new(1));
    assert_eq!(state.targets[0].state(), 2);
    assert_eq!(state.targets[0].raw[1], 0);
    for (before, delta, after) in [(98, 2, 99), (127, 1, 128), (255, 1, 0), (0, -1, 255)] {
        state.round_rings = before;
        state.add_round_rings(delta);
        assert_eq!(state.round_rings, after);
    }
}
