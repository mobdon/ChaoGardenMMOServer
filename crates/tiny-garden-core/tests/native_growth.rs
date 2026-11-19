use std::num::NonZeroU16;
use tiny_garden_core::native_growth::*;

#[test]
fn native_rng_wraps_before_extracting_fifteen_bits() {
    let mut rng = NativeRng::new(0);
    assert_eq!(rng.next_u15(), 0);
    assert_eq!(rng.state(), 0x3039);
    assert_eq!(rng.next_u15(), 21468);
    assert_eq!(rng.state(), 0xd3dc_167e);
    let mut all = NativeRng::new(u32::MAX);
    assert_eq!(all.bounded(NonZeroU16::new(5).unwrap()), 4);
    assert_eq!(all.state(), 0xbe39_e1cc);
    assert_eq!(SKILL_NAMES, ["swim", "fly", "run", "power", "stamina"]);
    assert_eq!(NATIVE_OPERATION_ADDRESS, 0x0200_493c);
    assert_eq!(NATIVE_OPERATION_END, 0x0200_4a4a);
}

#[test]
fn signed_ratings_clamp_needs_and_negative_experience_without_losing_levels_or_points() {
    let skill = NativeSkill {
        experience: 5,
        grade: 4,
        level: 12,
        points: 321,
    };
    let mut state = NativeGrowth {
        mood: 10,
        belly: 190,
        skills: [skill; 5],
    };
    let mut rng = NativeRng::new(0xabcdef01);
    let result = apply_fruit(&mut state, [-3, 2, -2, -3, -4, -5, 0], &mut rng);
    assert_eq!((state.mood, state.belly), (0, 200));
    assert_eq!(state.skills[4].experience, 5);
    for stat in &state.skills[..4] {
        assert_eq!(stat.experience, 0);
    }
    assert!(
        state
            .skills
            .iter()
            .all(|stat| stat.level == 12 && stat.points == 321 && stat.grade == 4)
    );
    assert_eq!(result.random_calls, 0);
    assert_eq!(rng.state(), 0xabcdef01);
}

#[test]
fn exactly_one_overflow_levels_up_and_uses_post_food_mood_strictly_above_180() {
    let stat = NativeSkill {
        experience: 90,
        grade: 2,
        level: 10,
        points: 400,
    };
    let initial = NativeGrowth {
        mood: 180,
        belly: 100,
        skills: [stat; 5],
    };
    let mut ordinary = initial;
    let mut happy = initial;
    let mut first = NativeRng::new(0);
    let mut second = first;
    let a = apply_fruit(&mut ordinary, [0, 0, 1, 0, 0, 0, 0], &mut first);
    let b = apply_fruit(&mut happy, [1, 0, 1, 0, 0, 0, 0], &mut second);
    assert_eq!(
        (
            ordinary.skills[0].experience,
            ordinary.skills[0].level,
            ordinary.skills[0].points
        ),
        (0, 11, 417)
    );
    assert_eq!(happy.skills[0].points, 418);
    assert_eq!(a.skills[0].random_roll, Some(0));
    assert_eq!(a.random_calls, 1);
    assert_eq!(b.random_calls, 1);
    assert_eq!(a.seed_after, b.seed_after);
    assert!(a.skills[1..].iter().all(|change| !change.leveled_up));
}

#[test]
fn native_max_level_skips_rng_while_level_98_can_reach_99_and_points_cap() {
    let mut state = NativeGrowth {
        mood: 200,
        belly: 200,
        skills: [NativeSkill {
            experience: 99,
            grade: 5,
            level: 99,
            points: 9990,
        }; 5],
    };
    state.skills[2].level = 98;
    let mut rng = NativeRng::new(0);
    let change = apply_fruit(&mut state, [0, 0, 1, 1, 1, 1, 1], &mut rng);
    assert_eq!(change.random_calls, 1);
    assert_eq!(
        state.skills[2],
        NativeSkill {
            experience: 9,
            grade: 5,
            level: 99,
            points: 9999
        }
    );
    for index in [0, 1, 3, 4] {
        assert_eq!(state.skills[index].experience, 99);
        assert_eq!(state.skills[index].points, 9990);
    }
}

#[test]
fn native_branch_is_not_an_added_multi_level_loop_and_byte_stores_wrap() {
    let mut state = NativeGrowth::default();
    let mut rng = NativeRng::new(0);
    let result = apply_fruit(&mut state, [0, 0, 127, 0, 0, 0, 0], &mut rng);
    assert_eq!(state.skills[0].experience, (1270_u16 - 100) as u8);
    assert_eq!(state.skills[0].level, 1);
    assert_eq!(result.random_calls, 1);
}

