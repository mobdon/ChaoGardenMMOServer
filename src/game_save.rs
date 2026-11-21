//! The player's native SA2 Chao garden save (the canonical Chao, mob's decision
//! 2026-11-21). The recompiled game borrows it through its server-backed memory
//! card: a GET checks it out under a lease, PUTs hand back new bytes, and while
//! the lease is held the server's other Chao mutations are refused so the two
//! never diverge. The bytes are stored exactly as the game wrote them.
use crate::game::Game;
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// A whole Memory Card 59 (59 blocks of 8 KB, as `sa2-boot/src/card.rs`
/// models it): no card file the game writes can be larger.
pub const MAX_BYTES: usize = 59 * 8192;
/// A game that stops renewing (crash, lost network) frees the Chao after this.
pub const LEASE: Duration = Duration::from_secs(600);

pub struct Lease {
    pub id: String,
    pub until: Instant,
}

/// True while a game holds this player's save.
pub fn leased(game: &Game, player: &str, now: Instant) -> bool {
    game.game_save_leases
        .get(player)
        .is_some_and(|lease| lease.until > now)
}

/// Take or renew the lease. `held` is the lease id the caller already has.
/// Returns None when another unexpired lease holds the save.
pub fn check_out(
    game: &mut Game,
    player: &str,
    held: Option<&str>,
    now: Instant,
) -> Option<String> {
    let id = match game.game_save_leases.get(player) {
        Some(lease) if lease.until > now && held != Some(lease.id.as_str()) => return None,
        Some(lease) if lease.until > now => lease.id.clone(),
        _ => uuid::Uuid::new_v4().to_string(),
    };
    game.game_save_leases.insert(
        player.into(),
        Lease {
            id: id.clone(),
            until: now + LEASE,
        },
    );
    Some(id)
}

/// True when `held` is the player's current, unexpired lease.
pub fn holds(game: &Game, player: &str, held: Option<&str>, now: Instant) -> bool {
    game.game_save_leases
        .get(player)
        .is_some_and(|lease| lease.until > now && held == Some(lease.id.as_str()))
}

fn path(dir: &Path, player: &str) -> PathBuf {
    // Hashed so any stored player id is a safe file name.
    dir.join("game-saves")
        .join(format!("{}.bin", crate::game::hash_token(player)))
}

/// The stored save as (revision, bytes); revision 0 and no bytes when none.
pub fn read(dir: &Path, player: &str) -> io::Result<(u64, Vec<u8>)> {
    match fs::read(path(dir, player)) {
        Ok(file) if file.len() >= 8 => {
            let revision = u64::from_le_bytes(file[..8].try_into().unwrap());
            Ok((revision, file[8..].to_vec()))
        }
        Ok(_) => Err(io::Error::other("game save file is truncated")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok((0, Vec::new())),
        Err(error) => Err(error),
    }
}

/// Store `bytes` as `revision` atomically; the previous save stays as `.prev`.
pub fn write(dir: &Path, player: &str, revision: u64, bytes: &[u8]) -> io::Result<()> {
    let target = path(dir, player);
    fs::create_dir_all(target.parent().unwrap())?;
    let temporary = target.with_extension("tmp");
    {
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&revision.to_le_bytes())?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if target.exists() {
        fs::copy(&target, target.with_extension("prev"))?;
    }
    fs::rename(temporary, target)
}
