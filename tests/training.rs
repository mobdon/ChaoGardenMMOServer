//! Adapted AFK host coverage uses disposable canonical profiles and a trusted
//! host clock. No client may submit these timestamps or claimed stat gains.
use chao_game_server::{
    care::{CareState, MAX_HISTORY},
    game::{Event, Game, Race},
    service::{AppState, Config},
    training,
};
use serde_json::{Value, json};
use std::{
    fs,
    sync::{Arc, Barrier},
};
use uuid::Uuid;

const NOW: u64 = 1_800_000_000_000;

fn request(revision: u64, intent: Value) -> Value {
    json!({"type":"care","requestId":Uuid::new_v4().to_string(),"expectedRevision":revision,"intent":intent})
}
fn start(revision: u64, stat: &str, duration: u64) -> Value {
    request(
        revision,
        json!({"kind":"training_start","stat":stat,"durationMs":duration}),
    )
}
fn close(revision: u64, kind: &str, job: u64) -> Value {
    request(revision, json!({"kind":kind,"jobId":job}))
}
fn fixture(enter: bool) -> (tempfile::TempDir, AppState, String) {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let id = {
        let mut game = state.game.lock().unwrap();
        let session = game.create_session(&json!({"name":"AFK fixture"})).unwrap();
        let id = session["player"]["id"].as_str().unwrap().to_owned();
        game.connect(&id);
        state.persist(&mut game).unwrap();
        if enter {
            let result = state
                .care_action_at(
                    &mut game,
                    &id,
                    &request(0, json!({"kind":"tiny_enter"})),
                    NOW,
                )
                .unwrap();
            assert_eq!(result["accepted"], true);
        }
        id
    };
    (directory, state, id)
}
fn events(game: &Game) -> Value {
    json!(
        game.events
            .iter()
            .map(|event| json!({"target":event.target,"message":event.message}))
            .collect::<Vec<_>>()
    )
}
fn saved(directory: &std::path::Path, id: &str) -> Value {
    let profiles: Value =
        serde_json::from_slice(&fs::read(directory.join("profiles.json")).unwrap()).unwrap();
    profiles
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap()
        .clone()
}
fn rejected(
    state: &AppState,
    game: &mut Game,
    id: &str,
    message: &Value,
    now: u64,
    expected: &str,
) {
    let before = game.players[id].profile();
    let event_before = events(game);
    let dirty = game.dirty;
    let result = state.care_action_at(game, id, message, now).unwrap();
    assert_eq!(result["accepted"], false, "{message}");
    assert_eq!(result["code"], expected, "{message}");
    assert_eq!(game.players[id].profile(), before, "{message}");
    assert_eq!(events(game), event_before);
    assert_eq!(game.dirty, dirty);
}

#[test]
fn start_requires_explicit_tiny_entry_and_accepts_only_five_stats_and_minute_durations() {
    let (_directory, state, id) = fixture(false);
    let mut game = state.game.lock().unwrap();
    rejected(
        &state,
        &mut game,
        &id,
        &start(0, "run", 60_000),
        NOW,
        "activity-conflict",
    );
    for stat in ["luck", "intelligence", "speed", "Run", ""] {
        rejected(
            &state,
            &mut game,
            &id,
            &start(0, stat, 60_000),
            NOW,
            "invalid-request",
        );
    }
    for duration in [0, 59_999, 60_001, 28_800_001, u64::MAX] {
        rejected(
            &state,
            &mut game,
            &id,
            &start(0, "run", duration),
            NOW,
            "invalid-request",
        );
    }
    drop(game);
    for stat in ["swim", "fly", "run", "power", "stamina"] {
        let (_directory, state, id) = fixture(true);
        let mut game = state.game.lock().unwrap();
        let before = game.players[&id].stats.clone();
        let result = state
            .care_action_at(&mut game, &id, &start(1, stat, 60_000), NOW)
            .unwrap();
        assert_eq!(result["accepted"], true, "{stat}");
        assert_eq!(game.players[&id].stats, before);
        let view = training::public(&game.players[&id], NOW);
        assert_eq!(view["job"]["stat"], stat);
        assert_eq!(view["job"]["phase"], "training");
        assert_eq!(view["job"]["deadlineServerMs"], NOW + 60_000);
        assert_eq!(view["job"]["projectedPoints"], 0);
        assert_eq!(view["policy"]["pointsPerTick"], 1);
        assert_eq!(view["policy"]["costRings"], 0);
        assert!(game.players[&id].watch.is_none());
        assert_eq!(
            game.players[&id].care.activity_lock.as_deref(),
            Some("training:afk:1")
        );
    }
}

