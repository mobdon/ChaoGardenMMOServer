//! Bounded original A2NE shop stock, purchase inventory and toy-unlock state.
//!
//! Source: sub_02007688, 02007978, 02007B00, 02007C4C and the immediate held
//! item constructors at 02007DA0/02008010/020080E0. Panel animation, dropping,
//! using toys, hatching, transfer, identity and durable MMO commits are outside
//! this slice. Callers own server RNG, eligible native task ticks and saves.
use super::native_growth::NativeRng;
use crate::{gba, shop};
use std::fmt;

pub const FRUIT_CAPACITY: usize = 8;
pub const FRUIT_COUNT: usize = 7;
pub const EGG_COUNT: usize = 12;
pub const TOY_COUNT: usize = 3;
pub const EMPTY_ID: u8 = 0xff;
pub const TOY_UNLOCK_TICKS: u32 = 0x34bc0;
pub const RAW_FIELDS_END: usize = 0xd0;
pub const STOCK_ADDRESS: u32 = 0x0200_7688;
pub const FRUIT_PURCHASE_ADDRESS: u32 = 0x0200_7978;
pub const EGG_PURCHASE_ADDRESS: u32 = 0x0200_7b00;
pub const TOY_PURCHASE_ADDRESS: u32 = 0x0200_7c4c;
pub const EGG_PRICES_OFFSET: usize = 0x12564;
pub const TOY_PRICES_OFFSET: usize = 0x12594;
pub const EGG_PRICES_SHA256: &str =
    "53168556e91578a45777528ddd9f9ce8ed0ae63bbdfc0afb8850ca8fdc055b2d";
