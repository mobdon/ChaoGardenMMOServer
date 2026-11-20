use tiny_garden_core::{native_bios_math, native_bounce, native_memory};

#[test]
fn version_gate_rejects_missing_payload_and_native_addresses_are_explicit() {
    assert!(native_bounce::A2neBounceSource::from_verified_payload(&[]).is_err());
    assert_eq!(native_bounce::INITIALIZE_ADDRESS, 0x0200_d294);
    assert_eq!(native_bounce::CHAO_ADDRESS, 0x0200_d958);
    assert_eq!(native_bounce::SPRING_ADDRESS, 0x0200_e964);
    assert_eq!(native_bounce::TARGET_ADDRESS, 0x0200_dd60);
    assert_eq!(native_bounce::MAX_NATIVE_RINGS, 113);
}

#[test]
#[ignore = "requires CHAO_TINY_GARDEN_PAYLOAD exact owner's decoded A2NE program"]
fn real_payload_can_play_until_a_durable_host_boundary_without_mutating_any_save() {
    let path = std::env::var_os("CHAO_TINY_GARDEN_PAYLOAD").expect("private payload");
    let payload = std::fs::read(path).unwrap();
    let source = native_bounce::A2neBounceSource::from_verified_payload(&payload).unwrap();
    let mut round = source.begin_round();
    let bios = native_bios_math::ReferenceBiosMath;
    let before = round.view();
    assert_eq!(
        round.step(
            native_memory::NativeButtons {
                pressed: 0x100,
                held: 0
            },
            &bios
        ),
        Err(native_bounce::Error::InvalidInput)
    );
    assert_eq!(round.view(), before);
    for frame in 0..20000 {
        if round.finished().is_some() {
            break;
        }
        round
            .step(
                native_memory::NativeButtons {
                    pressed: if frame % 60 == 0 { 1 } else { 0 },
                    held: 0,
                },
                &bios,
            )
            .unwrap();
    }
    assert!(round.finished().is_some());
    assert_eq!(round.view().phase, native_bounce::BouncePhase::Ended);
    let before = round.view();
    round
        .step(native_memory::NativeButtons::default(), &bios)
        .unwrap();
    assert_eq!(round.view(), before);
}
