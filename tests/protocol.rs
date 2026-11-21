use axum::{
    Router,
    body::Body,
    http::{Request, Response, StatusCode, header},
};
use chao_game_server::service::{AppState, Config, asset_filename, router, spawn_simulation};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{fs, time::Duration};
use tempfile::TempDir;
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::timeout,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{self, Message, client::IntoClientRequest},
};
use tower::ServiceExt;

fn fixture(owner: bool) -> (TempDir, AppState) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), owner)).unwrap();
    (dir, state)
}
async fn request(
    app: Router,
    method: &str,
    path: &str,
    body: impl Into<Body>,
    headers: &[(&str, &str)],
) -> Response<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    for (key, value) in headers {
        builder = builder.header(*key, *value);
    }
    app.oneshot(builder.body(body.into()).unwrap())
        .await
        .unwrap()
}
async fn json_body(response: Response<Body>) -> Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}
async fn session(state: &AppState, input: Value) -> Value {
    let response = request(
        router(state.clone()),
        "POST",
        "/api/session",
        input.to_string(),
        &[("content-type", "application/json")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

#[tokio::test]
async fn sessions_persist_hashes_resume_legacy_shape_and_ignore_privilege_fields() {
    let (dir, state) = fixture(true);
    let owner = session(
        &state,
        json!({"name":"<Owner>","vehicle":"dark-chao-walker"}),
    )
    .await;
    assert_eq!(owner["player"]["name"], "Owner");
    assert_eq!(owner["player"]["admin"], true);
    assert_eq!(owner["player"]["vehicle"], Value::Null);
    let regular = session(&state, json!({"name":"Visitor", "admin":true, "owner":true, "rings":9999, "stats":{"run":{"level":99}}, "vehicle":"dark-chao-walker"})).await;
    assert_eq!(regular["player"]["admin"], false);
    assert_eq!(regular["player"]["rings"], 20);
    assert_eq!(regular["player"]["stats"]["run"]["level"], 1);
    let persisted = fs::read_to_string(dir.path().join("profiles.json")).unwrap();
    assert!(!persisted.contains(owner["token"].as_str().unwrap()));
    assert!(!dir.path().join("profiles.json.tmp").exists());
    let restarted = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let resumed = session(
        &restarted,
        json!({"token":owner["token"], "name":"Ignored"}),
    )
    .await;
    // Resuming opens the Chao; only making it hands out its memory card code.
    assert_eq!(resumed["token"], owner["token"]);
    assert_eq!(resumed["player"], owner["player"]);
    assert!(owner["card"].is_string() && resumed.get("card").is_none());
}

#[tokio::test]
async fn session_rejects_invalid_types_content_and_oversized_bodies_without_creating_profiles() {
    let (dir, state) = fixture(false);
    for (body, content_type, expected) in [
        (
            "{}".to_owned(),
            "text/plain",
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        ("{".to_owned(), "application/json", StatusCode::BAD_REQUEST),
        ("[]".to_owned(), "application/json", StatusCode::BAD_REQUEST),
        (
            "null".to_owned(),
            "application/json",
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"name":"x".repeat(3000)}).to_string(),
            "application/json",
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let response = request(
            router(state.clone()),
            "POST",
            "/api/session",
            body,
            &[("content-type", content_type)],
        )
        .await;
        assert_eq!(response.status(), expected);
    }
    assert!(state.game.lock().unwrap().players.is_empty());
    assert!(!dir.path().join("profiles.json").exists());
}

#[tokio::test]
async fn resume_only_returns_404_without_mutating_save_slots() {
    let (_dir, state) = fixture(false);
    let owner = session(&state, json!({"name":"Original"})).await;
    for token in [Value::Null, json!("unknown"), json!("x".repeat(257))] {
        let response = request(
            router(state.clone()),
            "POST",
            "/api/session",
            json!({"resumeOnly":true,"token":token}).to_string(),
            &[("content-type", "application/json")],
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            json_body(response).await["error"],
            "Saved Chao was not found."
        );
        assert_eq!(state.game.lock().unwrap().players.len(), 1);
    }
    let resumed = session(&state, json!({"token":owner["token"],"resumeOnly":true})).await;
    assert_eq!(resumed["player"], owner["player"]);
}

#[tokio::test]
async fn activity_catalog_loads_once_exposes_details_and_keeps_snapshot_provenance_compact() {
    let (dir, _state) = fixture(false);
    fs::create_dir_all(dir.path().join("activities")).unwrap();
    fs::write(dir.path().join("activities/races.json"), json!({"courses":[{"id":"fixture-course","name":"Fixture course","group":"test","available":false,"source":{"evidence":"fixture-source"}}]}).to_string()).unwrap();
    let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
    let response = request(
        router(state.clone()),
        "GET",
        "/api/activities",
        Body::empty(),
        &[],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let catalog = json_body(response).await;
    assert_eq!(
        catalog["raceCourses"][0]["source"]["evidence"],
        "fixture-source"
    );
    assert_eq!(catalog["raceCourses"][0]["available"], false);
    let game = state.game.lock().unwrap();
    assert!(
        game.activities.public()["raceCourses"][0]
            .get("source")
            .is_none()
    );
}

#[tokio::test]
async fn origins_require_loopback_or_explicit_configuration() {
    let (dir, state) = fixture(false);
    for origin in [
        "https://evil.example",
        "null",
        "http://127.0.0.1.evil.example",
        "file://localhost",
    ] {
        let response = request(
            router(state.clone()),
            "GET",
            "/api/status",
            Body::empty(),
            &[("origin", origin)],
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{origin}");
    }
    let response = request(
        router(state),
        "GET",
        "/api/status",
        Body::empty(),
        &[("origin", "http://localhost:5173")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut config = Config::local(dir.path().to_owned(), false);
    config.allowed_origins.push("https://garden.example".into());
    let state = AppState::load(config).unwrap();
    assert_eq!(
        request(
            router(state),
            "GET",
            "/api/status",
            Body::empty(),
            &[("origin", "https://garden.example")]
        )
        .await
        .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn status_reports_cached_inventory_actual_imports_and_terrain_fallback() {
    let (dir, state) = fixture(false);
    fs::create_dir_all(dir.path().join("imports")).unwrap();
    fs::write(dir.path().join("imports/chao.glb"), b"test model").unwrap();
    fs::write(
        dir.path().join("game-inventory.json"),
        br#"{"found":true,"imports":{"chao":false,"garden":true}}"#,
    )
    .unwrap();
    let response = request(router(state), "GET", "/api/status", Body::empty(), &[]).await;
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    let status = json_body(response).await;
    assert_eq!(status["setup"]["found"], true);
    assert_eq!(status["setup"]["imports"]["chao"], true);
    assert_eq!(status["setup"]["imports"]["garden"], false);
    assert_eq!(status["terrain"]["loaded"], json!([]));
    assert_eq!(
        status["terrain"]["flatFallback"],
        json!(["hub", "garden", "hero", "dark"])
    );
}

#[tokio::test]
async fn assets_serve_exact_allowlisted_bytes_mime_head_and_reject_traversal() {
    let (dir, state) = fixture(false);
    let imports = dir.path().join("imports");
    fs::create_dir_all(&imports).unwrap();
    for (route, filename, mime) in [
        ("chao.glb", "chao.glb", "model/gltf-binary"),
        ("hub-grass.png", "nativehubgrass.png", "image/png"),
        ("voice-amy.mp3", "voice-amy.mp3", "audio/mpeg"),
        ("discord-icon.png", "discord-icon.png", "image/png"),
        ("login-chao.png", "login-chao.png", "image/png"),
        ("item-panel-24.png", "item-panel-24.png", "image/png"),
        (
            "chao-select-window.png",
            "chao-select-window.png",
            "image/png",
        ),
        ("item-panel-egg.png", "item-panel-egg.png", "image/png"),
        (
            "shadow-skate-motion.json",
            "shadow-skate-motion.json",
            "application/json; charset=utf-8",
        ),
        (
            "tiny-neutral-chaos.png",
            "tiny-neutral-chaos.png",
            "image/png",
        ),
        ("tiny-hero-chaos.png", "tiny-hero-chaos.png", "image/png"),
        ("tiny-dark-chaos.png", "tiny-dark-chaos.png", "image/png"),
        (
            "race-normal.glb",
            "activities/race-normal.glb",
            "model/gltf-binary",
        ),
        (
            "race-dark.glb",
            "activities/race-dark.glb",
            "model/gltf-binary",
        ),
        (
            "race-hero.glb",
            "activities/race-hero.glb",
            "model/gltf-binary",
        ),
        (
            "karate-arena.glb",
            "activities/karate-arena.glb",
            "model/gltf-binary",
        ),
    ] {
        let bytes = format!("fixture:{filename}").into_bytes();
        fs::create_dir_all(imports.join(filename).parent().unwrap()).unwrap();
        fs::write(imports.join(filename), &bytes).unwrap();
        let path = format!("/api/assets/{route}");
        let response = request(router(state.clone()), "GET", &path, Body::empty(), &[]).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], mime);
        assert_eq!(
            response.headers()[header::CONTENT_LENGTH],
            bytes.len().to_string()
        );
        assert_eq!(
            &response.into_body().collect().await.unwrap().to_bytes()[..],
            &bytes
        );
        let response = request(router(state.clone()), "HEAD", &path, Body::empty(), &[]).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_LENGTH],
            bytes.len().to_string()
        );
        assert!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .is_empty()
        );
    }
    for path in [
        "/api/assets/unknown.glb",
        "/api/assets/../../profiles.json",
        "/api/assets/%2e%2e/%2e%2e/profiles.json",
        "/api/assets/../assets/chao.glb",
        "/api/assets/%2e%2e/assets/chao.glb",
        "/api/assets/chao.glb/../../profiles.json",
        "/api/assets/activities/race-normal.glb",
        "/api/assets/activities/../chao.glb",
        "/api/assets/unknown.json",
        "/api/assets/shadow-skate-motion.json/../profiles.json",
    ] {
        assert!(asset_filename(path).is_none());
        assert_eq!(
            request(router(state.clone()), "GET", path, Body::empty(), &[])
                .await
                .status(),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    assert_eq!(
        request(
            router(state.clone()),
            "GET",
            "/api/assets/dark-chaos.glb",
            Body::empty(),
            &[]
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            router(state),
            "POST",
            "/api/assets/chao.glb",
            Body::empty(),
            &[]
        )
        .await
        .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn native_chao_shadow_png_is_exact_missing_safe_and_head_compatible() {
    let (directory, state) = fixture(false);
    let route = "/api/assets/chao-shadow.png";
    assert_eq!(asset_filename(route).as_deref(), Some("chao-shadow.png"));
    assert_eq!(
        asset_filename("/api/assets/chao-behaviour.glb").as_deref(),
        Some("chao-behaviour.glb")
    );
    for method in ["GET", "HEAD"] {
        let response = request(router(state.clone()), method, route, Body::empty(), &[]).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    }
    let imports = directory.path().join("imports");
    fs::create_dir_all(&imports).unwrap();
    let bytes = b"\x89PNG\r\n\x1a\nprivate synthetic shadow alias fixture";
    fs::write(imports.join("chao-shadow.png"), bytes).unwrap();
    fs::write(
        imports.join("chao-shadow-source.prs"),
        b"private native source",
    )
    .unwrap();
    for method in ["GET", "HEAD"] {
        let response = request(router(state.clone()), method, route, Body::empty(), &[]).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(
            response.headers()[header::CONTENT_LENGTH],
            bytes.len().to_string()
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        if method == "HEAD" {
            assert!(body.is_empty());
        } else {
            assert_eq!(&body[..], bytes);
        }
    }
    for path in [
        "/api/assets/chao-shadow-source.prs",
        "/api/assets/chao-shadow.json",
        "/api/assets/chao-shadow.png.json",
        "/api/assets/CHAO-SHADOW.PNG",
        "/api/assets/imports/chao-shadow.png",
        "/api/assets/chao-shadow.png/../profiles.json",
        "/api/assets/%2e%2e/imports/chao-shadow.png",
        "/api/assets/item-panel-25.png",
        "/api/assets/item-panel-01.png",
    ] {
        assert!(asset_filename(path).is_none(), "{path}");
        assert_eq!(
            request(router(state.clone()), "GET", path, Body::empty(), &[])
                .await
                .status(),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
}

#[tokio::test]
async fn audio_ranges_support_closed_open_suffix_and_unsatisfiable_requests() {
    let (dir, state) = fixture(false);
    fs::create_dir_all(dir.path().join("imports")).unwrap();
    fs::write(dir.path().join("imports/music-hub.mp3"), b"0123456789").unwrap();
    for (range, expected, content_range) in [
        ("bytes=2-5", "2345", "bytes 2-5/10"),
        ("bytes=7-", "789", "bytes 7-9/10"),
        ("bytes=-3", "789", "bytes 7-9/10"),
        ("bytes=8-99", "89", "bytes 8-9/10"),
    ] {
        let response = request(
            router(state.clone()),
            "GET",
            "/api/assets/music-hub.mp3",
            Body::empty(),
            &[("range", range)],
        )
        .await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[header::CONTENT_RANGE], content_range);
        assert_eq!(response.headers()[header::ACCEPT_RANGES], "bytes");
        assert_eq!(
            &response.into_body().collect().await.unwrap().to_bytes()[..],
            expected.as_bytes()
        );
    }
    for range in [
        "bytes=10-",
        "bytes=-0",
        "bytes=5-2",
        "bytes=0-1,5-6",
        "bytes=-",
        "invalid",
    ] {
        let response = request(
            router(state.clone()),
            "GET",
            "/api/assets/music-hub.mp3",
            Body::empty(),
            &[("range", range)],
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::RANGE_NOT_SATISFIABLE,
            "{range}"
        );
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes */10");
    }
}

#[tokio::test]
async fn native_stat_ui_exposes_only_selected_exact_png_routes() {
    let (directory, state) = fixture(false);
    fs::create_dir_all(directory.path().join("imports")).unwrap();
    for name in [
        "chao-stat-panel.png",
        "chao-stat-font.png",
        "chao-race-parameter.png",
    ] {
        let bytes = format!("isolated native alias fixture {name}");
        fs::write(
            directory.path().join("imports").join(name),
            bytes.as_bytes(),
        )
        .unwrap();
        let response = request(
            router(state.clone()),
            "GET",
            &format!("/api/assets/{name}"),
            Body::empty(),
            &[],
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(
            &response.into_body().collect().await.unwrap().to_bytes()[..],
            bytes.as_bytes()
        );
    }
    for path in [
        "/api/assets/al_tex_common.prs",
        "/api/assets/level00.png",
        "/api/assets/chao-ui-provenance.json",
        "/api/assets/../chao-stat-panel.png",
    ] {
        assert_eq!(
            request(router(state.clone()), "GET", path, Body::empty(), &[])
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
}

#[test]
fn bad_persistent_data_fails_startup_without_overwriting_profiles() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("profiles.json"), b"broken").unwrap();
    assert!(AppState::load(Config::local(dir.path().to_owned(), false)).is_err());
    assert_eq!(
        fs::read(dir.path().join("profiles.json")).unwrap(),
        b"broken"
    );
    fs::write(dir.path().join("profiles.json"), b"[]").unwrap();
    fs::create_dir_all(dir.path().join("terrain")).unwrap();
    fs::write(
        dir.path().join("terrain/garden.json"),
        br#"{"minX":0,"minZ":0,"step":0,"width":1,"depth":1,"heights":[0]}"#,
    )
    .unwrap();
    assert!(AppState::load(Config::local(dir.path().to_owned(), false)).is_err());
    let mut config = Config::local(dir.path().to_owned(), true);
    config.address.set_ip("0.0.0.0".parse().unwrap());
    assert!(AppState::load(config).is_err());
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
struct LiveServer {
    _directory: TempDir,
    state: AppState,
    url: String,
    serving: JoinHandle<()>,
    simulation: JoinHandle<()>,
}
impl LiveServer {
    async fn start(owner: bool) -> Self {
        let (directory, state) = fixture(owner);
        Self::start_state(directory, state).await
    }
    async fn start_state(directory: TempDir, state: AppState) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let app = router(state.clone());
        let mut shutdown = state.shutdown.subscribe();
        let serving = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown.changed().await;
                })
                .await
                .unwrap();
        });
        let simulation = spawn_simulation(state.clone());
        Self {
            _directory: directory,
            state,
            url,
            serving,
            simulation,
        }
    }
    async fn socket(&self, session: &Value) -> Socket {
        let request = format!(
            "{}/ws?token={}",
            self.url,
            session["token"].as_str().unwrap()
        );
        connect_async(request).await.unwrap().0
    }
}
impl Drop for LiveServer {
    fn drop(&mut self) {
        let _ = self.state.shutdown.send(true);
        self.serving.abort();
        self.simulation.abort();
    }
}
async fn snapshot_until(socket: &mut Socket, matches: impl Fn(&Value) -> bool) -> Value {
    timeout(Duration::from_secs(4), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&text)
                        && value["type"] == "snapshot"
                        && matches(&value)
                    {
                        return value;
                    }
                }
                Some(Err(error)) => panic!("WebSocket ended before expected snapshot: {error}"),
                None | Some(Ok(Message::Close(_))) => {
                    panic!("WebSocket closed before expected snapshot")
                }
                _ => {}
            }
        }
    })
    .await
    .expect("expected authoritative snapshot before timeout")
}
fn player<'a>(snapshot: &'a Value, id: &str) -> &'a Value {
    snapshot["players"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["id"] == id)
        .unwrap()
}

