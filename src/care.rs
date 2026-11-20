//! Authoritative care intents. Native rows are evidence; application is explicit MMO tuning.
use crate::{
    care_catalog::{CareCatalog, NativeItemStats, PART_CATEGORIES},
    chao_hooks,
    evolution::EvolutionEvidence,
    game::{Game, Player},
    garden::{self, Rand, Tree},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Wire contract label. Fruit, seed, garden and part additions are additive and
/// carry their own labels, so existing v1 clients keep working.
pub const RULES: &str = "adapted-care-v1";
pub const CARE_VERSION: u32 = 2;
/// Native part change when a Chao is given an animal: ALBHV_Capture (GameCube
/// ChaoMain 0x33388). Per part kind: 50% (CG+0x68) take the animal's part when
/// it has one for the Chao's age, else 20% (CG+0x6C) clear that kind.
pub const PART_RULES: &str = "native-gc-albhv-capture-v1";
const PART_TAKE: f32 = 0.5;
const PART_CLEAR: f32 = 0.2;
const HALF_FISH: u8 = 15;
const SKELETON_DOG: u8 = 16;
const BAT: u8 = 17;
const SHEEP: u8 = 13;
const DRAGON: u8 = 18;

/// Server randomness in 0..1 (`rand() / 32768` in the native code).
pub struct OsRand;
impl Rand for OsRand {
    fn next(&mut self) -> f32 {
        let bytes = *Uuid::new_v4().as_bytes();
        f32::from(u16::from_le_bytes([bytes[0], bytes[1]]) & 0x7fff) / 32768.
    }
}
#[cfg(test)]
thread_local! {
    /// Tests script the random sequence; production always uses `OsRand`.
    pub(crate) static SCRIPTED: std::cell::RefCell<Option<garden::Scripted>> =
        const { std::cell::RefCell::new(None) };
}
/// Plants the permanent grove trees the first time a Chao's garden is
/// loaded, so refused actions (which change nothing) never delay them.
pub fn seed_groves(p: &mut Player, now: f64) {
    with_rand(|rand| p.garden.seed_groves(garden::frame_at(now), rand));
}
/// GardenID, set once (native: OSGetTick and chRandomU32 at garden creation).
fn garden_id() -> [u32; 2] {
    let b = *uuid::Uuid::new_v4().as_bytes();
    [
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
    ]
}
fn with_rand<T>(f: impl FnOnce(&mut dyn Rand) -> T) -> T {
    #[cfg(test)]
    if let Some(mut scripted) = SCRIPTED.with(|s| s.borrow_mut().take()) {
        let out = f(&mut scripted);
        SCRIPTED.with(|s| *s.borrow_mut() = Some(scripted));
        return out;
    }
    f(&mut OsRand)
}
pub const MAX_RECEIPTS: usize = 4096;
pub const MAX_QUANTITY: u32 = 99;
pub const MAX_PICKUPS: usize = 100;
const EFFECT_COOLDOWN_MS: f64 = 1500.;
/// The existing hub shop point; this world has no Kindergarten room.
const MARKET_X: f64 = -4.;
const MARKET_Z: f64 = 8.;

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
    /// Entering the Black Market: rebuilds the stock if its timer ran out.
    Market,
    Sell {
        #[serde(rename = "itemId")]
        item_id: String,
    },
    /// Put a carried Black Market egg down in a garden (into the nest).
    #[serde(rename = "place-egg")]
    PlaceEgg {
        #[serde(rename = "itemId")]
        item_id: String,
    },
    /// Put a carried hat down by the Chao for it to pick up.
    #[serde(rename = "give-hat")]
    GiveHat {
        #[serde(rename = "itemId")]
        item_id: String,
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
    /// Native Chao param +0xD3, set by Half Fish (read as the fish-swim flag).
    #[serde(default)]
    pub flag_d3: bool,
    /// Native Chao param +0xD6, set by Bat (read as the bat-flight flag).
    #[serde(default)]
    pub flag_d6: bool,
    /// Native Black Market stock and restock timer.
    #[serde(default)]
    pub market: crate::black_market::Market,
    /// Native emblem record (the Black Market's emblem gates).
    #[serde(default)]
    pub emblems: crate::emblems::EmblemState,
    /// Worn hat id, native Chao param +0xD5 (0 = none).
    #[serde(default)]
    pub hat: Option<u8>,
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
            flag_d3: false,
            flag_d6: false,
            market: crate::black_market::Market::default(),
            emblems: crate::emblems::EmblemState::default(),
            hat: None,
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
        || crate::black_market::egg_id(item_id).is_some()
        || crate::black_market::hat_id(item_id).is_some()
}
fn valid_place(id: &str) -> bool {
    garden::position(id).is_some()
}
fn valid_request(r: &Request) -> bool {
    r.message_type == "care"
        && Uuid::parse_str(&r.request_id)
            .is_ok_and(|id| id.hyphenated().to_string() == r.request_id)
        && match &r.intent {
            Intent::Consume { item_id } => valid_item(item_id),
            Intent::Purchase { item_id } => {
                valid_item(item_id) || crate::black_market::theme_index(item_id).is_some()
            }
            Intent::Pickup { pickup_id } => !pickup_id.is_empty() && pickup_id.len() <= 128,
            Intent::Pet | Intent::Market => true,
            Intent::Sell { item_id } => crate::black_market::prices(item_id).is_some(),
            Intent::PlaceEgg { item_id } => crate::black_market::egg_id(item_id).is_some(),
            Intent::GiveHat { item_id } => crate::black_market::hat_id(item_id).is_some(),
            Intent::Plant { item_id, plot_id } => {
                garden::seed_item(item_id).is_some() && garden::plot(plot_id).is_some()
            }
            Intent::Water { plot_id } => valid_place(plot_id),
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
            || !state.market.valid()
            || !state.emblems.valid()
            || state
                .hat
                .is_some_and(|h| crate::black_market::hat(h).is_none())
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
/// Clears recorded species history.
pub fn reset_species_history(p: &mut Player) {
    p.care.evidence.species_history = 0;
}
/// Rebirth (AL_SucceedGeneParam, traced by the life module): MinimalFlag
/// (species history) becomes 0 and each kept animal part is removed with
/// probability 0.3. The D3/D6 flags at rebirth are not traced, so they stay.
pub fn rebirth(p: &mut Player) {
    reset_species_history(p);
    with_rand(|rand| {
        p.care
            .animal_parts
            .retain(|_, _| rand.next() >= REBIRTH_PART_LOSS);
    });
}
const REBIRTH_PART_LOSS: f32 = 0.3;

/// True when MinimalFlag records all 21 animal species, for body::ChaosInputs.
pub fn all_species_eaten(p: &Player) -> bool {
    let mask = crate::evolution::ALL_SPECIES_MASK;
    p.care.evidence.species_history & mask == mask
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
        || crate::game::training_conflict(p)
        || p.watch.is_some()
        || p.tiny_game.is_some()
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
        "pickups":p.care.pickups.values().collect::<Vec<_>>(),"dropRules":"adapted-mission-drop-v1","catalog":native,
        "animalParts":{"rules":PART_RULES,"categories":PART_CATEGORIES,"shown":p.care.animal_parts,
            "flagD3":p.care.flag_d3,"flagD6":p.care.flag_d6},
        "garden":p.garden.public(),
        "shop":shop(game),"market":market_view(p),"emblems":p.care.emblems.public(),"hat":p.care.hat,
        "eligibility":{"canCare":!conflict(p,false),"canPickup":!conflict(p,true),"activityLock":p.care.activity_lock,
            "effectReadyAt":p.care.last_effect_at.map(|at|at+EFFECT_COOLDOWN_MS),"receiptCapacityRemaining":MAX_RECEIPTS.saturating_sub(p.care.receipts.len())}})
}

/// The Black Market view plus native sell prices for what this profile carries.
fn market_view(p: &Player) -> Value {
    let mut view = p.care.market.public();
    let sell: BTreeMap<&String, u32> = p
        .care
        .inventory
        .keys()
        .filter_map(|id| {
            crate::black_market::prices(id)
                .filter(|(_, s)| *s > 0)
                .map(|(_, s)| (id, s))
        })
        .collect();
    view["sellPrices"] = json!(sell);
    view
}
fn shop(game: &Game) -> Value {
    let native = game.care_catalog.enabled();
    let offers = vec![
        json!({"itemId":"garden-fruit","price":3,"enabled":true,"rules":"adapted"}),
        json!({"itemId":"sa2:22","price":12,"enabled":native,"rules":"adapted"}),
        json!({"itemId":"sa2:23","price":12,"enabled":native,"rules":"adapted"}),
    ];
    // Seeds are not sold: no native Black Market seed price is traced yet.
    Value::Array(offers)
}
fn price(item_id: &str) -> Option<u32> {
    match item_id {
        "garden-fruit" => Some(3),
        "sa2:22" | "sa2:23" => Some(12),
        _ => None,
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

/// Records every raw field of a native row (audit totals only).
fn record_raw(p: &mut Player, raw: &NativeItemStats) {
    for (name, value) in raw.skill_deltas() {
        let total = p.care.raw_totals.entry(name.into()).or_default();
        *total = total.saturating_add(i64::from(value));
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
/// The row's seven skills in `ALL_STATS` order, as native Exp.
fn skill_row(raw: &NativeItemStats) -> [i16; 7] {
    raw.skill_deltas().map(|(_, v)| v)
}

/// Eats one whole native fruit: four bites (ALO_FruitHeldC 0x2B6A4) through
/// the life module, which applies AL_ParameterGrow(kind 3) per bite, hunger,
/// life and LifeMax. The Mood and Belly columns are never read natively.
/// `handed` is true when the owner gave the fruit (like +1, fn_60_2B8FC).
fn eat_fruit(game: &Game, p: &mut Player, id: u8, handed: bool) -> Result<Value, &'static str> {
    let fruit = game
        .care_catalog
        .fruit(id)
        .ok_or("native-data-unavailable")?;
    record_raw(p, &fruit.raw);
    if handed {
        chao_hooks::fruit_handed_over(p);
    }
    let mut effects = chao_hooks::fruit_bites(p, id, skill_row(&fruit.raw));
    effects["raw"] = json!(fruit.raw);
    effects["body"] = json!(chao_hooks::fruit_body(p, id));
    effects["fruitRules"] = json!("native-gc-fruit-grow-param-v1");
    effects["gradesChanged"] = json!(false);
    Ok(effects)
}

/// One shake of a tree. Dropped fruit go into the profile's inventory (native
/// fruit fall to the ground; this server has no loose ground fruit).
fn shake_tree(p: &mut Player, place: &str, rand: &mut dyn Rand) -> Result<(u8, u8), &'static str> {
    let tree = p.garden.tree_mut(place).ok_or("no-tree")?;
    if tree.ripe() == 0 {
        return Err("no-fruit");
    }
    let fruit = tree.fruit;
    Ok((fruit, tree.shake(rand)))
}

/// Native capture part change (ALBHV_Capture 0x33388).
fn capture_parts(
    p: &mut Player,
    native: &crate::care_catalog::NativeItem,
    rand: &mut dyn Rand,
) -> Value {
    let id = native.id;
    let list = if chao_hooks::is_adult(p) {
        &native.adult_parts
    } else {
        &native.child_parts
    };
    let mut changed = BTreeMap::new();
    let mut parts_applied = false;
    if let Some(has) = list {
        parts_applied = true;
        for category in PART_CATEGORIES {
            if has.contains(&category) {
                if rand.next() < PART_TAKE {
                    p.care.animal_parts.insert(category.into(), id);
                    changed.insert(category, Some(id));
                }
            } else if rand.next() < PART_CLEAR {
                if p.care.animal_parts.remove(category).is_some() {
                    changed.insert(category, None);
                }
                if category == "legs" && id != BAT {
                    p.care.flag_d6 = false;
                }
            }
        }
        let horns = p.care.animal_parts.get("horns").copied();
        let ears = p.care.animal_parts.get("ears").copied();
        if matches!(horns, Some(DRAGON | SHEEP)) && ears.is_some_and(|e| Some(e) != horns) {
            p.care.animal_parts.remove("ears");
            changed.insert("ears", None);
        }
    }
    if id != HALF_FISH && rand.next() < PART_CLEAR {
        p.care.flag_d3 = false;
    }
    let history = match id {
        HALF_FISH => {
            if !chao_hooks::is_adult(p) {
                p.care.flag_d3 = true;
            }
            rand.next() < 0.5
        }
        SKELETON_DOG => true,
        BAT => {
            p.care.flag_d6 = true;
            rand.next() < 0.5
        }
        _ => true,
    };
    if history {
        p.care.evidence.record_use(id);
    } else {
        p.care.evidence.animal_uses = p.care.evidence.animal_uses.saturating_add(1);
    }
    json!({"parts":changed,"partsApplied":parts_applied,"historyBit":history,
        "flagD3":p.care.flag_d3,"flagD6":p.care.flag_d6})
}
fn near_place(game: &Game, p: &Player, place: &str) -> bool {
    garden::position(place)
        .is_some_and(|(zone, x, z)| near(game, p, zone, x, z, garden::PLOT_RANGE))
}

fn apply(
    game: &Game,
    p: &mut Player,
    request: &Request,
    now: f64,
    rand: &mut dyn Rand,
) -> Result<Value, &'static str> {
    let catalog = &game.care_catalog;
    if conflict(p, matches!(request.intent, Intent::Pickup { .. })) {
        return Err("activity-conflict");
    }
    p.garden.advance(garden::frame_at(now), rand);
    match &request.intent {
        Intent::PlaceEgg { item_id } => {
            if !["garden", "hero", "dark"].contains(&p.zone.as_str()) {
                return Err("out-of-range");
            }
            let egg = crate::black_market::egg_id(item_id).ok_or("invalid-request")?;
            take_quantity(p, item_id)?;
            let egg_id = crate::life::lay_bought_egg(p, egg, now)?;
            Ok(json!({"inventory":{item_id:-1},"eggId":egg_id,
                "rules":"native-gc-item-egg-createegg-v1"}))
        }
        Intent::GiveHat { item_id } => {
            effect_ready(p, now)?;
            let hat = crate::black_market::hat_id(item_id).ok_or("invalid-request")?;
            if !p.care.inventory.contains_key(item_id) {
                return Err("item-not-owned");
            }
            if p.care.hat.is_some() {
                return Err("already-wearing");
            }
            // fn_60_40DFC: a Chao puts on a loose hat only with MinimalFlag
            // bit 16 (Skeleton Dog) set; otherwise the hat stays carried.
            if p.care.evidence.species_history & (1 << u32::from(SKELETON_DOG)) == 0 {
                return Err("chao-wont-wear");
            }
            take_quantity(p, item_id)?;
            p.care.hat = Some(hat);
            p.care.last_effect_at = Some(now);
            Ok(json!({"inventory":{item_id:-1},"hat":hat,"rules":"native-gc-hat-pickup-v1"}))
        }
        Intent::Purchase { item_id } if crate::black_market::prices(item_id).is_some() => {
            if !catalog.fruits_enabled() {
                return Err("native-data-unavailable");
            }
            if !near(game, p, "hub", MARKET_X, MARKET_Z, 2.) {
                return Err("out-of-range");
            }
            let index = p
                .care
                .market
                .stock
                .iter()
                .position(|i| i == item_id)
                .ok_or("not-in-stock")?;
            let (buy, _) = crate::black_market::prices(item_id).ok_or("not-for-sale")?;
            if p.rings < buy {
                return Err("insufficient-rings");
            }
            if let Some(theme) = crate::black_market::theme_index(item_id) {
                // A bought theme sets its owned flag; it never goes to the carry list.
                p.care.market.themes[theme] = true;
            } else {
                add_quantity(p, item_id)?;
            }
            p.rings -= buy;
            if crate::black_market::removed_when_bought(item_id) {
                p.care.market.stock.remove(index);
            }
            Ok(json!({"rings":-(i64::from(buy)),"inventory":{item_id:1},
                "rules":crate::black_market::MARKET_RULES}))
        }
        Intent::Market => {
            if !catalog.fruits_enabled() {
                return Err("native-data-unavailable");
            }
            if !near(game, p, "hub", MARKET_X, MARKET_Z, 2.) {
                return Err("out-of-range");
            }
            let frame = garden::frame_at(now);
            let restocked = p.care.market.due(frame);
            if p.care.market.garden_id.is_none() {
                p.care.market.garden_id = Some(garden_id());
            }
            if restocked {
                let emblems = p.care.emblems.count();
                p.care.market.restock(frame, emblems, rand);
            }
            Ok(json!({"restocked":restocked,"market":p.care.market.public()}))
        }
        Intent::Sell { item_id } => {
            if !catalog.fruits_enabled() {
                return Err("native-data-unavailable");
            }
            if !near(game, p, "hub", MARKET_X, MARKET_Z, 2.) {
                return Err("out-of-range");
            }
            let (_, sell) = crate::black_market::prices(item_id).ok_or("not-for-sale")?;
            if sell == 0 {
                return Err("not-sellable");
            }
            take_quantity(p, item_id)?;
            let before = p.rings;
            p.rings = p.rings.saturating_add(sell).min(999_999);
            Ok(
                json!({"rings":i64::from(p.rings) - i64::from(before),"inventory":{item_id:-1},
                "rules":crate::black_market::MARKET_RULES}),
            )
        }
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
            Ok(
                json!({"inventory":{item_id:1},"pickupId":pickup_id,"dropRules":"adapted-mission-drop-v1"}),
            )
        }
        Intent::Plant { item_id, plot_id } => {
            if !catalog.fruits_enabled() {
                return Err("native-data-unavailable");
            }
            if !near_place(game, p, plot_id) {
                return Err("out-of-range");
            }
            if p.garden.trees.contains_key(plot_id) {
                return Err("plot-occupied");
            }
            let (family, kind) = garden::seed_item(item_id).ok_or("unknown-item")?;
            take_quantity(p, item_id)?;
            p.garden
                .trees
                .insert(plot_id.clone(), Tree::planted(kind, garden::frame_at(now)));
            Ok(
                json!({"inventory":{item_id:-1},"plotId":plot_id,"family":family,"kind":kind,
                "stage":"seed","rules":garden::TREE_RULES}),
            )
        }
        Intent::Water { plot_id } => {
            if !near_place(game, p, plot_id) {
                return Err("out-of-range");
            }
            let tree = p.garden.tree_mut(plot_id).ok_or("no-tree")?;
            if !tree.water() {
                return Err("not-growing");
            }
            Ok(json!({"plotId":plot_id,"tree":tree.public(),"rules":garden::TREE_RULES}))
        }
        Intent::Harvest { plot_id } => {
            if !catalog.fruits_enabled() {
                return Err("native-data-unavailable");
            }
            if !near_place(game, p, plot_id) {
                return Err("out-of-range");
            }
            // `apply` works on a clone, so a full inventory discards the shake too.
            let (fruit, dropped) = shake_tree(p, plot_id, rand)?;
            let item_id = format!("sa2-fruit:{fruit}");
            for _ in 0..dropped {
                add_quantity(p, &item_id)?;
            }
            Ok(
                json!({"inventory":{item_id:dropped},"plotId":plot_id,"shaken":true,
                "dropped":dropped,"rules":garden::TREE_RULES}),
            )
        }
        Intent::Pet => {
            effect_ready(p, now)?;
            let before = p.happiness;
            p.happiness = (before + 2.).min(100.);
            let like = p.life.like;
            chao_hooks::pet(p);
            p.care.last_effect_at = Some(now);
            Ok(json!({"happiness":p.happiness-before,"like":p.life.like-like}))
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
                let mut effects = eat_fruit(game, p, fruit, true)?;
                effects["inventory"] = json!({item_id:-1});
                p.care.last_effect_at = Some(now);
                return Ok(effects);
            }
            let mut extra = json!({});
            let mut grown = json!({"levelsChanged":false});
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
                // AL_ParameterGrow(kind 2) adds the whole MinimalGrowParam row;
                // its Mood/Belly columns are never read natively.
                record_raw(p, &native.raw);
                grown = chao_hooks::capture_stats(p, skill_row(&native.raw));
                let body = chao_hooks::capture_body(p, native.id);
                extra = if native.kind == crate::care_catalog::NativeItemKind::Animal {
                    let mut capture = capture_parts(p, native, rand);
                    capture["body"] = json!(body);
                    capture["partRules"] = json!(PART_RULES);
                    capture
                } else {
                    p.care.evidence.record_use(native.id);
                    json!({"body":body})
                };
                json!(native.raw)
            };
            p.care.last_effect_at = Some(now);
            let mut effects = json!({"inventory":{item_id:-1},"points":delta,"energy":p.energy-energy,"satiety":p.hunger-hunger,
                "happiness":p.happiness-happiness,"raw":raw,"unknown18Applied":false,"gradesChanged":false});
            for extra in [grown, extra] {
                if let (Some(effects), Some(extra)) = (effects.as_object_mut(), extra.as_object()) {
                    effects.extend(extra.clone());
                }
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
    let effects = match with_rand(|rand| apply(game, &mut next, &request, game.time, rand)) {
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

/// Chao behaviour hook: the Chao shakes a nearby tree once and, if a fruit
/// drops, eats one; any other dropped fruit go to inventory. One server mutation,
/// no client receipt; the care revision advances. Errors leave the profile unchanged.
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
    let effects = with_rand(|rand| -> Result<Value, &'static str> {
        next.garden.advance(garden::frame_at(now), rand);
        let (fruit, dropped) = shake_tree(&mut next, place, rand)?;
        let item_id = format!("sa2-fruit:{fruit}");
        for _ in 1..dropped {
            add_quantity(&mut next, &item_id)?;
        }
        let mut effects = if dropped > 0 {
            eat_fruit(game, &mut next, fruit, false)?
        } else {
            json!({})
        };
        effects["plotId"] = json!(place);
        effects["fruitId"] = json!(item_id);
        effects["dropped"] = json!(dropped);
        effects["eaten"] = json!(dropped > 0);
        Ok(effects)
    })?;
    next.care.last_effect_at = Some(now);
    next.care.revision += 1;
    game.players.insert(id.into(), next);
    game.dirty = true;
    Ok(effects)
}

/// Like `conflict`, but a resident (offline, Watch-owned) Chao may still eat.
fn conflict_resident(p: &Player) -> bool {
    crate::game::training_conflict(p)
        || p.tiny_game.is_some()
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
