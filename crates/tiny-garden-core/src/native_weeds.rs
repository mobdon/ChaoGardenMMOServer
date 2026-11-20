//! Original A2NE weed spawning, emergence, pulling, removal and effect timing.
//! Native task callbacks remain separate, so the host preserves its task order.
//! No wallet/stat rewards, resident AI, renderer or service mutation is added.
use super::native_growth::NativeRng;
use crate::{gba, shop};
use std::{fmt, num::NonZeroU16};

pub const WEED_CAPACITY: usize = 16;
pub const MAP_WIDTH: usize = 22;
pub const MAP_HEIGHT: usize = 20;
pub const MAP_OFFSET: usize = 0x21bc4;
pub const TRIG_OFFSET: usize = 0x1fb74;
pub const HITBOX_OFFSET: usize = 0x20708;
pub const FIELDS_END: usize = 0x57;
pub const SPAWNER_ADDRESS: u32 = 0x0200_3424;
pub const EMERGENCE_ADDRESS: u32 = 0x0200_3534;
pub const LIFECYCLE_ADDRESS: u32 = 0x0200_3614;
pub const EFFECT_ADDRESS: u32 = 0x0200_36ec;
pub const PULL_ADDRESS: u32 = 0x0200_3c24;
pub const NATIVE_WEED_TILE: u16 = 0x109;
pub const NATIVE_EFFECT_TILE: u16 = 0x18e;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Catalog(shop::Error),
    SourceTable { offset: usize },
    TruncatedFields { actual: usize, minimum: usize },
    InvalidSlot,
    NoPendingSpawn,
    Capacity,
    PullBlocked,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeWeedAssets {
    pub map: [u16; MAP_WIDTH * MAP_HEIGHT],
    pub trig: [i16; 256],
    pub hitbox: [i8; 4],
}
fn checked_table<'a>(
    payload: &'a [u8],
    offset: usize,
    length: usize,
    hash: &str,
) -> Result<&'a [u8], Error> {
    let bytes = payload
        .get(offset..offset + length)
        .ok_or(Error::SourceTable { offset })?;
    if gba::sha256(bytes) != hash {
        return Err(Error::SourceTable { offset });
    }
    Ok(bytes)
}
impl NativeWeedAssets {
    /// Reuses the existing exact A2NE payload gate; no new ROM decoder exists.
    pub fn from_verified_payload(payload: &[u8]) -> Result<Self, Error> {
        shop::TinyShopCatalog::from_verified_payload(payload).map_err(Error::Catalog)?;
        let map = checked_table(
            payload,
            MAP_OFFSET,
            880,
            "05bb04f9013c42a4cca06d9159f5fcd36d6e29ab6abf2ef06bff7c9a16603a36",
        )?;
        let trig = checked_table(
            payload,
            TRIG_OFFSET,
            512,
            "b41552b15b70b61ebfaf7d857b9f99849fd89889f4d273733d1e069def5ac745",
        )?;
        let hitbox = checked_table(
            payload,
            HITBOX_OFFSET,
            4,
            "b4b1330198a0303f80edd2323a1097b743eddd753708ce78c9dd8af996576732",
        )?;
        Ok(Self {
            map: std::array::from_fn(|i| u16::from_le_bytes([map[2 * i], map[2 * i + 1]])),
            trig: std::array::from_fn(|i| i16::from_le_bytes([trig[2 * i], trig[2 * i + 1]])),
            hitbox: std::array::from_fn(|i| hitbox[i] as i8),
        })
    }
    pub fn permits_spawn(&self, x: u8, y: u8) -> bool {
        let index = usize::from(y / 8) * MAP_WIDTH + usize::from(x / 8);
        self.map
            .get(index)
            .is_some_and(|tile| matches!(tile, 9 | 13))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeWeedPosition {
    pub x: u8,
    pub y: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeWeedFields {
    pub spawn_ticks: u32,
    pub positions: [NativeWeedPosition; WEED_CAPACITY],
}
impl Default for NativeWeedFields {
    fn default() -> Self {
        Self {
            spawn_ticks: 0,
            positions: [NativeWeedPosition::default(); WEED_CAPACITY],
        }
    }
}
fn check_record(record: &[u8]) -> Result<(), Error> {
    if record.len() < FIELDS_END {
        Err(Error::TruncatedFields {
            actual: record.len(),
            minimum: FIELDS_END,
        })
    } else {
        Ok(())
    }
}
impl NativeWeedFields {
    pub fn read_raw_fields(record: &[u8]) -> Result<Self, Error> {
        check_record(record)?;
        Ok(Self {
            spawn_ticks: u32::from_le_bytes(record[0x10..0x14].try_into().unwrap()),
            positions: std::array::from_fn(|i| NativeWeedPosition {
                x: record[0x37 + 2 * i],
                y: record[0x38 + 2 * i],
            }),
        })
    }
    pub fn write_raw_fields(&self, record: &mut [u8]) -> Result<(), Error> {
        check_record(record)?;
        record[0x10..0x14].copy_from_slice(&self.spawn_ticks.to_le_bytes());
        for (i, p) in self.positions.iter().enumerate() {
            record[0x37 + 2 * i] = p.x;
            record[0x38 + 2 * i] = p.y;
        }
        Ok(())
    }
    pub fn count(&self) -> usize {
        self.positions.iter().filter(|p| p.x != 0).count()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeWeed {
    pub present: bool,
    pub emerging: bool,
    pub growth_frame: u8,
    pub x: u8,
    pub y: i16,
    pub base_y: u8,
    /// Native byte19: idle0, create effect1, pulling2, release effect3, wait4.
    pub interaction: u8,
    pub removal_requested: bool,
}
impl NativeWeed {
    fn restored(position: NativeWeedPosition) -> Self {
        Self {
            present: position.x != 0,
            x: position.x,
            y: i16::from(position.y),
            base_y: position.y,
            ..Self::default()
        }
    }
    fn emerging(position: NativeWeedPosition) -> Self {
        Self {
            present: true,
            emerging: true,
            x: position.x,
            base_y: position.y,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnEvent {
    Paused,
    Waiting,
    Pending,
    Spawned {
        slot: u8,
        position: NativeWeedPosition,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnUpdate {
    pub event: SpawnEvent,
    pub candidates: u16,
    pub random_calls: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectCommand {
    Create,
    Release,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeedUpdate {
    pub rendered: bool,
    pub removed: bool,
    pub effect: Option<EffectCommand>,
    /// Native ObjAffineSet input 16.14/frame, not a handmade render scale.
    pub affine_parameter: Option<i16>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativePullInput {
    pub global_flags: u8,
    pub modal_1509: u8,
    pub cursor_holding: bool,
    pub cursor_x: i16,
    pub cursor_y: i16,
    pub shop_boundary: i8,
    pub pressed_keys: u16,
    pub held_keys: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeWeedPull {
    pub slot: u8,
    pub phase: u8,
    pub counter: u8,
    pub cursor_x: i16,
    pub cursor_y: i16,
    pub cursor_layer: u8,
    pub cursor_tile: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PullUpdate {
    pub cursor: NativeWeedPull,
    pub finished: bool,
    pub native_dirty: bool,
    pub sound: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeWeeds {
    pub fields: NativeWeedFields,
    pub weeds: [NativeWeed; WEED_CAPACITY],
    pub pull: Option<NativeWeedPull>,
    pending_spawn: bool,
}
impl NativeWeeds {
    pub fn restore(fields: NativeWeedFields) -> Self {
        Self {
            weeds: fields.positions.map(NativeWeed::restored),
            fields,
            pull: None,
            pending_spawn: false,
        }
    }
    pub fn pending_spawn(&self) -> bool {
        self.pending_spawn
    }

    /// Original timer wraps at zero. Full capacity and flagsF3 pause even the
    /// timer. A pending rejection loop is resumed without a second clock tick
    /// or timer-reset draw; the host must finish it before advancing task time.
    pub fn spawn_tick(
        &mut self,
        flags: u8,
        assets: &NativeWeedAssets,
        rng: &mut NativeRng,
        budget: NonZeroU16,
    ) -> Result<SpawnUpdate, Error> {
        if self.pending_spawn {
            return self.continue_spawn(assets, rng, budget);
        }
        if flags & 0xf3 != 0 || self.fields.count() >= WEED_CAPACITY {
            return Ok(SpawnUpdate {
                event: SpawnEvent::Paused,
                candidates: 0,
                random_calls: 0,
            });
        }
        self.fields.spawn_ticks = self.fields.spawn_ticks.wrapping_sub(1);
        if self.fields.spawn_ticks != 0 {
            return Ok(SpawnUpdate {
                event: SpawnEvent::Waiting,
                candidates: 0,
                random_calls: 0,
            });
        }
        self.fields.spawn_ticks = (u32::from(rng.bounded(NonZeroU16::new(30).unwrap())) + 60) * 60;
        self.pending_spawn = true;
        let mut result = self.continue_spawn(assets, rng, budget)?;
        result.random_calls += 1;
        Ok(result)
    }
    pub fn continue_spawn(
        &mut self,
        assets: &NativeWeedAssets,
        rng: &mut NativeRng,
        budget: NonZeroU16,
    ) -> Result<SpawnUpdate, Error> {
        if !self.pending_spawn {
            return Err(Error::NoPendingSpawn);
        }
        let slot = self
            .fields
            .positions
            .iter()
            .position(|p| p.x == 0)
            .ok_or(Error::Capacity)?;
        let mut calls = 0;
        for attempt in 1..=budget.get() {
            let x = rng.bounded(NonZeroU16::new(160).unwrap()) as u8 + 8;
            let y = rng.bounded(NonZeroU16::new(136).unwrap()) as u8 + 24;
            calls += 2;
            if assets.permits_spawn(x, y) {
                let position = NativeWeedPosition { x, y };
                self.fields.positions[slot] = position;
                self.weeds[slot] = NativeWeed::emerging(position);
                self.pending_spawn = false;
                return Ok(SpawnUpdate {
                    event: SpawnEvent::Spawned {
                        slot: slot as u8,
                        position,
                    },
                    candidates: attempt,
                    random_calls: calls,
                });
            }
            // Native02003496 discards an extra raw RNG value after each rejected candidate.
            rng.next_u15();
            calls += 1;
        }
        Ok(SpawnUpdate {
            event: SpawnEvent::Pending,
            candidates: budget.get(),
            random_calls: calls,
        })
    }

    /// One original weed task callback. Effects are independent tasks managed
    /// by the host, so a child can outlive its removed parent for final frames.
    pub fn update_weed(&mut self, slot: u8, flags: u8) -> Result<WeedUpdate, Error> {
        let weed = self
            .weeds
            .get_mut(usize::from(slot))
            .ok_or(Error::InvalidSlot)?;
        if !weed.present || flags & 0x10 != 0 {
            return Ok(WeedUpdate::default());
        }
        let mut result = WeedUpdate {
            rendered: true,
            ..WeedUpdate::default()
        };
        if flags & 0x80 != 0 {
            return Ok(result);
        }
        if weed.emerging {
            weed.growth_frame = weed.growth_frame.wrapping_add(1);
            if weed.growth_frame > 63 {
                weed.y = i16::from(weed.base_y);
                weed.emerging = false;
            } else {
                weed.y = i16::from(weed.base_y) + ((64 - i16::from(weed.growth_frame)) >> 3);
                result.affine_parameter = Some(0x4000 / i16::from(weed.growth_frame));
            }
        } else {
            if weed.interaction == 1 {
                weed.interaction = 2;
                result.effect = Some(EffectCommand::Create);
            } else if weed.interaction == 3 {
                weed.interaction = 4;
                result.effect = Some(EffectCommand::Release);
            }
            if weed.interaction == 4 && weed.removal_requested {
                self.fields.positions[usize::from(slot)].x = 0;
                weed.present = false;
                result.removed = true;
                result.rendered = false;
            }
        }
        Ok(result)
    }

    /// Host supplies the topmost native pick-order subject; this checks the
    /// original class3 hit rectangle and idle cursor input, not scene sorting.
    pub fn begin_pull(
        &mut self,
        slot: u8,
        input: NativePullInput,
        assets: &NativeWeedAssets,
    ) -> Result<PullUpdate, Error> {
        let weed = self
            .weeds
            .get(usize::from(slot))
            .ok_or(Error::InvalidSlot)?;
        let x = input.cursor_x.wrapping_sub(8);
        let y = input.cursor_y.wrapping_add(8);
        let left = i16::from(weed.x).wrapping_add(i16::from(assets.hitbox[0]));
        let top = weed.y.wrapping_add(i16::from(assets.hitbox[2]));
        if self.pull.is_some()
            || !weed.present
            || weed.emerging
            || weed.interaction != 0
            || input.global_flags & 0xf3 != 0
            || input.modal_1509 != 0
            || input.cursor_holding
            || input.held_keys & 0xf0 != 0
            || input.pressed_keys & 1 == 0
            || input.cursor_x <= i16::from(input.shop_boundary)
            || x < left
            || x > left.wrapping_add(i16::from(assets.hitbox[1]))
            || y < top
            || y > top.wrapping_add(i16::from(assets.hitbox[3]))
        {
            return Err(Error::PullBlocked);
        }
        let cursor = NativeWeedPull {
            slot,
            phase: 0,
            counter: 0,
            cursor_x: i16::from(weed.x),
            cursor_y: weed.y.wrapping_sub(16),
            cursor_layer: weed.base_y.wrapping_add(1),
            cursor_tile: 0xb0,
        };
        self.pull = Some(cursor);
        Ok(PullUpdate {
            cursor,
            finished: false,
            native_dirty: true,
            sound: Some(0x15),
        })
    }

    /// Original02003C24 pauses only on flag80. The separate weed/effect task
    /// callbacks also check flag10; the host retains that original distinction.
    pub fn update_pull(
        &mut self,
        flags: u8,
        assets: &NativeWeedAssets,
    ) -> Result<PullUpdate, Error> {
        let mut pull = self.pull.ok_or(Error::PullBlocked)?;
        let weed = &mut self.weeds[usize::from(pull.slot)];
        let mut sound = None;
        let mut finished = false;
        if flags & 0x80 == 0 {
            match pull.phase {
                0 | 3 => {
                    pull.counter = pull.counter.wrapping_add(1);
                    if pull.counter > 7 {
                        pull.counter = 0;
                        if pull.phase == 0 {
                            weed.interaction = weed.interaction.wrapping_add(1);
                        }
                        pull.phase += 1;
                    }
                }
                1 | 4 => {
                    pull.cursor_y = pull.cursor_y.wrapping_sub(1);
                    weed.y = weed.y.wrapping_sub(1);
                    pull.counter = pull.counter.wrapping_add(1);
                    if pull.counter != 0 {
                        pull.counter = 0;
                        pull.phase += 1;
                    }
                }
                2 | 5 => {
                    pull.counter = pull.counter.wrapping_add(1);
                    if pull.counter > 15 {
                        pull.counter = 0;
                        if pull.phase == 2 {
                            pull.cursor_y = pull.cursor_y.wrapping_add(1);
                            weed.y = weed.y.wrapping_add(1);
                        } else {
                            sound = Some(0xd);
                            weed.interaction = weed.interaction.wrapping_add(1);
                        }
                        pull.phase += 1;
                    }
                }
                6 => {
                    let value = assets.trig[usize::from(pull.counter.wrapping_add(0xc0))];
                    weed.y = i16::from(weed.base_y).wrapping_sub(value >> 4);
                    pull.cursor_y = weed.y.wrapping_sub(16);
                    pull.counter = pull.counter.wrapping_add(5);
                    if pull.counter > 63 {
                        pull.counter = 0;
                        pull.cursor_tile = 0xb4;
                        pull.phase += 1;
                    }
                }
                7 => {
                    pull.counter = pull.counter.wrapping_add(1);
                    if pull.counter > 11 {
                        weed.removal_requested = true;
                        finished = true;
                    }
                }
                _ => return Err(Error::PullBlocked),
            }
        }
        self.pull = if finished { None } else { Some(pull) };
        Ok(PullUpdate {
            cursor: pull,
            finished,
            native_dirty: true,
            sound,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeWeedEffect {
    pub tile: u16,
    pub counter: u8,
    pub releasing: bool,
    pub disposed: bool,
}
impl Default for NativeWeedEffect {
    fn default() -> Self {
        Self {
            tile: NATIVE_EFFECT_TILE,
            counter: 0,
            releasing: false,
            disposed: false,
        }
    }
}
impl NativeWeedEffect {
    /// Returns whether the original task queues a sprite this update.
    pub fn update(&mut self, flags: u8) -> bool {
        if self.disposed || flags & 0x10 != 0 {
            return false;
        }
        if flags & 0x80 == 0 && self.releasing {
            self.counter = self.counter.wrapping_add(1);
            if self.counter > 9 {
                self.counter = 0;
                self.tile = self.tile.wrapping_add(2);
                if self.tile > 0x193 {
                    self.disposed = true;
                    return false;
                }
            }
        }
        true
    }
}
