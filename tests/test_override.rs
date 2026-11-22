//! The override: for named test accounts on a test server, and the admin anywhere.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use chao_game_server::service::{AppState, Config, router};
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

/// A new Chao with an account named `username`; returns its Bearer header.
async fn account(state: &AppState, username: &str) -> String {
    let app = || router(state.clone());
    let (_, session) = call(app(), "POST", "/api/session", "", json!({"name": username})).await;
    let token = session["token"].as_str().unwrap();
    let (status, _) = call(
        app(),
        "POST",
        "/api/account/register",
        "",
        json!({"username": username, "email": format!("{username}@example.test"),
               "password": "synthetic-pass", "card": session["card"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    format!("Bearer {token}")
}

fn state(dir: &std::path::Path, test_accounts: &[&str]) -> AppState {
    let mut config = Config::local(dir.to_owned(), false);
    config.test_accounts = test_accounts.iter().map(|s| s.to_string()).collect();
    AppState::load(config).unwrap()
}

#[tokio::test]
async fn the_route_does_not_exist_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path(), &[]);
    let auth = account(&state, "tester").await;
    let (status, _) = call(
        router(state),
        "POST",
        "/api/test/override",
        &auth,
        json!({"story": "all"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_a_named_test_account_can_override_and_it_sticks() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path(), &["tester"]);
    let app = || router(state.clone());
    let tester = account(&state, "tester").await;
    let other = account(&state, "player").await;
    // A Chao with no account at all.
    let (_, session) = call(app(), "POST", "/api/session", "", json!({"name": "Loose"})).await;
    let loose = format!("Bearer {}", session["token"].as_str().unwrap());
    let body = json!({"story": "all", "stats": {"run": {"level": 99, "grade": 5, "points": 9999}}});

    for auth in [&other, &loose] {
        let (status, _) = call(app(), "POST", "/api/test/override", auth, body.clone()).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (_, story) = call(app(), "GET", "/api/story", auth, json!({})).await;
        assert_eq!(story["cleared"], json!([]));
    }
    let (status, _) = call(
        app(),
        "POST",
        "/api/test/override",
        "Bearer nope",
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // A bad value changes nothing, not even the valid half of the request.
    let bad = json!({"story": "all", "stats": {"run": {"level": 100}}});
    let (status, _) = call(app(), "POST", "/api/test/override", &tester, bad).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, story) = call(app(), "GET", "/api/story", &tester, json!({})).await;
    assert_eq!(story["superChao"], false);

    let (status, _) = call(app(), "POST", "/api/test/override", &tester, body).await;
    assert_eq!(status, StatusCode::OK);
    let (_, story) = call(app(), "GET", "/api/story", &tester, json!({})).await;
    assert_eq!(story["superChao"], true);
    {
        let game = state.game.lock().unwrap();
        let run = &game
            .player_for_token(tester.trim_start_matches("Bearer "))
            .unwrap()
            .stats["run"];
        assert_eq!((run.level, run.grade, run.points), (99, 5, 9999));
    }

    let (status, _) = call(
        app(),
        "POST",
        "/api/test/override",
        &tester,
        json!({"story": "none"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, story) = call(app(), "GET", "/api/story", &tester, json!({})).await;
    assert_eq!(story["cleared"], json!([]));
}

#[tokio::test]
async fn an_admin_card_needs_the_admin_key() {
    const KEY: &str = "synthetic-admin-key-0123456789abcdef";
    let keyed = |app: Router, auth: &str, key: Option<&str>| {
        let mut request = Request::builder()
            .method("POST")
            .uri("/api/test/override")
            .header("authorization", auth)
            .header("content-type", "application/json");
        if let Some(key) = key {
            request = request.header("x-admin-key", key);
        }
        let request = request
            .body(Body::from(json!({"story": "all"}).to_string()))
            .unwrap();
        async move { app.oneshot(request).await.unwrap().status() }
    };
    let dir = tempfile::tempdir().unwrap();
    // No test accounts named: what a hosted server runs with.
    let mut config = Config::local(dir.path().to_owned(), false);
    config.admin_key_hash = Some(chao_game_server::game::hash_token(KEY));
    let state = AppState::load(config).unwrap();
    let app = || router(state.clone());
    let admin = account(&state, "owner").await;
    for p in state.game.lock().unwrap().players.values_mut() {
        p.admin = true;
    }
    let player = account(&state, "player").await;

    // The key alone is not enough, and neither is the admin card alone.
    for (auth, key) in [
        (&player, Some(KEY)),
        (&admin, None),
        (&admin, Some("wrong")),
    ] {
        assert_eq!(keyed(app(), auth, key).await, StatusCode::NOT_FOUND);
        let (_, story) = call(app(), "GET", "/api/story", auth, json!({})).await;
        assert_eq!(story["superChao"], false);
    }
    assert_eq!(keyed(app(), &admin, Some(KEY)).await, StatusCode::OK);
    let (_, story) = call(app(), "GET", "/api/story", &admin, json!({})).await;
    assert_eq!(story["superChao"], true);
}

#[tokio::test]
async fn an_admin_card_is_refused_on_a_server_with_no_admin_key() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path(), &["tester"]);
    let auth = account(&state, "owner").await;
    for p in state.game.lock().unwrap().players.values_mut() {
        p.admin = true;
    }
    let (status, _) = call(
        router(state),
        "POST",
        "/api/test/override",
        &auth,
        json!({"story": "all"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