#[test]
fn raw_native_fields_are_big_endian_and_opaque_bytes_survive_roundtrip() {
    let mut record = [0x5a; 0x120];
    let original = record;
    let state = NativeGrowth {
        mood: 100,
        belly: 180,
        skills: std::array::from_fn(|i| NativeSkill {
            experience: i as u8,
            grade: (i + 1) as u8,
            level: (i + 2) as u8,
            points: (0x1234 + i) as u16,
        }),
    };
    state.write_raw_fields(&mut record).unwrap();
    assert_eq!(&record[0x90..0x92], &[0x12, 0x34]);
    assert_eq!(NativeGrowth::read_raw_fields(&record).unwrap(), state);
    for index in 0..record.len() {
        if index == 0x73
            || index == 0x74
            || (0x78..0x7d).contains(&index)
            || (0x80..0x85).contains(&index)
            || (0x88..0x8d).contains(&index)
            || (0x90..0x9a).contains(&index)
        {
            continue;
        }
        assert_eq!(record[index], original[index], "opaque byte {index:x}");
    }
    assert!(NativeGrowth::read_raw_fields(&record[..0x99]).is_err());
    let mut short = [0x77; 0x99];
    let before = short;
    assert!(state.write_raw_fields(&mut short).is_err());
    assert_eq!(short, before);
}

#[test]
fn checked_boundary_rejects_each_invalid_field_without_touching_stats_or_rng() {
    let valid = NativeGrowth {
        mood: 200,
        belly: 200,
        skills: [NativeSkill {
            experience: 99,
            grade: 5,
            level: 99,
            points: 9999,
        }; 5],
    };
    valid.validate_gameplay_bounds().unwrap();
    for field in ["mood", "belly", "experience", "grade", "level", "points"] {
        let mut state = valid;
        match field {
            "mood" => state.mood = 201,
            "belly" => state.belly = 201,
            "experience" => state.skills[4].experience = 100,
            "grade" => state.skills[4].grade = 6,
            "level" => state.skills[4].level = 100,
            "points" => state.skills[4].points = 10_000,
            _ => unreachable!(),
        }
        let before = state;
        let mut rng = NativeRng::new(u32::MAX);
        let error = apply_fruit_checked(&mut state, [1; 7], &mut rng).unwrap_err();
        assert_eq!(error.field, field);
        assert_eq!(
            error.skill,
            if matches!(field, "mood" | "belly") {
                None
            } else {
                Some(4)
            }
        );
        assert_eq!(state, before);
        assert_eq!(rng.state(), u32::MAX);
    }
}

#[test]
fn checked_boundary_commits_success_and_rejects_invalid_result_atomically() {
    let mut state = NativeGrowth::default();
    let mut rng = NativeRng::new(42);
    let before = state;
    // Native single-overflow behavior leaves an out-of-range 146 experience.
    assert!(apply_fruit_checked(&mut state, [0, 0, 127, 0, 0, 0, 0], &mut rng).is_err());
    assert_eq!(state, before);
    assert_eq!(rng.state(), 42);
    state.skills[0].experience = 90;
    let change = apply_fruit_checked(&mut state, [0, 0, 1, 0, 0, 0, 0], &mut rng).unwrap();
    assert_eq!(state.skills[0].level, 1);
    assert_eq!(state.skills[0].experience, 0);
    assert_eq!(change.random_calls, 1);
    assert_eq!(change.seed_after, rng.state());
    assert_ne!(rng.state(), 42);
}

#[test]
fn level_99_negative_fruit_can_reduce_experience_without_losing_level_or_points() {
    let mut state = NativeGrowth {
        mood: 180,
        belly: 100,
        skills: [NativeSkill {
            experience: 99,
            grade: 5,
            level: 99,
            points: 9999,
        }; 5],
    };
    let mut rng = NativeRng::new(1234);
    let change = apply_fruit_checked(&mut state, [0, 0, -1, -128, 0, 1, 0], &mut rng).unwrap();
    assert_eq!(state.skills[0].experience, 89);
    assert_eq!(state.skills[1].experience, 0);
    assert_eq!(state.skills[3].experience, 99);
    assert!(
        state
            .skills
            .iter()
            .all(|skill| skill.level == 99 && skill.points == 9999)
    );
    assert_eq!(change.random_calls, 0);
    assert_eq!(rng.state(), 1234);
}
