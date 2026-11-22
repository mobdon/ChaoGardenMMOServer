//! The live Chao record relay: each game sends its own Chao's raw record,
//! other games in the same room read it to draw that Chao.
//! Record bytes here are synthetic test input, never game data.
use axum::{
    Router,
    body::Body,
    http::{Request, Response, StatusCode},
};
use chao_game_server::service::{AppState, CHAO_RECORD_BYTES, Config, router};
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
    auth: Option<&str>,
    body: Vec<u8>,
) -> Response<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = auth {
        builder = builder.header("authorization", format!("Bearer {token}"));
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
async fn session(state: &AppState, name: &str) -> (String, String) {
    let response = call(
        router(state.clone()),
        "POST",
        "/api/session",
        None,
        json!({ "name": name }).to_string().into_bytes(),
    )
    .await;
    let s: Value = serde_json::from_slice(&bytes(response).await).unwrap();
    (
        s["token"].as_str().unwrap().into(),
        s["player"]["id"].as_str().unwrap().into(),
    )
}

#[tokio::test]
async fn own_record_relays_to_the_room_with_a_version() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let app = || router(state.clone());
    let (a, a_id) = session(&state, "Alpha").await;
    let (b, _) = session(&state, "Beta").await;
    let (c, _) = session(&state, "Gamma").await;
    let record = |fill: u8| vec![fill; CHAO_RECORD_BYTES];
    let put = |token: Option<String>, body: Vec<u8>| {
        let app = app();
        async move { call(app, "PUT", "/api/chao-record", token.as_deref(), body).await }
    };

    // Only the caller's own record, exactly one record long.
    assert_eq!(
        put(None, record(1)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        put(Some("nope".into()), record(1)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        put(Some(a.clone()), vec![1; CHAO_RECORD_BYTES - 1])
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        put(Some(a.clone()), vec![1; CHAO_RECORD_BYTES + 1])
            .await
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let version = |r: Response<Body>| async move {
        assert_eq!(r.status(), StatusCode::OK);
        serde_json::from_slice::<Value>(&bytes(r).await).unwrap()["version"].clone()
    };
    assert_eq!(version(put(Some(a.clone()), record(1)).await).await, 1);
    // The same bytes again keep the version; a change bumps it.
    assert_eq!(version(put(Some(a.clone()), record(1)).await).await, 1);
    assert_eq!(version(put(Some(a.clone()), record(2)).await).await, 2);

    // Offline: nobody reads it.
    let path = format!("/api/chao-record/{a_id}");
    assert_eq!(
        call(app(), "GET", &path, Some(&b), vec![]).await.status(),
        StatusCode::NOT_FOUND
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let serving = tokio::spawn(axum::serve(listener, router(state.clone())).into_future());
    let mut sockets = Vec::new();
    for token in [&a, &b, &c] {
        sockets.push(
            connect_async(format!("ws://{address}/ws?token={token}"))
                .await
                .unwrap()
                .0,
        );
    }
    // Gamma goes to another Chao World area: a different room.
    sockets[2]
        .send(Message::Text(
            json!({"type":"action","action":"chao-world","area":"garden"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();

    // Beta's snapshot lists Alpha with the record's version.
    let seen = tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if let Some(Ok(Message::Text(text))) = sockets[1].next().await {
                let v: Value = serde_json::from_str(&text).unwrap();
                if v["type"] == "snapshot"
                    && let Some(p) = v["players"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| p["id"] == a_id.as_str())
                {
                    return p["chaoRecordVersion"].clone();
                }
            }
        }
    })
    .await
    .expect("a snapshot listing Alpha");
    assert_eq!(seen, 2);

    let got = call(app(), "GET", &path, Some(&b), vec![]).await;
    assert_eq!(got.status(), StatusCode::OK);
    assert_eq!(got.headers()["x-chao-record-version"], "2");
    assert_eq!(bytes(got).await, record(2));
    assert_eq!(
        call(app(), "GET", &path, None, vec![]).await.status(),
        StatusCode::UNAUTHORIZED
    );
    // Another room's player can't read it (wait for Gamma's move to apply).
    let mut status = StatusCode::OK;
    for _ in 0..40 {
        status = call(app(), "GET", &path, Some(&c), vec![]).await.status();
        if status == StatusCode::NOT_FOUND {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        call(app(), "GET", "/api/chao-record/nobody", Some(&b), vec![])
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    serving.abort();
}

#[tokio::test]
async fn a_record_saves_the_chaos_stats_across_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let load = || AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let state = load();
    let (token, id) = session(&state, "Alpha").await;
    let put = |body: Vec<u8>| {
        call(
            router(state.clone()),
            "PUT",
            "/api/chao-record",
            Some(&token),
            body,
        )
    };
    let mut record = vec![0u8; CHAO_RECORD_BYTES];
    record[0x80] = 5;
    record[0x30 + 2] = 42; // Run level
    record[0x38 + 4..0x38 + 6].copy_from_slice(&1234u16.to_be_bytes()); // Run points
    assert_eq!(put(record).await.status(), StatusCode::OK);
    // An empty record (type 0) keeps the saved stats.
    assert_eq!(
        put(vec![0; CHAO_RECORD_BYTES]).await.status(),
        StatusCode::OK
    );
    state.persist(&mut state.game.lock().unwrap()).unwrap();

    let reloaded = load();
    let game = reloaded.game.lock().unwrap();
    let stats = &game.players[&id].profile()["nativeStats"];
    assert_eq!(stats["type"], 5);
    assert_eq!(stats["skills"][2]["level"], 42);
    assert_eq!(stats["skills"][2]["points"], 1234);
    assert_eq!(stats["skills"][0]["points"], 0);
}
