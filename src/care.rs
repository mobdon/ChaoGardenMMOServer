//! Authoritative care intents. Native rows are evidence; application is explicit MMO tuning.
use crate::{
    care_catalog::{CareCatalog, NativeItemStats, PART_CATEGORIES},
    chao_hooks,
    evolution::EvolutionEvidence,
    game::{Game, Player},
    garden::{self, Stage, Tree},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Wire contract label. Fruit, seed, garden and part additions are additive and
/// carry their own labels, so existing v1 clients keep working.
pub const RULES: &str = "adapted-care-v1";
pub const CARE_VERSION: u32 = 2;
/// Drop-table selection for defeated mission robots (MMO tuning, not a retail table).
pub const DROP_RULES: &str = "adapted-mission-drop-v2";
/// Body-axis pull per item: (power - run) / 400 and (fly - swim) / 400 of the
/// native row, in AL_BODY_INFO -1..1 units. MMO tuning; the native magnitude is unknown.
pub const SHAPE_RULES: &str = "adapted-row-difference-per-400-v1";
const SHAPE_DIVISOR: f32 = 400.;
/// Hero and Dark Fruit pull hero/dark by this much (MMO tuning).
const FRUIT_ALIGNMENT: f32 = 0.05;
/// Every verified part category an animal provides replaces the current one (MMO tuning).
pub const PART_RULES: &str = "adapted-replace-every-verified-category-v1";
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
    Plant {
        #[serde(rename = "itemId")]
        item_id: String,
        #[serde(rename = "plotId")]
        plot_id: String,
    },
    Water {
        #[serde(rename = "plotId")]
        plot_id: String,
    },
    Harvest {
        #[serde(rename = "plotId")]
        plot_id: String,
    },
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
    /// Native `MinimalParts` mirror: part category -> native species ID shown there.
    #[serde(default)]
    pub animal_parts: BTreeMap<String, u8>,
}

impl Default for CareState {
    fn default() -> Self {
        Self {
            version: CARE_VERSION,
            revision: 0,
            inventory: BTreeMap::new(),
            evidence: EvolutionEvidence::default(),
            raw_totals: BTreeMap::new(),
            pickups: BTreeMap::new(),
            receipts: BTreeMap::new(),
            last_effect_at: None,
            activity_lock: None,
            animal_parts: BTreeMap::new(),
        }
    }
}

fn native_id(item_id: &str) -> Option<u8> {
    let id = item_id.strip_prefix("sa2:")?.parse::<u8>().ok()?;
    (id < 25 && item_id == format!("sa2:{id}")).then_some(id)
}
fn fruit_id(item_id: &str) -> Option<u8> {
    let id = item_id.strip_prefix("sa2-fruit:")?.parse::<u8>().ok()?;
    (id < crate::care_catalog::FRUIT_COUNT && item_id == format!("sa2-fruit:{id}")).then_some(id)
}
fn valid_item(item_id: &str) -> bool {
    item_id == "garden-fruit"
        || native_id(item_id).is_some()
        || fruit_id(item_id).is_some()
        || garden::seed_item(item_id).is_some()
}
fn valid_place(id: &str) -> bool {
    garden::position(id).is_some()
}
fn valid_request(r: &Request) -> bool {
    r.message_type == "care"
        && Uuid::parse_str(&r.request_id)
            .is_ok_and(|id| id.hyphenated().to_string() == r.request_id)
        && match &r.intent {
            Intent::Consume { item_id } | Intent::Purchase { item_id } => valid_item(item_id),
            Intent::Pickup { pickup_id } => !pickup_id.is_empty() && pickup_id.len() <= 128,
            Intent::Pet => true,
            Intent::Plant { item_id, plot_id } => {
                garden::seed_item(item_id).is_some() && garden::plot(plot_id).is_some()
            }
            Intent::Water { plot_id } => garden::plot(plot_id).is_some(),
            Intent::Harvest { plot_id } => valid_place(plot_id),
        }
}

impl CareState {
    /// Only an absent legacy field migrates. Malformed/newer data must stop loading.
    pub fn restore(value: Option<&Value>) -> Result<Self, String> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let mut state: Self =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid saved care state")?;
        if !(1..=CARE_VERSION).contains(&state.version)
            || (state.version == 1 && !state.animal_parts.is_empty())
            || state.animal_parts.len() > PART_CATEGORIES.len()
            || state
                .animal_parts
                .iter()
                .any(|(k, id)| !PART_CATEGORIES.contains(&k.as_str()) || *id > 20)
            || !state.evidence.valid()
            || state.inventory.len() > 64
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
        // Version 1 had no fruit, seed, garden or part state; nothing else changes.
        state.version = CARE_VERSION;
        Ok(state)
    }
}

