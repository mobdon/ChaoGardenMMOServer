//! Version 2 transport and transactions for canonical accounts and devices.
use crate::{
    game::{Game, Input},
    identity::IdentityError,
    service::AppState,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        OriginalUri, State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{Instant, MissedTickBehavior};

pub fn capabilities() -> Value {
    json!({"version":2,"authentication":["bearer-header","websocket-first-frame"],"pairing":true,
        "pairingTtlMs":crate::identity::PAIRING_TTL_MS,"deviceRevocation":true,"profileRevisions":true,
        "activityLeases":true,"leaseTtlMs":crate::identity::LEASE_TTL_MS,"intentSequence":true,
        "httpIntents":true,"durableTraining":false})
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/account", get(account))
        .route("/api/pairing", post(pairing))
        .route("/api/pairing/claim", post(claim))
        .route("/api/devices/revoke", post(revoke))
        .route("/api/lease", post(lease))
        .route("/api/intent", post(intent))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
pub(crate) fn expire_inputs(game: &mut Game) {
    let now = now_ms();
    for player in game.players.values_mut() {
        if player
            .identity
            .lease
            .as_ref()
            .is_some_and(|lease| lease.expires_at <= now)
        {
            player.input = Input::default();
            if let Some(run) = &mut player.tiny_game {
                let _ = run
                    .core
                    .input(tiny_garden_games::TinyInput::Move { x: 0, y: 0 });
            }
        }
    }
}
fn failure(code: &str) -> Response {
    let status = match code {
        "unauthorized" | "device_revoked" => StatusCode::UNAUTHORIZED,
        "desktop_approval_required" => StatusCode::FORBIDDEN,
        "invalid_pairing" => StatusCode::UNAUTHORIZED,
        "persistence_failed" => StatusCode::INTERNAL_SERVER_ERROR,
        "invalid_request" | "invalid_activity" => StatusCode::BAD_REQUEST,
        _ => StatusCode::CONFLICT,
    };
    (status, Json(json!({"error":code,"protocolVersion":2}))).into_response()
}
fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .filter(|token| {
            !token.is_empty() && token.len() <= 256 && !token.chars().any(char::is_whitespace)
        })
}
fn authenticated(game: &Game, headers: &HeaderMap) -> Result<(String, String), IdentityError> {
    let token = bearer(headers).ok_or(IdentityError("unauthorized"))?;
    let player = game
        .player_for_token(token)
        .ok_or(IdentityError("unauthorized"))?;
    Ok((
        player.id.clone(),
        player.identity.authenticate(token).unwrap(),
    ))
}
fn active(game: &Game, id: &str, device: &str) -> bool {
    game.players
        .get(id)
        .is_some_and(|p| p.identity.device_kind(device).is_some())
}
fn stop_abandoned_control(game: &mut Game, id: &str) {
    if let Some(mut player) = game.players.remove(id) {
        player.input = Input::default();
        if player.watch.is_none() {
            let _ = game.teleport(&mut player, "hub", 0., 6.);
            if !player
                .tiny_game
                .as_ref()
                .is_some_and(crate::phone::TinyRun::saving)
            {
                player.tiny_game = None;
            }
        }
        game.players.insert(id.to_owned(), player);
    }
}
fn disconnect_device(state: &AppState, id: &str, device: &str) {
    let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    let mut connections = state
        .device_connections
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let key = (id.to_owned(), device.to_owned());
    let remaining = connections
        .get_mut(&key)
        .map(|count| {
            *count = count.saturating_sub(1);
            *count
        })
        .unwrap_or(0);
    if remaining == 0 {
        connections.remove(&key);
    }
    let controls = game.players.get(id).is_some_and(|player| {
        player
            .identity
            .lease
            .as_ref()
            .is_some_and(|lease| lease.device_id == device)
    });
    game.disconnect(id);
    if remaining == 0 && controls {
        stop_abandoned_control(&mut game, id);
    }
}
fn snapshot(game: &Game, id: &str, device: &str) -> Value {
    let mut value = game.snapshot(id);
    let own = game.players[id].public();
    if let Some(players) = value["players"].as_array_mut()
        && !players.iter().any(|player| player["id"] == id)
    {
        players.push(own.clone());
    }
    value["self"] = own;
    value["identity"] = game.players[id].identity.public(device);
    value["protocolVersion"] = json!(2);
    value
}
fn body(headers: &HeaderMap, bytes: &Bytes, allowed: &[&str]) -> Result<Value, IdentityError> {
    if !headers
        .get(header::CONTENT_TYPE)
        .and_then(|s| s.to_str().ok())
        .is_some_and(|s| s.split(';').next() == Some("application/json"))
    {
        return Err(IdentityError("invalid_request"));
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| IdentityError("invalid_request"))?;
    if !value
        .as_object()
        .is_some_and(|map| map.keys().all(|key| allowed.contains(&key.as_str())))
    {
        return Err(IdentityError("invalid_request"));
    }
    Ok(value)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, IdentityError> {
    value[key].as_str().ok_or(IdentityError("invalid_request"))
}
fn revision(value: &Value) -> Result<u64, IdentityError> {
    value["expectedRevision"]
        .as_u64()
        .ok_or(IdentityError("invalid_request"))
}
fn expected(game: &Game, id: &str, value: &Value) -> Result<(), IdentityError> {
    if revision(value)? != game.players[id].identity.revision {
        return Err(IdentityError("stale_revision"));
    }
    Ok(())
}

// All proposals run under the game mutex. A failed write restores every runtime
// object (including receipts, world pickups and event queues), not only identity.
fn transaction(
    state: &AppState,
    game: &mut Game,
    apply: impl FnOnce(&mut Game) -> Result<Value, IdentityError>,
) -> Result<Value, IdentityError> {
    let before = game.clone();
    let proposed = match apply(game) {
        Ok(value) => value,
        Err(error) => {
            *game = before;
            return Err(error);
        }
    };
    game.dirty = true;
    if state.persist(game).is_err() {
        *game = before;
        return Err(IdentityError("persistence_failed"));
    }
    Ok(proposed)
}

async fn account(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let game = state.game.lock().unwrap_or_else(|p| p.into_inner());
    match authenticated(&game,&headers) {Ok((id,device))=>Json(json!({"identity":game.players[&id].identity.public(&device),"snapshot":snapshot(&game,&id,&device),"capabilities":capabilities()})).into_response(),Err(e)=>failure(e.0)}
}
async fn pairing(State(state): State<AppState>, headers: HeaderMap, bytes: Bytes) -> Response {
    let result = (|| {
        let value = body(&headers, &bytes, &["expectedRevision"])?;
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        let (id, device) = authenticated(&game, &headers)?;
        expected(&game, &id, &value)?;
        transaction(&state, &mut game, |game| {
            game.players
                .get_mut(&id)
                .unwrap()
                .identity
                .create_pairing(&device, now_ms())
        })
    })();
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e.0),
    }
}
async fn claim(State(state): State<AppState>, headers: HeaderMap, bytes: Bytes) -> Response {
    let result = (|| {
        let value = body(&headers, &bytes, &["pairingId", "pairingSecret"])?;
        let pairing_id = text(&value, "pairingId")?;
        let secret = text(&value, "pairingSecret")?;
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        let id = game
            .players
            .values()
            .find(|p| {
                p.identity
                    .pairing
                    .as_ref()
                    .is_some_and(|pair| pair.id == pairing_id)
            })
            .map(|p| p.id.clone())
            .ok_or(IdentityError("invalid_pairing"))?;
        transaction(&state, &mut game, |game| {
            game.players
                .get_mut(&id)
                .unwrap()
                .identity
                .claim_pairing(pairing_id, secret, now_ms())
        })
    })();
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e.0),
    }
}
async fn revoke(State(state): State<AppState>, headers: HeaderMap, bytes: Bytes) -> Response {
    let result: Result<Value, IdentityError> = (|| {
        let value = body(&headers, &bytes, &["deviceId", "expectedRevision"])?;
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        let (id, device) = authenticated(&game, &headers)?;
        expected(&game, &id, &value)?;
        transaction(&state, &mut game, |game| {
            let player = game.players.get_mut(&id).unwrap();
            let target = text(&value, "deviceId")?;
            let held = player
                .identity
                .lease
                .as_ref()
                .is_some_and(|lease| lease.device_id == target);
            player.identity.revoke(&device, target)?;
            if held {
                player.input = Input::default();
            }
            if held {
                stop_abandoned_control(game, &id);
            }
            Ok(json!({}))
        })?;
        Ok(json!({"identity":game.players[&id].identity.public(&device)}))
    })();
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e.0),
    }
}

