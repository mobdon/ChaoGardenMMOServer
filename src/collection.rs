//! Additive owned hatchlings. The legacy active Chao and credentials stay in
//! Player; newborn native records are separate, typed, inactive owned records.
use crate::{
    care::{Intent, MAX_RECEIPTS},
    game::{Game, Player},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use tiny_garden_core::{
    native_growth::{NativeGrowth, NativeRng, SKILL_NAMES},
    native_hatching::{
        NativeHatchContext, NativeHatchSource, RECORD_LEN, hatch_into_empty_native_slot,
    },
    native_shop_state::{NativeShopFields, NativeShopPrices, NativeShopState},
};
use uuid::Uuid;

pub const RULES: &str = "adapted-tiny-owned-eggs-v1";
pub const ROTATION_MS: u64 = 3_600_000;
pub const MAX_CHAO: usize = 8;

#[derive(Clone, Debug)]
pub struct EggSource {
    pub(crate) prices: NativeShopPrices,
    pub(crate) wave: [i16; 256],
    hatch: NativeHatchSource,
}
impl EggSource {
    pub fn from_verified_payload(payload: &[u8]) -> Result<Self, String> {
        Ok(Self {
            prices: NativeShopPrices::from_verified_payload(payload)
                .map_err(|error| error.to_string())?,
            wave: std::array::from_fn(|i| {
                i16::from_le_bytes(
                    payload[0x1fb74 + 2 * i..0x1fb76 + 2 * i]
                        .try_into()
                        .unwrap(),
                )
            }),
            hatch: NativeHatchSource::from_verified_payload(payload)
                .map_err(|error| error.to_string())?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnedEgg {
    pub id: String,
    pub native_egg_id: u8,
    pub purchased_at: u64,
    pub stock_epoch: u64,
    pub price: u32,
    purchase_receipt_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnedChao {
    pub id: String,
    pub name: String,
    pub origin: OwnedEgg,
    pub hatched_at: u64,
    hatch_receipt_id: String,
    /// Source constructor output remains private/local, never a public asset.
    native_record: Vec<u8>,
    native_record_sha256: String,
    native_name_index: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionState {
    version: u8,
    owner_id: String,
    stock_seed: u32,
    hatch_rng: u32,
    last_server_ms: u64,
    pub pending_egg: Option<OwnedEgg>,
    pub chao: Vec<OwnedChao>,
}

fn uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| id.hyphenated().to_string() == value)
}
fn digest(record: &[u8]) -> String {
    format!("{:x}", Sha256::digest(record))
}
fn initial_stock_seed(player: &Player) -> u32 {
    let hash =
        Sha256::digest(format!("tiny-stock-v1:{}:{}", player.id, player.token_hash).as_bytes());
    u32::from_le_bytes(hash[..4].try_into().unwrap())
}
fn stock_seed(player: &Player, epoch: u64) -> u32 {
    let seed = player
        .tiny_collection
        .as_ref()
        .map_or_else(|| initial_stock_seed(player), |state| state.stock_seed);
    let hash = Sha256::digest(format!("tiny-hour-v1:{seed}:{epoch}").as_bytes());
    u32::from_le_bytes(hash[..4].try_into().unwrap())
}
fn offer(game: &Game, player: &Player, now: u64) -> Result<(u8, u32), &'static str> {
    let source = game
        .tiny_egg_source
        .as_ref()
        .ok_or("native-data-unavailable")?;
    let epoch = now / ROTATION_MS;
    let fields = NativeShopFields {
        chao_field_58: 0,
        ..NativeShopFields::default()
    };
    let mut state =
        NativeShopState::restore(fields, player.rings).map_err(|_| "native-stock-unavailable")?;
    let mut rng = NativeRng::new(stock_seed(player, epoch));
    // Hourly reopening/seed and the caller clock are explicit host adaptations.
    // Selection weighting and original price still use the native Rust ports.
    let native_frame = (epoch % (i32::MAX as u64)) as u32;
    let id = state
        .open_stock(native_frame, 0, &mut rng)
        .map_err(|_| "native-stock-unavailable")?
        .egg
        .ok_or("native-stock-unavailable")?;
    Ok((id, source.prices.egg[usize::from(id)]))
}
fn egg_public(egg: &OwnedEgg) -> Value {
    json!({"id":egg.id,"nativeEggId":egg.native_egg_id,"purchasedAt":egg.purchased_at,"stockEpoch":egg.stock_epoch,"price":egg.price})
}
fn newborn_public(chao: &OwnedChao) -> Value {
    let growth =
        NativeGrowth::read_raw_fields(&chao.native_record).expect("validated owned native record");
    let stats:serde_json::Map<String,Value>=SKILL_NAMES.into_iter().zip(growth.skills).map(|(name,skill)|
        (name.into(),json!({"grade":skill.grade,"level":skill.level,"points":skill.points,"experience":skill.experience}))).collect();
    json!({"id":chao.id,"name":chao.name,"active":false,"nativeEggId":chao.origin.native_egg_id,
        "hatchedAt":chao.hatched_at,"stats":stats,"nativeNeeds":{"mood":growth.mood,"belly":growth.belly,"units":"native-0-to-200"},
        "source":"native-a2ne-initializer","playable":false,"appearanceMapping":"unverified"})
}
fn purchase_effect(egg: &OwnedEgg) -> Value {
    json!({"rings":-i64::from(egg.price),"egg":egg_public(egg),"rulesVersion":RULES})
}
fn hatch_effect(chao: &OwnedChao) -> Value {
    json!({"hatchedEggId":chao.origin.id,"chao":newborn_public(chao),"rulesVersion":RULES})
}

impl CollectionState {
    pub fn restore(value: Option<&Value>) -> Result<Option<Self>, String> {
        let Some(value) = value.filter(|value| !value.is_null()) else {
            return Ok(None);
        };
        let state: Self =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid owned Tiny collection")?;
        if state.version != 1
            || state.owner_id.is_empty()
            || state.owner_id.len() > 128
            || state.chao.len() >= MAX_CHAO
        {
            return Err("Invalid owned Tiny collection capacity or version".into());
        }
        Ok(Some(state))
    }
}
fn validate_egg(player: &Player, egg: &OwnedEgg, last_server_ms: u64) -> Result<(), String> {
    if !uuid(&egg.id)
        || egg.id == player.id
        || egg.native_egg_id >= 12
        || !uuid(&egg.purchase_receipt_id)
        || egg.purchased_at > last_server_ms
        || egg.stock_epoch != egg.purchased_at / ROTATION_MS
    {
        return Err("Invalid owned Tiny egg".into());
    }
    let receipt = player
        .care
        .receipts
        .get(&egg.purchase_receipt_id)
        .ok_or("Owned egg has no durable purchase receipt")?;
    if !matches!(&receipt.request.intent,Intent::TinyEggPurchase{egg_id,stock_epoch} if *egg_id==egg.native_egg_id && *stock_epoch==egg.stock_epoch)
        || receipt.effects != purchase_effect(egg)
    {
        return Err("Owned egg does not match its purchase receipt".into());
    }
    Ok(())
}
pub fn validate_player(player: &Player) -> Result<(), String> {
    let Some(state) = &player.tiny_collection else {
        return Ok(());
    };
    if state.owner_id != player.id
        || state.chao.len() + 1 + usize::from(state.pending_egg.is_some()) > MAX_CHAO
    {
        return Err("Tiny collection owner or capacity mismatch".into());
    }
    let mut ids = BTreeSet::from([player.id.clone()]);
    if let Some(egg) = &state.pending_egg {
        if player.care.receipts.len() >= MAX_RECEIPTS || player.care.revision == u64::MAX {
            return Err("Owned egg lost its reserved hatch capacity".into());
        }
        validate_egg(player, egg, state.last_server_ms)?;
        if !ids.insert(egg.id.clone()) {
            return Err("Duplicate owned Tiny identity".into());
        }
    }
    for chao in &state.chao {
        validate_egg(player, &chao.origin, state.last_server_ms)?;
        if !uuid(&chao.id)
            || !ids.insert(chao.id.clone())
            || !ids.insert(chao.origin.id.clone())
            || chao.name.is_empty()
            || chao.name.len() > 40
            || chao.name.chars().any(char::is_control)
            || !uuid(&chao.hatch_receipt_id)
            || chao.hatched_at < chao.origin.purchased_at
            || chao.hatched_at > state.last_server_ms
            || chao.native_record.len() != RECORD_LEN
            || digest(&chao.native_record) != chao.native_record_sha256
            || chao.native_name_index >= 10
            || chao.native_record[0x58] != 0
            || chao.native_record[0x59] != 0xff
            || chao.native_record[0x72] != chao.origin.native_egg_id
        {
            return Err("Invalid owned Tiny hatchling".into());
        }
        let growth = NativeGrowth::read_raw_fields(&chao.native_record)
            .map_err(|_| "Invalid owned native growth")?;
        if growth.mood > 200
            || growth.belly > 200
            || growth.skills.iter().any(|skill| {
                skill.grade > 5 || skill.level != 1 || skill.points != 0 || skill.experience != 0
            })
        {
            return Err("Invalid immutable newborn native fields".into());
        }
        let receipt = player
            .care
            .receipts
            .get(&chao.hatch_receipt_id)
            .ok_or("Owned Chao has no durable hatch receipt")?;
        if !matches!(&receipt.request.intent,Intent::TinyEggHatch{owned_egg_id} if *owned_egg_id==chao.origin.id)
            || receipt.effects != hatch_effect(chao)
        {
            return Err("Owned Chao does not match its hatch receipt".into());
        }
    }
    Ok(())
}

fn activity_block(player: &Player) -> Option<&'static str> {
    if player.connections == 0
        || player.tiny_game.is_some()
        || player.race.is_some()
        || player.karate.is_some()
        || player.vehicle.is_some()
        || player.care.activity_lock.is_some()
        || !["hub", "garden", "hero", "dark"].contains(&player.zone.as_str())
    {
        Some("activity-conflict")
    } else if !player.care.tiny_resident {
        Some("tiny-entry-required")
    } else if player.care.receipts.len() >= MAX_RECEIPTS || player.care.revision == u64::MAX {
        Some("receipt-capacity")
    } else {
        None
    }
}
fn clock_block(player: &Player, now: u64) -> bool {
    player
        .tiny_collection
        .as_ref()
        .is_some_and(|state| now < state.last_server_ms)
}
fn purchase_block(game: &Game, player: &Player, now: u64) -> Option<&'static str> {
    if game.tiny_egg_source.is_none() {
        return Some("native-data-unavailable");
    }
    if let Some(reason) = activity_block(player) {
        return Some(reason);
    }
    if clock_block(player, now) {
        return Some("collection-clock-error");
    }
    if player.care.receipts.len() > MAX_RECEIPTS - 2 || player.care.revision > u64::MAX - 2 {
        return Some("receipt-capacity");
    }
    let count = player
        .tiny_collection
        .as_ref()
        .map_or(1, |state| state.chao.len() + 1);
    if player
        .tiny_collection
        .as_ref()
        .is_some_and(|state| state.pending_egg.is_some())
    {
        return Some("egg-slot-full");
    }
    if count >= MAX_CHAO {
        return Some("collection-full");
    }
    match offer(game, player, now) {
        Ok((_, price)) if player.rings < price => Some("insufficient-rings"),
        Ok(_) => None,
        Err(reason) => Some(reason),
    }
}
pub fn shop_public(game: &Game, player: &Player, now: u64) -> Value {
    let epoch = now / ROTATION_MS;
    let offers=offer(game,player,now).map(|(egg,price)|vec![json!({"eggId":egg,"name":format!("Native Egg {egg}"),"price":price,"source":"native-a2ne"})]).unwrap_or_default();
    let reason = purchase_block(game, player, now);
    json!({"enabled":game.tiny_egg_source.is_some(),"rulesVersion":RULES,"epoch":epoch,
        "serverNowMs":now,"rotatesAt":epoch.saturating_add(1).saturating_mul(ROTATION_MS),"rotationMs":ROTATION_MS,
        "maxHeldEggs":1,"offers":offers,"eligibility":{"canPurchase":reason.is_none(),"reason":reason}})
}
pub fn public(game: &Game, player: &Player, now: u64) -> Value {
    let mut chao = vec![
        json!({"id":player.id,"name":player.name,"active":true,"stats":player.stats,
        "needs":{"happiness":player.happiness,"energy":player.energy,"satiety":player.hunger,"units":"adapted-0-to-100"},
        "form":player.form,"alignment":player.alignment,"gear":player.gear,"source":"canonical-legacy-chao","playable":true}),
    ];
    let pending = player
        .tiny_collection
        .as_ref()
        .and_then(|state| state.pending_egg.as_ref());
    if let Some(state) = &player.tiny_collection {
        chao.extend(state.chao.iter().map(newborn_public));
    }
    let reason = if game.tiny_egg_source.is_none() {
        Some("native-data-unavailable")
    } else if let Some(reason) = activity_block(player) {
        Some(reason)
    } else if clock_block(player, now) {
        Some("collection-clock-error")
    } else if pending.is_none() {
        Some("egg-unavailable")
    } else {
        None
    };
    json!({"enabled":true,"rulesVersion":RULES,"activeChaoId":player.id,"pendingEgg":pending.map(egg_public),
        "chao":chao,"maxChao":MAX_CHAO,"maxHeldEggs":1,"eligibility":{"canHatch":reason.is_none(),"reason":reason},
        "selection":{"enabled":false,"reason":"per-chao-activity-integration-required"}})
}

pub fn apply(
    game: &Game,
    player: &mut Player,
    intent: &Intent,
    now: u64,
    receipt_id: &str,
) -> Result<Value, &'static str> {
    if let Some(reason) = activity_block(player) {
        return Err(reason);
    }
    if clock_block(player, now) {
        return Err("collection-clock-error");
    }
    let source = game
        .tiny_egg_source
        .as_ref()
        .ok_or("native-data-unavailable")?;
    match intent {
        Intent::TinyEggPurchase {
            egg_id,
            stock_epoch,
        } => {
            if *stock_epoch != now / ROTATION_MS {
                return Err("stale-stock");
            }
            if let Some(reason) = purchase_block(game, player, now) {
                return Err(reason);
            }
            let (offered, price) = offer(game, player, now)?;
            if offered != *egg_id {
                return Err("egg-not-offered");
            }
            let seed = initial_stock_seed(player);
            let state = player
                .tiny_collection
                .get_or_insert_with(|| CollectionState {
                    version: 1,
                    owner_id: player.id.clone(),
                    stock_seed: seed,
                    hatch_rng: u32::from_le_bytes(
                        Uuid::new_v4().as_bytes()[..4].try_into().unwrap(),
                    ),
                    last_server_ms: now,
                    pending_egg: None,
                    chao: Vec::new(),
                });
            let egg = OwnedEgg {
                id: Uuid::new_v4().to_string(),
                native_egg_id: *egg_id,
                purchased_at: now,
                stock_epoch: *stock_epoch,
                price,
                purchase_receipt_id: receipt_id.into(),
            };
            player.rings -= price;
            let effects = purchase_effect(&egg);
            state.pending_egg = Some(egg);
            state.last_server_ms = now;
            Ok(effects)
        }
        Intent::TinyEggHatch { owned_egg_id } => {
            let state = player.tiny_collection.as_mut().ok_or("egg-unavailable")?;
            let egg = state.pending_egg.as_ref().ok_or("egg-unavailable")?;
            if egg.id != *owned_egg_id {
                return Err("egg-id-mismatch");
            }
            if state.chao.len() + 1 >= MAX_CHAO {
                return Err("collection-full");
            }
            let mut record = vec![0; RECORD_LEN];
            record[0x58] = 0xff;
            record[0x59] = egg.native_egg_id;
            let mut rng = NativeRng::new(state.hatch_rng);
            // Adapt only the native caller counter to trusted host time.
            let context = NativeHatchContext {
                alternate_name_table: false,
                frame_counter: (now & u64::from(u32::MAX)) as u32,
            };
            let outcome =
                hatch_into_empty_native_slot(&mut record, &source.hatch, context, &mut rng)
                    .map_err(|_| "native-hatch-failed")?;
            let chao = OwnedChao {
                id: Uuid::new_v4().to_string(),
                name: format!("Tiny Chao {}", state.chao.len() + 2),
                origin: egg.clone(),
                hatched_at: now,
                hatch_receipt_id: receipt_id.into(),
                native_record_sha256: digest(&record),
                native_record: record,
                native_name_index: outcome.name_index,
            };
            let effects = hatch_effect(&chao);
            state.pending_egg = None;
            state.chao.push(chao);
            state.hatch_rng = rng.state();
            state.last_server_ms = now;
            Ok(effects)
        }
        _ => Err("invalid-request"),
    }
}
