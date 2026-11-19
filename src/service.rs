use crate::game::{Event, Game, TERRAIN_ZONES, Terrain};
use crate::metrics::Metrics;
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{
        DefaultBodyLimit, OriginalUri, Query, Request, State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env, fs, io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{broadcast, watch},
    time::{Instant, MissedTickBehavior},
};

#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub address: SocketAddr,
    pub owner_bootstrap: bool,
    pub allowed_origins: Vec<String>,
}
impl Config {
    pub fn from_env() -> Result<Self, String> {
        let data_dir = env::var_os("CHAO_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("data"));
        let host: IpAddr = env::var("GAME_HOST")
            .unwrap_or_else(|_| "127.0.0.1".into())
            .parse()
            .map_err(|_| "Invalid GAME_HOST")?;
        let port = env::var("GAME_PORT")
            .unwrap_or_else(|_| "3001".into())
            .parse()
            .map_err(|_| "Invalid GAME_PORT")?;
        let owner_bootstrap =
            env::var("CHAO_OWNER_BOOTSTRAP").is_ok_and(|value| value == "1" || value == "true");
        if owner_bootstrap && !host.is_loopback() {
            return Err("Owner bootstrap is permitted only on a loopback listener".into());
        }
        let mut allowed_origins = vec![];
        for value in env::var("CHAO_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let parsed =
                url::Url::parse(value).map_err(|_| "Invalid CHAO_ALLOWED_ORIGINS entry")?;
            if !["http", "https"].contains(&parsed.scheme()) || parsed.host_str().is_none() {
                return Err("Origins must be HTTP(S) origins".into());
            }
            allowed_origins.push(parsed.origin().ascii_serialization());
        }
        Ok(Self {
            data_dir,
            address: SocketAddr::new(host, port),
            owner_bootstrap,
            allowed_origins,
        })
    }
    pub fn local(data_dir: PathBuf, owner_bootstrap: bool) -> Self {
        Self {
            data_dir,
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            owner_bootstrap,
            allowed_origins: vec![],
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub game: Arc<Mutex<Game>>,
    pub config: Arc<Config>,
    pub events: broadcast::Sender<Event>,
    pub shutdown: watch::Sender<bool>,
    pub metrics: Arc<Metrics>,
}
impl AppState {
    pub fn load(config: Config) -> io::Result<Self> {
        if config.owner_bootstrap && !config.address.ip().is_loopback() {
            return Err(io::Error::other("Owner bootstrap requires loopback"));
        }
        let profile_path = config.data_dir.join("profiles.json");
        let profiles: Vec<Value> = if profile_path.exists() {
            serde_json::from_slice(&fs::read(profile_path)?)?
        } else {
            vec![]
        };
        let mut terrains = BTreeMap::new();
        for zone in TERRAIN_ZONES {
            let path = config.data_dir.join("terrain").join(format!("{zone}.json"));
            if path.exists() {
                terrains.insert(
                    zone.to_owned(),
                    serde_json::from_slice::<Terrain>(&fs::read(path)?)?,
                );
            }
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
            * 1000.;
        let mut game =
            Game::new(profiles, terrains, config.owner_bootstrap, now).map_err(io::Error::other)?;
        let read_catalog = |name: &str| -> io::Result<Value> {
            let path = config
                .data_dir
                .join("activities")
                .join(format!("{name}.json"));
            if path.exists() {
                Ok(serde_json::from_slice(&fs::read(path)?)?)
            } else {
                Ok(json!({}))
            }
        };
        game.activities = crate::activities::ActivityCatalog::from_values(
            read_catalog("races")?,
            read_catalog("karate")?,
        )
        .map_err(io::Error::other)?;
        let worlds: std::collections::BTreeSet<_> = game
            .activities
            .courses
            .iter()
            .map(|c| c.world_id.as_deref().unwrap_or(&c.id).to_owned())
            .collect();
        for world in worlds {
            if !world
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            {
                return Err(io::Error::other("Invalid activity collision identifier"));
            }
            let path = config
                .data_dir
                .join("activities/terrain")
                .join(format!("{world}.json"));
            if path.exists() {
                let terrain: Terrain = serde_json::from_slice(&fs::read(path)?)?;
                terrain.validate().map_err(io::Error::other)?;
                game.activities.terrains.insert(world, terrain);
            }
        }
        let (events, _) = broadcast::channel(512);
        let (shutdown, _) = watch::channel(false);
        Ok(Self {
            game: Arc::new(Mutex::new(game)),
            config: Arc::new(config),
            events,
            shutdown,
            metrics: Metrics::new(),
        })
    }
    pub fn persist(&self, game: &mut Game) -> io::Result<()> {
        if !game.dirty {
            return Ok(());
        }
        fs::create_dir_all(&self.config.data_dir)?;
        let temporary = self.config.data_dir.join("profiles.json.tmp");
        let bytes = serde_json::to_vec_pretty(&game.export_profiles())?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        {
            use std::io::Write;
            let mut file = options.open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        fs::rename(temporary, self.config.data_dir.join("profiles.json"))?;
        game.dirty = false;
        crate::phone::commit_rewards(game);
        Ok(())
    }
    /// Controlled release migration, executed once per deploy before machines
    /// roll. Idempotent: re-running against migrated data is a no-op. The
    /// JSON stores are validated and re-serialised in canonical form so the
    /// new image never serves half-upgraded data, and a schema marker records
    /// the applied version for operators.
    pub fn release_migration(&self) -> io::Result<()> {
        const SCHEMA_VERSION: u32 = 1;
        fs::create_dir_all(&self.config.data_dir)?;
        let marker_path = self.config.data_dir.join(".schema-version");
        if fs::read_to_string(&marker_path).ok().as_deref() == Some("1\n") {
            return Ok(());
        }
        let profile_path = self.config.data_dir.join("profiles.json");
        if profile_path.exists() {
            // AppState::load already validated shape; rewrite canonically.
            let profiles: Vec<Value> = serde_json::from_slice(&fs::read(&profile_path)?)?;
            fs::write(&profile_path, serde_json::to_vec_pretty(&profiles)?)?;
        }
        fs::write(&marker_path, format!("{SCHEMA_VERSION}\n"))?;
        Ok(())
    }
    pub fn watch_action(&self, game: &mut Game, id: &str, message: &Value) -> io::Result<Value> {
        let before = game
            .players
            .get(id)
            .cloned()
            .ok_or_else(|| io::Error::other("Missing profile"))?;
        let was_dirty = game.dirty;
        let event_count = game.events.len();
        let before_watch = before
            .watch
            .as_ref()
            .map(|watch| watch.public(before.connections > 0));
        game.action(id, message);
        if let Err(error) = self.persist(game) {
            game.players.insert(id.to_owned(), before);
            game.dirty = was_dirty;
            game.events.truncate(event_count);
            return Err(error);
        }
        let player = &game.players[id];
        let watch = player
            .watch
            .as_ref()
            .map(|watch| watch.public(player.connections > 0));
        let accepted = before_watch != watch || before.happiness != player.happiness;
        Ok(
            json!({"type":"event","kind":"watch-saved","accepted":accepted,"watch":watch,"serverTime":game.time}),
        )
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/activities", get(activities))
        .route("/api/session", post(session))
        .route("/ws", get(upgrade))
        // Deployment probes and metrics: unauthenticated, cheap, excluded from
        // the request metrics themselves.
        .route("/livez", get(livez))
        .route("/readyz", get(readyz))
        .route("/metrics", get(prometheus))
        .fallback(asset_or_missing)
        .layer(DefaultBodyLimit::max(2048))
        .layer(middleware::from_fn_with_state(state.clone(), origin_guard))
        .with_state(state)
}

fn error(status: StatusCode, text: &str) -> Response {
    let mut response = (status, Json(json!({"error": text}))).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
fn origin_allowed(headers: &HeaderMap, config: &Config) -> bool {
    let Some(value) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Ok(parsed) = url::Url::parse(value) else {
        return false;
    };
    if !["http", "https"].contains(&parsed.scheme()) {
        return false;
    }
    if config
        .allowed_origins
        .contains(&parsed.origin().ascii_serialization())
    {
        return true;
    }
    config.address.ip().is_loopback()
        && matches!(
            parsed.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        )
}
async fn origin_guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let mut response = if !origin_allowed(request.headers(), &state.config) {
        error(
            StatusCode::FORBIDDEN,
            "Only configured application origins are allowed.",
        )
    } else {
        next.run(request).await
    };
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    state
        .metrics
        .record_request(response.status().as_u16(), started.elapsed());
    response
}

/// Liveness: the process is running. Always cheap; never touches dependencies.
async fn livez(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"ok": true, "uptimeSeconds": state.metrics.uptime_seconds()}))
}

/// Readiness: safe to receive traffic. Verifies the data directory is writable
/// so a read-only volume or saturated disk takes this machine out of rotation.
async fn readyz(State(state): State<AppState>) -> Response {
    match fs::create_dir_all(&state.config.data_dir) {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(io_error) => {
            eprintln!("Readiness failed: {io_error}");
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "Data directory is not writable.",
            )
        }
    }
}

/// Prometheus text exposition of the series in [`crate::metrics`].
async fn prometheus(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.metrics.render(),
    )
}

async fn activities(State(state): State<AppState>) -> Json<Value> {
    let game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    Json(game.activities.details())
}
async fn status(State(state): State<AppState>) -> Json<Value> {
    let mut setup = fs::read(state.config.data_dir.join("game-inventory.json")).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or_else(|| json!({"found": false, "note": "No local game inventory is available. Run the client asset inventory and set CHAO_DATA_DIR."}));
    setup["imports"] = json!({"directory": state.config.data_dir.join("imports").to_string_lossy(),
        "chao": state.config.data_dir.join("imports/chao.glb").is_file(), "garden": state.config.data_dir.join("imports/garden.glb").is_file()});
    let game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    Json(
        json!({"ok": true, "mode": if state.config.address.ip().is_loopback() { "local-development" } else { "server" },
        "online": game.online_connections(), "setup": setup,
        "tinyGarden":{"games":["memory","rock_paper_scissors","bounce","ball","search"],"rules":"adapted","maxRingReward":crate::phone::MAX_RING_REWARD},
        "terrain": {"loaded": game.terrains.keys().collect::<Vec<_>>(), "flatFallback": TERRAIN_ZONES.into_iter().filter(|zone| !game.terrains.contains_key(*zone)).collect::<Vec<_>>()}}),
    )
}
async fn session(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if !headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"))
    {
        return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Use application/json.");
    }
    let input: Value = match serde_json::from_slice(if body.is_empty() { b"{}" } else { &body }) {
        Ok(value) => value,
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid JSON."),
    };
    if !input.is_object() {
        return error(
            StatusCode::BAD_REQUEST,
            "Session data must be a JSON object.",
        );
    }
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    if input["resumeOnly"] == true
        && input["token"]
            .as_str()
            .and_then(|token| game.player_for_token(token))
            .is_none()
    {
        return error(StatusCode::NOT_FOUND, "Saved Chao was not found.");
    }
    match game.create_session(&input) {
        Ok(response) => match state.persist(&mut game) {
            Ok(()) => Json(response).into_response(),
            Err(error_value) => {
                eprintln!("Session save failed: {error_value}");
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Could not save your session.",
                )
            }
        },
        Err(error_value) => {
            eprintln!("Session creation failed: {error_value}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not create your session.",
            )
        }
    }
}

