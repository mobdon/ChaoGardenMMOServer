//! Authoritative care intents. Native rows are evidence; application is explicit MMO tuning.
use crate::{
    care_catalog::CareCatalog,
    evolution::EvolutionEvidence,
    game::{Game, Player},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const RULES: &str = "adapted-care-v1";
pub const MAX_RECEIPTS: usize = 4096;
pub const MAX_QUANTITY: u32 = 99;
pub const MAX_PICKUPS: usize = 100;
const EFFECT_COOLDOWN_MS: f64 = 1500.;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Intent {
    Consume {
        #[serde(rename = "itemId")]
        item_id: String,
    },
    Purchase {
        #[serde(rename = "itemId")]
        item_id: String,
    },
    Pickup {
        #[serde(rename = "pickupId")]
        pickup_id: String,
    },
    Pet,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    #[serde(rename = "type")]
    pub message_type: String,
    pub request_id: String,
    pub expected_revision: u64,
    pub intent: Intent,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pickup {
    pub id: String,
    pub native_id: u8,
    pub zone: String,
    pub x: f64,
    pub z: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub request: Request,
    pub revision: u64,
    pub effects: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CareState {
    pub version: u32,
    pub revision: u64,
    pub inventory: BTreeMap<String, u32>,
    pub evidence: EvolutionEvidence,
    pub raw_totals: BTreeMap<String, i64>,
    pub pickups: BTreeMap<String, Pickup>,
    pub receipts: BTreeMap<String, Receipt>,
    pub last_effect_at: Option<f64>,
    /// A server-owned integration hook for durable training / device activity leases.
    pub activity_lock: Option<String>,
}

impl Default for CareState {
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            inventory: BTreeMap::new(),
            evidence: EvolutionEvidence::default(),
            raw_totals: BTreeMap::new(),
            pickups: BTreeMap::new(),
            receipts: BTreeMap::new(),
            last_effect_at: None,
            activity_lock: None,
        }
    }
}

fn native_id(item_id: &str) -> Option<u8> {
    let id = item_id.strip_prefix("sa2:")?.parse::<u8>().ok()?;
    (id < 25 && item_id == format!("sa2:{id}")).then_some(id)
}
fn valid_item(item_id: &str) -> bool {
    item_id == "garden-fruit" || native_id(item_id).is_some()
}
fn valid_request(r: &Request) -> bool {
    r.message_type == "care"
        && Uuid::parse_str(&r.request_id)
            .is_ok_and(|id| id.hyphenated().to_string() == r.request_id)
        && match &r.intent {
            Intent::Consume { item_id } | Intent::Purchase { item_id } => valid_item(item_id),
            Intent::Pickup { pickup_id } => !pickup_id.is_empty() && pickup_id.len() <= 128,
            Intent::Pet => true,
        }
}

impl CareState {
    /// Only an absent legacy field migrates. Malformed/newer data must stop loading.
    pub fn restore(value: Option<&Value>) -> Result<Self, String> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let state: Self =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid saved care state")?;
        if state.version != 1
            || !state.evidence.valid()
            || state.inventory.len() > 26
            || state
                .inventory
                .iter()
                .any(|(key, n)| !valid_item(key) || *n == 0 || *n > MAX_QUANTITY)
            || state.receipts.len() > MAX_RECEIPTS
            || state.pickups.len() > MAX_PICKUPS
            || state
                .last_effect_at
                .is_some_and(|at| !at.is_finite() || at < 0.)
            || state
                .activity_lock
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 128)
            || state.raw_totals.keys().any(|s| {
                !crate::game::ALL_STATS.contains(&s.as_str())
                    && !["mood", "belly", "unknown18"].contains(&s.as_str())
            })
            || state.pickups.iter().any(|(id, p)| {
                id != &p.id
                    || id.is_empty()
                    || id.len() > 128
                    || p.native_id > 24
                    || p.zone != "mission"
                    || !p.x.is_finite()
                    || !p.z.is_finite()
                    || p.x.abs() > 22.
                    || p.z.abs() > 22.
            })
            || state.receipts.iter().any(|(id, r)| {
                id != &r.request.request_id
                    || !valid_request(&r.request)
                    || r.revision == 0
                    || r.revision > state.revision
                    || r.request.expected_revision.checked_add(1) != Some(r.revision)
            })
            || state
                .receipts
                .values()
                .map(|r| r.revision)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != state.receipts.len()
        {
            return Err("Invalid or unsupported saved care state".into());
        }
        Ok(state)
    }
}

