use native_growth::NativeRng;
use native_rps::{NativeRpsTarget, NativeRpsWaves};
use native_rps_game::*;
use tiny_garden_core::{native_growth, native_rps, native_rps_game};

fn synthetic() -> NativeRpsAssets {
    NativeRpsAssets {
        trig: [16384; 64],
        hitbox: [24; 47],
    }
}
fn collision(card_gesture: u8, target_gesture: u8) -> (NativeRpsLauncher, NativeRpsWaves) {
    let mut launcher = NativeRpsLauncher {
        shots: 5,
        ..NativeRpsLauncher::default()
    };
    launcher.cards[0].raw[0] = 1;
    launcher.cards[0].raw[2] = card_gesture;
    launcher.cards[0].raw[6..8].copy_from_slice(&80_i16.to_le_bytes());
    let mut waves = NativeRpsWaves {
        targets: [NativeRpsTarget { raw: [0; 12] }; 10],
        active_targets: 10,
        unhit_targets: 10,
        ..NativeRpsWaves::default()
    };
    for target in &mut waves.targets {
        target.raw[0] = 4;
    }
    waves.targets[0].raw[0] = 0;
    waves.targets[0].raw[2] = target_gesture;
    waves.targets[0].raw[6..8].copy_from_slice(&12_i16.to_le_bytes());
    waves.targets[0].raw[8..10].copy_from_slice(&76_i16.to_le_bytes());
    (launcher, waves)
}

#[test]
fn native_input_moves_left_then_right_and_fires_without_spending_or_resetting_counter() {
    let mut launcher = NativeRpsLauncher {
        lane: 0,
        marker_x: 12,
        shots: 5,
        ..NativeRpsLauncher::default()
    };
    launcher.cards[1].raw[1] = 79;
    assert_eq!(
        launcher.input_tick(INPUT_LEFT | INPUT_RIGHT | INPUT_A),
        Ok(true)
    );
    assert_eq!(
        (launcher.lane, launcher.marker_x, launcher.shots),
        (1, 28, 5)
    );
    assert_eq!(launcher.cards[1].raw[1], 79);
    assert_eq!(launcher.input_tick(INPUT_A), Ok(false));
    let before = launcher.clone();
    assert_eq!(launcher.input_tick(8), Err(Error::InvalidInput));
    assert_eq!(launcher, before);
    launcher.lane = 255;
    let before = launcher.clone();
    assert_eq!(launcher.input_tick(0), Err(Error::InvalidLane));
    assert_eq!(launcher, before);
}

#[test]
fn every_numeric_gesture_pair_uses_original_win_tie_reflection_branch() {
    for card in 0..3 {
        for target in 0..3 {
            let (mut launcher, mut waves) = collision(card, target);
            let mut rng = NativeRng::new(1);
            let seed = rng;
            let events = launcher
                .tick_projectiles(&mut waves, &synthetic(), &ReferenceBiosAngle, &mut rng)
                .unwrap();
            let difference = i16::from(card) + 2 - i16::from(target);
            if difference == 1 || difference == 4 {
                assert_eq!(
                    (events.won, waves.targets[0].state(), waves.unhit_targets),
                    (1, 2, 9)
                );
            } else if difference == 2 {
                assert_eq!(
                    (events.tied, waves.targets[0].state(), waves.unhit_targets),
                    (1, 3, 9)
                );
            } else {
                assert_eq!(
                    (
                        events.reflected,
                        launcher.cards[0].state(),
                        waves.unhit_targets
                    ),
                    (1, 5, 10)
                );
            }
            assert_eq!(rng, seed);
            assert_eq!(launcher.shots, 5);
        }
    }
}

#[test]
fn unavailable_bios_cannot_partially_advance_card_target_or_rng() {
    let (mut launcher, mut waves) = collision(0, 1);
    let mut rng = NativeRng::new(1);
    let before = (launcher.clone(), waves.clone(), rng);
    assert_eq!(
        launcher.tick_projectiles(&mut waves, &synthetic(), &RejectBiosAngle, &mut rng),
        Err(Error::BiosUnavailable)
    );
    assert_eq!((launcher, waves, rng), before);
}

#[test]
fn projectile_departure_and_native_respawn_delays_do_not_spend_on_fire() {
    let mut launcher = NativeRpsLauncher {
        shots: 5,
        ..NativeRpsLauncher::default()
    };
    launcher.cards[0].raw[0] = 1;
    launcher.cards[0].raw[6..8].copy_from_slice(&(-32_i16).to_le_bytes());
    let mut waves = NativeRpsWaves::default();
    let mut rng = NativeRng::new(123);
    launcher
        .tick_projectiles(&mut waves, &synthetic(), &RejectBiosAngle, &mut rng)
        .unwrap();
    assert_eq!(
        (
            launcher.shots,
            launcher.cards[0].state(),
            launcher.cards[0].raw[1]
        ),
        (4, 7, 0)
    );
    for _ in 0..20 {
        launcher
            .tick_projectiles(&mut waves, &synthetic(), &RejectBiosAngle, &mut rng)
            .unwrap();
    }
    assert_eq!(
        (launcher.cards[0].state(), launcher.cards[0].raw[1]),
        (7, 20)
    );
    launcher
        .tick_projectiles(&mut waves, &synthetic(), &RejectBiosAngle, &mut rng)
        .unwrap();
    assert_eq!(
        (launcher.cards[0].state(), launcher.cards[0].raw[1]),
        (8, 0)
    );
    let mut draws = 0;
    for _ in 0..21 {
        draws += launcher
            .tick_projectiles(&mut waves, &synthetic(), &RejectBiosAngle, &mut rng)
            .unwrap()
            .random_calls;
    }
    assert_eq!(
        (launcher.cards[0].state(), launcher.cards[0].position().1),
        (0, 116)
    );
    assert_eq!(draws, 2);
}

