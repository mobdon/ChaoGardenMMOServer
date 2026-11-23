use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chao_game_server::{
    game::Event,
    service::{AppState, Config, router},
};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{fs, time::Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tower::ServiceExt;

async fn create_session(state: &AppState, data: Value) -> (StatusCode, Value) {
    let response = router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (status, body)
}
async fn serve(state: &AppState) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/ws", listener.local_addr().unwrap());
    let application = router(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, application).await.unwrap();
    });
    (url, server)
}
async fn await_metric(state: &AppState, line: &str) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !state.metrics.render().contains(line) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn profile_cap_keeps_resume_and_durable_ownership_available() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::local(dir.path().to_owned(), false);
    config.max_profiles = 1;
    let state = AppState::load(config).unwrap();
    let (status, original) = create_session(&state, json!({"name":"Original"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        create_session(&state, json!({"name":"Extra"})).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let (status, resumed) = create_session(
        &state,
        json!({"token":original["token"],"resumeOnly":true,"rings":99999,"admin":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed, original);
    let saved: Value =
        serde_json::from_slice(&fs::read(dir.path().join("profiles.json")).unwrap()).unwrap();
    assert_eq!(saved.as_array().unwrap().len(), 1);
    assert!(
        state
            .metrics
            .render()
            .contains("chao_admission_rejected_total 1\n")
    );
    assert!(
        state
            .metrics
            .render()
            .contains("chao_save_duration_seconds_count 1\n")
    );
}

#[tokio::test]
async fn connection_admission_releases_capacity_and_reconnect_reuses_canonical_entity() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::local(dir.path().to_owned(), false);
    config.max_connections = 1;
    let state = AppState::load(config).unwrap();
    let (_, session) = create_session(&state, json!({"name":"One"})).await;
    let (url, server) = serve(&state).await;
    let token_url = format!("{url}?token={}", session["token"].as_str().unwrap());
    let (mut first, _) = connect_async(&token_url).await.unwrap();
    first.next().await.unwrap().unwrap();
    let rejected = connect_async(&token_url).await.unwrap_err();
    assert!(
        matches!(rejected, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 503)
    );
    first.close(None).await.unwrap();
    drop(first);
    await_metric(&state, "chao_ws_connections 0\n").await;
    let (mut resumed, _) = connect_async(&token_url).await.unwrap();
    let snapshot: Value =
        serde_json::from_str(resumed.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(snapshot["selfId"], session["player"]["id"]);
    assert_eq!(state.game.lock().unwrap().players.len(), 1);
    await_metric(&state, "chao_reconnects_total 1\n").await;
    resumed.close(None).await.unwrap();
    drop(resumed);
    await_metric(&state, "chao_ws_connections 0\n").await;
    server.abort();
}

#[tokio::test]
async fn oversized_initial_snapshot_disconnects_and_releases_admission() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::local(dir.path().to_owned(), false);
    config.max_connections = 1;
    config.max_snapshot_bytes = 1;
    let state = AppState::load(config).unwrap();
    let (_, session) = create_session(&state, json!({})).await;
    let (url, server) = serve(&state).await;
    let token_url = format!("{url}?token={}", session["token"].as_str().unwrap());
    for expected in [1, 2] {
        let (mut socket, _) = connect_async(&token_url).await.unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap();
        assert!(!matches!(result, Some(Ok(Message::Text(_)))));
        await_metric(
            &state,
            &format!("chao_oversized_snapshots_total {expected}\n"),
        )
        .await;
        await_metric(&state, "chao_ws_connections 0\n").await;
    }
    assert!(state.metrics.render().contains("chao_snapshots_total 0\n"));
    server.abort();
}

#[tokio::test]
async fn lagged_reliable_event_stream_disconnects_instead_of_silently_skipping() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::local(dir.path().to_owned(), false);
    config.event_capacity = 2;
    let state = AppState::load(config).unwrap();
    let (_, session) = create_session(&state, json!({})).await;
    let (url, server) = serve(&state).await;
    let (mut socket, _) = connect_async(format!(
        "{url}?token={}",
        session["token"].as_str().unwrap()
    ))
    .await
    .unwrap();
    socket.next().await.unwrap().unwrap();
    // No await in the burst: this task deterministically overruns the bounded channel.
    for _ in 0..100 {
        state
            .events
            .send(Event {
                target: Some(session["player"]["id"].as_str().unwrap().to_owned()),
                message: json!({"type":"event","kind":"saved"}),
            })
            .unwrap();
    }
    await_metric(&state, "chao_ws_connections 0\n").await;
    assert!(!state.metrics.render().contains("chao_event_lag_total 0\n"));
    server.abort();
}

#[test]
fn failed_atomic_save_records_failure_latency_and_keeps_dirty_state() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().join("blocked"), false)).unwrap();
    fs::write(dir.path().join("blocked"), b"not a directory").unwrap();
    let mut game = state.game.lock().unwrap();
    game.create_session(&json!({})).unwrap();
    assert!(state.persist(&mut game).is_err());
    assert!(game.dirty);
    let metrics = state.metrics.render();
    assert!(metrics.contains("chao_persist_failures_total 1\n"));
    assert!(metrics.contains("chao_save_duration_seconds_count 1\n"));
}