fn result(
    request_id: &str,
    accepted: bool,
    replayed: bool,
    code: &str,
    revision: u64,
    effects: Value,
) -> Value {
    json!({"type":"event","kind":"care-result","requestId":request_id,"accepted":accepted,
        "replayed":replayed,"code":code,"revision":revision,"rulesVersion":RULES,"effects":effects})
}
pub fn failure(game: &Game, id: &str, message: &Value, code: &str) -> Value {
    let request_id = message["requestId"]
        .as_str()
        .filter(|s| s.len() <= 64)
        .unwrap_or_default();
    result(
        request_id,
        false,
        false,
        code,
        game.players.get(id).map_or(0, |p| p.care.revision),
        Value::Null,
    )
}

fn conflict(p: &Player, pickup: bool) -> bool {
    p.connections == 0
        || p.care.activity_lock.is_some()
        || p.watch.is_some()
        || p.tiny_game.is_some()
        || p.race.is_some()
        || p.karate.is_some()
        || p.vehicle.is_some()
        || if pickup {
            p.zone != "mission"
        } else {
            !["hub", "garden", "hero", "dark"].contains(&p.zone.as_str())
        }
}

pub fn public(game: &Game, id: &str) -> Value {
    let Some(p) = game.players.get(id) else {
        return Value::Null;
    };
    let native = game.care_catalog.public();
    json!({"rulesVersion":RULES,"revision":p.care.revision,"inventory":p.care.inventory,
        "needs":{"energy":p.energy,"satiety":p.hunger,"happiness":p.happiness,"units":"adapted-0-to-100"},
        "evidence":p.care.evidence.public(),"rawTotals":p.care.raw_totals,
        "rawUnits":crate::care_catalog::RAW_UNITS,"pointPolicy":"adapted-one-raw-unit-to-one-point-clamp-0-9999-no-level-or-grade-change",
        "pickups":p.care.pickups.values().collect::<Vec<_>>(),"catalog":native,
        "shop":[{"itemId":"garden-fruit","price":3,"enabled":true,"rules":"adapted"},{"itemId":"sa2:22","price":12,"enabled":game.care_catalog.enabled(),"rules":"adapted"},{"itemId":"sa2:23","price":12,"enabled":game.care_catalog.enabled(),"rules":"adapted"}],
        "eligibility":{"canCare":!conflict(p,false),"canPickup":!conflict(p,true),"activityLock":p.care.activity_lock,
            "effectReadyAt":p.care.last_effect_at.map(|at|at+EFFECT_COOLDOWN_MS),"receiptCapacityRemaining":MAX_RECEIPTS.saturating_sub(p.care.receipts.len())}})
}

