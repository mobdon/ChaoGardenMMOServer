//! Loopback-only proof of canonical away entries and durable spectator results.
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

#[tokio::test]
async fn competition_start_fails_closed_over_the_socket_without_side_effects() {
    let server = LiveServer::start().await;
    let owner = server.session("Competitor");
    let id = owner["player"]["id"].as_str().unwrap();
    let mut control = server.socket(&owner).await;
    let snapshot = receive_until(&mut control, |value| value["type"] == "snapshot").await;
    assert_eq!(snapshot["competitionCatalog"]["race"], false);
    assert_eq!(snapshot["competitionCatalog"]["karate"], false);
    assert_eq!(snapshot["competitionCatalog"]["nativeTables"], true);
    assert!(
        snapshot["competitionCatalog"]["reason"]
            .as_str()
            .unwrap()
            .contains("not ported")
    );
    for kind in ["race", "karate"] {
        send(
            &mut control,
            json!({"type":"action","action":"competition-start","expectedRevision":0,"kind":kind,"matches":1}),
        )
        .await;
        let reply = receive_until(&mut control, |value| value["kind"] == "competition-saved").await;
        assert_eq!(reply["accepted"], false, "{kind} must not start");
    }
    let disk = server.disk_profile(id);
    assert_eq!(disk["rings"], owner["player"]["rings"]);
    assert!(disk["competition"].is_null() || disk["competition"]["plan"].is_null());
    assert!(disk["activityLease"].is_null());
}
