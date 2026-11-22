//! Player accounts: a username (or email) and password open a memory card.
//!
//! The server never keeps a card code in the clear: it is sealed with a key
//! derived from the password (Argon2id), so only a correct login can open it.
//! Passwords are stored as Argon2id PHC strings. Accounts live in
//! `accounts.json` next to `profiles.json`, written atomically with mode 0600.

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{
        SaltString,
        rand_core::{OsRng, RngCore},
    },
};
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce, aead::Aead};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::PathBuf,
    sync::OnceLock,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub username: String,
    pub email: String,
    /// Argon2id PHC string.
    password: String,
    /// Hash of the card code (as `Player::card_hash`), so a card links once.
    pub card_hash: String,
    key_salt: Vec<u8>,
    nonce: Vec<u8>,
    sealed_card: Vec<u8>,
}

pub struct Accounts {
    path: PathBuf,
    /// Keyed by lowercase username.
    list: BTreeMap<String, Account>,
    /// Failed logins per lowercase login name: count and time of the last one.
    failures: HashMap<String, (u32, Instant)>,
}

pub const PASSWORD_MIN: usize = 8;
const PASSWORD_MAX: usize = 128;
/// Failures before each further try for that name must wait `LOCKOUT`.
const FREE_TRIES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(60);

pub fn valid_username(name: &str) -> bool {
    (3..=20).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    email.len() <= 254
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.contains(char::is_whitespace)
        && !domain.contains('@')
}
pub fn valid_password(password: &str) -> bool {
    (PASSWORD_MIN..=PASSWORD_MAX).contains(&password.chars().count())
}

fn card_key(password: &str, salt: &[u8]) -> Result<Key, String> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| e.to_string())?;
    Ok(key.into())
}

impl Account {
    /// Hashes the password and seals the card code. Slow on purpose (Argon2id).
    pub fn new(username: &str, email: &str, password: &str, card: &str) -> Result<Self, String> {
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map_err(|e| e.to_string())?
            .to_string();
        let mut key_salt = vec![0u8; 16];
        let mut nonce = vec![0u8; 12];
        OsRng.fill_bytes(&mut key_salt);
        OsRng.fill_bytes(&mut nonce);
        let sealed_card = ChaCha20Poly1305::new(&card_key(password, &key_salt)?)
            .encrypt(Nonce::from_slice(&nonce), card.as_bytes())
            .map_err(|e| e.to_string())?;
        Ok(Self {
            username: username.to_owned(),
            email: email.to_owned(),
            password: password_hash,
            card_hash: crate::game::hash_token(card),
            key_salt,
            nonce,
            sealed_card,
        })
    }
    /// The card code, when `password` is right. Slow on purpose.
    pub fn open(&self, password: &str) -> Option<String> {
        let hash = PasswordHash::new(&self.password).ok()?;
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .ok()?;
        let card = ChaCha20Poly1305::new(&card_key(password, &self.key_salt).ok()?)
            .decrypt(Nonce::from_slice(&self.nonce), self.sealed_card.as_slice())
            .ok()?;
        String::from_utf8(card).ok()
    }
}

/// Spends the same Argon2 time as a real check, so an unknown name can't be
/// told apart from a wrong password by timing.
pub fn waste_a_check(password: &str) {
    static DUMMY: OnceLock<String> = OnceLock::new();
    let dummy = DUMMY.get_or_init(|| {
        Argon2::default()
            .hash_password(b"no account", &SaltString::generate(&mut OsRng))
            .map(|h| h.to_string())
            .unwrap_or_default()
    });
    if let Ok(hash) = PasswordHash::new(dummy) {
        let _ = Argon2::default().verify_password(password.as_bytes(), &hash);
    }
    let _ = card_key(password, &[0u8; 16]);
}

