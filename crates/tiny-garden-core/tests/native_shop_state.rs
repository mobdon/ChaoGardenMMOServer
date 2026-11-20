use tiny_garden_core::{native_growth::NativeRng, native_shop_state::*, shop};

fn prices() -> NativeShopPrices {
    // Deliberately synthetic prices; real native values require the opt-in gate.
    NativeShopPrices {
        fruit: [11, 22, 33, 44, 55, 66, 77],
        egg: [13; 12],
        toy: [17, 29, 41],
    }
}

fn state() -> NativeShopState {
    NativeShopState::restore(NativeShopFields::default(), 10_000).unwrap()
}

fn input(item: NativeShopItem) -> NativePurchaseInput {
    NativePurchaseInput {
        cursor_x: 40,
        cursor_y: item.row_y().unwrap() - 8,
        pressed_keys: 1,
        ..NativePurchaseInput::default()
    }
}

fn seed_before_state(after: u32) -> u32 {
    const INVERSE: u32 = 0xeeb9_eb65;
    assert_eq!(0x41c6_4e6d_u32.wrapping_mul(INVERSE), 1);
    after.wrapping_sub(0x3039).wrapping_mul(INVERSE)
}

fn seed_for_first_roll(roll: u16) -> u32 {
    seed_before_state(u32::from(roll) << 16)
}

fn seed_for_jewel_roll(roll: u16) -> u32 {
    // Invert the actual LCG to cover the second-roll branch boundaries while
    // ensuring the preceding native eligibility roll is <= 0x1FFF.
    for low in 0..=u16::MAX {
        let second = (u32::from(roll) << 16) | u32::from(low);
        let first = seed_before_state(second);
        if (first.wrapping_mul(2) >> 17) <= 0x1fff {
            return seed_before_state(first);
        }
    }
    panic!("no suitable native seed");
}

#[test]
fn native_stock_keeps_all_fruits_first_missing_toy_and_nonrandom_cycle() {
    for frame in 0..10 {
        let mut s = state();
        let mut rng = NativeRng::new(0x12345678);
        let stock = s.open_stock(frame, 0, &mut rng).unwrap().clone();
        assert_eq!(stock.fruit_available, [true; 7]);
        assert_eq!(
            (stock.egg, stock.toy, stock.random_calls),
            (Some(0), Some(0), 0)
        );
        assert_eq!(s.fields.egg_cycle, (frame % 5) as u8);
        assert_eq!(rng.state(), 0x12345678);
        assert_eq!(s.fields.shop_visited_ce, 1);
        s.close_stock();
        s.open_stock(frame + 1, 0, &mut rng).unwrap();
        assert_eq!(s.fields.egg_cycle, (frame % 5) as u8);
    }
    assert_eq!(
        (
            STOCK_ADDRESS,
            FRUIT_PURCHASE_ADDRESS,
            EGG_PURCHASE_ADDRESS,
            TOY_PURCHASE_ADDRESS
        ),
        (0x02007688, 0x02007978, 0x02007b00, 0x02007c4c)
    );
}

#[test]
fn unsupported_signed_native_frame_rollover_is_rejected_before_state_or_rng_mutation() {
    let mut s = state();
    let before = s.clone();
    let mut rng = NativeRng::new(123);
    assert_eq!(
        s.open_stock(0x8000_0000, 0, &mut rng),
        Err(Error::UnsupportedFrameCounter)
    );
    assert_eq!(s, before);
    assert_eq!(rng.state(), 123);
}

#[test]
fn ordinary_egg_all_32768_native_draws_match_inclusive_thresholds() {
    let mut counts = [0_u32; 7];
    for roll in 0..=0x7fff_u16 {
        let mut s = state();
        s.fields.chao_field_58 = 0;
        s.fields.jewel_field_68 = 1;
        let mut rng = NativeRng::new(seed_for_first_roll(roll));
        let stock = s.open_stock(0, 0, &mut rng).unwrap();
        let expected = [
            (0x1999, 0),
            (0x3332, 2),
            (0x4665, 1),
            (0x5998, 3),
            (0x6665, 4),
            (0x7332, 6),
            (0x7fff, 5),
        ]
        .into_iter()
        .find(|&(limit, _)| roll <= limit)
        .unwrap()
        .1;
        assert_eq!(stock.egg, Some(expected), "roll {roll:x}");
        assert_eq!(stock.random_calls, 1);
        counts[usize::from(expected)] += 1;
    }
    assert_eq!(counts, [6554, 4915, 6553, 4915, 3277, 3277, 3277]);
}

