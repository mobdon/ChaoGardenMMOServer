//! ASOE Janken active-round rules, original projectile states and bounded input.
//! Original scene transitions, OAM/art and wallet settlement are separate.
//! SWI0A is an explicit BIOS numerical boundary, not authentic BIOS execution.
use super::{gba, native_growth::NativeRng, native_rps::NativeRpsWaves};
use std::fmt;

pub const ASOE_LENGTH: usize = 138_752;
pub const ASOE_SHA256: &str = "989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0";
pub const TRIG_OFFSET: usize = 0x11c5e;
pub const TRIG_SHA256: &str = "647187a05a2e9ffb2711d0389755d07090e7ac9c4e2785526ddc8519315da0e2";
pub const HITBOX_OFFSET: usize = 0x11f84;
pub const HITBOX_SHA256: &str = "80f750d2fa5d848bc87e77a92bb55cd58581e64dae5ae5d69bc421b015e850fe";
pub const CARD_COUNT: usize = 3;
pub const INPUT_A: u16 = 1;
pub const INPUT_RIGHT: u16 = 0x10;
pub const INPUT_LEFT: u16 = 0x20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    SourceIdentity,
    SourceTable,
    InvalidLane,
    InvalidInput,
    BiosUnavailable,
    NativeScratchRead { initialized: u8, index: u8 },
    RandomBudget,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Source-bound ASOE tables, not another cartridge/decompression reader.
#[derive(Clone, Debug)]
pub struct NativeRpsAssets {
    pub trig: [i16; 64],
    pub hitbox: [i8; 47],
}
impl NativeRpsAssets {
    pub fn from_verified_payload(payload: &[u8]) -> Result<Self, Error> {
        if payload.len() != ASOE_LENGTH || gba::sha256(payload) != ASOE_SHA256 {
            return Err(Error::SourceIdentity);
        }
        let trig = &payload[TRIG_OFFSET..TRIG_OFFSET + 128];
        let hitbox = &payload[HITBOX_OFFSET..HITBOX_OFFSET + 47];
        if gba::sha256(trig) != TRIG_SHA256 || gba::sha256(hitbox) != HITBOX_SHA256 {
            return Err(Error::SourceTable);
        }
        Ok(Self {
            trig: std::array::from_fn(|i| i16::from_le_bytes([trig[2 * i], trig[2 * i + 1]])),
            hitbox: std::array::from_fn(|i| hitbox[i] as i8),
        })
    }

    /// Original 02007FB4 with its literal quarter-wave table and exact
    /// rounding/reflection. The table starts at cosine0, not sine0.
    pub fn native_trig(&self, angle: i32, divisor_scale: i32) -> i16 {
        let quotient = signed_divide(
            angle.wrapping_shl(7).wrapping_add(divisor_scale),
            divisor_scale.wrapping_mul(2),
        );
        let index = (quotient & 255) as usize;
        match index {
            0..=63 => self.trig[index],
            64..=127 => self.trig[index ^ 0x7f].wrapping_neg(),
            128..=191 => self.trig[index ^ 0x80].wrapping_neg(),
            _ => self.trig[index ^ 0xff],
        }
    }
}

fn signed_divide(a: i32, b: i32) -> i32 {
    if b == 0 {
        0
    } else {
        (i64::from(a) / i64::from(b)) as i32
    }
}

