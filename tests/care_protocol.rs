//! Real transport coverage against an isolated listener and temporary profile store.
use chao_game_server::service::{AppState, Config, router};
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
use uuid::Uuid;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct TestServer {
    directory: TempDir,
    state: AppState,
    url: String,
    serving: JoinHandle<()>,
}

impl TestServer {
    async fn start() -> (Self, Value, Value) {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
        let (owner, visitor) = {
            let mut game = state.game.lock().unwrap();
            let owner = game.create_session(&json!({"name":"Care owner"})).unwrap();
            let visitor = game.create_session(&json!({"name":"Observer"})).unwrap();
            let id = owner["player"]["id"].as_str().unwrap();
            let player = game.players.get_mut(id).unwrap();
            player.x = -4.;
            player.z = 8.;
            player.energy = 40.;
            player.hunger = 30.;
            player.happiness = 50.;
            state.persist(&mut game).unwrap();
            (owner, visitor)
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let app = router(state.clone());
        let serving = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (
            Self {
                directory,
                state,
                url,
                serving,
            },
            owner,
            visitor,
        )
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
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.state.shutdown.send(true);
        self.serving.abort();
    }
}

async fn receive(socket: &mut Socket, matches: impl Fn(&Value) -> bool) -> Value {
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
                Some(Ok(Message::Pong(_))) => {}
                value => panic!("Socket ended before expected care result: {value:?}"),
            }
        }
    })
    .await
    .expect("authoritative care response before timeout")
}

async fn send(socket: &mut Socket, request: &Value) {
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
}

async fn care_result(socket: &mut Socket, request: &Value) -> Value {
    receive(socket, |value| {
        value["kind"] == "care-result" && value["requestId"] == request["requestId"]
    })
    .await
}

fn request(revision: u64, intent: Value) -> Value {
    json!({"type":"care","requestId":Uuid::new_v4().to_string(),"expectedRevision":revision,"intent":intent})
}

fn saved_profile(server: &TestServer, id: &str) -> Value {
    let profiles: Value =
        serde_json::from_slice(&fs::read(server.directory.path().join("profiles.json")).unwrap())
            .unwrap();
    profiles
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap()
        .clone()
}

