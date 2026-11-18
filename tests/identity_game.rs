use chao_game_server::game::{Game, hash_token};
use chao_game_server::service::{AppState, Config};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};

fn legacy_profile() -> Value {
    json!({
        "id":"legacy-chao", "tokenHash":hash_token("existing-desktop-secret"),
        "name":"Earned Chao", "admin":false,
        "stats":{
            "swim":{"grade":3,"level":42,"points":1200},
            "fly":{"grade":4,"level":38,"points":1100},
            "run":{"grade":5,"level":71,"points":3000},
            "power":{"grade":2,"level":23,"points":900},
            "stamina":{"grade":1,"level":16,"points":400},
            "luck":{"grade":80,"level":20,"points":530},
            "intelligence":{"grade":90,"level":30,"points":740}
        },
        "rings":1734, "alignment":-80.0, "form":"chaos", "happiness":89.0,
        "ownedCosmetics":["eggshell"], "equipped":"eggshell",
        "missionsCompleted":12,"racesCompleted":29,"bestRaceMs":5020.0,
        "raceRecords":{"beginner-crab-pool":{"completions":7,"bestMs":5020.0}},
        "karateRecords":{"beginner":{"completions":4,"bestMs":2080.0}},
        "tinyRecords":{"memory":{"plays":5,"completions":4,"ringsEarned":386,
            "lastResultId":"0897a367-28d1-458a-abd7-fc01de439c3f"}}
    })
}

#[test]
fn migration_preserves_canonical_chao_and_earned_progression_across_restart() {
    let legacy = legacy_profile();
    let game = Game::new(vec![legacy.clone()], BTreeMap::new(), false, 1000.).unwrap();
    assert!(
        game.dirty,
        "legacy migration must be saved before acknowledgement"
    );
    let player = game.player_for_token("existing-desktop-secret").unwrap();
    assert_eq!(player.id, "legacy-chao");
    assert_eq!(player.identity.chao_id, player.id);
    assert!(!player.admin);
    let exported = game.export_profiles();
    for field in [
        "id",
        "tokenHash",
        "name",
        "admin",
        "stats",
        "rings",
        "alignment",
        "form",
        "happiness",
        "ownedCosmetics",
        "equipped",
        "missionsCompleted",
        "racesCompleted",
        "bestRaceMs",
        "raceRecords",
        "karateRecords",
        "tinyRecords",
    ] {
        assert_eq!(
            exported[0][field], legacy[field],
            "migration changed {field}"
        );
    }
    let identity = exported[0]["identity"].clone();
    let restarted = Game::new(exported.clone(), BTreeMap::new(), false, 2000.).unwrap();
    assert!(
        !restarted.dirty,
        "migration must not reissue identities on each restart"
    );
    assert_eq!(restarted.players.len(), 1);
    assert_eq!(restarted.export_profiles(), exported);
    assert_eq!(restarted.export_profiles()[0]["identity"], identity);
    assert_eq!(
        restarted
            .player_for_token("existing-desktop-secret")
            .unwrap()
            .rings,
        1734
    );
    let public = restarted.snapshot("legacy-chao").to_string();
    assert!(!public.contains("existing-desktop-secret"));
    assert!(!public.contains(&hash_token("existing-desktop-secret")));
    assert!(!public.contains("credentialHash"));
    let diagnostic = format!("{player:?}");
    assert!(!diagnostic.contains(&hash_token("existing-desktop-secret")));
}

#[test]
fn revoking_legacy_device_disables_old_token_hash_after_restart() {
    let mut game = Game::new(vec![legacy_profile()], BTreeMap::new(), false, 1000.).unwrap();
    game.players
        .get_mut("legacy-chao")
        .unwrap()
        .identity
        .devices[0]
        .revoked = true;
    assert!(game.player_for_token("existing-desktop-secret").is_none());
    let mut restarted = Game::new(game.export_profiles(), BTreeMap::new(), false, 2000.).unwrap();
    assert!(
        restarted
            .player_for_token("existing-desktop-secret")
            .is_none()
    );
    assert!(
        restarted
            .create_session(&json!({"token":"existing-desktop-secret"}))
            .is_err()
    );
    assert_eq!(restarted.players.len(), 1);
}

