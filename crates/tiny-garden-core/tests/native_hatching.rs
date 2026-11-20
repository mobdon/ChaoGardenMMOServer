use tiny_garden_core::{native_growth, native_hatching};

#[test]
fn native_hatching_source_addresses_are_explicit() {
    assert_eq!(native_hatching::NATIVE_INITIALIZE_ADDRESS, 0x0200_1e74);
    assert_eq!(native_hatching::NATIVE_INITIALIZE_END, 0x0200_218c);
}

#[test]
#[ignore = "requires CHAO_TINY_GARDEN_PAYLOAD, the owner's exact private decoded A2NE payload"]
fn verified_owned_payload_is_required_for_new_native_hatch_data() {
    use native_hatching::*;
    let path =
        std::env::var_os("CHAO_TINY_GARDEN_PAYLOAD").expect("explicit private payload required");
    let payload = std::fs::read(path).unwrap();
    let source = NativeHatchSource::from_verified_payload(&payload).unwrap();
    let mut record = [0xff; RECORD_LEN];
    record[0x59] = 11;
    let mut rng = native_growth::NativeRng::new(42);
    let result = hatch_into_empty_native_slot(
        &mut record,
        &source,
        NativeHatchContext::default(),
        &mut rng,
    )
    .unwrap();
    assert_eq!(result.egg_id, 11);
    assert_eq!(record[0x72], 11);
    native_growth::NativeGrowth::read_raw_fields(&record)
        .unwrap()
        .validate_gameplay_bounds()
        .unwrap();
    let mut corrupted = payload;
    corrupted[BODY_TEMPLATE_OFFSET] ^= 1;
    assert!(NativeHatchSource::from_verified_payload(&corrupted).is_err());
}
