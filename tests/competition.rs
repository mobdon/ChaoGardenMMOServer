use chao_game_server::{
    competition,
    game::Game,
    service::{AppState, Config},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

// Synthetic catalogs exercise the locally extracted schema. No native assets,
// original data rows, credentials, or live profiles are included in this test.
fn race_catalog() -> Value {
    json!({
        "courses":[{
            "id":"beginner-0-1", "name":"Synthetic swim reference", "group":"beginner", "level":1,
            "available":false, "geometryValidated":false,
            "source":{"courseIndex":0,"tableIndex":0,"rivalSetIndex":-1,"raw":[-1,0,0,0,-1]}
        },{
            "id":"beginner-0-2", "name":"Unproved next course", "group":"beginner", "level":2,
            "available":false, "geometryValidated":false,
            "source":{"courseIndex":0,"tableIndex":1,"rivalSetIndex":-1,"raw":[-1,0,0,0,-1]}
        }],
        "raceBots":(0..4).map(|index|json!({
            "index":index,"address":format!("synthetic-{index}"),"statMode":0,"sharedStat":1,
            "perStatPairs":{"swim":[1,1],"fly":[1,1],"run":[1,1],"power":[1,1],
                "stamina":[1,1],"luck":[1,1],"intelligence":[1,1]},
            "appearanceBytes":vec![0;16],"morphParameters":vec![0;8]
        })).collect::<Vec<_>>()
    })
}

fn karate_catalog() -> Value {
    json!({"tiers":[{
        "id":"karate-beginner","name":"Synthetic Beginner",
        "source":{"nativeTablesJoined":true,"rosterStartDescriptor":2,"tierSelectionCallsiteValidated":false},
        "opponents":[{
            "id":"native-karate-opponent-2","name":"Synthetic CPU","nativeIndex":2,
            "skills":[1,1,1,1,1,1,1,0],"appearance":{"synthetic":true}
        },{
            "id":"native-karate-opponent-3","name":"Unproved next CPU","nativeIndex":3,
            "skills":[10,10,10,10,10,10,10,0],"appearance":{"synthetic":true}
        }]
    }]})
}

/// Synthetic native-shaped tables (no game data): 27 karate descriptors, 28
/// skill rows, 62 course rows, 115 racer records and 62 rival sets, all weak
/// so a test Chao wins.
fn native_tables() -> Value {
    json!({
        "karate":{
            "descriptors":(0..27).map(|i|json!({"skills":0,"name":i,"appearance":0,"kind":-1,"alternate":-1})).collect::<Vec<_>>(),
            "skills":(0..28).map(|_|json!([0,0,0,0,0,0,0,0])).collect::<Vec<_>>()
        },
        "raceCourses":(0..62).map(|_|json!({"index":0,"land":0,"sections":2,"e010":2,"unlock":-1})).collect::<Vec<_>>(),
        "raceBots":(0..115).map(|_|json!({"mode":0,"shared":0,"pairs":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0]]})).collect::<Vec<_>>(),
        "rivalSets":(0..62).map(|_|json!({"fixed":0,"unknown4":0,"bots":[0,0,0,0,0,0,0]})).collect::<Vec<_>>()
    })
}

fn write_catalogs(path: &Path) {
    fs::create_dir_all(path.join("activities")).unwrap();
    fs::write(
        path.join("activities/races.json"),
        race_catalog().to_string(),
    )
    .unwrap();
    fs::write(
        path.join("activities/karate.json"),
        karate_catalog().to_string(),
    )
    .unwrap();
    fs::write(
        path.join("activities/competition-native.json"),
        native_tables().to_string(),
    )
    .unwrap();
}

fn fixture() -> (tempfile::TempDir, AppState, String, String) {
    let dir = tempfile::tempdir().unwrap();
    write_catalogs(dir.path());
    let state = AppState::load(Config::local(dir.path().to_owned(), true)).unwrap();
    let (id, token) = {
        let mut game = state.game.lock().unwrap();
        let session = game
            .create_session(&json!({"name":"Competition test"}))
            .unwrap();
        let id = session["player"]["id"].as_str().unwrap().to_owned();
        let token = session["token"].as_str().unwrap().to_owned();
        game.connect(&id);
        state.persist(&mut game).unwrap();
        (id, token)
    };
    (dir, state, id, token)
}

