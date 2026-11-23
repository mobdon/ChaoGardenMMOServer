//! Queues for group Chao Race and player-versus-player Chao Karate.
//!
//! A player in the matching Chao World area joins a queue (ranked or not; a
//! race also names its course) and lands in a group. Every member marks
//! ready; the group starts once all have, with at least two members. A Karate
//! group's bout is then run here by the native round and its result sent to
//! both; a ranked one lands in each player's win/loss record. A race group
//! only starts: the native race is not ported end to end, so nothing runs it.
use crate::competition;
use crate::game::{Event, Game, Player};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// A player's ranked record for one event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ranked {
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
}

pub fn restore_ranked(value: &Value) -> BTreeMap<String, Ranked> {
    let mut all: BTreeMap<String, Ranked> =
        serde_json::from_value(value.clone()).unwrap_or_default();
    all.retain(|event, _| event == "chao-karate" || event == "chao-race");
    all
}

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
    /// The other members of the player's group: who its group chat and voice reach.
    pub fn mates(&self, id: &str) -> Option<Vec<String>> {
        let others = self.group_of(id)?.members.iter();
        Some(others.filter(|m| m.0 != id).map(|m| m.0.clone()).collect())
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
            if group.started && group.event == "chao-karate" {
                bout(game, player, &group);
            }
        }
        "queue-leave" => leave(game, &player.id),
        _ => {}
    }
}

/// The owner Chao's karate entry: raw skill points and whether it is a child.
fn fighter(p: &Player) -> Option<([u16; 7], bool, bool, i16)> {
    let child = match p.life.stage {
        crate::life::Stage::Child => true,
        crate::life::Stage::Adult => false,
        _ => return None,
    };
    let skills = crate::game::ALL_STATS.map(|s| p.stats[s].points.min(u32::from(u16::MAX)) as u16);
    Some((skills, false, child, 0))
}

/// Runs a started Karate group's bout: one native round, first member on the
/// left. Without the local native karate data, or with a Chao that cannot
/// fight, the group is told and released with no result.
fn bout(game: &mut Game, actor: &mut Player, group: &Group) {
    let entrant = |game: &Game, id: &str| {
        let p = if id == actor.id {
            Some(&*actor)
        } else {
            game.players.get(id)
        };
        p.and_then(fighter)
    };
    let sides = (
        entrant(game, &group.members[0].0),
        entrant(game, &group.members[1].0),
    );
    let ready = (game.karate.clone(), game.native_chao.motion.clone(), sides);
    let (Some(data), Some(motion), (Some(a), Some(b))) = ready else {
        for (id, _) in &group.members {
            game.notice(
                id,
                "The bout could not run: Chao Karate needs the native karate data and two child or adult Chao.",
                "error",
            );
        }
        finish(game, actor, group.id, None);
        return;
    };
    let round = crate::karate::fight(
        &data.rows,
        &motion,
        &data.arena,
        &mut game.karate_rng,
        [a, b],
    );
    let bout = json!({"winner": round.winner, "reason": round.reason, "frames": round.frames, "hp": round.hp});
    finish(
        game,
        actor,
        group.id,
        Some((usize::try_from(round.winner).ok().filter(|w| *w < 2), bout)),
    );
}