#[tokio::test]
async fn two_authenticated_sockets_consume_once_keep_inventory_private_and_replay_after_restart() {
    let (server, owner, visitor) = TestServer::start().await;
    let id = owner["player"]["id"].as_str().unwrap();
    let mut first = server.socket(&owner).await;
    let mut second = server.socket(&owner).await;
    let mut observer = server.socket(&visitor).await;
    let purchase = request(0, json!({"kind":"purchase","itemId":"garden-fruit"}));
    tokio::join!(send(&mut first, &purchase), send(&mut second, &purchase));
    let (one, two) = tokio::join!(
        care_result(&mut first, &purchase),
        care_result(&mut second, &purchase)
    );
    assert_eq!(one["accepted"], true);
    assert_eq!(two["accepted"], true);
    assert_ne!(one["replayed"], two["replayed"]);
    assert_eq!(one["revision"], 1);
    assert_eq!(two["revision"], 1);
    let own = receive(&mut first, |v| {
        v["type"] == "snapshot" && v["care"]["revision"] == 1
    })
    .await;
    assert_eq!(own["care"]["inventory"]["garden-fruit"], 1);
    assert!(own["care"].get("receipts").is_none());
    let public = receive(&mut observer, |v| {
        v["type"] == "snapshot"
            && v["players"]
                .as_array()
                .is_some_and(|players| players.len() == 2)
    })
    .await;
    assert_eq!(public["care"]["inventory"], json!({}));
    assert_eq!(public["care"]["revision"], 0);
    for player in public["players"].as_array().unwrap() {
        assert!(player.get("care").is_none());
        assert!(player.get("inventory").is_none());
        assert!(player.get("tokenHash").is_none());
    }
    let mut forged = request(
        1,
        json!({"kind":"consume","itemId":"garden-fruit","points":{"stamina":9999}}),
    );
    forged["rings"] = json!(99999);
    send(&mut second, &forged).await;
    let rejected = care_result(&mut second, &forged).await;
    assert_eq!(rejected["accepted"], false);
    assert_eq!(rejected["code"], "invalid-request");
    let consume = request(1, json!({"kind":"consume","itemId":"garden-fruit"}));
    tokio::join!(send(&mut first, &consume), send(&mut second, &consume));
    let (one, two) = tokio::join!(
        care_result(&mut first, &consume),
        care_result(&mut second, &consume)
    );
    assert_eq!(one["accepted"], true);
    assert_eq!(two["accepted"], true);
    assert_ne!(one["replayed"], two["replayed"]);
    assert_eq!(one["effects"]["points"]["stamina"], 25);
    assert_eq!(two["effects"]["points"]["stamina"], 25);
    let disk = saved_profile(&server, id);
    assert_eq!(disk["rings"], 17);
    assert_eq!(disk["care"]["inventory"], json!({}));
    assert_eq!(disk["care"]["receipts"].as_object().unwrap().len(), 2);
    assert_eq!(disk["stats"]["stamina"]["points"], 25);
    assert_eq!(disk["stats"]["stamina"]["level"], 1);
    assert_eq!(disk["energy"], 75.);
    assert_eq!(disk["hunger"], 60.);
    assert_eq!(disk["happiness"], 58.);
    assert!(!disk.to_string().contains(owner["token"].as_str().unwrap()));
    first.close(None).await.unwrap();
    second.close(None).await.unwrap();
    observer.close(None).await.unwrap();
    // Reload the store independently: committed receipts survive loss of all sockets.
    let restarted =
        AppState::load(Config::local(server.directory.path().to_owned(), false)).unwrap();
    let mut game = restarted.game.lock().unwrap();
    let replay = restarted.care_action(&mut game, id, &consume).unwrap();
    assert_eq!(replay["accepted"], true);
    assert_eq!(replay["replayed"], true);
    assert_eq!(game.players[id].rings, 17);
    assert_eq!(game.players[id].stats["stamina"].points, 25);
    assert_eq!(game.players[id].care.receipts.len(), 2);
}

#[tokio::test]
async fn websocket_save_failure_rejects_then_same_request_can_commit_and_replay() {
    let (server, owner, _) = TestServer::start().await;
    let id = owner["player"]["id"].as_str().unwrap();
    let mut socket = server.socket(&owner).await;
    let request = request(0, json!({"kind":"purchase","itemId":"garden-fruit"}));
    let disk_before = fs::read(server.directory.path().join("profiles.json")).unwrap();
    let blocked = server.directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocked).unwrap();
    send(&mut socket, &request).await;
    let failed = care_result(&mut socket, &request).await;
    assert_eq!(failed["accepted"], false);
    assert_eq!(failed["code"], "save-failed");
    assert_eq!(failed["revision"], 0);
    {
        let game = server.state.game.lock().unwrap();
        assert_eq!(game.players[id].rings, 20);
        assert!(game.players[id].care.inventory.is_empty());
        assert!(game.players[id].care.receipts.is_empty());
    }
    assert_eq!(
        fs::read(server.directory.path().join("profiles.json")).unwrap(),
        disk_before
    );
    fs::remove_dir(&blocked).unwrap();
    send(&mut socket, &request).await;
    let committed = care_result(&mut socket, &request).await;
    assert_eq!(committed["accepted"], true);
    assert_eq!(committed["replayed"], false);
    send(&mut socket, &request).await;
    let replay = care_result(&mut socket, &request).await;
    assert_eq!(replay["accepted"], true);
    assert_eq!(replay["replayed"], true);
    assert_eq!(saved_profile(&server, id)["rings"], 17);
    socket.close(None).await.unwrap();
}