/// Durable autonomous jobs must extend this check when integrated. An expired
/// device lease never erases the active server-owned game/resident/job.
pub fn runtime_activity(game: &Game, id: &str) -> Option<&'static str> {
    let player = &game.players[id];
    if player.watch.is_some() {
        Some("watch")
    } else if player
        .tiny_game
        .as_ref()
        .is_some_and(|run| run.public()["phase"] != "finished")
    {
        Some("tiny")
    } else if player.race.is_some() || player.karate.is_some() {
        Some("competition")
    } else if player.zone == "mission" {
        Some("direct")
    } else {
        None
    }
}
async fn lease(State(state): State<AppState>, headers: HeaderMap, bytes: Bytes) -> Response {
    let result: Result<Value, IdentityError> = (|| {
        let value = body(
            &headers,
            &bytes,
            &["operation", "activity", "leaseId", "expectedRevision"],
        )?;
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        let (id, device) = authenticated(&game, &headers)?;
        transaction(&state, &mut game, |game| {
            let operation = text(&value, "operation")?;
            if operation == "acquire" {
                let activity = text(&value, "activity")?;
                if runtime_activity(game, &id).is_some_and(|current| current != activity) {
                    return Err(IdentityError("activity_conflict"));
                }
            }
            let player = game.players.get_mut(&id).unwrap();
            let expected = revision(&value)?;
            let now = now_ms();
            match operation {
                "acquire" => {
                    player
                        .identity
                        .acquire(&device, text(&value, "activity")?, expected, now)?;
                    player.input = Input::default();
                }
                "renew" => {
                    player
                        .identity
                        .renew(&device, text(&value, "leaseId")?, expected, now)?;
                }
                "release" => {
                    player
                        .identity
                        .release(&device, text(&value, "leaseId")?, expected, now)?;
                    player.input = Input::default();
                }
                _ => return Err(IdentityError("invalid_request")),
            };
            Ok(json!({}))
        })?;
        Ok(json!({"identity":game.players[&id].identity.public(&device)}))
    })();
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e.0),
    }
}

