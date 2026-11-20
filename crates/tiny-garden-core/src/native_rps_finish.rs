//! Original ASOE Janken result transition: mode2 animation, mode4 wait and
//! mode5 fade at02008744/020087CC. Only scalar phases/input timing are ported;
//! original graphics/audio remain presentation resources. Native targets keep
//! updating until this transition completes, preserving delayed score awards.
use super::{
    native_growth::NativeRng,
    native_rps_game::{BiosAngle, Error, NativeRpsAssets, NativeRpsRound, RoundEvents},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinishError {
    Game(Error),
    InvalidPhase,
    UnsupportedPause,
}
impl From<Error> for FinishError {
    fn from(value: Error) -> Self {
        Self::Game(value)
    }
}
impl std::fmt::Display for FinishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FinishError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeRpsFinish {
    pub mode: u8,
    pub phase: u8,
    pub counter: u16,
}
impl Default for NativeRpsFinish {
    fn default() -> Self {
        Self {
            mode: 2,
            phase: 0,
            counter: 0,
        }
    }
}
impl NativeRpsFinish {
    /// Original scalar callback decision. A/Start can shorten only the result
    /// wait; the native63-update animation and17-update final fade still run.
    pub fn tick(&mut self, keys: u16) -> Result<bool, FinishError> {
        let mut next = *self;
        let completed = match next.mode {
            2 => {
                match next.phase {
                    0 => {
                        next.phase = 1;
                        next.counter = 0;
                    }
                    1 => {
                        let count = i32::from(next.counter as i16);
                        let x = 280 - (count * count) / 4;
                        next.counter = next.counter.wrapping_add(1);
                        if x <= 183 {
                            next.phase = 2;
                            next.counter = 0;
                        }
                    }
                    2 => {
                        next.counter = next.counter.wrapping_add(1);
                        if next.counter == 8 {
                            next.phase = 3;
                            next.counter = 0;
                        }
                    }
                    3 => {
                        next.counter = next.counter.wrapping_add(1);
                        if next.counter == 16 {
                            next.phase = 4;
                        }
                    }
                    4 => {
                        next.counter = next.counter.wrapping_sub(1);
                        if (next.counter as i16) < 0 {
                            next.mode = 4;
                            next.counter = 0;
                        }
                    }
                    _ => return Err(FinishError::InvalidPhase),
                }
                false
            }
            4 => {
                let exit = if keys & 9 != 0 {
                    true
                } else {
                    next.counter = next.counter.wrapping_add(1);
                    next.counter == 180
                };
                if exit {
                    next.mode = 5;
                    next.counter = 0;
                }
                false
            }
            5 => {
                let done = next.counter == 16;
                next.counter = next.counter.wrapping_add(1);
                done
            }
            _ => return Err(FinishError::InvalidPhase),
        };
        *self = next;
        Ok(completed)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeRpsSession {
    pub round: NativeRpsRound,
    pub finish: Option<NativeRpsFinish>,
    /// Final native round score only. Durable wallet policy is a host adapter.
    pub result: Option<u8>,
}
impl NativeRpsSession {
    pub fn initialize(&mut self, rng: &mut NativeRng) {
        self.round.initialize(rng);
        self.finish = None;
        self.result = None;
    }
    /// One eligible source update. Start is handled only during result wait;
    /// the original active pause menu/task is not silently simulated here.
    pub fn tick(
        &mut self,
        keys: u16,
        assets: &NativeRpsAssets,
        bios: &impl BiosAngle,
        rng: &mut NativeRng,
    ) -> Result<SessionEvents, FinishError> {
        if keys & !0x39 != 0 {
            return Err(Error::InvalidInput.into());
        }
        if self.result.is_some() {
            return Ok(SessionEvents::default());
        }
        if !self.round.finishing {
            if keys & 8 != 0 {
                return Err(FinishError::UnsupportedPause);
            }
            return self
                .round
                .tick_active(keys, assets, bios, rng)
                .map(|active| SessionEvents {
                    active,
                    ..SessionEvents::default()
                })
                .map_err(Into::into);
        }
        let (mut next, mut next_rng) = (self.clone(), *rng);
        // Phase3 sets native mode2/phase0, changes to5 and falls through to
        // A8EC+87CC in the same source update. Each later phase5 does both.
        let targets = next.round.waves.tick_targets(&mut next_rng);
        let done = next
            .finish
            .get_or_insert_with(NativeRpsFinish::default)
            .tick(keys)?;
        if done {
            next.result = Some(next.round.waves.round_rings);
        }
        next.round.launcher.tick_shot_display();
        let events = SessionEvents {
            finish_targets: targets,
            completed: done,
            ..SessionEvents::default()
        };
        (*self, *rng) = (next, next_rng);
        Ok(events)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionEvents {
    pub active: RoundEvents,
    pub finish_targets: super::native_rps::NativeRpsWaveUpdate,
    pub completed: bool,
}
