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
    collections::{BTreeMap, BTreeSet},
    env, fs, io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, broadcast, watch},
    time::{Instant, MissedTickBehavior},
};

#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub address: SocketAddr,
    pub owner_bootstrap: bool,
    pub allowed_origins: Vec<String>,
    pub max_connections: usize,
    pub max_profiles: usize,
    pub event_capacity: usize,
    pub send_timeout: Duration,
    pub max_snapshot_bytes: usize,
}
fn bounded_env(name: &str, default: usize, maximum: usize) -> Result<usize, String> {
    let value = env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse::<usize>()
        .map_err(|_| format!("Invalid {name}"))?;
    if value == 0 || value > maximum {
        return Err(format!("{name} must be between 1 and {maximum}"));
    }
    Ok(value)
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
            max_connections: bounded_env("CHAO_MAX_CONNECTIONS", 600, 10000)?,
            max_profiles: bounded_env("CHAO_MAX_PROFILES", 10000, 100000)?,
            event_capacity: bounded_env("CHAO_EVENT_CAPACITY", 512, 65536)?,
            send_timeout: Duration::from_millis(
                bounded_env("CHAO_SEND_TIMEOUT_MS", 1000, 30000)? as u64
            ),
            max_snapshot_bytes: bounded_env("CHAO_MAX_SNAPSHOT_BYTES", 1048576, 8388608)?,
        })
    }
    pub fn local(data_dir: PathBuf, owner_bootstrap: bool) -> Self {
        Self {
            data_dir,
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            owner_bootstrap,
            allowed_origins: vec![],
            max_connections: 600,
            max_profiles: 10000,
            event_capacity: 512,
            send_timeout: Duration::from_secs(1),
            max_snapshot_bytes: 1048576,
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
    connections: Arc<Semaphore>,
    seen_connections: Arc<Mutex<BTreeSet<String>>>,
}
impl AppState {
    pub fn load(config: Config) -> io::Result<Self> {
        if config.max_connections == 0
            || config.max_profiles == 0
            || config.event_capacity == 0
            || config.send_timeout.is_zero()
            || config.max_snapshot_bytes == 0
            || config.max_connections > 10000
            || config.max_profiles > 100000
            || config.event_capacity > 65536
            || config.send_timeout > Duration::from_secs(30)
            || config.max_snapshot_bytes > 8388608
        {
            return Err(io::Error::other("Capacity limits outside supported bounds"));
        }
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
        match (
            env::var_os("CHAO_CARE_AUDIT_DIR"),
            env::var_os("CHAO_CARE_EXECUTABLE"),
        ) {
            (None, None) => {}
            (Some(audit), Some(executable)) => {
                game.care_catalog = crate::care_catalog::CareCatalog::load(
                    &PathBuf::from(audit),
                    &PathBuf::from(executable),
                )
                .map_err(io::Error::other)?;
            }
            _ => {
                return Err(io::Error::other(
                    "Set both CHAO_CARE_AUDIT_DIR and CHAO_CARE_EXECUTABLE",
                ));
            }
        }
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
        let races = read_catalog("races")?;
        let karate = read_catalog("karate")?;
        game.competition_catalog =
            crate::competition::CompetitionCatalog::from_values(&races, &karate)
                .map_err(io::Error::other)?;
        game.activities = crate::activities::ActivityCatalog::from_values(races, karate)
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
        let (events, _) = broadcast::channel(config.event_capacity);
        let (shutdown, _) = watch::channel(false);
        let connections = Arc::new(Semaphore::new(config.max_connections));
        Ok(Self {
            game: Arc::new(Mutex::new(game)),
            config: Arc::new(config),
            events,
            shutdown,
            metrics: Metrics::new(),
            connections,
            seen_connections: Arc::new(Mutex::new(BTreeSet::new())),
        })
    }
    pub fn persist(&self, game: &mut Game) -> io::Result<()> {
        if !game.dirty {
            return Ok(());
        }
        let started = Instant::now();
        let result = (|| {
            crate::competition::validate_reservations(game).map_err(io::Error::other)?;
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
            crate::competition::commit(game);
            Ok(())
        })();
        self.metrics.save.record(started.elapsed());
        if result.is_err() {
            self.metrics.record_persist_failure();
        }
        result
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
    /// The WebSocket mutex serializes devices; success is visible only after durable commit.
    pub fn care_action(&self, game: &mut Game, id: &str, message: &Value) -> io::Result<Value> {
        let before = game
            .players
            .get(id)
            .cloned()
            .ok_or_else(|| io::Error::other("Missing profile"))?;
        let was_dirty = game.dirty;
        let event_count = game.events.len();
        let response = crate::care::action(game, id, message);
        if response["accepted"] == true
            && response["replayed"] == false
            && let Err(error) = self.persist(game)
        {
            game.players.insert(id.into(), before);
            game.dirty = was_dirty;
            game.events.truncate(event_count);
            return Err(error);
        }
        Ok(response)
    }

    /// Opt-in and cancellation are acknowledged only after the profile transaction.
    /// Failed writes retain the prior lease, entry budget and queued events.
    pub fn competition_action(
        &self,
        game: &mut Game,
        id: &str,
        message: &Value,
    ) -> io::Result<Value> {
        let before = game
            .players
            .get(id)
            .cloned()
            .ok_or_else(|| io::Error::other("Missing profile"))?;
        let before_competition = crate::competition::public(&before);
        let was_dirty = game.dirty;
        let event_count = game.events.len();
        game.action(id, message);
        let accepted = before_competition != crate::competition::public(&game.players[id]);
        if let Err(error) = self.persist(game) {
            game.players.insert(id.to_owned(), before);
            game.dirty = was_dirty;
            game.events.truncate(event_count);
            return Err(error);
        }
        let competition = crate::competition::public(&game.players[id]);
        Ok(json!({
            "type":"event", "kind":"competition-saved",
            "accepted":accepted,
            "competition":competition, "serverTime":game.time
        }))
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
    let probe = state
        .config
        .data_dir
        .join(format!(".readiness-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        use std::io::Write;
        fs::create_dir_all(&state.config.data_dir)?;
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&probe)?;
            file.write_all(b"ready\n")?;
            file.sync_all()
        })();
        let cleanup = fs::remove_file(&probe);
        result.and(cleanup)
    })();
    match result {
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
    let resumes_existing = input["token"]
        .as_str()
        .and_then(|token| game.player_for_token(token))
        .is_some();
    if input["resumeOnly"] == true
        && input["token"]
            .as_str()
            .and_then(|token| game.player_for_token(token))
            .is_none()
    {
        return error(StatusCode::NOT_FOUND, "Saved Chao was not found.");
    }
    if !resumes_existing && game.players.len() >= state.config.max_profiles {
        state.metrics.record_admission_rejected();
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Profile capacity reached. Try again later.",
        );
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
    if chao_appearance_asset(filename) {
        return Some(filename.into());
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
/// Locally converted evolution bodies, face and look textures (never in Git).
/// `chao-body-al_<align><family>z.glb`, `chao-face.json`, `chao-look.json`,
/// `chao-eye-<name>.png`, `chao-mouth-<name>.png` and `chao-look-<name>.png` with a bounded lowercase name.
fn chao_appearance_asset(filename: &str) -> bool {
    if filename == "chao-face.json" || filename == "chao-look.json" {
        return true;
    }
    if let Some(family) = filename
        .strip_prefix("chao-body-al_")
        .and_then(|rest| rest.strip_suffix("z.glb"))
    {
        let bytes = family.as_bytes();
        return bytes.len() == 2 && b"ndh".contains(&bytes[0]) && b"cnsfrp".contains(&bytes[1]);
    }
    ["chao-eye-", "chao-look-", "chao-mouth-"]
        .iter()
        .any(|prefix| {
            filename
                .strip_prefix(prefix)
                .and_then(|rest| rest.strip_suffix(".png"))
                .is_some_and(|name| {
                    (1..=32).contains(&name.len())
                        && name
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                })
        })
}

#[cfg(test)]
mod appearance_asset_tests {
    use super::asset_filename;

    #[test]
    fn appearance_assets_are_bounded() {
        for ok in [
            "chao-body-al_ncz.glb",
            "chao-body-al_dpz.glb",
            "chao-face.json",
            "chao-eye-al_eye01.png",
            "chao-look-al_jewel_ruby.png",
        ] {
            assert_eq!(
                asset_filename(&format!("/api/assets/{ok}")).as_deref(),
                Some(ok)
            );
        }
        for bad in [
            "chao-body-al_xcz.glb",
            "chao-body-al_nczz.glb",
            "chao-eye-../x.png",
            "chao-look-.png",
            "chao-eye-AL.png",
            "chao-look-a/b.png",
        ] {
            assert_eq!(asset_filename(&format!("/api/assets/{bad}")), None, "{bad}");
        }
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
    let Ok(permit) = state.connections.clone().try_acquire_owned() else {
        state.metrics.record_admission_rejected();
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Connection capacity reached. Retry with backoff.",
        );
    };
    ws.max_message_size(2048)
        .max_frame_size(2048)
        .write_buffer_size(0)
        .max_write_buffer_size(state.config.max_snapshot_bytes + 65536)
        .on_upgrade(move |socket| handle_socket(socket, state, id, permit))
}

// One in-flight frame per socket; snapshots are made on demand, never queued.
// A stalled transport is cancelled at the deadline, then the socket is dropped.
async fn send_bounded<S>(writer: &mut S, message: Message, state: &AppState) -> bool
where
    S: futures_util::Sink<Message> + Unpin,
{
    let started = Instant::now();
    let result = tokio::time::timeout(state.config.send_timeout, writer.send(message)).await;
    state.metrics.send.record(started.elapsed());
    if result.is_err() {
        state.metrics.record_slow_client();
    }
    matches!(result, Ok(Ok(())))
}

fn snapshot_message(state: &AppState, id: &str) -> Option<(Message, usize)> {
    let started = Instant::now();
    let snapshot = state
        .game
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .snapshot(id);
    // The snapshot owns its values; JSON encoding need not block authority.
    let encoded = snapshot.to_string();
    state.metrics.snapshot.record(started.elapsed());
    if encoded.len() > state.config.max_snapshot_bytes {
        state.metrics.record_oversized_snapshot();
        return None;
    }
    let bytes = encoded.len();
    Some((Message::Text(encoded.into()), bytes))
}

fn publish_events(state: &AppState, game: &mut Game) {
    for event in game.drain_events() {
        let _ = state.events.send(event);
    }
    state.metrics.set_queue_depth(state.events.len());
}

struct ConnectedSocket {
    state: AppState,
    id: String,
}
impl Drop for ConnectedSocket {
    fn drop(&mut self) {
        self.state.metrics.ws_closed();
        self.state
            .game
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .disconnect(&self.id);
    }
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    id: String,
    _permit: OwnedSemaphorePermit,
) {
    let mut events = state.events.subscribe();
    let mut shutdown = state.shutdown.subscribe();
    let (mut writer, mut reader) = socket.split();
    {
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        if !game.connect(&id) {
            return;
        }
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
        publish_events(&state, &mut game);
    }
    let reconnect = !state
        .seen_connections
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(id.clone());
    state.metrics.ws_opened(reconnect);
    let _connected = ConnectedSocket {
        state: state.clone(),
        id: id.clone(),
    };
    let Some((initial, bytes)) = snapshot_message(&state, &id) else {
        return;
    };
    if !send_bounded(&mut writer, initial, &state).await {
        return;
    }
    state.metrics.record_snapshot_sent(bytes);
    let mut snapshots = tokio::time::interval_at(
        Instant::now() + Duration::from_millis(100),
        Duration::from_millis(100),
    );
    snapshots.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut heartbeat = tokio::time::interval_at(
        Instant::now() + Duration::from_secs(20),
        Duration::from_secs(20),
    );
    let mut alive = true;
    let mut window = Instant::now();
    let mut count = 0;
    loop {
        let outgoing: Option<(Message, usize)> = tokio::select! {
            _ = shutdown.changed() => break,
            _ = snapshots.tick() => match snapshot_message(&state, &id) { Some(frame) => Some(frame), None => break },
            event = events.recv() => match event {
                Ok(event) => {
                    state.metrics.set_queue_depth(state.events.len());
                    let visible = state.game.lock().unwrap_or_else(|p| p.into_inner()).event_visible(&id, &event);
                    visible.then(|| (Message::Text(event.message.to_string().into()), 0))
                }
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    // A missing reward or saved event must not look successful.
                    // The persisted receipt survives; reconnect requests current state.
                    state.metrics.record_event_lag(count);
                    break;
                }
                Err(_) => break,
            },
            _ = heartbeat.tick() => {
                if !alive { break; } alive = false; Some((Message::Ping(Bytes::new()), 0))
            },
            incoming = reader.next() => {
                let Some(Ok(message)) = incoming else { break; };
                // Include control frames in the rate bound; ping floods also cost work.
                if window.elapsed() >= Duration::from_secs(1) { window = Instant::now(); count = 0; }
                count += 1;
                if count > 80 {
                    state.metrics.record_rate_disconnect();
                    let _ = send_bounded(&mut writer, Message::Close(Some(CloseFrame { code: 1008, reason: "Message rate exceeded.".into() })), &state).await;
                    break;
                }
                match message {
                    Message::Pong(_) => { alive = true; None },
                    Message::Ping(bytes) => Some((Message::Pong(bytes), 0)),
                    Message::Close(_) => break,
                    Message::Text(_) | Message::Binary(_) => {
                        let mut response = None;
                        if let Ok(value) = serde_json::from_slice::<Value>(&message.into_data()) {
                            let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
                            match value["type"].as_str() {
                                Some("input") => game.set_input(&id, &value),
                                Some("tiny-input") => game.tiny_input(&id, &value),
                                Some("care") => {
                                    let result=match state.care_action(&mut game,&id,&value) {
                                        Ok(result)=>result,
                                        Err(error)=>{eprintln!("Could not save care: {error}");crate::care::failure(&game,&id,&value,"save-failed")}
                                    };
                                    response=Some((Message::Text(result.to_string().into()), 0));
                                }
                                Some("action") if value["action"] == "save" => {
                                    let acknowledgement = if !game.request_save(&id) {
                                        json!({"type":"notice","kind":"error","text":"Please wait a moment before saving again."})
                                    } else {
                                        match state.persist(&mut game) {
                                            Ok(()) => json!({"type":"event","kind":"saved","playerId":id,"serverTime":game.time}),
                                            Err(error) => {
                                                eprintln!("Explicit save failed: {error}");
                                                json!({"type":"notice","kind":"error","text":"Could not save your progress. Stay in the garden and try again."})
                                            }
                                        }
                                    };
                                    response = Some((Message::Text(acknowledgement.to_string().into()), 0));
                                }
                                Some("action") if value["action"].as_str().is_some_and(|action|action.starts_with("competition-")) => {
                                    let result = match state.competition_action(&mut game, &id, &value) {
                                        Ok(result) => result,
                                        Err(error) => {
                                            eprintln!("Could not save competition choice: {error}");
                                            json!({"type":"notice","kind":"error","text":"Your competition choice could not save. Your previous choice was kept; please retry."})
                                        }
                                    };
                                    response = Some((Message::Text(result.to_string().into()), 0));
                                }
                                Some("action") if value["action"].as_str().is_some_and(|action| action.starts_with("watch-")) => {
                                    let result = match state.watch_action(&mut game, &id, &value) {
                                        Ok(result) => result,
                                        Err(error) => {
                                            eprintln!("Could not save resident choice: {error}");
                                            json!({"type":"notice","kind":"error","text":"Your resident choice could not save. Your previous mode was kept; please retry."})
                                        }
                                    };
                                    response = Some((Message::Text(result.to_string().into()), 0));
                                }
                                Some("action") => game.action(&id, &value), _ => {}
                            }
                            // Do not accumulate a second unbounded application event queue.
                            publish_events(&state, &mut game);
                        }
                        response
                    }
                }
            }
        };
        if let Some((message, snapshot_bytes)) = outgoing {
            if !send_bounded(&mut writer, message, &state).await {
                break;
            }
            if snapshot_bytes > 0 {
                state.metrics.record_snapshot_sent(snapshot_bytes);
            }
        }
    }
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
                    let waiting = Instant::now();
                    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
                    state.metrics.tick_lock_wait.record(waiting.elapsed());
                    let started = Instant::now();
                    game.tick(0.05);
                    state.metrics.resident_tick.record(game.last_resident_tick);
                    state.metrics.set_residents(game.last_tick_residents);
                    if (crate::phone::has_pending(&game) || crate::competition::has_pending(&game))
                        && game.time >= game.tiny_save_retry_at
                        && let Err(value)=state.persist(&mut game) {
                        eprintln!("Could not save Tiny Garden result: {value}");
                        game.tiny_save_retry_at=game.time+1000.;
                        crate::phone::save_failed(&mut game);
                        crate::competition::save_failed(&mut game);
                    }
                    publish_events(&state, &mut game);
                    state.metrics.tick.record(started.elapsed());
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

#[cfg(test)]
mod capacity_tests {
    use super::*;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    struct BlockedSink;
    impl futures_util::Sink<Message> for BlockedSink {
        type Error = ();
        fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Ready(Ok(()))
        }
        fn start_send(self: Pin<&mut Self>, _: Message) -> Result<(), ()> {
            Ok(())
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Pending
        }
        fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), ()>> {
            Poll::Pending
        }
    }

    #[tokio::test]
    async fn blocked_initial_control_and_snapshot_sends_have_a_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::local(dir.path().to_owned(), false);
        config.send_timeout = Duration::from_millis(5);
        let state = AppState::load(config).unwrap();
        for message in [
            Message::Text("initial snapshot".into()),
            Message::Close(None),
        ] {
            assert!(!send_bounded(&mut BlockedSink, message, &state).await);
        }
        let rendered = state.metrics.render();
        assert!(rendered.contains("chao_slow_clients_total 2\n"));
        assert!(rendered.contains("chao_send_duration_seconds_count 2\n"));
    }

    #[test]
    fn histogram_sum_and_cumulative_buckets_are_real_measurements() {
        let metrics = Metrics::new();
        metrics.tick.record(Duration::from_millis(2));
        metrics.tick.record(Duration::from_millis(100));
        let rendered = metrics.render();
        assert!(rendered.contains("chao_tick_duration_seconds_bucket{le=\"0.0025\"} 1\n"));
        assert!(rendered.contains("chao_tick_duration_seconds_bucket{le=\"0.1\"} 2\n"));
        assert!(rendered.contains("chao_tick_duration_seconds_sum 0.102\n"));
        assert!(rendered.contains("chao_tick_duration_seconds_count 2\n"));
    }
}
