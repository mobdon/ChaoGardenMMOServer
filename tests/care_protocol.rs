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

#[tokio::test]
async fn tiny_shop_remote_buy_feed_duplicate_sockets_and_transport_replay_after_restart() {
    let (server, owner, _) = TestServer::start().await;
    let id = owner["player"]["id"].as_str().unwrap();
    {
        let mut game = server.state.game.lock().unwrap();
        let player = game.players.get_mut(id).unwrap();
        player.zone = "garden".into();
        player.x = 10.;
        player.z = 10.;
        player.y = 0.;
        player.ground_y = 0.;
        game.dirty = true;
        server.state.persist(&mut game).unwrap();
    }
    let mut first = server.socket(&owner).await;
    let mut second = server.socket(&owner).await;
    let remote_desktop_buy = request(0, json!({"kind":"purchase","itemId":"garden-fruit"}));
    send(&mut first, &remote_desktop_buy).await;
    assert_eq!(
        care_result(&mut first, &remote_desktop_buy).await["code"],
        "out-of-range"
    );

    let buy = request(0, json!({"kind":"tiny_purchase","itemId":"garden-fruit"}));
    tokio::join!(send(&mut first, &buy), send(&mut second, &buy));
    let (one, two) = tokio::join!(
        care_result(&mut first, &buy),
        care_result(&mut second, &buy)
    );
    assert_eq!(one["accepted"], true);
    assert_eq!(two["accepted"], true);
    assert_ne!(one["replayed"], two["replayed"]);
    assert_eq!(
        one["effects"],
        json!({"rings":-3,"inventory":{"garden-fruit":1}})
    );
    let snapshot = receive(&mut first, |v| {
        v["type"] == "snapshot" && v["care"]["revision"] == 1
    })
    .await;
    assert_eq!(
        snapshot["care"]["tinyShop"]["rulesVersion"],
        "adapted-tiny-shop-v1"
    );
    assert_eq!(snapshot["care"]["tinyShop"]["items"][0]["price"], 3);
    assert_eq!(
        snapshot["care"]["tinyShop"]["eligibility"]["canPurchase"],
        true
    );
    assert_eq!(snapshot["care"]["inventory"]["garden-fruit"], 1);

    let eat = request(1, json!({"kind":"consume","itemId":"garden-fruit"}));
    tokio::join!(send(&mut first, &eat), send(&mut second, &eat));
    let (one, two) = tokio::join!(
        care_result(&mut first, &eat),
        care_result(&mut second, &eat)
    );
    assert_eq!(one["accepted"], true);
    assert_eq!(two["accepted"], true);
    assert_ne!(one["replayed"], two["replayed"]);
    assert_eq!(one["effects"]["points"]["stamina"], 25);
    let disk = saved_profile(&server, id);
    assert_eq!(disk["rings"], 17);
    assert_eq!(disk["care"]["inventory"], json!({}));
    assert_eq!(disk["care"]["revision"], 2);
    assert_eq!(disk["care"]["receipts"].as_object().unwrap().len(), 2);
    first.close(None).await.unwrap();
    second.close(None).await.unwrap();
    server.serving.abort();
    let restarted =
        AppState::load(Config::local(server.directory.path().to_owned(), false)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = router(restarted.clone());
    let serving = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let mut socket = connect_async(format!(
        "ws://{address}/ws?token={}",
        owner["token"].as_str().unwrap()
    ))
    .await
    .unwrap()
    .0;
    for original in [&buy, &eat] {
        send(&mut socket, original).await;
        let replay = care_result(&mut socket, original).await;
        assert_eq!(replay["accepted"], true);
        assert_eq!(replay["replayed"], true);
    }
    {
        let game = restarted.game.lock().unwrap();
        assert_eq!(game.players[id].rings, 17);
        assert_eq!(game.players[id].stats["stamina"].points, 25);
        assert!(game.players[id].care.inventory.is_empty());
        assert_eq!(game.players[id].care.receipts.len(), 2);
    }
    socket.close(None).await.unwrap();
    serving.abort();
}

#[tokio::test]
async fn tiny_enter_minigame_and_passive_widget_keep_one_resident_without_a_take_control_step() {
    let (server, owner, _) = TestServer::start().await;
    let id = owner["player"]["id"].as_str().unwrap();
    let mut phone = server.socket(&owner).await;
    let initial = receive(&mut phone, |v| v["type"] == "snapshot").await;
    assert_eq!(initial["care"]["tinyResident"]["enabled"], false);
    assert_eq!(initial["connectionMode"], "control");
    let enter = request(0, json!({"kind":"tiny_enter"}));
    send(&mut phone, &enter).await;
    assert_eq!(care_result(&mut phone, &enter).await["accepted"], true);
    assert_eq!(saved_profile(&server, id)["care"]["tinyResident"], true);
    let resident = receive(&mut phone, |v| {
        v["type"] == "snapshot" && v["care"]["tinyResident"]["active"] == true
    })
    .await;
    assert_eq!(resident["care"]["tinyResident"]["mode"], "resident");

    let mut widget = connect_async(format!(
        "{}/ws?token={}&observe=1",
        server.url,
        owner["token"].as_str().unwrap()
    ))
    .await
    .unwrap()
    .0;
    let observed = receive(&mut widget, |v| v["type"] == "snapshot").await;
    assert_eq!(observed["connectionMode"], "observe");
    assert_eq!(server.state.game.lock().unwrap().players[id].connections, 1);
    let before = server.state.game.lock().unwrap().players[id].profile();
    for forbidden in [
        request(1, json!({"kind":"tiny_purchase","itemId":"garden-fruit"})),
        request(1, json!({"kind":"tiny_enter"})),
        json!({"type":"action","action":"tiny-start","gameId":"memory"}),
        json!({"type":"action","action":"watch-stop","sessionId":"forged"}),
        json!({"type":"action","action":"save"}),
        json!({"type":"input","dx":1,"dz":0}),
        json!({"type":"tiny-input","sessionId":"forged","input":{"kind":"move","x":1,"y":0}}),
    ] {
        send(&mut widget, &forbidden).await;
        let rejected = receive(&mut widget, |v| v["code"] == "observer-read-only").await;
        assert_eq!(rejected["kind"], "error");
        assert_eq!(
            server.state.game.lock().unwrap().players[id].profile(),
            before
        );
    }

    send(
        &mut phone,
        &json!({"type":"action","action":"tiny-start","gameId":"memory"}),
    )
    .await;
    let playing = receive(&mut phone, |v| {
        v["type"] == "snapshot" && v["tinyGame"]["phase"] == "playing"
    })
    .await;
    assert_eq!(playing["care"]["tinyResident"]["mode"], "minigame");
    assert_eq!(playing["care"]["tinyResident"]["active"], false);
    assert!(
        server.state.game.lock().unwrap().players[id]
            .watch
            .is_none()
    );
    let blocked_buy = request(1, json!({"kind":"tiny_purchase","itemId":"garden-fruit"}));
    send(&mut phone, &blocked_buy).await;
    assert_eq!(
        care_result(&mut phone, &blocked_buy).await["code"],
        "activity-conflict"
    );
    // This fixture runs only the transport router. Advance its trusted game
    // clock past the existing action cooldown before the next distinct action.
    server.state.game.lock().unwrap().tick(0.1);
    send(
        &mut phone,
        &json!({"type":"action","action":"tiny-stop","sessionId":playing["tinyGame"]["id"]}),
    )
    .await;
    receive(&mut phone, |v| {
        v["type"] == "snapshot"
            && v["tinyGame"].is_null()
            && v["care"]["tinyResident"]["active"] == true
    })
    .await;
    server.state.game.lock().unwrap().tick(0.1);
    send(
        &mut phone,
        &json!({"type":"action","action":"tiny-start","gameId":"ball"}),
    )
    .await;
    receive(&mut phone, |v| {
        v["type"] == "snapshot" && v["tinyGame"]["phase"] == "playing"
    })
    .await;
    phone.close(None).await.unwrap();
    let away = receive(&mut widget, |v| {
        v["type"] == "snapshot"
            && v["tinyGame"].is_null()
            && v["care"]["tinyResident"]["active"] == true
            && v["players"].as_array().is_some_and(|players| {
                players
                    .iter()
                    .any(|p| p["id"] == id && p["connected"] == false)
            })
    })
    .await;
    assert_eq!(away["connectionMode"], "observe");
    {
        let game = server.state.game.lock().unwrap();
        let p = &game.players[id];
        assert_eq!(
            p.connections, 0,
            "passive widget cannot keep a controlling phone online"
        );
        assert!(p.watch.is_some());
        assert_eq!(p.rings, 20);
        assert_eq!(p.care.revision, 1);
        assert!(p.care.inventory.is_empty());
    }
    widget.close(None).await.unwrap();
}
