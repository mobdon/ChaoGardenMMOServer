//! A2NE Chao Bounce fixed-point tasks, spring timing, targets and stage flow.
//! Native art/audio and BIOS numerical parity remain explicit adapter boundaries.
use crate::{native_bios_math::BiosMath, native_memory::NativeButtons, shop};
use std::fmt;

pub const INITIALIZE_ADDRESS: u32 = 0x0200_d294;
pub const CHAO_ADDRESS: u32 = 0x0200_d958;
pub const SPRING_ADDRESS: u32 = 0x0200_e964;
pub const TARGET_ADDRESS: u32 = 0x0200_dd60;
pub const TARGETS: usize = 26;
/// Twenty-six targets per stage, with one final-stage target worth ten.
pub const MAX_NATIVE_RINGS: u16 = 113;
pub const ACTOR_LEN: usize = 36;
pub const TRIG_OFFSET: usize = 0x1fb74;
pub const SPRING_OFFSETS: usize = 0x2eb64;
const CHAO_INIT: u32 = 0x0200_dc6c;
const CHAO_LOSS: u32 = 0x0200_d814;
const SPRING_INIT: u32 = 0x0200_ebb0;
const SPRING_WAIT: u32 = 0x0200_eb14;
const SPRING_CLEAR: u32 = 0x0200_dff8;
const TARGET_BURST: u32 = 0x0200_dcc0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Source(shop::Error),
    InvalidState,
    InvalidInput,
    BiosUnavailable,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug)]
pub struct A2neBounceSource {
    trig: [i16; 256],
    spring_offsets: [u8; 5],
    burst_tiles: [u8; 6],
}
impl A2neBounceSource {
    pub fn from_verified_payload(payload: &[u8]) -> Result<Self, Error> {
        shop::TinyShopCatalog::from_verified_payload(payload).map_err(Error::Source)?;
        Ok(Self {
            trig: std::array::from_fn(|i| {
                i16::from_le_bytes(
                    payload[TRIG_OFFSET + i * 2..TRIG_OFFSET + i * 2 + 2]
                        .try_into()
                        .unwrap(),
                )
            }),
            spring_offsets: payload[SPRING_OFFSETS..SPRING_OFFSETS + 5]
                .try_into()
                .unwrap(),
            burst_tiles: payload[0x2eb5a..0x2eb60].try_into().unwrap(),
        })
    }
    fn cos(&self, angle: u8) -> i32 {
        i32::from(self.trig[usize::from(angle)])
    }
    fn sin(&self, angle: u8) -> i32 {
        self.cos(angle.wrapping_add(0xc0))
    }
    pub fn begin_round(&self) -> NativeBounceRound {
        NativeBounceRound::begin(self.clone())
    }
}

