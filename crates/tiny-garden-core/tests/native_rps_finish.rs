use native_growth::NativeRng;
use native_rps::{NativeRpsTarget, NativeRpsWaves};
use native_rps_finish::*;
use native_rps_game::*;
use tiny_garden_core::{native_growth, native_rps, native_rps_finish, native_rps_game};

#[test]
fn original_finish_automatically_takes260_updates_or81_with_immediate_a() {
    for (keys, last) in [(0, 260), (1, 81), (8, 81)] {
        let mut finish = NativeRpsFinish::default();
        for tick in 1..=last {
            assert_eq!(finish.tick(keys), Ok(tick == last));
        }
        assert_eq!((finish.mode, finish.phase, finish.counter), (5, 4, 17));
    }
}

#[test]
fn finish_boundaries_match_original_mode2_to4_to5() {
    let mut finish = NativeRpsFinish::default();
    for _ in 0..63 {
        assert_eq!(finish.tick(0), Ok(false));
    }
    assert_eq!((finish.mode, finish.phase, finish.counter), (4, 4, 0));
    for _ in 0..179 {
        assert_eq!(finish.tick(0), Ok(false));
    }
    assert_eq!((finish.mode, finish.counter), (4, 179));
    assert_eq!(finish.tick(0), Ok(false));
    assert_eq!((finish.mode, finish.counter), (5, 0));
}

#[test]
fn final_native_score_waits_for_delayed_target_removal_and_then_freezes() {
    let assets = NativeRpsAssets {
        trig: [0; 64],
        hitbox: [0; 47],
    };
    let mut waves = NativeRpsWaves {
        targets: [NativeRpsTarget { raw: [0; 12] }; 10],
        wave: 5,
        active_targets: 2,
        unhit_targets: 1,
        ..NativeRpsWaves::default()
    };
    for target in &mut waves.targets {
        target.raw[0] = 4;
    }
    waves.targets[0].raw[0] = 2;
    let mut session = NativeRpsSession {
        round: NativeRpsRound {
            waves,
            finishing: true,
            ..NativeRpsRound::default()
        },
        ..NativeRpsSession::default()
    };
    let mut rng = NativeRng::new(123);
    let seed = rng;
    for tick in 1..=81 {
        let event = session
            .tick(INPUT_A, &assets, &RejectBiosAngle, &mut rng)
            .unwrap();
        assert_eq!(event.completed, tick == 81);
        if tick == 30 {
            assert_eq!(session.round.waves.round_rings, 0);
        }
        if tick == 31 {
            assert_eq!(session.round.waves.round_rings, 3);
        }
        if tick < 81 {
            assert_eq!(session.result, None);
        }
    }
    assert_eq!(session.result, Some(3));
    assert_eq!(rng, seed);
    let before = (session.clone(), rng);
    assert_eq!(
        session.tick(0, &assets, &RejectBiosAngle, &mut rng),
        Ok(SessionEvents::default())
    );
    assert_eq!((session, rng), before);
}

#[test]
fn active_pause_and_invalid_finish_phase_fail_without_mutating_source_state() {
    let mut finish = NativeRpsFinish {
        mode: 2,
        phase: 255,
        counter: 17,
    };
    let before = finish;
    assert_eq!(finish.tick(0), Err(FinishError::InvalidPhase));
    assert_eq!(finish, before);
    let mut session = NativeRpsSession::default();
    let mut rng = NativeRng::new(123);
    let assets = NativeRpsAssets {
        trig: [0; 64],
        hitbox: [0; 47],
    };
    let before = (session.clone(), rng);
    assert_eq!(
        session.tick(8, &assets, &RejectBiosAngle, &mut rng),
        Err(FinishError::UnsupportedPause)
    );
    assert_eq!((session, rng), before);
}