fn advance(game: &mut Game, ms: usize) {
    for _ in 0..ms.div_ceil(50) {
        game.tick(0.05);
    }
}

fn public(game: &Game, id: &str) -> Value {
    competition::public(&game.players[id])
}

#[test]
fn starting_either_kind_fails_closed_with_the_native_reason() {
    let (_dir, state, id, _) = fixture();
    let mut game = state.game.lock().unwrap();
    let rings = game.players[&id].rings;
    let catalog = game.competition_catalog.public();
    assert_eq!(
        (catalog["race"].clone(), catalog["karate"].clone()),
        (json!(false), json!(false))
    );
    assert_eq!(catalog["nativeTables"], true);
    for kind in ["race", "karate"] {
        advance(&mut game, 100);
        let reply = state
            .competition_action(
                &mut game,
                &id,
                &json!({"type":"action","action":"competition-start",
                "expectedRevision":0,"kind":kind,"matches":1}),
            )
            .unwrap();
        assert_eq!(reply["accepted"], false);
        assert!(public(&game, &id)["plan"].is_null());
        assert!(game.players[&id].activity_lease.is_none());
    }
    advance(&mut game, 60_000);
    assert_eq!(game.players[&id].rings, rings);
    assert!(public(&game, &id)["results"].as_array().unwrap().is_empty());
}

fn legacy_simulation(owner: &str, kind: &str) -> Value {
    json!({"kind":kind,"rulesVersion":"adapted-ai-competition-v1","durationMs":1000,
        "competitors":[{"id":owner,"name":"o","stats":[1,1,1,1,1,1,1],"source":{}},
                       {"id":"rival","name":"r","stats":[1,1,1,1,1,1,1],"source":{}}],
        "frames":[{"elapsedMs":0,"competitors":[
            {"id":owner,"progress":0,"hp":0,"stamina":100,"action":"swim"},
            {"id":"rival","progress":0,"hp":0,"stamina":100,"action":"swim"}]}],
        "placements":[{"id":owner,"rank":1,"finishMs":1000,"remainingHp":null},
                      {"id":"rival","rank":2,"finishMs":1100,"remainingHp":null}],
        "reason":"finished"})
}

#[test]
fn a_saved_legacy_plan_is_cancelled_without_payout_and_its_history_kept() {
    let (_dir, state, id, _) = fixture();
    let game = state.game.lock().unwrap();
    let mut profiles = game.export_profiles();
    let plan = "11111111-1111-4111-8111-111111111111";
    profiles[0]["competition"] = json!({"version":1,"revision":4,
        "plan":{"id":plan,"kind":"race","remaining":0,"waitRemainingMs":0},
        "active":{"id":"22222222-2222-4222-8222-222222222222","kind":"race","elapsedMs":10,
            "seed":1,"simulation":legacy_simulation(&id,"race"),"reference":{}},
        "results":[{"id":"33333333-3333-4333-8333-333333333333","kind":"race","rank":1,"rings":25,
            "earnedRings":25,"simulation":legacy_simulation(&id,"race"),"reference":{},"seed":2}],
        "completedTotal":1});
    profiles[0]["activityLease"] = json!({"kind":"competition","id":plan});
    let rings = profiles[0]["rings"].as_u64().unwrap();
    let mut restored = Game::new(profiles.clone(), BTreeMap::new(), false, 0.).unwrap();
    assert!(competition::reserved(&restored.players[&id]));
    restored.tick(0.05);
    competition::commit(&mut restored);
    let p = &restored.players[&id];
    assert!(p.competition.plan.is_none() && p.competition.active.is_none());
    assert!(p.activity_lease.is_none());
    assert_eq!(u64::from(p.rings), rings);
    assert_eq!(p.competition.results.len(), 1);
    // Bounds still fail closed for tampered history.
    profiles[0]["competition"]["results"][0]["rings"] = json!(999);
    assert!(Game::new(profiles, BTreeMap::new(), false, 0.).is_err());
}

#[test]
fn malformed_native_tables_refuse_to_load() {
    let dir = tempfile::tempdir().unwrap();
    write_catalogs(dir.path());
    fs::write(
        dir.path().join("activities/competition-native.json"),
        json!({"karate":{"descriptors":[],"skills":[]},
        "raceCourses":[],"raceBots":[],"rivalSets":[]})
        .to_string(),
    )
    .unwrap();
    assert!(AppState::load(Config::local(dir.path().to_owned(), true)).is_err());
}