/// Trusted inspection record. Never a client save or network import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeBounceActor {
    pub raw: [u8; ACTOR_LEN],
}
impl Default for NativeBounceActor {
    fn default() -> Self {
        let mut value = Self {
            raw: [0; ACTOR_LEN],
        };
        value.raw[0x1c] = 255;
        value
    }
}
impl NativeBounceActor {
    fn reallocated(mut self) -> Self {
        // Native 02000D04 resets only these task fields. In particular,1A and
        // the opaque tail survive reuse until each source initializer writes.
        self.raw[7] = 0;
        self.raw[8..10].fill(0);
        self.raw[12..14].fill(0);
        self.raw[0x16..0x1a].fill(0);
        self.raw[0x1c] = 255;
        self
    }
    fn u16(&self, offset: usize) -> u16 {
        u16::from_le_bytes(self.raw[offset..offset + 2].try_into().unwrap())
    }
    fn i16(&self, offset: usize) -> i16 {
        self.u16(offset) as i16
    }
    fn i32(&self, offset: usize) -> i32 {
        i32::from_le_bytes(self.raw[offset..offset + 4].try_into().unwrap())
    }
    fn set16(&mut self, offset: usize, value: i32) {
        self.raw[offset..offset + 2].copy_from_slice(&(value as u16).to_le_bytes());
    }
    fn set32(&mut self, offset: usize, value: i32) {
        self.raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn add8(&mut self, offset: usize, value: i8) {
        self.raw[offset] = self.raw[offset].wrapping_add(value as u8);
    }
    fn add16(&mut self, offset: usize, value: i32) {
        self.set16(offset, i32::from(self.u16(offset)).wrapping_add(value));
    }
    fn callback(&self) -> u32 {
        self.i32(0) as u32
    }
    fn set_callback(&mut self, address: u32) {
        self.set32(0, address as i32);
    }
    fn flags(&self) -> u8 {
        self.raw[7]
    }
    fn x(&self) -> i16 {
        self.i16(0xa)
    }
    fn y(&self) -> i16 {
        self.i16(0xe)
    }
    fn world_y(&self) -> i16 {
        self.i16(0x22)
    }
    fn visible(&self) -> bool {
        self.u16(0xe).wrapping_add(24) <= 224
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BouncePhase {
    Playing,
    Paused,
    Result,
    Fade,
    Ended,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Return {
    Garden,
    Restart,
    NextStage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BounceActorView {
    pub x: i16,
    pub y: i16,
    pub tile: u16,
    pub flags: u8,
    pub direction: u8,
    pub visible: bool,
    pub descriptor_offset: Option<u32>,
    pub style: u8,
    pub transform: u8,
    pub packed_palette_layer: u8,
}
impl From<NativeBounceActor> for BounceActorView {
    fn from(a: NativeBounceActor) -> Self {
        Self {
            x: a.x(),
            y: a.y(),
            tile: a.u16(0x14),
            flags: a.flags(),
            direction: a.raw[0x1d],
            visible: a.visible(),
            descriptor_offset: (0x0200_0000..0x0204_0000)
                .contains(&(a.i32(0x10) as u32))
                .then_some((a.i32(0x10) as u32).wrapping_sub(0x0200_0000)),
            style: a.raw[0x16],
            transform: a.raw[0x17],
            packed_palette_layer: a.raw[0x18],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBounceView {
    pub phase: BouncePhase,
    pub stage: u8,
    pub lives: u8,
    pub rings: u16,
    pub scroll: i16,
    pub chao: BounceActorView,
    pub spring: BounceActorView,
    pub targets: [Option<BounceActorView>; TARGETS],
    pub bumpers: [Option<BounceActorView>; 2],
    pub clouds: [BounceActorView; 3],
    pub compression: i8,
    pub power: i8,
    pub pause_selection: u8,
}

/// Server-owned original video-frame state. No trusted time/score from clients.
#[derive(Clone, Debug)]
pub struct NativeBounceRound {
    source: A2neBounceSource,
    pub stage: u8,
    pub lives: u8,
    pub rings: u16,
    pub scroll: i16,
    pub flags: u8,
    pub chao: NativeBounceActor,
    pub spring: NativeBounceActor,
    pub target_master: NativeBounceActor,
    pub targets: [Option<NativeBounceActor>; TARGETS],
    pub target_kind: [u8; TARGETS],
    pub remaining: u8,
    pub target_sound: u8,
    pub clouds: [NativeBounceActor; 3],
    pub bumpers: [Option<NativeBounceActor>; 2],
    pub vy: i32,
    pub vx: i32,
    pub hitbox: [i16; 4],
    intro: u8,
    pause_delay: u8,
    pause_selection: u8,
    pending: Option<Return>,
    fade: Option<u8>,
    ended: bool,
    won: bool,
    right_visual_flags: u8,
    right_visual_phase: u8,
    cloud_generator_reuse: NativeBounceActor,
}
impl NativeBounceRound {
    fn begin(source: A2neBounceSource) -> Self {
        let mut result = Self {
            source,
            stage: 0,
            lives: 3,
            rings: 0,
            scroll: 0,
            flags: 1,
            chao: NativeBounceActor::default(),
            spring: NativeBounceActor::default(),
            target_master: NativeBounceActor::default(),
            targets: [None; TARGETS],
            target_kind: [1; TARGETS],
            remaining: 26,
            target_sound: 0,
            clouds: [NativeBounceActor::default(); 3],
            bumpers: [None; 2],
            vy: 0,
            vx: 0,
            hitbox: [256, 0, 0, 0],
            intro: 16,
            pause_delay: 4,
            pause_selection: 0,
            pending: None,
            fade: None,
            ended: false,
            won: false,
            right_visual_flags: 0,
            right_visual_phase: 0,
            cloud_generator_reuse: NativeBounceActor::default(),
        };
        result.initialize_actors();
        result
    }
    fn initialize_actors(&mut self) {
        self.chao = self.chao.reallocated();
        self.chao.set_callback(CHAO_INIT);
        self.spring = self.spring.reallocated();
        self.spring.set_callback(SPRING_INIT);
        self.target_master = self.target_master.reallocated();
        self.target_master.set_callback(0x0200_deec);
        self.targets = [None; TARGETS];
        self.target_sound = 0;
        self.clouds = self.clouds.map(NativeBounceActor::reallocated);
        for cloud in &mut self.clouds {
            cloud.set_callback(0x0200_f048);
        }
        let previous_bumpers = self.bumpers;
        self.bumpers = std::array::from_fn(|i| {
            if self.stage > 1 && (i == 0 || self.stage > 2) {
                let mut a = previous_bumpers[i].unwrap_or_default().reallocated();
                a.raw[0x1c] = i as u8;
                a.set_callback(0x0200_f4c0);
                Some(a)
            } else {
                None
            }
        });
        self.flags = 1;
        self.intro = 16;
        self.pause_delay = 4;
        self.pause_selection = 0;
        self.fade = None;
        self.pending = None;
    }
    pub fn finished(&self) -> Option<(bool, u16)> {
        self.ended.then_some((self.won, self.rings))
    }
    pub fn view(&self) -> NativeBounceView {
        NativeBounceView {
            phase: if self.ended {
                BouncePhase::Ended
            } else if self.fade.is_some() {
                BouncePhase::Fade
            } else if self.flags & 0x80 != 0 {
                BouncePhase::Paused
            } else if self.flags & 0x10 != 0 {
                BouncePhase::Result
            } else {
                BouncePhase::Playing
            },
            stage: self.stage,
            lives: self.lives,
            rings: self.rings,
            scroll: self.scroll,
            chao: self.chao.into(),
            spring: self.spring.into(),
            targets: self.targets.map(|a| a.map(Into::into)),
            bumpers: self.bumpers.map(|a| a.map(Into::into)),
            clouds: self.clouds.map(Into::into),
            compression: self.spring.raw[0x1f] as i8,
            power: self.spring.raw[0x1e] as i8,
            pause_selection: self.pause_selection,
        }
    }
    fn validate(&self) -> Result<(), Error> {
        if self.stage > 3
            || self.lives > 3
            || self.remaining > 26
            || self.target_kind.iter().any(|&kind| kind > 2)
            || self.spring.raw[0x1d] > 4
            || self.chao.raw[0x1d]
                > if self.chao.callback() == CHAO_LOSS {
                    4
                } else {
                    1
                }
            || self.pause_selection > 1
        {
            return Err(Error::InvalidState);
        }
        if ![CHAO_INIT, CHAO_ADDRESS, CHAO_LOSS].contains(&self.chao.callback())
            || ![SPRING_INIT, SPRING_WAIT, SPRING_ADDRESS, SPRING_CLEAR]
                .contains(&self.spring.callback())
        {
            return Err(Error::InvalidState);
        }
        Ok(())
    }
    pub fn step(&mut self, buttons: NativeButtons, bios: &impl BiosMath) -> Result<(), Error> {
        self.validate()?;
        if buttons.pressed & !0xff != 0 || buttons.held & !0xff != 0 {
            return Err(Error::InvalidInput);
        }
        if self.ended {
            return Ok(());
        }
        let mut next = self.clone();
        next.step_inner(buttons, bios)?;
        next.validate()?;
        *self = next;
        Ok(())
    }
    fn step_inner(
        &mut self,
        mut buttons: NativeButtons,
        bios: &impl BiosMath,
    ) -> Result<(), Error> {
        if let Some(ticks) = self.fade {
            if ticks == 15 && self.pending == Some(Return::Garden) {
                self.ended = true;
            } else if ticks == 16 {
                match self.pending.take().unwrap_or(Return::Garden) {
                    Return::Garden => self.ended = true,
                    Return::Restart => {
                        self.scroll = 160;
                        self.initialize_actors();
                    }
                    Return::NextStage => {
                        let old_bumpers = usize::from(self.stage > 1) + usize::from(self.stage > 2);
                        self.stage += 1;
                        let new_bumpers = usize::from(self.stage > 1) + usize::from(self.stage > 2);
                        if new_bumpers > old_bumpers {
                            // D538 inserts each added bumper before the other
                            // root tasks. Native allocation reuses those exact
                            // old slots, including their unwritten fields.
                            self.bumpers[new_bumpers - 1] = Some(self.cloud_generator_reuse);
                            self.cloud_generator_reuse = self.spring;
                            self.spring = self.target_master;
                            self.target_master = self.chao;
                            self.chao = NativeBounceActor::default();
                            self.chao.raw[0x1a] = self.pause_delay;
                        }
                        self.scroll = 0;
                        self.target_kind = [1; TARGETS];
                        if self.stage > 2 {
                            self.target_kind[6] = 2;
                        }
                        self.remaining = 26;
                        self.initialize_actors();
                    }
                }
            } else {
                self.fade = Some(ticks + 1);
            }
            return Ok(());
        }
        if self.intro > 0 {
            self.intro -= 1;
            if self.intro == 0 {
                self.flags &= !1;
            }
        }
        self.pause_step(&mut buttons);
        let mut chao = self.chao;
        self.chao_step(&mut chao, &mut buttons)?;
        self.chao = chao;
        self.target_step();
        let mut spring = self.spring;
        self.spring_step(&mut spring, buttons)?;
        self.spring = spring;
        self.spring_child_step();
        self.cloud_step();
        for index in (0..2).rev() {
            if let Some(mut actor) = self.bumpers[index] {
                self.bumper_step(&mut actor, bios)?;
                self.bumpers[index] = Some(actor);
            }
        }
        if self.flags & 2 != 0 {
            self.fade = Some(0);
        }
        Ok(())
    }
    fn pause_step(&mut self, buttons: &mut NativeButtons) {
        if self.flags & 0x11 != 0 {
            return;
        }
        self.pause_delay = self.pause_delay.saturating_sub(1);
        if self.flags & 0x80 == 0 {
            if buttons.pressed & 8 != 0 && self.pause_delay == 0 {
                self.pause_selection = 0;
                self.pause_delay = 4;
                self.flags |= 0x80;
            }
        } else if buttons.pressed & 0xb != 0 && self.pause_delay == 0 {
            if buttons.held & 2 == 0 && self.pause_selection != 0 {
                self.flags |= 2;
                self.pending = Some(Return::Garden);
            } else {
                self.flags &= 0x7f;
            }
            self.pause_delay = 4;
            buttons.pressed &= !1;
        } else if buttons.pressed & 64 != 0 && self.pause_selection != 0 {
            self.pause_selection = 0;
        } else if buttons.pressed & 128 != 0 && self.pause_selection == 0 {
            self.pause_selection = 1;
        }
    }
    fn chao_step(
        &mut self,
        a: &mut NativeBounceActor,
        buttons: &mut NativeButtons,
    ) -> Result<(), Error> {
        if a.callback() == CHAO_INIT {
            a.raw[7] = 5;
            a.set16(0xa, 122);
            a.set16(0xe, 137);
            a.set32(0x10, 0x0202_e4e8);
            a.set16(0x14, 346);
            a.raw[0x18] = 12;
            a.raw[0x1d] = 0;
            a.set16(0x22, 297);
            a.set16(0x20, 0);
            self.vy = 0;
            self.vx = 0;
            self.hitbox[0] = 256;
            a.set_callback(CHAO_ADDRESS);
            return Ok(());
        }
        if a.callback() == CHAO_LOSS {
            self.loss_step(a, *buttons);
            return Ok(());
        }
        if self.flags & 0x80 != 0 {
            return Ok(());
        }
        if a.flags() & 5 == 0 {
            self.vy = self.vy.wrapping_add(2048);
            a.set32(0x20, a.i32(0x20).wrapping_add(self.vy));
        }
        if a.flags() & 4 != 0 {
            if self.scroll <= 159 {
                self.scroll = self.scroll.wrapping_add(1);
                if buttons.pressed & 1 != 0 {
                    self.scroll = 160;
                    buttons.pressed &= !1;
                }
            }
        } else {
            let y = i32::from(a.world_y());
            self.scroll = if y <= 63 {
                0
            } else if y > 175 {
                160
            } else {
                (((y - 64) * 365) >> 8) as i16
            };
        }
        a.set16(0xe, i32::from(a.world_y().wrapping_sub(self.scroll)));
        if !a.visible() {
            self.hitbox[0] = 256;
            return Ok(());
        }
        if a.flags() & 0x80 != 0 {
            if a.world_y() <= 318 {
                a.set32(0x10, 0x0202_e4d8 + i32::from(a.raw[0x1d]) * 8);
                a.set16(0x14, 326);
                a.set32(8, a.i32(8).wrapping_add(self.vx));
                if a.x() <= 5 {
                    a.set16(0xa, 6);
                    a.raw[0x1d] = 1;
                    a.set32(0x10, 0x0202_e4d0);
                    self.vx = self.vx.wrapping_neg();
                } else if a.x() > 185 {
                    a.set16(0xa, 185);
                    a.raw[0x1d] = 0;
                    a.set32(0x10, 0x0202_e4c8);
                    self.vx = self.vx.wrapping_neg();
                }
            } else {
                if a.flags() & 0x40 != 0 {
                    a.set32(0x10, 0x0202_e4e8);
                    a.set16(0x14, 338);
                    a.add8(0x1a, 1);
                    if a.raw[0x1a] == 45 {
                        self.spring.raw[0x1e] = 255;
                        self.spring.raw[0x1f] = 0;
                        self.spring.raw[0x1c] = 0;
                        self.spring.set_callback(SPRING_CLEAR);
                    }
                    if a.raw[0x1a] > 44 {
                        a.set16(0x14, 346);
                        a.raw[0x1a] = 46;
                    }
                } else {
                    a.set16(0x14, if a.raw[0x1a] <= 15 { 330 } else { 334 });
                    a.add8(0x1a, 1);
                    if a.raw[0x1a] > 89 {
                        self.lives = self.lives.wrapping_sub(1);
                        if self.lives != 0 {
                            self.flags |= 2;
                            self.pending = Some(Return::Restart);
                        } else {
                            a.set_callback(CHAO_LOSS);
                            self.flags |= 0x10;
                        }
                    }
                }
                a.set16(0xe, 159);
                a.set16(0x22, 319);
                self.vy = 0;
            }
        } else if a.flags() & 1 == 0 {
            a.add8(0x1a, 1);
            if a.raw[0x1a] > 5 {
                a.raw[0x1a] = 0;
                a.raw[0x19] = if a.raw[0x19] == 0 { 4 } else { 0 };
            }
            a.set16(
                0x14,
                (if self.vy > 0 { 318 } else { 310 }) + i32::from(a.raw[0x19]),
            );
            a.set32(8, a.i32(8).wrapping_add(self.vx));
            if a.x() <= 5 {
                a.set16(0xa, 6);
                a.raw[0x1d] = 1;
                a.set32(0x10, 0x0202_e4d0);
                self.vx = self.vx.wrapping_neg();
            } else if a.x() > 185 {
                a.set16(0xa, 185);
                a.raw[0x1d] = 0;
                a.set32(0x10, 0x0202_e4c8);
                self.vx = self.vx.wrapping_neg();
            }
        }
        self.hitbox = [
            a.x().wrapping_sub(11),
            a.x().wrapping_add(12),
            a.y().wrapping_sub(20),
            a.y().wrapping_add(6),
        ];
        Ok(())
    }
    fn loss_step(&mut self, a: &mut NativeBounceActor, buttons: NativeButtons) {
        if a.flags() & 0x10 == 0 {
            a.raw[7] |= 0x10;
            a.raw[0x1d] = 0;
            a.set16(0x20, 0);
        }
        let timer = i32::from(a.i16(0x20));
        match a.raw[0x1d] {
            0 => {
                a.add16(0x20, 1);
                if 280 - timer * timer / 4 <= 183 {
                    a.raw[0x1d] = 1;
                    a.set16(0x20, 0);
                }
            }
            1 => {
                a.add16(0x20, 1);
                if a.i16(0x20) > 7 {
                    a.raw[0x1d] += 1;
                    a.set16(0x20, 0);
                }
            }
            2 => {
                a.add16(0x20, 1);
                if a.i16(0x20) > 15 {
                    a.raw[0x1d] += 1;
                }
            }
            3 => {
                a.add16(0x20, -1);
                if a.i16(0x20) < 0 {
                    a.raw[0x1d] += 1;
                    a.set16(0x20, 0);
                }
            }
            _ => {
                if buttons.pressed & 9 == 0 {
                    a.add16(0x20, 1);
                }
                if buttons.pressed & 9 != 0 || a.i16(0x20) > 179 {
                    self.flags |= 2;
                    self.pending = Some(Return::Garden);
                }
            }
        }
    }
    fn target_step(&mut self) {
        if self.target_master.callback() == 0x0200_deec {
            let base = match self.stage {
                0 => 168,
                1 => 104,
                2 => 72,
                _ => 24,
            };
            self.target_master.set16(0x22, base);
            self.targets = std::array::from_fn(|i| {
                if self.target_kind[i] == 0 {
                    None
                } else {
                    let mut a = NativeBounceActor::default();
                    a.set_callback(TARGET_ADDRESS);
                    a.set16(0xa, (i % 13) as i32 * 16 + 8);
                    a.raw[0x19] = (i / 13) as u8;
                    a.raw[0x1a] = (i % 13) as u8;
                    a.raw[0x1f] = (i / 13 * 16) as u8;
                    a.set32(
                        0x20,
                        if self.stage.is_multiple_of(2) ^ (i >= 13) {
                            16384
                        } else {
                            -16384
                        },
                    );
                    a.raw[0x1d] = (self.target_kind[i] - 1) * 16;
                    Some(a)
                }
            });
            self.target_master.set_callback(0x0200_de98);
            self.target_master.raw[0x1a] = 0;
        } else if self.flags & 0x80 == 0 {
            self.target_master.add8(0x19, 1);
            if self.target_master.raw[0x19] > 6 {
                self.target_master.raw[0x19] = 0;
                self.target_master.raw[0x1a] = self.target_master.raw[0x1a].wrapping_add(4) & 12;
            }
            self.target_master.set16(
                0xe,
                i32::from(self.target_master.world_y().wrapping_sub(self.scroll)),
            );
            self.target_sound = self.target_sound.saturating_sub(1);
        }
        for index in (0..TARGETS).rev() {
            let Some(mut a) = self.targets[index] else {
                continue;
            };
            if self.flags & 0x80 == 0 {
                if a.callback() == TARGET_BURST {
                    a.set16(
                        0xe,
                        i32::from(self.target_master.y()) + i32::from(a.raw[0x1f] as i8),
                    );
                    a.add8(0x1a, 1);
                    if a.raw[0x1a] > 5 {
                        a.raw[0x1a] = 0;
                        a.add8(0x19, 1);
                        if a.raw[0x19] > 5 {
                            self.targets[index] = None;
                            continue;
                        }
                    }
                    a.set16(
                        0x14,
                        i32::from(self.source.burst_tiles[usize::from(a.raw[0x19])]),
                    );
                } else {
                    let speed = a.i32(0x20);
                    a.set32(8, a.i32(8).wrapping_add(speed));
                    if speed < 0 && a.x() < -8 {
                        a.add16(0xa, 208);
                    } else if speed >= 0 && a.x() > 200 {
                        a.add16(0xa, -208);
                    }
                    a.set16(
                        0xe,
                        i32::from(self.target_master.y()) + i32::from(a.raw[0x1f] as i8),
                    );
                    if a.visible()
                        && a.x() >= self.hitbox[0]
                        && a.x() <= self.hitbox[1]
                        && a.y() >= self.hitbox[2]
                        && a.y() <= self.hitbox[3]
                    {
                        let cell = usize::from(a.raw[0x19]) * 13 + usize::from(a.raw[0x1a]);
                        self.rings = self.rings.wrapping_add(if self.target_kind[cell] > 1 {
                            10
                        } else {
                            1
                        });
                        self.target_kind[cell] = 0;
                        self.remaining = self.remaining.wrapping_sub(1);
                        self.target_sound = 30;
                        a.raw[0x1a] = 0;
                        a.raw[0x19] = 0;
                        a.set_callback(TARGET_BURST);
                        // DD60 calls DCC0 immediately on the collection frame.
                        a.add8(0x1a, 1);
                        a.set16(0x14, i32::from(self.source.burst_tiles[0]));
                    } else if a.visible() {
                        a.set16(
                            0x14,
                            i32::from(self.target_master.raw[0x1a]) + i32::from(a.raw[0x1d]),
                        );
                    }
                }
            }
            self.targets[index] = Some(a);
        }
    }
    fn spring_move(&self, a: &mut NativeBounceActor, buttons: NativeButtons) {
        a.raw[7] &= !4;
        if self.remaining == 0 {
            if self.chao.x() < a.x().wrapping_sub(3) {
                a.add16(0xa, -3);
                if a.x() <= 11 {
                    a.set16(0xa, 12);
                } else {
                    a.raw[7] = (a.flags() & !1) | 12;
                }
            } else if self.chao.x() > a.x().wrapping_add(2) {
                a.add16(0xa, 3);
                if a.x() > 180 {
                    a.set16(0xa, 180);
                } else {
                    a.raw[7] |= 13;
                }
            }
            return;
        }
        if buttons.held & 0x30 == 0 || a.flags() & 0x92 != 0 {
            return;
        }
        a.raw[7] |= 4;
        let speed = if buttons.held & 2 != 0 {
            a.raw[7] |= 8;
            196608
        } else {
            a.raw[7] &= !8;
            98304
        };
        if buttons.held & 32 != 0 {
            a.raw[7] &= !1;
            a.set32(8, a.i32(8).wrapping_sub(speed));
            if a.x() <= 11 {
                a.raw[7] &= !4;
                a.set16(0xa, 12);
            }
        } else {
            a.raw[7] |= 1;
            a.set32(8, a.i32(8).wrapping_add(speed));
            if a.x() > 180 {
                a.raw[7] &= !4;
                a.set16(0xa, 180);
            }
        }
    }
    fn spring_step(
        &mut self,
        a: &mut NativeBounceActor,
        buttons: NativeButtons,
    ) -> Result<(), Error> {
        if a.callback() == SPRING_INIT {
            self.right_visual_flags = 0;
            self.right_visual_phase = 0;
            a.set16(0xa, 128);
            a.raw[0x1d] = 1;
            a.raw[0x1e] = 255;
            a.set16(0x22, 0);
            a.raw[0x1f] = 0;
            a.set_callback(SPRING_WAIT);
            return Ok(());
        }
        if self.flags & 0x80 != 0 {
            return Ok(());
        }
        if a.callback() == SPRING_CLEAR {
            self.clear_step(a, buttons);
            return Ok(());
        }
        if a.callback() == SPRING_WAIT {
            a.set16(0xe, i32::from(319_i16.wrapping_sub(self.scroll)));
            if self.scroll > 159 {
                self.spring_move(a, buttons);
                self.chao.set16(0xa, i32::from(a.x().wrapping_sub(6)));
                if buttons.pressed & 1 == 0 {
                    a.add8(0x19, 1);
                }
                if buttons.pressed & 1 != 0 || a.raw[0x19] > 239 {
                    self.chao.raw[7] &= !4;
                    a.raw[7] |= 0xa0;
                    a.set16(0x14, 321);
                    a.raw[0x1a] = 0;
                    a.raw[0x19] = 0;
                    a.raw[0x1e] = 8;
                    self.spring_compress(a);
                    a.set_callback(SPRING_ADDRESS);
                }
            }
            return Ok(());
        }
        if self.chao.flags() & 0x80 != 0 {
            return Ok(());
        }
        a.set16(0xe, i32::from(319_i16.wrapping_sub(self.scroll)));
        self.spring_move(a, buttons);
        if a.flags() & 2 == 0 {
            if buttons.pressed & 1 != 0 && a.flags() & 0x10 == 0 && self.remaining != 0 {
                a.raw[7] |= 2;
                a.raw[0x19] = 12;
            }
        } else {
            if buttons.held & 1 != 0 {
                a.raw[0x19] = 12;
            }
            if a.flags() & 0x80 == 0 {
                a.add8(0x19, -1);
            }
            if a.raw[0x19] == 0 || self.remaining == 0 {
                a.raw[7] &= !2;
            }
        }
        if a.flags() & 0x80 != 0 && self.vy > 0 {
            if a.raw[0x1e] as i8 >= 0 {
                self.spring_compress(a);
            }
            return Ok(());
        }
        let mut y = if a.flags() & 2 != 0 { 298 } else { 303 };
        let x = i32::from(self.chao.x());
        let sx = i32::from(a.x());
        if (sx - 16..=sx + 15).contains(&x) {
            self.spring_contact(a, y, false);
        } else if (sx + 20..=sx + 43).contains(&x) {
            if a.flags() & 2 != 0 {
                y += 5;
            }
            self.spring_contact(a, y, true);
        } else if i32::from(self.chao.world_y()) > y {
            self.chao.raw[7] |= 0x80;
            self.chao.raw[0x1a] = 0;
            if self.remaining == 0 {
                self.chao.raw[7] |= 0x40;
            }
            a.raw[7] &= !2;
            return Ok(());
        }
        if a.raw[0x1e] as i8 >= 0 {
            self.spring_compress(a);
        }
        Ok(())
    }
    fn spring_contact(&mut self, a: &mut NativeBounceActor, y: i32, right: bool) {
        let sx = i32::from(a.x()) + if right { 33 } else { 0 };
        let cx = i32::from(self.chao.x());
        let (direction, margin) = if cx < sx - 8 {
            (0, 7)
        } else if cx < sx - 2 {
            (1, 8)
        } else if cx > sx + 7 {
            (4, 7)
        } else if cx > sx + 1 {
            (3, 8)
        } else {
            (2, 9)
        };
        if i32::from(self.chao.world_y()) <= y - margin {
            return;
        }
        a.raw[0x1d] = direction;
        self.chao.raw[7] |= 1;
        self.chao
            .set32(0x10, 0x0202_e4e8 + i32::from(self.chao.raw[0x1d]) * 8);
        self.chao.set16(0x14, 338);
        if right {
            a.raw[7] = (a.flags() | 0x90) & !2;
            a.raw[0x19] = 0;
            self.right_visual_flags = 1;
        } else {
            a.raw[7] = (a.flags() & !0x10) | 0x80;
        }
        a.set16(0x14, 191);
        a.raw[0x1a] = 0;
        a.raw[0x1e] = 8;
        if !right && self.remaining == 0 {
            self.chao.set32(0x10, 0x0202_e4e8);
            a.raw[7] &= !2;
            a.raw[0x1c] = 0;
            a.set_callback(SPRING_CLEAR);
        }
    }
    fn spring_compress(&mut self, a: &mut NativeBounceActor) {
        let mut release = false;
        if self.remaining == 0 {
            a.add16(0x14, 16);
            a.add8(0x1a, 1);
            if a.raw[0x1a] > 7 {
                a.raw[0x1a] = 0;
                a.add8(0x1e, -1);
            }
        } else if a.u16(0x14) > 256 {
            if a.flags() & 0x80 != 0 {
                a.add16(0x14, 18);
                if a.u16(0x14) > 382 {
                    a.set16(0x14, 384);
                    self.spring_launch(a);
                    release = true;
                }
            } else {
                a.add16(0x14, 25);
                a.add8(0x1a, 1);
                if a.raw[0x1a] > 4 {
                    a.raw[0x1a] = 0;
                    a.add8(0x1e, -1);
                }
            }
        } else if a.u16(0x14) > 250 {
            if a.flags() & 2 == 0 {
                a.add8(0x1a, 1);
            }
            if a.flags() & 2 != 0 || a.raw[0x1a] != 0 {
                a.raw[0x1a] = 0;
                a.add16(0x14, 6);
            }
        } else {
            a.add16(0x14, 6);
        }
        a.raw[0x1f] = (-(self.source.cos(a.raw[0x14]) * i32::from(a.raw[0x1e] as i8) / 256)) as u8;
        if a.flags() & 0x80 != 0 {
            let offset = i32::from(self.source.spring_offsets[usize::from(a.raw[0x1d])]);
            let y = 159
                - offset
                - i32::from(a.raw[0x1f] as i8)
                - if a.flags() & 2 != 0 { 5 } else { 0 };
            self.chao.set16(0xe, y);
            if a.flags() & 2 != 0 && a.u16(0x22) == 0 {
                a.set16(0x22, i32::from(a.u16(0x14)));
            }
            self.chao.set16(0x22, y + 160);
            if release {
                a.raw[7] &= !0x80;
                self.chao.set16(0x20, 0);
                a.set16(0x22, 0);
            }
        }
    }
    fn spring_child_step(&mut self) {
        if self.flags & 0x80 != 0 || self.right_visual_flags & 1 == 0 {
            return;
        }
        if self.right_visual_phase != 0 {
            if self.right_visual_flags & 2 == 0 {
                self.right_visual_flags |= 2;
                self.spring.raw[0x1e] = 255;
                self.spring.raw[0x1f] = 0;
            }
            self.right_visual_phase = self.right_visual_phase.wrapping_add(6);
            if (self.right_visual_phase as i8) < 0 {
                self.right_visual_phase = 0;
                self.right_visual_flags = 0;
                self.spring.raw[7] &= !0x10;
            }
        } else if self.spring.flags() & 0x10 != 0 {
            self.right_visual_flags |= 2;
            if self.spring.raw[0x1f] as i8 > 0 && self.spring.flags() & 0x80 == 0 {
                self.right_visual_flags &= !2;
                self.right_visual_phase = 1;
            }
        }
    }
    fn spring_launch(&mut self, a: &NativeBounceActor) {
        let scale = if a.flags() & 0x10 != 0 {
            0x1eed9
        } else if a.u16(0x22) == 0 || a.u16(0x22) == 197 {
            0x293ce
        } else {
            let t = match a.u16(0x22) {
                245 => 28,
                239 | 329 => 56,
                233 => 84,
                227 => 112,
                221 | 347 => 140,
                215 => 168,
                209 => 197,
                251 | 257 | 275 | 293 | 311 => 0,
                _ => 225,
            };
            0x40bf8 - (0x1782a * t / 256)
        };
        let angle = a.raw[0x1d].wrapping_mul(4).wrapping_add(184);
        self.vy = self.source.sin(angle).wrapping_mul(scale) / 256;
        self.vx = self.source.cos(angle).wrapping_mul(scale) / 256;
        if a.raw[0x1d] <= 1 {
            self.chao.raw[0x1d] = 0;
        } else if a.raw[0x1d] > 2 {
            self.chao.raw[0x1d] = 1;
        }
        self.chao.raw[7] &= !1;
        self.chao
            .set32(0x10, 0x0202_e4c8 + i32::from(self.chao.raw[0x1d]) * 8);
        self.chao.set16(0x14, 310);
    }
    fn clear_step(&mut self, a: &mut NativeBounceActor, buttons: NativeButtons) {
        if a.raw[0x1c] != 0 {
            let t = i32::from(a.i16(0x20));
            match a.raw[0x1c] {
                1 => {
                    a.raw[0x1c] = 2;
                    a.set16(0x20, 0);
                    a.add16(0x20, 1);
                }
                2 => {
                    a.add16(0x20, 1);
                    if 96 + t * t / 4 > 184 {
                        a.raw[0x1c] += 1;
                        a.set16(0x20, 0);
                    }
                }
                3 => {
                    a.add16(0x20, 1);
                    if a.i16(0x20) > 7 {
                        a.raw[0x1c] += 1;
                        a.set16(0x20, 0);
                    }
                }
                4 => {
                    a.add16(0x20, 1);
                    if a.i16(0x20) > 15 {
                        a.raw[0x1c] += 1;
                    }
                }
                5 => {
                    a.add16(0x20, -1);
                    if a.i16(0x20) < 0 {
                        a.raw[0x1c] += 1;
                        a.set16(0x20, 0);
                    }
                }
                _ => {
                    if buttons.pressed & 9 == 0 {
                        a.add16(0x20, 1);
                    }
                    if buttons.pressed & 9 != 0 || a.i16(0x20) > 179 {
                        self.flags |= 2;
                        self.pending = Some(Return::Garden);
                        self.won = true;
                    }
                }
            }
        } else if a.raw[0x1e] as i8 >= 0 {
            self.spring_compress(a);
        } else if a.raw[0x1e] as i8 >= -4 {
            self.chao.set16(0x14, 346);
            a.raw[0x1a] = 0;
            a.raw[0x1e] = 248;
            if self.stage > 2 {
                a.raw[0x1c] = 1;
                self.flags |= 0x10;
            }
        } else {
            a.add8(0x1a, 1);
            if a.raw[0x1a] > 89 {
                self.flags |= 2;
                self.pending = Some(Return::NextStage);
            }
        }
    }
    fn cloud_step(&mut self) {
        let heights = match self.stage {
            0 => [232, 204, 228],
            1 => [228, 194, 221],
            2 => [225, 184, 215],
            _ => [222, 174, 208],
        };
        for (index, cloud) in self.clouds.iter_mut().enumerate() {
            if cloud.callback() == 0x0200_f048 {
                let x = index as i32 * 60 + i32::from(self.stage & 3) * 30 + 16;
                cloud.set16(0xa, if x > 216 { x - 240 } else { x });
                cloud.set16(0x22, heights[index]);
                cloud.raw[0x19] = index as u8;
                cloud.set16(0x14, 60);
                cloud.set_callback(0x0200_efa0);
            }
            cloud.set16(0xe, i32::from(cloud.world_y().wrapping_sub(self.scroll)));
            if self.flags & 0x80 == 0 {
                let speed = if index & 1 == 0 { 6400 } else { 10496 };
                cloud.set32(
                    8,
                    if self.stage & 1 != 0 {
                        cloud.i32(8).wrapping_sub(speed)
                    } else {
                        cloud.i32(8).wrapping_add(speed)
                    },
                );
                if self.stage & 1 != 0 && cloud.x() < -24 {
                    cloud.add16(0xa, 240);
                } else if self.stage & 1 == 0 && cloud.x() > 216 {
                    cloud.add16(0xa, -240);
                }
            }
        }
    }
    fn bumper_step(
        &mut self,
        a: &mut NativeBounceActor,
        bios: &impl BiosMath,
    ) -> Result<(), Error> {
        if a.callback() == 0x0200_f4c0 {
            a.set16(
                0xa,
                if self.stage == 2 || a.raw[0x1c] != 0 {
                    96
                } else {
                    120
                },
            );
            a.set16(
                0x22,
                if a.raw[0x1c] != 0 {
                    176
                } else if self.stage == 2 {
                    152
                } else {
                    80
                },
            );
            a.raw[0x1f] = if a.raw[0x1c] != 0 { 64 } else { 192 };
            a.raw[0x1e] = if a.raw[0x1c] != 0 { 255 } else { 1 };
            a.set_callback(0x0200_f11c);
            return Ok(());
        }
        if self.flags & 0x80 != 0 {
            return Ok(());
        }
        a.raw[0x19] = a.raw[0x19].saturating_sub(1);
        a.set32(
            8,
            if a.raw[0x1c] != 0 {
                a.i32(8).wrapping_sub(20480)
            } else {
                a.i32(8).wrapping_add(20480)
            },
        );
        if a.raw[0x1c] != 0 && a.x() < -24 {
            a.add16(0xa, 240);
        } else if a.raw[0x1c] == 0 && a.x() > 216 {
            a.add16(0xa, -240);
        }
        a.add8(0x1f, a.raw[0x1e] as i8);
        if (a.raw[0x1e] as i8 > 0 && a.raw[0x1f] as i8 > 63)
            || (a.raw[0x1e] as i8 <= 0 && a.raw[0x1f] as i8 <= -64)
        {
            a.raw[0x1e] = (a.raw[0x1e] as i8).wrapping_neg() as u8;
        }
        a.set16(0xe, i32::from(a.world_y()));
        a.set32(
            0xc,
            a.i32(0xc).wrapping_add(i32::from(a.raw[0x1f] as i8) * 256),
        );
        a.set16(0x22, i32::from(a.y()));
        a.add16(0xe, -i32::from(self.scroll));
        if a.flags() != 0 {
            a.add16(0x20, 9);
            if a.u16(0x20) > 255 {
                a.add8(0x1a, 1);
                if a.raw[0x1a] > 11 {
                    a.raw[0x1a] = 0;
                    a.add8(0x1d, -1);
                    if a.raw[0x1d] == 0 {
                        a.raw[7] = 0;
                    }
                }
            }
        }
        if !a.visible() {
            return Ok(());
        }
        let dx = self.chao.x().wrapping_sub(a.x());
        let dy = self.chao.y().wrapping_sub(a.y().wrapping_add(7));
        let distance = bios
            .sqrt(
                (i32::from(dx).wrapping_mul(i32::from(dx)))
                    .wrapping_add(i32::from(dy).wrapping_mul(i32::from(dy))) as u32,
            )
            .map_err(|_| Error::BiosUnavailable)?;
        if distance > 19 {
            return Ok(());
        }
        let angle = (bios
            .atan2(dx.wrapping_mul(4), dy.wrapping_mul(4))
            .map_err(|_| Error::BiosUnavailable)?
            >> 8) as u8;
        self.chao
            .set16(0xa, i32::from(a.x()) + self.source.cos(angle) * 22 / 256);
        let y = i32::from(a.y()) + 7 + self.source.sin(angle) * 22 / 256;
        self.chao.set16(0xe, y);
        self.chao.set16(8, i32::from(a.u16(8)));
        self.chao.set16(0xc, i32::from(a.u16(0xc)));
        self.chao.set16(0x22, y + i32::from(self.scroll));
        a.raw[7] = if (32..=96).contains(&angle) || (160..=224).contains(&angle) {
            1
        } else {
            2
        };
        let qx = (self.vx / 256) as i16;
        let qy = (self.vy / 256) as i16;
        let movement = (bios
            .atan2(qx.wrapping_mul(4), qy.wrapping_mul(4))
            .map_err(|_| Error::BiosUnavailable)?
            >> 8) as u8;
        let speed = bios
            .sqrt(
                (i32::from(qx).wrapping_mul(i32::from(qx)))
                    .wrapping_add(i32::from(qy).wrapping_mul(i32::from(qy))) as u32,
            )
            .map_err(|_| Error::BiosUnavailable)?;
        let normal = angle.wrapping_add(64);
        let reflection = normal.wrapping_sub(movement.wrapping_sub(normal));
        self.vx = self
            .source
            .cos(reflection)
            .wrapping_mul(i32::from(speed))
            .wrapping_add(if a.raw[0x1c] != 0 { -20480 } else { 20480 });
        self.vy = self
            .source
            .sin(reflection)
            .wrapping_mul(i32::from(speed))
            .wrapping_add(i32::from(a.raw[0x1f] as i8) * 256);
        self.chao.raw[0x1d] = u8::from(self.vx >= 0);
        self.chao
            .set32(0x10, 0x0202_e4c8 + i32::from(self.chao.raw[0x1d]) * 8);
        if a.raw[0x19] == 0 {
            a.raw[0x19] = 15;
        }
        a.raw[0x1a] = 0;
        a.set16(0x20, 0);
        a.raw[0x1d] = ((speed >> 8) * 2 + 2).min(12) as u8;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_bios_math::{ReferenceBiosMath, RejectBiosMath};
    fn synthetic() -> NativeBounceRound {
        NativeBounceRound::begin(A2neBounceSource {
            trig: [0; 256],
            spring_offsets: [0; 5],
            burst_tiles: [0; 6],
        })
    }
    #[test]
    fn native_allocator_clears_fractional_positions_and_keeps_integer_and_opaque_tail() {
        let before = NativeBounceActor {
            raw: std::array::from_fn(|i| i as u8),
        };
        let after = before.reallocated();
        for i in 0..ACTOR_LEN {
            let expected = if i == 7
                || (8..10).contains(&i)
                || (12..14).contains(&i)
                || (0x16..0x1a).contains(&i)
            {
                0
            } else if i == 0x1c {
                255
            } else {
                before.raw[i]
            };
            assert_eq!(after.raw[i], expected, "byte{i}");
        }
    }
    #[test]
    fn jewel_collection_awards_ten_exactly_once_before_its_native_burst_finishes() {
        let mut round = synthetic();
        round.target_master.set_callback(0x0200_de98);
        round.targets = [None; TARGETS];
        round.remaining = 1;
        round.target_kind = [0; TARGETS];
        round.target_kind[6] = 2;
        round.hitbox = [-1, 1, -1, 1];
        let mut target = NativeBounceActor::default();
        target.set_callback(TARGET_ADDRESS);
        target.raw[0x1a] = 6;
        round.targets[6] = Some(target);
        round.target_step();
        assert_eq!(
            (round.rings, round.remaining, round.target_kind[6]),
            (10, 0, 0)
        );
        assert_eq!(round.targets[6].unwrap().callback(), TARGET_BURST);
        for _ in 0..35 {
            round.target_step();
        }
        assert_eq!(round.rings, 10);
        assert!(round.targets[6].is_none());
    }
    #[test]
    fn native_a_dismiss_keeps_result_counter_and_completion_waits_full_fade() {
        let mut round = synthetic();
        round.intro = 0;
        round.flags = 0x10;
        round.chao.set_callback(CHAO_LOSS);
        round.chao.raw[7] = 0x90;
        round.chao.raw[0x1d] = 4;
        round.chao.set16(0x20, 31);
        round
            .step(
                NativeButtons {
                    pressed: 1,
                    held: 1,
                },
                &ReferenceBiosMath,
            )
            .unwrap();
        assert_eq!(round.chao.u16(0x20), 31);
        assert_eq!(round.finished(), None);
        for _ in 0..15 {
            round
                .step(NativeButtons::default(), &ReferenceBiosMath)
                .unwrap();
            assert_eq!(round.finished(), None);
        }
        round
            .step(NativeButtons::default(), &ReferenceBiosMath)
            .unwrap();
        assert_eq!(round.finished(), Some((false, 0)));
        let before = round.view();
        round
            .step(
                NativeButtons {
                    pressed: 1,
                    held: 1,
                },
                &ReferenceBiosMath,
            )
            .unwrap();
        assert_eq!(round.view(), before);
    }
    #[test]
    fn first_miss_spends_only_one_life_and_uses_native_restart_dispatch_frame() {
        let mut round = synthetic();
        round.intro = 0;
        round.flags = 0;
        round.chao.set_callback(CHAO_ADDRESS);
        round.chao.raw[7] = 0x80;
        round.chao.raw[0x1a] = 89;
        round.chao.set16(0x22, 319);
        round
            .step(NativeButtons::default(), &ReferenceBiosMath)
            .unwrap();
        assert_eq!(round.lives, 2);
        assert_eq!(round.finished(), None);
        for _ in 0..16 {
            round
                .step(NativeButtons::default(), &ReferenceBiosMath)
                .unwrap();
        }
        assert_eq!(round.flags & 2, 2);
        assert_ne!(round.chao.callback(), CHAO_INIT);
        round
            .step(NativeButtons::default(), &ReferenceBiosMath)
            .unwrap();
        assert_eq!(round.chao.callback(), CHAO_INIT);
        assert_eq!(round.flags, 1);
        assert_eq!(round.scroll, 160);
    }
    #[test]
    fn unavailable_bios_rejects_the_entire_candidate_frame() {
        let mut round = synthetic();
        round.stage = 2;
        let mut bumper = NativeBounceActor::default();
        bumper.set_callback(0x0200_f11c);
        bumper.set16(0x22, 152);
        bumper.raw[0x1e] = 1;
        round.bumpers[0] = Some(bumper);
        let before = round.view();
        let chao = round.chao;
        let spring = round.spring;
        let vx = round.vx;
        let vy = round.vy;
        assert_eq!(
            round.step(NativeButtons::default(), &RejectBiosMath),
            Err(Error::BiosUnavailable)
        );
        assert_eq!(round.view(), before);
        assert_eq!(round.chao, chao);
        assert_eq!(round.spring, spring);
        assert_eq!((round.vx, round.vy), (vx, vy));
    }
    #[test]
    fn right_spring_child_keeps_native_lockout_and_resets_power_on_its_next_update() {
        let mut round = synthetic();
        round.flags = 0;
        round.right_visual_flags = 1;
        round.right_visual_phase = 1;
        round.spring.raw[7] = 0x10;
        round.spring.raw[0x1e] = 8;
        round.spring.raw[0x1f] = 6;
        round.spring_child_step();
        assert_eq!((round.spring.raw[0x1e], round.spring.raw[0x1f]), (255, 0));
        assert_eq!(round.spring.flags() & 0x10, 0x10);
        for _ in 0..21 {
            round.spring_child_step();
        }
        assert_eq!(round.spring.flags() & 0x10, 0);
        assert_eq!(round.right_visual_flags, 0);
    }
}
