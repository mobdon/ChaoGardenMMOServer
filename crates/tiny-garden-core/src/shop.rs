//! Bounded read-only fruit tables from the exact verified A2NE Tiny Garden payload.
//!
//! Raw signed channels deliberately carry no application, clamp or stat-order
//! assumptions. A server adapter must document any gameplay interpretation.
use crate::gba;
use serde::Serialize;
use std::fmt;

pub const FRUIT_COUNT: usize = 7;
pub const EFFECTS_OFFSET: usize = 0x12510;
pub const EFFECTS_LEN: usize = FRUIT_COUNT * 7;
pub const PRICES_OFFSET: usize = 0x12548;
pub const PRICES_LEN: usize = FRUIT_COUNT * 4;
pub const EFFECTS_SHA256: &str = "b33ae558a366ac031e7326d7ade4d3a38f3818f21c6139f6a97a8ceeaa3c49bd";
pub const PRICES_SHA256: &str = "e9eaed988f3156a9b8c28f72dc48adac7c1735915afd280c7a2406529cc2dc19";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TinyFruit {
    pub id: u8,
    pub price: u32,
    pub raw: [i8; 7],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TinyShopCatalog {
    pub fruits: [TinyFruit; FRUIT_COUNT],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    WrongPayloadLength {
        actual: usize,
        expected: usize,
    },
    UnsupportedPayloadSha1,
    TableOutOfBounds {
        offset: usize,
        length: usize,
        actual: usize,
    },
    TableHashMismatch {
        offset: usize,
        length: usize,
    },
    WrongTableLength {
        table: &'static str,
        actual: usize,
        expected: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

fn checked_table<'a>(
    input: &'a [u8],
    offset: usize,
    length: usize,
    sha256: &str,
) -> Result<&'a [u8], Error> {
    let bytes = offset
        .checked_add(length)
        .and_then(|end| input.get(offset..end))
        .ok_or(Error::TableOutOfBounds {
            offset,
            length,
            actual: input.len(),
        })?;
    if gba::sha256(bytes) != sha256 {
        return Err(Error::TableHashMismatch { offset, length });
    }
    Ok(bytes)
}

fn parse_tables(effects: &[u8], prices: &[u8]) -> Result<TinyShopCatalog, Error> {
    let effects: &[u8; EFFECTS_LEN] = effects.try_into().map_err(|_| Error::WrongTableLength {
        table: "effects",
        actual: effects.len(),
        expected: EFFECTS_LEN,
    })?;
    let prices: &[u8; PRICES_LEN] = prices.try_into().map_err(|_| Error::WrongTableLength {
        table: "prices",
        actual: prices.len(),
        expected: PRICES_LEN,
    })?;
    Ok(TinyShopCatalog {
        fruits: std::array::from_fn(|id| TinyFruit {
            id: id as u8,
            price: u32::from_le_bytes(std::array::from_fn(|byte| prices[id * 4 + byte])),
            raw: std::array::from_fn(|channel| effects[id * 7 + channel] as i8),
        }),
    })
}

impl TinyShopCatalog {
    /// Accepts only the exact known decoded multiboot program, not a full ROM,
    /// partial table, another region/version or caller-asserted fingerprint.
    /// Identity is checked before either source-version-specific offset is used.
    pub fn from_verified_payload(payload: &[u8]) -> Result<Self, Error> {
        if payload.len() != gba::REFERENCE_GARDEN_LEN {
            return Err(Error::WrongPayloadLength {
                actual: payload.len(),
                expected: gba::REFERENCE_GARDEN_LEN,
            });
        }
        if gba::sha1(payload) != gba::REFERENCE_GARDEN_SHA1 {
            return Err(Error::UnsupportedPayloadSha1);
        }
        let effects = checked_table(payload, EFFECTS_OFFSET, EFFECTS_LEN, EFFECTS_SHA256)?;
        let prices = checked_table(payload, PRICES_OFFSET, PRICES_LEN, PRICES_SHA256)?;
        parse_tables(effects, prices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_rows_keep_signed_channels_and_little_endian_prices_exact() {
        // Made-up data, not an extracted original fruit table.
        let mut effects = [0_u8; EFFECTS_LEN];
        for (index, byte) in effects.iter_mut().enumerate() {
            *byte = index as u8;
        }
        effects[..7].copy_from_slice(&[0, 127, 128, 255, 42, 254, 1]);
        let values = [0_u32, 1, 0x0102_0304, u32::MAX, 512, 65537, 77];
        let mut prices = [0_u8; PRICES_LEN];
        for (chunk, value) in prices.as_chunks_mut::<4>().0.iter_mut().zip(values) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        let catalog = parse_tables(&effects, &prices).unwrap();
        assert_eq!(catalog.fruits[0].raw, [0, 127, -128, -1, 42, -2, 1]);
        for (id, fruit) in catalog.fruits.iter().enumerate() {
            assert_eq!(usize::from(fruit.id), id);
            assert_eq!(fruit.price, values[id]);
            if id > 0 {
                assert_eq!(
                    fruit.raw,
                    std::array::from_fn(|channel| (id * 7 + channel) as i8)
                );
            }
        }
    }

    #[test]
    fn synthetic_parser_requires_both_exact_table_lengths() {
        for size in [0, EFFECTS_LEN - 1, EFFECTS_LEN + 1] {
            assert_eq!(
                parse_tables(&vec![0; size], &[0; PRICES_LEN]),
                Err(Error::WrongTableLength {
                    table: "effects",
                    actual: size,
                    expected: EFFECTS_LEN,
                })
            );
        }
        for size in [0, PRICES_LEN - 1, PRICES_LEN + 1] {
            assert_eq!(
                parse_tables(&[0; EFFECTS_LEN], &vec![0; size]),
                Err(Error::WrongTableLength {
                    table: "prices",
                    actual: size,
                    expected: PRICES_LEN,
                })
            );
        }
    }

    #[test]
    fn checked_spans_reject_truncation_overflow_and_wrong_hash() {
        let input = [11_u8, 22, 33, 44];
        assert_eq!(
            checked_table(&input, 1, 2, &gba::sha256(&input[1..3])).unwrap(),
            &[22, 33]
        );
        for (offset, length) in [(3, 2), (usize::MAX, 2), (5, 0)] {
            assert_eq!(
                checked_table(&input, offset, length, "unused"),
                Err(Error::TableOutOfBounds {
                    offset,
                    length,
                    actual: input.len()
                })
            );
        }
        assert_eq!(
            checked_table(&input, 1, 2, &gba::sha256(&input[..2])),
            Err(Error::TableHashMismatch {
                offset: 1,
                length: 2
            })
        );
    }

    #[test]
    fn public_reader_rejects_wrong_lengths_and_unverified_payload_before_parsing() {
        for size in [
            0,
            gba::REFERENCE_GARDEN_LEN - 1,
            gba::REFERENCE_GARDEN_LEN + 1,
        ] {
            assert_eq!(
                TinyShopCatalog::from_verified_payload(&vec![0; size]),
                Err(Error::WrongPayloadLength {
                    actual: size,
                    expected: gba::REFERENCE_GARDEN_LEN
                })
            );
        }
        let unknown = vec![0; gba::REFERENCE_GARDEN_LEN];
        assert_eq!(
            TinyShopCatalog::from_verified_payload(&unknown),
            Err(Error::UnsupportedPayloadSha1)
        );
    }
}