#[test]
fn shot_display_counts_twelfths_every_second_update_without_touching_actual_shots() {
    let mut launcher = NativeRpsLauncher {
        shots: 4,
        displayed_shots: 5,
        ..NativeRpsLauncher::default()
    };
    launcher.tick_shot_display();
    assert_eq!(
        (launcher.displayed_shots, launcher.display_fraction),
        (5, 0)
    );
    launcher.tick_shot_display();
    assert_eq!(
        (launcher.displayed_shots, launcher.display_fraction),
        (4, 11)
    );
    for _ in 0..22 {
        launcher.tick_shot_display();
    }
    assert_eq!(
        (
            launcher.shots,
            launcher.displayed_shots,
            launcher.display_fraction
        ),
        (4, 4, 0)
    );
}

#[test]
fn complete_active_step_preserves_source_order_and_native_finish_rule() {
    let mut round = NativeRpsRound::default();
    let mut rng = NativeRng::new(123);
    round.initialize(&mut rng);
    assert_eq!(
        (
            round.waves.clock.seconds,
            round.launcher.shots,
            round.launcher.lane,
            round.launcher.marker_x
        ),
        (30, 5, 1, 76)
    );
    let mut expected = NativeRng::new(123);
    for _ in 0..13 {
        expected.next_u15();
    }
    assert_eq!(rng, expected);
    round.waves.clock.set(0);
    round.waves.unhit_targets = 1;
    assert!(
        round
            .tick_active(0, &synthetic(), &RejectBiosAngle, &mut rng)
            .unwrap()
            .finish_started
    );
    let before = (round.clone(), rng);
    assert_eq!(
        round
            .tick_active(INPUT_A, &synthetic(), &RejectBiosAngle, &mut rng)
            .unwrap(),
        RoundEvents::default()
    );
    assert_eq!((round.clone(), rng), before);
    round.finishing = false;
    round.waves.unhit_targets = 0;
    assert!(
        !round
            .tick_active(0, &synthetic(), &RejectBiosAngle, &mut rng)
            .unwrap()
            .finish_started
    );
    round.launcher.shots = 0;
    assert!(
        round
            .tick_active(0, &synthetic(), &RejectBiosAngle, &mut rng)
            .unwrap()
            .finish_started
    );
}

#[test]
fn native_actor_moves_before_decrement_and_uses_same_rng_for_next_choice() {
    let mut actor = NativeRpsActor {
        x: 80,
        countdown: 2,
        mode: 0,
    };
    let mut rng = NativeRng::new(123);
    assert_eq!(actor.tick(&mut rng).unwrap().random_calls, 0);
    assert_eq!((actor.x, actor.countdown), (79, 1));
    let event = actor.tick(&mut rng).unwrap();
    assert!(event.random_calls >= 2);
    assert_eq!(actor.x, 78);
    assert!(actor.countdown > 0);
}

#[test]
fn source_gate_rejects_unknown_payloads_and_bios_reference_axes_are_explicit() {
    assert!(matches!(
        NativeRpsAssets::from_verified_payload(&[]),
        Err(Error::SourceIdentity)
    ));
    assert!(matches!(
        NativeRpsAssets::from_verified_payload(&vec![0; ASOE_LENGTH]),
        Err(Error::SourceIdentity)
    ));
    for (x, y, angle) in [
        (0, 0, 0),
        (1, 0, 0),
        (-1, 0, 0x8000),
        (0, 1, 0x4000),
        (0, -1, 0xc000),
        (1, 1, 0x2000),
        (-1, -1, 0xa000),
    ] {
        assert_eq!(ReferenceBiosAngle.atan2(x, y), Ok(angle));
    }
}

#[test]
fn native_uninitialized_scratch_roll_rejects_atomically_instead_of_inventing_stack_bytes() {
    let (mut launcher, mut waves) = collision(0, 0);
    launcher.cards[0].raw[0] = 8;
    launcher.cards[0].raw[1] = 10;
    let mut rng = NativeRng::new(858717501);
    let before = (launcher.clone(), waves.clone(), rng);
    assert_eq!(
        launcher.tick_projectiles(&mut waves, &synthetic(), &ReferenceBiosAngle, &mut rng),
        Err(Error::NativeScratchRead {
            initialized: 1,
            index: 1
        })
    );
    assert_eq!((launcher, waves, rng), before);
}

#[test]
#[ignore = "requires CHAO_TINY_ASOE_PAYLOAD pointing to the owner's exact decoded ASOE payload"]
fn verified_owned_asoe_tables_match_native_trig_and_source_gate_rejects_corruption() {
    let path =
        std::env::var_os("CHAO_TINY_ASOE_PAYLOAD").expect("explicit owned ASOE source required");
    let mut payload = std::fs::read(path).unwrap();
    let assets = NativeRpsAssets::from_verified_payload(&payload).unwrap();
    assert_eq!(assets.trig.len(), 64);
    assert_eq!(assets.hitbox.len(), 47);
    assert_eq!(assets.native_trig(0, 0x4000), 16384);
    assert_eq!(assets.native_trig(0x8000, 0x4000), -16384);
    payload[TRIG_OFFSET] ^= 1;
    assert!(matches!(
        NativeRpsAssets::from_verified_payload(&payload),
        Err(Error::SourceIdentity)
    ));
}
