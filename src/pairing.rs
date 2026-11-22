//! One-time phone pairing for Tiny Chao Garden.
//!
//! The PC (holding a Chao's session token) asks for a short single-use code. A phone
//! signed in to the same memory card enters it once and gets a device key. Tiny
//! Garden sockets for that Chao then need the device key. Pairings live in
//! `pairings.json` (mode 0600) next to `profiles.json`; only key hashes are stored.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs, io,
    path::PathBuf,
    time::{Duration, Instant},
};

/// How long a pairing code works.
pub const CODE_LIFE: Duration = Duration::from_secs(10 * 60);
/// No 0/O, 1/I/L: easy to read off a screen and type on a phone.
const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const CODE_LEN: usize = 6;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pairing {
    chao_id: String,
    device_hash: String,
}

struct OpenCode {
    chao_id: String,
    card_hash: String,
    made: Instant,
}

pub struct Pairings {
    path: PathBuf,
    /// One paired phone per Chao: pairing again replaces it.
    list: Vec<Pairing>,
    codes: HashMap<String, OpenCode>,
}

fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buffer);
    buffer.iter().map(|b| format!("{b:02x}")).collect()
}

impl Pairings {
    pub fn load(data_dir: PathBuf) -> io::Result<Self> {
        let path = data_dir.join("pairings.json");
        let list = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            vec![]
        };
        Ok(Self {
            path,
            list,
            codes: HashMap::new(),
        })
    }
    /// A fresh code for the Chao `chao_id` on card `card_hash`; it replaces any open
    /// code for that Chao.
    pub fn start(&mut self, chao_id: &str, card_hash: &str, now: Instant) -> String {
        self.codes
            .retain(|_, open| open.chao_id != chao_id && now.duration_since(open.made) < CODE_LIFE);
        let code = loop {
            let mut raw = [0u8; CODE_LEN];
            OsRng.fill_bytes(&mut raw);
            let code: String = raw
                .iter()
                .map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char)
                .collect();
            if !self.codes.contains_key(&code) {
                break code;
            }
        };
        self.codes.insert(
            code.clone(),
            OpenCode {
                chao_id: chao_id.into(),
                card_hash: card_hash.into(),
                made: now,
            },
        );
        code
    }
    /// Uses up `code` for a phone holding card `card_hash`: (Chao id, device key).
    /// A wrong card leaves the code open for the right phone.
    pub fn claim(
        &mut self,
        code: &str,
        card_hash: &str,
        now: Instant,
    ) -> Result<Option<(String, String)>, io::Error> {
        let code = code.trim().to_ascii_uppercase();
        let Some(open) = self.codes.get(&code) else {
            return Ok(None);
        };
        if now.duration_since(open.made) >= CODE_LIFE {
            self.codes.remove(&code);
            return Ok(None);
        }
        if open.card_hash != card_hash {
            return Ok(None);
        }
        let open = self.codes.remove(&code).expect("checked");
        let key = random_hex(32);
        let before = self.list.clone();
        self.list.retain(|p| p.chao_id != open.chao_id);
        self.list.push(Pairing {
            chao_id: open.chao_id.clone(),
            device_hash: crate::game::hash_token(&key),
        });
        if let Err(cause) = self.save() {
            self.list = before;
            return Err(cause);
        }
        Ok(Some((open.chao_id, key)))
    }
    /// True when `device_key` is the paired phone of `chao_id`.
    pub fn paired(&self, chao_id: &str, device_key: &str) -> bool {
        device_key.len() <= 128 && {
            let hash = crate::game::hash_token(device_key);
            self.list
                .iter()
                .any(|p| p.chao_id == chao_id && p.device_hash == hash)
        }
    }
    fn save(&self) -> io::Result<()> {
        crate::accounts::write_private_json(&self.path, &serde_json::to_vec_pretty(&self.list)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_codes_are_single_use_card_bound_and_expire() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = Pairings::load(dir.path().into()).unwrap();
        let now = Instant::now();
        let code = list.start("chao-1", "card-a", now);
        assert_eq!(code.len(), CODE_LEN);
        assert!(code.bytes().all(|b| ALPHABET.contains(&b)));
        // Another card can't take it, and that doesn't burn it.
        assert!(list.claim(&code, "card-b", now).unwrap().is_none());
        let (chao, key) = list
            .claim(&code.to_lowercase(), "card-a", now)
            .unwrap()
            .unwrap();
        assert_eq!(chao, "chao-1");
        assert!(list.claim(&code, "card-a", now).unwrap().is_none());
        assert!(list.paired("chao-1", &key));
        assert!(!list.paired("chao-2", &key) && !list.paired("chao-1", "guess"));
        // Survives a restart, and the key itself is not on disk.
        let list2 = Pairings::load(dir.path().into()).unwrap();
        assert!(list2.paired("chao-1", &key));
        let disk = fs::read_to_string(dir.path().join("pairings.json")).unwrap();
        assert!(!disk.contains(&key));
        // A stale code is dead; pairing again replaces the old phone.
        let late = list.start("chao-1", "card-a", now);
        assert!(
            list.claim(&late, "card-a", now + CODE_LIFE)
                .unwrap()
                .is_none()
        );
        let again = list.start("chao-1", "card-a", now);
        let (_, key2) = list.claim(&again, "card-a", now).unwrap().unwrap();
        assert!(list.paired("chao-1", &key2) && !list.paired("chao-1", &key));
    }
}
