//! Opt-in acceptance reads only an explicitly supplied owned verified payload.
//! Saves and generated profiles remain inside disposable directories.
use chao_game_server::{
    care,
    game::ALL_STATS,
    service::{AppState, Config},
};
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf};
use tiny_garden_core::shop::{EFFECTS_OFFSET, PRICES_OFFSET};
use uuid::Uuid;

fn request(revision: u64, kind: &str, item: &str) -> Value {
    json!({"type":"care","requestId":Uuid::new_v4().to_string(),"expectedRevision":revision,
        "intent":{"kind":kind,"itemId":item}})
}

#[test]
#[ignore = "Requires an explicitly configured owned CHAO_TINY_GARDEN_PAYLOAD"]
fn all_seven_verified_fruit_prices_feeding_and_receipts_survive_restart() {
    let path = PathBuf::from(
        env::var_os("CHAO_TINY_GARDEN_PAYLOAD")
            .expect("Set CHAO_TINY_GARDEN_PAYLOAD for opt-in acceptance"),
    );
    let verified =
        care::load_tiny_shop(&path).expect("Exact native source identity and spans must verify");
    let bytes = fs::read(&path).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::local(directory.path().to_owned(), false);
    config.tiny_garden_payload = Some(path);
    let state = AppState::load(config.clone()).unwrap();
    let (id, expected_care, expected_stats, expected_rings, requests) = {
        let mut game = state.game.lock().unwrap();
        let session = game
            .create_session(&json!({"name":"Owned native Tiny acceptance"}))
            .unwrap();
        let id = session["player"]["id"].as_str().unwrap().to_owned();
        game.connect(&id);
        {
            let p = game.players.get_mut(&id).unwrap();
            p.zone = "garden".into();
            p.x = 15.;
            p.z = 15.;
            p.rings = 9999;
        }
        let mut expected_rings = 9999;
        let mut requests = Vec::new();
        for fruit in &verified.fruits {
            let index = usize::from(fruit.id);
            let item = format!("tiny-fruit:{index}");
            let price = u32::from_le_bytes(
                bytes[PRICES_OFFSET + index * 4..PRICES_OFFSET + index * 4 + 4]
                    .try_into()
                    .unwrap(),
            );
            let raw: [i8; 7] =
                std::array::from_fn(|channel| bytes[EFFECTS_OFFSET + index * 7 + channel] as i8);
            assert_eq!(fruit.price, price);
            assert_eq!(fruit.raw, raw);
            let catalog = game.snapshot(&id)["care"]["tinyShop"].clone();
            let offer = catalog["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["itemId"] == item)
                .unwrap();
            assert_eq!(offer["price"], price);
            assert_eq!(offer["rawEffects"]["values"], json!(raw));
            assert_eq!(catalog["nativeCatalog"]["enabled"], true);
            game.time += 2000.;
            let revision = game.players[&id].care.revision;
            let buy = request(revision, "tiny_purchase", &item);
            assert_eq!(
                state.care_action(&mut game, &id, &buy).unwrap()["accepted"],
                true
            );
            expected_rings -= price;
            assert_eq!(game.players[&id].rings, expected_rings);
            {
                let p = game.players.get_mut(&id).unwrap();
                p.happiness = 45.;
                p.hunger = 45.;
                p.energy = 33.;
                for stat in p.stats.values_mut() {
                    stat.level = 7;
                    stat.grade = 2;
                    stat.points = 100;
                }
            }
            let feed = request(revision + 1, "consume", &item);
            let result = state.care_action(&mut game, &id, &feed).unwrap();
            assert_eq!(result["accepted"], true);
            assert_eq!(result["effects"]["raw"]["values"], json!(raw));
            assert_eq!(result["effects"]["raw"]["nativeLevelUpPorted"], false);
            let p = &game.players[&id];
            assert_eq!(p.happiness, (45. + f64::from(raw[0]) * 10.).clamp(0., 100.));
            assert_eq!(p.hunger, (45. + f64::from(raw[1]) * 10.).clamp(0., 100.));
            assert_eq!(p.energy, 33.);
            for (name, value) in ["swim", "fly", "run", "power", "stamina"]
                .into_iter()
                .zip(raw[2..].iter())
            {
                assert_eq!(
                    p.stats[name].points,
                    (100_i32 + i32::from(*value) * 10).clamp(0, 9999) as u32
                );
            }
            for name in ALL_STATS {
                assert_eq!(p.stats[name].level, 7);
                assert_eq!(p.stats[name].grade, 2);
            }
            assert_eq!(p.stats["luck"].points, 100);
            assert_eq!(p.stats["intelligence"].points, 100);
            assert!(!p.care.inventory.contains_key(&item));
            requests.extend([buy, feed]);
        }
        let p = &game.players[&id];
        (
            id.clone(),
            p.care.clone(),
            p.stats.clone(),
            expected_rings,
            requests,
        )
    };
    let saved = fs::read(directory.path().join("profiles.json")).unwrap();
    let restarted = AppState::load(config).unwrap();
    let mut game = restarted.game.lock().unwrap();
    assert_eq!(game.players[&id].care, expected_care);
    assert_eq!(game.players[&id].stats, expected_stats);
    assert_eq!(game.players[&id].rings, expected_rings);
    for request in requests {
        let replay = restarted.care_action(&mut game, &id, &request).unwrap();
        assert_eq!(replay["accepted"], true);
        assert_eq!(replay["replayed"], true);
    }
    assert_eq!(game.players[&id].care.receipts.len(), 14);
    assert_eq!(game.players[&id].rings, expected_rings);
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        saved
    );
}

#[test]
fn configured_invalid_native_payload_fails_closed_without_creating_a_save() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("wrong-payload.gba");
    fs::write(&path, [0_u8; 32]).unwrap();
    let mut config = Config::local(directory.path().to_owned(), false);
    config.tiny_garden_payload = Some(path);
    assert!(AppState::load(config).is_err());
    assert!(!directory.path().join("profiles.json").exists());
}
