//! Raw GBA headers and bounded BIOS-format LZ77 (type 0x10) decoding.

use serde::Serialize;
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::fmt;

pub const GBA_HEADER_LEN: usize = 0xc0;
pub const MAX_ROM_LEN: usize = 32 * 1024 * 1024;
pub const EWRAM_LEN: usize = 0x40000;
pub const REFERENCE_ROM_SHA1: &str = "7bcd6a07af7c894746fa28073fe0c0e34408022d";
pub const REFERENCE_GARDEN_SHA1: &str = "2c7da8e1c9af85986eecf4effae39197cdac9abf";
pub const REFERENCE_GARDEN_LEN: usize = 191_340;
pub const REFERENCE_GARDEN_ROM_OFFSET: usize = 0xaed70;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryError {
    HeaderTooShort {
        actual: usize,
    },
    InputTooLarge {
        actual: usize,
        limit: usize,
    },
    Truncated {
        offset: usize,
    },
    WrongCompression {
        actual: u8,
    },
    OutputLimit {
        declared: usize,
        limit: usize,
    },
    BackReferenceBeforeStart {
        output_offset: usize,
        distance: usize,
    },
    MatchExceedsOutput {
        output_offset: usize,
        length: usize,
    },
    AllocationFailed,
    ScanBudgetExceeded,
    UnsupportedReferenceRom,
    ReferencePayloadMismatch,
}

impl fmt::Display for BinaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BinaryError {}

#[derive(Debug)]
pub struct GbaHeader<'a> {
    raw: &'a [u8; GBA_HEADER_LEN],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderReport {
    pub title: String,
    pub game_code: String,
    pub maker_code: String,
    pub unit_code: u8,
    pub device_type: u8,
    pub revision: u8,
    pub fixed_value: u8,
    pub fixed_value_valid: bool,
    pub stored_complement: u8,
    pub calculated_complement: u8,
    pub checksum_valid: bool,
    pub reserved_bytes_zero: bool,
    pub logo_sha256: String,
}

fn field_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(['\0', ' '])
        .to_owned()
}

impl<'a> GbaHeader<'a> {
    /// Parses only the raw cartridge-header layout. A valid checksum does not
    /// prove that the cartridge is a particular retail version or executable.
    pub fn parse(input: &'a [u8]) -> Result<Self, BinaryError> {
        let raw = input
            .get(..GBA_HEADER_LEN)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(BinaryError::HeaderTooShort {
                actual: input.len(),
            })?;
        Ok(Self { raw })
    }

