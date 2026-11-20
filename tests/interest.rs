use chao_game_server::game::{Enemy, Event, Game, Mission, Ring};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

fn session(game: &mut Game, name: &str, zone: &str) -> (String, String) {
    let session = game.create_session(&json!({"name":name})).unwrap();
    let id = session["player"]["id"].as_str().unwrap().to_owned();
    game.connect(&id);
    game.players.get_mut(&id).unwrap().zone = zone.to_owned();
    (id, session["token"].as_str().unwrap().to_owned())
}

fn ids(snapshot: &Value) -> BTreeSet<&str> {
    snapshot["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect()
}

#[test]
fn snapshot_contains_only_self_and_same_room_active_entities() {
    let mut game = Game::new(vec![], BTreeMap::new(), false, 1000.).unwrap();
    let (self_id, _) = session(&mut game, "Viewer", "garden");
    let (neighbor, _) = session(&mut game, "Neighbor", "garden");
    let (other, _) = session(&mut game, "Other room", "hub");
    let (offline, _) = session(&mut game, "Offline", "garden");
    game.disconnect(&offline);
    // A disconnected direct player still owns its own snapshot, but is not an entity for peers.
    assert!(ids(&game.snapshot(&offline)).contains(offline.as_str()));
    let snapshot = game.snapshot(&self_id);
    assert_eq!(
        ids(&snapshot),
        BTreeSet::from([self_id.as_str(), neighbor.as_str()])
    );
    assert!(!ids(&snapshot).contains(other.as_str()));
    assert!(
        snapshot["rings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|ring| ring["zone"] == "garden")
    );
    let hub = game.snapshot(&other);
    assert!(hub["rings"].as_array().unwrap().is_empty());
    let unknown = game.snapshot("unknown");
    assert!(ids(&unknown).is_empty());
    assert!(unknown["rings"].as_array().unwrap().is_empty());
    assert!(unknown["enemies"].as_array().unwrap().is_empty());
    assert!(unknown["mission"].is_null());
}

#[test]
fn mission_state_and_effects_do_not_cross_room_boundaries() {
    let mut game = Game::new(vec![], BTreeMap::new(), false, 1000.).unwrap();
    let (hub, _) = session(&mut game, "Hub", "hub");
    let (fighter, _) = session(&mut game, "Mission", "mission");
    game.mission = Some(Mission {
        id: "encounter".into(),
        completed: false,
        members: BTreeSet::from([fighter.clone()]),
        rewarded: BTreeSet::new(),
        enemies: vec![Enemy {
            id: "robot".into(),
            x: 0.,
            z: 0.,
            hp: 10.,
            max_hp: 10.,
            last_attack_at: 0.,
        }],
        rings: vec![Ring {
            id: "mission-ring".into(),
            x: 0.,
            z: 0.,
            zone: "mission".into(),
            available_at: 0.,
        }],
    });
    let outside = game.snapshot(&hub);
    assert!(outside["mission"].is_null());
    assert!(outside["enemies"].as_array().unwrap().is_empty());
    assert!(outside["rings"].as_array().unwrap().is_empty());
    let inside = game.snapshot(&fighter);
    assert_eq!(inside["mission"]["id"], "encounter");
    assert_eq!(inside["enemies"].as_array().unwrap().len(), 1);
    assert_eq!(inside["rings"][0]["id"], "mission-ring");
    let effect = Event {
        target: None,
        message: json!({"type":"event","kind":"attack","playerId":fighter,"zone":"mission"}),
    };
    assert!(game.event_visible(&fighter, &effect));
    assert!(!game.event_visible(&hub, &effect));
    let private = Event {
        target: Some(fighter.clone()),
        message: json!({"kind":"saved"}),
    };
    assert!(game.event_visible(&fighter, &private));
    assert!(!game.event_visible(&hub, &private));
    assert!(!game.event_visible(
        &hub,
        &Event {
            target: None,
            message: json!({"kind":"unknown"})
        }
    ));
}

#[test]
fn disconnected_resident_remains_in_its_room_and_reconnects_as_one_entity() {
    let mut game = Game::new(vec![], BTreeMap::new(), false, 1000.).unwrap();
    let (resident, token) = session(&mut game, "Resident", "hub");
    game.action(&resident, &json!({"type":"action","action":"watch-start"}));
    let watch_id = game.players[&resident].watch.as_ref().unwrap().id.clone();
    game.disconnect(&resident);
    let (visitor, _) = session(&mut game, "Visitor", "garden");
    let (other, _) = session(&mut game, "Other", "hub");
    assert!(ids(&game.snapshot(&visitor)).contains(resident.as_str()));
    assert!(!ids(&game.snapshot(&other)).contains(resident.as_str()));
    let count = game.players.len();
    for _ in 0..2 {
        let resumed = game
            .create_session(&json!({"token":token,"resumeOnly":true}))
            .unwrap();
        assert_eq!(resumed["player"]["id"], resident);
        assert!(game.connect(&resident));
    }
    assert_eq!(game.players.len(), count);
    assert_eq!(game.players[&resident].watch.as_ref().unwrap().id, watch_id);
    assert_eq!(
        ids(&game.snapshot(&visitor))
            .iter()
            .filter(|id| **id == resident)
            .count(),
        1
    );
    game.tick(0.05);
    assert_eq!(game.last_tick_residents, 1);
    game.players.get_mut(&resident).unwrap().watch = None;
    game.tick(0.05);
    assert_eq!(game.last_tick_residents, 0);
    assert_eq!(game.last_resident_tick, Duration::ZERO);
}