/// Clears every shown animal part. For the life cycle's reincarnation step.
pub fn reset_animal_parts(p: &mut Player) {
    p.care.animal_parts.clear();
}
/// Clears recorded species history. For the life cycle's reincarnation step;
/// the native reset timing is still unknown.
pub fn reset_species_history(p: &mut Player) {
    p.care.evidence.species_history = 0;
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
        || crate::competition::training_conflict(p)
        || crate::competition::reserved(p)
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
        "pickups":p.care.pickups.values().collect::<Vec<_>>(),"dropRules":DROP_RULES,"catalog":native,
        "animalParts":{"rules":PART_RULES,"categories":PART_CATEGORIES,"shown":p.care.animal_parts},
        "shapeRules":SHAPE_RULES,"garden":p.garden.public(game.time),
        "shop":shop(game),
        "eligibility":{"canCare":!conflict(p,false),"canPickup":!conflict(p,true),"activityLock":p.care.activity_lock,
            "effectReadyAt":p.care.last_effect_at.map(|at|at+EFFECT_COOLDOWN_MS),"receiptCapacityRemaining":MAX_RECEIPTS.saturating_sub(p.care.receipts.len())}})
}

fn shop(game: &Game) -> Value {
    let native = game.care_catalog.enabled();
    let mut offers = vec![
        json!({"itemId":"garden-fruit","price":3,"enabled":true,"rules":"adapted"}),
        json!({"itemId":"sa2:22","price":12,"enabled":native,"rules":"adapted"}),
        json!({"itemId":"sa2:23","price":12,"enabled":native,"rules":"adapted"}),
    ];
    let fruits = game.care_catalog.fruits_enabled();
    offers.extend(garden::SEEDS.iter().map(|(family, _, price)| {
        json!({"itemId":format!("seed:{family}"),"price":price,"enabled":fruits,"rules":garden::TREE_RULES})
    }));
    Value::Array(offers)
}
fn price(item_id: &str) -> Option<u32> {
    match item_id {
        "garden-fruit" => Some(3),
        "sa2:22" | "sa2:23" => Some(12),
        _ => garden::seed_item(item_id).map(|(_, _, price)| price),
    }
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
    chao_hooks::grant_stat(p, name, delta)
}

/// Applies one native row's seven skills and records every raw field.
fn apply_raw(p: &mut Player, raw: &NativeItemStats, delta: &mut BTreeMap<&'static str, i64>) {
    for (name, value) in raw.skill_deltas() {
        let total = p.care.raw_totals.entry(name.into()).or_default();
        *total = total.saturating_add(i64::from(value));
        delta.insert(name, points(p, name, value));
    }
    for (name, value) in [
        ("mood", raw.mood),
        ("belly", raw.belly),
        ("unknown18", raw.unknown_18),
    ] {
        let total = p.care.raw_totals.entry(name.into()).or_default();
        *total = total.saturating_add(i64::from(value));
    }
}

fn shape_from_row(raw: &NativeItemStats) -> (f32, f32) {
    (
        (f32::from(raw.power) - f32::from(raw.run)) / SHAPE_DIVISOR,
        (f32::from(raw.fly) - f32::from(raw.swim)) / SHAPE_DIVISOR,
    )
}

/// Eats one native fruit already removed from inventory or a tree.
fn eat_fruit(game: &Game, p: &mut Player, id: u8) -> Result<Value, &'static str> {
    let fruit = game
        .care_catalog
        .fruit(id)
        .ok_or("native-data-unavailable")?;
    let (hunger, happiness) = (p.hunger, p.happiness);
    let mut delta = BTreeMap::new();
    apply_raw(p, &fruit.raw, &mut delta);
    chao_hooks::feed(p, Some(id), fruit.raw.mood, fruit.raw.belly);
    let hero_dark = match fruit.key {
        "hero" => FRUIT_ALIGNMENT,
        "dark" => -FRUIT_ALIGNMENT,
        _ => 0.,
    };
    let alignment = chao_hooks::shape(p, 0., 0., hero_dark);
    Ok(
        json!({"points":delta,"satiety":p.hunger-hunger,"happiness":p.happiness-happiness,
        "raw":fruit.raw,"alignment":alignment,"fruitRules":"native-row-adapted-mapping-v1",
        "specialEffect":"unknown-not-applied","unknown18Applied":false,"levelsChanged":false,"gradesChanged":false}),
    )
}