    pub fn raw_bytes(&self) -> &'a [u8; GBA_HEADER_LEN] {
        self.raw
    }

    pub fn calculated_complement(&self) -> u8 {
        self.raw[0xa0..=0xbc]
            .iter()
            .fold(0_u8, |sum, byte| sum.wrapping_sub(*byte))
            .wrapping_sub(0x19)
    }

    pub fn report(&self) -> HeaderReport {
        let calculated = self.calculated_complement();
        HeaderReport {
            title: field_text(&self.raw[0xa0..0xac]),
            game_code: field_text(&self.raw[0xac..0xb0]),
            maker_code: field_text(&self.raw[0xb0..0xb2]),
            fixed_value: self.raw[0xb2],
            fixed_value_valid: self.raw[0xb2] == 0x96,
            unit_code: self.raw[0xb3],
            device_type: self.raw[0xb4],
            revision: self.raw[0xbc],
            stored_complement: self.raw[0xbd],
            calculated_complement: calculated,
            checksum_valid: self.raw[0xbd] == calculated,
            reserved_bytes_zero: self.raw[0xb5..0xbc]
                .iter()
                .chain(self.raw[0xbe..0xc0].iter())
                .all(|byte| *byte == 0),
            // Preserve and fingerprint the user's logo bytes; no copyrighted
            // Nintendo logo fixture is embedded in this crate.
            logo_sha256: sha256(&self.raw[4..0xa0]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedLz77 {
    pub bytes: Vec<u8>,
    /// Exact compressed bytes consumed, excluding optional trailing padding.
    pub consumed: usize,
}

pub fn lz77_declared_size(input: &[u8]) -> Result<usize, BinaryError> {
    let header = input.get(..4).ok_or(BinaryError::Truncated {
        offset: input.len(),
    })?;
    if header[0] != 0x10 {
        return Err(BinaryError::WrongCompression { actual: header[0] });
    }
    Ok(usize::from(header[1]) | usize::from(header[2]) << 8 | usize::from(header[3]) << 16)
}

/// Decodes WRAM byte-output LZ77 with overlapping back references. Malformed
/// tokens that would overrun the declared result are rejected, not emulated
/// as hardware memory corruption. The caller sets an explicit output cap.
pub fn decode_lz77(input: &[u8], max_output: usize) -> Result<DecodedLz77, BinaryError> {
    let length = lz77_declared_size(input)?;
    if length > max_output {
        return Err(BinaryError::OutputLimit {
            declared: length,
            limit: max_output,
        });
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| BinaryError::AllocationFailed)?;
    let mut cursor = 4;
    while output.len() < length {
        let flags = *input
            .get(cursor)
            .ok_or(BinaryError::Truncated { offset: cursor })?;
        cursor += 1;
        for bit in (0..8).rev() {
            if output.len() == length {
                break;
            }
            if flags & (1 << bit) == 0 {
                let byte = *input
                    .get(cursor)
                    .ok_or(BinaryError::Truncated { offset: cursor })?;
                cursor += 1;
                output.push(byte);
            } else {
                let pair = input
                    .get(cursor..cursor + 2)
                    .ok_or(BinaryError::Truncated { offset: cursor })?;
                cursor += 2;
                let distance = ((usize::from(pair[0]) & 0xf) << 8 | usize::from(pair[1])) + 1;
                let count = usize::from(pair[0] >> 4) + 3;
                if distance > output.len() {
                    return Err(BinaryError::BackReferenceBeforeStart {
                        output_offset: output.len(),
                        distance,
                    });
                }
                if count > length - output.len() {
                    return Err(BinaryError::MatchExceedsOutput {
                        output_offset: output.len(),
                        length: count,
                    });
                }
                for _ in 0..count {
                    let byte = output[output.len() - distance];
                    output.push(byte);
                }
            }
        }
    }
    Ok(DecodedLz77 {
        bytes: output,
        consumed: cursor,
    })
}

pub fn sha1(bytes: &[u8]) -> String {
    format!("{:x}", Sha1::digest(bytes))
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Source-version-specific extraction, gated by both full-ROM and decoded
/// payload fingerprints. It never applies the USA offset to another ROM.
pub fn extract_reference_garden(rom: &[u8]) -> Result<DecodedLz77, BinaryError> {
    if rom.len() > MAX_ROM_LEN {
        return Err(BinaryError::InputTooLarge {
            actual: rom.len(),
            limit: MAX_ROM_LEN,
        });
    }
    if sha1(rom) != REFERENCE_ROM_SHA1 {
        return Err(BinaryError::UnsupportedReferenceRom);
    }
    let compressed = rom
        .get(REFERENCE_GARDEN_ROM_OFFSET..)
        .ok_or(BinaryError::ReferencePayloadMismatch)?;
    let decoded = decode_lz77(compressed, EWRAM_LEN)?;
    if decoded.bytes.len() != REFERENCE_GARDEN_LEN || sha1(&decoded.bytes) != REFERENCE_GARDEN_SHA1
    {
        return Err(BinaryError::ReferencePayloadMismatch);
    }
    Ok(decoded)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadMatch {
    pub rom_offset: usize,
    pub compressed_bytes: usize,
    pub decoded_bytes: usize,
    pub sha1: String,
    pub sha256: String,
}

/// Searches 4-byte-aligned type-0x10 streams for this one verified payload.
/// A miss says nothing about other regional/game versions. Search work is
/// bounded independently of caller-controlled ROM headers.
pub fn find_reference_garden(rom: &[u8]) -> Result<Option<PayloadMatch>, BinaryError> {
    if rom.len() > MAX_ROM_LEN {
        return Err(BinaryError::InputTooLarge {
            actual: rom.len(),
            limit: MAX_ROM_LEN,
        });
    }
    let mut candidates = 0;
    for offset in (0..rom.len().saturating_sub(3)).step_by(4) {
        let compressed = &rom[offset..];
        if lz77_declared_size(compressed).ok() != Some(REFERENCE_GARDEN_LEN) {
            continue;
        }
        candidates += 1;
        if candidates > 512 {
            return Err(BinaryError::ScanBudgetExceeded);
        }
        if let Ok(decoded) = decode_lz77(compressed, EWRAM_LEN)
            && sha1(&decoded.bytes) == REFERENCE_GARDEN_SHA1
        {
            return Ok(Some(PayloadMatch {
                rom_offset: offset,
                compressed_bytes: decoded.consumed,
                decoded_bytes: decoded.bytes.len(),
                sha1: sha1(&decoded.bytes),
                sha256: sha256(&decoded.bytes),
            }));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_preserves_unknown_bytes_and_checks_complement() {
        let mut bytes = [0_u8; GBA_HEADER_LEN];
        bytes[0xa0..0xac].copy_from_slice(b"TEST GARDEN\0");
        bytes[0xac..0xb0].copy_from_slice(b"A2NE");
        bytes[0xb0..0xb2].copy_from_slice(b"78");
        bytes[0xb2] = 0x96;
        bytes[0xbc] = 2;
        bytes[0xbd] = GbaHeader::parse(&bytes).unwrap().calculated_complement();
        let parsed = GbaHeader::parse(&bytes).unwrap();
        assert_eq!(parsed.raw_bytes(), &bytes);
        assert_eq!(parsed.report().title, "TEST GARDEN");
        assert!(parsed.report().checksum_valid);
        bytes[0xa0] ^= 1;
        assert!(!GbaHeader::parse(&bytes).unwrap().report().checksum_valid);
    }

    #[test]
    fn every_short_header_is_an_error() {
        for length in 0..GBA_HEADER_LEN {
            assert!(GbaHeader::parse(&vec![0; length]).is_err());
        }
    }

    #[test]
    fn literals_flags_and_consumed_length() {
        let decoded = decode_lz77(&[0x10, 3, 0, 0, 0, b'a', b'b', b'c', 0xee], 20).unwrap();
        assert_eq!(decoded.bytes, b"abc");
        assert_eq!(decoded.consumed, 8);
        assert_eq!(decode_lz77(&[0x10, 0, 0, 0], 0).unwrap().bytes, b"");
    }

    #[test]
    fn overlapping_and_mixed_back_references() {
        assert_eq!(
            decode_lz77(&[0x10, 19, 0, 0, 0x40, b'A', 0xf0, 0], 19)
                .unwrap()
                .bytes,
            vec![b'A'; 19]
        );
        assert_eq!(
            decode_lz77(&[0x10, 9, 0, 0, 0x10, b'A', b'B', b'C', 0x30, 2], 9)
                .unwrap()
                .bytes,
            b"ABCABCABC"
        );
    }

    #[test]
    fn malformed_lz_cannot_escape_either_buffer() {
        let valid = [0x10, 9, 0, 0, 0x10, b'A', b'B', b'C', 0x30, 2];
        for length in 0..valid.len() {
            assert!(decode_lz77(&valid[..length], 20).is_err());
        }
        assert!(matches!(
            decode_lz77(&[0x10, 1, 0, 0, 0x80, 0, 0], 20),
            Err(BinaryError::BackReferenceBeforeStart { .. })
        ));
        assert!(matches!(
            decode_lz77(&[0x10, 4, 0, 0, 0x40, b'A', 0xf0, 0], 20),
            Err(BinaryError::MatchExceedsOutput { .. })
        ));
        assert!(matches!(
            decode_lz77(&[0x10, 0xff, 0xff, 0xff], 0x40000),
            Err(BinaryError::OutputLimit { .. })
        ));
        assert!(matches!(
            decode_lz77(&[0x11, 0, 0, 0], 20),
            Err(BinaryError::WrongCompression { .. })
        ));
    }

    #[test]
    fn tiny_or_wrong_rom_is_not_given_a_known_version_offset() {
        assert_eq!(
            extract_reference_garden(&[0; 20]),
            Err(BinaryError::UnsupportedReferenceRom)
        );
        assert!(find_reference_garden(&[0x10, 1, 2]).unwrap().is_none());
    }
}