pub fn asset_filename(path: &str) -> Option<String> {
    let filename = path.strip_prefix("/api/assets/")?;
    let glbs = [
        "chao",
        "hub",
        "garden",
        "hero-garden",
        "dark-garden",
        "neutral-chaos",
        "hero-chaos",
        "dark-chaos",
        "tails",
        "knuckles",
        "amy",
        "chaos-emerald",
        "tree-neutral",
        "dark-chao-walker",
        "dark-walker-shot",
        "dark-walker-booster",
        "shadow-air-shoes-left",
        "shadow-air-shoes-right",
        "knuckles-hammer-gloves-left",
        "knuckles-hammer-gloves-right",
    ];
    let colors = ["red", "purple", "cyan", "green", "blue", "yellow", "white"];
    let pngs = [
        "chao-flame.png",
        "chao-shadow.png",
        "emotion-flame.png",
        "emotion-flame-atlas.png",
        "emotion-sphere.png",
        "chao-stat-panel.png",
        "chao-stat-font.png",
        "chao-race-parameter.png",
        "discord-icon.png",
        "login-chao.png",
        "tiny-neutral-chaos.png",
        "tiny-hero-chaos.png",
        "tiny-dark-chaos.png",
    ];
    let audio = [
        "music-hub",
        "music-garden",
        "music-hero",
        "music-dark",
        "voice-knuckles",
        "voice-tails",
        "voice-amy",
    ];
    if filename == "hub-grass.png" {
        return Some("nativehubgrass.png".into());
    }
    if filename == "shadow-skate-motion.json" {
        return Some(filename.into());
    }
    if [
        "race-normal.glb",
        "race-dark.glb",
        "race-hero.glb",
        "karate-arena.glb",
    ]
    .contains(&filename)
    {
        return Some(format!("activities/{filename}"));
    }
    if glbs.iter().any(|name| filename == format!("{name}.glb"))
        || colors.iter().any(|color| {
            filename == format!("emerald-{color}.glb") || filename == format!("emerald-{color}.png")
        })
        || filename == "emerald-sky.png"
        || pngs.contains(&filename)
        || audio.iter().any(|name| filename == format!("{name}.mp3"))
    {
        Some(filename.into())
    } else {
        None
    }
}
fn byte_range(header_value: &str, length: usize) -> Option<(usize, usize)> {
    let range = header_value.strip_prefix("bytes=")?;
    let (left, right) = range.split_once('-')?;
    if left.is_empty() {
        let suffix = right.parse::<usize>().ok()?;
        if suffix == 0 || length == 0 {
            return None;
        }
        return Some((length.saturating_sub(suffix), length - 1));
    }
    let start = left.parse::<usize>().ok()?;
    if start >= length {
        return None;
    }
    let end = if right.is_empty() {
        length - 1
    } else {
        right.parse::<usize>().ok()?.min(length - 1)
    };
    if end < start {
        None
    } else {
        Some((start, end))
    }
}
async fn asset_or_missing(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
) -> Response {
    let Some(filename) = asset_filename(uri.path()) else {
        return error(StatusCode::NOT_FOUND, "Not found.");
    };
    if method != Method::GET && method != Method::HEAD {
        return error(StatusCode::METHOD_NOT_ALLOWED, "Use GET or HEAD.");
    }
    let contents =
        match tokio::fs::read(state.config.data_dir.join("imports").join(&filename)).await {
            Ok(bytes) => bytes,
            Err(value) => {
                return error(
                    if matches!(
                        value.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                    ) {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    },
                    "Local asset not found.",
                );
            }
        };
    let audio = filename.ends_with(".mp3");
    let mime = if audio {
        "audio/mpeg"
    } else if filename.ends_with(".png") {
        "image/png"
    } else if filename.ends_with(".json") {
        "application/json; charset=utf-8"
    } else {
        "model/gltf-binary"
    };
    let total = contents.len();
    let mut status = StatusCode::OK;
    let mut start = 0;
    let mut end_exclusive = total;
    let mut builder = Response::builder().header(header::CONTENT_TYPE, mime);
    if audio {
        builder = builder.header(header::ACCEPT_RANGES, "bytes");
        if let Some(value) = headers.get(header::RANGE) {
            let Some((first, last)) = value
                .to_str()
                .ok()
                .and_then(|value| byte_range(value, total))
            else {
                return builder
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{total}"))
                    .header(header::CONTENT_LENGTH, "0")
                    .body(Body::empty())
                    .unwrap();
            };
            status = StatusCode::PARTIAL_CONTENT;
            start = first;
            end_exclusive = last + 1;
            builder = builder.header(
                header::CONTENT_RANGE,
                format!("bytes {first}-{last}/{total}"),
            );
        }
    }
    builder
        .status(status)
        .header(header::CONTENT_LENGTH, end_exclusive - start)
        .body(if method == Method::HEAD {
            Body::empty()
        } else {
            Body::from(contents[start..end_exclusive].to_vec())
        })
        .unwrap()
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}
async fn upgrade(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let id = query.token.as_deref().and_then(|token| {
        state
            .game
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .player_for_token(token)
            .map(|p| p.id.clone())
    });
    let Some(id) = id else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    ws.max_message_size(2048)
        .max_frame_size(2048)
        .on_upgrade(move |socket| handle_socket(socket, state, id))
}
async fn handle_socket(socket: WebSocket, state: AppState, id: String) {
    let mut events = state.events.subscribe();
    let mut shutdown = state.shutdown.subscribe();
    let (mut writer, mut reader) = socket.split();
    state.metrics.ws_opened();
    let initial = {
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        game.connect(&id);
        let owner = game.players.get(&id).is_some_and(|p| p.admin);
        game.notice(
            &id,
            if owner {
                "Welcome, owner! Your admin Chao has all S grades, level 99, and max hidden stats."
            } else {
                "Welcome to the garden! Move with WASD and explore with other Chao."
            },
            "info",
        );
        game.snapshot(&id)
    };
    if writer
        .send(Message::Text(initial.to_string().into()))
        .await
        .is_err()
    {
        state.metrics.ws_closed();
        state
            .game
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .disconnect(&id);
        return;
    }
    let mut snapshots = tokio::time::interval(Duration::from_millis(100));
    snapshots.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut heartbeat = tokio::time::interval_at(
        Instant::now() + Duration::from_secs(20),
        Duration::from_secs(20),
    );
    let mut alive = true;
    let mut window = Instant::now();
    let mut count = 0;
    loop {
        let outgoing: Option<Message> = tokio::select! {
            _ = shutdown.changed() => break,
            _ = snapshots.tick() => Some(Message::Text(state.game.lock().unwrap_or_else(|p| p.into_inner()).snapshot(&id).to_string().into())),
            event = events.recv() => match event {
                Ok(event) if event.target.as_deref().is_none_or(|target| target == id) => Some(Message::Text(event.message.to_string().into())),
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => None, Err(_) => break,
            },
            _ = heartbeat.tick() => { if !alive { break; } alive = false; Some(Message::Ping(Bytes::new())) },
            incoming = reader.next() => {
                let Some(Ok(message)) = incoming else { break; };
                match message {
                    Message::Pong(_) => { alive = true; None }, Message::Ping(bytes) => Some(Message::Pong(bytes)), Message::Close(_) => break,
                    Message::Text(_) | Message::Binary(_) => {
                        if window.elapsed() >= Duration::from_secs(1) { window = Instant::now(); count = 0; } count += 1;
                        if count > 80 { let _ = writer.send(Message::Close(Some(CloseFrame { code: 1008, reason: "Message rate exceeded.".into() }))).await; break; }
                        let mut response = None;
                        if let Ok(value) = serde_json::from_slice::<Value>(&message.into_data()) {
                            let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
                            match value["type"].as_str() {
                                Some("input") => game.set_input(&id, &value),
                                Some("tiny-input") => game.tiny_input(&id, &value),
                                Some("action") if value["action"] == "save" => {
                                    let acknowledgement = if !game.request_save(&id) {
                                        json!({"type":"notice","kind":"error","text":"Please wait a moment before saving again."})
                                    } else {
                                        match state.persist(&mut game) {
                                            Ok(()) => json!({"type":"event","kind":"saved","playerId":id,"serverTime":game.time}),
                                            Err(error) => {
                                                eprintln!("Explicit save failed: {error}");
                                                state.metrics.record_persist_failure();
                                                json!({"type":"notice","kind":"error","text":"Could not save your progress. Stay in the garden and try again."})
                                            }
                                        }
                                    };
                                    response = Some(Message::Text(acknowledgement.to_string().into()));
                                }
                                Some("action") if value["action"].as_str().is_some_and(|action|action.starts_with("watch-")) => {
                                    let result=match state.watch_action(&mut game,&id,&value) {
                                        Ok(result)=>result,
                                        Err(error)=>{eprintln!("Could not save resident choice: {error}");json!({"type":"notice","kind":"error","text":"Your resident choice could not save. Your previous mode was kept; please retry."})}
                                    };
                                    response=Some(Message::Text(result.to_string().into()));
                                }
                                Some("action") => game.action(&id, &value), _ => {}
                            }
                        }
                        response
                    }
                }
            }
        };
        if let Some(message) = outgoing
            && !matches!(
                tokio::time::timeout(Duration::from_secs(5), writer.send(message)).await,
                Ok(Ok(()))
            )
        {
            break;
        }
    }
    state.metrics.ws_closed();
    state
        .game
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .disconnect(&id);
}

pub fn spawn_simulation(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut shutdown = state.shutdown.subscribe();
        let mut simulation = tokio::time::interval(Duration::from_millis(50));
        simulation.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut saving = tokio::time::interval(Duration::from_secs(5));
        saving.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = simulation.tick() => {
                    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner()); game.tick(0.05);
                    if crate::phone::has_pending(&game) && game.time >= game.tiny_save_retry_at
                        && let Err(value)=state.persist(&mut game) {
                        eprintln!("Could not save Tiny Garden result: {value}");
                        state.metrics.record_persist_failure();
                        game.tiny_save_retry_at=game.time+1000.;
                        crate::phone::save_failed(&mut game);
                    }
                    for event in game.drain_events() { let _ = state.events.send(event); }
                }
                _ = saving.tick() => {
                    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
                    if let Err(value) = state.persist(&mut game) { eprintln!("Could not save profiles: {value}"); }
                }
            }
        }
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(value) = state.persist(&mut game) {
            eprintln!("Final save failed: {value}");
        }
    })
}