#[test]
fn jewel_threshold_boundaries_rotate_by_saved_cycle_and_cache_consumes_no_more_rng() {
    for cycle in 0..5 {
        for (roll, index) in [
            (0, 1),
            (0x40a3, 1),
            (0x40a4, 0),
            (0x4147, 0),
            (0x4148, 3),
            (0x4b84, 3),
            (0x4b85, 4),
            (0x65c1, 4),
            (0x65c2, 2),
            (0x7fff, 2),
        ] {
            let mut s = state();
            s.fields.chao_field_58 = 0;
            s.fields.egg_cycle = cycle;
            let mut rng = NativeRng::new(seed_for_jewel_roll(roll));
            let stock = s.open_stock(99, 0, &mut rng).unwrap().clone();
            assert_eq!(stock.egg, Some(7 + (cycle + index) % 5));
            assert_eq!(stock.random_calls, 2);
            assert_eq!(s.cached_egg(), stock.egg);
            let seed = rng.state();
            s.close_stock();
            let again = s.open_stock(101, 0, &mut rng).unwrap();
            assert_eq!(again.egg, stock.egg);
            assert_eq!(again.random_calls, 0);
            assert_eq!(rng.state(), seed);
        }
    }
}

#[test]
fn egg_block_gates_empty_chao_shortcut_and_failed_jewel_roll_preserve_native_rng_call_order() {
    for occupied in [false, true] {
        let mut s = state();
        s.fields.chao_field_58 = 0;
        if occupied {
            s.fields.egg_id = 3;
        }
        let mut rng = NativeRng::new(123);
        let stock = s.open_stock(8, u8::from(!occupied), &mut rng).unwrap();
        assert_eq!(stock.egg, None);
        assert_eq!(stock.random_calls, 0);
        assert_eq!(rng.state(), 123);
        assert_eq!(s.cached_egg(), None);
    }
    let mut s = state();
    s.fields.chao_field_58 = 0;
    let mut rng = NativeRng::new(seed_for_first_roll(0x2000));
    let mut expected_rng = rng;
    assert_eq!(expected_rng.next_u15(), 0x2000);
    expected_rng.next_u15();
    let stock = s.open_stock(1, 0, &mut rng).unwrap();
    assert!(stock.egg.unwrap() < 7);
    assert_eq!(stock.random_calls, 2);
    assert_eq!(rng.state(), expected_rng.state());
}

#[test]
fn toy_unlock_uses_native_task_updates_and_only_flag_10_pauses_it() {
    let mut s = state();
    s.fields.toy_positions[0].x = 8;
    s.fields.toy_positions[1].x = 9;
    s.fields.toy_unlock_ticks = 3;
    assert_eq!(s.fields.next_toy(), None);
    s.advance_shop_ticks(100, 0x10);
    assert_eq!(s.fields.toy_unlock_ticks, 3);
    s.advance_shop_ticks(1, 0x80);
    assert_eq!(s.fields.toy_unlock_ticks, 2);
    s.advance_shop_ticks(2, 0);
    assert_eq!(s.fields.toy_unlock_ticks, 0);
    assert_eq!(s.fields.next_toy(), Some(2));
    s.advance_shop_ticks(u32::MAX, 0);
    assert_eq!(s.fields.toy_unlock_ticks, 0);
    s.fields.toy_positions[2].x = 10;
    assert_eq!(s.fields.next_toy(), None);
    s.fields.toy_positions[0].x = 0;
    assert_eq!(s.fields.next_toy(), Some(0));
}