pub use super::native_bios_math::{
    BiosMath as BiosAngle, ReferenceBiosMath as ReferenceBiosAngle,
    RejectBiosMath as RejectBiosAngle,
};
impl From<super::native_bios_math::MathError> for Error {
    fn from(_: super::native_bios_math::MathError) -> Self {
        Self::BiosUnavailable
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeRpsCard {
    pub raw: [u8; 12],
}
impl NativeRpsCard {
    pub fn state(self) -> u8 {
        self.raw[0]
    }
    pub fn gesture(self) -> u8 {
        self.raw[2]
    }
    pub fn position(self) -> (i16, i16) {
        (self.half(4), self.half(6))
    }
    fn half(self, offset: usize) -> i16 {
        i16::from_le_bytes(self.raw[offset..offset + 2].try_into().unwrap())
    }
    fn set_half(&mut self, offset: usize, value: i16) {
        self.raw[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeRpsLauncher {
    pub cards: [NativeRpsCard; CARD_COUNT],
    pub shots: u8,
    pub displayed_shots: u8,
    pub display_fraction: u8,
    pub display_counter: u8,
    pub lane: u8,
    pub marker_x: u8,
}
impl NativeRpsLauncher {
    /// Original ABE8 then B3B4, with3 RNG draws and unchanged opaque fields.
    pub fn initialize(&mut self, rng: &mut NativeRng) {
        for card in &mut self.cards {
            card.raw[0] = 0;
            card.set_half(6, 116);
            card.raw[2] = (rng.next_u15() / 0x2aab) as u8;
        }
        self.shots = 5;
        self.displayed_shots = 5;
        self.display_fraction = 0;
        self.display_counter = 0;
        self.lane = 1;
        self.marker_x = 76;
    }

    /// Original B3EC, using its bounded key-event bits. Left is processed
    /// before right; simultaneous bits retain that native edge behavior.
    pub fn input_tick(&mut self, keys: u16) -> Result<bool, Error> {
        if self.lane > 2 {
            return Err(Error::InvalidLane);
        }
        if keys & !(INPUT_A | INPUT_LEFT | INPUT_RIGHT) != 0 {
            return Err(Error::InvalidInput);
        }
        if keys & INPUT_LEFT != 0 && self.lane > 0 {
            self.lane -= 1;
        }
        if keys & INPUT_RIGHT != 0 && self.lane < 2 {
            self.lane += 1;
        }
        let target = i32::from(self.lane) * 64 + 12;
        let marker = i32::from(self.marker_x);
        let difference = marker - target;
        self.marker_x = if difference == 0 {
            marker
        } else if (difference + 2) as u32 <= 4 {
            target
        } else if difference > 32 {
            marker - 16
        } else if difference < -32 {
            marker + 16
        } else {
            (marker + target) / 2
        } as u8;
        let card = &mut self.cards[usize::from(self.lane)];
        let fired = keys & INPUT_A != 0 && card.state() == 0;
        if fired {
            card.raw[0] = 1;
        } // Original counter is not reset here.
        Ok(fired)
    }

    /// Original AB84 display-only countdown. Actual shots decrement in the
    /// projectile callback, never because a user presses the fire button.
    pub fn tick_shot_display(&mut self) {
        if self.displayed_shots == self.shots && self.display_fraction == 0 {
            return;
        }
        self.display_counter = self.display_counter.wrapping_add(1);
        if self.display_counter == 2 {
            self.display_counter = 0;
            self.display_fraction = self.display_fraction.wrapping_sub(1);
            if (self.display_fraction as i8) < 0 {
                self.display_fraction = 11;
                self.displayed_shots = self.displayed_shots.wrapping_sub(1);
            }
        }
    }

    /// ACCC state transitions; all input effects occur later in B3EC.
    /// Clone/commit preserves state and RNG if BIOS/scratch validation fails.
    pub fn tick_projectiles(
        &mut self,
        waves: &mut NativeRpsWaves,
        assets: &NativeRpsAssets,
        bios: &impl BiosAngle,
        rng: &mut NativeRng,
    ) -> Result<ProjectileEvents, Error> {
        let (mut next, mut next_waves, mut next_rng) = (self.clone(), waves.clone(), *rng);
        let events = next.projectile_step(&mut next_waves, assets, bios, &mut next_rng)?;
        (*self, *waves, *rng) = (next, next_waves, next_rng);
        Ok(events)
    }

    fn projectile_step(
        &mut self,
        waves: &mut NativeRpsWaves,
        assets: &NativeRpsAssets,
        bios: &impl BiosAngle,
        rng: &mut NativeRng,
    ) -> Result<ProjectileEvents, Error> {
        let mut events = ProjectileEvents::default();
        for (index, card) in self.cards.iter_mut().enumerate() {
            let counter = card.raw[1] as i8;
            card.raw[1] = card.raw[1].wrapping_add(1);
            let lane_x = index as i16 * 64 + 12;
            match card.state() {
                1 => {
                    let y = card.half(6).wrapping_sub(4);
                    card.set_half(6, y);
                    if y < -32 {
                        self.shots = self.shots.wrapping_sub(1);
                        card.raw[0] = if (self.shots as i8) > 2 { 7 } else { 9 };
                        card.raw[1] = 0;
                        continue;
                    }
                    for (target_index, target) in waves.targets.iter_mut().enumerate() {
                        if target.state() != 0 || target.route() < 0 {
                            continue;
                        }
                        let (x, ty) = target.position();
                        let (dx, dy) = (
                            i32::from(x) - i32::from(lane_x),
                            i32::from(ty) - i32::from(y),
                        );
                        let table_index = (dy + 25) as u32;
                        if table_index > 46 {
                            continue;
                        }
                        let width = i32::from(assets.hitbox[table_index as usize]);
                        if dx < -width || dx >= width {
                            continue;
                        }
                        let difference =
                            i32::from(card.gesture() as i8) + 2 - i32::from(target.gesture() as i8);
                        if difference == 1 || difference == 4 {
                            card.raw[0] = 2;
                            target.raw[0] = 2;
                            waves.unhit_targets = waves.unhit_targets.wrapping_sub(1);
                            let angle = bios.atan2(dx as i16, dy as i16)?;
                            target.raw[10] =
                                (i32::from(assets.native_trig(i32::from(angle), 0x4000)) / 2048)
                                    as u8;
                            target.raw[11] =
                                (i32::from(assets.native_trig(i32::from(angle) + 0xc000, 0x4000))
                                    / 2048) as u8;
                            events.won |= 1 << target_index;
                        } else if difference == 2 {
                            card.raw[0] = 2;
                            target.raw[0] = 3;
                            waves.unhit_targets = waves.unhit_targets.wrapping_sub(1);
                            events.tied |= 1 << target_index;
                        } else {
                            card.raw[0] = 5;
                            let angle = bios.atan2(dx as i16, dy as i16)? ^ 0x8000;
                            card.raw[8] = (i32::from(assets.native_trig(i32::from(angle), 0x4000))
                                / 2048) as u8;
                            card.raw[9] =
                                (i32::from(assets.native_trig(i32::from(angle) + 0xc000, 0x4000))
                                    / 2048) as u8;
                            card.set_half(4, lane_x);
                            events.reflected |= 1 << index;
                        }
                        card.raw[1] = 0;
                        target.raw[1] = 0;
                        break;
                    }
                }
                2 if counter == 30 => {
                    card.raw[0] = 3;
                    card.raw[1] = 0;
                }
                3 if counter > 19 => {
                    card.raw[0] = 4;
                    card.raw[1] = 0;
                }
                4 if counter == 20 => {
                    card.raw[0] = 7;
                    card.raw[1] = 0;
                }
                5 | 6 => {
                    if card.state() == 5 {
                        let (x, y) = card.position();
                        if (i32::from(x) + 32) as u32 <= 271 && (-32..=159).contains(&y) {
                            card.set_half(4, x.wrapping_add(i16::from(card.raw[8] as i8)));
                            card.set_half(6, y.wrapping_add(i16::from(card.raw[9] as i8)));
                        }
                    }
                    if counter == 30 {
                        self.shots = self.shots.wrapping_sub(1);
                        card.raw[0] = if (self.shots as i8) > 2 { 7 } else { 9 };
                        card.raw[1] = 0;
                    }
                }
                7 if counter == 20 => {
                    card.raw[0] = 8;
                    card.raw[1] = 0;
                }
                8 => {
                    if counter == 10 {
                        let mut gesture = (rng.next_u15() / 0x2aab) as u8;
                        events.random_calls += 1;
                        let roll = rng.next_u15();
                        events.random_calls += 1;
                        if roll <= 0x1fff {
                            let eligible: Vec<_> = waves
                                .targets
                                .iter()
                                .enumerate()
                                .filter(|(_, t)| t.state() == 0)
                                .map(|(i, _)| i)
                                .collect();
                            if !eligible.is_empty() {
                                let roll = u32::from(rng.next_u15());
                                events.random_calls += 1;
                                let divisor =
                                    (eligible.len() as u32 + 0x7ffe) / eligible.len() as u32;
                                let pick = (roll / divisor) as usize;
                                let Some(&index) = eligible.get(pick) else {
                                    return Err(Error::NativeScratchRead {
                                        initialized: eligible.len() as u8,
                                        index: pick as u8,
                                    });
                                };
                                let original = waves.targets[index].gesture() as i8;
                                let next = i16::from(original) + 2;
                                gesture = if next > 2 {
                                    original.wrapping_sub(1) as u8
                                } else {
                                    next as u8
                                };
                            }
                        }
                        card.raw[2] = gesture;
                    }
                    if counter > 19 {
                        card.raw[0] = 0;
                        card.set_half(6, 116);
                    }
                }
                _ => {}
            }
        }
        Ok(events)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectileEvents {
    pub won: u16,
    pub tied: u16,
    pub reflected: u8,
    pub random_calls: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeRpsActor {
    pub x: i16,
    pub countdown: u16,
    pub mode: u8,
}
impl Default for NativeRpsActor {
    fn default() -> Self {
        Self {
            x: 80,
            countdown: 60,
            mode: 2,
        }
    }
}
impl NativeRpsActor {
    /// Original A528 autonomous actor. Its random choices share the same
    /// native RNG and therefore must run before targets/projectiles.
    pub fn tick(&mut self, rng: &mut NativeRng) -> Result<ActorEvents, Error> {
        let (mut next, mut next_rng) = (*self, *rng);
        let events = next.step(&mut next_rng)?;
        (*self, *rng) = (next, next_rng);
        Ok(events)
    }
    fn step(&mut self, rng: &mut NativeRng) -> Result<ActorEvents, Error> {
        let mut events = ActorEvents::default();
        if self.mode <= 1 {
            self.x = self.x.wrapping_add(if self.mode == 0 { -1 } else { 1 });
            events.frame = Some(match ((self.countdown as i16) >> 3) & 3 {
                1 => 63,
                3 => 72,
                _ => 54,
            });
        }
        self.countdown = self.countdown.wrapping_sub(1);
        if self.countdown != 0 {
            return Ok(events);
        }
        self.mode = (rng.next_u15() / 0x1556) as u8;
        events.random_calls += 1;
        match self.mode {
            0 | 1 => {
                let mut target = self.x;
                for _ in 0..128 {
                    target = (rng.next_u15() / 456) as i16 + 44;
                    events.random_calls += 1;
                    if target != self.x {
                        break;
                    }
                }
                if target == self.x {
                    return Err(Error::RandomBudget);
                }
                self.mode = u8::from(target > self.x);
                self.countdown = (i32::from(target) - i32::from(self.x)).unsigned_abs() as u16;
                events.facing = Some(self.mode);
            }
            _ => {
                if self.mode == 5 {
                    events.facing = Some(1);
                }
                events.frame = Some(match self.mode {
                    2 => 0,
                    3 => 27,
                    _ => 54,
                });
                self.countdown = rng.next_u15() / 182 + 60;
                events.random_calls += 1;
            }
        }
        Ok(events)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActorEvents {
    pub frame: Option<u8>,
    pub facing: Option<u8>,
    pub random_calls: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeRpsRound {
    pub waves: NativeRpsWaves,
    pub launcher: NativeRpsLauncher,
    pub actor: NativeRpsActor,
    pub finishing: bool,
}
impl NativeRpsRound {
    pub fn initialize(&mut self, rng: &mut NativeRng) {
        self.actor = NativeRpsActor::default();
        self.waves.round_rings = 0;
        self.waves.initialize(rng);
        self.launcher.initialize(rng);
        self.finishing = false;
    }
    /// Original active controller B80C and shot-display common tail. This
    /// reports the finish transition; native fade/after-finish callbacks must
    /// still run before a host may freeze a final native score.
    pub fn tick_active(
        &mut self,
        keys: u16,
        assets: &NativeRpsAssets,
        bios: &impl BiosAngle,
        rng: &mut NativeRng,
    ) -> Result<RoundEvents, Error> {
        if self.finishing {
            return Ok(RoundEvents::default());
        }
        if keys & !(INPUT_A | INPUT_LEFT | INPUT_RIGHT) != 0 {
            return Err(Error::InvalidInput);
        }
        let (mut next, mut next_rng) = (self.clone(), *rng);
        let actor = next.actor.tick(&mut next_rng)?;
        let targets = next.waves.tick_targets(&mut next_rng);
        let projectiles =
            next.launcher
                .tick_projectiles(&mut next.waves, assets, bios, &mut next_rng)?;
        let fired = next.launcher.input_tick(keys)?;
        let expired = if next.launcher.shots == 0 {
            false
        } else {
            next.waves.clock.tick()
        };
        next.finishing =
            next.launcher.shots == 0 || (expired && (next.waves.unhit_targets as i8) > 0);
        next.launcher.tick_shot_display();
        let events = RoundEvents {
            actor,
            targets,
            projectiles,
            fired,
            finish_started: next.finishing,
        };
        (*self, *rng) = (next, next_rng);
        Ok(events)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoundEvents {
    pub actor: ActorEvents,
    pub targets: super::native_rps::NativeRpsWaveUpdate,
    pub projectiles: ProjectileEvents,
    pub fired: bool,
    pub finish_started: bool,
}
