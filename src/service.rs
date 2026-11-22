use crate::game::{Event, Game, TERRAIN_ZONES, Terrain};
use crate::metrics::Metrics;
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{
        DefaultBodyLimit, OriginalUri, Path, Query, Request, State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
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

/// Trusted wall time for persisted AFK deadlines, independent of skipped simulation ticks.
pub fn trusted_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

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
    pub tiny_garden_payload: Option<PathBuf>,
    pub tiny_a3ve_payload: Option<PathBuf>,
    pub tiny_asoe_payload: Option<PathBuf>,
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
            tiny_asoe_payload: env::var_os("CHAO_TINY_ASOE_PAYLOAD").map(PathBuf::from),
            tiny_a3ve_payload: env::var_os("CHAO_TINY_A3VE_PAYLOAD").map(PathBuf::from),
            tiny_garden_payload: env::var_os("CHAO_TINY_GARDEN_PAYLOAD").map(PathBuf::from),
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
            tiny_garden_payload: None,
            tiny_a3ve_payload: None,
            tiny_asoe_payload: None,
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
    pub accounts: Arc<Mutex<crate::accounts::Accounts>>,
    mailer: Arc<Option<crate::mail::Mailer>>,
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
        game.life_policy = crate::life::Policy::from_env().map_err(io::Error::other)?;
        // Native Chao behaviour data is extracted locally from the owner's disc
        // and never committed. Missing files leave the resident on its
        // pre-existing routine; malformed files stop the server.
        let imports = config.data_dir.join("imports");
        let motion = imports.join("chao-motion-table.json");
        if motion.is_file() {
            game.native_chao.motion = Some(std::sync::Arc::new(
                crate::behaviour::MotionTable::load(&motion)
                    .map_err(|e| io::Error::other(format!("chao-motion-table.json: {e}")))?,
            ));
        }
        let aim = imports.join("chao-garden-aim.json");
        if aim.is_file() {
            game.native_chao.aim = Some(std::sync::Arc::new(
                crate::behaviour::AimLists::load(&aim)
                    .map_err(|e| io::Error::other(format!("chao-garden-aim.json: {e}")))?,
            ));
        }
        if let Some(path) = &config.tiny_garden_payload {
            let bytes = crate::care::load_tiny_payload(path)?;
            game.native_tiny.load_a2ne(&bytes)?;
            game.tiny_shop_catalog = Some(
                tiny_garden_core::shop::TinyShopCatalog::from_verified_payload(&bytes)
                    .map_err(io::Error::other)?,
            );
            game.tiny_egg_source = Some(
                crate::collection::EggSource::from_verified_payload(&bytes)
                    .map_err(io::Error::other)?,
            );
        }
        if let Some(path) = &config.tiny_asoe_payload {
            game.native_tiny.load_rps(path)?;
        }
        if let Some(path) = &config.tiny_a3ve_payload {
            game.native_tiny.load_ball(path)?;
        }
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
            accounts: Arc::new(Mutex::new(crate::accounts::Accounts::load(
                config.data_dir.clone(),
            )?)),
            mailer: Arc::new(crate::mail::Mailer::from_env()),
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
    /// Life intents may also change the named partner, so both profiles roll back together.
    pub fn life_action(&self, game: &mut Game, id: &str, message: &Value) -> io::Result<Value> {
        let mut before = vec![(
            id.to_owned(),
            game.players
                .get(id)
                .cloned()
                .ok_or_else(|| io::Error::other("Missing profile"))?,
        )];
        if let Some(partner) = crate::life::partner_of(message).filter(|p| p != id)
            && let Some(player) = game.players.get(&partner)
        {
            before.push((partner, player.clone()));
        }
        let was_dirty = game.dirty;
        let event_count = game.events.len();
        let response = crate::life::action(game, id, message);
        if response["accepted"] == true
            && response["replayed"] == false
            && let Err(error) = self.persist(game)
        {
            for (key, player) in before {
                game.players.insert(key, player);
            }
            game.dirty = was_dirty;
            game.events.truncate(event_count);
            return Err(error);
        }
        Ok(response)
    }
    /// The WebSocket mutex serializes devices; success is visible only after durable commit.
    pub fn care_action(&self, game: &mut Game, id: &str, message: &Value) -> io::Result<Value> {
        self.care_action_at(game, id, message, trusted_unix_ms())
    }
    /// Host/test clock injection only; there is no client clock field or endpoint.
    pub fn care_action_at(
        &self,
        game: &mut Game,
        id: &str,
        message: &Value,
        now: u64,
    ) -> io::Result<Value> {
        self.care_action_inner(game, id, message, now, false)
    }
    fn care_action_inner(
        &self,
        game: &mut Game,
        id: &str,
        message: &Value,
        now: u64,
        automatic: bool,
    ) -> io::Result<Value> {
        let before = game
            .players
            .get(id)
            .cloned()
            .ok_or_else(|| io::Error::other("Missing profile"))?;
        let was_dirty = game.dirty;
        let event_count = game.events.len();
        let response = crate::care::action_at(game, id, message, now, automatic);
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
    /// Closes ready jobs under the same mutex and atomic store as explicit care.
    pub fn settle_ready_training_at(&self, game: &mut Game, now: u64) -> io::Result<usize> {
        let ready: Vec<_> = game
            .players
            .iter()
            .filter(|(_, p)| !crate::game_save::leased(game, p.card(), std::time::Instant::now()))
            .filter_map(|(id, p)| crate::training::ready_job(p, now).map(|job| (id.clone(), job)))
            .collect();
        let mut count = 0;
        for (id, job) in ready {
            let revision = game.players[&id].care.revision;
            let request = json!({"type":"care","requestId":uuid::Uuid::new_v4().to_string(),"expectedRevision":revision,"intent":{"kind":"training_settle","jobId":job}});
            let result = self.care_action_inner(game, &id, &request, now, true)?;
            if result["accepted"] == true && result["replayed"] == false {
                count += 1;
                game.events.push(Event{target:Some(id),message:json!({"type":"event","kind":"training-result","revision":result["revision"],"result":result["effects"]["training"],"rulesVersion":crate::training::RULES})});
            }
        }
        Ok(count)
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/activities", get(activities))
        .route("/api/session", post(session))
        .route("/ws", get(upgrade))
        .route(
            "/api/game-save",
            get(game_save_get)
                .put(game_save_put)
                .layer(DefaultBodyLimit::max(crate::game_save::MAX_BYTES)),
        )
        .route(
            "/api/game-save/lease",
            post(game_save_renew).delete(game_save_release),
        )
        .route("/api/game-save/status", get(game_save_status))
        .route("/api/chao-record", put(chao_record_put))
        .route("/api/chao-record/{id}", get(chao_record_get))
        .route("/api/card", get(card))
        .route("/api/chao", delete(delete_chao))
        .route("/api/account/register", post(account_register))
        .route("/api/account/login", post(account_login))
        .route("/api/account/forgot", post(account_forgot))
        .route("/api/account/reset", post(account_reset))
        .route("/api/tiny-care", post(tiny_care))
        .route(
            "/api/tiny-chao",
            get(tiny_chao_get)
                .put(tiny_chao_put)
                .layer(DefaultBodyLimit::max(crate::tiny_chao::MODEL_MAX)),
        )
        .route("/api/settings", get(settings_get).post(settings_post))
        .route("/api/gear-pickup", post(gear_pickup))
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
        "online": game.online_connections(), "setup": setup,"capabilities":{"careMutations":true},
        "tinyGarden":{"games":["memory","rock_paper_scissors","bounce","ball","search"],"rules":game.native_tiny.rules(),"nativeTinyGames":game.native_tiny.capabilities(),"maxRingReward":game.native_tiny.max_ring_reward(),"shop":crate::care::tiny_shop_catalog(&game),"autoResident":true,"observerSockets":true,"training":crate::training::capability(),"ownedEggs":{"enabled":game.tiny_egg_source.is_some(),"rulesVersion":crate::collection::RULES,"rotationMs":crate::collection::ROTATION_MS,"maxHeldEggs":1,"maxChao":crate::collection::MAX_CHAO,"selectionEnabled":false}},
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
    let mut input: Value = match serde_json::from_slice(if body.is_empty() { b"{}" } else { &body })
    {
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
    if input["ownerLogin"] == true {
        // Owner bootstrap only runs on loopback (checked at startup).
        let Some(token) = state
            .config
            .owner_bootstrap
            .then(|| game.owner_token())
            .flatten()
        else {
            return error(StatusCode::FORBIDDEN, "Owner login is off.");
        };
        input["token"] = json!(token);
        input["resumeOnly"] = json!(true);
    }
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
    if !resumes_existing && let Some(card) = input["card"].as_str() {
        let on_card = game.card_chao(card).len();
        if on_card == 0 {
            return error(StatusCode::NOT_FOUND, "Memory card was not found.");
        }
        if on_card >= crate::game::CARD_CHAO {
            return error(
                StatusCode::CONFLICT,
                "This memory card already holds 8 Chao.",
            );
        }
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
        // An egg that can't hatch says why; the egg stays carried.
        Err(error_value) if !input["egg"].is_null() => error(StatusCode::BAD_REQUEST, &error_value),
        Err(error_value) => {
            eprintln!("Session creation failed: {error_value}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not create your session.",
            )
        }
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}
/// The memory card the caller opens (`Authorization: Bearer <Chao token or
/// card code>`) and the lease header. Game saves and leases are per card.
fn save_caller(game: &Game, headers: &HeaderMap) -> Option<(String, Option<String>)> {
    let id = game.card_for(bearer(headers)?)?;
    let lease = headers
        .get("x-save-lease")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    Some((id, lease))
}
fn save_failed(what: &str, cause: io::Error) -> Response {
    eprintln!("Game save {what} failed: {cause}");
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Could not reach your saved Chao.",
    )
}
/// The memory card's read: checks the save out (or renews the caller's lease).
async fn game_save_get(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    let Some((id, held)) = save_caller(&game, &headers) else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    let Some(lease) =
        crate::game_save::check_out(&mut game, &id, held.as_deref(), std::time::Instant::now())
    else {
        return error(
            StatusCode::CONFLICT,
            "Your Chao is already open in another game.",
        );
    };
    match crate::game_save::read(&state.config.data_dir, &id) {
        Ok((revision, bytes)) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_string()),
                (
                    header::HeaderName::from_static("x-save-revision"),
                    revision.to_string(),
                ),
                (header::HeaderName::from_static("x-save-lease"), lease),
            ],
            bytes,
        )
            .into_response(),
        Err(io_error) => save_failed("read", io_error),
    }
}
/// The memory card's write: needs the caller's lease and the revision it read.
async fn game_save_put(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    let Some((id, held)) = save_caller(&game, &headers) else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    if !crate::game_save::holds(&game, &id, held.as_deref(), std::time::Instant::now()) {
        return error(
            StatusCode::CONFLICT,
            "This game no longer holds your Chao. Load it again.",
        );
    }
    let Some(expected) = headers
        .get("x-save-revision")
        .and_then(|v| v.to_str().ok()?.parse::<u64>().ok())
    else {
        return error(StatusCode::BAD_REQUEST, "Missing save revision.");
    };
    if body.is_empty() {
        return error(StatusCode::BAD_REQUEST, "Empty save.");
    }
    // shortcut: no content check yet; check with the recompiled AL_Crc32 once the shared headless runtime exposes it.
    let dir = &state.config.data_dir;
    let result = crate::game_save::read(dir, &id).and_then(|(revision, _)| {
        if revision != expected {
            return Ok(None);
        }
        crate::game_save::write(dir, &id, revision + 1, &body).map(|()| Some(revision + 1))
    });
    match result {
        Ok(Some(revision)) => Json(json!({"ok": true, "revision": revision})).into_response(),
        Ok(None) => error(
            StatusCode::CONFLICT,
            "Your save changed elsewhere. Load it again.",
        ),
        Err(io_error) => save_failed("write", io_error),
    }
}
/// The game page reports the game's own shoe pickup in a level.
async fn gear_pickup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    let Some(token) = bearer(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    if game.player_for_token(token).is_none() {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    }
    match game.gear_pickup_for_token(token, body["item"].as_str().unwrap_or_default()) {
        Some(found) => Json(json!({ "foundGear": found })).into_response(),
        None => error(StatusCode::BAD_REQUEST, "Unknown gear item."),
    }
}
/// Size of one native Chao save record.
pub const CHAO_RECORD_BYTES: usize = 0x800;
/// The caller's own Chao record, raw, from its game (client chao_player
/// export_record). A changed record bumps the version others see in the
/// snapshot as players[].chaoRecordVersion.
async fn chao_record_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if body.len() != CHAO_RECORD_BYTES {
        return error(StatusCode::BAD_REQUEST, "A Chao record is 2048 bytes.");
    }
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    let Some(id) = bearer(&headers)
        .and_then(|t| game.player_for_token(t))
        .map(|p| p.id.clone())
    else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    let Some(p) = game.players.get_mut(&id) else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    if p.chao_record != body {
        p.chao_record = body.to_vec();
        p.chao_record_version += 1;
    }
    Json(json!({"ok": true, "version": p.chao_record_version})).into_response()
}
/// Another connected player's Chao record in the caller's room (the same
/// players its snapshot lists), for the caller's game to draw that Chao.
async fn chao_record_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    let Some(viewer) = bearer(&headers).and_then(|t| game.player_for_token(t)) else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    match game
        .players
        .get(&id)
        .filter(|p| p.connections > 0 && !p.chao_record.is_empty() && game.same_room(viewer, p))
    {
        Some(p) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_string()),
                (
                    header::HeaderName::from_static("x-chao-record-version"),
                    p.chao_record_version.to_string(),
                ),
            ],
            p.chao_record.clone(),
        )
            .into_response(),
        None => error(StatusCode::NOT_FOUND, "That Chao isn't here."),
    }
}
/// Insert a memory card: every Chao on it, with the token that opens each.
async fn card(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    let chao = bearer(&headers)
        .map(|code| game.card_sessions(code))
        .unwrap_or_default();
    if chao.is_empty() {
        return error(StatusCode::NOT_FOUND, "Memory card was not found.");
    }
    Json(json!({"chao": chao})).into_response()
}
/// The pause menu's Extras settings, for the game page outside a room.
async fn settings_get(State(state): State<AppState>, headers: HeaderMap) -> Response {
    settings_reply(&state, &headers, None)
}
async fn settings_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(changes): Json<Value>,
) -> Response {
    settings_reply(&state, &headers, Some(&changes))
}
fn settings_reply(state: &AppState, headers: &HeaderMap, changes: Option<&Value>) -> Response {
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    match bearer(headers).and_then(|token| game.settings_for_token(token, changes)) {
        Some(settings) => Json(json!({ "settings": settings })).into_response(),
        None => error(StatusCode::UNAUTHORIZED, "Invalid session token."),
    }
}
#[allow(clippy::result_large_err)]
fn json_body(headers: &HeaderMap, body: &Bytes) -> Result<Value, Response> {
    if !headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"))
    {
        return Err(error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Use application/json.",
        ));
    }
    serde_json::from_slice(body).map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid JSON."))
}
/// Makes an account for the memory card the caller holds (its card code).
async fn account_register(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let input = match json_body(&headers, &body) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let field = |k: &str| input[k].as_str().unwrap_or_default().trim().to_owned();
    let (username, email, card) = (field("username"), field("email"), field("card"));
    let password = input["password"].as_str().unwrap_or_default().to_owned();
    if !crate::accounts::valid_username(&username) {
        return error(
            StatusCode::BAD_REQUEST,
            "Usernames are 3 to 20 letters, numbers, _ or -.",
        );
    }
    if !crate::accounts::valid_email(&email) {
        return error(StatusCode::BAD_REQUEST, "That email doesn't look right.");
    }
    if !crate::accounts::valid_password(&password) {
        return error(
            StatusCode::BAD_REQUEST,
            "Passwords are 8 to 128 characters.",
        );
    }
    if state
        .game
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .card_chao(&card)
        .is_empty()
    {
        return error(StatusCode::NOT_FOUND, "Memory card was not found.");
    }
    let made = {
        let (username, email, password, card) =
            (username.clone(), email.clone(), password, card.clone());
        tokio::task::spawn_blocking(move || {
            crate::accounts::Account::new(&username, &email, &password, &card)
        })
        .await
    };
    let Ok(Ok(account)) = made else {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not make your account.",
        );
    };
    let mut accounts = state.accounts.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(why) = accounts.taken(&username, &email, &account.card_hash) {
        return error(StatusCode::CONFLICT, why);
    }
    match accounts.insert(account) {
        Ok(()) => Json(json!({"username": username})).into_response(),
        Err(cause) => {
            eprintln!("Account save failed: {cause}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not save your account.",
            )
        }
    }
}
/// Username (or email) and password: answers with the card code and the
/// card's Chao, as `/api/card` does for a card code.
/// Mails a password-reset link. Always answers the same, so it can't tell who has an account.
async fn account_forgot(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let input = match json_body(&headers, &body) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let login = input["login"].as_str().unwrap_or_default().trim();
    if !login.is_empty() && login.len() <= 254 {
        let started = state
            .accounts
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .start_reset(login, std::time::Instant::now());
        match (started, state.mailer.as_ref()) {
            (Some((email, username, token)), Some(_)) => {
                let mailer = state.mailer.clone();
                tokio::task::spawn_blocking(move || {
                    if let Some(mailer) = mailer.as_ref()
                        && let Err(cause) = mailer.send_reset(&email, &username, &token)
                    {
                        eprintln!("Reset mail failed: {cause}");
                    }
                });
            }
            (Some(_), None) => eprintln!("Reset asked for, but mail is not configured."),
            (None, _) => {}
        }
    }
    Json(json!({"ok": true, "text": "If that account exists, a reset link is on its way."}))
        .into_response()
}
/// Sets a new password with a mailed token. The card moves to a new code (the old
/// one is sealed under the forgotten password), so every device signs in again.
async fn account_reset(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let input = match json_body(&headers, &body) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let password = input["password"].as_str().unwrap_or_default().to_owned();
    if !crate::accounts::valid_password(&password) {
        return error(StatusCode::BAD_REQUEST, "Use 8 to 128 characters.");
    }
    let token = input["token"].as_str().unwrap_or_default();
    let Some(old) = state
        .accounts
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take_reset(token, std::time::Instant::now())
    else {
        return error(
            StatusCode::GONE,
            "That reset link is used up or too old. Ask for a new one.",
        );
    };
    let card = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let (username, email, sealed_for) = (old.username.clone(), old.email.clone(), card.clone());
    let made = tokio::task::spawn_blocking(move || {
        crate::accounts::Account::new(&username, &email, &password, &sealed_for)
    })
    .await;
    let Ok(Ok(account)) = made else {
        return error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not reset your password.",
        );
    };
    let failed = |what: &str, cause: io::Error| {
        eprintln!("Password reset {what} failed: {cause}");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not reset your password.",
        )
    };
    let new_hash = account.card_hash.clone();
    let dir = &state.config.data_dir;
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    let before = game.move_card(&old.card_hash, &card);
    if let Err(cause) = crate::game_save::rename(dir, &old.card_hash, &new_hash) {
        game.restore_players(before);
        return failed("save move", cause);
    }
    let undo = |game: &mut Game, before| {
        let _ = crate::game_save::rename(dir, &new_hash, &old.card_hash);
        game.restore_players(before);
        let _ = state.persist(game);
    };
    if let Err(cause) = state.persist(&mut game) {
        undo(&mut game, before);
        return failed("profile save", cause);
    }
    if let Err(cause) = state
        .accounts
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .replace(account)
    {
        undo(&mut game, before);
        return failed("account save", cause);
    }
    let chao = game.card_sessions(&card);
    let mut response = Json(json!({"card": card, "chao": chao})).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
