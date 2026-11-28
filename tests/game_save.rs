//! The native game save route the recompiled game's server memory card uses.
//! Save bytes here are synthetic test input, never game data.
use axum::{
    Router,
    body::Body,
    http::{Request, Response, StatusCode},
};
use chao_game_server::{
    game_save::MAX_BYTES,
    service::{AppState, Config, router},
};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tower::ServiceExt;

async fn call(
    app: Router,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> Response<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    for (key, value) in headers {
        builder = builder.header(*key, *value);
    }
    app.oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}
async fn bytes(response: Response<Body>) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}
fn header(response: &Response<Body>, name: &str) -> String {
    response.headers()[name].to_str().unwrap().to_owned()
}
async fn token(state: &AppState) -> String {
    let response = call(
        router(state.clone()),
        "POST",
        "/api/session",
        &[("content-type", "application/json")],
        json!({"name":"Card"}).to_string().into_bytes(),
    )
    .await;
    let session: Value = serde_json::from_slice(&bytes(response).await).unwrap();
    session["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn checkout_write_conflicts_and_restart_keep_exact_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let app = || router(state.clone());
    let token = token(&state).await;
    let auth = format!("Bearer {token}");

    assert_eq!(
        call(app(), "GET", "/api/game-save", &[], vec![])
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            app(),
            "GET",
            "/api/game-save",
            &[("authorization", "Bearer wrong")],
            vec![]
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );

    // A new account has an empty card file at revision 0.
    let first = call(
        app(),
        "GET",
        "/api/game-save",
        &[("authorization", &auth)],
        vec![],
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(header(&first, "x-save-revision"), "0");
    let lease = header(&first, "x-save-lease");
    assert!(bytes(first).await.is_empty());

    // A second game can't take it; the holder can renew.
    let other = call(
        app(),
        "GET",
        "/api/game-save",
        &[("authorization", &auth)],
        vec![],
    )
    .await;
    assert_eq!(other.status(), StatusCode::CONFLICT);
    let renew = call(
        app(),
        "POST",
        "/api/game-save/lease",
        &[("authorization", &auth), ("x-save-lease", &lease)],
        vec![],
    )
    .await;
    assert_eq!(renew.status(), StatusCode::OK);

    let save: Vec<u8> = (0..70_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let put = |lease: &str, revision: &str, body: Vec<u8>| {
        let headers = [
            ("authorization", auth.as_str()),
            ("x-save-lease", lease),
            ("x-save-revision", revision),
        ];
        let headers: Vec<(String, String)> = headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let app = app();
        async move {
            let refs: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            call(app, "PUT", "/api/game-save", &refs, body)
                .await
                .status()
        }
    };
    assert_eq!(
        put("not-the-lease", "0", save.clone()).await,
        StatusCode::CONFLICT
    );
    assert_eq!(put(&lease, "5", save.clone()).await, StatusCode::CONFLICT);
    assert_eq!(put(&lease, "0", vec![]).await, StatusCode::BAD_REQUEST);
    assert_eq!(
        put(&lease, "0", vec![0; MAX_BYTES + 1]).await,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(put(&lease, "0", save.clone()).await, StatusCode::OK);
    // A stale revision can't overwrite a newer save.
    assert_eq!(put(&lease, "0", vec![1; 10]).await, StatusCode::CONFLICT);

    // Released, then read back after a restart: same bytes, revision 1.
    let release = call(
        app(),
        "DELETE",
        "/api/game-save/lease",
        &[("authorization", &auth), ("x-save-lease", &lease)],
        vec![],
    )
    .await;
    assert_eq!(release.status(), StatusCode::OK);
    let restarted = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let back = call(
        router(restarted),
        "GET",
        "/api/game-save",
        &[("authorization", &auth)],
        vec![],
    )
    .await;
    assert_eq!(back.status(), StatusCode::OK);
    assert_eq!(header(&back, "x-save-revision"), "1");
    assert_eq!(bytes(back).await, save);
    assert!(
        !std::fs::read_dir(dir.path().join("game-saves"))
            .unwrap()
            .any(|e| { e.unwrap().path().extension().is_some_and(|x| x == "tmp") })
    );
}

#[tokio::test]
async fn care_on_the_socket_is_refused_while_the_game_holds_the_chao() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let token = token(&state).await;
    let auth = format!("Bearer {token}");
    let checkout = call(
        router(state.clone()),
        "GET",
        "/api/game-save",
        &[("authorization", &auth)],
        vec![],
    )
    .await;
    assert_eq!(checkout.status(), StatusCode::OK);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let serving = tokio::spawn(axum::serve(listener, router(state.clone())).into_future());
    let (mut socket, _) = connect_async(format!("ws://{address}/ws?token={token}"))
        .await
        .unwrap();
    socket
        .send(Message::Text(
            json!({"type":"care","requestId":"r1","expectedRevision":0,"intent":{"kind":"pet"}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let refused = tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if let Some(Ok(Message::Text(text))) = socket.next().await {
                let value: Value = serde_json::from_str(&text).unwrap();
                // The first reply to the care request: a refusal, or a care result if the gate failed.
                if value["code"].is_string() || value["requestId"] == "r1" {
                    return value;
                }
            }
        }
    })
    .await
    .expect("a notice before the timeout");
    assert_eq!(refused["code"], "in-game");
    // Chat still reaches the room while the game holds the Chao.
    socket
        .send(Message::Text(
            json!({"type":"action","action":"chat","channel":"world","text":"hi"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let chat = tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if let Some(Ok(Message::Text(text))) = socket.next().await {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["kind"] == "chat" || value["code"].is_string() {
                    return value;
                }
            }
        }
    })
    .await
    .expect("a chat event before the timeout");
    assert_eq!(chat["kind"], "chat", "{chat}");

    // The one care request the game may send: it removes the item and nothing else.
    let before = {
        let mut game = state.game.lock().unwrap();
        let p = game.players.values_mut().next().unwrap();
        p.care.inventory.insert("garden-fruit".into(), 1);
        (
            p.care.revision,
            p.hunger,
            p.energy,
            p.happiness,
            p.stats.clone(),
        )
    };
    let give = |id: &str| {
        Message::Text(
            json!({"type":"care","requestId":id,"expectedRevision":before.0,
                "intent":{"kind":"game_give","itemId":"garden-fruit"}})
            .to_string()
            .into(),
        )
    };
    let first = "11111111-1111-4111-8111-111111111111";
    socket.send(give(first)).await.unwrap();
    let given = tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if let Some(Ok(Message::Text(text))) = socket.next().await {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["code"] == "in-game" || value["requestId"] == first {
                    return value;
                }
            }
        }
    })
    .await
    .expect("a care result before the timeout");
    assert_eq!(given["kind"], "care-result");
    assert_eq!(given["accepted"], true, "{given}");
    {
        let game = state.game.lock().unwrap();
        let p = game.players.values().next().unwrap();
        assert!(p.care.inventory.is_empty());
        assert_eq!(
            (
                p.care.revision,
                p.hunger,
                p.energy,
                p.happiness,
                p.stats.clone()
            ),
            (before.0 + 1, before.1, before.2, before.3, before.4)
        );
    }
    // Outside the game the same request is refused and the item stays.
    let id = {
        let mut game = state.game.lock().unwrap();
        game.game_save_leases.clear();
        let id = game.players.keys().next().unwrap().clone();
        game.players
            .get_mut(&id)
            .unwrap()
            .care
            .inventory
            .insert("garden-fruit".into(), 1);
        id
    };
    let mut game = state.game.lock().unwrap();
    let outside = state
        .care_action(
            &mut game,
            &id,
            &json!({"type":"care","requestId":"22222222-2222-4222-8222-222222222222",
            "expectedRevision":before.0 + 1,"intent":{"kind":"game_give","itemId":"garden-fruit"}}),
        )
        .unwrap();
    assert_eq!(outside["accepted"], false);
    assert_eq!(outside["code"], "not-in-game");
    assert_eq!(game.players[&id].care.inventory["garden-fruit"], 1);
    drop(game);
    serving.abort();
}

#[tokio::test]
async fn status_shows_the_card_without_taking_the_chao() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let app = || router(state.clone());
    let token = token(&state).await;
    let auth = format!("Bearer {token}");
    let status = || async {
        let response = call(
            app(),
            "GET",
            "/api/game-save/status",
            &[("authorization", &auth)],
            vec![],
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        serde_json::from_slice::<Value>(&bytes(response).await).unwrap()
    };
    assert_eq!(
        call(app(), "GET", "/api/game-save/status", &[], vec![])
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        status().await,
        json!({"revision":0,"savedAt":null,"inGame":false})
    );

    // Looking never checks the save out: the game can still take it.
    let checkout = call(
        app(),
        "GET",
        "/api/game-save",
        &[("authorization", &auth)],
        vec![],
    )
    .await;
    assert_eq!(checkout.status(), StatusCode::OK);
    let lease = header(&checkout, "x-save-lease");
    assert_eq!(status().await["inGame"], true);

    let put = call(
        app(),
        "PUT",
        "/api/game-save",
        &[
            ("authorization", &auth),
            ("x-save-lease", &lease),
            ("x-save-revision", "0"),
        ],
        vec![7; 100],
    )
    .await;
    assert_eq!(put.status(), StatusCode::OK);
    let saved = status().await;
    assert_eq!(saved["revision"], 1);
    assert!(saved["savedAt"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn every_chao_on_a_card_shares_its_game_save() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let session = |body: Value| {
        let app = router(state.clone());
        async move {
            let response = call(
                app,
                "POST",
                "/api/session",
                &[("content-type", "application/json")],
                body.to_string().into_bytes(),
            )
            .await;
            serde_json::from_slice::<Value>(&bytes(response).await).unwrap()
        }
    };
    let first = session(json!({"name":"One"})).await;
    let card = first["card"].as_str().unwrap().to_owned();
    let second = session(json!({"name":"Two","card":card})).await;
    let by = |credential: &str| format!("Bearer {credential}");

    // The game signs in with the card code; the Chao's own tokens see the same card.
    let checkout = call(
        router(state.clone()),
        "GET",
        "/api/game-save",
        &[("authorization", &by(&card))],
        vec![],
    )
    .await;
    assert_eq!(checkout.status(), StatusCode::OK);
    for chao in [&first, &second] {
        let status = call(
            router(state.clone()),
            "GET",
            "/api/game-save/status",
            &[("authorization", &by(chao["token"].as_str().unwrap()))],
            vec![],
        )
        .await;
        assert_eq!(json_of(status).await["inGame"], true);
    }
    // Inserting the card lists both Chao with the tokens that open them.
    let listed = call(
        router(state.clone()),
        "GET",
        "/api/card",
        &[("authorization", &by(&card))],
        vec![],
    )
    .await;
    let listed = json_of(listed).await;
    let tokens: Vec<_> = listed["chao"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["token"].clone())
        .collect();
    assert!(tokens.contains(&first["token"]) && tokens.contains(&second["token"]));
    assert_eq!(
        call(
            router(state.clone()),
            "GET",
            "/api/card",
            &[("authorization", "Bearer nope")],
            vec![]
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}
async fn json_of(response: Response<Body>) -> Value {
    serde_json::from_slice(&bytes(response).await).unwrap()
}

#[tokio::test]
async fn level_shoe_pickup_route_needs_a_token_and_a_known_item() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let app = || router(state.clone());
    let auth = format!("Bearer {}", token(&state).await);
    let json = ("content-type", "application/json");
    let pick = |item: &str| json!({ "item": item }).to_string().into_bytes();
    let no_token = call(
        app(),
        "POST",
        "/api/gear-pickup",
        &[json],
        pick("sonic-light-shoes"),
    )
    .await;
    assert_eq!(no_token.status(), StatusCode::UNAUTHORIZED);
    let unknown = call(
        app(),
        "POST",
        "/api/gear-pickup",
        &[json, ("authorization", &auth)],
        pick("knuckles-hammer-gloves"),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
    let ok = call(
        app(),
        "POST",
        "/api/gear-pickup",
        &[json, ("authorization", &auth)],
        pick("sonic-light-shoes"),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
    assert_eq!(json_of(ok).await["foundGear"], json!(["sonic-light-shoes"]));
}
