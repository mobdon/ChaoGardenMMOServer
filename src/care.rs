//! Authoritative care intents. Native rows are evidence; application is explicit MMO tuning.
use crate::{
    care_catalog::CareCatalog,
    evolution::EvolutionEvidence,
    game::{Game, Player},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::{fs, io, io::Read, path::Path};
use uuid::Uuid;

pub const RULES: &str = "adapted-care-v1";
pub const TINY_SHOP_RULES: &str = "adapted-tiny-shop-v1";
pub const TINY_FRUIT_RULES: &str = "adapted-tiny-fruit-care-v1";
pub const MAX_RECEIPTS: usize = 4096;
pub const MAX_HISTORY: usize = 64;
pub const MAX_QUANTITY: u32 = 99;
pub const MAX_PICKUPS: usize = 100;
const EFFECT_COOLDOWN_MS: f64 = 1500.;
const GARDEN_FRUIT_PRICE: u32 = 3;

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
    #[serde(rename = "tiny_purchase")]
    TinyPurchase {
        #[serde(rename = "itemId")]
        item_id: String,
    },
    #[serde(rename = "tiny_enter")]
    TinyEnter,
    #[serde(rename = "tiny_egg_purchase")]
    TinyEggPurchase {
        #[serde(rename = "eggId")]
        egg_id: u8,
        #[serde(rename = "stockEpoch")]
        stock_epoch: u64,
    },
    #[serde(rename = "tiny_egg_hatch")]
    TinyEggHatch {
        #[serde(rename = "ownedEggId")]
        owned_egg_id: String,
    },
    #[serde(rename = "training_start")]
    TrainingStart {
        stat: tiny_garden_core::training::Stat,
        #[serde(rename = "durationMs")]
        duration_ms: u64,
    },
    #[serde(rename = "training_cancel")]
    TrainingCancel {
        #[serde(rename = "jobId")]
        job_id: u64,
    },
    #[serde(rename = "training_settle")]
    TrainingSettle {
        #[serde(rename = "jobId")]
        job_id: u64,
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

/// A bounded display log of actual accepted care transactions. The receipt
/// ledger remains complete and authoritative when older display rows expire.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryEntry {
    pub id: String,
    /// Trusted host Unix time, never the browser or simulation clock.
    pub at: u64,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    pub result: Value,
}

impl HistoryEntry {
    fn from_receipt(receipt: &Receipt, at: u64) -> Self {
        let (kind, item_id) = match &receipt.request.intent {
            Intent::Consume { item_id } => ("consume", Some(item_id.clone())),
            Intent::Purchase { item_id } => ("purchase", Some(item_id.clone())),
            Intent::TinyPurchase { item_id } => ("tiny_purchase", Some(item_id.clone())),
            Intent::TinyEnter => ("tiny_enter", None),
            Intent::TinyEggPurchase { .. } => ("tiny_egg_purchase", None),
            Intent::TinyEggHatch { .. } => ("tiny_egg_hatch", None),
            Intent::TrainingStart { .. } => ("training_start", None),
            Intent::TrainingCancel { .. } => ("training_cancel", None),
            Intent::TrainingSettle { .. } => ("training_settle", None),
            Intent::Pickup { .. } => ("pickup", None),
            Intent::Pet => ("pet", None),
        };
        Self {
            id: receipt.request.request_id.clone(),
            at,
            kind: kind.into(),
            item_id,
            result: receipt.effects.clone(),
        }
    }
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
    /// Legacy profiles begin empty: never invent activities from old receipts.
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    pub last_effect_at: Option<f64>,
    /// A server-owned integration hook for durable training / device activity leases.
    pub activity_lock: Option<String>,
    /// Durable, explicit Tiny Garden residency opt-in. Legacy profiles stay manual.
    #[serde(default)]
    pub tiny_resident: bool,
    #[serde(default)]
    pub training: Option<crate::training::TrainingState>,
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
            history: Vec::new(),
            last_effect_at: None,
            activity_lock: None,
            tiny_resident: false,
            training: None,
        }
    }
}