#[test]
fn forged_clock_target_policy_reward_extra_envelope_fields_and_noninteger_durations_are_rejected() {
    let (_directory, state, id) = fixture(true);
    let mut game = state.game.lock().unwrap();
    for (field, value) in [
        ("now", json!(NOW + 60_000)),
        ("serverNowMs", json!(NOW)),
        ("targetChaoId", json!("another-chao")),
        ("points", json!(9999)),
        ("policy", json!({"pointsPerTick":9999})),
        ("durationMs", json!(60_000.5)),
    ] {
        let mut message = start(1, "run", 60_000);
        message["intent"][field] = value;
        rejected(&state, &mut game, &id, &message, NOW, "invalid-request");
    }
    for field in ["serverNowMs", "rings", "targetChaoId", "award"] {
        let mut message = start(1, "run", 60_000);
        message[field] = json!(9999);
        rejected(&state, &mut game, &id, &message, NOW, "invalid-request");
    }
}

#[test]
fn cancel_and_settle_obey_clock_readiness_uuid_replay_and_revision_serialization() {
    let (_directory, state, id) = fixture(true);
    let mut game = state.game.lock().unwrap();
    let begin = start(1, "run", 60_000);
    let started = state.care_action_at(&mut game, &id, &begin, NOW).unwrap();
    assert_eq!(started["accepted"], true);
    assert_eq!(
        state
            .care_action_at(&mut game, &id, &begin, NOW + 1)
            .unwrap()["replayed"],
        true
    );
    let mut conflict = begin.clone();
    conflict["intent"]["stat"] = json!("fly");
    rejected(
        &state,
        &mut game,
        &id,
        &conflict,
        NOW,
        "request-id-conflict",
    );
    rejected(
        &state,
        &mut game,
        &id,
        &start(1, "fly", 60_000),
        NOW,
        "stale-revision",
    );
    rejected(
        &state,
        &mut game,
        &id,
        &start(2, "fly", 60_000),
        NOW,
        "activity-conflict",
    );
    rejected(
        &state,
        &mut game,
        &id,
        &close(2, "training_settle", 1),
        NOW - 1,
        "training-clock-error",
    );
    rejected(
        &state,
        &mut game,
        &id,
        &close(2, "training_settle", 1),
        NOW + 59_999,
        "training-not-ready",
    );
    rejected(
        &state,
        &mut game,
        &id,
        &close(2, "training_cancel", 1),
        NOW + 60_000,
        "training-ready-settle",
    );
    rejected(
        &state,
        &mut game,
        &id,
        &close(2, "training_cancel", 2),
        NOW + 1,
        "activity-conflict",
    );
    let cancel = close(2, "training_cancel", 1);
    let canceled = state
        .care_action_at(&mut game, &id, &cancel, NOW + 59_999)
        .unwrap();
    assert_eq!(canceled["effects"]["training"]["awardedPoints"], 0);
    assert!(game.players[&id].watch.is_some());
    assert!(game.players[&id].care.activity_lock.is_none());
    assert_eq!(
        state
            .care_action_at(&mut game, &id, &cancel, NOW + 60_000)
            .unwrap()["replayed"],
        true
    );
    assert_eq!(
        state
            .care_action_at(&mut game, &id, &start(3, "run", 28_800_000), NOW + 60_000)
            .unwrap()["accepted"],
        true
    );
    let settle = close(4, "training_settle", 2);
    let result = state
        .care_action_at(&mut game, &id, &settle, NOW + 100_000_000)
        .unwrap();
    assert_eq!(result["effects"]["training"]["awardedPoints"], 480);
    assert_eq!(result["effects"]["training"]["appliedPoints"], 480);
    assert_eq!(game.players[&id].stats["run"].points, 480);
    assert_eq!(game.players[&id].care.revision, 5);
    assert_eq!(game.players[&id].care.history.len(), 5);
    assert!(game.players[&id].watch.is_some());
    assert_eq!(
        state
            .care_action_at(&mut game, &id, &settle, NOW + 100_000_001)
            .unwrap()["replayed"],
        true
    );
    assert_eq!(game.players[&id].stats["run"].points, 480);
    assert_eq!(game.players[&id].care.history.len(), 5);
}

