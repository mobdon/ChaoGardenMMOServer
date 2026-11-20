//! Source-backed A2NE native egg initialization, isolated from MMO ownership.
//!
//! Native sub_02001E74 replaces the native Chao body. This checked port only
//! accepts an empty native slot and must build a NEW record for MMO adoption;
//! never pass an existing canonical Chao's record or trust a phone's output.

use crate::{gba, native_growth::NativeRng, shop};
use std::{fmt, num::NonZeroU16};

pub const RECORD_LEN: usize = 0x858;
pub const NATIVE_INITIALIZE_ADDRESS: u32 = 0x0200_1e74;
pub const NATIVE_INITIALIZE_END: u32 = 0x0200_218c;
pub const NAME_TABLE_OFFSET: usize = 0x202b6;
pub const NAME_TABLE_LEN: usize = 2 * 10 * 7;
pub const BODY_TEMPLATE_OFFSET: usize = 0x2071c;
pub const BODY_TEMPLATE_LEN: usize = 0x780;
pub const NAME_TABLE_SHA256: &str =
    "3c3e22f3f953b3ab0579bc713390ac5387ba6b139e33b24fab63904cb4c7d2f5";
pub const BODY_TEMPLATE_SHA256: &str =
    "ed21c8ce8c09c5bfde4eb07b7b2d3d8042e501ff7b70f01f293d5c4df35ebfe9";
/// Bounded safety limit, not a replacement native probability/grade rule.
pub const MAX_RANDOM_CALLS: u16 = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Source(shop::Error),
    InvalidSourceSpan { offset: usize },
    TruncatedRecord { actual: usize, minimum: usize },
    OccupiedNativeSlot,
    InvalidEggId(u8),
    RandomBudgetExceeded,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeHatchSource {
    names: [u8; NAME_TABLE_LEN],
    body_template: [u8; BODY_TEMPLATE_LEN],
}

fn verified_span<const N: usize>(
    payload: &[u8],
    offset: usize,
    hash: &str,
) -> Result<[u8; N], Error> {
    let bytes = payload
        .get(offset..offset + N)
        .ok_or(Error::InvalidSourceSpan { offset })?;
    if gba::sha256(bytes) != hash {
        return Err(Error::InvalidSourceSpan { offset });
    }
    bytes
        .try_into()
        .map_err(|_| Error::InvalidSourceSpan { offset })
}