pub const TOY_PRICES_SHA256: &str =
    "0935002e4c7afc324f5aca05efcb6b3d674834a5cd1e6391f2fdf7e66c940909";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Catalog(shop::Error),
    SourceTable { offset: usize },
    TruncatedFields { actual: usize, minimum: usize },
    InvalidSavedId { field: &'static str, value: u8 },
    InvalidItem,
    UnsupportedFrameCounter,
    NotOffered,
    InputBlocked,
    HoldingItem,
    FruitCapacity,
    InsufficientRings,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeShopPrices {
    pub fruit: [u32; FRUIT_COUNT],
    pub egg: [u32; EGG_COUNT],
    pub toy: [u32; TOY_COUNT],
}

fn price_rows<const N: usize>(
    payload: &[u8],
    offset: usize,
    hash: &str,
) -> Result<[u32; N], Error> {
    let bytes = payload
        .get(offset..offset + N * 4)
        .ok_or(Error::SourceTable { offset })?;
    if gba::sha256(bytes) != hash {
        return Err(Error::SourceTable { offset });
    }
    Ok(std::array::from_fn(|id| {
        u32::from_le_bytes(bytes[id * 4..id * 4 + 4].try_into().unwrap())
    }))
}

impl NativeShopPrices {
    /// Reuses the existing complete A2NE payload/fruit-table identity gate.
    /// No second cartridge, compression, region or checksum reader is added.
    pub fn from_verified_payload(payload: &[u8]) -> Result<Self, Error> {
        let fruit =
            shop::TinyShopCatalog::from_verified_payload(payload).map_err(Error::Catalog)?;
        Ok(Self {
            fruit: fruit.fruits.map(|row| row.price),
            egg: price_rows(payload, EGG_PRICES_OFFSET, EGG_PRICES_SHA256)?,
            toy: price_rows(payload, TOY_PRICES_OFFSET, TOY_PRICES_SHA256)?,
        })
    }

    pub fn price(&self, item: NativeShopItem) -> Result<u32, Error> {
        match item {
            NativeShopItem::Fruit(id) => self.fruit.get(usize::from(id)),
            NativeShopItem::Egg(id) => self.egg.get(usize::from(id)),
            NativeShopItem::Toy(id) => self.toy.get(usize::from(id)),
        }
        .copied()
        .ok_or(Error::InvalidItem)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativePosition {
    pub x: u8,
    pub y: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeShopFields {
    pub toy_unlock_ticks: u32,
    /// 0x1C: FF until the first opening, then native frame_counter % 5.
    pub egg_cycle: u8,
    pub egg_position: NativePosition,
    pub fruit_positions: [NativePosition; FRUIT_CAPACITY],
    /// Saved toy presence is its x byte != 0 at 0x31 + 2*id.
    pub toy_positions: [NativePosition; TOY_COUNT],
    /// Preserve raw source fields until their broader meanings are traced.
    pub chao_field_58: u8,
    pub egg_id: u8,
    pub fruit_ids: [u8; FRUIT_CAPACITY],
    pub jewel_field_68: u8,
    pub shop_visited_ce: u8,
    pub duck_field_cf: u8,
}

impl Default for NativeShopFields {
    fn default() -> Self {
        Self {
            toy_unlock_ticks: 0,
            egg_cycle: EMPTY_ID,
            egg_position: NativePosition::default(),
            fruit_positions: [NativePosition::default(); FRUIT_CAPACITY],
            toy_positions: [NativePosition::default(); TOY_COUNT],
            chao_field_58: EMPTY_ID,
            egg_id: EMPTY_ID,
            fruit_ids: [EMPTY_ID; FRUIT_CAPACITY],
            jewel_field_68: 0,
            shop_visited_ce: 0,
            duck_field_cf: 0,
        }
    }
}

fn check_record(record: &[u8]) -> Result<(), Error> {
    if record.len() < RAW_FIELDS_END {
        Err(Error::TruncatedFields {
            actual: record.len(),
            minimum: RAW_FIELDS_END,
        })
    } else {
        Ok(())
    }
}

impl NativeShopFields {
    pub fn validate(&self) -> Result<(), Error> {
        for (field, value, count) in [
            ("egg_cycle", self.egg_cycle, 5),
            ("egg_id", self.egg_id, EGG_COUNT),
        ] {
            if value != EMPTY_ID && usize::from(value) >= count {
                return Err(Error::InvalidSavedId { field, value });
            }
        }
        for &value in &self.fruit_ids {
            if value != EMPTY_ID && usize::from(value) >= FRUIT_COUNT {
                return Err(Error::InvalidSavedId {
                    field: "fruit_id",
                    value,
                });
            }
        }
        Ok(())
    }

    /// Known byte fields relative to gSaveGameState. Not a save checksum or
    /// identity gate, and imported native values do not authorize MMO gains.
    pub fn read_raw_fields(record: &[u8]) -> Result<Self, Error> {
        check_record(record)?;
        let position = |offset| NativePosition {
            x: record[offset],
            y: record[offset + 1],
        };
        let fields = Self {
            toy_unlock_ticks: u32::from_le_bytes(record[0x14..0x18].try_into().unwrap()),
            egg_cycle: record[0x1c],
            egg_position: position(0x1f),
            fruit_positions: std::array::from_fn(|id| position(0x21 + id * 2)),
            toy_positions: std::array::from_fn(|id| position(0x31 + id * 2)),
            chao_field_58: record[0x58],
            egg_id: record[0x59],
            fruit_ids: record[0x5a..0x62].try_into().unwrap(),
            jewel_field_68: record[0x68],
            shop_visited_ce: record[0xce],
            duck_field_cf: record[0xcf],
        };
        fields.validate()?;
        Ok(fields)
    }

    /// Writes only this slice's known fields, preserving every opaque byte.
    pub fn write_raw_fields(&self, record: &mut [u8]) -> Result<(), Error> {
        check_record(record)?;
        self.validate()?;
        record[0x14..0x18].copy_from_slice(&self.toy_unlock_ticks.to_le_bytes());
        record[0x1c] = self.egg_cycle;
        record[0x1f..0x21].copy_from_slice(&[self.egg_position.x, self.egg_position.y]);
        for (id, position) in self.fruit_positions.iter().enumerate() {
            record[0x21 + id * 2..0x23 + id * 2].copy_from_slice(&[position.x, position.y]);
        }
        for (id, position) in self.toy_positions.iter().enumerate() {
            record[0x31 + id * 2..0x33 + id * 2].copy_from_slice(&[position.x, position.y]);
        }
        record[0x58] = self.chao_field_58;
        record[0x59] = self.egg_id;
        record[0x5a..0x62].copy_from_slice(&self.fruit_ids);
        record[0x68] = self.jewel_field_68;
        record[0xce] = self.shop_visited_ce;
        record[0xcf] = self.duck_field_cf;
        Ok(())
    }

    pub fn fruit_count(&self) -> usize {
        self.fruit_ids.iter().filter(|&&id| id != EMPTY_ID).count()
    }

    pub fn next_toy(&self) -> Option<u8> {
        if self.toy_positions[0].x == 0 {
            Some(0)
        } else if self.toy_positions[1].x == 0 {
            Some(1)
        } else if self.toy_positions[2].x == 0 && self.toy_unlock_ticks == 0 {
            Some(2)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeShopItem {
    Fruit(u8),
    Egg(u8),
    Toy(u8),
}

impl NativeShopItem {
    pub fn row_y(self) -> Option<i16> {
        match self {
            Self::Fruit(id) if usize::from(id) < FRUIT_COUNT => Some(26 + i16::from(id) * 16),
            Self::Egg(id) if usize::from(id) < EGG_COUNT => Some(0x8a),
            Self::Toy(id) if usize::from(id) < TOY_COUNT => Some(0x9a),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeShopStock {
    pub fruit_available: [bool; FRUIT_COUNT],
    pub egg: Option<u8>,
    pub toy: Option<u8>,
    pub random_calls: u8,
}

impl NativeShopStock {
    pub fn contains(&self, item: NativeShopItem) -> bool {
        match item {
            NativeShopItem::Fruit(id) => self
                .fruit_available
                .get(usize::from(id))
                .copied()
                .unwrap_or(false),
            NativeShopItem::Egg(id) => self.egg == Some(id),
            NativeShopItem::Toy(id) => self.toy == Some(id),
        }
    }
    fn consume(&mut self, item: NativeShopItem) {
        match item {
            NativeShopItem::Fruit(id) => self.fruit_available[usize::from(id)] = false,
            NativeShopItem::Egg(_) => self.egg = None,
            NativeShopItem::Toy(_) => self.toy = None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeHeldItem {
    pub item: NativeShopItem,
    pub fruit_slot: Option<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativePurchaseInput {
    pub global_flags: u8,
    pub panel_x: i16,
    pub modal_1509: u8,
    pub cursor_x: i16,
    pub cursor_y: i16,
    pub pressed_keys: u16,
    pub held_keys: u16,
}

impl NativePurchaseInput {
    fn permits(self, item: NativeShopItem) -> bool {
        let Some(row_y) = item.row_y() else {
            return false;
        };
        let y = self.cursor_y.wrapping_add(8);
        self.global_flags & 0x90 == 0
            && self.panel_x >= 0
            && self.modal_1509 == 0
            && self.cursor_x <= 0x33
            && y <= row_y
            && y > row_y - 0x10
            && self.pressed_keys & 1 != 0
            && self.held_keys & 0xf0 == 0
    }
    fn held_position(self) -> NativePosition {
        NativePosition {
            x: self.cursor_x as u8,
            y: self.cursor_y.wrapping_add(0x13) as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativePurchase {
    pub item: NativeShopItem,
    pub price: u32,
    pub rings_after: u32,
    pub fruit_slot: Option<u8>,
    /// Native 0300152C becomes one; persistence/once-only MMO commit is separate.
    pub native_dirty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeShopState {
    pub fields: NativeShopFields,
    /// Runtime native wallet 03005274, supplied by the authoritative host.
    pub rings: u32,
    pub held: Option<NativeHeldItem>,
    cached_egg: Option<u8>,
    stock: Option<NativeShopStock>,
}

fn ordinary_egg(roll: u16) -> u8 {
    match roll {
        0..=0x1999 => 0,
        0x199a..=0x3332 => 2,
        0x3333..=0x4665 => 1,
        0x4666..=0x5998 => 3,
        0x5999..=0x6665 => 4,
        0x6666..=0x7332 => 6,
        _ => 5,
    }
}

fn jewel_egg(roll: u16, cycle: u8) -> u8 {
    let index = match roll {
        0..=0x40a3 => 1,
        0x40a4..=0x4147 => 0,
        0x4148..=0x4b84 => 3,
        0x4b85..=0x65c1 => 4,
        _ => 2,
    };
    7 + (cycle + index) % 5
}

impl NativeShopState {
    /// Restoring the garden resets native cached egg 03001530 to FF. Rings
    /// remain a separate host-owned value; no unknown wallet save offset is invented.
    pub fn restore(fields: NativeShopFields, rings: u32) -> Result<Self, Error> {
        fields.validate()?;
        Ok(Self {
            fields,
            rings,
            held: None,
            cached_egg: None,
            stock: None,
        })
    }

    pub fn cached_egg(&self) -> Option<u8> {
        self.cached_egg
    }
    pub fn stock(&self) -> Option<&NativeShopStock> {
        self.stock.as_ref()
    }

    /// One decrement per original shop-task update unless flag10 is set.
    /// Flag80 does NOT pause it. `ticks` is host-owned eligible task updates,
    /// not milliseconds, an offline duration or a client-claimed elapsed time.
    pub fn advance_shop_ticks(&mut self, ticks: u32, global_flags: u8) {
        if global_flags & 0x10 == 0 {
            self.fields.toy_unlock_ticks = self.fields.toy_unlock_ticks.saturating_sub(ticks);
        }
    }

    /// Stock-building branch 0200773E..0200793C, called once after the host's
    /// original panel/open-input gates pass. It does not implement those gates
    /// or panel animation. `egg_block_1501` is preserved as a raw runtime gate.
    pub fn open_stock(
        &mut self,
        frame_counter: u32,
        egg_block_1501: u8,
        rng: &mut NativeRng,
    ) -> Result<&NativeShopStock, Error> {
        self.fields.validate()?;
        if self.held.is_some() {
            return Err(Error::HoldingItem);
        }
        if self.fields.egg_cycle == EMPTY_ID {
            // Native Mod invokes signed BIOS Div. Negative frame-counter
            // rollover can produce invalid cycle bytes; reject that unsupported
            // source domain before mutation instead of inventing a safe modulo.
            if frame_counter > i32::MAX as u32 {
                return Err(Error::UnsupportedFrameCounter);
            }
            self.fields.egg_cycle = (frame_counter % 5) as u8;
        }
        let mut random_calls = 0;
        let egg = if self.fields.egg_id != EMPTY_ID || egg_block_1501 != 0 {
            None
        } else if self.fields.chao_field_58 == EMPTY_ID {
            Some(0)
        } else if let Some(id) = self.cached_egg {
            Some(id)
        } else {
            let jewel = self.fields.jewel_field_68 == 0 && {
                random_calls += 1;
                rng.next_u15() <= 0x1fff
            };
            random_calls += 1;
            let roll = rng.next_u15();
            let id = if jewel {
                jewel_egg(roll, self.fields.egg_cycle)
            } else {
                ordinary_egg(roll)
            };
            self.cached_egg = Some(id);
            Some(id)
        };
        self.stock = Some(NativeShopStock {
            fruit_available: [true; FRUIT_COUNT],
            egg,
            toy: self.fields.next_toy(),
            random_calls,
        });
        self.fields.shop_visited_ce = 1;
        Ok(self.stock.as_ref().unwrap())
    }

    pub fn close_stock(&mut self) {
        self.stock = None;
    }

    /// Purchase-to-held-item transition only. The host supplies native cursor
    /// state/input, verified prices, canonical inventory and wallet. No caller
    /// may use this pure function as an unauthenticated MMO mutation endpoint.
    pub fn purchase(
        &mut self,
        prices: &NativeShopPrices,
        item: NativeShopItem,
        input: NativePurchaseInput,
    ) -> Result<NativePurchase, Error> {
        self.fields.validate()?;
        let price = prices.price(item)?;
        if !input.permits(item) {
            return Err(Error::InputBlocked);
        }
        if self.held.is_some() {
            return Err(Error::HoldingItem);
        }
        if !self
            .stock
            .as_ref()
            .is_some_and(|stock| stock.contains(item))
        {
            return Err(Error::NotOffered);
        }
        let fruit_slot = if matches!(item, NativeShopItem::Fruit(_)) {
            Some(
                self.fields
                    .fruit_ids
                    .iter()
                    .position(|&id| id == EMPTY_ID)
                    .ok_or(Error::FruitCapacity)? as u8,
            )
        } else {
            None
        };
        if self.rings < price {
            return Err(Error::InsufficientRings);
        }
        let position = input.held_position();
        match item {
            NativeShopItem::Fruit(id) => {
                let slot = usize::from(fruit_slot.unwrap());
                self.fields.fruit_ids[slot] = id;
                self.fields.fruit_positions[slot] = position;
            }
            NativeShopItem::Egg(id) => {
                self.fields.egg_id = id;
                self.fields.egg_position = position;
            }
            NativeShopItem::Toy(id) => {
                self.fields.toy_positions[usize::from(id)] = position;
                if id == 1 {
                    self.fields.toy_unlock_ticks = TOY_UNLOCK_TICKS;
                    self.fields.duck_field_cf = 0;
                }
            }
        }
        self.rings -= price;
        self.held = Some(NativeHeldItem { item, fruit_slot });
        self.stock.as_mut().unwrap().consume(item);
        Ok(NativePurchase {
            item,
            price,
            rings_after: self.rings,
            fruit_slot,
            native_dirty: true,
        })
    }
}
