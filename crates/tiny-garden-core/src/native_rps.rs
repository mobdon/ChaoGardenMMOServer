//! Original ASOE Tiny Chao Garden Janken clock and target-wave subsystem.
//!
//! Ports 0200A77C/A798/A7C0, A800/A8C8/A8EC and round-score helper 8128.
//! This is not the full game: cannon input, projectiles, collision/reflection,
//! render/OAM, sound playback, task scheduling and wallet settlement are absent.
//! Callers own eligible native updates and authoritative RNG. No client gains
//! or wall clocks are accepted, and round rings are not a durable MMO award.
use super::native_growth::NativeRng;

pub const TARGET_COUNT: usize = 10;
pub const TARGET_RECORD_SIZE: usize = 12;
pub const NATIVE_CONTROLLER: u32 = 0x0200_b4e0;
pub const NATIVE_TARGET_UPDATE: u32 = 0x0200_a8ec;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeRpsClock {
    pub seconds: u8,
    pub subframes: u8,
}

impl NativeRpsClock {
    /// Original A77C: the low input byte is stored without a new clamp.
    pub fn set(&mut self, seconds: u8) {
        self.seconds = seconds;
        self.subframes = 0;
    }

    /// Original A798 uses signed current seconds and caps results above 98
    /// at 99:00. Otherwise it preserves the existing subframe byte.
    pub fn add_seconds(&mut self, delta: i32) {
        let next = i32::from(self.seconds as i8).wrapping_add(delta);
        if next > 98 {
            self.seconds = 99;
            self.subframes = 0;
        } else {
            self.seconds = next as u8;
        }
    }

    /// Original A7C0. At 30:00, update1800 reaches 00:00; update1801 reports
    /// expiry. Preserve that boundary instead of replacing it with a timer.
    pub fn tick(&mut self) -> bool {
        self.subframes = self.subframes.wrapping_sub(1);
        if (self.subframes as i8) < 0 {
            self.subframes = 59;
            self.seconds = self.seconds.wrapping_sub(1);
            if (self.seconds as i8) < 0 {
                self.seconds = 0;
                self.subframes = 0;
                return true;
            }
        }
        false
    }
}

/// The native twelve-byte record is retained in full. Initializing a wave
/// changes only state, gesture and route; counter/position/velocity and opaque
/// byte3 must survive. Numeric gestures remain unlabeled until art is traced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeRpsTarget {
    pub raw: [u8; TARGET_RECORD_SIZE],
}

impl NativeRpsTarget {
    pub fn state(self) -> u8 {
        self.raw[0]
    }

    pub fn gesture(self) -> u8 {
        self.raw[2]
    }

    pub fn route(self) -> i16 {
        self.half(4)
    }

    pub fn position(self) -> (i16, i16) {
        (self.half(6), self.half(8))
    }

    fn half(self, offset: usize) -> i16 {
        i16::from_le_bytes(self.raw[offset..offset + 2].try_into().unwrap())
    }