#[tokio::test]
async fn two_real_websocket_peers_share_server_movement_and_reject_forged_state() {
    let server = LiveServer::start(false).await;
    let first = session(&server.state, json!({"name":"First"})).await;
    let second = session(&server.state, json!({"name":"Second"})).await;
    let first_id = first["player"]["id"].as_str().unwrap();
    let mut one = server.socket(&first).await;
    let mut two = server.socket(&second).await;
    snapshot_until(&mut one, |s| s["players"].as_array().unwrap().len() == 2).await;
    snapshot_until(&mut two, |s| s["players"].as_array().unwrap().len() == 2).await;
    one.send(Message::Text(json!({"type":"input","dx":99999,"dz":99999,"x":9999,"y":9999,"z":9999,"admin":true,"rings":9999,"vehicle":"dark-chao-walker","boost":true,"boostEnergy":999999}).to_string().into())).await.unwrap();
    one.send(Message::Text(
        json!({"type":"action","action":"mount-dark-walker","admin":true})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    one.send(Message::Text(json!({"type":"action","action":"mission-complete","rings":9999,"stats":{"run":{"level":99}}}).to_string().into())).await.unwrap();
    let observed = snapshot_until(&mut two, |s| {
        player(s, first_id)["x"].as_f64().unwrap() > 0.2
    })
    .await;
    let p = player(&observed, first_id);
    let x = p["x"].as_f64().unwrap();
    let z = p["z"].as_f64().unwrap();
    assert!((x - (z - 6.)).abs() < 0.001);
    assert!(x.hypot(z - 6.) < 1.5);
    assert_eq!(p["y"], 0.);
    assert_eq!(p["admin"], false);
    assert_eq!(p["vehicle"], Value::Null);
    assert_eq!(p["rings"], 20);
    assert_eq!(p["stats"]["run"]["level"], 1);
    assert!(
        !observed
            .to_string()
            .contains(first["token"].as_str().unwrap())
    );
    one.close(None).await.unwrap();
    snapshot_until(&mut two, |s| s["players"].as_array().unwrap().len() == 1).await;
    two.close(None).await.unwrap();
}

#[tokio::test]
async fn garden_resident_remains_visible_after_owner_disconnect_and_rejoins_once() {
    let server = LiveServer::start(false).await;
    let owner = session(&server.state, json!({"name":"Resident"})).await;
    let visitor = session(&server.state, json!({"name":"Visitor"})).await;
    let id = owner["player"]["id"].as_str().unwrap();
    let mut resident = server.socket(&owner).await;
    let mut observer = server.socket(&visitor).await;
    let initial = snapshot_until(&mut resident, |s| {
        s["players"].as_array().unwrap().len() == 2
    })
    .await;
    let stats = player(&initial, id)["stats"].clone();
    let rings = player(&initial, id)["rings"].clone();
    resident
        .send(Message::Text(
            json!({"type":"action","action":"watch-start"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let active = snapshot_until(&mut resident, |s| player(s, id)["watch"].is_object()).await;
    // A hub observer cannot see the garden resident until entering that room.
    snapshot_until(&mut observer, |s| {
        s["players"].as_array().unwrap().len() == 1
    })
    .await;
    observer
        .send(Message::Text(
            json!({"type":"action","action":"garden"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    snapshot_until(&mut observer, |s| {
        s["players"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == id)
    })
    .await;
    let run = player(&active, id)["watch"]["id"].clone();
    let disk: Value =
        serde_json::from_slice(&fs::read(server._directory.path().join("profiles.json")).unwrap())
            .unwrap();
    let saved = disk
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap();
    assert_eq!(saved["resident"]["enabled"], true);
    resident.close(None).await.unwrap();
    let away = snapshot_until(&mut observer, |s| player(s, id)["connected"] == false).await;
    assert_eq!(player(&away, id)["watch"]["ownerOnline"], false);
    let x = player(&away, id)["x"].as_f64().unwrap();
    let z = player(&away, id)["z"].as_f64().unwrap();
    let moved = snapshot_until(&mut observer, |s| {
        let p = player(s, id);
        (p["x"].as_f64().unwrap() - x).hypot(p["z"].as_f64().unwrap() - z) > 0.1
    })
    .await;
    assert_eq!(player(&moved, id)["stats"], stats);
    assert_eq!(player(&moved, id)["rings"], rings);
    let mut returned = server.socket(&owner).await;
    let joined = snapshot_until(&mut returned, |s| player(s, id)["connected"] == true).await;
    assert_eq!(
        joined["players"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["id"] == id)
            .count(),
        1
    );
    assert_eq!(player(&joined, id)["watch"]["id"], run);
    let after = joined["serverTime"].as_f64().unwrap() + 150.;
    snapshot_until(&mut returned, |s| {
        s["serverTime"].as_f64().unwrap() >= after
    })
    .await;
    returned
        .send(Message::Text(
            json!({"type":"action","action":"watch-stop","sessionId":run})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let direct = snapshot_until(&mut returned, |s| player(s, id)["watch"].is_null()).await;
    assert_eq!(player(&direct, id)["stats"], stats);
    assert_eq!(player(&direct, id)["rings"], rings);
    let disk: Value =
        serde_json::from_slice(&fs::read(server._directory.path().join("profiles.json")).unwrap())
            .unwrap();
    let saved = disk
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap();
    assert!(saved["resident"].is_null());
    returned.close(None).await.unwrap();
    observer.close(None).await.unwrap();
}

#[tokio::test]
async fn websocket_handshake_requires_a_real_token_and_allowed_origin() {
    let server = LiveServer::start(false).await;
    let error = connect_async(format!("{}/ws?token=invalid", server.url))
        .await
        .unwrap_err();
    assert!(
        matches!(error, tungstenite::Error::Http(response) if response.status() == StatusCode::UNAUTHORIZED)
    );
    let session = session(&server.state, json!({})).await;
    let mut request = format!(
        "{}/ws?token={}",
        server.url,
        session["token"].as_str().unwrap()
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert(header::ORIGIN, "https://evil.example".parse().unwrap());
    let error = connect_async(request).await.unwrap_err();
    assert!(
        matches!(error, tungstenite::Error::Http(response) if response.status() == StatusCode::FORBIDDEN)
    );
    assert_eq!(server.state.game.lock().unwrap().online_connections(), 0);
}

#[tokio::test]
async fn websocket_flood_closes_with_policy_violation() {
    let server = LiveServer::start(false).await;
    let session = session(&server.state, json!({})).await;
    let mut socket = server.socket(&session).await;
    for _ in 0..90 {
        if socket.send(Message::Text("{}".into())).await.is_err() {
            break;
        }
    }
    let close = timeout(Duration::from_secs(4), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Close(frame))) => return frame,
                Some(Err(_)) | None => panic!("missing policy close"),
                _ => {}
            }
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        close.code,
        tungstenite::protocol::frame::coding::CloseCode::Policy
    );
}

#[tokio::test]
async fn owner_mech_thrust_and_shot_events_are_authoritative_for_other_peers() {
    let server = LiveServer::start(true).await;
    let owner = session(&server.state, json!({"name":"Pilot"})).await;
    let guest = session(&server.state, json!({"name":"Observer"})).await;
    let owner_id = owner["player"]["id"].as_str().unwrap();
    let mut pilot = server.socket(&owner).await;
    let mut observer = server.socket(&guest).await;
    pilot
        .send(Message::Text(
            json!({"type":"action","action":"mount-dark-walker"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    snapshot_until(&mut observer, |s| {
        player(s, owner_id)["vehicle"] == "dark-chao-walker"
    })
    .await;
    pilot
        .send(Message::Text(
            json!({"type":"input","boost":true,"boostEnergy":999999,"y":99999})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let flying = snapshot_until(&mut observer, |s| {
        player(s, owner_id)["mech"]["boosting"] == true
    })
    .await;
    let mech = &player(&flying, owner_id)["mech"];
    assert!(mech["boostEnergy"].as_f64().unwrap() < 100.);
    assert_eq!(mech["grounded"], false);
    assert!(player(&flying, owner_id)["jumpOffset"].as_f64().unwrap() <= 3.);
    pilot
        .send(Message::Text(
            json!({"type":"action","action":"mech-fire","damage":999999,"hitX":999999})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let shot = timeout(Duration::from_secs(4), async {
        loop {
            match observer.next().await {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if value["kind"] == "mech-shot" {
                        return value;
                    }
                }
                Some(Err(error)) => panic!("observer WebSocket failed: {error}"),
                None | Some(Ok(Message::Close(_))) => panic!("observer closed before cannon event"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(shot["playerId"], owner_id);
    assert_eq!(shot["hit"], false);
    assert_eq!(shot["targetId"], Value::Null);
    let distance = (shot["hitX"].as_f64().unwrap() - shot["x"].as_f64().unwrap())
        .hypot(shot["hitZ"].as_f64().unwrap() - shot["z"].as_f64().unwrap());
    assert!((distance - 9.).abs() < 0.001);
    snapshot_until(&mut observer, |s| {
        player(s, owner_id)["mech"]["grounded"] == true
            && player(s, owner_id)["mech"]["boosting"] == false
    })
    .await;
    pilot.close(None).await.unwrap();
    observer.close(None).await.unwrap();
}

#[tokio::test]
async fn explicit_save_acknowledges_only_a_successful_authoritative_flush_and_recovers_from_failure()
 {
    let server = LiveServer::start(true).await;
    let session = session(&server.state, json!({"name":"Save owner"})).await;
    let id = session["player"]["id"].as_str().unwrap();
    let mut socket = server.socket(&session).await;
    socket
        .send(Message::Text(
            json!({"type":"action","action":"equip-owner-gear","item":"shadow-air-shoes"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    snapshot_until(&mut socket, |s| {
        player(s, id)["gear"]["footwear"] == "shadow-air-shoes"
    })
    .await;
    socket.send(Message::Text(json!({"type":"action","action":"save","playerId":"forged","rings":999999,"profiles":[{"admin":false}]}).to_string().into())).await.unwrap();
    let acknowledgement = save_result(&mut socket).await;
    assert_eq!(acknowledgement["kind"], "saved");
    assert_eq!(acknowledgement["playerId"], id);
    let profiles: Value =
        serde_json::from_slice(&fs::read(server._directory.path().join("profiles.json")).unwrap())
            .unwrap();
    assert_eq!(profiles[0]["gear"]["footwear"], "shadow-air-shoes");
    assert_eq!(profiles[0]["rings"], 20);
    assert_eq!(profiles[0]["admin"], true);
    tokio::time::sleep(Duration::from_millis(350)).await;
    let blocked = server._directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocked).unwrap();
    socket
        .send(Message::Text(
            json!({"type":"action","action":"save"}).to_string().into(),
        ))
        .await
        .unwrap();
    let failure = save_result(&mut socket).await;
    assert_eq!(failure["type"], "notice");
    assert_eq!(failure["kind"], "error");
    assert!(failure["text"].as_str().unwrap().contains("Could not save"));
    assert!(server.state.game.lock().unwrap().dirty);
    fs::remove_dir(blocked).unwrap();
    tokio::time::sleep(Duration::from_millis(350)).await;
    socket
        .send(Message::Text(
            json!({"type":"action","action":"save"}).to_string().into(),
        ))
        .await
        .unwrap();
    assert_eq!(save_result(&mut socket).await["kind"], "saved");
    let restarted =
        AppState::load(Config::local(server._directory.path().to_owned(), false)).unwrap();
    let saved = restarted.game.lock().unwrap();
    let restored = saved
        .player_for_token(session["token"].as_str().unwrap())
        .unwrap();
    assert_eq!(restored.gear.footwear.as_deref(), Some("shadow-air-shoes"));
    assert!(restored.admin);
}

async fn save_result(socket: &mut Socket) -> Value {
    timeout(Duration::from_secs(4), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if value["kind"] == "saved"
                        || (value["type"] == "notice" && value["kind"] == "error")
                    {
                        return value;
                    }
                }
                Some(Err(error)) => panic!("save WebSocket failed: {error}"),
                None | Some(Ok(Message::Close(_))) => panic!("WebSocket closed before save result"),
                _ => {}
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn real_phone_socket_runs_all_five_and_obeys_session_owned_input_and_stop() {
    let server = LiveServer::start(false).await;
    let session = session(&server.state, json!({"name":"Phone"})).await;
    let id = session["player"]["id"].as_str().unwrap();
    let mut socket = server.socket(&session).await;
    let mut last = snapshot_until(&mut socket, |s| s["tinyGame"].is_null()).await;
    for game_id in ["memory", "rock_paper_scissors", "bounce", "ball", "search"] {
        let after = last["serverTime"].as_f64().unwrap() + 150.;
        snapshot_until(&mut socket, |s| s["serverTime"].as_f64().unwrap() >= after).await;
        socket
            .send(Message::Text(
                json!({"type":"action","action":"tiny-start","gameId":game_id,"rings":999999})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        let active = snapshot_until(&mut socket, |s| s["tinyGame"]["gameId"] == game_id).await;
        let run = active["tinyGame"]["id"].clone();
        assert_eq!(active["tinyGame"]["view"]["game"], game_id);
        assert_eq!(player(&active, id)["rings"], 20);
        socket.send(Message::Text(json!({"type":"tiny-input","sessionId":run,"input":{"kind":"finish","rings":999999},"dt":999999}).to_string().into())).await.unwrap();
        if game_id == "bounce" {
            socket
                .send(Message::Text(
                    json!({"type":"tiny-input","sessionId":run,"input":{"kind":"bounce_spring"}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            snapshot_until(&mut socket, |s| {
                s["tinyGame"]["view"]["state"]["waiting"] == false
            })
            .await;
        }
        let after = active["serverTime"].as_f64().unwrap() + 150.;
        snapshot_until(&mut socket, |s| s["serverTime"].as_f64().unwrap() >= after).await;
        socket
            .send(Message::Text(
                json!({"type":"action","action":"tiny-stop","sessionId":run})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        let stopped = snapshot_until(&mut socket, |s| s["tinyGame"].is_null()).await;
        assert_eq!(player(&stopped, id)["rings"], 20);
        last = stopped;
    }
    socket.close(None).await.unwrap();
}

#[tokio::test]
async fn phone_socket_result_waits_for_durable_receipt_and_retries_without_duplicate_credit() {
    let server = LiveServer::start(false).await;
    let session = session(&server.state, json!({"name":"Atomic socket"})).await;
    let id = session["player"]["id"].as_str().unwrap();
    let mut socket = server.socket(&session).await;
    socket
        .send(Message::Text(
            json!({"type":"action","action":"tiny-start","gameId":"rock_paper_scissors"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let active = snapshot_until(&mut socket, |s| s["tinyGame"]["phase"] == "playing").await;
    let run = active["tinyGame"]["id"].clone();
    let blocked = server._directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocked).unwrap();
    {
        let mut game = server.state.game.lock().unwrap();
        for _ in 0..300 {
            game.tick(0.1);
        }
    }
    let pending = snapshot_until(&mut socket, |s| s["tinyGame"]["phase"] == "saving").await;
    assert_eq!(player(&pending, id)["rings"], 20);
    assert_eq!(pending["tinyGame"]["result"]["saved"], false);
    assert!(
        player(&pending, id)["tinyRecords"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    fs::remove_dir(blocked).unwrap();
    socket
        .send(Message::Text(
            json!({"type":"action","action":"save","rings":999999})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let result = timeout(Duration::from_secs(4), async {
        loop {
            if let Some(Ok(Message::Text(bytes))) = socket.next().await {
                let value: Value = serde_json::from_str(&bytes).unwrap();
                if value["kind"] == "tiny-game-result" {
                    break value;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(result["id"], run);
    assert_eq!(result["saved"], true);
    assert_eq!(result["rings"], 0);
    assert_eq!(result["reason"], "time_up");
    let snapshot = snapshot_until(&mut socket, |s| s["tinyGame"]["phase"] == "finished").await;
    assert_eq!(
        player(&snapshot, id)["tinyRecords"]["rock_paper_scissors"]["plays"],
        1
    );
    let disk: Value =
        serde_json::from_slice(&fs::read(server._directory.path().join("profiles.json")).unwrap())
            .unwrap();
    assert_eq!(
        disk[0]["tinyRecords"]["rock_paper_scissors"]["lastResultId"],
        run
    );
    assert_eq!(disk[0]["rings"], 20);
    socket.send(Message::Text(json!({"type":"tiny-input","sessionId":run,"input":{"kind":"rps_shoot"},"rings":999999}).to_string().into())).await.unwrap();
    let unchanged = snapshot_until(&mut socket, |s| s["tinyGame"]["phase"] == "finished").await;
    assert_eq!(player(&unchanged, id)["rings"], 20);
    assert_eq!(
        player(&unchanged, id)["tinyRecords"]["rock_paper_scissors"]["plays"],
        1
    );
    socket.close(None).await.unwrap();
}

#[test]
fn care_model_routes_are_exact_allowlisted_names() {
    for (path, local) in [
        (
            "/api/assets/animal-part-penguin-child-leftArm.glb",
            "care/animal-part-penguin-child-leftArm.glb",
        ),
        (
            "/api/assets/animal-part-phoenix-adult-tail.glb",
            "care/animal-part-phoenix-adult-tail.glb",
        ),
        (
            "/api/assets/tree-hero-mature.glb",
            "care/tree-hero-mature.glb",
        ),
        (
            "/api/assets/fruit-chao-garden.glb",
            "care/fruit-chao-garden.glb",
        ),
        ("/api/assets/hat-3.glb", "care/hat-3.glb"),
        ("/api/assets/hat-15.glb", "care/hat-15.glb"),
        ("/api/assets/hat-9-eye.glb", "care/hat-9-eye.glb"),
        ("/api/assets/hat-9-jaw.glb", "care/hat-9-jaw.glb"),
    ] {
        assert_eq!(asset_filename(path).as_deref(), Some(local), "{path}");
    }
    for path in [
        "/api/assets/animal-part-yellow-chaos-drive-child-tail.glb",
        "/api/assets/animal-part-penguin-teen-tail.glb",
        "/api/assets/animal-part-penguin-child-hat.glb",
        "/api/assets/tree-banana-mature.glb",
        "/api/assets/tree-hero-seed.glb",
        "/api/assets/fruit-../profiles.json",
        "/api/assets/fruit-chao-garden.png",
        "/api/assets/care/tree-hero-mature.glb",
        "/api/assets/hat-1.glb",
        "/api/assets/hat-2.glb",
        "/api/assets/hat-16.glb",
        "/api/assets/hat-03.glb",
        "/api/assets/hat-3-eye.glb",
        "/api/assets/hat-9.png",
    ] {
        assert!(asset_filename(path).is_none(), "{path}");
    }
}
async fn wire(socket: &mut Socket, v: Value) {
    socket
        .send(Message::Text(v.to_string().into()))
        .await
        .unwrap();
}
async fn native_start(socket: &mut Socket, id: &str) -> Value {
    wire(
        socket,
        json!({"type":"action","action":"tiny-start","gameId":id}),
    )
    .await;
    snapshot_until(socket, |s| s["tinyGame"]["gameId"] == id).await["tinyGame"].clone()
}
async fn native_intent(socket: &mut Socket, run: &Value, input: Value) {
    wire(
        socket,
        json!({"type":"tiny-input","sessionId":run,"input":input}),
    )
    .await;
}
async fn buttons(socket: &mut Socket, run: &Value, held: u16) {
    native_intent(socket, run, json!({"kind":"native_buttons","held":held})).await;
}
fn native_clock(server: &LiveServer, ticks: usize) {
    let mut g = server.state.game.lock().unwrap();
    for _ in 0..ticks {
        g.tick(0.05);
    }
}
fn private_payload(variable: &str) -> std::path::PathBuf {
    std::env::var_os(variable)
        .map(Into::into)
        .unwrap_or_else(|| panic!("Set {variable} to the matching private file to run this test"))
}
#[test]
fn native_sources_fail_closed_on_wrong_bytes_and_report_adapted_when_absent() {
    let (dir, state) = fixture(false);
    let sources = &state.game.lock().unwrap().native_tiny;
    assert_eq!(sources.rules(), "adapted");
    for row in sources.capabilities().as_object().unwrap().values() {
        assert_eq!(row["rulesVersion"], "adapted-v1");
        assert_eq!(row["native"], false);
    }
    let path = dir.path().join("bad");
    let mut cfg = Config::local(dir.path().to_owned(), false);
    cfg.tiny_a3ve_payload = Some(path.clone());
    assert!(AppState::load(cfg.clone()).is_err());
    fs::write(&path, vec![0; 188_716]).unwrap();
    assert!(AppState::load(cfg.clone()).is_err());
    cfg.tiny_a3ve_payload = None;
    cfg.tiny_asoe_payload = Some(path.clone());
    fs::write(&path, vec![0; 138752]).unwrap();
    assert!(AppState::load(cfg.clone()).is_err());
    cfg.tiny_asoe_payload = None;
    cfg.tiny_garden_payload = Some(path);
    assert!(AppState::load(cfg).is_err());
}

#[tokio::test]
#[ignore = "requires private verified A2NE and A3VE payloads"]
async fn native_button_socket_and_memory_wallet_survive_write_failure_retry_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_owned(), true);
    cfg.tiny_garden_payload = Some(private_payload("CHAO_TINY_GARDEN_PAYLOAD"));
    cfg.tiny_a3ve_payload = Some(private_payload("CHAO_TINY_A3VE_PAYLOAD"));
    let server = LiveServer::start_state(dir, AppState::load(cfg.clone()).unwrap()).await;
    let status = json_body(
        request(
            router(server.state.clone()),
            "GET",
            "/api/status",
            Body::empty(),
            &[],
        )
        .await,
    )
    .await;
    assert_eq!(
        status["tinyGarden"]["nativeTinyGames"]["memory"]["rulesVersion"],
        "a2ne-memory-v1"
    );
    assert_eq!(
        status["tinyGarden"]["nativeTinyGames"]["ball"]["maxRingReward"],
        128
    );
    assert_eq!(
        status["tinyGarden"]["nativeTinyGames"]["search"]["native"],
        false
    );
    let owner = session(&server.state, json!({"name":"Native adapter fixture"})).await;
    let id = owner["player"]["id"].as_str().unwrap();
    let mut s = server.socket(&owner).await;
    let ball = native_start(&mut s, "ball").await;
    assert_eq!(ball["rulesVersion"], "a3ve-ball-v1");
    assert_eq!(ball["view"]["state"]["board"].as_array().unwrap().len(), 10);
    assert!(!ball.to_string().contains("feed"));
    let run = &ball["id"];
    for invalid in [
        json!({"kind":"native_buttons","held":256}),
        json!({"kind":"native_buttons","held":16,"rings":900}),
        json!({"kind":"ball_shoot","color":"red"}),
    ] {
        native_intent(&mut s, run, invalid).await;
        assert_eq!(
            save_result(&mut s).await["text"],
            "Invalid Tiny Garden input."
        );
    }
    native_clock(&server, 12);
    buttons(&mut s, run, 8).await;
    buttons(&mut s, run, 0).await;
    snapshot_until(&mut s, |s| {
        s["tinyGame"]["view"]["state"]["screen"] == "pause"
    })
    .await;
    buttons(&mut s, run, 128).await;
    buttons(&mut s, run, 0).await;
    snapshot_until(&mut s, |s| {
        s["tinyGame"]["view"]["state"]["quitSelected"] == true
    })
    .await;
    wire(
        &mut s,
        json!({"type":"action","action":"tiny-stop","sessionId":run}),
    )
    .await;
    snapshot_until(&mut s, |s| s["tinyGame"].is_null()).await;
    native_clock(&server, 5);
    let mem = native_start(&mut s, "memory").await;
    let run = &mem["id"];
    assert_eq!(mem["rulesVersion"], "a2ne-memory-v1");
    let mut known = [None; 30];
    {
        // Accelerate only the authority clock. Track faces/occupancy through the
        // same sanitized projection sent over the socket; never read the seed.
        let mut g = server.state.game.lock().unwrap();
        let mut previous = [false; 30];
        for _ in 0..2400 {
            let public = g.players[id].tiny_game.as_ref().unwrap().public();
            let v = &public["view"]["state"];
            let cells = v["cells"].as_array().unwrap();
            let occupied: [bool; 30] = std::array::from_fn(|i| cells[i]["occupied"] == true);
            let from = (0..30).find(|&i| previous[i] && !occupied[i]);
            let to = (0..30).find(|&i| !previous[i] && occupied[i]);
            if let (Some(from), Some(to)) = (from, to) {
                known[to] = known[from].take();
            }
            for (i, c) in cells.iter().enumerate() {
                if let Some(face) = c["face"].as_u64() {
                    known[i] = Some(face);
                }
            }
            if v["phase"] == "matching" {
                assert!(cells.iter().all(|c| c["face"].is_null()));
                break;
            }
            previous = occupied;
            g.tick(0.02);
        }
    }
    let mut snap = snapshot_until(&mut s, |s| {
        s["tinyGame"]["view"]["state"]["phase"] == "matching"
    })
    .await;
    let blocked = server._directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocked).unwrap();
    let mut base = 0;
    for face in 0..7 {
        let pair: Vec<_> = (0..30).filter(|&i| known[i] == Some(face)).collect();
        assert_eq!(pair.len(), 2, "missing public cards for face {face}");
        for (pick, cell) in pair.into_iter().enumerate() {
            let v = &snap["tinyGame"]["view"]["state"];
            base += v["cells"][cell]["ringValue"].as_u64().unwrap();
            let mut x = v["cursor"][0].as_u64().unwrap() as usize;
            let mut y = v["cursor"][1].as_u64().unwrap() as usize;
            let (tx, ty) = (cell % 6, cell / 6);
            // Shortest wrapped path: at most three direction presses plus A; the
            // browser-equivalent FIFO never contains more than eight transitions.
            while (x, y) != (tx, ty) {
                let mut mask = 0;
                if x != tx {
                    if (tx + 6 - x) % 6 <= 3 {
                        x = (x + 1) % 6;
                        mask |= 16;
                    } else {
                        x = (x + 5) % 6;
                        mask |= 32;
                    }
                }
                if y != ty {
                    if (ty + 5 - y) % 5 <= 2 {
                        y = (y + 1) % 5;
                        mask |= 128;
                    } else {
                        y = (y + 4) % 5;
                        mask |= 64;
                    }
                }
                buttons(&mut s, run, mask).await;
                buttons(&mut s, run, 0).await;
            }
            buttons(&mut s, run, 1).await;
            buttons(&mut s, run, 0).await;
            snap = snapshot_until(&mut s, |s| {
                let sel = &s["tinyGame"]["view"]["state"]["selected"][pick];
                sel["column"] == tx && sel["row"] == ty
            })
            .await;
            if pick == 1 {
                native_clock(&server, 60);
                snap = snapshot_until(&mut s, |s| {
                    let v = &s["tinyGame"]["view"]["state"];
                    v["matchedPairs"] == face + 1
                        && (v["phase"] != "matching" || v["selected"][0].is_null())
                })
                .await;
            }
        }
    }
    native_clock(&server, 250);
    let pending = snapshot_until(&mut s, |s| s["tinyGame"]["phase"] == "saving").await;
    let earned = (base + 60).min(99);
    assert!(earned > 0);
    assert_eq!(pending["tinyGame"]["result"]["earnedRings"], earned);
    assert_eq!(pending["tinyGame"]["result"]["saved"], false);
    assert_eq!(player(&pending, id)["rings"], 20);
    assert!(player(&pending, id)["tinyRecords"]["memory"].is_null());
    let profile_path = server._directory.path().join("profiles.json");
    let before: Value = serde_json::from_slice(&fs::read(&profile_path).unwrap()).unwrap();
    assert_eq!(before[0]["rings"], 20);
    assert!(before[0]["tinyRecords"]["memory"].is_null());
    fs::remove_dir(blocked).unwrap();
    wire(&mut s, json!({"type":"action","action":"save"})).await;
    let credited = snapshot_until(&mut s, |s| s["tinyGame"]["phase"] == "finished").await;
    assert_eq!(credited["tinyGame"]["result"]["saved"], true);
    assert_eq!(credited["tinyGame"]["result"]["id"], *run);
    assert_eq!(credited["tinyGame"]["result"]["completed"], true);
    buttons(&mut s, run, 1).await;
    native_clock(&server, 10);
    wire(&mut s, json!({"type":"action","action":"save"})).await;
    assert_eq!(save_result(&mut s).await["kind"], "saved");
    s.close(None).await.unwrap();
    let reload = AppState::load(cfg).unwrap();
    let g = reload.game.lock().unwrap();
    let p = g
        .player_for_token(owner["token"].as_str().unwrap())
        .unwrap();
    assert_eq!(p.id, id);
    assert!(p.admin);
    assert_eq!(p.rings, 20 + earned as u32);
    assert_eq!(
        p.tiny_records["memory"].last_result_id.as_deref(),
        run.as_str()
    );
    assert_eq!(p.tiny_records["memory"].rings_earned, earned as u32);
    assert_eq!(p.tiny_records["memory"].plays, 1);
    assert!(!p.tiny_records.contains_key("ball"));
}

#[tokio::test]
#[ignore = "requires private verified ASOE payload"]
async fn native_asoe_socket_owns_lane_clock_cards_and_finalizes_only_after_finish() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_owned(), false);
    cfg.tiny_asoe_payload = Some(private_payload("CHAO_TINY_ASOE_PAYLOAD"));
    let server = LiveServer::start_state(dir, AppState::load(cfg.clone()).unwrap()).await;
    let owner = session(&server.state, json!({"name":"ASOE fixture"})).await;
    let mut s = server.socket(&owner).await;
    let run = native_start(&mut s, "rock_paper_scissors").await;
    let id = &run["id"];
    assert_eq!(run["rulesVersion"], "asoe-rps-v1");
    let view = &run["view"]["state"];
    assert_eq!(view["shots"], 5);
    assert_eq!(view["lane"], 1);
    for (lane, card) in view["cards"].as_array().unwrap().iter().enumerate() {
        assert_eq!(card["lane"], lane);
        assert_eq!(card["x"], 12 + 64 * lane);
        assert_eq!(card["y"], 116);
        assert!(card["gesture"].as_u64().unwrap() <= 2);
    }
    assert!(view["targets"].as_array().unwrap().len() < 10);
    assert!(view.get("rng").is_none());
    wire(&mut s,json!({"type":"tiny-input","sessionId":id,"input":{"kind":"native_buttons","held":32},"dt":5000})).await;
    assert_eq!(
        save_result(&mut s).await["text"],
        "Invalid Tiny Garden input."
    );
    for invalid in [
        json!({"kind":"rps_select","slot":0}),
        json!({"kind":"native_buttons","held":8}),
        json!({"kind":"native_buttons","held":32,"gesture":0}),
    ] {
        native_intent(&mut s, id, invalid).await;
        assert_eq!(save_result(&mut s).await["kind"], "error");
    }
    buttons(&mut s, id, 32).await;
    snapshot_until(&mut s, |s| s["tinyGame"]["view"]["state"]["lane"] == 0).await;
    // A second identical mask after the stale-input deadline must create a new edge.
    buttons(&mut s, id, 16).await;
    snapshot_until(&mut s, |s| s["tinyGame"]["view"]["state"]["lane"] == 1).await;
    native_clock(&server, 12);
    buttons(&mut s, id, 16).await;
    snapshot_until(&mut s, |s| s["tinyGame"]["view"]["state"]["lane"] == 2).await;
    buttons(&mut s, id, 0).await;
    native_clock(&server, 604);
    let finishing = snapshot_until(&mut s, |s| {
        s["tinyGame"]["view"]["state"]["screen"] == "result"
    })
    .await;
    assert_eq!(finishing["tinyGame"]["phase"], "playing");
    assert!(finishing["tinyGame"]["result"].is_null());
    native_clock(&server, 100);
    let finished = snapshot_until(&mut s, |s| s["tinyGame"]["phase"] == "finished").await;
    assert_eq!(
        finished["tinyGame"]["result"]["rulesVersion"],
        "asoe-rps-v1"
    );
    assert_eq!(finished["tinyGame"]["result"]["reason"], "time_up");
    assert_eq!(finished["tinyGame"]["result"]["rings"], 0);
    assert_eq!(finished["tinyGame"]["result"]["saved"], true);
    s.close(None).await.unwrap();
    let reload = AppState::load(cfg).unwrap();
    let game = reload.game.lock().unwrap();
    let p = game
        .player_for_token(owner["token"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        p.tiny_records["rock_paper_scissors"]
            .last_result_id
            .as_deref(),
        id.as_str()
    );
    assert_eq!(p.tiny_records["rock_paper_scissors"].plays, 1);
}

#[tokio::test]
#[ignore = "requires private A2NE payload"]
async fn native_bounce_spring_and_quit_persist_over_socket() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::local(dir.path().to_owned(), true);
    cfg.tiny_garden_payload = Some(private_payload("CHAO_TINY_GARDEN_PAYLOAD"));
    let server = LiveServer::start_state(dir, AppState::load(cfg.clone()).unwrap()).await;
    let owner = session(&server.state, json!({"name":"Bounce fixture"})).await;
    let id = owner["player"]["id"].as_str().unwrap();
    let mut s = server.socket(&owner).await;
    let run = native_start(&mut s, "bounce").await;
    assert_eq!(run["rulesVersion"], "a2ne-bounce-v1");
    let rid = &run["id"];
    native_intent(&mut s, rid, json!({"kind":"native_buttons","held":256})).await;
    assert_eq!(
        save_result(&mut s).await["text"],
        "Invalid Tiny Garden input."
    );
    native_clock(&server, 75);
    let v = snapshot_until(&mut s, |s| {
        s["tinyGame"]["view"]["state"]["springParts"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    })
    .await;
    let x = v["tinyGame"]["view"]["state"]["springX"].clone();
    buttons(&mut s, rid, 16).await;
    snapshot_until(&mut s, |s| s["tinyGame"]["view"]["state"]["springX"] != x).await;
    buttons(&mut s, rid, 8).await;
    buttons(&mut s, rid, 0).await;
    snapshot_until(&mut s, |s| {
        s["tinyGame"]["view"]["state"]["screen"] == "pause"
    })
    .await;
    buttons(&mut s, rid, 128).await;
    buttons(&mut s, rid, 0).await;
    snapshot_until(&mut s, |s| {
        s["tinyGame"]["view"]["state"]["quitSelected"] == true
    })
    .await;
    native_clock(&server, 4);
    buttons(&mut s, rid, 1).await;
    buttons(&mut s, rid, 0).await;
    snapshot_until(&mut s, |s| s["tinyGame"]["phase"] == "finished").await;
    let reloaded = AppState::load(cfg).unwrap();
    let g = reloaded.game.lock().unwrap();
    assert_eq!(g.players[id].tiny_records["bounce"].plays, 1);
}
