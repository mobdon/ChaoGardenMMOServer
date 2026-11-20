//! Loopback-only proof of canonical away entries and durable spectator results.
use chao_game_server::competition;
use chao_game_server::service::{AppState, Config, router, spawn_simulation};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{fs, time::Duration};
use tempfile::TempDir;
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::timeout,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct LiveServer {
    directory: TempDir,
    state: AppState,
    url: String,
    serving: JoinHandle<()>,
    simulation: JoinHandle<()>,
}

impl LiveServer {
    async fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        write_catalogs(directory.path());
        let state = AppState::load(Config::local(directory.path().to_owned(), true)).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let app = router(state.clone());
        let serving = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let simulation = spawn_simulation(state.clone());
        Self {
            directory,
            state,
            url,
            serving,
            simulation,
        }
    }

    fn session(&self, name: &str) -> Value {
        let mut game = self.state.game.lock().unwrap();
        let session = game.create_session(&json!({"name":name})).unwrap();
        self.state.persist(&mut game).unwrap();
        session
    }

    async fn socket(&self, session: &Value) -> Socket {
        connect_async(format!(
            "{}/ws?token={}",
            self.url,
            session["token"].as_str().unwrap()
        ))
        .await
        .unwrap()
        .0
    }

    fn disk_profile(&self, id: &str) -> Value {
        let disk: Vec<Value> =
            serde_json::from_slice(&fs::read(self.directory.path().join("profiles.json")).unwrap())
                .unwrap();
        disk.into_iter()
            .find(|profile| profile["id"] == id)
            .unwrap()
    }
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        let _ = self.state.shutdown.send(true);
        self.serving.abort();
        self.simulation.abort();
    }
}

async fn receive_until(socket: &mut Socket, matches: impl Fn(&Value) -> bool) -> Value {
    timeout(Duration::from_secs(4), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if matches(&value) {
                        return value;
                    }
                }
                Some(Ok(Message::Ping(bytes))) => socket.send(Message::Pong(bytes)).await.unwrap(),
                Some(Ok(Message::Close(_))) | None => {
                    panic!("socket closed before expected result")
                }
                Some(Err(error)) => panic!("socket failed before expected result: {error}"),
                _ => {}
            }
        }
    })
    .await
    .expect("expected authoritative response")
}

async fn send(socket: &mut Socket, value: Value) {
    // The canonical host limits mutation messages to one per 80 ms.
    tokio::time::sleep(Duration::from_millis(100)).await;
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}

