//! Explicit local acceptance only: reads an owned, audited installation and
//! writes saves solely inside a fresh TempDir. No native table is distributed.
use chao_game_server::{
    care_catalog::{CareCatalog, NativeItemStats},
    game::{ALL_STATS, Game, Player},
    service::{AppState, Config},
};
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf};
use uuid::Uuid;

fn local_catalog() -> CareCatalog {
    let audit = PathBuf::from(
        env::var_os("CHAO_CARE_AUDIT_DIR")
            .expect("Opt-in native smoke requires CHAO_CARE_AUDIT_DIR"),
    );
    let executable = PathBuf::from(
        env::var_os("CHAO_CARE_EXECUTABLE")
            .expect("Opt-in native smoke requires CHAO_CARE_EXECUTABLE"),
    );
    CareCatalog::load(&audit, &executable)
        .expect("Local native source must pass provenance and PE-row verification")
}

fn fixture(catalog: &CareCatalog) -> (tempfile::TempDir, AppState, String) {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let id = {
        let mut game = state.game.lock().unwrap();
        game.care_catalog = catalog.clone();
        let session = game
            .create_session(&json!({"name":"Local native care smoke"}))
            .unwrap();
        let id = session["player"]["id"].as_str().unwrap().to_owned();
        game.connect(&id);
        let player = game.players.get_mut(&id).unwrap();
        assert!(!player.admin);
        player.energy = 50.;
        player.hunger = 50.;
        player.happiness = 50.;
        for name in ALL_STATS {
            let stat = player.stats.get_mut(name).unwrap();
            stat.points = 100;
            stat.level = 7;
            stat.grade = 2;
        }
        id
    };
    (directory, state, id)
}

fn request(revision: u64, intent: Value) -> Value {
    json!({"type":"care","requestId":Uuid::new_v4().to_string(),
        "expectedRevision":revision,"intent":intent})
}

fn assert_source_effect(
    player: &Player,
    before: &Value,
    raw: &NativeItemStats,
    native_id: u8,
    result: &Value,
) {
    assert_eq!(result["accepted"], true);
    assert_eq!(result["replayed"], false);
    for (name, delta) in raw.skill_deltas() {
        let prior = before["stats"][name]["points"].as_i64().unwrap();
        let expected = (prior + i64::from(delta)).clamp(0, 9999);
        assert_eq!(
            i64::from(player.stats[name].points),
            expected,
            "{name} signed points"
        );
        assert_eq!(
            result["effects"]["points"][name],
            expected - prior,
            "{name} applied delta"
        );
        assert_eq!(
            player.care.raw_totals[name],
            i64::from(delta),
            "{name} raw evidence"
        );
        assert_eq!(
            json!(player.stats[name].level),
            before["stats"][name]["level"]
        );
        assert_eq!(
            json!(player.stats[name].grade),
            before["stats"][name]["grade"]
        );
    }
    let old_happiness = before["happiness"].as_f64().unwrap();
    let old_satiety = before["hunger"].as_f64().unwrap();
    let expected_happiness = (old_happiness + f64::from(raw.mood)).clamp(0., 100.);
    let expected_satiety = (old_satiety + f64::from(raw.belly)).clamp(0., 100.);
    assert_eq!(player.happiness, expected_happiness);
    assert_eq!(player.hunger, expected_satiety);
    assert_eq!(
        result["effects"]["happiness"],
        expected_happiness - old_happiness
    );
    assert_eq!(result["effects"]["satiety"], expected_satiety - old_satiety);
    assert_eq!(json!(player.energy), before["energy"]);
    assert_eq!(player.care.raw_totals["mood"], i64::from(raw.mood));
    assert_eq!(player.care.raw_totals["belly"], i64::from(raw.belly));
    assert_eq!(
        player.care.raw_totals["unknown18"],
        i64::from(raw.unknown_18)
    );
    assert_eq!(result["effects"]["unknown18Applied"], false);
    assert_eq!(result["effects"]["levelsChanged"], false);
    assert_eq!(result["effects"]["gradesChanged"], false);
    assert_eq!(json!(player.form), before["form"]);
    assert_eq!(json!(player.alignment), before["alignment"]);
    assert!(!player.admin);
    assert_eq!(
        player.care.evidence.species_history,
        if native_id < 21 { 1 << native_id } else { 0 }
    );
    assert_eq!(player.care.evidence.animal_uses, u64::from(native_id < 21));
    assert_eq!(player.care.evidence.drive_uses, u64::from(native_id >= 21));
    let evidence = player.care.evidence.public();
    assert_eq!(evidence["nativeEvolution"]["ready"], false);
    assert_eq!(evidence["learnedBehavior"]["enabled"], false);
    assert_eq!(evidence["animalAppearance"]["enabled"], false);
}