#[test]
fn active_training_blocks_care_shop_games_races_mech_ai_and_player_movement() {
    let (_directory, state, id) = fixture(true);
    let mut game = state.game.lock().unwrap();
    assert_eq!(
        state
            .care_action_at(&mut game, &id, &start(1, "run", 60_000), NOW)
            .unwrap()["accepted"],
        true
    );
    for intent in [
        json!({"kind":"pet"}),
        json!({"kind":"tiny_purchase","itemId":"garden-fruit"}),
        json!({"kind":"consume","itemId":"garden-fruit"}),
        json!({"kind":"tiny_enter"}),
    ] {
        rejected(
            &state,
            &mut game,
            &id,
            &request(2, intent),
            NOW,
            "activity-conflict",
        );
    }
    let position = (
        game.players[&id].x,
        game.players[&id].y,
        game.players[&id].z,
    );
    for action in [
        "tiny-start",
        "start-race",
        "karate-start",
        "mount-dark-walker",
        "watch-start",
        "watch-stop",
    ] {
        game.time += 100.;
        game.action(
            &id,
            &json!({"type":"action","action":action,"game":"memory"}),
        );
        let p = &game.players[&id];
        assert!(
            p.tiny_game.is_none()
                && p.race.is_none()
                && p.karate.is_none()
                && p.vehicle.is_none()
                && p.watch.is_none()
        );
    }
    game.set_input(&id, &json!({"dx":1,"dz":1,"jump":true,"boost":true}));
    for _ in 0..20 {
        game.tick(0.05);
    }
    let p = &game.players[&id];
    assert_eq!((p.x, p.y, p.z), position);
    assert_eq!(p.input.dx, 0.);
    assert_eq!(p.input.dz, 0.);
    assert!(!p.input.jump && !p.input.boost);
    assert!(p.watch.is_none());
    assert_eq!(p.stats["run"].points, 0);
    assert!(training::public(p, NOW)["eligibility"]["canStart"] == false);
}

fn failed_save_restores(
    directory: &std::path::Path,
    state: &AppState,
    game: &mut Game,
    id: &str,
    message: &Value,
    now: u64,
) {
    let before = game.players[id].profile();
    let before_input = (game.players[id].input.dx, game.players[id].input.dz);
    let before_events = events(game);
    let dirty = game.dirty;
    let disk = fs::read(directory.join("profiles.json")).unwrap();
    let blocked = directory.join("profiles.json.tmp");
    fs::create_dir(&blocked).unwrap();
    assert!(state.care_action_at(game, id, message, now).is_err());
    assert_eq!(game.players[id].profile(), before);
    assert_eq!(
        (game.players[id].input.dx, game.players[id].input.dz),
        before_input
    );
    assert_eq!(events(game), before_events);
    assert_eq!(game.dirty, dirty);
    assert_eq!(fs::read(directory.join("profiles.json")).unwrap(), disk);
    fs::remove_dir(blocked).unwrap();
    assert_eq!(
        state.care_action_at(game, id, message, now).unwrap()["accepted"],
        true
    );
}