fn intended_activity(game: &Game, id: &str, message: &Value) -> &'static str {
    let action = message["action"].as_str().unwrap_or("");
    if (message["type"] == "input"
        && game.players[id]
            .identity
            .lease
            .as_ref()
            .is_some_and(|lease| lease.activity == "competition"))
        || (runtime_activity(game, id) == Some("competition")
            && matches!(action, "attack" | "hub" | "garden"))
    {
        "competition"
    } else if message["type"] == "tiny-input" || action.starts_with("tiny-") {
        "tiny"
    } else if action.starts_with("watch-") {
        "watch"
    } else if action.contains("race") || action.contains("karate") {
        "competition"
    } else {
        "direct"
    }
}

pub fn apply_intent(
    state: &AppState,
    game: &mut Game,
    id: &str,
    device: &str,
    envelope: &Value,
    http: bool,
) -> Result<Value, IdentityError> {
    if !active(game, id, device) {
        return Err(IdentityError("device_revoked"));
    }
    let mut message = if http {
        envelope["message"].clone()
    } else {
        envelope.clone()
    };
    if !message.is_object() {
        return Err(IdentityError("invalid_request"));
    }
    // A flush uploads no state and is safe for authenticated observers.
    if message["type"] == "action" && message["action"] == "save" {
        game.dirty = true;
        state
            .persist(game)
            .map_err(|_| IdentityError("persistence_failed"))?;
        return Ok(
            json!({"type":"event","kind":"saved","playerId":id,"serverTime":game.time,"identity":game.players[id].identity.public(device)}),
        );
    }
    let lease_id = text(envelope, "leaseId")?;
    let expected = revision(envelope)?;
    let sequence = envelope["sequence"]
        .as_u64()
        .ok_or(IdentityError("invalid_request"))?;
    let kind = message["type"]
        .as_str()
        .ok_or(IdentityError("invalid_request"))?;
    if !["input", "tiny-input", "action"].contains(&kind) {
        return Err(IdentityError("invalid_request"));
    }
    let activity = intended_activity(game, id, &message);
    if game.players[id]
        .identity
        .lease
        .as_ref()
        .is_some_and(|lease| lease.activity != activity)
    {
        return Err(IdentityError("activity_conflict"));
    }
    if runtime_activity(game, id).is_some_and(|current| current != activity) {
        return Err(IdentityError("activity_conflict"));
    }
    for key in ["leaseId", "expectedRevision", "sequence"] {
        message.as_object_mut().unwrap().remove(key);
    }
    let before_watch = game.players[id]
        .watch
        .as_ref()
        .map(|w| w.public(game.players[id].connections > 0));
    let before_happiness = game.players[id].happiness;
    transaction(state, game, |game| {
        let player = game.players.get_mut(id).unwrap();
        player
            .identity
            .accept_intent(device, lease_id, expected, sequence, now_ms())?;
        // HTTP widget care uses the same host validator without fabricating a
        // lasting socket or triggering disconnect's gameplay cancellation.
        if http {
            player.connections += 1;
        }
        match message["type"].as_str() {
            Some("input") => game.set_input(id, &message),
            Some("tiny-input") => game.tiny_input(id, &message),
            Some("action") => game.action(id, &message),
            _ => unreachable!(),
        }
        if http {
            game.players.get_mut(id).unwrap().connections -= 1;
        }
        Ok(json!({}))
    })?;
    let player = &game.players[id];
    let watch = player
        .watch
        .as_ref()
        .map(|w| w.public(player.connections > 0));
    let mut result = json!({"type":"event","kind":"intent-accepted","sequence":sequence,"identity":player.identity.public(device)});
    if message["action"]
        .as_str()
        .is_some_and(|action| action.starts_with("watch-"))
    {
        result["kind"] = json!("watch-saved");
        result["accepted"] = json!(before_watch != watch || before_happiness != player.happiness);
        result["watch"] = json!(watch);
        result["serverTime"] = json!(game.time);
    }
    Ok(result)
}
async fn intent(State(state): State<AppState>, headers: HeaderMap, bytes: Bytes) -> Response {
    let result = (|| {
        let value = body(
            &headers,
            &bytes,
            &["leaseId", "expectedRevision", "sequence", "message"],
        )?;
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        let (id, device) = authenticated(&game, &headers)?;
        apply_intent(&state, &mut game, &id, &device, &value, true)
    })();
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => failure(e.0),
    }
}

