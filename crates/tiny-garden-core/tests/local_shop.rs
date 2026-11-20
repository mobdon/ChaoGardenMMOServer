//! Opt-in parity against the owner's private, already decoded A2NE payload.
use tiny_garden_core::{
    gba,
    shop::{self, TinyShopCatalog},
};

#[test]
#[ignore = "requires CHAO_TINY_GARDEN_PAYLOAD pointing to the owner's verified local payload"]
fn verified_local_payload_preserves_all_seven_fruit_rows_and_prices() {
    let path = std::env::var_os("CHAO_TINY_GARDEN_PAYLOAD").expect("Set CHAO_TINY_GARDEN_PAYLOAD");
    assert_eq!(
        std::fs::metadata(&path).unwrap().len(),
        gba::REFERENCE_GARDEN_LEN as u64
    );
    let payload = std::fs::read(path).unwrap();
    assert_eq!(gba::sha1(&payload), gba::REFERENCE_GARDEN_SHA1);
    assert_eq!(
        gba::sha256(&payload[shop::EFFECTS_OFFSET..shop::EFFECTS_OFFSET + shop::EFFECTS_LEN]),
        shop::EFFECTS_SHA256
    );
    assert_eq!(
        gba::sha256(&payload[shop::PRICES_OFFSET..shop::PRICES_OFFSET + shop::PRICES_LEN]),
        shop::PRICES_SHA256
    );
    let catalog = TinyShopCatalog::from_verified_payload(&payload).unwrap();
    assert_eq!(catalog.fruits.len(), 7);
    for (index, fruit) in catalog.fruits.iter().enumerate() {
        assert_eq!(usize::from(fruit.id), index);
        let effects =
            &payload[shop::EFFECTS_OFFSET + index * 7..shop::EFFECTS_OFFSET + (index + 1) * 7];
        assert_eq!(
            fruit.raw.as_slice(),
            effects.iter().map(|byte| *byte as i8).collect::<Vec<_>>()
        );
        let prices =
            &payload[shop::PRICES_OFFSET + index * 4..shop::PRICES_OFFSET + (index + 1) * 4];
        assert_eq!(fruit.price, u32::from_le_bytes(prices.try_into().unwrap()));
        assert!(fruit.price > 0);
    }
    // A same-sized mutation must fail the full identity check, not be parsed.
    let mut changed = payload.clone();
    changed[shop::EFFECTS_OFFSET] ^= 1;
    assert_eq!(
        TinyShopCatalog::from_verified_payload(&changed),
        Err(shop::Error::UnsupportedPayloadSha1)
    );
}
