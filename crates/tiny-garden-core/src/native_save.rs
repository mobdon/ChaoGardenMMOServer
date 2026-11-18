//! Read-only native sector validation. Body fields remain opaque bytes.

use serde::Serialize;
use std::fmt;

pub const SECTOR_STRIDE: usize = 0x1000;
pub const FIRST_GARDEN_SECTOR: usize = 10;
pub const GARDEN_SECTOR_COUNT: usize = 6;
pub const SAVE_RECORD_LEN: usize = 0x858;
pub const MIN_FLASH_LEN: usize = 16 * SECTOR_STRIDE;
pub const SAVE_MARKER: u16 = 0x1965;
pub const REFERENCE_CRC_TABLE_OFFSET: usize = 0x1fd74;
pub const REFLECTED_CRC_POLYNOMIAL: u16 = 0x8408;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    TruncatedRecord {
        actual: usize,
    },
    TruncatedFlash {
        actual: usize,
    },
    WrongMarker {
        actual: u16,
    },
    ChecksumMismatch {
        stored: u16,
        calculated: u16,
    },
    CrcTableMissing,
    CrcTableMismatch {
        index: usize,
        actual: u16,
        expected: u16,
    },
}
impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SaveError {}

fn table_entry(mut value: u16) -> u16 {
    for _ in 0..8 {
        value = if value & 1 != 0 {
            (value >> 1) ^ REFLECTED_CRC_POLYNOMIAL
        } else {
            value >> 1
        };
    }
    value
}

/// Verified against every one of the native 256 little-endian table words
/// in the fingerprint-matched USA Tiny Chao Garden multiboot program.
pub fn verify_reference_crc_table(multiboot: &[u8]) -> Result<(), SaveError> {
    let table = multiboot
        .get(REFERENCE_CRC_TABLE_OFFSET..REFERENCE_CRC_TABLE_OFFSET + 512)
        .ok_or(SaveError::CrcTableMissing)?;
    for (index, pair) in table.as_chunks::<2>().0.iter().enumerate() {
        let actual = u16::from_le_bytes([pair[0], pair[1]]);
        let expected = table_entry(index as u16);
        if actual != expected {
            return Err(SaveError::CrcTableMismatch {
                index,
                actual,
                expected,
            });
        }
    }
    Ok(())
}

/// Native routine sub_0200185c: reflected CRC-16, init FFFF, polynomial8408,
/// final one's complement. A save record checks bytes2..0x858, not its CRC.
pub fn checksum(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for byte in bytes {
        crc = table_entry((crc ^ u16::from(*byte)) & 0xff) ^ (crc >> 8);
    }
    !crc
}

#[derive(Debug)]
pub struct SaveRecord<'a> {
    raw: &'a [u8; SAVE_RECORD_LEN],
}

