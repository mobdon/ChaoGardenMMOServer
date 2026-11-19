//! Pure A2NE Tiny Chao Garden fruit-growth arithmetic.
//!
//! This is the bounded stat operation at native 0200493C..02004A4A, not the
//! surrounding food selection, chewing, animation, inventory or save engine.
//! The caller supplies a verified seven-byte fruit row and owns authoritative
//! RNG/save transactions. This module never reads a ROM or trusts phone gains.

use std::{fmt, num::NonZeroU16};

pub const SKILL_COUNT: usize = 5;
pub const SKILL_NAMES: [&str; SKILL_COUNT] = ["swim", "fly", "run", "power", "stamina"];
pub const NATIVE_FIELDS_END: usize = 0x9a;
pub const NATIVE_OPERATION_ADDRESS: u32 = 0x0200_493c;
pub const NATIVE_OPERATION_END: u32 = 0x0200_4a4a;

/// Native sub_02001528/sub_02001534/sub_02001554. The seed is caller-owned;
/// no wall clock, operating-system randomness or floating point is involved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeRng {
    state: u32,
}

impl NativeRng {
    pub const fn new(state: u32) -> Self {
        Self { state }
    }

    pub const fn state(self) -> u32 {
        self.state
    }

    pub fn next_u15(&mut self) -> u16 {
        self.state = self.state.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
        // The original u32 left shift discards bit31 before the right shift.
        (self.state.wrapping_mul(2) >> 17) as u16
    }

