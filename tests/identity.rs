use chao_game_server::{game::hash_token, identity::*};
use serde_json::{Value, json};

fn legacy() -> Value {
    json!({"id":"canonical-chao","tokenHash":hash_token("desktop-secret"),
        "name":"Earned","admin":false,"rings":4382,"stats":{"run":{"grade":4,"level":73,"points":8123}},
        "tinyRecords":{"memory":{"plays":12,"completions":9,"ringsEarned":487,"lastResultId":"reward-once"}},
        "raceRecords":{"beginner":{"wins":4}},"resident":{"enabled":true,"zone":"garden","x":2,"z":8}})
}

fn identity() -> (AccountIdentity, String) {
    let identity = AccountIdentity::from_profile(&legacy()).unwrap();
    let desktop = identity.authenticate("desktop-secret").unwrap();
    (identity, desktop)
}

fn pair(identity: &mut AccountIdentity, desktop: &str, now: u64) -> Value {
    let invite = identity.create_pairing(desktop, now).unwrap();
    identity
        .claim_pairing(
            invite["pairingId"].as_str().unwrap(),
            invite["pairingSecret"].as_str().unwrap(),
            now + 1,
        )
        .unwrap()
}

#[test]
fn legacy_migration_changes_only_identity_and_is_stable_after_persistence() {
    let mut profile = legacy();
    let original = profile.clone();
    let identity = AccountIdentity::from_profile(&profile).unwrap();
    assert_eq!(identity.chao_id, "canonical-chao");
    assert_eq!(identity.revision, 1);
    assert_eq!(identity.devices.len(), 1);
    assert_eq!(identity.devices[0].kind, "desktop");
    assert!(identity.authenticate("desktop-secret").is_some());
    profile["identity"] = serde_json::to_value(&identity).unwrap();
    assert_eq!(AccountIdentity::from_profile(&profile).unwrap(), identity);
    profile.as_object_mut().unwrap().remove("identity");
    assert_eq!(profile, original);
}

#[test]
fn corrupt_or_future_identity_never_falls_back_to_legacy_authentication() {
    let mut profile = legacy();
    profile["identity"] = Value::Null;
    assert_eq!(
        AccountIdentity::from_profile(&profile).unwrap_err(),
        IdentityError("invalid_identity")
    );
    let (identity, _) = identity();
    profile["identity"] = serde_json::to_value(identity).unwrap();
    profile["identity"]["schemaVersion"] = json!(99);
    assert!(AccountIdentity::from_profile(&profile).is_err());
    profile["identity"]["schemaVersion"] = json!(SCHEMA_VERSION);
    profile["identity"]["chaoId"] = json!("different-chao");
    assert!(AccountIdentity::from_profile(&profile).is_err());
}

#[test]
fn duplicate_device_identity_and_hash_fail_closed() {
    let (identity, _) = identity();
    let mut profile = legacy();
    profile["identity"] = serde_json::to_value(identity).unwrap();
    let first = profile["identity"]["devices"][0].clone();
    profile["identity"]["devices"]
        .as_array_mut()
        .unwrap()
        .push(first);
    assert!(AccountIdentity::from_profile(&profile).is_err());
    profile["identity"]["devices"][1]["id"] = json!("different-id-same-key");
    assert!(AccountIdentity::from_profile(&profile).is_err());
}

#[test]
fn unusable_hash_encodings_are_rejected_during_load() {
    for hash in [
        "".to_owned(),
        "a".repeat(63),
        "G".repeat(64),
        hash_token("desktop-secret").to_ascii_uppercase(),
    ] {
        let mut profile = legacy();
        profile["tokenHash"] = json!(hash);
        assert!(AccountIdentity::from_profile(&profile).is_err());
    }
}