/// Ends a started group: `outcome` is the winner's member slot (`None` for a
/// draw) with the match detail, or `None` when nothing was run. Every member
/// gets the result, a ranked group's records are updated, and the group is
/// dissolved so its members can queue again.
pub fn finish(
    game: &mut Game,
    actor: &mut Player,
    group_id: u64,
    outcome: Option<(Option<usize>, Value)>,
) {
    let Some(at) = game.queues.groups.iter().position(|g| g.id == group_id) else {
        return;
    };
    let group = game.queues.groups.remove(at);
    let ids: Vec<&String> = group.members.iter().map(|m| &m.0).collect();
    let mut records = serde_json::Map::new();
    if let Some((winner, _)) = &outcome {
        for (slot, id) in ids.iter().enumerate() {
            let p = if **id == actor.id {
                Some(&mut *actor)
            } else {
                game.players.get_mut(*id)
            };
            let Some(p) = p.filter(|_| group.ranked) else {
                continue;
            };
            let record = p.ranked.entry(group.event.into()).or_default();
            match winner {
                Some(w) if *w == slot => record.wins += 1,
                Some(_) => record.losses += 1,
                None => record.draws += 1,
            }
            records.insert((*id).clone(), json!(record));
        }
        game.dirty |= group.ranked;
    }
    let (winner, detail) = match outcome {
        Some((w, detail)) => (json!(w.map(|w| ids[w])), detail),
        None => (Value::Null, Value::Null),
    };
    let message = json!({"type":"event","kind":"queue-result","group":group.id,"event":group.event,
        "ranked":group.ranked,"course":group.course,"members":ids,"run":!detail.is_null(),
        "winner":winner,"result":detail,"records":records});
    for id in ids {
        game.events.push(Event {
            target: Some(id.clone()),
            message: message.clone(),
        });
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
        let told: Vec<_> = game
            .events
            .iter()
            .filter(|e| e.message["kind"] == "queue" && e.message["started"] == true)
            .filter_map(|e| e.target.clone())
            .collect();
        assert_eq!(told, ids[..2]);
        // The bout is over at once (here: not run, no karate data): the group
        // is dissolved and the third player gets a group of their own.
        join_karate(&mut game, &ids[2], false);
        assert_eq!(game.queues.groups.len(), 1);
    }

    #[test]
    fn a_ranked_result_reaches_both_and_lands_in_their_records() {
        let (mut game, ids) = game_with(2);
        join_karate(&mut game, &ids[0], true);
        join_karate(&mut game, &ids[1], true);
        let group = game.queues.groups[0].id;
        game.events.clear();
        // The acting player is out of `game.players` while its action runs.
        let mut actor = game.players.remove(&ids[1]).unwrap();
        finish(
            &mut game,
            &mut actor,
            group,
            Some((Some(1), json!({"frames": 9}))),
        );
        game.players.insert(ids[1].clone(), actor);
        let wins = |game: &Game, id: &String| game.players[id].ranked["chao-karate"];
        assert_eq!(
            (wins(&game, &ids[0]).wins, wins(&game, &ids[0]).losses),
            (0, 1)
        );
        assert_eq!(
            (wins(&game, &ids[1]).wins, wins(&game, &ids[1]).losses),
            (1, 0)
        );
        let results: Vec<_> = game
            .events
            .iter()
            .filter(|e| e.message["kind"] == "queue-result")
            .collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].message["winner"], json!(ids[1]));
        assert_eq!(results[0].message["records"][&ids[0]]["losses"], 1);
        // The record survives a save and the group is gone: both can queue again.
        let saved = game.players[&ids[1]].profile();
        assert_eq!(restore_ranked(&saved["ranked"])["chao-karate"].wins, 1);
        assert!(game.queues.groups.is_empty());
    }

    #[test]
    fn an_unranked_result_or_a_bout_that_could_not_run_leaves_records_alone() {
        let (mut game, ids) = game_with(2);
        join_karate(&mut game, &ids[0], false);
        join_karate(&mut game, &ids[1], false);
        ready(&mut game, &ids[0]);
        game.events.clear();
        // No native karate data in this test game: the bout cannot run.
        ready(&mut game, &ids[1]);
        let result = game
            .events
            .iter()
            .find(|e| e.message["kind"] == "queue-result")
            .expect("both are told");
        assert_eq!(result.message["run"], false);
        assert!(game.queues.groups.is_empty());
        assert!(ids.iter().all(|id| game.players[id].ranked.is_empty()));
    }

    #[test]
    fn group_chat_and_voice_reach_only_the_group_and_the_staff_tap() {
        let (mut game, ids) = game_with(3);
        for id in &ids {
            join_karate(&mut game, id, false);
        }
        // ids[0] and ids[1] share a group; ids[2] waits alone in the same area.
        let heard = |game: &mut Game, kind: &str, message: Value| {
            game.time += 1100.;
            game.events.clear();
            game.action(&ids[0], &message);
            let mut to: Vec<_> = game
                .events
                .iter()
                .filter(|e| e.message["kind"] == kind)
                .filter_map(|e| e.target.clone())
                .collect();
            to.sort();
            to
        };
        let mut pair = ids[..2].to_vec();
        pair.sort();
        let chat = json!({"type":"action","action":"chat","channel":"group","text":"hi"});
        assert_eq!(heard(&mut game, "chat", chat.clone()), pair);
        game.voice_tap_until = u64::MAX;
        let voice = json!({"type":"action","action":"voice","data":"AA"});
        assert_eq!(heard(&mut game, "voice", voice.clone()), ids[1..2]);
        assert_eq!(game.voice_tap.back().unwrap()["room"], "chao-karate");
        // Out of the group there is nobody to talk to.
        send(
            &mut game,
            &ids[0],
            json!({"type":"action","action":"queue-leave"}),
        );
        assert!(heard(&mut game, "chat", chat).is_empty());
        assert!(heard(&mut game, "voice", voice).is_empty());
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