#[test]
#[ignore = "requires explicit CHAO_CARE_AUDIT_DIR and CHAO_CARE_EXECUTABLE for owned local native data"]
fn source_verified_signed_effects_and_exact_retry_survive_durable_restart() {
    let catalog = local_catalog();
    let raw = catalog.item(0).unwrap().raw;
    assert!(
        raw.power < 0,
        "Source-backed Penguin tradeoff must remain negative"
    );
    let (directory, state, id) = fixture(&catalog);
    let message = request(0, json!({"kind":"consume","itemId":"sa2:0"}));
    let (result, expected_care, expected_profile) = {
        let mut game = state.game.lock().unwrap();
        // Server-created fixture ownership; no client may seed this inventory.
        game.players
            .get_mut(&id)
            .unwrap()
            .care
            .inventory
            .insert("sa2:0".into(), 1);
        game.dirty = true;
        state.persist(&mut game).unwrap();
        assert_eq!(game.players[&id].care.evidence.species_history, 0);
        let before = game.players[&id].public();
        let result = state.care_action(&mut game, &id, &message).unwrap();
        let player = &game.players[&id];
        assert_source_effect(player, &before, &raw, 0, &result);
        assert!(result["effects"]["points"]["power"].as_i64().unwrap() < 0);
        assert!(player.care.inventory.is_empty());
        (result, player.care.clone(), player.profile())
    };
    let saved = fs::read(directory.path().join("profiles.json")).unwrap();
    let restored = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let mut game = restored.game.lock().unwrap();
    assert_eq!(game.care_catalog.source_sha256(), catalog.source_sha256());
    assert_eq!(game.players[&id].connections, 0);
    assert_eq!(game.players[&id].profile(), expected_profile);
    let replay = restored.care_action(&mut game, &id, &message).unwrap();
    assert_eq!(replay["accepted"], true);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["effects"], result["effects"]);
    assert_eq!(replay["revision"], result["revision"]);
    assert_eq!(game.players[&id].care, expected_care);
    assert_eq!(game.players[&id].profile(), expected_profile);
    assert!(game.players[&id].care.inventory.is_empty());
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        saved
    );
}

fn action(game: &mut Game, id: &str, name: &str) {
    game.time += 100.;
    game.action(id, &json!({"type":"action","action":name}));
}

#[test]
#[ignore = "requires explicit CHAO_CARE_AUDIT_DIR and CHAO_CARE_EXECUTABLE for owned local native data"]
fn real_combat_drop_claim_and_care_use_the_same_source_verified_inventory() {
    let catalog = local_catalog();
    let (directory, state, id) = fixture(&catalog);
    let (message, result, expected_care) = {
        let mut game = state.game.lock().unwrap();
        action(&mut game, &id, "mission");
        assert_eq!(game.players[&id].zone, "mission");
        // Isolate one ordinary combat target in this temporary test world.
        let mission = game.mission.as_mut().unwrap();
        for enemy in &mut mission.enemies {
            enemy.x = 20.;
            enemy.z = 20.;
        }
        mission.enemies[0].x = 0.;
        mission.enemies[0].z = 0.;
        mission.enemies[0].hp = 1.;
        let player = game.players.get_mut(&id).unwrap();
        player.x = 0.;
        player.y = 0.;
        player.z = 0.;
        player.ground_y = 0.;
        player.jump_offset = 0.;
        action(&mut game, &id, "attack");
        assert_eq!(game.mission.as_ref().unwrap().enemies[0].hp, 0.);
        assert_eq!(game.players[&id].care.pickups.len(), 1);
        assert!(game.players[&id].care.inventory.is_empty());
        assert_eq!(game.players[&id].care.evidence.species_history, 0);
        let pickup = game.players[&id]
            .care
            .pickups
            .values()
            .next()
            .unwrap()
            .clone();
        let advertised = game.snapshot(&id)["care"]["pickups"].clone();
        assert_eq!(advertised.as_array().unwrap().len(), 1);
        assert_eq!(advertised[0]["id"], pickup.id);
        let claim = request(
            game.players[&id].care.revision,
            json!({"kind":"pickup","pickupId":pickup.id}),
        );
        assert_eq!(
            state.care_action(&mut game, &id, &claim).unwrap()["accepted"],
            true
        );
        let item_id = format!("sa2:{}", pickup.native_id);
        assert_eq!(game.players[&id].care.inventory[&item_id], 1);
        assert_eq!(game.players[&id].care.evidence.species_history, 0);
        assert!(game.players[&id].care.pickups.is_empty());
        action(&mut game, &id, "hub");
        let before = game.players[&id].public();
        let raw = catalog.item(pickup.native_id).unwrap().raw;
        let message = request(
            game.players[&id].care.revision,
            json!({"kind":"consume","itemId":item_id}),
        );
        let result = state.care_action(&mut game, &id, &message).unwrap();
        let player = &game.players[&id];
        assert_source_effect(player, &before, &raw, pickup.native_id, &result);
        assert!(player.care.inventory.is_empty());
        (message, result, player.care.clone())
    };
    let saved = fs::read(directory.path().join("profiles.json")).unwrap();
    let restored = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let mut game = restored.game.lock().unwrap();
    assert_eq!(game.players[&id].care, expected_care);
    let replay = restored.care_action(&mut game, &id, &message).unwrap();
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["effects"], result["effects"]);
    assert_eq!(game.players[&id].care, expected_care);
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        saved
    );
}
