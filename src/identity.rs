//! Device authentication and exclusive activity ownership for one canonical Chao.
//!
//! Mutations are proposals: the caller must persist a cloned profile before
//! exposing it or returning a newly issued secret. No raw secret is serialized.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 2;
pub const PAIRING_TTL_MS: u64 = 120_000;
pub const LEASE_TTL_MS: u64 = 30_000;
pub const MAX_DEVICES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdentityError(pub &'static str);

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}
impl std::error::Error for IdentityError {}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Device {
    pub id: String,
    pub kind: String,
    pub credential_hash: String,
    pub revoked: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pairing {
    pub id: String,
    pub secret_hash: String,
    pub created_by: String,
    pub expires_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityLease {
    pub id: String,
    pub device_id: String,
    pub activity: String,
    pub expires_at: u64,
    #[serde(default)]
    pub last_sequence: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountIdentity {
    pub schema_version: u32,
    pub account_id: String,
    pub chao_id: String,
    pub revision: u64,
    pub devices: Vec<Device>,
    pub pairing: Option<Pairing>,
    pub lease: Option<ActivityLease>,
}

// Player is Debug; its identity must never expand secret hashes/challenges into
// a diagnostic, even when an enclosing object is logged during an error.
impl fmt::Debug for AccountIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccountIdentity")
            .field("schema_version", &self.schema_version)
            .field("revision", &self.revision)
            .field("device_count", &self.devices.len())
            .field("pairing_pending", &self.pairing.is_some())
            .field("lease", &self.lease)
            .finish()
    }
}

fn digest(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

fn same_hash(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

fn valid_activity(value: &str) -> bool {
    matches!(
        value,
        "direct" | "tiny" | "watch" | "training" | "competition"
    )
}

fn random_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

impl AccountIdentity {
    pub fn new(chao_id: &str, token_hash: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            account_id: Uuid::new_v4().to_string(),
            chao_id: chao_id.into(),
            revision: 1,
            devices: vec![Device {
                id: Uuid::new_v4().to_string(),
                kind: "desktop".into(),
                credential_hash: token_hash.into(),
                revoked: false,
            }],
            pairing: None,
            lease: None,
        }
    }

    /// A missing identity migrates a legacy profile once. A present but invalid
    /// identity fails closed, including `null`; it never restores revoked keys.
    pub fn from_profile(profile: &Value) -> Result<Self, IdentityError> {
        let chao_id = profile["id"]
            .as_str()
            .filter(|value| valid_id(value))
            .ok_or(IdentityError("invalid_identity"))?;
        let identity = if let Some(value) = profile.get("identity") {
            serde_json::from_value::<Self>(value.clone())
                .map_err(|_| IdentityError("invalid_identity"))?
        } else {
            let token_hash = profile["tokenHash"]
                .as_str()
                .filter(|value| valid_hash(value))
                .ok_or(IdentityError("invalid_identity"))?;
            Self::new(chao_id, token_hash)
        };
        identity.validate_stored(chao_id)?;
        Ok(identity)
    }

    fn validate_stored(&self, chao_id: &str) -> Result<(), IdentityError> {
        let invalid = || IdentityError("invalid_identity");
        if self.schema_version != SCHEMA_VERSION
            || self.chao_id != chao_id
            || !valid_id(&self.account_id)
            || self.revision == 0
            || self.devices.is_empty()
            || self.devices.len() > MAX_DEVICES
        {
            return Err(invalid());
        }
        let mut ids = BTreeSet::new();
        let mut hashes = BTreeSet::new();
        for device in &self.devices {
            if !valid_id(&device.id)
                || !matches!(device.kind.as_str(), "desktop" | "android")
                || !valid_hash(&device.credential_hash)
                || !ids.insert(&device.id)
                || !hashes.insert(device.credential_hash.to_ascii_lowercase())
            {
                return Err(invalid());
            }
        }
        if let Some(pairing) = &self.pairing
            && (!valid_id(&pairing.id)
                || !valid_hash(&pairing.secret_hash)
                || pairing.expires_at == 0
                || !self.devices.iter().any(|device| {
                    device.id == pairing.created_by && device.kind == "desktop" && !device.revoked
                }))
        {
            return Err(invalid());
        }
        if let Some(lease) = &self.lease
            && (!valid_id(&lease.id)
                || !valid_activity(&lease.activity)
                || lease.expires_at == 0
                || !self
                    .devices
                    .iter()
                    .any(|device| device.id == lease.device_id && !device.revoked))
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn authenticate(&self, token: &str) -> Option<String> {
        if token.is_empty() || token.len() > 256 {
            return None;
        }
        let candidate = digest(token);
        self.devices
            .iter()
            .find(|device| !device.revoked && same_hash(&device.credential_hash, &candidate))
            .map(|device| device.id.clone())
    }

    pub fn device_kind(&self, device_id: &str) -> Option<&str> {
        self.devices
            .iter()
            .find(|device| device.id == device_id && !device.revoked)
            .map(|device| device.kind.as_str())
    }

    fn require_device(&self, device_id: &str) -> Result<(), IdentityError> {
        self.device_kind(device_id)
            .map(|_| ())
            .ok_or(IdentityError("device_revoked"))
    }

    fn require_desktop(&self, device_id: &str) -> Result<(), IdentityError> {
        self.require_device(device_id)?;
        if self.device_kind(device_id) != Some("desktop") {
            return Err(IdentityError("desktop_approval_required"));
        }
        Ok(())
    }

    pub fn public(&self, device_id: &str) -> Value {
        json!({
            "schemaVersion":self.schema_version,"accountId":self.account_id,
            "chaoId":self.chao_id,"revision":self.revision,"deviceId":device_id,
            "devices":self.devices.iter().map(|device|json!({"id":device.id,"kind":device.kind,"revoked":device.revoked})).collect::<Vec<_>>(),
            "lease":self.lease,
        })
    }

    pub fn bump(&mut self) -> Result<u64, IdentityError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(IdentityError("revision_exhausted"))?;
        Ok(self.revision)
    }

    /// Creating this invitation is the authenticated desktop's explicit
    /// approval. The short-lived secret is returned once, never stored in clear.
    pub fn create_pairing(&mut self, device_id: &str, now_ms: u64) -> Result<Value, IdentityError> {
        self.require_desktop(device_id)?;
        if self.devices.len() >= MAX_DEVICES {
            return Err(IdentityError("device_limit"));
        }
        let expires_at = now_ms
            .checked_add(PAIRING_TTL_MS)
            .ok_or(IdentityError("invalid_server_time"))?;
        let secret = random_secret();
        let id = Uuid::new_v4().to_string();
        self.bump()?;
        self.pairing = Some(Pairing {
            id: id.clone(),
            secret_hash: digest(&secret),
            created_by: device_id.into(),
            expires_at,
        });
        Ok(
            json!({"pairingId":id,"pairingSecret":secret,"expiresAt":expires_at,"revision":self.revision}),
        )
    }

    pub fn claim_pairing(
        &mut self,
        pairing_id: &str,
        secret: &str,
        now_ms: u64,
    ) -> Result<Value, IdentityError> {
        // One static error for unknown, expired and already-consumed invites.
        let invalid = || IdentityError("invalid_pairing");
        let pairing = self.pairing.as_ref().ok_or_else(invalid)?;
        if pairing.id != pairing_id
            || secret.is_empty()
            || secret.len() > 256
            || now_ms >= pairing.expires_at
            || !same_hash(&pairing.secret_hash, &digest(secret))
            || self.require_desktop(&pairing.created_by).is_err()
        {
            return Err(invalid());
        }
        if self.devices.len() >= MAX_DEVICES {
            return Err(IdentityError("device_limit"));
        }
        let token = random_secret();
        let id = Uuid::new_v4().to_string();
        self.bump()?;
        self.devices.push(Device {
            id: id.clone(),
            kind: "android".into(),
            credential_hash: digest(&token),
            revoked: false,
        });
        self.pairing = None;
        Ok(json!({"token":token,"deviceId":id,"identity":self.public(&id)}))
    }

    pub fn revoke(&mut self, actor: &str, target: &str) -> Result<(), IdentityError> {
        self.require_desktop(actor)?;
        let position = self
            .devices
            .iter()
            .position(|device| device.id == target)
            .ok_or(IdentityError("unknown_device"))?;
        if self.devices[position].revoked {
            return Ok(());
        }
        self.bump()?;
        self.devices[position].revoked = true;
        if self
            .lease
            .as_ref()
            .is_some_and(|lease| lease.device_id == target)
        {
            self.lease = None;
        }
        if self
            .pairing
            .as_ref()
            .is_some_and(|pairing| pairing.created_by == target)
        {
            self.pairing = None;
        }
        Ok(())
    }

    pub fn acquire(
        &mut self,
        device_id: &str,
        activity: &str,
        expected_revision: u64,
        now_ms: u64,
    ) -> Result<ActivityLease, IdentityError> {
        self.require_device(device_id)?;
        if expected_revision != self.revision {
            return Err(IdentityError("stale_revision"));
        }
        if !valid_activity(activity) {
            return Err(IdentityError("invalid_activity"));
        }
        if self
            .lease
            .as_ref()
            .is_some_and(|lease| now_ms < lease.expires_at)
        {
            return Err(IdentityError("activity_conflict"));
        }
        let expires_at = now_ms
            .checked_add(LEASE_TTL_MS)
            .ok_or(IdentityError("invalid_server_time"))?;
        self.bump()?;
        let lease = ActivityLease {
            id: Uuid::new_v4().to_string(),
            device_id: device_id.into(),
            activity: activity.into(),
            expires_at,
            last_sequence: 0,
        };
        self.lease = Some(lease.clone());
        Ok(lease)
    }

    pub fn validate(
        &self,
        device_id: &str,
        lease_id: &str,
        expected_revision: u64,
        now_ms: u64,
    ) -> Result<(), IdentityError> {
        self.require_device(device_id)?;
        if expected_revision != self.revision {
            return Err(IdentityError("stale_revision"));
        }
        let lease = self.lease.as_ref().ok_or(IdentityError("lease_required"))?;
        if lease.id != lease_id || lease.device_id != device_id {
            return Err(IdentityError("stale_lease"));
        }
        if now_ms >= lease.expires_at {
            return Err(IdentityError("lease_expired"));
        }
        Ok(())
    }

    pub fn renew(
        &mut self,
        device_id: &str,
        lease_id: &str,
        expected_revision: u64,
        now_ms: u64,
    ) -> Result<ActivityLease, IdentityError> {
        self.validate(device_id, lease_id, expected_revision, now_ms)?;
        let expires_at = now_ms
            .checked_add(LEASE_TTL_MS)
            .ok_or(IdentityError("invalid_server_time"))?;
        self.bump()?;
        let lease = self.lease.as_mut().expect("validated lease");
        lease.expires_at = expires_at;
        Ok(lease.clone())
    }

    /// Fence an input before applying its gameplay mutation. Sequence and
    /// gameplay must be committed together by the caller's profile transaction.
    /// Renewing/reconnecting retains this counter; a new lease has a new ID.
    pub fn accept_intent(
        &mut self,
        device_id: &str,
        lease_id: &str,
        expected_revision: u64,
        sequence: u64,
        now_ms: u64,
    ) -> Result<(), IdentityError> {
        self.validate(device_id, lease_id, expected_revision, now_ms)?;
        let lease = self.lease.as_mut().expect("validated lease");
        if sequence <= lease.last_sequence {
            return Err(IdentityError("replayed_intent"));
        }
        lease.last_sequence = sequence;
        Ok(())
    }

    pub fn release(
        &mut self,
        device_id: &str,
        lease_id: &str,
        expected_revision: u64,
        now_ms: u64,
    ) -> Result<(), IdentityError> {
        self.validate(device_id, lease_id, expected_revision, now_ms)?;
        self.bump()?;
        self.lease = None;
        Ok(())
    }
}