#[test]
fn start_cancel_and_settle_roll_back_job_lock_stats_history_receipts_events_and_disk_on_save_failure()
 {
    let (directory, state, id) = fixture(true);
    let mut game = state.game.lock().unwrap();
    game.events.push(Event {
        target: Some(id.clone()),
        message: json!({"kind":"existing"}),
    });
    failed_save_restores(
        directory.path(),
        &state,
        &mut game,
        &id,
        &start(1, "run", 60_000),
        NOW,
    );
    failed_save_restores(
        directory.path(),
        &state,
        &mut game,
        &id,
        &close(2, "training_cancel", 1),
        NOW + 1,
    );
    failed_save_restores(
        directory.path(),
        &state,
        &mut game,
        &id,
        &start(3, "run", 60_000),
        NOW + 1,
    );
    failed_save_restores(
        directory.path(),
        &state,
        &mut game,
        &id,
        &close(4, "training_settle", 2),
        NOW + 60_001,
    );
    assert_eq!(game.players[&id].stats["run"].points, 1);
    assert_eq!(game.players[&id].care.history.len(), 5);
}

#[test]
fn offline_job_resumes_after_restart_settles_automatically_once_and_survives_two_more_restarts() {
    let (directory, state, id) = fixture(true);
    {
        let mut game = state.game.lock().unwrap();
        assert_eq!(
            state
                .care_action_at(&mut game, &id, &start(1, "fly", 120_000), NOW)
                .unwrap()["accepted"],
            true
        );
        game.disconnect(&id);
        assert_eq!(
            state
                .settle_ready_training_at(&mut game, NOW + 119_999)
                .unwrap(),
            0
        );
    }
    let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    {
        let mut game = restarted.game.lock().unwrap();
        let p = &game.players[&id];
        assert_eq!(p.connections, 0);
        assert!(p.watch.is_none());
        assert_eq!(
            training::public(p, NOW + 30_000)["job"]["remainingMs"],
            90_000
        );
        assert_eq!(
            restarted
                .settle_ready_training_at(&mut game, NOW + 120_000)
                .unwrap(),
            1
        );
        let p = &game.players[&id];
        assert_eq!(p.stats["fly"].points, 2);
        assert!(p.watch.is_some() && p.care.activity_lock.is_none());
        assert_eq!(p.care.history.last().unwrap().kind, "training_settle");
        assert_eq!(p.care.history.last().unwrap().at, NOW + 120_000);
        assert_eq!(
            game.events
                .iter()
                .filter(|event| event.message["kind"] == "training-result")
                .count(),
            1
        );
        assert_eq!(
            restarted
                .settle_ready_training_at(&mut game, NOW + 120_001)
                .unwrap(),
            0
        );
    }
    let disk = fs::read(directory.path().join("profiles.json")).unwrap();
    for _ in 0..2 {
        let next = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
        let mut game = next.game.lock().unwrap();
        assert_eq!(
            next.settle_ready_training_at(&mut game, NOW + 999_999)
                .unwrap(),
            0
        );
        assert_eq!(game.players[&id].stats["fly"].points, 2);
        assert_eq!(game.players[&id].care.receipts.len(), 3);
        assert!(game.players[&id].watch.is_some());
        assert_eq!(
            fs::read(directory.path().join("profiles.json")).unwrap(),
            disk
        );
    }
}

#[test]
fn automatic_settlement_save_failure_keeps_ready_job_and_emits_no_result_until_durable() {
    let (directory, state, id) = fixture(true);
    let mut game = state.game.lock().unwrap();
    assert_eq!(
        state
            .care_action_at(&mut game, &id, &start(1, "stamina", 60_000), NOW)
            .unwrap()["accepted"],
        true
    );
    game.disconnect(&id);
    let before = game.players[&id].profile();
    let disk = fs::read(directory.path().join("profiles.json")).unwrap();
    fs::create_dir(directory.path().join("profiles.json.tmp")).unwrap();
    assert!(
        state
            .settle_ready_training_at(&mut game, NOW + 60_000)
            .is_err()
    );
    assert_eq!(game.players[&id].profile(), before);
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        disk
    );
    assert!(
        !game
            .events
            .iter()
            .any(|event| event.message["kind"] == "training-result")
    );
    fs::remove_dir(directory.path().join("profiles.json.tmp")).unwrap();
    assert_eq!(
        state
            .settle_ready_training_at(&mut game, NOW + 60_000)
            .unwrap(),
        1
    );
    assert_eq!(game.players[&id].stats["stamina"].points, 1);
    assert_eq!(
        game.events
            .iter()
            .filter(|event| event.message["kind"] == "training-result")
            .count(),
        1
    );
}

