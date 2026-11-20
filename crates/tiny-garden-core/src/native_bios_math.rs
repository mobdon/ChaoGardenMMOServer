//! Explicit GBA BIOS scalar boundary shared by native Tiny minigames.
//!
//! ReferenceBiosMath is an integer numerical reference, independently checked
//! against pinned mGBA0.10.3 C (source SHA2568885962e1ca2d8c79a7f13a5d1e074bb2
//! b39704a64aa164f76e1506868c8759a). No authentic BIOS dump, cycle timing or
//! undocumented clobber-register proof is claimed. RejectBiosMath fails closed.
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathError {
    BiosUnavailable { service: u8 },
}
impl fmt::Display for MathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MathError {}

/// Pure scalar services. Native callers here sign-extend16-bit ArcTan2 inputs.
pub trait BiosMath {
    fn sqrt(&self, value: u32) -> Result<u16, MathError>;
    fn atan2(&self, x: i16, y: i16) -> Result<u16, MathError>;
}
#[derive(Clone, Copy, Debug, Default)]
pub struct RejectBiosMath;
impl BiosMath for RejectBiosMath {
    fn sqrt(&self, _: u32) -> Result<u16, MathError> {
        Err(MathError::BiosUnavailable { service: 8 })
    }
    fn atan2(&self, _: i16, _: i16) -> Result<u16, MathError> {
        Err(MathError::BiosUnavailable { service: 10 })
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ReferenceBiosMath;
impl BiosMath for ReferenceBiosMath {
    fn sqrt(&self, value: u32) -> Result<u16, MathError> {
        Ok(integer_sqrt(value))
    }
    fn atan2(&self, x: i16, y: i16) -> Result<u16, MathError> {
        Ok(reference_atan2(x, y))
    }
}

/// Mathematical floor square root, exactly16 bounded candidate checks.
pub fn integer_sqrt(value: u32) -> u16 {
    let mut root = 0_u32;
    for shift in (0..16).rev() {
        let next = root | (1 << shift);
        if u64::from(next) * u64::from(next) <= u64::from(value) {
            root = next;
        }
    }
    root as u16
}
fn reference_atan(ratio: i32) -> i32 {
    let squared = -((ratio * ratio) >> 14);
    let polynomial = [0x390, 0x91c, 0xfb6, 0x16aa, 0x2081, 0x3651, 0xa2f9]
        .into_iter()
        .fold(0xa9, |value, coefficient| {
            ((value * squared) >> 14) + coefficient
        });
    (ratio * polynomial) >> 16
}
fn reference_atan2(x: i16, y: i16) -> u16 {
    let (x, y) = (i32::from(x), i32::from(y));
    if y == 0 {
        return if x < 0 { 0x8000 } else { 0 };
    }
    if x == 0 {
        return if y < 0 { 0xc000 } else { 0x4000 };
    }
    let horizontal = x.abs() > y.abs() || (x.abs() == y.abs() && !(x < 0 && y < 0));
    let angle = if horizontal {
        let base = if x < 0 {
            0x8000
        } else if y < 0 {
            0x10000
        } else {
            0
        };
        base + reference_atan((y << 14) / x)
    } else {
        let base = if y < 0 { 0xc000 } else { 0x4000 };
        base - reference_atan((x << 14) / y)
    };
    angle as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn floor_sqrt_covers_every_u16_root_and_adjacent_boundaries() {
        for root in 0..=u32::from(u16::MAX) {
            let square = root * root;
            assert_eq!(integer_sqrt(square), root as u16);
            if square > 0 {
                assert_eq!(integer_sqrt(square - 1), (root - 1) as u16);
            }
            if root < u32::from(u16::MAX) {
                assert_eq!(integer_sqrt(square + 2 * root), root as u16);
            }
        }
        assert_eq!(integer_sqrt(u32::MAX), u16::MAX);
    }
    #[test]
    fn numerical_reference_axes_and_octants_are_explicit() {
        for (x, y, a) in [
            (0, 0, 0),
            (1, 0, 0),
            (-1, 0, 0x8000),
            (0, 1, 0x4000),
            (0, -1, 0xc000),
            (1, 1, 0x2000),
            (-1, 1, 0x6000),
            (-1, -1, 0xa000),
            (1, -1, 0xe000),
        ] {
            assert_eq!(ReferenceBiosMath.atan2(x, y), Ok(a));
        }
        assert_eq!(
            RejectBiosMath.atan2(1, 1),
            Err(MathError::BiosUnavailable { service: 10 })
        );
        assert_eq!(
            RejectBiosMath.sqrt(1),
            Err(MathError::BiosUnavailable { service: 8 })
        );
    }
}