impl<'a> SaveRecord<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, SaveError> {
        let raw: &[u8; SAVE_RECORD_LEN] = bytes
            .get(..SAVE_RECORD_LEN)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(SaveError::TruncatedRecord {
                actual: bytes.len(),
            })?;
        let marker = u16::from_le_bytes([raw[2], raw[3]]);
        if marker != SAVE_MARKER {
            return Err(SaveError::WrongMarker { actual: marker });
        }
        let stored = u16::from_le_bytes([raw[0], raw[1]]);
        let calculated = checksum(&raw[2..]);
        if stored != calculated {
            return Err(SaveError::ChecksumMismatch { stored, calculated });
        }
        Ok(Self { raw })
    }

    pub fn raw_bytes(&self) -> &'a [u8; SAVE_RECORD_LEN] {
        self.raw
    }

    /// No inferred stat/form/type/genetic labels are assigned to this body.
    /// Even unrecognized Chao type codes survive inspection byte for byte.
    pub fn opaque_body(&self) -> &'a [u8] {
        &self.raw[8..]
    }

    /// Native header.unk4 is incremented on save and compared unsigned by
    /// LoadGameState. Its original counter-wrap behavior is kept here.
    pub fn save_counter(&self) -> u32 {
        u32::from_le_bytes([self.raw[4], self.raw[5], self.raw[6], self.raw[7]])
    }

    pub fn stored_checksum(&self) -> u16 {
        u16::from_le_bytes([self.raw[0], self.raw[1]])
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectorReport {
    pub physical_sector: usize,
    pub byte_offset: usize,
    pub valid: bool,
    pub save_counter: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct FlashInspection<'a> {
    pub sectors: Vec<SectorReport>,
    pub selected_physical_sector: Option<usize>,
    pub selected: Option<SaveRecord<'a>>,
    /// Larger input dumps are neither truncated nor assigned bank semantics.
    pub trailing_bytes_after_first_bank: usize,
}

/// Reads native physical sectors10..15 at 4KiB spacing. Selects the first
/// valid record with the greatest unsigned header counter, including the
/// original first-wins tie behavior. It never edits or repairs a save dump.
pub fn inspect_flash(bytes: &[u8]) -> Result<FlashInspection<'_>, SaveError> {
    if bytes.len() < MIN_FLASH_LEN {
        return Err(SaveError::TruncatedFlash {
            actual: bytes.len(),
        });
    }
    let mut report = FlashInspection {
        sectors: Vec::with_capacity(GARDEN_SECTOR_COUNT),
        selected_physical_sector: None,
        selected: None,
        trailing_bytes_after_first_bank: bytes.len() - MIN_FLASH_LEN,
    };
    for sector in FIRST_GARDEN_SECTOR..FIRST_GARDEN_SECTOR + GARDEN_SECTOR_COUNT {
        let offset = sector * SECTOR_STRIDE;
        match SaveRecord::parse(&bytes[offset..offset + SAVE_RECORD_LEN]) {
            Ok(record) => {
                let counter = record.save_counter();
                if report
                    .selected
                    .as_ref()
                    .is_none_or(|previous| counter > previous.save_counter())
                {
                    report.selected_physical_sector = Some(sector);
                    report.selected = Some(record);
                }
                report.sectors.push(SectorReport {
                    physical_sector: sector,
                    byte_offset: offset,
                    valid: true,
                    save_counter: Some(counter),
                    error: None,
                });
            }
            Err(error) => report.sectors.push(SectorReport {
                physical_sector: sector,
                byte_offset: offset,
                valid: false,
                save_counter: None,
                error: Some(error.to_string()),
            }),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(counter: u32, body_seed: u8) -> [u8; SAVE_RECORD_LEN] {
        let mut raw = [body_seed; SAVE_RECORD_LEN];
        raw[2..4].copy_from_slice(&SAVE_MARKER.to_le_bytes());
        raw[4..8].copy_from_slice(&counter.to_le_bytes());
        let crc = checksum(&raw[2..]);
        raw[..2].copy_from_slice(&crc.to_le_bytes());
        raw
    }

    #[test]
    fn standard_crc_check_vector_and_empty() {
        assert_eq!(checksum(b"123456789"), 0x906e);
        assert_eq!(checksum(b""), 0);
    }

    #[test]
    fn opaque_native_body_is_preserved() {
        let mut raw = record(7, 0xa7);
        raw[0x80] = 0x1c;
        raw[0x529] = 0xfd;
        let crc = checksum(&raw[2..]);
        raw[..2].copy_from_slice(&crc.to_le_bytes());
        let parsed = SaveRecord::parse(&raw).unwrap();
        assert_eq!(parsed.raw_bytes(), &raw);
        assert_eq!(parsed.opaque_body(), &raw[8..]);
        assert_eq!(parsed.save_counter(), 7);
    }

    #[test]
    fn detects_corruption_marker_and_truncation() {
        let mut raw = record(1, 0);
        raw[500] ^= 1;
        assert!(matches!(
            SaveRecord::parse(&raw),
            Err(SaveError::ChecksumMismatch { .. })
        ));
        raw[2] = 0;
        assert!(matches!(
            SaveRecord::parse(&raw),
            Err(SaveError::WrongMarker { .. })
        ));
        assert!(SaveRecord::parse(&raw[..SAVE_RECORD_LEN - 1]).is_err());
        assert!(inspect_flash(&vec![0; MIN_FLASH_LEN - 1]).is_err());
    }

    #[test]
    fn six_sector_selection_rejects_corruption_and_preserves_original_ties() {
        let mut flash = vec![0xff; MIN_FLASH_LEN];
        for (physical, counter) in [(10, 3), (11, 9), (12, 9), (15, 8)] {
            let offset = physical * SECTOR_STRIDE;
            flash[offset..offset + SAVE_RECORD_LEN]
                .copy_from_slice(&record(counter, physical as u8));
        }
        let before = flash.clone();
        let result = inspect_flash(&flash).unwrap();
        assert_eq!(result.sectors.len(), 6);
        assert_eq!(result.selected_physical_sector, Some(11));
        assert_eq!(result.selected.unwrap().opaque_body()[0], 11);
        assert_eq!(flash, before);
        flash[11 * SECTOR_STRIDE + 17] ^= 1;
        assert_eq!(
            inspect_flash(&flash).unwrap().selected_physical_sector,
            Some(12)
        );
    }

    #[test]
    fn erased_flash_has_no_fabricated_default_chao() {
        let bytes = vec![0xff; MIN_FLASH_LEN];
        let result = inspect_flash(&bytes).unwrap();
        assert!(result.selected.is_none());
        assert!(result.sectors.iter().all(|entry| !entry.valid));
    }

    #[test]
    fn unsigned_counter_wrap_matches_native_comparison() {
        let mut flash = vec![0xff; MIN_FLASH_LEN];
        for (physical, count) in [(10, u32::MAX), (11, 0)] {
            let start = physical * SECTOR_STRIDE;
            flash[start..start + SAVE_RECORD_LEN].copy_from_slice(&record(count, 0));
        }
        assert_eq!(
            inspect_flash(&flash).unwrap().selected_physical_sector,
            Some(10)
        );
    }

    #[test]
    fn table_proof_checks_every_word() {
        let mut payload = vec![0; REFERENCE_CRC_TABLE_OFFSET + 512];
        for i in 0..256 {
            payload[REFERENCE_CRC_TABLE_OFFSET + i * 2..REFERENCE_CRC_TABLE_OFFSET + i * 2 + 2]
                .copy_from_slice(&table_entry(i as u16).to_le_bytes());
        }
        assert!(verify_reference_crc_table(&payload).is_ok());
        payload[REFERENCE_CRC_TABLE_OFFSET + 511] ^= 1;
        assert!(matches!(
            verify_reference_crc_table(&payload),
            Err(SaveError::CrcTableMismatch { index: 255, .. })
        ));
    }
}