#[test]
fn conflicting_runtime_activity_postpones_automatic_settlement_without_unlock_or_award() {
    let (_directory, state, id) = fixture(true);
    let mut game = state.game.lock().unwrap();
    assert_eq!(
        state
            .care_action_at(&mut game, &id, &start(1, "run", 60_000), NOW)
            .unwrap()["accepted"],
        true
    );
    game.players.get_mut(&id).unwrap().race = Some(Race {
        started_at: 0.,
        next_checkpoint: 0,
        course_id: None,
    });
    assert_eq!(
        state
            .settle_ready_training_at(&mut game, NOW + 60_000)
            .unwrap(),
        0
    );
    assert_eq!(game.players[&id].stats["run"].points, 0);
    assert_eq!(
        game.players[&id].care.activity_lock.as_deref(),
        Some("training:afk:1")
    );
    game.players.get_mut(&id).unwrap().race = None;
    assert_eq!(
        state
            .settle_ready_training_at(&mut game, NOW + 60_000)
            .unwrap(),
        1
    );
}

#[test]
fn two_devices_same_uuid_or_revision_cannot_start_or_settle_twice() {
    for same_uuid in [true, false] {
        let (_directory, state, id) = fixture(true);
        let begin = start(1, "power", 60_000);
        let requests = [
            begin.clone(),
            if same_uuid {
                begin
            } else {
                start(1, "fly", 60_000)
            },
        ];
        let barrier = Arc::new(Barrier::new(3));
        let results = std::thread::scope(|scope| {
            let jobs: Vec<_> = requests
                .iter()
                .map(|message| {
                    let barrier = barrier.clone();
                    let state = &state;
                    let id = &id;
                    scope.spawn(move || {
                        barrier.wait();
                        let mut game = state.game.lock().unwrap();
                        state.care_action_at(&mut game, id, message, NOW).unwrap()
                    })
                })
                .collect();
            barrier.wait();
            jobs.into_iter()
                .map(|job| job.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            results
                .iter()
                .filter(|r| r["accepted"] == true && r["replayed"] == false)
                .count(),
            1
        );
        assert_eq!(
            results.iter().filter(|r| r["replayed"] == true).count(),
            usize::from(same_uuid)
        );
        let settle = close(2, "training_settle", 1);
        let barrier = Arc::new(Barrier::new(3));
        let results = std::thread::scope(|scope| {
            let jobs: Vec<_> = (0..2)
                .map(|_| {
                    let barrier = barrier.clone();
                    let state = &state;
                    let id = &id;
                    let settle = &settle;
                    scope.spawn(move || {
                        barrier.wait();
                        let mut game = state.game.lock().unwrap();
                        state
                            .care_action_at(&mut game, id, settle, NOW + 60_000)
                            .unwrap()
                    })
                })
                .collect();
            barrier.wait();
            jobs.into_iter()
                .map(|job| job.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|r| r["replayed"] == false).count(), 1);
        let game = state.game.lock().unwrap();
        assert_eq!(game.players[&id].care.revision, 3);
        assert_eq!(game.players[&id].care.history.len(), 3);
        assert_eq!(
            game.players[&id]
                .stats
                .values()
                .map(|s| s.points)
                .sum::<u32>(),
            1
        );
    }
}

#[test]
fn applied_points_clamp_and_perfect_admin_stats_levels_grades_hidden_stats_and_metadata_survive() {
    for admin in [false, true] {
        let (directory, state, id) = fixture(true);
        let mut game = state.game.lock().unwrap();
        let p = game.players.get_mut(&id).unwrap();
        p.admin = admin;
        p.stats.get_mut("run").unwrap().points = if admin { 9999 } else { 9998 };
        let before = p.stats.clone();
        assert_eq!(
            state
                .care_action_at(&mut game, &id, &start(1, "run", 120_000), NOW)
                .unwrap()["accepted"],
            true
        );
        let result = state
            .care_action_at(
                &mut game,
                &id,
                &close(2, "training_settle", 1),
                NOW + 120_000,
            )
            .unwrap();
        assert_eq!(result["effects"]["training"]["awardedPoints"], 2);
        assert_eq!(
            result["effects"]["training"]["appliedPoints"],
            if admin { 0 } else { 1 }
        );
        for (name, original) in &before {
            let current = &game.players[&id].stats[name];
            assert_eq!(current.level, original.level);
            assert_eq!(current.grade, original.grade);
            assert_eq!(
                current.points,
                if name == "run" { 9999 } else { original.points }
            );
        }
        assert_eq!(saved(directory.path(), &id)["admin"], admin);
    }
    let (directory, state, id) = fixture(true);
    let mut profile = saved(directory.path(), &id);
    profile["futureNamespace"] = json!({"opaque":[1,2,3]});
    profile["stats"]["run"]["futureMetadata"] = json!({"revision":7});
    profile["gear"]["futureMetadata"] = json!(["retained"]);
    fs::write(
        directory.path().join("profiles.json"),
        serde_json::to_vec(&json!([profile])).unwrap(),
    )
    .unwrap();
    drop(state);
    let next = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let mut game = next.game.lock().unwrap();
    game.connect(&id);
    assert_eq!(
        next.care_action_at(&mut game, &id, &start(1, "run", 60_000), NOW)
            .unwrap()["accepted"],
        true
    );
    assert_eq!(
        next.settle_ready_training_at(&mut game, NOW + 60_000)
            .unwrap(),
        1
    );
    let profile = saved(directory.path(), &id);
    assert_eq!(profile["futureNamespace"], json!({"opaque":[1,2,3]}));
    assert_eq!(
        profile["stats"]["run"]["futureMetadata"],
        json!({"revision":7})
    );
    assert_eq!(profile["gear"]["futureMetadata"], json!(["retained"]));
}

#[test]
fn corrupted_target_policy_lock_opt_in_receipt_or_history_fails_load_without_writing() {
    let (directory, state, id) = fixture(true);
    {
        let mut game = state.game.lock().unwrap();
        assert_eq!(
            state
                .care_action_at(&mut game, &id, &start(1, "run", 60_000), NOW)
                .unwrap()["accepted"],
            true
        );
    }
    let original = saved(directory.path(), &id);
    let mut variants = Vec::new();
    let mut p = original.clone();
    p["care"]["training"]["ledger"]["targetChaoId"] = json!("wrong-target");
    variants.push(p);
    let mut p = original.clone();
    p["care"]["training"]["ledger"]["job"]["policy"]["pointsPerTick"] = json!(2);
    variants.push(p);
    let mut p = original.clone();
    p["care"]["activityLock"] = Value::Null;
    variants.push(p);
    let mut p = original.clone();
    p["care"]["tinyResident"] = json!(false);
    variants.push(p);
    let mut p = original.clone();
    p["care"]["receipts"] = json!({});
    p["care"]["history"] = json!([]);
    variants.push(p);
    let mut p = original.clone();
    p["care"]["history"][0]["result"] = json!({"rings":999});
    variants.push(p);
    for p in variants {
        let bytes = serde_json::to_vec(&json!([p])).unwrap();
        fs::write(directory.path().join("profiles.json"), &bytes).unwrap();
        assert!(AppState::load(Config::local(directory.path().to_owned(), false)).is_err());
        assert_eq!(
            fs::read(directory.path().join("profiles.json")).unwrap(),
            bytes
        );
    }
    fs::write(
        directory.path().join("profiles.json"),
        serde_json::to_vec(&json!([original])).unwrap(),
    )
    .unwrap();
    let next = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    {
        let mut game = next.game.lock().unwrap();
        assert_eq!(
            next.settle_ready_training_at(&mut game, NOW + 60_000)
                .unwrap(),
            1
        );
    }
    let mut closed = saved(directory.path(), &id);
    let receipt_id = closed["care"]["training"]["lastResult"]["receiptId"]
        .as_str()
        .unwrap()
        .to_owned();
    closed["care"]["receipts"]
        .as_object_mut()
        .unwrap()
        .remove(&receipt_id);
    closed["care"]["history"] = json!([]);
    fs::write(
        directory.path().join("profiles.json"),
        serde_json::to_vec(&json!([closed])).unwrap(),
    )
    .unwrap();
    assert!(AppState::load(Config::local(directory.path().to_owned(), false)).is_err());
}

#[test]
fn activity_history_is_real_bounded_private_durable_and_not_manufactured_for_legacy_receipts() {
    let (directory, state, id) = fixture(true);
    let mut game = state.game.lock().unwrap();
    for n in 1..=70 {
        let message = request(n, json!({"kind":"tiny_enter"}));
        let response = state
            .care_action_at(&mut game, &id, &message, NOW + n)
            .unwrap();
        assert_eq!(response["accepted"], true);
        assert_eq!(
            state
                .care_action_at(&mut game, &id, &message, NOW + n + 1)
                .unwrap()["replayed"],
            true
        );
    }
    let p = &game.players[&id];
    assert_eq!(p.care.history.len(), MAX_HISTORY);
    assert_eq!(p.care.receipts.len(), 71);
    assert_eq!(p.care.history[0].at, NOW + 7);
    assert_eq!(p.care.history.last().unwrap().at, NOW + 70);
    assert!(p.public().get("history").is_none());
    assert_eq!(
        game.snapshot(&id)["care"]["history"],
        serde_json::to_value(&p.care.history).unwrap()
    );
    let expected = p.care.history.clone();
    drop(game);
    let next = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    assert_eq!(
        next.game.lock().unwrap().players[&id].care.history,
        expected
    );
    let mut legacy = saved(directory.path(), &id)["care"].clone();
    legacy.as_object_mut().unwrap().remove("history");
    let restored = CareState::restore(Some(&legacy)).unwrap();
    assert!(restored.history.is_empty());
    assert_eq!(restored.receipts.len(), 71);
    let mut invalid = legacy.clone();
    invalid["history"] =
        json!([{ "id":Uuid::new_v4().to_string(),"at":NOW,"kind":"pet","result":{"happiness":2} }]);
    assert!(CareState::restore(Some(&invalid)).is_err());
}

#[test]
fn training_start_rejects_every_unsafe_zone_vehicle_race_minigame_and_disconnected_profile() {
    for scenario in [
        "zone",
        "vehicle",
        "race",
        "minigame",
        "disconnected",
        "foreign-lock",
    ] {
        let (_directory, state, id) = fixture(true);
        let mut game = state.game.lock().unwrap();
        if scenario == "minigame" {
            game.action(
                &id,
                &json!({"type":"action","action":"tiny-start","gameId":"memory"}),
            );
            assert!(game.players[&id].tiny_game.is_some());
        }
        let p = game.players.get_mut(&id).unwrap();
        match scenario {
            "zone" => p.zone = "mission".into(),
            "vehicle" => p.vehicle = Some("dark-chao-walker".into()),
            "race" => {
                p.race = Some(Race {
                    started_at: 0.,
                    next_checkpoint: 0,
                    course_id: None,
                })
            }
            "disconnected" => p.connections = 0,
            "foreign-lock" => p.care.activity_lock = Some("another-activity".into()),
            _ => {}
        }
        rejected(
            &state,
            &mut game,
            &id,
            &start(1, "run", 60_000),
            NOW,
            "activity-conflict",
        );
    }
}

#[test]
fn start_reserves_the_final_closing_revision_and_receipt_slots() {
    let (_directory, state, id) = fixture(true);
    let mut game = state.game.lock().unwrap();
    game.players.get_mut(&id).unwrap().care.revision = u64::MAX - 1;
    rejected(
        &state,
        &mut game,
        &id,
        &start(u64::MAX - 1, "run", 60_000),
        NOW,
        "receipt-capacity",
    );
    game.players.get_mut(&id).unwrap().care.revision = u64::MAX - 2;
    assert_eq!(
        state
            .care_action_at(&mut game, &id, &start(u64::MAX - 2, "run", 60_000), NOW)
            .unwrap()["accepted"],
        true
    );
    assert_eq!(
        state
            .settle_ready_training_at(&mut game, NOW + 60_000)
            .unwrap(),
        1
    );
    assert_eq!(game.players[&id].care.revision, u64::MAX);
    assert_eq!(game.players[&id].stats["run"].points, 1);
}
