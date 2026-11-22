//! The player's native SA2 Chao garden save (the canonical Chao, decided
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

/// The whole memory card the game sends (`SA2CARD1` pack, see
/// `sa2-boot/src/mmo.rs`): 59 blocks of 8 KB plus room for names and status.
pub const MAX_BYTES: usize = 65 * 8192;
/// A game that stops renewing (crash, lost network) frees the Chao after this.
pub const LEASE: Duration = Duration::from_secs(600);

pub struct Lease {
    pub id: String,
    pub until: Instant,
}

/// True while a game holds this memory card's save (keyed by card hash).
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

/// Moves a card's save to another card key (a password reset moves the card). No save: Ok.
pub fn rename(dir: &Path, from: &str, to: &str) -> io::Result<()> {
    match fs::rename(path(dir, from), path(dir, to)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        other => other?,
    }
    crate::story::rename(dir, from, to)
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

/// The card label without the bytes: (revision, when it was written), or None before the first save.
pub fn status(dir: &Path, player: &str) -> io::Result<Option<(u64, std::time::SystemTime)>> {
    use std::io::Read;
    let mut file = match fs::File::open(path(dir, player)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut revision = [0; 8];
    file.read_exact(&mut revision)?;
    Ok(Some((
        u64::from_le_bytes(revision),
        file.metadata()?.modified()?,
    )))
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

/// One skill of a Chao as its native save record holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeSkill {
    pub points: u16,
    pub level: u8,
    pub grade: u8,
    pub progress: u8,
}
/// A Chao's type byte and its seven skills in the game's order: Swim, Fly,
/// Run, Power, Stamina, Luck, Intelligence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeStats {
    #[serde(rename = "type")]
    pub kind: u8,
    pub skills: [NativeSkill; 7],
}

impl NativeStats {
    /// Puts the record's numbers in the profile: level, grade, points and progress
    /// of the seven skills, in the game's order, replace the profile's own. Glue:
    /// level and points are held to the profile's limits (99 and 9999).
    pub fn apply(
        &self,
        stats: &mut std::collections::BTreeMap<String, crate::game::Stat>,
        exp: &mut [i16; 7],
    ) {
        *exp = self.skills.map(|skill| skill.progress.into());
        for (name, skill) in crate::game::ALL_STATS.into_iter().zip(self.skills) {
            stats.insert(
                name.into(),
                crate::game::Stat {
                    grade: skill.grade.into(),
                    level: u16::from(skill.level).min(99),
                    points: u32::from(skill.points).min(9999),
                },
            );
        }
    }
}

/// Size of one native Chao save record.
pub const CHAO_RECORD: usize = 0x800;

/// The stats in one native Chao save record; None for a wrong size or an
/// empty record (type byte 0).
/// Offsets (GSNE8P ChaoMain AL_ParameterGetSkill / AL_ParameterGrow /
/// AL_AblLevelUp / CreateChaoExtra): progress +0x20+i, grade +0x28+i, level
/// +0x30+i, points u16 big-endian +0x38+2i, type +0x80.
pub fn native_stats(r: &[u8]) -> Option<NativeStats> {
    (r.len() == CHAO_RECORD && r[0x80] != 0).then(|| NativeStats {
        kind: r[0x80],
        skills: std::array::from_fn(|i| NativeSkill {
            points: u16::from_be_bytes([r[0x38 + 2 * i], r[0x39 + 2 * i]]),
            level: r[0x30 + i],
            grade: r[0x28 + i],
            progress: r[0x20 + i],
        }),
    })
}

#[cfg(test)]
mod stats_tests {
    use super::*;

    /// Synthetic record bytes (test input, never game data).
    #[test]
    fn reads_a_record_and_refuses_empty_or_wrong_sized_ones() {
        let mut r = vec![0u8; CHAO_RECORD];
        assert_eq!(native_stats(&r), None, "type 0 is an empty record");
        r[0x80] = 0x16;
        for i in 0..7 {
            r[0x20 + i] = 10 + i as u8;
            r[0x28 + i] = i as u8 % 6;
            r[0x30 + i] = 90 + i as u8;
            r[0x38 + 2 * i..0x3A + 2 * i]
                .copy_from_slice(&(0x0102 + i as u16 * 0x100).to_be_bytes());
        }
        let chao = native_stats(&r).unwrap();
        assert_eq!(chao.kind, 0x16);
        assert_eq!(
            chao.skills[2],
            NativeSkill {
                points: 0x0302,
                level: 92,
                grade: 2,
                progress: 12
            }
        );
        assert_eq!(chao.skills[6].points, 0x0702);
        let mut stats = std::collections::BTreeMap::new();
        let mut exp = [0; 7];
        chao.apply(&mut stats, &mut exp);
        assert_eq!(exp, [10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(stats.len(), 7);
        let run = &stats["run"];
        assert_eq!((run.grade, run.level, run.points), (2, 92, 0x0302));
        assert_eq!(stats["intelligence"].level, 96);
        assert_eq!(native_stats(&r[..CHAO_RECORD - 1]), None);
        r.push(0);
        assert_eq!(native_stats(&r), None);
    }
}