    fn set_half(&mut self, offset: usize, value: i16) {
        self.raw[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeRpsWaves {
    pub clock: NativeRpsClock,
    pub targets: [NativeRpsTarget; TARGET_COUNT],
    pub wave: u8,
    pub route_speed: u8,
    pub active_targets: u8,
    pub unhit_targets: u8,
    pub round_rings: u8,
}

/// Native render/sound decisions, without drawing replacement sprites or
/// playing substitute effects. Masks use the original record order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeRpsWaveUpdate {
    pub appeared: u16,
    pub hidden: u16,
    pub defeated_removed: u16,
    pub tie_removed: u16,
    pub tie_blink_visible: u16,
    pub tie_blink_hidden: u16,
    pub wave_started: bool,
    pub random_calls: u8,
}

impl NativeRpsWaves {
    /// Original A8C8 followed by A800; consumes exactly ten RNG draws.
    pub fn initialize(&mut self, rng: &mut NativeRng) {
        self.wave = 0xff;
        self.route_speed = 2;
        self.next_wave(rng);
    }

    /// Original A800. Does not reset record counters or other stale bytes.
    pub fn next_wave(&mut self, rng: &mut NativeRng) {
        self.wave = self.wave.wrapping_add(1);
        self.active_targets = TARGET_COUNT as u8;
        self.unhit_targets = TARGET_COUNT as u8;
        for (index, target) in self.targets.iter_mut().enumerate() {
            target.raw[0] = 0;
            target.set_half(4, -(index as i16) * 32 - i16::from(self.route_speed as i8));
            target.raw[2] = (rng.next_u15() / 0x2aab) as u8;
        }
        if self.wave == 0 {
            self.clock.set(30);
        } else {
            self.clock.add_seconds(10);
        }
    }

    /// Original 8128 wraps to a byte before its signed cap comparison.
    /// This value is the native minigame score, not the canonical wallet.
    pub fn add_round_rings(&mut self, delta: i32) {
        self.round_rings = i32::from(self.round_rings).wrapping_add(delta) as u8;
        if (self.round_rings as i8) > 99 {
            self.round_rings = 99;
        }
    }

    /// Original A8EC. This does not run the countdown or collision callbacks;
    /// the controller calls those separately and their ordering is preserved.
    pub fn tick_targets(&mut self, rng: &mut NativeRng) -> NativeRpsWaveUpdate {
        let mut update = NativeRpsWaveUpdate::default();
        let mut awards = 0_u8;
        for (index, target) in self.targets.iter_mut().enumerate() {
            let bit = 1 << index;
            let counter = target.raw[1] as i8;
            target.raw[1] = target.raw[1].wrapping_add(1);
            match target.state() {
                2 => {
                    if counter > 29 {
                        target.raw[0] = 4;
                        self.active_targets = self.active_targets.wrapping_sub(1);
                        update.defeated_removed |= bit;
                        update.hidden |= bit;
                        awards += 1;
                    }
                    // The native routine still moves the record on the frame
                    // which changes state2 to4. Its x-window uses unsigned >.
                    let (x, y) = target.position();
                    if (i32::from(x) + 32) as u32 <= 271 && (-32..=159).contains(&y) {
                        target.set_half(6, x.wrapping_add(i16::from(target.raw[10] as i8)));
                        target.set_half(8, y.wrapping_add(i16::from(target.raw[11] as i8)));
                    }
                }
                3 => {
                    if counter > 29 {
                        target.raw[0] = 4;
                        self.active_targets = self.active_targets.wrapping_sub(1);
                        update.tie_removed |= bit;
                        update.hidden |= bit;
                    }
                    // Native blink decision occurs even on its removal frame.
                    match counter & 7 {
                        0 => update.tie_blink_hidden |= bit,
                        3 => update.tie_blink_visible |= bit,
                        _ => {}
                    }
                }
                4 => {}
                _ => {
                    let old = target.route();
                    let speed = i16::from(self.route_speed as i8);
                    if old < 0 && i32::from(old) + i32::from(speed) >= 0 {
                        update.appeared |= bit;
                    }
                    let mut route = old.wrapping_add(speed);
                    target.set_half(4, route);
                    if route < 0 {
                        update.hidden |= bit;
                        continue;
                    }
                    if route > 392 {
                        route -= 392;
                        target.set_half(4, route);
                    }
                    let (x, y) = match route {
                        ..=63 => (10, route + 8),
                        64..=195 => (route - 54, 72),
                        196..=259 => (142, 268 - route),
                        _ => (402 - route, 8),
                    };
                    target.set_half(6, x);
                    target.set_half(8, y);
                }
            }
        }
        // The original helper is invoked once per removal; retaining byte
        // wrapping and signed clamping requires the same separate operations.
        for _ in 0..awards {
            self.add_round_rings(i32::from(self.wave as i8) / 2 + 1);
        }
        if self.active_targets == 0 {
            self.next_wave(rng);
            update.wave_started = true;
            update.random_calls = TARGET_COUNT as u8;
            update.hidden = (1 << TARGET_COUNT) - 1;
        }
        self.route_speed = if (self.unhit_targets as i8) <= 1 {
            4
        } else {
            2
        };
        update
    }
}