fn player<'a>(snapshot: &'a Value, id: &str) -> &'a Value {
    static MISSING: Value = Value::Null;
    snapshot["players"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or(&MISSING)
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

fn write_catalogs(path: &std::path::Path) {
    fs::create_dir_all(path.join("activities")).unwrap();
    let races = json!({
        "courses":[{"id":"beginner-0-1","name":"Synthetic swim reference","group":"beginner",
            "level":1,"available":false,"geometryValidated":false,
            "source":{"courseIndex":0,"tableIndex":0,"rivalSetIndex":-1,"raw":[-1,0,0,0,-1]}}],
        "raceBots":(0..4).map(|index|json!({
            "index":index,"address":format!("synthetic-{index}"),"statMode":0,"sharedStat":1,
            "perStatPairs":{"swim":[1,1],"fly":[1,1],"run":[1,1],"power":[1,1],
                "stamina":[1,1],"luck":[1,1],"intelligence":[1,1]},
            "appearanceBytes":vec![0;16],"morphParameters":vec![0;8]
        })).collect::<Vec<_>>()
    });
    let karate = json!({"tiers":[{
        "id":"karate-beginner","name":"Synthetic Beginner",
        "source":{"nativeTablesJoined":true,"rosterStartDescriptor":2,"tierSelectionCallsiteValidated":false},
        "opponents":[{"id":"native-karate-opponent-2","name":"Synthetic CPU","nativeIndex":2,
            "skills":[1,1,1,1,1,1,1,0],"appearance":{"synthetic":true}}]
    }]});
    fs::write(path.join("activities/races.json"), races.to_string()).unwrap();
    fs::write(path.join("activities/karate.json"), karate.to_string()).unwrap();
    fs::write(
        path.join("activities/competition-native.json"),
        native_tables().to_string(),
    )
    .unwrap();
}

/// Advances the actual host/core while sockets observe it. Every staged boundary
/// goes through the real durable service transaction, with no client clock input.
fn finish_one(server: &LiveServer, id: &str, expected_results: usize) {
    let mut game = server.state.game.lock().unwrap();
    for _ in 0..20_000 {
        game.tick(0.05);
        if chao_game_server::competition::has_pending(&game) {
            server.state.persist(&mut game).unwrap();
        }
        if chao_game_server::competition::public(&game.players[id])["results"]
            .as_array()
            .unwrap()
            .len()
            >= expected_results
        {
            return;
        }
    }
    panic!("server-owned match did not durably finish within bounded simulation");
}

#[tokio::test]
async fn race_and_karate_keep_one_away_chao_and_publish_only_durable_results() {
    let server = LiveServer::start().await;
    let owner = server.session("Competitor");
    let visitor = server.session("Spectator");
    let id = owner["player"]["id"].as_str().unwrap();
    let mut observer = server.socket(&visitor).await;
    let mut control = server.socket(&owner).await;
    let mut duplicate = server.socket(&owner).await;
    receive_until(&mut control, |value| value["type"] == "snapshot").await;
    send(
        &mut control,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    let watched = receive_until(&mut control, |value| value["kind"] == "watch-saved").await;
    assert_eq!(watched["accepted"], true);
    send(&mut observer, json!({"type":"action","action":"garden"})).await;
    receive_until(&mut observer, |value| {
        value["type"] == "snapshot" && player(value, id)["connected"] == true
    })
    .await;

    let forged = json!({"type":"action","action":"competition-start","expectedRevision":0,
        "kind":"race","matches":1,"rings":999999,"winner":id});
    send(&mut control, forged).await;
    let rejected = receive_until(&mut control, |value| value["kind"] == "competition-saved").await;
    assert_eq!(rejected["accepted"], false);
    assert_eq!(server.disk_profile(id)["rings"], owner["player"]["rings"]);

    for (index, kind) in ["race", "karate"].into_iter().enumerate() {
        let before = {
            let game = server.state.game.lock().unwrap();
            chao_game_server::competition::public(&game.players[id])
        };
        let start = json!({"type":"action","action":"competition-start",
            "expectedRevision":before["revision"],"kind":kind,"matches":1});
        send(&mut control, start.clone()).await;
        let accepted =
            receive_until(&mut control, |value| value["kind"] == "competition-saved").await;
        assert_eq!(
            accepted["accepted"], true,
            "{kind} entry must durably opt in"
        );
        send(&mut duplicate, start).await;
        let replayed =
            receive_until(&mut duplicate, |value| value["kind"] == "competition-saved").await;
        assert_eq!(
            replayed["accepted"], false,
            "same revision cannot create another plan"
        );

        control.close(None).await.unwrap();
        duplicate.close(None).await.unwrap();
        receive_until(&mut observer, |value| {
            value["type"] == "snapshot" && player(value, id)["connected"] == false
        })
        .await;
        finish_one(&server, id, index + 1);
        let result_snapshot = receive_until(&mut observer, |value| {
            value["type"] == "snapshot"
                && player(value, id)["competition"]["results"]
                    .as_array()
                    .is_some_and(|results| results.iter().any(|result| result["kind"] == kind))
        })
        .await;
        assert_eq!(
            result_snapshot["players"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["id"] == id)
                .count(),
            1
        );
        let result = player(&result_snapshot, id)["competition"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|result| result["kind"] == kind)
            .unwrap()
            .clone();
        assert_eq!(result["kind"], kind);
        assert_eq!(result["saved"], true);
        let disk = server.disk_profile(id);
        assert_eq!(disk["rings"], player(&result_snapshot, id)["rings"]);
        assert!(disk.to_string().contains(result["id"].as_str().unwrap()));

        control = server.socket(&owner).await;
        duplicate = server.socket(&owner).await;
        send(
            &mut control,
            json!({"type":"action","action":"competition-replay","matchId":result["id"]}),
        )
        .await;
        let replay =
            receive_until(&mut control, |value| value["kind"] == "competition-replay").await;
        assert!(replay.to_string().contains(result["id"].as_str().unwrap()));
        assert_eq!(server.disk_profile(id)["rings"], disk["rings"]);
    }
    control.close(None).await.unwrap();
    duplicate.close(None).await.unwrap();
    observer.close(None).await.unwrap();
    let restarted =
        AppState::load(Config::local(server.directory.path().to_owned(), false)).unwrap();
    let game = restarted.game.lock().unwrap();
    let restored = game
        .player_for_token(owner["token"].as_str().unwrap())
        .unwrap();
    assert_eq!(restored.id, id);
    assert_eq!(
        chao_game_server::competition::public(restored)["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        restored.rings,
        server.disk_profile(id)["rings"].as_u64().unwrap() as u32
    );
}

#[test]
fn rejected_intent_flushing_a_pending_result_is_not_acknowledged_as_accepted() {
    for (kind, action) in [
        ("race", "competition-start"),
        ("karate", "competition-stop"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        write_catalogs(directory.path());
        let state = AppState::load(Config::local(directory.path().to_owned(), true)).unwrap();
        let mut game = state.game.lock().unwrap();
        let session = game
            .create_session(&json!({"name":"Pending result"}))
            .unwrap();
        let id = session["player"]["id"].as_str().unwrap();
        game.connect(id);
        state.persist(&mut game).unwrap();
        assert_eq!(
            chao_game_server::care::public(&game, id)["eligibility"]["canCare"],
            true
        );
        let accepted = state
            .competition_action(
                &mut game,
                id,
                &json!({
                    "type":"action", "action":"competition-start", "kind":kind,
                    "matches":1, "expectedRevision":0
                }),
            )
            .unwrap();
        assert_eq!(accepted["accepted"], true);
        let rejected_care = state
            .care_action(
                &mut game,
                id,
                &json!({"type":"care", "requestId":uuid::Uuid::new_v4().to_string(),
                    "expectedRevision":0, "intent":{"kind":"purchase", "itemId":"garden-fruit"}}),
            )
            .unwrap();
        assert_eq!(rejected_care["accepted"], false);
        assert_eq!(rejected_care["code"], "activity-conflict");
        assert_eq!(rejected_care["revision"], 0);
        let plan_id = competition::public(&game.players[id])["plan"]["id"].clone();
        let baseline = game.players[id].rings;
        let mut terminal = false;
        for _ in 0..5000 {
            game.tick(0.05);
            if competition::has_pending(&game) {
                let exported = game.export_profiles();
                let projected = exported.iter().find(|profile| profile["id"] == id).unwrap();
                if projected["competition"]["results"]
                    .as_array()
                    .unwrap()
                    .len()
                    == 1
                {
                    terminal = true;
                    break;
                }
                state.persist(&mut game).unwrap();
            }
        }
        assert!(terminal, "server match must stage a terminal result");
        let disk_before = fs::read(directory.path().join("profiles.json")).unwrap();
        fs::create_dir(directory.path().join("profiles.json.tmp")).unwrap();
        assert!(state.persist(&mut game).is_err());
        competition::save_failed(&mut game);
        assert_eq!(game.players[id].rings, baseline);
        assert_eq!(
            fs::read(directory.path().join("profiles.json")).unwrap(),
            disk_before
        );
        assert!(
            competition::public(&game.players[id])["results"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        fs::remove_dir(directory.path().join("profiles.json.tmp")).unwrap();

        let revision = competition::public(&game.players[id])["revision"].clone();
        let rejected = if action == "competition-start" {
            json!({"type":"action", "action":action, "expectedRevision":revision,
                "kind":kind, "matches":1})
        } else {
            json!({"type":"action", "action":action, "expectedRevision":revision, "planId":plan_id})
        };
        let acknowledgement = state.competition_action(&mut game, id, &rejected).unwrap();
        assert_eq!(acknowledgement["kind"], "competition-saved");
        assert_eq!(
            acknowledgement["accepted"], false,
            "committing a prior earned result must not accept the rejected intent"
        );
        let result = acknowledgement["competition"]["results"][0].clone();
        assert_eq!(result["saved"], true);
        assert!(acknowledgement["competition"]["plan"].is_null());
        let credited = game.players[id].rings;
        assert_eq!(
            credited,
            baseline + result["rings"].as_u64().unwrap() as u32
        );
        assert_eq!(
            game.drain_events()
                .iter()
                .filter(|event| event.message["kind"] == "competition-result")
                .count(),
            1
        );
        state.persist(&mut game).unwrap();
        assert_eq!(game.players[id].rings, credited);
        assert!(
            !game
                .drain_events()
                .iter()
                .any(|event| event.message["kind"] == "competition-result")
        );
        drop(game);
        let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
        let game = restarted.game.lock().unwrap();
        assert_eq!(game.players[id].rings, credited);
        assert_eq!(competition::public(&game.players[id])["results"][0], result);
    }
}
