//! Story progress routes: the server gates the game's stage order per card.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use chao_game_server::{
    service::{AppState, Config, router},
    story::{DARK, HERO},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(
    app: Router,
    method: &str,
    path: &str,
    auth: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", auth)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn clears_open_the_next_stage_in_order_and_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let load = || AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let state = load();
    let app = || router(state.clone());
    let (_, session) = call(app(), "POST", "/api/session", "", json!({"name":"Card"})).await;
    let auth = format!("Bearer {}", session["token"].as_str().unwrap());

    let (status, _) = call(app(), "GET", "/api/story", "Bearer nope", json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, story) = call(app(), "GET", "/api/story", &auth, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(story["unlocked"], json!([HERO[0], DARK[0]]));
    assert_eq!(story["superChao"], false);

    let clear = |stage: (u8, u8)| json!({"stage": stage.0, "character": stage.1});
    let (status, _) = call(app(), "POST", "/api/story/clear", &auth, clear(HERO[1])).await;
    assert_eq!(status, StatusCode::CONFLICT, "skipping ahead is refused");
    let (status, _) = call(
        app(),
        "POST",
        "/api/story/clear",
        &auth,
        json!({"stage": 13}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, story) = call(app(), "POST", "/api/story/clear", &auth, clear(HERO[0])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(story["unlocked"], json!([HERO[0], HERO[1], DARK[0]]));

    // A restarted server still has it.
    let (_, story) = call(router(load()), "GET", "/api/story", &auth, json!({})).await;
    assert_eq!(story["cleared"], json!([HERO[0]]));
}