#[test]
fn fruit_purchase_uses_first_empty_slot_exact_wallet_capacity_and_one_subject_per_opening() {
    let item = NativeShopItem::Fruit(3);
    let mut s = state();
    s.fields.fruit_ids = [0, 1, EMPTY_ID, 2, EMPTY_ID, 4, 5, 6];
    s.rings = 44;
    s.open_stock(0, 0, &mut NativeRng::new(0)).unwrap();
    let p = s.purchase(&prices(), item, input(item)).unwrap();
    assert_eq!(
        (p.price, p.rings_after, p.fruit_slot, p.native_dirty),
        (44, 0, Some(2), true)
    );
    assert_eq!(s.fields.fruit_ids[2], 3);
    assert_eq!(s.fields.fruit_positions[2], NativePosition { x: 40, y: 85 });
    assert_eq!(
        s.held,
        Some(NativeHeldItem {
            item,
            fruit_slot: Some(2)
        })
    );
    let before = s.clone();
    assert_eq!(
        s.open_stock(3, 0, &mut NativeRng::new(0)),
        Err(Error::HoldingItem)
    );
    assert_eq!(s, before);
    // Host finishing the native held-item controller is outside this slice.
    s.held = None;
    s.rings = 44;
    let before = s.clone();
    assert_eq!(
        s.purchase(&prices(), item, input(item)),
        Err(Error::NotOffered)
    );
    assert_eq!(s, before);
    s.open_stock(3, 0, &mut NativeRng::new(0)).unwrap();
    s.fields.fruit_ids = [0; 8];
    let before = s.clone();
    assert_eq!(
        s.purchase(&prices(), item, input(item)),
        Err(Error::FruitCapacity)
    );
    assert_eq!(s, before);
}

#[test]
fn egg_purchase_and_sequential_toys_write_native_positions_and_duck_timer_only() {
    let mut s = state();
    s.open_stock(0, 0, &mut NativeRng::new(0)).unwrap();
    let item = NativeShopItem::Egg(0);
    s.purchase(&prices(), item, input(item)).unwrap();
    assert_eq!(s.fields.egg_id, 0);
    assert_eq!(s.fields.egg_position, NativePosition { x: 40, y: 149 });
    s.held = None;
    let item = NativeShopItem::Toy(0);
    s.purchase(&prices(), item, input(item)).unwrap();
    assert_eq!(s.fields.toy_positions[0], NativePosition { x: 40, y: 165 });
    assert_eq!(s.fields.toy_unlock_ticks, 0);
    s.held = None;
    s.fields.duck_field_cf = 99;
    s.open_stock(1, 0, &mut NativeRng::new(0)).unwrap();
    let item = NativeShopItem::Toy(1);
    s.purchase(&prices(), item, input(item)).unwrap();
    assert_eq!(s.fields.toy_unlock_ticks, 216_000);
    assert_eq!(s.fields.duck_field_cf, 0);
    s.held = None;
    s.open_stock(2, 0, &mut NativeRng::new(0)).unwrap();
    assert_eq!(s.stock().unwrap().toy, None);
    s.advance_shop_ticks(TOY_UNLOCK_TICKS, 0);
    // Opening, rather than elapsed time alone, rebuilds the displayed stock.
    assert_eq!(s.stock().unwrap().toy, None);
    s.open_stock(3, 0, &mut NativeRng::new(0)).unwrap();
    let item = NativeShopItem::Toy(2);
    s.purchase(&prices(), item, input(item)).unwrap();
    assert_eq!(s.fields.next_toy(), None);
}