pub async fn upgrade(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if uri.query().is_some() {
        return failure("invalid_request");
    }
    let auth = if headers.contains_key(header::AUTHORIZATION) {
        match authenticated(
            &state.game.lock().unwrap_or_else(|p| p.into_inner()),
            &headers,
        ) {
            Ok(auth) => Some(auth),
            Err(e) => return failure(e.0),
        }
    } else {
        None
    };
    ws.max_message_size(2048)
        .max_frame_size(2048)
        .on_upgrade(move |socket| handle_socket(socket, state, auth))
}
async fn handle_socket(mut socket: WebSocket, state: AppState, auth: Option<(String, String)>) {
    let auth = match auth {
        Some(auth) => Some(auth),
        None => match tokio::time::timeout(Duration::from_secs(5), socket.recv()).await {
            Ok(Some(Ok(Message::Text(value)))) => {
                let value = serde_json::from_str::<Value>(&value).ok();
                value
                    .filter(|v| {
                        v["type"] == "authenticate" && v.as_object().is_some_and(|m| m.len() == 2)
                    })
                    .and_then(|v| {
                        let token = v["token"].as_str()?;
                        let game = state.game.lock().unwrap_or_else(|p| p.into_inner());
                        let player = game.player_for_token(token)?;
                        Some((player.id.clone(), player.identity.authenticate(token)?))
                    })
            }
            _ => None,
        },
    };
    let Some((id, device)) = auth else {
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 1008,
                reason: "Authentication required.".into(),
            })))
            .await;
        return;
    };
    let mut events = state.events.subscribe();
    let mut shutdown = state.shutdown.subscribe();
    let initial = {
        let mut game = state.game.lock().unwrap_or_else(|p| p.into_inner());
        if !active(&game, &id, &device) {
            return;
        }
        game.connect(&id);
        *state
            .device_connections
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry((id.clone(), device.clone()))
            .or_default() += 1;
        snapshot(&game, &id, &device)
    };
    let (mut writer, mut reader) = socket.split();
    if writer
        .send(Message::Text(initial.to_string().into()))
        .await
        .is_err()
    {
        disconnect_device(&state, &id, &device);
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
            _=shutdown.changed()=>break,
            _=snapshots.tick()=>{
                let game=state.game.lock().unwrap_or_else(|p|p.into_inner());
                if !active(&game,&id,&device){Some(Message::Close(Some(CloseFrame{code:1008,reason:"Device revoked.".into()})))}
                else{Some(Message::Text(snapshot(&game,&id,&device).to_string().into()))}
            },
            event=events.recv()=>match event {
                Ok(event) if event.target.as_deref().is_none_or(|target|target==id)=>{
                    let game=state.game.lock().unwrap_or_else(|p|p.into_inner());
                    if active(&game,&id,&device){Some(Message::Text(event.message.to_string().into()))}else{break;}
                },Ok(_)|Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>None,Err(_)=>break,
            },
            _=heartbeat.tick()=>{if !alive{break;}alive=false;Some(Message::Ping(Bytes::new()))},
            incoming=reader.next()=>{
                let Some(Ok(message))=incoming else{break;};
                match message {
                    Message::Pong(_)=>{alive=true;None},Message::Ping(bytes)=>Some(Message::Pong(bytes)),Message::Close(_)=>break,
                    Message::Text(_)|Message::Binary(_)=>{
                        if window.elapsed()>=Duration::from_secs(1){window=Instant::now();count=0;}count+=1;
                        if count>80{let _=writer.send(Message::Close(Some(CloseFrame{code:1008,reason:"Message rate exceeded.".into()}))).await;break;}
                        let result=serde_json::from_slice::<Value>(&message.into_data()).map_err(|_|IdentityError("invalid_request")).and_then(|value|{
                            let mut game=state.game.lock().unwrap_or_else(|p|p.into_inner());apply_intent(&state,&mut game,&id,&device,&value,false)
                        });
                        let value=match result{Ok(v)=>v,Err(e)=>json!({"type":"error","error":e.0,"protocolVersion":2})};
                        Some(Message::Text(value.to_string().into()))
                    }
                }
            }
        };
        if let Some(message) = outgoing {
            let closing = matches!(message, Message::Close(_));
            if !matches!(
                tokio::time::timeout(Duration::from_secs(5), writer.send(message)).await,
                Ok(Ok(()))
            ) || closing
            {
                break;
            }
        }
    }
    disconnect_device(&state, &id, &device);
}