#[test]
fn approved_invitation_is_short_lived_one_time_and_creates_no_new_chao() {
    let (mut identity, desktop) = identity();
    let original_account = identity.account_id.clone();
    let invite = identity.create_pairing(&desktop, 10_000).unwrap();
    assert_eq!(invite["expiresAt"], 10_000 + PAIRING_TTL_MS);
    let pairing_id = invite["pairingId"].as_str().unwrap();
    let secret = invite["pairingSecret"].as_str().unwrap();
    let result = identity.claim_pairing(pairing_id, secret, 10_001).unwrap();
    assert_eq!(identity.chao_id, "canonical-chao");
    assert_eq!(identity.account_id, original_account);
    let android = result["deviceId"].as_str().unwrap();
    assert_eq!(identity.device_kind(android), Some("android"));
    assert_eq!(
        identity
            .authenticate(result["token"].as_str().unwrap())
            .as_deref(),
        Some(android)
    );
    assert_eq!(
        identity
            .claim_pairing(pairing_id, secret, 10_002)
            .unwrap_err(),
        IdentityError("invalid_pairing")
    );
    assert_eq!(identity.devices.len(), 2);
}

#[test]
fn expired_wrong_or_replaced_pairing_does_not_mutate_identity() {
    let (mut identity, desktop) = identity();
    let first = identity.create_pairing(&desktop, 0).unwrap();
    let before = identity.clone();
    for (id, secret, now) in [
        ("unknown", first["pairingSecret"].as_str().unwrap(), 1),
        (first["pairingId"].as_str().unwrap(), "wrong", 1),
        (
            first["pairingId"].as_str().unwrap(),
            first["pairingSecret"].as_str().unwrap(),
            PAIRING_TTL_MS,
        ),
    ] {
        assert_eq!(
            identity.claim_pairing(id, secret, now).unwrap_err(),
            IdentityError("invalid_pairing")
        );
        assert_eq!(identity, before);
    }
    identity.create_pairing(&desktop, 2).unwrap();
    assert!(
        identity
            .claim_pairing(
                first["pairingId"].as_str().unwrap(),
                first["pairingSecret"].as_str().unwrap(),
                3
            )
            .is_err()
    );
}

#[test]
fn android_cannot_pair_or_revoke_another_device() {
    let (mut identity, desktop) = identity();
    let paired = pair(&mut identity, &desktop, 0);
    let android = paired["deviceId"].as_str().unwrap();
    assert_eq!(
        identity.create_pairing(android, 10).unwrap_err(),
        IdentityError("desktop_approval_required")
    );
    assert_eq!(
        identity.revoke(android, &desktop).unwrap_err(),
        IdentityError("desktop_approval_required")
    );
}

#[test]
fn durable_revocation_invalidates_authentication_and_held_lease() {
    let (mut identity, desktop) = identity();
    let paired = pair(&mut identity, &desktop, 0);
    let android = paired["deviceId"].as_str().unwrap();
    let lease = identity
        .acquire(android, "tiny", identity.revision, 1)
        .unwrap();
    identity.revoke(&desktop, android).unwrap();
    assert!(identity.lease.is_none());
    assert!(
        identity
            .authenticate(paired["token"].as_str().unwrap())
            .is_none()
    );
    assert_eq!(
        identity
            .validate(android, &lease.id, identity.revision, 2)
            .unwrap_err(),
        IdentityError("device_revoked")
    );
    let mut profile = legacy();
    profile["identity"] = serde_json::to_value(&identity).unwrap();
    let restored = AccountIdentity::from_profile(&profile).unwrap();
    assert!(
        restored
            .authenticate(paired["token"].as_str().unwrap())
            .is_none()
    );
    assert!(restored.authenticate("desktop-secret").is_some());
}

#[test]
fn desktop_revocation_cancels_its_outstanding_pairing_and_has_no_legacy_bypass() {
    let (mut identity, desktop) = identity();
    let invite = identity.create_pairing(&desktop, 0).unwrap();
    identity.revoke(&desktop, &desktop).unwrap();
    assert!(identity.pairing.is_none());
    assert!(identity.authenticate("desktop-secret").is_none());
    assert_eq!(
        identity
            .claim_pairing(
                invite["pairingId"].as_str().unwrap(),
                invite["pairingSecret"].as_str().unwrap(),
                1
            )
            .unwrap_err(),
        IdentityError("invalid_pairing")
    );
    let mut profile = legacy();
    profile["identity"] = serde_json::to_value(&identity).unwrap();
    assert!(
        AccountIdentity::from_profile(&profile)
            .unwrap()
            .authenticate("desktop-secret")
            .is_none()
    );
}