#[test]
fn migration_keeps_existing_owner_role_chaos_form_and_equipment() {
    let mut original = Game::new(vec![], BTreeMap::new(), true, 1000.).unwrap();
    let session = original.create_session(&json!({"name":"Owner"})).unwrap();
    let token = session["token"].as_str().unwrap();
    let id = session["player"]["id"].as_str().unwrap();
    let owner = original.players.get_mut(id).unwrap();
    owner.rings = 822;
    owner.form = "chaos".into();
    owner.alignment = -100.;
    owner.vehicle = Some("dark-chao-walker".into());
    owner.gear.footwear = Some("shadow-air-shoes".into());
    owner.gear.hands = Some("knuckles-hammer-gloves".into());
    let mut legacy = original.export_profiles();
    legacy[0].as_object_mut().unwrap().remove("identity");
    let migrated = Game::new(legacy.clone(), BTreeMap::new(), false, 2000.).unwrap();
    let restored = migrated.player_for_token(token).unwrap();
    assert!(restored.admin);
    assert_eq!(restored.stats["luck"].grade, 255);
    assert_eq!(restored.stats["intelligence"].points, 9999);
    let mut migrated_profile = migrated.export_profiles()[0].clone();
    migrated_profile.as_object_mut().unwrap().remove("identity");
    assert_eq!(migrated_profile, legacy[0]);
}

#[test]
fn invalid_or_ambiguous_persistent_identity_fails_without_losing_a_profile() {
    let game = Game::new(vec![legacy_profile()], BTreeMap::new(), false, 1000.).unwrap();
    let profiles = game.export_profiles();
    assert!(
        Game::new(
            vec![profiles[0].clone(), profiles[0].clone()],
            BTreeMap::new(),
            false,
            1000.
        )
        .is_err()
    );
    let mut mismatch = profiles[0].clone();
    mismatch["identity"]["chaoId"] = json!("different-chao");
    assert!(Game::new(vec![mismatch], BTreeMap::new(), false, 1000.).is_err());
    let mut invalid = profiles[0].clone();
    invalid["identity"]["schemaVersion"] = json!(999);
    assert!(Game::new(vec![invalid], BTreeMap::new(), false, 1000.).is_err());
    assert!(
        Game::new(
            vec![json!({"id":"missing-credential"})],
            BTreeMap::new(),
            false,
            1000.
        )
        .is_err()
    );
}

#[test]
fn account_and_device_metadata_only_appears_in_private_session_response() {
    let mut game = Game::new(vec![legacy_profile()], BTreeMap::new(), false, 1000.).unwrap();
    let response = game
        .create_session(&json!({"token":"existing-desktop-secret","resumeOnly":true}))
        .unwrap();
    assert_eq!(response["player"]["id"], "legacy-chao");
    assert_eq!(response["player"]["chaoId"], "legacy-chao");
    assert_eq!(response["identity"]["chaoId"], "legacy-chao");
    assert!(response["identity"]["accountId"].is_string());
    assert!(
        !game.players["legacy-chao"]
            .public()
            .to_string()
            .contains("accountId")
    );
    assert!(!response["identity"].to_string().contains("credentialHash"));
}

fn opaque_profile() -> Value {
    let mut profile = legacy_profile();
    profile["nativeMetadata"] = json!({
        "saveVersion":7,"opaqueBytes":[4,1,9],"privateNote":"opaque-private-marker"
    });
    profile["stats"]["run"]["nativeHint"] = json!({"bits":429,"source":"native-save"});
    profile["stats"]["futureSkill"] = json!({"level":80,"unmapped":[1,3,5]});
    profile["tinyRecords"]["memory"]["nativeReceipt"] = json!({"slot":6,"raw":[8,2]});
    profile["tinyRecords"]["futureGame"] = json!({"receipts":["unmodeled-receipt"]});
    profile["gear"] = json!({"footwear":"unowned-gear","futureSlot":{"variant":7}});
    profile["ownedCosmetics"] = json!(["eggshell", "unowned-cosmetic"]);
    profile["resident"] = json!({"enabled":false,"unmapped":"stale-mode"});
    profile["untrustedFutureWallet"] = json!({"rings":999999,"admin":true});
    profile
}

fn assert_opaque_fields(profile: &Value, original: &Value) {
    assert_eq!(profile["nativeMetadata"], original["nativeMetadata"]);
    assert_eq!(
        profile["stats"]["run"]["nativeHint"],
        original["stats"]["run"]["nativeHint"]
    );
    assert_eq!(
        profile["stats"]["futureSkill"],
        original["stats"]["futureSkill"]
    );
    assert_eq!(
        profile["tinyRecords"]["memory"]["nativeReceipt"],
        original["tinyRecords"]["memory"]["nativeReceipt"]
    );
    assert_eq!(
        profile["tinyRecords"]["futureGame"],
        original["tinyRecords"]["futureGame"]
    );
    assert_eq!(
        profile["gear"]["futureSlot"],
        original["gear"]["futureSlot"]
    );
    assert_eq!(
        profile["untrustedFutureWallet"],
        original["untrustedFutureWallet"]
    );
}