fn native_id(item_id: &str) -> Option<u8> {
    let id = item_id.strip_prefix("sa2:")?.parse::<u8>().ok()?;
    (id < 25 && item_id == format!("sa2:{id}")).then_some(id)
}
fn valid_item(item_id: &str) -> bool {
    item_id == "garden-fruit" || native_id(item_id).is_some() || tiny_fruit_id(item_id).is_some()
}
fn tiny_fruit_id(item_id: &str) -> Option<usize> {
    let id = item_id.strip_prefix("tiny-fruit:")?.parse::<usize>().ok()?;
    (id < 7 && item_id == format!("tiny-fruit:{id}")).then_some(id)
}
fn valid_request(r: &Request) -> bool {
    r.message_type == "care"
        && Uuid::parse_str(&r.request_id)
            .is_ok_and(|id| id.hyphenated().to_string() == r.request_id)
        && match &r.intent {
            Intent::Consume { item_id }
            | Intent::Purchase { item_id }
            | Intent::TinyPurchase { item_id } => valid_item(item_id),
            Intent::Pickup { pickup_id } => !pickup_id.is_empty() && pickup_id.len() <= 128,
            Intent::Pet | Intent::TinyEnter => true,
            Intent::TinyEggPurchase { egg_id, .. } => *egg_id < 12,
            Intent::TinyEggHatch { owned_egg_id } => Uuid::parse_str(owned_egg_id)
                .is_ok_and(|id| id.hyphenated().to_string() == *owned_egg_id),
            Intent::TrainingStart { duration_ms, .. } => {
                *duration_ms >= 60000
                    && *duration_ms <= 28800000
                    && duration_ms.is_multiple_of(60000)
            }
            Intent::TrainingCancel { job_id } | Intent::TrainingSettle { job_id } => *job_id > 0,
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
            || state
                .training
                .as_ref()
                .is_some_and(|training| training.validate(None).is_err())
            || !state.evidence.valid()
            || state.inventory.len() > 33
            || state
                .inventory
                .iter()
                .any(|(key, n)| !valid_item(key) || *n == 0 || *n > MAX_QUANTITY)
            || state.receipts.len() > MAX_RECEIPTS
            || state.history.len() > MAX_HISTORY
            || state.history.iter().any(|entry| {
                state
                    .receipts
                    .get(&entry.id)
                    .is_none_or(|receipt| *entry != HistoryEntry::from_receipt(receipt, entry.at))
            })
            || state.history.windows(2).any(|pair| {
                state.receipts[&pair[0].id].revision >= state.receipts[&pair[1].id].revision
            })
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
        || p.watch.is_some() && !p.care.tiny_resident
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

/// This inventory offer is an explicit adaptation, not an audited native fruit table.
pub fn tiny_shop_catalog(game: &Game) -> Value {
    let mut items = vec![
        json!({"itemId":"garden-fruit","name":"Garden Fruit","price":GARDEN_FRUIT_PRICE,
        "enabled":true,"maxQuantity":MAX_QUANTITY}),
    ];
    let native = if let Some(catalog) = &game.tiny_shop_catalog {
        for fruit in &catalog.fruits {
            let name = ["Orange", "Blue", "Pink", "Green", "Purple", "Yellow", "Red"]
                [usize::from(fruit.id)];
            items.push(json!({"itemId":format!("tiny-fruit:{}",fruit.id),"name":format!("{name} Fruit"),
                "price":fruit.price,"enabled":true,"maxQuantity":MAX_QUANTITY,"source":"native-a2ne",
                "effectRules":TINY_FRUIT_RULES,"rawEffects":{"units":"signed-native-tiny-fruit-bytes",
                    "fields":["mood","belly","swim","fly","run","power","stamina"],"values":fruit.raw}}));
        }
        json!({"enabled":true,"payloadSha1":tiny_garden_core::gba::REFERENCE_GARDEN_SHA1,
            "effectsOffset":tiny_garden_core::shop::EFFECTS_OFFSET,"pricesOffset":tiny_garden_core::shop::PRICES_OFFSET,
            "effectsSha256":tiny_garden_core::shop::EFFECTS_SHA256,"pricesSha256":tiny_garden_core::shop::PRICES_SHA256,
            "applicationRules":TINY_FRUIT_RULES,"nativeLevelUpPorted":false})
    } else {
        json!({"enabled":false,"reason":"native-data-unavailable"})
    };
    json!({"rulesVersion":TINY_SHOP_RULES,"enabled":true,"items":items,"nativeCatalog":native})
}

pub fn load_tiny_shop(path: &Path) -> io::Result<tiny_garden_core::shop::TinyShopCatalog> {
    let bytes = load_tiny_payload(path)?;
    tiny_garden_core::shop::TinyShopCatalog::from_verified_payload(&bytes).map_err(io::Error::other)
}

/// Shared bounded local-source read; each reader still verifies its exact spans.
pub fn load_tiny_payload(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    let expected = tiny_garden_core::gba::REFERENCE_GARDEN_LEN;
    if !metadata.is_file() || metadata.len() != expected as u64 {
        return Err(io::Error::other(
            "Tiny Garden payload length is unsupported",
        ));
    }
    let mut bytes = Vec::with_capacity(expected);
    fs::File::open(path)?
        .take(expected as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn tiny_purchase_block(game: &Game, p: &Player) -> Option<&'static str> {
    if conflict(p, false) {
        Some("activity-conflict")
    } else if p.care.receipts.len() >= MAX_RECEIPTS || p.care.revision == u64::MAX {
        Some("receipt-capacity")
    } else {
        let mut offers = vec![("garden-fruit".to_owned(), GARDEN_FRUIT_PRICE)];
        if let Some(catalog) = &game.tiny_shop_catalog {
            offers.extend(
                catalog
                    .fruits
                    .iter()
                    .map(|fruit| (format!("tiny-fruit:{}", fruit.id), fruit.price)),
            );
        }
        if !offers.iter().any(|(_, price)| p.rings >= *price) {
            Some("insufficient-rings")
        } else if !offers.iter().any(|(id, price)| {
            p.rings >= *price && p.care.inventory.get(id).copied().unwrap_or(0) < MAX_QUANTITY
        }) {
            Some("inventory-full")
        } else {
            None
        }
    }
}

pub fn public(game: &Game, id: &str) -> Value {
    let Some(p) = game.players.get(id) else {
        return Value::Null;
    };
    let native = game.care_catalog.public();
    let mut tiny_shop = tiny_shop_catalog(game);
    let blocked = tiny_purchase_block(game, p);
    tiny_shop["eligibility"] = json!({"canPurchase":blocked.is_none(),"reason":blocked});
    json!({"rulesVersion":RULES,"revision":p.care.revision,"inventory":p.care.inventory,
        "needs":{"energy":p.energy,"satiety":p.hunger,"happiness":p.happiness,"units":"adapted-0-to-100"},
        "evidence":p.care.evidence.public(),"rawTotals":p.care.raw_totals,
        "rawUnits":crate::care_catalog::RAW_UNITS,"pointPolicy":"adapted-one-raw-unit-to-one-point-clamp-0-9999-no-level-or-grade-change",
        "pickups":p.care.pickups.values().collect::<Vec<_>>(),"catalog":native,"tinyShop":tiny_shop,
        "tinyResident":crate::watch::tiny_public(p),
        "training":crate::training::public(p,crate::service::trusted_unix_ms()),
        "tinyEggShop":crate::collection::shop_public(game,p,crate::service::trusted_unix_ms()),
        "tinyCollection":crate::collection::public(game,p,crate::service::trusted_unix_ms()),
        "history":p.care.history,
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

fn apply(
    game: &Game,
    p: &mut Player,
    request: &Request,
    now: f64,
    training_now: u64,
    automatic: bool,
) -> Result<Value, &'static str> {
    let catalog = &game.care_catalog;
    if matches!(
        request.intent,
        Intent::TrainingStart { .. }
            | Intent::TrainingCancel { .. }
            | Intent::TrainingSettle { .. }
    ) {
        return crate::training::apply(
            game,
            p,
            &request.intent,
            training_now,
            &request.request_id,
            automatic,
        );
    }
    if !matches!(request.intent, Intent::TinyEnter)
        && conflict(p, matches!(request.intent, Intent::Pickup { .. }))
    {
        return Err("activity-conflict");
    }
    match &request.intent {
        Intent::TrainingStart { .. }
        | Intent::TrainingCancel { .. }
        | Intent::TrainingSettle { .. } => unreachable!("training is dispatched above"),
        Intent::TinyEggPurchase { .. } | Intent::TinyEggHatch { .. } => {
            crate::collection::apply(game, p, &request.intent, training_now, &request.request_id)
        }
        Intent::TinyEnter => {
            crate::watch::enter_tiny(game, p)?;
            Ok(json!({"tinyResident":crate::watch::tiny_public(p)}))
        }
        Intent::TinyPurchase { item_id } => {
            let price = if item_id == "garden-fruit" {
                GARDEN_FRUIT_PRICE
            } else if let Some(id) = tiny_fruit_id(item_id) {
                game.tiny_shop_catalog
                    .as_ref()
                    .ok_or("native-data-unavailable")?
                    .fruits[id]
                    .price
            } else {
                return Err("not-for-sale");
            };
            if p.rings < price {
                return Err("insufficient-rings");
            }
            add_quantity(p, item_id)?;
            p.rings -= price;
            Ok(json!({"rings":-(i64::from(price)),"inventory":{item_id:1}}))
        }
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
            let raw = if let Some(id) = tiny_fruit_id(item_id) {
                let fruit = &game
                    .tiny_shop_catalog
                    .as_ref()
                    .ok_or("native-data-unavailable")?
                    .fruits[id];
                take_quantity(p, item_id)?;
                // Native needs use signed-byte ×20 in a 0..200 range. Map to
                // this host's 0..100 needs explicitly; native experience-bar
                // level-up behavior is not copied into our canonical skills.
                p.happiness = (p.happiness + f64::from(fruit.raw[0]) * 10.).clamp(0., 100.);
                p.hunger = (p.hunger + f64::from(fruit.raw[1]) * 10.).clamp(0., 100.);
                for (name, value) in ["swim", "fly", "run", "power", "stamina"]
                    .into_iter()
                    .zip(fruit.raw[2..].iter())
                {
                    delta.insert(name, points(p, name, i16::from(*value) * 10));
                }
                json!({"source":"native-a2ne","fruitId":fruit.id,"fields":["mood","belly","swim","fly","run","power","stamina"],
                    "values":fruit.raw,"units":"signed-native-tiny-fruit-bytes","applicationRules":TINY_FRUIT_RULES,"nativeLevelUpPorted":false})
            } else if item_id == "garden-fruit" {
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
#[cfg(test)]
pub(crate) fn action(game: &mut Game, id: &str, message: &Value) -> Value {
    action_at(
        game,
        id,
        message,
        game.time.max(0.).min(u64::MAX as f64) as u64,
        false,
    )
}
pub(crate) fn action_at(
    game: &mut Game,
    id: &str,
    message: &Value,
    training_now: u64,
    automatic: bool,
) -> Value {
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
    let closes_egg = matches!(request.intent, Intent::TinyEggHatch { .. });
    let pending_egg = p
        .tiny_collection
        .as_ref()
        .is_some_and(|state| state.pending_egg.is_some());
    let reserve = if matches!(
        request.intent,
        Intent::TrainingStart { .. } | Intent::TinyEggPurchase { .. }
    ) {
        2
    } else {
        1
    } + usize::from(pending_egg && !closes_egg);
    if p.care.receipts.len() > MAX_RECEIPTS.saturating_sub(reserve)
        || p.care.revision > u64::MAX - reserve as u64
    {
        return failure(game, id, message, "receipt-capacity");
    }
    let mut next = p.clone();
    let effects = match apply(
        game,
        &mut next,
        &request,
        game.time,
        training_now,
        automatic,
    ) {
        Ok(e) => e,
        Err(code) => return failure(game, id, message, code),
    };
    next.care.revision += 1;
    let revision = next.care.revision;
    let receipt = Receipt {
        request: request.clone(),
        revision,
        effects: effects.clone(),
    };
    next.care
        .history
        .push(HistoryEntry::from_receipt(&receipt, training_now));
    if next.care.history.len() > MAX_HISTORY {
        next.care.history.remove(0);
    }
    next.care
        .receipts
        .insert(request.request_id.clone(), receipt);
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
    let reserved_hatch = u64::from(
        p.tiny_collection
            .as_ref()
            .is_some_and(|state| state.pending_egg.is_some()),
    );
    if !catalog.enabled()
        || p.care.pickups.len() >= MAX_PICKUPS
        || p.care.revision >= u64::MAX - reserved_hatch
    {
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