impl NativeHatchSource {
    /// Reuses the existing exact-payload gate. Native names and body bytes are
    /// private source inputs, never a shipped substitute table or sprite proof.
    pub fn from_verified_payload(payload: &[u8]) -> Result<Self, Error> {
        shop::TinyShopCatalog::from_verified_payload(payload).map_err(Error::Source)?;
        Ok(Self {
            names: verified_span(payload, NAME_TABLE_OFFSET, NAME_TABLE_SHA256)?,
            body_template: verified_span(payload, BODY_TEMPLATE_OFFSET, BODY_TEMPLATE_SHA256)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeHatchContext {
    /// Native gUnknown_03003330.unk10 != 0; no Unicode/language decoding inferred.
    pub alternate_name_table: bool,
    /// Original caller-owned frame counter, not the phone's clock or account ID.
    pub frame_counter: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeHatchOutcome {
    pub egg_id: u8,
    pub name_index: u8,
    pub grade_branch: u8,
    pub random_calls: u16,
    pub seed_after: u32,
    /// The three original transient byte writes. Applying them to a running
    /// engine is outside this raw-record constructor; addresses are evidence.
    pub transient_writes: [(u32, u8); 3],
}

fn draw(rng: &mut NativeRng, calls: &mut u16, limit: u16) -> Result<u16, Error> {
    if *calls >= limit {
        return Err(Error::RandomBudgetExceeded);
    }
    *calls += 1;
    Ok(rng.next_u15())
}

fn roll(rng: &mut NativeRng, calls: &mut u16, bound: u16, limit: u16) -> Result<u8, Error> {
    let bound = NonZeroU16::new(bound).expect("native hatch roll bound is positive");
    Ok((draw(rng, calls, limit)? % bound) as u8)
}

fn make_grades(rng: &mut NativeRng, calls: &mut u16, limit: u16) -> Result<(u8, [u8; 5]), Error> {
    let branch = roll(rng, calls, 3, limit)?;
    let mut grades = [0; 5];
    match branch {
        0 => {
            let mut pool = [0, 1, 2, 3, 4];
            for (index, grade) in grades[..4].iter_mut().enumerate() {
                let selected = usize::from(roll(rng, calls, 5 - index as u16, limit)?);
                *grade = pool[selected];
                for position in selected..4 {
                    pool[position] = pool[position + 1];
                }
            }
            grades[4] = pool[0];
        }
        1 => {
            for grade in &mut grades {
                *grade = roll(rng, calls, 3, limit)? + 1;
            }
        }
        2 => {
            let mut remaining = 12;
            'distribute: loop {
                for grade in &mut grades {
                    if *grade > 4 {
                        continue;
                    }
                    let bound = if remaining > 4 { 6 } else { remaining + 1 };
                    let amount = roll(rng, calls, u16::from(bound), limit)?;
                    if *grade + amount > 5 {
                        continue;
                    }
                    *grade += amount;
                    remaining -= amount;
                    if remaining == 0 {
                        break 'distribute;
                    }
                }
            }
        }
        _ => unreachable!("native modulus3 branch"),
    }
    Ok((branch, grades))
}

/// Generates one new native Chao body in an EMPTY native slot. Unknown prefix
/// and trailing bytes survive. Failure changes neither the record nor RNG.
/// The native egg must be one of the twelve source IDs, and field58 must be FF.
/// This is not a durable save, owner collection, genome decoder or hatch timer.
pub fn hatch_into_empty_native_slot(
    record: &mut [u8],
    source: &NativeHatchSource,
    context: NativeHatchContext,
    rng: &mut NativeRng,
) -> Result<NativeHatchOutcome, Error> {
    hatch_with_budget(record, source, context, rng, MAX_RANDOM_CALLS)
}

fn hatch_with_budget(
    record: &mut [u8],
    source: &NativeHatchSource,
    context: NativeHatchContext,
    rng: &mut NativeRng,
    limit: u16,
) -> Result<NativeHatchOutcome, Error> {
    if record.len() < RECORD_LEN {
        return Err(Error::TruncatedRecord {
            actual: record.len(),
            minimum: RECORD_LEN,
        });
    }
    if record[0x58] != 0xff {
        return Err(Error::OccupiedNativeSlot);
    }
    let egg_id = record[0x59];
    if egg_id >= 12 {
        return Err(Error::InvalidEggId(egg_id));
    }
    let mut next_rng = *rng;
    let mut calls = 0;
    let name_index = roll(&mut next_rng, &mut calls, 10, limit)?;
    let (grade_branch, grades) = make_grades(&mut next_rng, &mut calls, limit)?;
    let mut identifiers = [0; 5];
    for identifier in &mut identifiers[..4] {
        let high = u32::from(draw(&mut next_rng, &mut calls, limit)?);
        let low = u32::from(draw(&mut next_rng, &mut calls, limit)?);
        *identifier = (high << 16)
            .wrapping_add(low)
            .wrapping_add(context.frame_counter);
    }
    identifiers[4] = u32::from(draw(&mut next_rng, &mut calls, limit)?)
        .wrapping_add(context.frame_counter)
        .wrapping_shl(16);
    // All fallible operations finish before either caller-owned state changes.
    record[0x6a..RECORD_LEN].fill(0);
    record[0x72] = egg_id;
    record[0x59] = 0xff;
    for offset in [0x58, 0x62, 0x69] {
        record[offset] = 0;
    }
    let name_start = usize::from(context.alternate_name_table) * 70 + usize::from(name_index) * 7;
    record[0x6a..0x71].copy_from_slice(&source.names[name_start..name_start + 7]);
    record[0x73] = 100;
    record[0x74] = 100;
    record[0x80..0x85].copy_from_slice(&grades);
    record[0x85..0x88].fill(0xff);
    record[0x88..0x90].fill(1);
    record[0xd8..RECORD_LEN].copy_from_slice(&source.body_template);
    for (index, identifier) in identifiers.into_iter().enumerate() {
        record[0xe8 + index * 4..0xec + index * 4].copy_from_slice(&identifier.to_be_bytes());
    }
    for offset in [0x132, 0x528, 0x529] {
        record[offset] = egg_id;
    }
    for (index, grade) in grades.into_iter().enumerate() {
        record[0x4ec + index * 2..0x4ee + index * 2].fill(grade);
    }
    *rng = next_rng;
    Ok(NativeHatchOutcome {
        egg_id,
        name_index,
        grade_branch,
        random_calls: calls,
        seed_after: rng.state(),
        transient_writes: [(0x0300_1501, 0xff), (0x0300_152f, 0), (0x0300_152d, 9)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_source() -> NativeHatchSource {
        NativeHatchSource {
            names: std::array::from_fn(|index| index as u8),
            body_template: std::array::from_fn(|index| (index % 251) as u8),
        }
    }

    fn empty_record(egg: u8) -> [u8; RECORD_LEN + 16] {
        let mut record = [0x5a; RECORD_LEN + 16];
        record[0x58] = 0xff;
        record[0x59] = egg;
        record
    }

    #[test]
    fn each_grade_branch_obeys_native_invariants_across_seeded_corpus() {
        let source = synthetic_source();
        let mut branches = [0; 3];
        for seed in 0..1024 {
            let mut record = empty_record((seed % 12) as u8);
            let mut rng = NativeRng::new(seed);
            let result = hatch_into_empty_native_slot(
                &mut record,
                &source,
                NativeHatchContext::default(),
                &mut rng,
            )
            .unwrap();
            let grades = &record[0x80..0x85];
            branches[usize::from(result.grade_branch)] += 1;
            match result.grade_branch {
                0 => {
                    let mut sorted = grades.to_vec();
                    sorted.sort();
                    assert_eq!(sorted, [0, 1, 2, 3, 4]);
                }
                1 => assert!(grades.iter().all(|grade| (1..=3).contains(grade))),
                2 => {
                    assert_eq!(grades.iter().copied().sum::<u8>(), 12);
                    assert!(grades.iter().all(|grade| *grade <= 5));
                }
                _ => unreachable!(),
            }
            assert_eq!(rng.state(), result.seed_after);
            assert!((14..MAX_RANDOM_CALLS).contains(&result.random_calls));
        }
        assert!(branches.into_iter().all(|count| count > 0));
    }

    #[test]
    fn native_hatch_resets_only_body_and_traced_flags_while_preserving_prefix_and_tail() {
        let source = synthetic_source();
        let mut record = empty_record(11);
        let original = record;
        let mut rng = NativeRng::new(u32::MAX);
        let result = hatch_into_empty_native_slot(
            &mut record,
            &source,
            NativeHatchContext {
                alternate_name_table: true,
                frame_counter: u32::MAX,
            },
            &mut rng,
        )
        .unwrap();
        assert_eq!(record[0x72], 11);
        assert_eq!(record[0x59], 0xff);
        assert_eq!(&record[0x73..0x75], &[100, 100]);
        assert_eq!(&record[0x85..0x88], &[0xff; 3]);
        assert_eq!(&record[0x88..0x90], &[1; 8]);
        assert_eq!(&record[0x90..0x9a], &[0; 10]);
        let start = 70 + usize::from(result.name_index) * 7;
        assert_eq!(&record[0x6a..0x71], &source.names[start..start + 7]);
        for offset in [0x132, 0x528, 0x529] {
            assert_eq!(record[offset], 11);
        }
        for index in 0..5 {
            assert_eq!(
                &record[0x4ec + index * 2..0x4ee + index * 2],
                &[record[0x80 + index]; 2]
            );
        }
        for offset in 0..0x6a {
            if [0x58, 0x59, 0x62, 0x69].contains(&offset) {
                continue;
            }
            assert_eq!(record[offset], original[offset], "prefix {offset:x}");
        }
        assert_eq!(&record[RECORD_LEN..], &original[RECORD_LEN..]);
        assert_eq!(
            result.transient_writes,
            [(0x0300_1501, 0xff), (0x0300_152f, 0), (0x0300_152d, 9)]
        );
    }

    #[test]
    fn occupied_invalid_truncated_and_exhausted_requests_change_neither_record_nor_rng() {
        let source = synthetic_source();
        for (occupied, egg, limit) in [
            (true, 0, MAX_RANDOM_CALLS),
            (false, 12, MAX_RANDOM_CALLS),
            (false, 0xff, MAX_RANDOM_CALLS),
            (false, 0, 0),
            (false, 0, 2),
        ] {
            let mut record = empty_record(egg);
            if occupied {
                record[0x58] = 0;
            }
            let before = record;
            let mut rng = NativeRng::new(73);
            assert!(
                hatch_with_budget(
                    &mut record,
                    &source,
                    NativeHatchContext::default(),
                    &mut rng,
                    limit
                )
                .is_err()
            );
            assert_eq!(record, before);
            assert_eq!(rng.state(), 73);
        }
        let mut short = [0xff; RECORD_LEN - 1];
        let before = short;
        let mut rng = NativeRng::new(73);
        assert!(
            hatch_into_empty_native_slot(
                &mut short,
                &source,
                NativeHatchContext::default(),
                &mut rng
            )
            .is_err()
        );
        assert_eq!(short, before);
        assert_eq!(rng.state(), 73);
    }

    #[test]
    fn unverified_payload_and_source_spans_fail_closed() {
        assert!(NativeHatchSource::from_verified_payload(&[]).is_err());
        assert!(
            NativeHatchSource::from_verified_payload(&vec![0; gba::REFERENCE_GARDEN_LEN]).is_err()
        );
        assert!(verified_span::<7>(&[0; 6], 0, "unused").is_err());
        assert!(verified_span::<7>(&[0; 7], 0, "wrong").is_err());
    }
}