#[test]
fn source_input_boundaries_and_rejected_purchases_never_mutate_state() {
    let item = NativeShopItem::Fruit(0);
    let good = input(item);
    let variants = [
        NativePurchaseInput {
            global_flags: 0x10,
            ..good
        },
        NativePurchaseInput {
            global_flags: 0x80,
            ..good
        },
        NativePurchaseInput {
            panel_x: -1,
            ..good
        },
        NativePurchaseInput {
            cursor_x: 52,
            ..good
        },
        NativePurchaseInput {
            modal_1509: 1,
            ..good
        },
        NativePurchaseInput {
            pressed_keys: 0,
            ..good
        },
        NativePurchaseInput {
            held_keys: 0x10,
            ..good
        },
        NativePurchaseInput {
            cursor_y: 2,
            ..good
        },
        NativePurchaseInput {
            cursor_y: 19,
            ..good
        },
    ];
    for bad in variants {
        let mut s = state();
        s.open_stock(0, 0, &mut NativeRng::new(0)).unwrap();
        let before = s.clone();
        assert_eq!(s.purchase(&prices(), item, bad), Err(Error::InputBlocked));
        assert_eq!(s, before);
    }
    for y in [3, 18] {
        let mut s = state();
        s.open_stock(0, 0, &mut NativeRng::new(0)).unwrap();
        s.purchase(
            &prices(),
            item,
            NativePurchaseInput {
                global_flags: 0x24,
                cursor_x: 51,
                cursor_y: y,
                ..good
            },
        )
        .unwrap();
    }
    let mut s = state();
    s.rings = 10;
    s.open_stock(0, 0, &mut NativeRng::new(0)).unwrap();
    let before = s.clone();
    assert_eq!(
        s.purchase(&prices(), item, good),
        Err(Error::InsufficientRings)
    );
    assert_eq!(s, before);
    assert_eq!(
        s.purchase(&prices(), NativeShopItem::Toy(3), good),
        Err(Error::InvalidItem)
    );
    assert_eq!(s, before);
}

#[test]
fn raw_fields_preserve_opaque_bytes_and_reject_truncated_or_unknown_ids_before_writing() {
    let mut record = [0x5a; 0x120];
    let before = record;
    let fields = NativeShopFields::default();
    fields.write_raw_fields(&mut record).unwrap();
    assert_eq!(NativeShopFields::read_raw_fields(&record).unwrap(), fields);
    for (i, byte) in record.iter().enumerate() {
        if (0x14..0x18).contains(&i)
            || i == 0x1c
            || (0x1f..0x37).contains(&i)
            || (0x58..0x62).contains(&i)
            || i == 0x68
            || i == 0xce
            || i == 0xcf
        {
            continue;
        }
        assert_eq!(*byte, before[i], "opaque {i:x}");
    }
    assert!(NativeShopFields::read_raw_fields(&record[..0xcf]).is_err());
    let mut short = [0x5a; 0xcf];
    let before = short;
    assert!(fields.write_raw_fields(&mut short).is_err());
    assert_eq!(short, before);
    for (offset, value) in [(0x1c, 5), (0x59, 12), (0x5a, 7)] {
        let mut bad = record;
        bad[offset] = value;
        assert!(NativeShopFields::read_raw_fields(&bad).is_err());
    }
    let mut invalid = fields;
    invalid.egg_id = 12;
    let before = record;
    assert!(invalid.write_raw_fields(&mut record).is_err());
    assert_eq!(record, before);
}

#[test]
#[ignore = "requires CHAO_TINY_GARDEN_PAYLOAD, the owner's exact private decoded A2NE payload"]
fn actual_verified_native_prices_reuse_existing_source_gate_and_reject_corruption() {
    let path = std::env::var_os("CHAO_TINY_GARDEN_PAYLOAD").expect("Set CHAO_TINY_GARDEN_PAYLOAD");
    let payload = std::fs::read(path).unwrap();
    let p = NativeShopPrices::from_verified_payload(&payload).unwrap();
    assert_eq!(p.fruit, [30, 60, 55, 50, 30, 55, 70]);
    assert_eq!(
        p.egg,
        [
            0, 1000, 500, 5000, 7000, 10000, 8000, 14000, 12000, 20000, 16000, 18000
        ]
    );
    assert_eq!(p.toy, [1000, 2000, 8000]);
    let mut altered = payload.clone();
    altered[EGG_PRICES_OFFSET] ^= 1;
    assert!(matches!(
        NativeShopPrices::from_verified_payload(&altered),
        Err(Error::Catalog(shop::Error::UnsupportedPayloadSha1))
    ));
    assert!(NativeShopPrices::from_verified_payload(&payload[..payload.len() - 1]).is_err());
    let mut altered = payload;
    altered[TOY_PRICES_OFFSET] ^= 1;
    assert!(NativeShopPrices::from_verified_payload(&altered).is_err());
}