/// Takes one fruit hanging on a plot tree or garden grove. Proximity is checked by the caller.
fn pick_fruit(p: &mut Player, place: &str, now: f64) -> Result<u8, &'static str> {
    if let Some((_, _, _, _, fruit)) = garden::grove(place) {
        let grove = p.garden.grove_mut(place, now);
        if grove.fruit_ready == 0 {
            return Err("no-fruit");
        }
        grove.fruit_ready -= 1;
        grove.harvested = grove.harvested.saturating_add(1);
        return Ok(fruit);
    }
    let tree = p.garden.trees.get_mut(place).ok_or("no-tree")?;
    tree.advance(now);
    if tree.fruit_ready == 0 {
        return Err("no-fruit");
    }
    let fruit = tree.fruit_id().ok_or("no-tree")?;
    tree.fruit_ready -= 1;
    tree.harvested = tree.harvested.saturating_add(1);
    Ok(fruit)
}
fn near_place(game: &Game, p: &Player, place: &str) -> bool {
    garden::position(place)
        .is_some_and(|(zone, x, z)| near(game, p, zone, x, z, garden::PLOT_RANGE))
}

fn apply(game: &Game, p: &mut Player, request: &Request, now: f64) -> Result<Value, &'static str> {
    let catalog = &game.care_catalog;
    if conflict(p, matches!(request.intent, Intent::Pickup { .. })) {
        return Err("activity-conflict");
    }
    match &request.intent {
        Intent::Purchase { item_id } => {
            let price = price(item_id).ok_or("not-for-sale")?;
            if native_id(item_id).is_some_and(|id| catalog.item(id).is_none())
                || (garden::seed_item(item_id).is_some() && !catalog.fruits_enabled())
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
            Ok(json!({"inventory":{item_id:1},"pickupId":pickup_id,"dropRules":DROP_RULES}))
        }
        Intent::Plant { item_id, plot_id } => {
            if !catalog.fruits_enabled() {
                return Err("native-data-unavailable");
            }
            if !near_place(game, p, plot_id) {
                return Err("out-of-range");
            }
            if p.garden
                .trees
                .get(plot_id)
                .is_some_and(|t| t.stage(now) != Stage::Dead)
            {
                return Err("plot-occupied");
            }
            let (family, _, _) = garden::seed_item(item_id).ok_or("unknown-item")?;
            take_quantity(p, item_id)?;
            p.garden
                .trees
                .insert(plot_id.clone(), Tree::new(family, now));
            Ok(
                json!({"inventory":{item_id:-1},"plotId":plot_id,"family":family,"stage":"seed","rules":garden::TREE_RULES}),
            )
        }
        Intent::Water { plot_id } => {
            if !near_place(game, p, plot_id) {
                return Err("out-of-range");
            }
            let tree = p.garden.trees.get_mut(plot_id).ok_or("no-tree")?;
            if !matches!(tree.stage(now), Stage::Seed | Stage::Young) {
                return Err("not-growing");
            }
            if tree
                .last_watered_at
                .is_some_and(|at| now < at + garden::WATER_COOLDOWN_MS)
            {
                return Err("cooldown");
            }
            let bonus = garden::WATER_BONUS_MS.min(garden::MATURE_MS - tree.water_bonus);
            // Re-anchor fruit accounting so watering never back-dates fruit.
            tree.fruit_clock_at = tree.fruit_clock_at.max(now);
            tree.water_bonus += bonus;
            tree.last_watered_at = Some(now);
            Ok(
                json!({"plotId":plot_id,"growthMs":bonus,"stage":tree.stage(now).name(),"rules":garden::TREE_RULES}),
            )
        }
        Intent::Harvest { plot_id } => {
            if !catalog.fruits_enabled() {
                return Err("native-data-unavailable");
            }
            if !near_place(game, p, plot_id) {
                return Err("out-of-range");
            }
            // `apply` works on a clone, so a full inventory discards the pick too.
            let fruit = pick_fruit(p, plot_id, now)?;
            let item_id = format!("sa2-fruit:{fruit}");
            add_quantity(p, &item_id)?;
            Ok(json!({"inventory":{item_id:1},"plotId":plot_id,"rules":garden::TREE_RULES}))
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
            if let Some(fruit) = fruit_id(item_id) {
                if catalog.fruit(fruit).is_none() {
                    return Err("native-data-unavailable");
                }
                take_quantity(p, item_id)?;
                let mut effects = eat_fruit(game, p, fruit)?;
                effects["inventory"] = json!({item_id:-1});
                effects["energy"] = json!(0.);
                p.care.last_effect_at = Some(now);
                return Ok(effects);
            }
            let mut extra = json!({});
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
                apply_raw(p, &native.raw, &mut delta);
                chao_hooks::feed(p, None, native.raw.mood, native.raw.belly);
                let (run_power, swim_fly) = shape_from_row(&native.raw);
                chao_hooks::shape(p, run_power, swim_fly, 0.);
                let mut parts = BTreeMap::new();
                for category in &native.part_categories {
                    p.care.animal_parts.insert((*category).into(), native.id);
                    parts.insert(*category, native.id);
                }
                extra = json!({"shape":{"runPower":run_power,"swimFly":swim_fly,"heroDark":0.,"rules":SHAPE_RULES},
                    "parts":parts,"partRules":PART_RULES});
                p.care.evidence.record_use(native.id);
                json!(native.raw)
            };
            p.care.last_effect_at = Some(now);
            let mut effects = json!({"inventory":{item_id:-1},"points":delta,"energy":p.energy-energy,"satiety":p.hunger-hunger,
                "happiness":p.happiness-happiness,"raw":raw,"unknown18Applied":false,"levelsChanged":false,"gradesChanged":false});
            if let (Some(effects), Some(extra)) = (effects.as_object_mut(), extra.as_object()) {
                effects.extend(extra.clone());
            }
            Ok(effects)
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

/// Weighted mission drop table (MMO tuning; native stage tables are unknown).
/// Common animals 0-14 weigh 40 each, rare animals 15-20 weigh 4 each and each
/// Chaos Drive weighs 30: total 744. Rare species stay findable but uncommon.
pub const DROP_WEIGHTS: [u16; 25] = [
    40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 4, 4, 4, 4, 4, 4, 30, 30, 30, 30,
];
pub fn drop_choice(roll: u16) -> u8 {
    let total: u32 = DROP_WEIGHTS.iter().map(|w| u32::from(*w)).sum();
    let mut pick = u32::from(roll) % total;
    for (id, weight) in DROP_WEIGHTS.iter().enumerate() {
        if pick < u32::from(*weight) {
            return id as u8;
        }
        pick -= u32::from(*weight);
    }
    unreachable!("weights cover the roll")
}

/// Chao behaviour hook: take one hanging fruit from a nearby tree or grove and
/// eat it in a single server mutation. No client request or receipt is involved;
/// the care revision advances so clients refresh. Errors leave the profile unchanged.
pub fn eat_garden_fruit(game: &mut Game, id: &str, place: &str) -> Result<Value, &'static str> {
    let p = game.players.get(id).ok_or("profile-unavailable")?;
    if conflict_resident(p) {
        return Err("activity-conflict");
    }
    if !game.care_catalog.fruits_enabled() {
        return Err("native-data-unavailable");
    }
    if !near_place(game, p, place) {
        return Err("out-of-range");
    }
    let now = game.time;
    let mut next = p.clone();
    effect_ready(&next, now)?;
    if next.care.revision == u64::MAX {
        return Err("receipt-capacity");
    }
    let fruit = pick_fruit(&mut next, place, now)?;
    let mut effects = eat_fruit(game, &mut next, fruit)?;
    effects["plotId"] = json!(place);
    effects["fruitId"] = json!(format!("sa2-fruit:{fruit}"));
    next.care.last_effect_at = Some(now);
    next.care.revision += 1;
    game.players.insert(id.into(), next);
    game.dirty = true;
    Ok(effects)
}

/// Like `conflict`, but a resident (offline, Watch-owned) Chao may still eat.
fn conflict_resident(p: &Player) -> bool {
    crate::competition::training_conflict(p)
        || crate::competition::reserved(p)
        || p.tiny_game.is_some()
        || p.race.is_some()
        || p.karate.is_some()
        || p.vehicle.is_some()
        || !["garden", "hero", "dark"].contains(&p.zone.as_str())
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
    let digest = Sha256::digest(id.as_bytes());
    let native_id = drop_choice(u16::from_be_bytes([digest[0], digest[1]]));
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