fn add_quantity(p: &mut Player, key: &str) -> Result<(), &'static str> {
    let n = p.care.inventory.get(key).copied().unwrap_or(0);
    if n >= MAX_QUANTITY {
        return Err("inventory-full");
    }
    p.care.inventory.insert(key.into(), n + 1);
    Ok(())
}
fn take_quantity(p: &mut Player, key: &str) -> Result<(), &'static str> {
    let n = p.care.inventory.get(key).copied().unwrap_or(0);
    if n == 0 {
        return Err("item-not-owned");
    }
    if n == 1 {
        p.care.inventory.remove(key);
    } else {
        p.care.inventory.insert(key.into(), n - 1);
    }
    Ok(())
}
fn near(game: &Game, p: &Player, zone: &str, x: f64, z: f64, range: f64) -> bool {
    p.zone == zone
        && p.jump_offset <= 0.5
        && game.sample_ground(zone, x, z).is_some_and(|ground| {
            ((p.x - x).powi(2) + (p.z - z).powi(2) + (p.y - ground).powi(2)).sqrt() <= range
        })
}
fn effect_ready(p: &Player, now: f64) -> Result<(), &'static str> {
    if p.care
        .last_effect_at
        .is_some_and(|at| now < at + EFFECT_COOLDOWN_MS)
    {
        Err("cooldown")
    } else {
        Ok(())
    }
}
fn points(p: &mut Player, name: &str, delta: i16) -> i64 {
    let stat = p.stats.get_mut(name).expect("seven canonical stats");
    let before = stat.points;
    if !p.admin {
        stat.points = (i64::from(before) + i64::from(delta)).clamp(0, 9999) as u32;
    }
    i64::from(stat.points) - i64::from(before)
}

fn apply(game: &Game, p: &mut Player, request: &Request, now: f64) -> Result<Value, &'static str> {
    let catalog = &game.care_catalog;
    if conflict(p, matches!(request.intent, Intent::Pickup { .. })) {
        return Err("activity-conflict");
    }
    match &request.intent {
        Intent::Purchase { item_id } => {
            let price = match item_id.as_str() {
                "garden-fruit" => 3,
                "sa2:22" | "sa2:23" => 12,
                _ => return Err("not-for-sale"),
            };
            if item_id != "garden-fruit"
                && native_id(item_id).and_then(|id| catalog.item(id)).is_none()
            {
                return Err("native-data-unavailable");
            }
            let shop = near(game, p, "hub", -4., 8., 2.);
            let grove = item_id == "garden-fruit" && near(game, p, "garden", -7., -5., 2.);
            if !shop && !grove {
                return Err("out-of-range");
            }
            if p.rings < price {
                return Err("insufficient-rings");
            }
            add_quantity(p, item_id)?;
            p.rings -= price;
            Ok(json!({"rings":-(i64::from(price)),"inventory":{item_id:1}}))
        }
        Intent::Pickup { pickup_id } => {
            let pickup = p
                .care
                .pickups
                .get(pickup_id)
                .cloned()
                .ok_or("pickup-unavailable")?;
            if !near(game, p, &pickup.zone, pickup.x, pickup.z, 1.25) {
                return Err("out-of-range");
            }
            if catalog.item(pickup.native_id).is_none() {
                return Err("native-data-unavailable");
            }
            let item_id = format!("sa2:{}", pickup.native_id);
            add_quantity(p, &item_id)?;
            p.care.pickups.remove(pickup_id);
            Ok(
                json!({"inventory":{item_id:1},"pickupId":pickup_id,"dropRules":"adapted-mission-drop-v1"}),
            )
        }
        Intent::Pet => {
            effect_ready(p, now)?;
            let before = p.happiness;
            p.happiness = (before + 2.).min(100.);
            p.care.last_effect_at = Some(now);
            Ok(json!({"happiness":p.happiness-before}))
        }
        Intent::Consume { item_id } => {
            effect_ready(p, now)?;
            let (energy, hunger, happiness) = (p.energy, p.hunger, p.happiness);
            let mut delta = BTreeMap::new();
            let raw = if item_id == "garden-fruit" {
                take_quantity(p, item_id)?;
                p.energy = (p.energy + 35.).min(100.);
                p.hunger = (p.hunger + 30.).min(100.);
                p.happiness = (p.happiness + 8.).min(100.);
                delta.insert("stamina", points(p, "stamina", 25));
                Value::Null
            } else {
                let native = catalog
                    .item(native_id(item_id).ok_or("unknown-item")?)
                    .ok_or("native-data-unavailable")?;
                // Resolve effect data before taking the item; never accept a client delta.
                take_quantity(p, item_id)?;
                for (name, value) in native.raw.skill_deltas() {
                    let total = p.care.raw_totals.entry(name.into()).or_default();
                    *total = total.saturating_add(i64::from(value));
                    delta.insert(name, points(p, name, value));
                }
                for (name, value) in [
                    ("mood", native.raw.mood),
                    ("belly", native.raw.belly),
                    ("unknown18", native.raw.unknown_18),
                ] {
                    let total = p.care.raw_totals.entry(name.into()).or_default();
                    *total = total.saturating_add(i64::from(value));
                }
                p.happiness = (p.happiness + f64::from(native.raw.mood)).clamp(0., 100.);
                p.hunger = (p.hunger + f64::from(native.raw.belly)).clamp(0., 100.);
                p.care.evidence.record_use(native.id);
                json!(native.raw)
            };
            p.care.last_effect_at = Some(now);
            Ok(
                json!({"inventory":{item_id:-1},"points":delta,"energy":p.energy-energy,"satiety":p.hunger-hunger,
                "happiness":p.happiness-happiness,"raw":raw,"unknown18Applied":false,"levelsChanged":false,"gradesChanged":false}),
            )
        }
    }
}