impl Accounts {
    pub fn load(data_dir: PathBuf) -> io::Result<Self> {
        let path = data_dir.join("accounts.json");
        let list: Vec<Account> = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            vec![]
        };
        Ok(Self {
            path,
            list: list
                .into_iter()
                .map(|a| (a.username.to_lowercase(), a))
                .collect(),
            failures: HashMap::new(),
        })
    }
    /// The account a login name (username or email, any case) names.
    pub fn find(&self, login: &str) -> Option<&Account> {
        let login = login.to_lowercase();
        self.list
            .get(&login)
            .or_else(|| self.list.values().find(|a| a.email.to_lowercase() == login))
    }
    /// Why a new account can't be made, if it can't.
    pub fn taken(&self, username: &str, email: &str, card_hash: &str) -> Option<&'static str> {
        if self.list.contains_key(&username.to_lowercase()) {
            return Some("That username is taken.");
        }
        let email = email.to_lowercase();
        if self.list.values().any(|a| a.email.to_lowercase() == email) {
            return Some("That email already has an account.");
        }
        if self.list.values().any(|a| a.card_hash == card_hash) {
            return Some("This memory card already has an account.");
        }
        None
    }
    pub fn insert(&mut self, account: Account) -> io::Result<()> {
        let key = account.username.to_lowercase();
        self.list.insert(key.clone(), account);
        self.save().inspect_err(|_| {
            self.list.remove(&key);
        })
    }
    /// True while `login` must wait after too many wrong passwords.
    pub fn locked(&self, login: &str, now: Instant) -> bool {
        self.failures
            .get(&login.to_lowercase())
            .is_some_and(|(n, at)| *n >= FREE_TRIES && now.duration_since(*at) < LOCKOUT)
    }
    pub fn failed(&mut self, login: &str, now: Instant) {
        let entry = self
            .failures
            .entry(login.to_lowercase())
            .or_insert((0, now));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = now;
    }
    pub fn succeeded(&mut self, login: &str) {
        self.failures.remove(&login.to_lowercase());
    }
    fn save(&self) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&self.list.values().collect::<Vec<_>>())?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        {
            use std::io::Write;
            let mut file = options.open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        fs::rename(temporary, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_card_opens_only_with_the_password() {
        let a = Account::new("Rider", "r@example.com", "correct horse", "card-code").unwrap();
        assert_eq!(a.open("correct horse").as_deref(), Some("card-code"));
        assert_eq!(a.open("wrong horse"), None);
        let text = serde_json::to_string(&a).unwrap();
        assert!(!text.contains("card-code") && !text.contains("correct horse"));
    }

    #[test]
    fn accounts_survive_reload_and_names_stay_unique() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = Accounts::load(dir.path().into()).unwrap();
        let a = Account::new("Rider", "R@Example.com", "correct horse", "card").unwrap();
        let hash = a.card_hash.clone();
        list.insert(a).unwrap();
        let list = Accounts::load(dir.path().into()).unwrap();
        assert!(list.find("rider").is_some() && list.find("r@example.COM").is_some());
        assert!(list.taken("RIDER", "x@y.z", "other").is_some());
        assert!(list.taken("new", "r@example.com", "other").is_some());
        assert!(list.taken("new", "x@y.z", &hash).is_some());
        assert!(list.taken("new", "x@y.z", "other").is_none());
    }

    #[test]
    fn too_many_wrong_passwords_wait() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = Accounts::load(dir.path().into()).unwrap();
        let now = Instant::now();
        for _ in 0..FREE_TRIES {
            assert!(!list.locked("Rider", now));
            list.failed("rider", now);
        }
        assert!(list.locked("RIDER", now));
        assert!(!list.locked("rider", now + LOCKOUT));
    }

    #[test]
    fn input_checks() {
        assert!(valid_username("chao_99") && !valid_username("a b") && !valid_username("ab"));
        assert!(valid_email("a@b.co") && !valid_email("a@b") && !valid_email("@b.co"));
        assert!(valid_password("12345678") && !valid_password("1234567"));
    }
}
