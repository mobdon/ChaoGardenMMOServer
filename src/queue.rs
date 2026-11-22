//! Queues for group Chao Race and player-versus-player Chao Karate.
//!
//! A player in the matching Chao World area joins a queue (ranked or not; a
//! race also names its course) and lands in a group. Every member marks
//! ready; the group starts once all have, with at least two members. The
//! server only forms the group here: the bout or race itself and its saved
//! result are separate work.
use crate::competition;
use crate::game::{Event, Game, Player};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: u64,
    /// `chao-karate` or `chao-race`: also the area its members stand in.
    pub event: &'static str,
    pub ranked: bool,
    /// Race course id; `None` for Karate.
    pub course: Option<String>,
    /// Player ids in joining order, each with its ready mark. The order is the
    /// fighter slot or lane.
    pub members: Vec<(String, bool)>,
    pub started: bool,
}

impl Group {
    fn capacity(&self) -> usize {
        // A bout has two fighters; a race has eight lanes.
        if self.event == "chao-karate" { 2 } else { 8 }
    }
    pub fn has(&self, id: &str) -> bool {
        self.members.iter().any(|(m, _)| m == id)
    }
}

#[derive(Default)]
pub struct Queues {
    pub groups: Vec<Group>,
    made: u64,
}

impl Queues {
    pub fn group_of(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.has(id))
    }
}

/// Tells every member who is in the group, who is ready and whether it started.
fn announce(game: &mut Game, group: &Group, joiner: Option<&Player>) {
    let members: Vec<Value> = group
        .members
        .iter()
        .map(|(id, ready)| {
            // The acting player is out of `game.players` while its action runs.
            let name = joiner
                .filter(|p| &p.id == id)
                .or_else(|| game.players.get(id))
                .map(|p| p.name.clone());
            json!({"id": id, "name": name, "ready": ready})
        })
        .collect();
    let message = json!({"type":"event","kind":"queue","group":group.id,"event":group.event,
        "ranked":group.ranked,"course":group.course,"members":members,"started":group.started});
    for (id, _) in &group.members {
        game.events.push(Event {
            target: Some(id.clone()),
            message: message.clone(),
        });
    }
}

/// Takes the player out of its group, if any, and tells the rest.
pub fn leave(game: &mut Game, id: &str) {
    let Some(at) = game.queues.groups.iter().position(|g| g.has(id)) else {
        return;
    };
    game.queues.groups[at].members.retain(|(m, _)| m != id);
    game.events.push(Event {
        target: Some(id.into()),
        message: json!({"type":"event","kind":"queue","group":Value::Null}),
    });
    if game.queues.groups[at].members.is_empty() {
        game.queues.groups.remove(at);
    } else {
        let group = game.queues.groups[at].clone();
        announce(game, &group, None);
    }
}

pub fn action(game: &mut Game, player: &mut Player, message: &Value) {
    match message["action"].as_str().unwrap_or_default() {
        "queue-join" => join(game, player, message),
        "queue-ready" => {
            let ready = message["ready"].as_bool().unwrap_or(true);
            let Some(group) = game
                .queues
                .groups
                .iter_mut()
                .find(|g| g.has(&player.id) && !g.started)
            else {
                return;
            };
            for member in &mut group.members {
                if member.0 == player.id {
                    member.1 = ready;
                }
            }
            group.started = group.members.len() >= 2 && group.members.iter().all(|m| m.1);
            let group = group.clone();
            announce(game, &group, Some(player));
        }
        "queue-leave" => leave(game, &player.id),
        _ => {}
    }
}