#[test]
fn leases_fence_concurrent_devices_stale_revisions_and_replayed_ownership() {
    let (mut identity, desktop) = identity();
    let paired = pair(&mut identity, &desktop, 0);
    let android = paired["deviceId"].as_str().unwrap();
    let prior_revision = identity.revision;
    let lease = identity
        .acquire(&desktop, "direct", prior_revision, 5)
        .unwrap();
    assert_eq!(
        identity
            .acquire(android, "tiny", prior_revision, 6)
            .unwrap_err(),
        IdentityError("stale_revision")
    );
    assert_eq!(
        identity
            .acquire(android, "tiny", identity.revision, 6)
            .unwrap_err(),
        IdentityError("activity_conflict")
    );
    assert_eq!(
        identity
            .validate(android, &lease.id, identity.revision, 6)
            .unwrap_err(),
        IdentityError("stale_lease")
    );
    assert_eq!(
        identity
            .validate(&desktop, "previous-lease", identity.revision, 6)
            .unwrap_err(),
        IdentityError("stale_lease")
    );
    assert_eq!(
        identity
            .validate(&desktop, &lease.id, prior_revision, 6)
            .unwrap_err(),
        IdentityError("stale_revision")
    );
    identity
        .release(&desktop, &lease.id, identity.revision, 6)
        .unwrap();
    let next = identity
        .acquire(android, "tiny", identity.revision, 7)
        .unwrap();
    assert_ne!(next.id, lease.id);
    assert_eq!(
        identity
            .validate(&desktop, &lease.id, identity.revision, 8)
            .unwrap_err(),
        IdentityError("stale_lease")
    );
}

#[test]
fn reconnect_renews_existing_lease_and_expiry_requires_a_new_generation() {
    let (mut identity, desktop) = identity();
    let lease = identity
        .acquire(&desktop, "direct", identity.revision, 10)
        .unwrap();
    let mut profile = legacy();
    profile["identity"] = serde_json::to_value(&identity).unwrap();
    let mut restored = AccountIdentity::from_profile(&profile).unwrap();
    let renewed = restored
        .renew(&desktop, &lease.id, restored.revision, 100)
        .unwrap();
    assert_eq!(renewed.id, lease.id);
    assert_eq!(renewed.expires_at, 100 + LEASE_TTL_MS);
    assert_eq!(
        restored
            .validate(&desktop, &lease.id, restored.revision, renewed.expires_at)
            .unwrap_err(),
        IdentityError("lease_expired")
    );
    assert_eq!(
        restored
            .renew(&desktop, &lease.id, restored.revision, renewed.expires_at)
            .unwrap_err(),
        IdentityError("lease_expired")
    );
    let next = restored
        .acquire(&desktop, "direct", restored.revision, renewed.expires_at)
        .unwrap();
    assert_ne!(next.id, lease.id);
}

#[test]
fn persistence_and_debug_never_contain_raw_issued_secrets() {
    let (mut identity, desktop) = identity();
    let invite = identity.create_pairing(&desktop, 0).unwrap();
    let secret = invite["pairingSecret"].as_str().unwrap();
    let serialized = serde_json::to_string(&identity).unwrap();
    assert!(!serialized.contains(secret));
    let debug = format!("{identity:?}");
    assert!(!debug.contains(secret));
    assert!(!debug.contains(&identity.devices[0].credential_hash));
    assert!(
        !identity
            .public(&desktop)
            .to_string()
            .contains(&identity.devices[0].credential_hash)
    );
    let paired = identity
        .claim_pairing(invite["pairingId"].as_str().unwrap(), secret, 1)
        .unwrap();
    assert!(
        !serde_json::to_string(&identity)
            .unwrap()
            .contains(paired["token"].as_str().unwrap())
    );
}