/// The PC stores its player's own Chao model for the phone (Bearer: the Chao's token).
async fn tiny_chao_put(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    // The Chao that stores its model is the account's phone Chao: the other Chao on the same
    // memory card give theirs up.
    let id = bearer(&headers).and_then(|token| {
        let game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        let player = game.player_for_token(token)?;
        let others: Vec<String> = game
            .players
            .values()
            .filter(|p| p.card() == player.card() && p.id != player.id)
            .map(|p| p.id.clone())
            .collect();
        Some((player.id.clone(), others))
    });
    let Some((id, others)) = id else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    if crate::tiny_chao::model_type(&body).is_none() {
        return error(StatusCode::BAD_REQUEST, "Send a PNG or GLB file.");
    }
    let path = crate::tiny_chao::model_path(&state.config.data_dir, &id);
    match crate::accounts::write_private(&path, &body) {
        Ok(()) => {
            for other in others {
                let _ =
                    fs::remove_file(crate::tiny_chao::model_path(&state.config.data_dir, &other));
            }
            Json(json!({"ok": true, "bytes": body.len()})).into_response()
        }
        Err(cause) => {
            eprintln!("Chao model save failed: {cause}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not save the Chao model.",
            )
        }
    }
}
/// The phone, logged in to the same account, fetches its player's Chao model (Bearer: the
/// Chao's token). Nobody else can read it.
async fn tiny_chao_get(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let id = bearer(&headers).and_then(|token| {
        let game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        game.player_for_token(token).map(|p| p.id.clone())
    });
    let Some(id) = id else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    match tokio::fs::read(crate::tiny_chao::model_path(&state.config.data_dir, &id)).await {
        Ok(bytes) => match crate::tiny_chao::model_type(&bytes) {
            Some(mime) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
            None => error(StatusCode::NOT_FOUND, "No Chao model stored yet."),
        },
        Err(cause) if cause.kind() == io::ErrorKind::NotFound => {
            error(StatusCode::NOT_FOUND, "No Chao model stored yet.")
        }
        Err(cause) => {
            eprintln!("Chao model read failed: {cause}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not read the Chao model.",
            )
        }
    }
}
/// The phone widget's Pet / Rest / Wake (Bearer: the Chao's token). Body: {"kind": "pet" | "rest" | "wake"}.
async fn tiny_care(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let input = match json_body(&headers, &body) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let Some(kind) = input["kind"]
        .as_str()
        .filter(|kind| ["pet", "rest", "wake"].contains(kind))
    else {
        return error(StatusCode::BAD_REQUEST, "Unknown care.");
    };
    let Some(token) = bearer(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    match game.widget_care(token, kind) {
        Ok(player) => Json(json!({"player": player})).into_response(),
        Err(text) if text == "Invalid session token." => error(StatusCode::UNAUTHORIZED, text),
        Err(text) => error(StatusCode::CONFLICT, text),
    }
}
async fn account_login(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let input = match json_body(&headers, &body) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let login = input["login"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let password = input["password"].as_str().unwrap_or_default().to_owned();
    let refused = || error(StatusCode::UNAUTHORIZED, "Wrong username or password.");
    if login.is_empty() || login.len() > 254 || password.len() > 512 {
        return refused();
    }
    let account = {
        let accounts = state.accounts.lock().unwrap_or_else(|p| p.into_inner());
        if accounts.locked(&login, std::time::Instant::now()) {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many tries. Wait a minute.",
            );
        }
        accounts.find(&login).cloned()
    };
    let card = tokio::task::spawn_blocking(move || match account {
        Some(account) => account.open(&password),
        None => {
            crate::accounts::waste_a_check(&password);
            None
        }
    })
    .await
    .ok()
    .flatten();
    let mut accounts = state.accounts.lock().unwrap_or_else(|p| p.into_inner());
    let Some(card) = card else {
        accounts.failed(&login, std::time::Instant::now());
        return refused();
    };
    accounts.succeeded(&login);
    drop(accounts);
    let chao = state
        .game
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .card_sessions(&card);
    let mut response = Json(json!({"card": card, "chao": chao})).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
/// The login screen's delete: copies profiles.json aside first, then removes the Chao for good.
async fn delete_chao(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(token) = bearer(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    if game.player_for_token(token).is_none() {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    }
    let profiles = state.config.data_dir.join("profiles.json");
    if profiles.exists() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Err(cause) = fs::copy(
            &profiles,
            state
                .config
                .data_dir
                .join(format!("profiles.before-delete-{stamp}.json")),
        ) {
            return save_failed("backup", cause);
        }
    }
    match game.delete_chao(token) {
        Ok(id) => match state.persist(&mut game) {
            Ok(()) => {
                let _ = fs::remove_file(crate::tiny_chao::model_path(&state.config.data_dir, &id));
                Json(json!({"deleted": id})).into_response()
            }
            Err(cause) => save_failed("delete", cause),
        },
        Err(message) => error(StatusCode::CONFLICT, message),
    }
}
/// The title screen's look at the card: never checks the save out.
async fn game_save_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (id, in_game) = {
        let game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        let Some((id, _)) = save_caller(&game, &headers) else {
            return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
        };
        let in_game = crate::game_save::leased(&game, &id, std::time::Instant::now());
        (id, in_game)
    };
    match crate::game_save::status(&state.config.data_dir, &id) {
        Ok(saved) => {
            let saved_at = saved.and_then(|(_, at)| {
                Some(at.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64)
            });
            Json(json!({
                "revision": saved.map_or(0, |(revision, _)| revision),
                "savedAt": saved_at,
                "inGame": in_game,
            }))
            .into_response()
        }
        Err(io_error) => save_failed("status", io_error),
    }
}
/// The game is still running with the card in: keep the lease without moving the save.
async fn game_save_renew(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    let Some((id, held)) = save_caller(&game, &headers) else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    let now = std::time::Instant::now();
    if !crate::game_save::holds(&game, &id, held.as_deref(), now) {
        return error(
            StatusCode::CONFLICT,
            "This game no longer holds your Chao. Load it again.",
        );
    }
    crate::game_save::check_out(&mut game, &id, held.as_deref(), now);
    Json(json!({"ok": true})).into_response()
}
/// The game ejected the card or closed: the Chao is free for other surfaces.
async fn game_save_release(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    let Some((id, held)) = save_caller(&game, &headers) else {
        return error(StatusCode::UNAUTHORIZED, "Invalid session token.");
    };
    if crate::game_save::holds(&game, &id, held.as_deref(), std::time::Instant::now()) {
        game.game_save_leases.remove(&id);
    }
    Json(json!({"ok": true})).into_response()
}

/// The only game-derived files the server hands out: what the Tiny Chao Garden phone app
/// and widget draw. Everything else comes from each player's own game files in the browser.
pub fn asset_filename(path: &str) -> Option<String> {
    match path {
        "/__widget-art/tiny-garden-a2ne.png" => return Some("tiny-garden-a2ne.png".into()),
        "/__widget-art/tiny-garden-a2ne-sidebar-1.png" => {
            return Some("tiny-garden-sidebar-1-a2ne.png".into());
        }
        _ => {}
    }
    let filename = path.strip_prefix("/api/assets/")?;
    const TINY: [&str; 11] = [
        "tiny-neutral-chaos.png",
        "tiny-hero-chaos.png",
        "tiny-dark-chaos.png",
        "tiny-garden-a2ne.png",
        "tiny-garden-sidebar-0-a2ne.png",
        "tiny-garden-sidebar-1-a2ne.png",
        // The phone page's stat panel, face emeralds and flame (src/tiny.js, tiny-avatar.js).
        "chao-stat-panel.png",
        "chao-stat-font.png",
        "chao-flame.png",
        "emerald-sky.png",
        // Not game data.
        "discord-icon.png",
    ];
    let colors = ["red", "purple", "cyan", "green", "blue", "yellow", "white"];
    (TINY.contains(&filename)
        || colors
            .iter()
            .any(|color| filename == format!("emerald-{color}.png")))
    .then(|| filename.into())
}

#[cfg(test)]
mod asset_list_tests {
    use super::asset_filename;

    #[test]
    fn only_tiny_garden_assets_are_served() {
        for ok in [
            "tiny-dark-chaos.png",
            "tiny-garden-a2ne.png",
            "chao-flame.png",
            "emerald-cyan.png",
            "discord-icon.png",
        ] {
            assert_eq!(
                asset_filename(&format!("/api/assets/{ok}")).as_deref(),
                Some(ok)
            );
        }
        assert_eq!(
            asset_filename("/__widget-art/tiny-garden-a2ne-sidebar-1.png").as_deref(),
            Some("tiny-garden-sidebar-1-a2ne.png")
        );
        for gone in [
            "chao.glb",
            "hub.glb",
            "garden.glb",
            "shadow-air-shoes-left.glb",
            "music-hub.mp3",
            "medchart-page-a.png",
            "item-panel-3.png",
            "chao-body-al_ncz.glb",
            "metal-chao-rigged.glb",
            "hat-1.glb",
            "activities/race-normal.glb",
            "race-normal.glb",
            "hub-grass.png",
            "shadow-skate-motion.json",
            "emerald-cyan.glb",
            "../tiny-dark-chaos.png",
        ] {
            assert_eq!(
                asset_filename(&format!("/api/assets/{gone}")),
                None,
                "{gone}"
            );
        }
    }
}

async fn asset_or_missing(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
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
    // Every name on the list is a PNG.
    Response::builder()
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CONTENT_LENGTH, contents.len())
        .body(if method == Method::HEAD {
            Body::empty()
        } else {
            Body::from(contents)
        })
        .unwrap()
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
    observe: Option<u8>,
    /// 1 when Tiny Chao Garden connects; any other control connection is the main game.
    tiny: Option<u8>,
}
fn connection_snapshot(game: &Game, id: &str, observer: bool) -> Value {
    let mut snapshot = game.snapshot(id);
    snapshot["connectionMode"] = json!(if observer { "observe" } else { "control" });
    snapshot
}
/// Tiny Chao Garden is for phones only: the browser or app must say it is a mobile one.
fn phone_agent(headers: &HeaderMap) -> bool {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|agent| {
            ["Mobi", "Android", "iPhone", "iPad"]
                .iter()
                .any(|m| agent.contains(m))
        })
}
async fn upgrade(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let observer = match query.observe {
        None | Some(0) => false,
        Some(1) => true,
        _ => return error(StatusCode::BAD_REQUEST, "Invalid observation mode."),
    };
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
    let tiny = query.tiny == Some(1);
    if tiny && !phone_agent(&headers) {
        return error(StatusCode::FORBIDDEN, "Tiny Chao Garden is on your phone.");
    }
    if tiny && !crate::tiny_chao::played_once(&state.config.data_dir, &id) {
        return error(
            StatusCode::FORBIDDEN,
            "Play once on your PC first, then your Chao is here.",
        );
    }
    if !observer && !tiny {
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        game.bring_tiny_egg(&id);
    }
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
        .on_upgrade(move |socket| handle_socket(socket, state, id, permit, observer, tiny))
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

fn snapshot_message(state: &AppState, id: &str, observer: bool) -> Option<(Message, usize)> {
    let started = Instant::now();
    let snapshot = connection_snapshot(
        &state.game.lock().unwrap_or_else(|p| p.into_inner()),
        id,
        observer,
    );
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
    observer: bool,
}
impl Drop for ConnectedSocket {
    fn drop(&mut self) {
        self.state.metrics.ws_closed();
        if self.observer {
            return;
        }
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
    observer: bool,
    tiny: bool,
) {
    let mut events = state.events.subscribe();
    let mut shutdown = state.shutdown.subscribe();
    let (mut writer, mut reader) = socket.split();
    {
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        if !observer {
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
        }
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
        observer,
    };
    let Some((initial, bytes)) = snapshot_message(&state, &id, observer) else {
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
            _ = snapshots.tick() => {
                if !observer && state.game.lock().unwrap_or_else(|p| p.into_inner()).benched.contains(&id) {
                    let _ = send_bounded(&mut writer, Message::Close(Some(CloseFrame { code: 4001, reason: "Another Chao from this memory card is out now.".into() })), &state).await;
                    break;
                }
                match snapshot_message(&state, &id, observer) { Some(frame) => Some(frame), None => break }
            },
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
                        if observer {
                            response=Some((Message::Text(json!({"type":"notice","kind":"error","code":"observer-read-only","text":"This connection only observes your Chao. Open Tiny Garden for activities."}).to_string().into()), 0));
                        } else if let Ok(value) = serde_json::from_slice::<Value>(&message.into_data()) {
                            let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
                            let moving = value["type"] == "input" || (value["type"] == "action" && value["action"] == "save");
                            let tiny_play = value["type"] == "tiny-input" || (value["type"] == "action" && value["action"].as_str().is_some_and(|a| a.starts_with("tiny-")));
                            if tiny_play && !tiny {
                                response=Some((Message::Text(json!({"type":"notice","kind":"error","code":"tiny-needs-phone","text":"Tiny Chao Garden plays on your phone."}).to_string().into()), 0));
                            } else if !moving && game.players.get(&id).is_some_and(|p| crate::game_save::leased(&game, p.card(), std::time::Instant::now())) {
                                response=Some((Message::Text(json!({"type":"notice","kind":"error","code":"in-game","text":"Your Chao is in the big garden right now. Leave the game to care for it here."}).to_string().into()), 0));
                            } else {
                            match value["type"].as_str() {
                                Some("input") => game.set_input(&id, &value),
                                Some("tiny-input") => game.tiny_input(&id, &value),
                                Some("life") => {
                                    let result=match state.life_action(&mut game,&id,&value) {
                                        Ok(result)=>result,
                                        Err(error)=>{eprintln!("Could not save life: {error}");crate::life::failure(&game,&id,&value,"save-failed")}
                                    };
                                    response=Some((Message::Text(result.to_string().into()), 0));
                                }
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
                    if let Err(error) = state.settle_ready_training_at(&mut game, trusted_unix_ms()) {
                        eprintln!("Could not settle ready training: {error}");
                    }
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

#[cfg(test)]
mod account_tests {
    use super::*;
    use tower::ServiceExt;

    async fn post(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::post(path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn register_then_login_returns_the_card() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
        let app = router(state.clone());
        let (status, made) = post(&app, "/api/session", json!({"name": "Test"})).await;
        assert_eq!(status, StatusCode::OK, "{made}");
        let card = made["card"].as_str().unwrap().to_owned();
        let account = json!({"username": "tester", "email": "t@example.com", "password": "long enough pw", "card": card});
        assert_eq!(
            post(&app, "/api/account/register", account.clone()).await.0,
            StatusCode::OK
        );
        assert_eq!(
            post(&app, "/api/account/register", account).await.0,
            StatusCode::CONFLICT
        );
        let (status, login) = post(
            &app,
            "/api/account/login",
            json!({"login": "T@example.com", "password": "long enough pw"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(login["card"], card);
        assert_eq!(login["chao"][0]["token"], made["token"]);
        let wrong = json!({"login": "tester", "password": "wrong password"});
        assert_eq!(
            post(&app, "/api/account/login", wrong).await.0,
            StatusCode::UNAUTHORIZED
        );
        let on_disk = fs::read_to_string(dir.path().join("accounts.json")).unwrap();
        assert!(!on_disk.contains(&card) && !on_disk.contains("long enough pw"));
    }

    async fn new_account(app: &Router) -> (String, String, String) {
        let (_, made) = post(app, "/api/session", json!({"name": "Test"})).await;
        let card = made["card"].as_str().unwrap().to_owned();
        let token = made["token"].as_str().unwrap().to_owned();
        let id = made["player"]["id"].as_str().unwrap().to_owned();
        let account = json!({"username": "tester", "email": "t@example.com", "password": "long enough pw", "card": card});
        assert_eq!(
            post(app, "/api/account/register", account).await.0,
            StatusCode::OK
        );
        (card, token, id)
    }

    #[tokio::test]
    async fn widget_care_acts_on_a_chao_that_is_out_in_a_garden() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
        let app = router(state.clone());
        let (_, token, id) = new_account(&app).await;
        let care = |bearer: String, kind: &'static str| {
            let app = app.clone();
            async move {
                let request = axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/tiny-care")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
                    .body(axum::body::Body::from(json!({"kind": kind}).to_string()))
                    .unwrap();
                let response = app.oneshot(request).await.unwrap();
                let status = response.status();
                let bytes = http_body_util::BodyExt::collect(response.into_body())
                    .await
                    .unwrap()
                    .to_bytes();
                (
                    status,
                    serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
                )
            }
        };
        assert_eq!(care("bad".into(), "pet").await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(care(token.clone(), "feed").await.0, StatusCode::BAD_REQUEST);
        // Not out in a garden yet.
        assert_eq!(care(token.clone(), "pet").await.0, StatusCode::CONFLICT);
        {
            let mut game = state.game.lock().unwrap();
            assert!(game.connect(&id));
            game.action(&id, &json!({"type": "action", "action": "watch-start"}));
            game.disconnect(&id);
        }
        let (status, cared) = care(token.clone(), "pet").await;
        assert_eq!(status, StatusCode::OK, "{cared}");
        assert_eq!(cared["player"]["id"], id);
        assert_eq!(cared["player"]["watch"]["lastCare"]["kind"], "pet");
        // A second tap inside the care cooldown changes nothing.
        assert_eq!(care(token, "pet").await.0, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn the_last_chao_to_store_a_model_is_the_phone_chao() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
        let app = router(state.clone());
        let (card, first, _) = new_account(&app).await;
        let (status, made) = post(
            &app,
            "/api/session",
            json!({"name": "Second", "card": card}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
        let second = made["token"].as_str().unwrap().to_owned();
        // Synthetic test bytes with a PNG magic.
        let mut sheet = b"\x89PNG\r\n\x1a\n".to_vec();
        sheet.resize(64, 1);
        assert_eq!(
            tiny_chao(&app, "PUT", &first, sheet.clone()).await.0,
            StatusCode::OK
        );
        assert_eq!(
            tiny_chao(&app, "GET", &first, vec![]).await.0,
            StatusCode::OK
        );
        assert_eq!(
            tiny_chao(&app, "PUT", &second, sheet).await.0,
            StatusCode::OK
        );
        assert_eq!(
            tiny_chao(&app, "GET", &second, vec![]).await.0,
            StatusCode::OK
        );
        assert_eq!(
            tiny_chao(&app, "GET", &first, vec![]).await.0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn forgot_password_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
        let app = router(state.clone());
        let (card, token, id) = new_account(&app).await;
        crate::game_save::write(
            dir.path(),
            &crate::game::hash_token(&card),
            3,
            b"SA2CARD1 save",
        )
        .unwrap();
        // The same answer whether or not the account exists.
        for login in ["tester", "nobody"] {
            let (status, body) = post(&app, "/api/account/forgot", json!({"login": login})).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["ok"], true);
        }
        // Mail is off in tests: take a fresh link the way the mailer would get it.
        let reset = state
            .accounts
            .lock()
            .unwrap()
            .start_reset(
                "t@example.com",
                std::time::Instant::now() + crate::accounts::MAIL_GAP,
            )
            .unwrap()
            .2;
        let short = json!({"token": reset, "password": "short"});
        assert_eq!(
            post(&app, "/api/account/reset", short).await.0,
            StatusCode::BAD_REQUEST
        );
        let (status, done) = post(
            &app,
            "/api/account/reset",
            json!({"token": reset, "password": "a brand new pw"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{done}");
        let new_card = done["card"].as_str().unwrap().to_owned();
        assert_ne!(new_card, card);
        assert_eq!(done["chao"][0]["player"]["id"], id.as_str());
        // Single use.
        let again = json!({"token": reset, "password": "another new pw"});
        assert_eq!(
            post(&app, "/api/account/reset", again).await.0,
            StatusCode::GONE
        );
        // Old password and old credentials are dead; the new password opens the same Chao.
        let old_login = json!({"login": "tester", "password": "long enough pw"});
        assert_eq!(
            post(&app, "/api/account/login", old_login).await.0,
            StatusCode::UNAUTHORIZED
        );
        let resume = json!({"token": token, "resumeOnly": true});
        assert_eq!(
            post(&app, "/api/session", resume).await.0,
            StatusCode::NOT_FOUND
        );
        let (status, login) = post(
            &app,
            "/api/account/login",
            json!({"login": "tester", "password": "a brand new pw"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(login["card"], new_card.as_str());
        let new_token = login["chao"][0]["token"].as_str().unwrap();
        let resume = json!({"token": new_token, "resumeOnly": true});
        assert_eq!(post(&app, "/api/session", resume).await.0, StatusCode::OK);
        // The game save moved with the card.
        let (revision, bytes) =
            crate::game_save::read(dir.path(), &crate::game::hash_token(&new_card)).unwrap();
        assert_eq!((revision, bytes.as_slice()), (3, &b"SA2CARD1 save"[..]));
        // It survives a restart.
        let reloaded = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
        assert!(
            reloaded
                .game
                .lock()
                .unwrap()
                .player_for_token(new_token)
                .is_some()
        );
        assert!(
            reloaded
                .game
                .lock()
                .unwrap()
                .player_for_token(&token)
                .is_none()
        );
    }

    async fn tiny_chao(
        app: &Router,
        method: &str,
        bearer: &str,
        body: Vec<u8>,
    ) -> (StatusCode, Vec<u8>) {
        let request = axum::http::Request::builder()
            .method(method)
            .uri("/api/tiny-chao")
            .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
            .body(axum::body::Body::from(body))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        (status, bytes.to_vec())
    }

    #[tokio::test]
    async fn phone_works_from_the_same_login_after_one_pc_play() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::{
            connect_async,
            tungstenite::{Message as WsMessage, client::IntoClientRequest},
        };
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::load(Config::local(dir.path().to_owned(), false)).unwrap();
        let app = router(state.clone());
        let (_, token, id) = new_account(&app).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, app.clone()).into_future());
        let url = |extra: &str| format!("ws://{address}/ws?token={token}{extra}");

        // Before the PC has played: no model, and the phone's Tiny Garden does not connect.
        assert_eq!(
            tiny_chao(&app, "GET", &token, vec![]).await.0,
            StatusCode::NOT_FOUND
        );
        assert!(connect_async(url("&tiny=1")).await.is_err());

        // The PC plays once and stores the player's own Chao model.
        // Synthetic test bytes with a GLB magic, larger than the default 2 KB body limit.
        let mut model = b"glTF".to_vec();
        model.resize(8192, 7);
        assert_eq!(
            tiny_chao(&app, "PUT", "bad", model.clone()).await.0,
            StatusCode::UNAUTHORIZED
        );
        let junk = b"not a model".to_vec();
        assert_eq!(
            tiny_chao(&app, "PUT", &token, junk).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            tiny_chao(&app, "PUT", &token, model.clone()).await.0,
            StatusCode::OK
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = crate::tiny_chao::model_path(dir.path(), &id);
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        // The same login on the phone (a fresh login returns the same Chao token) gets the model.
        let (_, login) = post(
            &app,
            "/api/account/login",
            json!({"login": "tester", "password": "long enough pw"}),
        )
        .await;
        let phone_token = login["chao"][0]["token"].as_str().unwrap();
        let (status, bytes) = tiny_chao(&app, "GET", phone_token, vec![]).await;
        assert_eq!((status, bytes), (StatusCode::OK, model));
        // Another player gets their own (missing) model, never this one.
        let (_, other) = post(&app, "/api/session", json!({"name": "Other"})).await;
        let other_token = other["token"].as_str().unwrap();
        assert_eq!(
            tiny_chao(&app, "GET", other_token, vec![]).await.0,
            StatusCode::NOT_FOUND
        );

        // Phone only: a desktop browser is refused even after the PC has played.
        assert!(
            connect_async(url("&tiny=1")).await.is_err(),
            "desktop agent"
        );
        let mut request = url("&tiny=1").into_client_request().unwrap();
        let agent = "Mozilla/5.0 (Linux; Android 14) Mobile Safari/537.36";
        request
            .headers_mut()
            .insert(header::USER_AGENT, agent.parse().unwrap());
        let (mut phone, _) = connect_async(request).await.expect("phone connects");
        let _ = phone.close(None).await;
        // A PC socket can't play Tiny Garden.
        let (mut pc, _) = connect_async(url("")).await.unwrap();
        let start = json!({"type":"action","action":"tiny-start","gameId":"memory"});
        pc.send(WsMessage::Text(start.to_string().into()))
            .await
            .unwrap();
        let refused = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(Ok(message)) = pc.next().await {
                if let WsMessage::Text(text) = message
                    && text.contains("tiny-needs-phone")
                {
                    return true;
                }
            }
            false
        })
        .await;
        assert_eq!(refused, Ok(true));
    }
}