fn join(game: &mut Game, player: &mut Player, message: &Value) {
    let event = match message["event"].as_str() {
        Some("chao-karate") => "chao-karate",
        Some("chao-race") => "chao-race",
        _ => return,
    };
    let ranked = message["ranked"].as_bool().unwrap_or(false);
    let course =
        (event == "chao-race").then(|| message["courseId"].as_str().unwrap_or_default().to_owned());
    if course
        .as_ref()
        .is_some_and(|id| !game.activities.courses.iter().any(|c| &c.id == id))
    {
        game.notice(&player.id, "Unknown native race course.", "error");
        return;
    }
    if player.zone != event {
        game.notice(&player.id, "Go to that area to queue for it.", "error");
        return;
    }
    if game.queues.group_of(&player.id).is_some() {
        game.notice(&player.id, "Leave your current queue first.", "error");
        return;
    }
    if player.race.is_some()
        || player.karate.is_some()
        || competition::reserved(player)
        || competition::training_conflict(player)
    {
        game.notice(
            &player.id,
            "Your Chao is busy with another activity.",
            "error",
        );
        return;
    }
    let open = game.queues.groups.iter().position(|g| {
        g.event == event
            && g.ranked == ranked
            && g.course == course
            && !g.started
            && g.members.len() < g.capacity()
    });
    let at = open.unwrap_or_else(|| {
        game.queues.made += 1;
        game.queues.groups.push(Group {
            id: game.queues.made,
            event,
            ranked,
            course,
            members: vec![],
            started: false,
        });
        game.queues.groups.len() - 1
    });
    game.queues.groups[at]
        .members
        .push((player.id.clone(), false));
    let group = game.queues.groups[at].clone();
    announce(game, &group, Some(player));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn game_with(players: usize) -> (Game, Vec<String>) {
        let mut game = Game::new(vec![], BTreeMap::new(), false, 1000.).unwrap();
        // Synthetic course id: only its presence in the catalog matters here.
        game.activities.courses.push(
            serde_json::from_value(json!({"id":"test-course","name":"Test","group":"test"}))
                .unwrap(),
        );
        let ids = (0..players)
            .map(|n| {
                let session = game
                    .create_session(&json!({"name": format!("Queue test {n}")}))
                    .unwrap();
                let id = session["player"]["id"].as_str().unwrap().to_owned();
                game.connect(&id);
                id
            })
            .collect();
        (game, ids)
    }
    /// Past the per-player action rate limit each time.
    fn send(game: &mut Game, id: &str, message: Value) {
        game.time += 100.;
        game.action(id, &message);
    }
    fn act(game: &mut Game, id: &str, zone: &str, message: Value) {
        game.players.get_mut(id).unwrap().zone = zone.into();
        send(game, id, message);
    }
    fn join_karate(game: &mut Game, id: &str, ranked: bool) {
        act(
            game,
            id,
            "chao-karate",
            json!({"type":"action","action":"queue-join","event":"chao-karate","ranked":ranked}),
        );
    }
    fn ready(game: &mut Game, id: &str) {
        send(game, id, json!({"type":"action","action":"queue-ready"}));
    }

    #[test]
    fn karate_starts_only_when_both_are_ready() {
        let (mut game, ids) = game_with(3);
        join_karate(&mut game, &ids[0], false);
        ready(&mut game, &ids[0]);
        assert!(
            !game.queues.groups[0].started,
            "one ready player is not a bout"
        );
        join_karate(&mut game, &ids[1], false);
        assert!(!game.queues.groups[0].started);
        game.events.clear();
        ready(&mut game, &ids[1]);
        assert!(game.queues.groups[0].started);
        let told: Vec<_> = game
            .events
            .iter()
            .filter(|e| e.message["kind"] == "queue" && e.message["started"] == true)
            .filter_map(|e| e.target.clone())
            .collect();
        assert_eq!(told, ids[..2]);
        // A bout holds two: the third player gets a group of their own.
        join_karate(&mut game, &ids[2], false);
        assert_eq!(game.queues.groups.len(), 2);
    }

    #[test]
    fn ranked_and_unranked_do_not_mix_and_one_queue_at_a_time() {
        let (mut game, ids) = game_with(2);
        join_karate(&mut game, &ids[0], true);
        join_karate(&mut game, &ids[1], false);
        assert_eq!(game.queues.groups.len(), 2);
        join_karate(&mut game, &ids[0], false);
        assert_eq!(game.queues.groups[1].members.len(), 1);
    }

    #[test]
    fn joining_needs_the_area_and_a_known_course() {
        let (mut game, ids) = game_with(1);
        let race = |course: &str| json!({"type":"action","action":"queue-join","event":"chao-race","courseId":course});
        act(&mut game, &ids[0], "hub", race("test-course"));
        act(&mut game, &ids[0], "chao-race", race("no-such-course"));
        assert!(game.queues.groups.is_empty());
        act(&mut game, &ids[0], "chao-race", race("test-course"));
        assert_eq!(game.queues.groups.len(), 1);
    }

    #[test]
    fn a_race_group_holds_eight() {
        let (mut game, ids) = game_with(9);
        for id in &ids {
            act(
                &mut game,
                id,
                "chao-race",
                json!({"type":"action","action":"queue-join","event":"chao-race","courseId":"test-course"}),
            );
        }
        assert_eq!(game.queues.groups[0].members.len(), 8);
        assert_eq!(game.queues.groups[1].members.len(), 1);
    }

    #[test]
    fn leaving_the_area_or_the_game_leaves_the_queue() {
        let (mut game, ids) = game_with(2);
        join_karate(&mut game, &ids[0], false);
        join_karate(&mut game, &ids[1], false);
        send(
            &mut game,
            &ids[0],
            json!({"type":"action","action":"chao-world","area":"hub"}),
        );
        assert_eq!(game.queues.groups[0].members.len(), 1);
        game.disconnect(&ids[1]);
        assert!(game.queues.groups.is_empty());
    }
}