#[test]
fn overflow_or_invalid_activity_rejects_without_partially_changing_ownership() {
    let (mut identity, desktop) = identity();
    let before = identity.clone();
    assert_eq!(
        identity
            .acquire(&desktop, "grant-rings", identity.revision, 0)
            .unwrap_err(),
        IdentityError("invalid_activity")
    );
    assert_eq!(identity, before);
    assert_eq!(
        identity.create_pairing(&desktop, u64::MAX).unwrap_err(),
        IdentityError("invalid_server_time")
    );
    assert_eq!(identity, before);
    identity.revision = u64::MAX;
    let before = identity.clone();
    assert_eq!(
        identity
            .acquire(&desktop, "direct", u64::MAX, 0)
            .unwrap_err(),
        IdentityError("revision_exhausted")
    );
    assert_eq!(identity, before);
}

#[test]
fn intent_sequences_reject_duplicate_reordered_and_zero_inputs_without_mutation() {
    let (mut identity, desktop) = identity();
    let lease = identity
        .acquire(&desktop, "watch", identity.revision, 10)
        .unwrap();
    let revision = identity.revision;
    for sequence in [1, 2, 9] {
        identity
            .accept_intent(&desktop, &lease.id, revision, sequence, 11)
            .unwrap();
    }
    assert_eq!(identity.lease.as_ref().unwrap().last_sequence, 9);
    assert_eq!(identity.revision, revision);
    let before = identity.clone();
    for sequence in [0, 1, 2, 8, 9] {
        assert_eq!(
            identity
                .accept_intent(&desktop, &lease.id, revision, sequence, 12)
                .unwrap_err(),
            IdentityError("replayed_intent")
        );
        assert_eq!(identity, before);
    }
}

#[test]
fn intent_sequence_survives_atomic_profile_reload_and_reconnect_renewal() {
    let (mut identity, desktop) = identity();
    let lease = identity
        .acquire(&desktop, "tiny", identity.revision, 10)
        .unwrap();
    identity
        .accept_intent(&desktop, &lease.id, identity.revision, 32, 11)
        .unwrap();
    // The host persists this identity in the same profile as the earned change.
    identity.bump().unwrap();
    let mut profile = legacy();
    profile["rings"] = json!(4482);
    profile["identity"] = serde_json::to_value(&identity).unwrap();
    let mut restored = AccountIdentity::from_profile(&profile).unwrap();
    let renewed = restored
        .renew(&desktop, &lease.id, restored.revision, 100)
        .unwrap();
    assert_eq!(renewed.id, lease.id);
    assert_eq!(renewed.last_sequence, 32);
    assert_eq!(
        restored
            .accept_intent(&desktop, &lease.id, restored.revision, 32, 101)
            .unwrap_err(),
        IdentityError("replayed_intent")
    );
    restored
        .accept_intent(&desktop, &lease.id, restored.revision, 33, 101)
        .unwrap();
    assert_eq!(profile["rings"], 4482);
    assert_eq!(restored.public(&desktop)["lease"]["lastSequence"], 33);
}

#[test]
fn intent_fencing_checks_expiry_revocation_and_old_lease_generation() {
    let (mut identity, desktop) = identity();
    let paired = pair(&mut identity, &desktop, 0);
    let android = paired["deviceId"].as_str().unwrap();
    let lease = identity
        .acquire(android, "tiny", identity.revision, 10)
        .unwrap();
    assert_eq!(
        identity
            .accept_intent(android, &lease.id, identity.revision, 1, lease.expires_at)
            .unwrap_err(),
        IdentityError("lease_expired")
    );
    assert_eq!(identity.lease.as_ref().unwrap().last_sequence, 0);
    let replacement = identity
        .acquire(android, "tiny", identity.revision, lease.expires_at)
        .unwrap();
    assert_eq!(
        identity
            .accept_intent(android, &lease.id, identity.revision, 1, lease.expires_at)
            .unwrap_err(),
        IdentityError("stale_lease")
    );
    identity.revoke(&desktop, android).unwrap();
    assert_eq!(
        identity
            .accept_intent(
                android,
                &replacement.id,
                identity.revision,
                1,
                lease.expires_at,
            )
            .unwrap_err(),
        IdentityError("device_revoked")
    );
}
