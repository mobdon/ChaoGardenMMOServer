//! Account boundary regressions use synthetic saves and isolated temporary stores.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use chao_game_server::{
    game::hash_token,
    service::{AppState, Config, router, spawn_simulation},
};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{fs, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::timeout,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tower::ServiceExt;

fn legacy_profile(token: &str, admin: bool) -> Value {
    let stats = [
        "swim",
        "fly",
        "run",
        "power",
        "stamina",
        "luck",
        "intelligence",
    ]
    .into_iter()
    .map(|name| (name.to_owned(), json!({"grade":3,"level":27,"points":921})))
    .collect::<serde_json::Map<_, _>>();
    json!({
        "id": if admin {"legacy-owner"} else {"legacy-earned-chao"},
        "tokenHash":hash_token(token), "name":"Earned Chao", "admin":admin,
        "stats":stats, "rings":4387, "alignment":-80.0, "form":"chaos",
        "happiness":87.0, "ownedCosmetics":["eggshell"], "equipped":"eggshell",
        "vehicle": if admin {json!("dark-chao-walker")} else {Value::Null},
        "gear": if admin {json!({"footwear":"shadow-air-shoes","hands":"knuckles-hammer-gloves"})}
            else {json!({"footwear":null,"hands":null})},
        "missionsCompleted":17,"racesCompleted":11,"bestRaceMs":18421.0,
        "raceRecords":{"fixture-course":{"completions":8,"bestMs":18421.0}},
        "karateRecords":{"beginner":{"completions":3,"bestMs":28750.0}},
        "tinyRecords":{"memory":{"plays":19,"completions":15,"ringsEarned":1121,
            "lastResultId":"0a34d267-33f8-427a-bd96-348b4042fed1"}},
        "resident":null
    })
}

async fn call(
    app: Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = app
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

fn identity(value: &Value) -> &Value {
    &value["identity"]
}

async fn account(state: &AppState, token: &str) -> Value {
    let (status, account) = call(
        router(state.clone()),
        "GET",
        "/api/account",
        Some(token),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{account}");
    account
}

async fn new_session(state: &AppState) -> Value {
    let (status, session) = call(
        router(state.clone()),
        "POST",
        "/api/session",
        None,
        json!({"name":"Paired Chao"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{session}");
    session
}

async fn pairing(state: &AppState, token: &str) -> Value {
    let current = account(state, token).await;
    let (status, pairing) = call(
        router(state.clone()),
        "POST",
        "/api/pairing",
        Some(token),
        json!({"expectedRevision":identity(&current)["revision"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pairing}");
    pairing
}

async fn claim(state: &AppState, invitation: &Value) -> (StatusCode, Value) {
    call(
        router(state.clone()),
        "POST",
        "/api/pairing/claim",
        None,
        json!({"pairingId":invitation["pairingId"],"pairingSecret":invitation["pairingSecret"]}),
    )
    .await
}

async fn lease_operation(
    state: &AppState,
    token: &str,
    operation: &str,
    activity: &str,
    lease_id: &str,
    revision: &Value,
) -> (StatusCode, Value) {
    call(router(state.clone()), "POST", "/api/lease", Some(token),
        json!({"operation":operation,"activity":activity,"leaseId":lease_id,"expectedRevision":revision})).await
}

async fn acquire(state: &AppState, token: &str, activity: &str) -> Value {
    let current = account(state, token).await;
    let (status, result) = lease_operation(
        state,
        token,
        "acquire",
        activity,
        "",
        &identity(&current)["revision"],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    account(state, token).await
}

async fn intent(
    state: &AppState,
    token: &str,
    lease_id: &Value,
    revision: &Value,
    sequence: u64,
    message: Value,
) -> (StatusCode, Value) {
    call(router(state.clone()), "POST", "/api/intent", Some(token),
        json!({"leaseId":lease_id,"expectedRevision":revision,"sequence":sequence,"message":message})).await
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct LiveAccounts {
    _directory: tempfile::TempDir,
    state: AppState,
    url: String,
    serving: JoinHandle<()>,
}

impl LiveAccounts {
    async fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let app = router(state.clone());
        let serving = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            _directory: directory,
            state,
            url,
            serving,
        }
    }
    async fn header_socket(&self, token: &str) -> Socket {
        let mut request = format!("{}/ws", self.url).into_client_request().unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        connect_async(request).await.unwrap().0
    }
}

impl Drop for LiveAccounts {
    fn drop(&mut self) {
        let _ = self.state.shutdown.send(true);
        self.serving.abort();
    }
}

async fn next_snapshot(socket: &mut Socket) -> Value {
    timeout(Duration::from_secs(3), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    if value["type"] == "snapshot" {
                        return value;
                    }
                }
                Some(Ok(Message::Ping(payload))) => {
                    socket.send(Message::Pong(payload)).await.unwrap();
                }
                Some(Err(error)) => panic!("socket failed before authenticated snapshot: {error}"),
                None | Some(Ok(Message::Close(_))) => {
                    panic!("socket closed before authenticated snapshot")
                }
                _ => {}
            }
        }
    })
    .await
    .expect("authenticated snapshot timeout")
}

async fn snapshot_matching(socket: &mut Socket, matches: impl Fn(&Value) -> bool) -> Value {
    timeout(Duration::from_secs(3), async {
        loop {
            let value = next_snapshot(socket).await;
            if matches(&value) {
                return value;
            }
        }
    })
    .await
    .expect("expected canonical activity change before timeout")
}

#[test]
fn migration_preserves_earned_progression_and_is_stable_across_restart() {
    let directory = tempfile::tempdir().unwrap();
    let token = "synthetic-legacy-desktop-credential";
    let original = legacy_profile(token, false);
    let original_bytes = serde_json::to_vec_pretty(&json!([original])).unwrap();
    let save = directory.path().join("profiles.json");
    fs::write(&save, &original_bytes).unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let migrated: Value = serde_json::from_slice(&fs::read(&save).unwrap()).unwrap();
    let identity = migrated[0]["identity"].clone();
    assert_eq!(identity["schemaVersion"], 2);
    assert_eq!(identity["chaoId"], original["id"]);
    assert!(
        identity["accountId"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    for key in [
        "id",
        "tokenHash",
        "name",
        "admin",
        "stats",
        "rings",
        "alignment",
        "form",
        "happiness",
        "ownedCosmetics",
        "equipped",
        "vehicle",
        "gear",
        "missionsCompleted",
        "racesCompleted",
        "bestRaceMs",
        "raceRecords",
        "karateRecords",
        "tinyRecords",
        "resident",
    ] {
        assert_eq!(
            migrated[0][key], original[key],
            "migration changed earned field {key}"
        );
    }
    assert!(!fs::read_to_string(&save).unwrap().contains(token));
    assert!(state.game.lock().unwrap().player_for_token(token).is_some());
    drop(state);
    let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    assert_eq!(restarted.game.lock().unwrap().players.len(), 1);
    let after: Value = serde_json::from_slice(&fs::read(&save).unwrap()).unwrap();
    assert_eq!(after[0]["identity"], identity);
    let backups = fs::read_dir(directory.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path() != save && entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| fs::read(entry.path()).ok())
        .filter(|bytes| bytes == &original_bytes)
        .count();
    assert_eq!(
        backups, 1,
        "migration must retain one exact recoverable original"
    );
}

#[test]
fn malformed_or_ambiguous_saves_fail_closed_without_replacing_the_original() {
    let regular = legacy_profile("credential-one", false);
    let mut duplicate_hash = legacy_profile("credential-one", true);
    duplicate_hash["id"] = json!("different-chao");
    let mut missing_hash = regular.clone();
    missing_hash.as_object_mut().unwrap().remove("tokenHash");
    let mut invalid_hash = regular.clone();
    invalid_hash["tokenHash"] = json!("not-a-sha256-hash");
    for profiles in [
        json!([regular.clone(), regular.clone()]),
        json!([regular.clone(), duplicate_hash]),
        json!([missing_hash]),
        json!([invalid_hash]),
        json!([null]),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let bytes = serde_json::to_vec(&profiles).unwrap();
        let save = directory.path().join("profiles.json");
        fs::write(&save, &bytes).unwrap();
        assert!(
            AppState::load(Config::local(directory.path().to_owned(), true)).is_err(),
            "ambiguous save must not start or grant bootstrap ownership: {profiles}"
        );
        assert_eq!(fs::read(save).unwrap(), bytes);
    }
}

#[test]
fn migration_failure_keeps_original_bytes_and_can_retry_after_storage_recovers() {
    let directory = tempfile::tempdir().unwrap();
    let profile = legacy_profile("synthetic-recovery-credential", false);
    let bytes = serde_json::to_vec_pretty(&json!([profile])).unwrap();
    let save = directory.path().join("profiles.json");
    fs::write(&save, &bytes).unwrap();
    let blocker = directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocker).unwrap();
    assert!(AppState::load(Config::local(directory.path().to_owned(), false)).is_err());
    assert_eq!(fs::read(&save).unwrap(), bytes);
    fs::remove_dir(blocker).unwrap();
    let recovered = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    assert_eq!(recovered.game.lock().unwrap().players.len(), 1);
    let stored: Value = serde_json::from_slice(&fs::read(save).unwrap()).unwrap();
    assert_eq!(stored[0]["identity"]["schemaVersion"], 2);
    assert_eq!(stored[0]["rings"], 4387);
}

#[tokio::test]
async fn profile_revision_commits_with_progression_and_does_not_advance_on_failed_or_noop_save() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let session = new_session(&state).await;
    let token = session["token"].as_str().unwrap();
    let id = session["player"]["id"].as_str().unwrap();
    let before = account(&state, token).await;
    let before_revision = identity(&before)["revision"].as_u64().unwrap();
    let stored_before = fs::read(directory.path().join("profiles.json")).unwrap();
    let blocker = directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocker).unwrap();
    {
        let mut game = state.game.lock().unwrap();
        game.players.get_mut(id).unwrap().rings = 137;
        game.dirty = true;
        assert!(state.persist(&mut game).is_err());
        assert_eq!(game.players[id].identity.revision, before_revision);
    }
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        stored_before
    );
    fs::remove_dir(blocker).unwrap();
    {
        let mut game = state.game.lock().unwrap();
        state.persist(&mut game).unwrap();
        let revision = game.players[id].identity.revision;
        assert!(revision > before_revision);
        game.dirty = true;
        state.persist(&mut game).unwrap();
        assert_eq!(game.players[id].identity.revision, revision);
    }
    let saved: Value =
        serde_json::from_slice(&fs::read(directory.path().join("profiles.json")).unwrap()).unwrap();
    let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let game = restarted.game.lock().unwrap();
    let restored = game.player_for_token(token).unwrap();
    assert_eq!(restored.id, id);
    assert_eq!(restored.rings, 137);
    assert_eq!(
        restored.identity.revision,
        saved[0]["identity"]["revision"].as_u64().unwrap()
    );
}

#[tokio::test]
async fn revoked_or_unknown_tokens_never_create_replacement_profiles() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let (status, session) = call(
        router(state.clone()),
        "POST",
        "/api/session",
        None,
        json!({"name":"Canonical"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let save = fs::read(directory.path().join("profiles.json")).unwrap();
    for token in [json!("unknown-credential"), json!(""), json!(42)] {
        let (status, _) = call(
            router(state.clone()),
            "POST",
            "/api/session",
            None,
            json!({"token":token}),
        )
        .await;
        assert!(status.is_client_error());
        assert_eq!(state.game.lock().unwrap().players.len(), 1);
        assert_eq!(
            fs::read(directory.path().join("profiles.json")).unwrap(),
            save
        );
    }
    let (status, resumed) = call(
        router(state),
        "POST",
        "/api/session",
        None,
        json!({"token":session["token"],"resumeOnly":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["player"]["id"], session["player"]["id"]);
}

#[tokio::test]
async fn pairing_is_single_use_and_android_cannot_mint_further_device_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let desktop = new_session(&state).await;
    let desktop_token = desktop["token"].as_str().unwrap();
    let invitation = pairing(&state, desktop_token).await;
    let secret = invitation["pairingSecret"].as_str().unwrap();
    let persisted = fs::read_to_string(directory.path().join("profiles.json")).unwrap();
    assert!(!persisted.contains(secret));
    let (status, android) = claim(&state, &invitation).await;
    assert_eq!(status, StatusCode::OK, "{android}");
    let android_token = android["token"].as_str().unwrap();
    let mobile_account = account(&state, android_token).await;
    let desktop_account = account(&state, desktop_token).await;
    assert_eq!(identity(&mobile_account)["chaoId"], desktop["player"]["id"]);
    assert_eq!(
        identity(&mobile_account)["accountId"],
        identity(&desktop_account)["accountId"]
    );
    assert_ne!(
        identity(&mobile_account)["deviceId"],
        identity(&desktop_account)["deviceId"]
    );
    assert_eq!(state.game.lock().unwrap().players.len(), 1);
    let before_replay = fs::read(directory.path().join("profiles.json")).unwrap();
    assert!(claim(&state, &invitation).await.0.is_client_error());
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        before_replay
    );
    let (status, _) = call(
        router(state.clone()),
        "POST",
        "/api/pairing",
        Some(android_token),
        json!({"expectedRevision":identity(&mobile_account)["revision"]}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let raw = fs::read_to_string(directory.path().join("profiles.json")).unwrap();
    assert!(!raw.contains(android_token));
    assert!(!raw.contains(desktop_token));
    assert!(!raw.contains(secret));
}

#[tokio::test]
async fn failed_pairing_claim_and_revocation_rollback_memory_credentials_and_revision() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let desktop = new_session(&state).await;
    let token = desktop["token"].as_str().unwrap();
    let invitation = pairing(&state, token).await;
    let before_claim = account(&state, token).await;
    let stored = fs::read(directory.path().join("profiles.json")).unwrap();
    let blocker = directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocker).unwrap();
    let (status, rejected) = claim(&state, &invitation).await;
    assert!(status.is_server_error(), "{rejected}");
    assert!(rejected.get("token").is_none());
    assert_eq!(
        identity(&account(&state, token).await),
        identity(&before_claim)
    );
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        stored
    );
    fs::remove_dir(&blocker).unwrap();
    let (status, android) = claim(&state, &invitation).await;
    assert_eq!(status, StatusCode::OK, "{android}");
    let mobile_token = android["token"].as_str().unwrap();
    let current = account(&state, token).await;
    let mobile = account(&state, mobile_token).await;
    let revoke = json!({"deviceId":identity(&mobile)["deviceId"],"expectedRevision":identity(&current)["revision"]});
    let before_revoke = fs::read(directory.path().join("profiles.json")).unwrap();
    fs::create_dir(&blocker).unwrap();
    let (status, _) = call(
        router(state.clone()),
        "POST",
        "/api/devices/revoke",
        Some(token),
        revoke.clone(),
    )
    .await;
    assert!(status.is_server_error());
    assert_eq!(identity(&account(&state, token).await), identity(&current));
    assert_eq!(
        identity(&account(&state, mobile_token).await),
        identity(&mobile)
    );
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        before_revoke
    );
    fs::remove_dir(&blocker).unwrap();
    let (status, result) = call(
        router(state.clone()),
        "POST",
        "/api/devices/revoke",
        Some(token),
        revoke,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let (status, _) = call(
        router(state.clone()),
        "GET",
        "/api/account",
        Some(mobile_token),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let (status, _) = call(
        router(restarted.clone()),
        "GET",
        "/api/account",
        Some(mobile_token),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        identity(&account(&restarted, token).await)["chaoId"],
        desktop["player"]["id"]
    );
}

#[tokio::test]
async fn browser_first_frame_and_native_header_authenticate_without_credentials_in_urls() {
    let server = LiveAccounts::start().await;
    let session = new_session(&server.state).await;
    let token = session["token"].as_str().unwrap();
    let mut native = server.header_socket(token).await;
    let native_snapshot = next_snapshot(&mut native).await;
    assert!(!native_snapshot.to_string().contains(token));
    let (mut browser, _) = connect_async(format!("{}/ws", server.url)).await.unwrap();
    browser
        .send(Message::Text(
            json!({"type":"authenticate","token":token})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let browser_snapshot = next_snapshot(&mut browser).await;
    assert!(!browser_snapshot.to_string().contains(token));
    assert_eq!(server.state.game.lock().unwrap().players.len(), 1);
    let query = connect_async(format!("{}/ws?token={token}", server.url)).await;
    assert!(
        query.is_err(),
        "credential URLs must be rejected even for valid credentials"
    );
    native.close(None).await.unwrap();
    browser.close(None).await.unwrap();
}

#[tokio::test]
async fn revocation_disconnects_existing_android_socket_and_reconnect_cannot_restore_access() {
    let server = LiveAccounts::start().await;
    let desktop = new_session(&server.state).await;
    let token = desktop["token"].as_str().unwrap();
    let invitation = pairing(&server.state, token).await;
    let (status, mobile) = claim(&server.state, &invitation).await;
    assert_eq!(status, StatusCode::OK);
    let mobile_token = mobile["token"].as_str().unwrap();
    let mut socket = server.header_socket(mobile_token).await;
    next_snapshot(&mut socket).await;
    let mobile_account = account(&server.state, mobile_token).await;
    let desktop_account = account(&server.state, token).await;
    let (status, result) = call(router(server.state.clone()), "POST", "/api/devices/revoke", Some(token),
        json!({"deviceId":identity(&mobile_account)["deviceId"],"expectedRevision":identity(&desktop_account)["revision"]})).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    timeout(Duration::from_secs(3), async {
        loop {
            match socket.next().await {
                None | Some(Ok(Message::Close(_))) | Some(Err(_)) => return,
                Some(Ok(Message::Ping(payload))) => {
                    let _ = socket.send(Message::Pong(payload)).await;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("revoked socket must close before another snapshot interval can continue indefinitely");
    let mut request = format!("{}/ws", server.url).into_client_request().unwrap();
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {mobile_token}").parse().unwrap(),
    );
    assert!(connect_async(request).await.is_err());
    assert_eq!(server.state.game.lock().unwrap().players.len(), 1);
}

#[tokio::test]
async fn concurrent_devices_cannot_both_acquire_or_use_one_activity_lease() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let desktop = new_session(&state).await;
    let desktop_token = desktop["token"].as_str().unwrap();
    let invitation = pairing(&state, desktop_token).await;
    let (status, mobile) = claim(&state, &invitation).await;
    assert_eq!(status, StatusCode::OK);
    let mobile_token = mobile["token"].as_str().unwrap();
    let current = account(&state, desktop_token).await;
    let revision = &identity(&current)["revision"];
    let (desktop_attempt, mobile_attempt) = tokio::join!(
        lease_operation(&state, desktop_token, "acquire", "watch", "", revision),
        lease_operation(&state, mobile_token, "acquire", "watch", "", revision)
    );
    assert_eq!(
        [desktop_attempt.0, mobile_attempt.0]
            .into_iter()
            .filter(|status| *status == StatusCode::OK)
            .count(),
        1
    );
    let loser = if desktop_attempt.0 == StatusCode::OK {
        mobile_token
    } else {
        desktop_token
    };
    let winning = account(&state, desktop_token).await;
    let before = fs::read(directory.path().join("profiles.json")).unwrap();
    let (status, _) = intent(
        &state,
        loser,
        &identity(&winning)["lease"]["id"],
        &identity(&winning)["revision"],
        1,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert!(status.is_client_error());
    assert!(
        state
            .game
            .lock()
            .unwrap()
            .players
            .values()
            .all(|player| player.watch.is_none())
    );
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        before
    );
    assert_eq!(state.game.lock().unwrap().players.len(), 1);
}

#[tokio::test]
async fn intents_require_current_revision_correct_activity_and_an_unreplayed_sequence() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let desktop = new_session(&state).await;
    let token = desktop["token"].as_str().unwrap();
    let id = desktop["player"]["id"].as_str().unwrap();
    let initial = account(&state, token).await;
    let direct = acquire(&state, token, "direct").await;
    let (status, _) = intent(
        &state,
        token,
        &identity(&direct)["lease"]["id"],
        &identity(&direct)["revision"],
        1,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert!(
        status.is_client_error(),
        "direct control must not enter a watch activity"
    );
    assert!(state.game.lock().unwrap().players[id].watch.is_none());
    let current = account(&state, token).await;
    let (status, result) = lease_operation(
        &state,
        token,
        "release",
        "direct",
        identity(&current)["lease"]["id"].as_str().unwrap(),
        &identity(&current)["revision"],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let watch = acquire(&state, token, "watch").await;
    let lease_id = &identity(&watch)["lease"]["id"];
    let (status, _) = intent(
        &state,
        token,
        lease_id,
        &identity(&initial)["revision"],
        1,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert!(
        status.is_client_error(),
        "stale revision cannot mutate a newer mode"
    );
    assert!(state.game.lock().unwrap().players[id].watch.is_none());
    let (status, result) = intent(
        &state,
        token,
        lease_id,
        &identity(&watch)["revision"],
        1,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert!(
        state.game.lock().unwrap().players[id].watch.is_some(),
        "an authenticated REST intent must work without a preexisting browser socket"
    );
    let after = account(&state, token).await;
    assert!(
        identity(&after)["revision"].as_u64().unwrap()
            > identity(&watch)["revision"].as_u64().unwrap()
    );
    let stored = fs::read(directory.path().join("profiles.json")).unwrap();
    let (status, _) = intent(
        &state,
        token,
        lease_id,
        &identity(&after)["revision"],
        1,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert!(
        status.is_client_error(),
        "fresh revision cannot make a consumed sequence reusable"
    );
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        stored
    );
    let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let returned = account(&restarted, token).await;
    let (status, _) = intent(
        &restarted,
        token,
        lease_id,
        &identity(&returned)["revision"],
        1,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert!(
        status.is_client_error(),
        "restart must retain the replay fence"
    );
    assert_eq!(restarted.game.lock().unwrap().players.len(), 1);
}

#[tokio::test]
async fn expired_lease_rejects_inputs_and_renewal_then_reacquisition_fences_old_requests() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let desktop = new_session(&state).await;
    let token = desktop["token"].as_str().unwrap();
    let id = desktop["player"]["id"].as_str().unwrap();
    let old = acquire(&state, token, "watch").await;
    // Synthetic trusted-storage fixture: no wall-clock sleep or client clock is involved.
    let save = directory.path().join("profiles.json");
    let mut profiles: Value = serde_json::from_slice(&fs::read(&save).unwrap()).unwrap();
    profiles[0]["identity"]["lease"]["expiresAt"] = json!(1);
    fs::write(&save, serde_json::to_vec(&profiles).unwrap()).unwrap();
    let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let current = account(&restarted, token).await;
    let (status, _) = lease_operation(
        &restarted,
        token,
        "renew",
        "watch",
        identity(&old)["lease"]["id"].as_str().unwrap(),
        &identity(&current)["revision"],
    )
    .await;
    assert!(status.is_client_error());
    let (status, _) = intent(
        &restarted,
        token,
        &identity(&old)["lease"]["id"],
        &identity(&current)["revision"],
        1,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert!(status.is_client_error());
    assert!(restarted.game.lock().unwrap().players[id].watch.is_none());
    let replacement = acquire(&restarted, token, "watch").await;
    assert_ne!(
        identity(&replacement)["lease"]["id"],
        identity(&old)["lease"]["id"]
    );
    let (status, _) = intent(
        &restarted,
        token,
        &identity(&old)["lease"]["id"],
        &identity(&replacement)["revision"],
        2,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert!(status.is_client_error());
    let (status, result) = intent(
        &restarted,
        token,
        &identity(&replacement)["lease"]["id"],
        &identity(&replacement)["revision"],
        1,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert!(restarted.game.lock().unwrap().players[id].watch.is_some());
}

#[tokio::test]
async fn failed_care_commit_does_not_consume_sequence_or_publish_effects_and_retry_pays_once() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let desktop = new_session(&state).await;
    let token = desktop["token"].as_str().unwrap();
    let id = desktop["player"]["id"].as_str().unwrap();
    let watch = acquire(&state, token, "watch").await;
    let lease_id = &identity(&watch)["lease"]["id"];
    let (status, result) = intent(
        &state,
        token,
        lease_id,
        &identity(&watch)["revision"],
        1,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let watch_id = {
        let mut game = state.game.lock().unwrap();
        game.time += 2000.;
        game.players[id].watch.as_ref().unwrap().id.clone()
    };
    let current = account(&state, token).await;
    let before_happiness = state.game.lock().unwrap().players[id].happiness;
    let stored = fs::read(directory.path().join("profiles.json")).unwrap();
    let blocker = directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocker).unwrap();
    let pet = json!({"type":"action","action":"watch-care","sessionId":watch_id,"kind":"pet"});
    let (status, _) = intent(
        &state,
        token,
        lease_id,
        &identity(&current)["revision"],
        2,
        pet.clone(),
    )
    .await;
    assert!(status.is_server_error());
    assert_eq!(identity(&account(&state, token).await), identity(&current));
    assert_eq!(
        state.game.lock().unwrap().players[id].happiness,
        before_happiness
    );
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        stored
    );
    fs::remove_dir(blocker).unwrap();
    let (status, result) = intent(
        &state,
        token,
        lease_id,
        &identity(&current)["revision"],
        2,
        pet.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(
        state.game.lock().unwrap().players[id].happiness,
        before_happiness + 2.
    );
    let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let current = account(&restarted, token).await;
    let (status, _) = intent(
        &restarted,
        token,
        lease_id,
        &identity(&current)["revision"],
        2,
        pet,
    )
    .await;
    assert!(status.is_client_error());
    assert_eq!(
        restarted.game.lock().unwrap().players[id].happiness,
        before_happiness + 2.
    );
}

#[tokio::test]
async fn competition_owner_can_move_in_an_authoritative_race_under_the_same_lease() {
    let server = LiveAccounts::start().await;
    let session = new_session(&server.state).await;
    let token = session["token"].as_str().unwrap();
    let id = session["player"]["id"].as_str().unwrap();
    let mut socket = server.header_socket(token).await;
    next_snapshot(&mut socket).await;
    let competition = acquire(&server.state, token, "competition").await;
    let lease_id = &identity(&competition)["lease"]["id"];
    let (status, result) = intent(
        &server.state,
        token,
        lease_id,
        &identity(&competition)["revision"],
        1,
        json!({"type":"action","action":"race"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert!(server.state.game.lock().unwrap().players[id].race.is_some());
    let current = account(&server.state, token).await;
    let (status, result) = intent(
        &server.state,
        token,
        lease_id,
        &identity(&current)["revision"],
        2,
        json!({"type":"input","dx":1,"dz":0}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "competition movement was rejected: {result}"
    );
    {
        let mut game = server.state.game.lock().unwrap();
        let before = game.players[id].x;
        game.tick(0.05);
        assert!(
            game.players[id].x > before,
            "server-owned race movement must still advance"
        );
    }
    socket.close(None).await.unwrap();
}

#[tokio::test]
async fn reconnect_response_reports_the_revision_committed_by_its_own_save() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let session = new_session(&state).await;
    let token = session["token"].as_str().unwrap();
    let id = session["player"]["id"].as_str().unwrap();
    {
        let mut game = state.game.lock().unwrap();
        game.players.get_mut(id).unwrap().rings = 211;
        game.dirty = true;
    }
    let (status, resumed) = call(
        router(state.clone()),
        "POST",
        "/api/session",
        None,
        json!({"token":token,"resumeOnly":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{resumed}");
    let current = account(&state, token).await;
    assert_eq!(
        identity(&resumed)["revision"],
        identity(&current)["revision"],
        "resume must return the revision after its persistence transaction"
    );
    assert_eq!(resumed["player"]["rings"], 211);
    let (status, result) = lease_operation(
        &state,
        token,
        "acquire",
        "direct",
        "",
        &identity(&resumed)["revision"],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "fresh reconnect revision was stale: {result}"
    );
}

#[tokio::test]
async fn android_snapshot_includes_its_canonical_chao_when_every_socket_is_offline() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let desktop = new_session(&state).await;
    let invitation = pairing(&state, desktop["token"].as_str().unwrap()).await;
    let (status, mobile) = claim(&state, &invitation).await;
    assert_eq!(status, StatusCode::OK);
    let account = account(&state, mobile["token"].as_str().unwrap()).await;
    let own = account["snapshot"]["players"]
        .as_array()
        .unwrap()
        .iter()
        .find(|player| player["id"] == desktop["player"]["id"])
        .expect("REST widget snapshot must contain its own offline canonical Chao");
    assert_eq!(own["connected"], false);
    assert_eq!(own["stats"], desktop["player"]["stats"]);
    assert_eq!(own["rings"], desktop["player"]["rings"]);
    assert_eq!(own["happiness"], desktop["player"]["happiness"]);
    assert!(own["energy"].is_number() && own["hunger"].is_number());
    let game = state.game.lock().unwrap();
    assert_eq!(game.online_connections(), 0);
    assert!(
        game.snapshot("unrelated-observer")["players"]
            .as_array()
            .unwrap()
            .is_empty(),
        "private offline self visibility must not create public residency"
    );
}

#[tokio::test]
async fn stale_widget_care_reports_no_effect_when_no_browser_is_connected() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let session = new_session(&state).await;
    let token = session["token"].as_str().unwrap();
    let id = session["player"]["id"].as_str().unwrap();
    let watch = acquire(&state, token, "watch").await;
    let lease_id = &identity(&watch)["lease"]["id"];
    let (status, result) = intent(
        &state,
        token,
        lease_id,
        &identity(&watch)["revision"],
        1,
        json!({"type":"action","action":"watch-start"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["accepted"], true);
    let before_happiness = {
        let mut game = state.game.lock().unwrap();
        game.time += 200.;
        game.players[id].happiness
    };
    let current = account(&state, token).await;
    let (status, result) = intent(&state, token, lease_id, &identity(&current)["revision"], 2,
        json!({"type":"action","action":"watch-care","sessionId":"stale-watch-session","kind":"pet"})).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["kind"], "watch-saved");
    assert_eq!(
        result["accepted"], false,
        "connectivity metadata is not an accepted care effect"
    );
    let game = state.game.lock().unwrap();
    assert_eq!(game.online_connections(), 0);
    assert_eq!(game.players[id].happiness, before_happiness);
}

#[tokio::test]
async fn restoring_latest_v2_backup_retains_identity_revocation_and_earned_receipts() {
    let source = tempfile::tempdir().unwrap();
    let desktop_token = "synthetic-migrated-backup-desktop";
    let original = legacy_profile(desktop_token, false);
    fs::write(
        source.path().join("profiles.json"),
        serde_json::to_vec_pretty(&json!([original])).unwrap(),
    )
    .unwrap();
    let state = AppState::load(Config::local(source.path().to_owned(), false)).unwrap();
    let invitation = pairing(&state, desktop_token).await;
    let (status, mobile) = claim(&state, &invitation).await;
    assert_eq!(status, StatusCode::OK);
    let mobile_token = mobile["token"].as_str().unwrap();
    let mobile_account = account(&state, mobile_token).await;
    let desktop_account = account(&state, desktop_token).await;
    let (status, result) = call(router(state.clone()), "POST", "/api/devices/revoke", Some(desktop_token),
        json!({"deviceId":identity(&mobile_account)["deviceId"],"expectedRevision":identity(&desktop_account)["revision"]})).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let expected = account(&state, desktop_token).await;
    let latest = fs::read(source.path().join("profiles.json")).unwrap();
    let latest_backup = source.path().join("latest-v2.backup.json");
    fs::write(&latest_backup, &latest).unwrap();
    drop(state);

    let recovery = tempfile::tempdir().unwrap();
    let recovered_save = recovery.path().join("profiles.json");
    fs::copy(&latest_backup, &recovered_save).unwrap();
    let restored = AppState::load(Config::local(recovery.path().to_owned(), false)).unwrap();
    assert_eq!(
        fs::read(&recovered_save).unwrap(),
        latest,
        "loading v2 backup must not migrate or regenerate identity"
    );
    assert!(!recovery.path().join("profiles.v1.backup.json").exists());
    let actual = account(&restored, desktop_token).await;
    assert_eq!(identity(&actual), identity(&expected));
    assert_eq!(identity(&actual)["chaoId"], original["id"]);
    assert_eq!(actual["snapshot"]["self"]["admin"], false);
    for key in [
        "rings",
        "stats",
        "raceRecords",
        "karateRecords",
        "tinyRecords",
        "missionsCompleted",
        "racesCompleted",
    ] {
        assert_eq!(
            actual["snapshot"]["self"][key], original[key],
            "recovery changed earned field {key}"
        );
    }
    let (status, _) = call(
        router(restored.clone()),
        "GET",
        "/api/account",
        Some(mobile_token),
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "restoring latest backup must retain revocation"
    );
    let (status, _) = call(
        router(restored.clone()),
        "POST",
        "/api/session",
        None,
        json!({"token":mobile_token,"resumeOnly":true}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(claim(&restored, &invitation).await.0.is_client_error());
    assert_eq!(restored.game.lock().unwrap().players.len(), 1);
    assert_eq!(fs::read(recovered_save).unwrap(), latest);
}

#[tokio::test]
async fn failed_initial_session_does_not_consume_owner_bootstrap_or_create_a_hidden_account() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), true)).unwrap();
    let blocker = directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocker).unwrap();
    let (status, rejected) = call(
        router(state.clone()),
        "POST",
        "/api/session",
        None,
        json!({"name":"Uncommitted first Chao"}),
    )
    .await;
    assert!(status.is_server_error(), "{rejected}");
    assert!(rejected.get("token").is_none() && rejected.get("identity").is_none());
    assert!(state.game.lock().unwrap().players.is_empty());
    assert!(!directory.path().join("profiles.json").exists());
    let (status, _) = call(
        router(state.clone()),
        "POST",
        "/api/session",
        None,
        json!({"token":"invalid-credential-from-a-local-save-slot"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(state.game.lock().unwrap().players.is_empty());
    fs::remove_dir(blocker).unwrap();

    let (status, owner) = call(
        router(state.clone()),
        "POST",
        "/api/session",
        None,
        json!({"name":"Committed first Chao"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{owner}");
    assert_eq!(owner["player"]["admin"], true);
    assert_eq!(owner["player"]["stats"]["run"]["level"], 99);
    assert_eq!(state.game.lock().unwrap().players.len(), 1);
    let saved = fs::read(directory.path().join("profiles.json")).unwrap();
    let (status, _) = call(
        router(state.clone()),
        "POST",
        "/api/session",
        None,
        json!({"token":"invalid-credential-from-a-local-save-slot"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(state.game.lock().unwrap().players.len(), 1);
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        saved
    );
    let restarted = AppState::load(Config::local(directory.path().to_owned(), true)).unwrap();
    let resumed = account(&restarted, owner["token"].as_str().unwrap()).await;
    assert_eq!(identity(&resumed), identity(&owner));
    assert_eq!(resumed["snapshot"]["self"]["admin"], true);
    assert_eq!(restarted.game.lock().unwrap().players.len(), 1);
}

#[tokio::test]
async fn passive_android_observer_does_not_keep_abandoned_desktop_tiny_round_alive() {
    let server = LiveAccounts::start().await;
    let desktop = new_session(&server.state).await;
    let token = desktop["token"].as_str().unwrap();
    let id = desktop["player"]["id"].as_str().unwrap();
    let invitation = pairing(&server.state, token).await;
    let (status, android) = claim(&server.state, &invitation).await;
    assert_eq!(status, StatusCode::OK);
    let mut controller = server.header_socket(token).await;
    let mut observer = server
        .header_socket(android["token"].as_str().unwrap())
        .await;
    next_snapshot(&mut controller).await;
    next_snapshot(&mut observer).await;
    let tiny = acquire(&server.state, token, "tiny").await;
    let (status, result) = intent(
        &server.state,
        token,
        &identity(&tiny)["lease"]["id"],
        &identity(&tiny)["revision"],
        1,
        json!({"type":"action","action":"tiny-start","gameId":"memory"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    snapshot_matching(&mut observer, |snapshot| {
        snapshot["tinyGame"]["gameId"] == "memory"
    })
    .await;
    let rings = server.state.game.lock().unwrap().players[id].rings;
    controller.close(None).await.unwrap();
    let stopped = snapshot_matching(&mut observer, |snapshot| snapshot["tinyGame"].is_null()).await;
    assert_eq!(stopped["self"]["rings"], rings);
    {
        let game = server.state.game.lock().unwrap();
        assert_eq!(game.players.len(), 1);
        assert_eq!(
            game.players[id].connections, 1,
            "observer must remain connected"
        );
        assert!(game.players[id].tiny_game.is_none());
        assert_eq!(game.players[id].rings, rings);
        assert!(game.players[id].tiny_records.is_empty());
    }
    observer.close(None).await.unwrap();
}

#[tokio::test]
async fn revoking_android_controller_cancels_unfinished_tiny_while_desktop_keeps_observing() {
    let server = LiveAccounts::start().await;
    let desktop = new_session(&server.state).await;
    let desktop_token = desktop["token"].as_str().unwrap();
    let id = desktop["player"]["id"].as_str().unwrap();
    let invitation = pairing(&server.state, desktop_token).await;
    let (status, android) = claim(&server.state, &invitation).await;
    assert_eq!(status, StatusCode::OK);
    let android_token = android["token"].as_str().unwrap();
    let mut controller = server.header_socket(android_token).await;
    let mut observer = server.header_socket(desktop_token).await;
    next_snapshot(&mut controller).await;
    next_snapshot(&mut observer).await;
    let tiny = acquire(&server.state, android_token, "tiny").await;
    let (status, result) = intent(
        &server.state,
        android_token,
        &identity(&tiny)["lease"]["id"],
        &identity(&tiny)["revision"],
        1,
        json!({"type":"action","action":"tiny-start","gameId":"memory"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    snapshot_matching(&mut observer, |snapshot| {
        snapshot["tinyGame"]["gameId"] == "memory"
    })
    .await;
    let current = account(&server.state, desktop_token).await;
    let (status, result) = call(router(server.state.clone()), "POST", "/api/devices/revoke", Some(desktop_token),
        json!({"deviceId":identity(&tiny)["deviceId"],"expectedRevision":identity(&current)["revision"]})).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let stopped = snapshot_matching(&mut observer, |snapshot| snapshot["tinyGame"].is_null()).await;
    assert_eq!(stopped["self"]["rings"], 20);
    timeout(Duration::from_secs(3), async {
        loop {
            match controller.next().await {
                None | Some(Ok(Message::Close(_))) | Some(Err(_)) => break,
                _ => {}
            }
        }
    })
    .await
    .expect("revoked controller must disconnect");
    {
        let game = server.state.game.lock().unwrap();
        assert_eq!(
            game.players[id].connections, 1,
            "passive desktop must remain connected"
        );
        assert!(game.players[id].tiny_game.is_none());
        assert_eq!(game.players[id].rings, 20);
        assert!(game.players[id].tiny_records.is_empty());
    }
    observer.close(None).await.unwrap();
}

#[tokio::test]
async fn stale_loaded_authority_cannot_overwrite_a_newer_profile_commit() {
    let directory = tempfile::tempdir().unwrap();
    let writer = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let original = new_session(&writer).await;
    let id = original["player"]["id"].as_str().unwrap();
    let stale = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    {
        let mut game = writer.game.lock().unwrap();
        game.players.get_mut(id).unwrap().rings = 143;
        game.dirty = true;
        writer.persist(&mut game).unwrap();
    }
    let latest = fs::read(directory.path().join("profiles.json")).unwrap();
    let (status, rejected) = call(
        router(stale.clone()),
        "POST",
        "/api/session",
        None,
        json!({"name":"Stale writer must not create this account"}),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{rejected}");
    assert!(rejected.get("token").is_none());
    assert_eq!(
        stale.game.lock().unwrap().players.len(),
        1,
        "failed stale write must roll back account creation"
    );
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        latest
    );
    let refreshed = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let canonical = account(&refreshed, original["token"].as_str().unwrap()).await;
    assert_eq!(canonical["snapshot"]["self"]["rings"], 143);
    assert_eq!(
        identity(&canonical)["accountId"],
        identity(&original)["accountId"]
    );
    assert_eq!(refreshed.game.lock().unwrap().players.len(), 1);
    let added = new_session(&refreshed).await;
    assert_ne!(added["player"]["id"], original["player"]["id"]);
    assert_eq!(refreshed.game.lock().unwrap().players[id].rings, 143);
}

#[tokio::test]
async fn externally_held_writer_lock_prevents_partial_creation_and_recovers_after_release() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let original = new_session(&state).await;
    let latest = fs::read(directory.path().join("profiles.json")).unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.path().join("profiles.lock"))
        .unwrap();
    held.try_lock().unwrap();
    let (status, rejected) = call(
        router(state.clone()),
        "POST",
        "/api/session",
        None,
        json!({"name":"Blocked account"}),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{rejected}");
    assert!(rejected.get("token").is_none());
    assert_eq!(state.game.lock().unwrap().players.len(), 1);
    assert_eq!(
        fs::read(directory.path().join("profiles.json")).unwrap(),
        latest
    );
    drop(held);
    let added = new_session(&state).await;
    assert_ne!(added["player"]["id"], original["player"]["id"]);
    assert_eq!(state.game.lock().unwrap().players.len(), 2);
    let restarted = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    assert_eq!(restarted.game.lock().unwrap().players.len(), 2);
    assert!(
        restarted
            .game
            .lock()
            .unwrap()
            .player_for_token(added["token"].as_str().unwrap())
            .is_some()
    );
}

#[tokio::test]
async fn observer_keeps_pending_terminal_receipt_after_controller_disconnect_until_one_commit() {
    let server = LiveAccounts::start().await;
    let desktop = new_session(&server.state).await;
    let token = desktop["token"].as_str().unwrap();
    let id = desktop["player"]["id"].as_str().unwrap();
    let invitation = pairing(&server.state, token).await;
    let (status, android) = claim(&server.state, &invitation).await;
    assert_eq!(status, StatusCode::OK);
    let android_token = android["token"].as_str().unwrap();
    let mut controller = server.header_socket(token).await;
    let mut observer = server.header_socket(android_token).await;
    next_snapshot(&mut controller).await;
    next_snapshot(&mut observer).await;
    let tiny = acquire(&server.state, token, "tiny").await;
    let (status, result) = intent(
        &server.state,
        token,
        &identity(&tiny)["lease"]["id"],
        &identity(&tiny)["revision"],
        1,
        json!({"type":"action","action":"tiny-start","gameId":"rock_paper_scissors"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    snapshot_matching(&mut observer, |snapshot| {
        snapshot["tinyGame"]["phase"] == "playing"
    })
    .await;
    let blocker = server._directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocker).unwrap();
    let run_id = {
        let mut game = server.state.game.lock().unwrap();
        for _ in 0..300 {
            game.tick(0.1);
        }
        assert!(game.players[id].tiny_game.as_ref().unwrap().saving());
        assert!(server.state.persist(&mut game).is_err());
        game.players[id].tiny_game.as_ref().unwrap().id.clone()
    };
    controller.close(None).await.unwrap();
    timeout(Duration::from_secs(3), async {
        loop {
            if server.state.game.lock().unwrap().players[id].connections == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("controller disconnect cleanup must finish");
    let pending = snapshot_matching(&mut observer, |snapshot| {
        snapshot["tinyGame"]["phase"] == "saving"
    })
    .await;
    assert_eq!(pending["tinyGame"]["id"], run_id);
    assert_eq!(pending["tinyGame"]["result"]["saved"], false);
    assert_eq!(pending["self"]["rings"], 20);
    assert!(
        pending["self"]["tinyRecords"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    fs::remove_dir(blocker).unwrap();
    let save = json!({"message":{"type":"action","action":"save"}});
    let (status, saved) = call(
        router(server.state.clone()),
        "POST",
        "/api/intent",
        Some(android_token),
        save.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["kind"], "saved");
    {
        let game = server.state.game.lock().unwrap();
        assert_eq!(
            game.players[id].tiny_records["rock_paper_scissors"].plays,
            1
        );
        assert_eq!(
            game.players[id].tiny_records["rock_paper_scissors"]
                .last_result_id
                .as_deref(),
            Some(run_id.as_str())
        );
        assert_eq!(game.players[id].rings, 20);
    }
    let committed = fs::read(server._directory.path().join("profiles.json")).unwrap();
    let (status, _) = call(
        router(server.state.clone()),
        "POST",
        "/api/intent",
        Some(android_token),
        save,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        fs::read(server._directory.path().join("profiles.json")).unwrap(),
        committed
    );
    let restarted =
        AppState::load(Config::local(server._directory.path().to_owned(), false)).unwrap();
    assert_eq!(
        restarted.game.lock().unwrap().players[id].tiny_records["rock_paper_scissors"].plays,
        1
    );
    observer.close(None).await.unwrap();
}

#[test]
fn release_migration_never_bypasses_atomic_save_for_an_old_marker_and_preserves_v2_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let token = "synthetic-release-migration-desktop";
    let mut original = legacy_profile(token, false);
    original["futureMetadata"] = json!({"inventoryEpoch":7,"notes":["preserve this opaque data"]});
    original["stats"]["run"]["provenance"] = json!({"origin":"earned"});
    original["tinyRecords"]["memory"]["audit"] = json!({"source":"trusted-receipt"});
    let save = directory.path().join("profiles.json");
    fs::write(
        &save,
        serde_json::to_vec_pretty(&json!([original])).unwrap(),
    )
    .unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let before = fs::read(&save).unwrap();
    let expected: Value = serde_json::from_slice(&before).unwrap();
    let expected_revision = expected[0]["identity"]["revision"].as_u64().unwrap();
    fs::write(directory.path().join(".schema-version"), b"1").unwrap();
    let blocker = directory.path().join("profiles.json.tmp");
    fs::create_dir(&blocker).unwrap();
    assert!(
        state.release_migration().is_err(),
        "an old marker cannot skip the common atomic writer"
    );
    assert_eq!(fs::read(&save).unwrap(), before);
    {
        let game = state.game.lock().unwrap();
        let player = game.player_for_token(token).unwrap();
        assert_eq!(player.identity.revision, expected_revision);
        assert_eq!(player.tiny_records["memory"].plays, 19);
        assert_eq!(game.players.len(), 1);
    }
    fs::remove_dir(blocker).unwrap();
    for _ in 0..2 {
        state.release_migration().unwrap();
        assert_eq!(
            fs::read(&save).unwrap(),
            before,
            "repeat release migration must preserve the exact v2 document"
        );
    }
    let recovered = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let profiles = recovered.game.lock().unwrap().export_profiles();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0]["identity"], expected[0]["identity"]);
    assert_eq!(profiles[0]["futureMetadata"], original["futureMetadata"]);
    assert_eq!(
        profiles[0]["stats"]["run"]["provenance"],
        original["stats"]["run"]["provenance"]
    );
    assert_eq!(
        profiles[0]["tinyRecords"]["memory"],
        original["tinyRecords"]["memory"]
    );
}

#[tokio::test]
async fn expired_lease_stops_held_input_before_the_next_real_simulation_tick() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::load(Config::local(directory.path().to_owned(), false)).unwrap();
    let session = new_session(&state).await;
    let token = session["token"].as_str().unwrap();
    let id = session["player"]["id"].as_str().unwrap();
    acquire(&state, token, "direct").await;
    let (initial_x, initial_time) = {
        let mut game = state.game.lock().unwrap();
        game.connect(id);
        game.players
            .get_mut(id)
            .unwrap()
            .identity
            .lease
            .as_mut()
            .unwrap()
            .expires_at = 1;
        game.set_input(id, &json!({"dx":1,"dz":0}));
        (game.players[id].x, game.time)
    };
    let simulation = spawn_simulation(state.clone());
    timeout(Duration::from_secs(2), async {
        loop {
            if state.game.lock().unwrap().time >= initial_time + 100. {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("simulation must process trusted ticks");
    {
        let game = state.game.lock().unwrap();
        assert_eq!(
            game.players[id].x, initial_x,
            "expired control cannot move during the stale-input grace period"
        );
        assert_eq!(game.players[id].input.dx, 0.);
        assert_eq!(game.players[id].input.dz, 0.);
    }
    state.shutdown.send(true).unwrap();
    timeout(Duration::from_secs(2), simulation)
        .await
        .unwrap()
        .unwrap();
}