#[test]
fn opaque_profile_survives_failed_migration_persistence_and_restart_without_becoming_authority() {
    let directory = tempfile::tempdir().unwrap();
    let original = opaque_profile();
    let bytes = serde_json::to_vec(&vec![original.clone()]).unwrap();
    let path = directory.path().join("profiles.json");
    fs::write(&path, &bytes).unwrap();
    let blocked = directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocked).unwrap();
    assert!(AppState::load(Config::local(directory.path().to_owned(), false)).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::remove_dir(blocked).unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    {
        let mut game = state.game.lock().unwrap();
        let player = game.players.get_mut("legacy-chao").unwrap();
        assert_eq!(player.rings, 1734);
        assert!(!player.admin);
        assert_eq!(player.stats.len(), 7);
        assert!(player.gear.footwear.is_none());
        assert!(player.watch.is_none());
        player.stats.get_mut("run").unwrap().level = 72;
        player.rings = 1700;
        game.dirty = true;
        state.persist(&mut game).unwrap();
        let public = game.players["legacy-chao"].public().to_string();
        let diagnostic = format!("{:?}", game.players["legacy-chao"]);
        for private in [
            "opaque-private-marker",
            "nativeHint",
            "futureSlot",
            "unmodeled-receipt",
            "untrustedFutureWallet",
        ] {
            assert!(!public.contains(private));
            assert!(!diagnostic.contains(private));
        }
    }
    let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_opaque_fields(&saved[0], &original);
    assert_eq!(saved[0]["rings"], 1700);
    assert_eq!(saved[0]["stats"]["run"]["level"], 72);
    assert_eq!(saved[0]["ownedCosmetics"], json!(["eggshell"]));
    assert!(saved[0]["gear"]["footwear"].is_null());
    assert!(saved[0]["resident"].is_null());
    let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let reexported = restarted.game.lock().unwrap().export_profiles();
    assert_opaque_fields(&reexported[0], &original);
    assert_eq!(reexported[0], saved[0]);
}

#[test]
fn opaque_receipt_metadata_survives_projected_tiny_result_and_durable_commit() {
    let directory = tempfile::tempdir().unwrap();
    let mut original = opaque_profile();
    original["tinyRecords"]["rock_paper_scissors"] = json!({
        "plays":2,"completions":1,"ringsEarned":3,
        "lastResultId":"d550c0df-a85b-4cc1-a9e9-a964c142a9af",
        "nativeReceipt":{"slot":9,"bytes":[2,7]}
    });
    let path = directory.path().join("profiles.json");
    fs::write(&path, serde_json::to_vec(&vec![original.clone()]).unwrap()).unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    {
        let mut game = state.game.lock().unwrap();
        game.connect("legacy-chao");
        game.action(
            "legacy-chao",
            &json!({"type":"action","action":"tiny-start","gameId":"rock_paper_scissors"}),
        );
        for _ in 0..301 {
            game.tick(0.1);
        }
        let projected = game.export_profiles();
        assert_eq!(
            projected[0]["tinyRecords"]["rock_paper_scissors"]["plays"],
            3
        );
        assert_eq!(
            game.players["legacy-chao"].tiny_records["rock_paper_scissors"].plays,
            2
        );
        assert_eq!(
            projected[0]["tinyRecords"]["rock_paper_scissors"]["nativeReceipt"],
            original["tinyRecords"]["rock_paper_scissors"]["nativeReceipt"]
        );
        state.persist(&mut game).unwrap();
        assert_eq!(
            game.players["legacy-chao"].tiny_records["rock_paper_scissors"].plays,
            3
        );
    }
    let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_opaque_fields(&saved[0], &original);
    assert_eq!(saved[0]["tinyRecords"]["rock_paper_scissors"]["plays"], 3);
    let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let reexported = restarted.game.lock().unwrap().export_profiles();
    assert_eq!(reexported[0], saved[0]);
    assert_eq!(
        reexported[0]["tinyRecords"]["rock_paper_scissors"]["nativeReceipt"],
        original["tinyRecords"]["rock_paper_scissors"]["nativeReceipt"]
    );
}