/// Mutates only one canonical profile. The host commits it before returning success.
pub(crate) fn action(game: &mut Game, id: &str, message: &Value) -> Value {
    let Ok(request) = serde_json::from_value::<Request>(message.clone()) else {
        return failure(game, id, message, "invalid-request");
    };
    if !valid_request(&request) {
        return failure(game, id, message, "invalid-request");
    }
    let Some(p) = game.players.get(id) else {
        return failure(game, id, message, "profile-unavailable");
    };
    if let Some(receipt) = p.care.receipts.get(&request.request_id) {
        return if receipt.request == request {
            result(
                &request.request_id,
                true,
                true,
                "ok",
                receipt.revision,
                receipt.effects.clone(),
            )
        } else {
            failure(game, id, message, "request-id-conflict")
        };
    }
    if request.expected_revision != p.care.revision {
        return failure(game, id, message, "stale-revision");
    }
    if p.care.receipts.len() >= MAX_RECEIPTS || p.care.revision == u64::MAX {
        return failure(game, id, message, "receipt-capacity");
    }
    let mut next = p.clone();
    let effects = match apply(game, &mut next, &request, game.time) {
        Ok(e) => e,
        Err(code) => return failure(game, id, message, code),
    };
    next.care.revision += 1;
    let revision = next.care.revision;
    next.care.receipts.insert(
        request.request_id.clone(),
        Receipt {
            request: request.clone(),
            revision,
            effects: effects.clone(),
        },
    );
    game.players.insert(id.into(), next);
    game.dirty = true;
    result(&request.request_id, true, false, "ok", revision, effects)
}

/// Server combat calls this once for a defeated robot. Identity ties the drop to its source.
/// Species/drive selection and persistent private drops are MMO rules, not retail drop tables.
pub(crate) fn mission_drop(
    p: &mut Player,
    catalog: &CareCatalog,
    mission_id: &str,
    enemy_id: &str,
    x: f64,
    z: f64,
) {
    if !catalog.enabled() || p.care.pickups.len() >= MAX_PICKUPS || p.care.revision == u64::MAX {
        return;
    }
    let id = format!("{mission_id}:{enemy_id}");
    if p.care.pickups.contains_key(&id)
        || p.care
            .receipts
            .values()
            .any(|r| matches!(&r.request.intent,Intent::Pickup{pickup_id} if pickup_id==&id))
    {
        return;
    }
    use sha2::{Digest, Sha256};
    let native_id = Sha256::digest(id.as_bytes())[0] % 25;
    if catalog.item(native_id).is_none() {
        return;
    }
    p.care.pickups.insert(
        id.clone(),
        Pickup {
            id,
            native_id,
            zone: "mission".into(),
            x,
            z,
        },
    );
    p.care.revision += 1;
}
