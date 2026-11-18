//! Opt-in validation using the user's local ROM; no ROM fixture is committed.
use tiny_garden_core::{gba, native_save};

#[test]
#[ignore = "Requires TCG_REFERENCE_ROM pointing to the user's verified SA2 USA raw GBA file"]
fn supplied_retail_rom_decodes_to_the_exact_native_program_and_crc_table() {
    let path = std::env::var_os("TCG_REFERENCE_ROM").expect("Set TCG_REFERENCE_ROM");
    let rom = std::fs::read(path).unwrap();
    assert_eq!(gba::sha1(&rom), gba::REFERENCE_ROM_SHA1);
    let header = gba::GbaHeader::parse(&rom).unwrap().report();
    assert_eq!(header.game_code, "A2NE");
    assert!(header.fixed_value_valid && header.checksum_valid);
    let garden = gba::extract_reference_garden(&rom).unwrap();
    assert_eq!(garden.bytes.len(), 191_340);
    assert_eq!(garden.consumed, 155_273);
    assert_eq!(gba::sha1(&garden.bytes), gba::REFERENCE_GARDEN_SHA1);
    assert_eq!(
        gba::sha256(&garden.bytes),
        "84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca"
    );
    native_save::verify_reference_crc_table(&garden.bytes).unwrap();
    let found = gba::find_reference_garden(&rom).unwrap().unwrap();
    assert_eq!(found.rom_offset, gba::REFERENCE_GARDEN_ROM_OFFSET);
}