    /// Positive-bound native Mod result. Zero is excluded because the GBA
    /// BIOS division-by-zero path is not a supported source of growth rolls.
    pub fn bounded(&mut self, bound: NonZeroU16) -> u16 {
        self.next_u15() % bound
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeSkill {
    pub experience: u8,
    pub grade: u8,
    pub level: u8,
    pub points: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeGrowth {
    pub mood: u8,
    pub belly: u8,
    pub skills: [NativeSkill; SKILL_COUNT],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TruncatedNativeFields {
    pub actual: usize,
    pub minimum: usize,
}

impl fmt::Display for TruncatedNativeFields {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "native growth fields need {} bytes, got {}",
            self.minimum, self.actual
        )
    }
}
impl std::error::Error for TruncatedNativeFields {}

fn check_record(bytes: &[u8]) -> Result<(), TruncatedNativeFields> {
    if bytes.len() < NATIVE_FIELDS_END {
        Err(TruncatedNativeFields {
            actual: bytes.len(),
            minimum: NATIVE_FIELDS_END,
        })
    } else {
        Ok(())
    }
}

impl NativeGrowth {
    /// Known fields relative to native gSaveGameState, not a new save schema.
    /// This does not validate a sector checksum or authorize imported values.
    pub fn read_raw_fields(record: &[u8]) -> Result<Self, TruncatedNativeFields> {
        check_record(record)?;
        Ok(Self {
            mood: record[0x73],
            belly: record[0x74],
            skills: std::array::from_fn(|index| NativeSkill {
                experience: record[0x78 + index],
                grade: record[0x80 + index],
                level: record[0x88 + index],
                points: u16::from_be_bytes([record[0x90 + index * 2], record[0x91 + index * 2]]),
            }),
        })
    }

    /// Changes only traced fields, preserving every opaque byte. The caller
    /// must separately reseal a native checksum before writing a native save;
    /// this is not a durable server-save or client-import operation.
    pub fn write_raw_fields(self, record: &mut [u8]) -> Result<(), TruncatedNativeFields> {
        check_record(record)?;
        record[0x73] = self.mood;
        record[0x74] = self.belly;
        for (index, skill) in self.skills.iter().enumerate() {
            record[0x78 + index] = skill.experience;
            record[0x80 + index] = skill.grade;
            record[0x88 + index] = skill.level;
            record[0x90 + index * 2..0x92 + index * 2].copy_from_slice(&skill.points.to_be_bytes());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkillChange {
    pub leveled_up: bool,
    pub random_roll: Option<u8>,
    pub points_added: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrowthChange {
    pub skills: [SkillChange; SKILL_COUNT],
    pub random_calls: u8,
    pub seed_after: u32,
}

/// Rejected gameplay state. The raw native reader/operation deliberately remain
/// byte-faithful; authoritative adapters must use the checked boundary below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidGrowthState {
    pub skill: Option<usize>,
    pub field: &'static str,
    pub value: u16,
    pub maximum: u16,
}

impl fmt::Display for InvalidGrowthState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "growth field {} ({:?}) is {}, maximum {}",
            self.field, self.skill, self.value, self.maximum
        )
    }
}
impl std::error::Error for InvalidGrowthState {}

impl NativeGrowth {
    /// Validates gameplay ranges without silently repairing corrupt/imported
    /// state. This is not ownership, checksum, grade-provenance or save validation.
    pub fn validate_gameplay_bounds(&self) -> Result<(), InvalidGrowthState> {
        for (field, value) in [("mood", self.mood), ("belly", self.belly)] {
            if value > 200 {
                return Err(InvalidGrowthState {
                    skill: None,
                    field,
                    value: u16::from(value),
                    maximum: 200,
                });
            }
        }
        for (index, skill) in self.skills.iter().enumerate() {
            for (field, value, maximum) in [
                ("experience", u16::from(skill.experience), 99),
                ("grade", u16::from(skill.grade), 5),
                ("level", u16::from(skill.level), 99),
                ("points", skill.points, 9999),
            ] {
                if value > maximum {
                    return Err(InvalidGrowthState {
                        skill: Some(index),
                        field,
                        value,
                        maximum,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Applies the exact operation only when both initial and resulting state have
/// valid gameplay ranges. Failure changes neither stats nor the caller's RNG.
/// Ratings must still come from a verified server-owned fruit catalog.
pub fn apply_fruit_checked(
    growth: &mut NativeGrowth,
    ratings: [i8; 7],
    rng: &mut NativeRng,
) -> Result<GrowthChange, InvalidGrowthState> {
    growth.validate_gameplay_bounds()?;
    let mut candidate = *growth;
    let mut candidate_rng = *rng;
    let change = apply_fruit(&mut candidate, ratings, &mut candidate_rng);
    candidate.validate_gameplay_bounds()?;
    *growth = candidate;
    *rng = candidate_rng;
    Ok(change)
}

/// Applies one already-reached native feeding stat operation. Ratings are
/// [Mood, Belly, Swim, Fly, Run, Power, Stamina] signed bytes. The original
/// performs at most ONE level-up per skill, even with out-of-domain ratings.
/// Byte stores wrap exactly as ARM STRB does; no added normalization is used.
pub fn apply_fruit(
    growth: &mut NativeGrowth,
    ratings: [i8; 7],
    rng: &mut NativeRng,
) -> GrowthChange {
    growth.mood = (i16::from(growth.mood) + i16::from(ratings[0]) * 20).clamp(0, 200) as u8;
    growth.belly = (i16::from(growth.belly) + i16::from(ratings[1]) * 20).clamp(0, 200) as u8;
    let mut changes = [SkillChange::default(); SKILL_COUNT];
    let mut random_calls = 0;
    for (index, skill) in growth.skills.iter_mut().enumerate() {
        let mut experience =
            (i16::from(skill.experience) + i16::from(ratings[index + 2]) * 10).max(0);
        if experience > 99 {
            experience -= 100;
            if skill.level > 98 {
                experience = 99;
            } else {
                skill.level += 1;
                let roll =
                    rng.bounded(NonZeroU16::new(5).expect("native growth bound is positive")) as u8;
                random_calls += 1;
                let before = skill.points;
                let increment = u32::from(roll)
                    + 3 * u32::from(skill.grade)
                    + 11
                    + u32::from(growth.mood > 180);
                skill.points = (u32::from(before) + increment).min(9999) as u16;
                changes[index] = SkillChange {
                    leveled_up: true,
                    random_roll: Some(roll),
                    points_added: i32::from(skill.points) - i32::from(before),
                };
            }
        }
        skill.experience = experience as u8;
    }
    GrowthChange {
        skills: changes,
        random_calls,
        seed_after: rng.state(),
    }
}
