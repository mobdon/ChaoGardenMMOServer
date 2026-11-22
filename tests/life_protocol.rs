//! Life intents over the real WebSocket transport with a temporary profile store.
use chao_game_server::life::{Stage, native::BREED_INVITE};
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
            let owner = game.create_session(&json!({"name":"Mother"})).unwrap();
            let visitor = game.create_session(&json!({"name":"Father"})).unwrap();
            for (session, x) in [(&owner, -2.), (&visitor, -1.)] {
                let id = session["player"]["id"].as_str().unwrap().to_owned();
                let mut player = game.players.remove(&id).unwrap();
                game.teleport(&mut player, "garden", x, 2.).unwrap();
                player.life.stage = Stage::Adult;
                player.life.emotion.state[3] = BREED_INVITE;
                game.players.insert(id, player);
            }
            game.dirty = true;
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
                value => panic!("Socket ended before expected life result: {value:?}"),
            }
        }
    })
    .await
    .expect("authoritative life response before timeout")
}

async fn send(socket: &mut Socket, request: &Value) {
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
}

async fn life_result(socket: &mut Socket, request: &Value) -> Value {
    receive(socket, |value| {
        value["kind"] == "life-result" && value["requestId"] == request["requestId"]
    })
    .await
}

fn request(revision: u64, intent: Value) -> Value {
    json!({"type":"life","requestId":Uuid::new_v4().to_string(),"expectedRevision":revision,"intent":intent})
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
async fn breeding_over_sockets_lays_one_durable_egg_and_rolls_back_both_parents_on_save_failure() {
    let (server, mother, father) = TestServer::start().await;
    let mother_id = mother["player"]["id"].as_str().unwrap();
    let father_id = father["player"]["id"].as_str().unwrap();
    let mut mother_socket = server.socket(&mother).await;
    let mut father_socket = server.socket(&father).await;
    let offer = request(0, json!({"kind":"breed","partnerId":father_id}));
    send(&mut mother_socket, &offer).await;
    assert_eq!(
        life_result(&mut mother_socket, &offer).await["code"],
        "offer-sent"
    );
    let notice = receive(&mut father_socket, |v| v["kind"] == "life-breed-offer").await;
    assert_eq!(notice["fromId"], mother_id);

    // A failed save must leave both parents exactly as before.
    let accept = request(0, json!({"kind":"breed","partnerId":mother_id}));
    let blocked = server.directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocked).unwrap();
    send(&mut father_socket, &accept).await;
    let failed = life_result(&mut father_socket, &accept).await;
    assert_eq!(failed["accepted"], false);
    assert_eq!(failed["code"], "save-failed");
    {
        let game = server.state.game.lock().unwrap();
        assert!(game.players[mother_id].life.nest.is_empty());
        assert!(game.players[mother_id].life.breed_offer.is_some());
        assert_eq!(game.players[father_id].life.revision, 0);
        assert_eq!(game.players[father_id].life.emotion.state[3], BREED_INVITE);
    }
    fs::remove_dir(&blocked).unwrap();

    send(&mut father_socket, &accept).await;
    let laid = life_result(&mut father_socket, &accept).await;
    assert_eq!(laid["accepted"], true, "{laid}");
    assert_eq!(laid["code"], "egg-laid");
    send(&mut father_socket, &accept).await;
    let replay = life_result(&mut father_socket, &accept).await;
    assert_eq!(replay["replayed"], true);
    let disk = saved_profile(&server, mother_id);
    assert_eq!(disk["life"]["nest"].as_array().unwrap().len(), 1);
    assert_eq!(disk["life"]["nest"][0]["gene"]["fatherName"], "Father");
    let own = receive(&mut mother_socket, |v| {
        v["type"] == "snapshot" && v["life"]["nest"].as_array().is_some_and(|n| n.len() == 1)
    })
    .await;
    for player in own["players"].as_array().unwrap() {
        assert!(player["life"].get("nest").is_none());
        assert!(player["life"].get("gene").is_none());
    }
    mother_socket.close(None).await.unwrap();
    father_socket.close(None).await.unwrap();
    let restarted =
        AppState::load(Config::local(server.directory.path().to_owned(), false)).unwrap();
    let mut game = restarted.game.lock().unwrap();
    let again = restarted
        .life_action(&mut game, father_id, &accept)
        .unwrap();
    assert_eq!(again["replayed"], true);
    assert_eq!(game.players[mother_id].life.nest.len(), 1);
}

#[tokio::test]
async fn an_own_chao_on_the_same_card_is_a_partner_without_playing_or_answering() {
    let (server, mother, own) = TestServer::start().await;
    let mother_id = mother["player"]["id"].as_str().unwrap();
    let own_id = own["player"]["id"].as_str().unwrap();
    {
        // The second Chao sits on the first one's card, elsewhere, not in the mood.
        let mut game = server.state.game.lock().unwrap();
        let card = game.players[mother_id].card().to_owned();
        let mut chao = game.players.remove(own_id).unwrap();
        chao.card_hash = Some(card);
        chao.life.emotion.state[3] = 0;
        game.teleport(&mut chao, "hub", 0., 6.).unwrap();
        game.players.insert(own_id.to_owned(), chao);
    }
    let mut socket = server.socket(&mother).await;
    let breed = request(0, json!({"kind":"breed","partnerId":own_id}));
    send(&mut socket, &breed).await;
    let laid = life_result(&mut socket, &breed).await;
    assert_eq!(laid["code"], "egg-laid", "{laid}");
    let disk = saved_profile(&server, mother_id);
    assert_eq!(disk["life"]["nest"].as_array().unwrap().len(), 1);
    assert_eq!(disk["life"]["nest"][0]["gene"]["fatherName"], "Father");
    let game = server.state.game.lock().unwrap();
    assert!(game.players[mother_id].life.breed_offer.is_none());
    assert_eq!(game.players[mother_id].life.emotion.state[3], 0);
    assert!(game.players[own_id].life.nest.is_empty());
}
