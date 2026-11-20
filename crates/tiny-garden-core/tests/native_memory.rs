use tiny_garden_core::native_memory;

#[test]
fn original_native_memory_addresses_and_range_are_explicit() {
    use native_memory::*;
    assert_eq!((COLUMNS, ROWS, CELLS), (6, 5, 30));
    assert_eq!(BOARD_INITIALIZE_ADDRESS, 0x0200_a554);
    assert_eq!(MATCH_PHASE_ADDRESS, 0x0200_b6a4);
    assert_eq!(NATIVE_FIELDS_BASE, 0x0300_1970);
}
