//! Story progress per memory card: which action stages the card has cleared
//! and which the game's Stage Select may show. The stage after a cleared one
//! opens in the game's own story order with the boss and versus fights left
//! out; one finished side opens Cannon's Core, and clearing that opens the
//! Super Chao stage. The server trusts the game's report that a stage was
//! cleared: it decides the order and keeps the record.
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

/// A stage as the Stage Select lists it: (stage number, character). The
/// character tells Route 101 (6) from Route 280 (5), both stage 70. Pairs are
/// the rows of the Stage Select table (Steam sonic2app.exe VA 0xC75078, 33
/// rows of stage, character, column, row; advertiseD.rel .data 0x6E64).
pub type Stage = (u8, u8);

/// Hero story, type-1 (level) entries of the story sequence at Steam
/// sonic2app.exe file offset 0x1337B48, without stages 60, 20, 19, 63, 64,
/// 33, 29 and 42 (boss and versus fights).
pub const HERO: [Stage; 16] = [
    (13, 0),
    (16, 4),
    (9, 6),
    (10, 0),
    (3, 0),
    (5, 4),
    (17, 6),
    (7, 4),
    (70, 6),
    (23, 6),
    (28, 0),
    (25, 4),
    (24, 6),
    (32, 4),
    (22, 0),
    (30, 0),
];
/// Dark story, the sequence at file offset 0x1337D70, without stages 61, 20,
/// 62, 19, 67, 33, 29 and 42.
pub const DARK: [Stage; 14] = [
    (11, 7),
    (18, 5),
    (21, 7),
    (14, 1),
    (26, 5),
    (27, 7),
    (12, 7),
    (8, 5),
    (4, 1),
    (70, 5),
    (6, 1),
    (44, 5),
    (43, 7),
    (40, 1),
];
/// Cannon's Core as the Stage Select lists it. The Last story (file offset
/// 0x1337F80) is stage 36, then 65 (a boss), then 66.
pub const CANNONS_CORE: Stage = (34, 0);

/// The stage a clear is recorded under. The game keeps one record for
/// Cannon's Core's five parts, stages 34 to 38 (main.dol 0x8005A67C).
fn listed(stage: Stage) -> Stage {
    if (34..=38).contains(&stage.0) {
        CANNONS_CORE
    } else {
        stage
    }
}

fn side_done(cleared: &BTreeSet<Stage>, side: &[Stage]) -> bool {
    side.iter().all(|s| cleared.contains(s))
}

/// Every stage the card may play: each side up to and including its first
/// uncleared stage, and Cannon's Core once a side is finished.
pub fn unlocked(cleared: &BTreeSet<Stage>) -> Vec<Stage> {
    let mut open = Vec::new();
    for side in [&HERO[..], &DARK[..]] {
        let done = side.iter().take_while(|s| cleared.contains(s)).count();
        open.extend(&side[..(done + 1).min(side.len())]);
    }
    if side_done(cleared, &HERO) || side_done(cleared, &DARK) {
        open.push(CANNONS_CORE);
    }
    open
}

/// True once Cannon's Core is cleared: the card may play the Super Chao stage.
pub fn super_chao(cleared: &BTreeSet<Stage>) -> bool {
    cleared.contains(&CANNONS_CORE)
}

/// Record a clear. False when the stage is not open for this card.
pub fn clear(cleared: &mut BTreeSet<Stage>, stage: Stage) -> bool {
    let stage = listed(stage);
    let open = unlocked(cleared).contains(&stage);
    if open {
        cleared.insert(stage);
    }
    open
}

fn path(dir: &Path, card: &str) -> PathBuf {
    // Hashed like the game save, so any stored card id is a safe file name.
    dir.join("game-saves")
        .join(format!("{}.story.json", crate::game::hash_token(card)))
}

/// The card's cleared stages; none before the first clear.
pub fn read(dir: &Path, card: &str) -> io::Result<BTreeSet<Stage>> {
    match fs::read(path(dir, card)) {
        Ok(file) => serde_json::from_slice(&file).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(error) => Err(error),
    }
}

/// Store the card's cleared stages atomically.
pub fn write(dir: &Path, card: &str, cleared: &BTreeSet<Stage>) -> io::Result<()> {
    let target = path(dir, card);
    fs::create_dir_all(target.parent().unwrap())?;
    let temporary = target.with_extension("tmp");
    {
        use std::io::Write;
        let mut file = fs::File::create(&temporary)?;
        file.write_all(&serde_json::to_vec(cleared).map_err(io::Error::other)?)?;
        file.sync_all()?;
    }
    fs::rename(temporary, target)
}

/// Moves a card's progress to another card key, with its game save. None: Ok.
pub fn rename(dir: &Path, from: &str, to: &str) -> io::Result<()> {
    match fs::rename(path(dir, from), path(dir, to)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_card_sees_only_the_first_stage_of_each_side() {
        assert_eq!(unlocked(&BTreeSet::new()), [HERO[0], DARK[0]]);
    }

    #[test]
    fn each_clear_opens_the_next_stage_and_skipping_ahead_is_refused() {
        let mut cleared = BTreeSet::new();
        assert!(!clear(&mut cleared, HERO[1]));
        assert!(!clear(&mut cleared, CANNONS_CORE));
        assert!(clear(&mut cleared, HERO[0]));
        assert!(clear(&mut cleared, HERO[0]), "a replay is fine");
        assert_eq!(unlocked(&cleared), [HERO[0], HERO[1], DARK[0]]);
    }

    #[test]
    fn the_two_routes_are_different_stages() {
        let mut cleared: BTreeSet<Stage> = HERO[..8].iter().copied().collect();
        assert!(
            !clear(&mut cleared, (70, 5)),
            "Route 280 is on the Dark side"
        );
        assert!(clear(&mut cleared, (70, 6)));
    }

    #[test]
    fn one_finished_side_opens_cannons_core_and_that_opens_super_chao() {
        for side in [&HERO[..], &DARK[..]] {
            let mut cleared = BTreeSet::new();
            for (i, stage) in side.iter().enumerate() {
                assert!(!unlocked(&cleared).contains(&CANNONS_CORE), "step {i}");
                assert!(clear(&mut cleared, *stage));
            }
            assert!(!super_chao(&cleared));
            // The game reports whichever of Cannon's Core's parts it ends on.
            assert!(clear(&mut cleared, (36, 6)));
            assert!(super_chao(&cleared));
        }
    }

    #[test]
    fn progress_survives_a_restart_and_moves_with_the_card() {
        let dir = tempfile::tempdir().unwrap();
        let cleared = BTreeSet::from([HERO[0]]);
        write(dir.path(), "card", &cleared).unwrap();
        assert_eq!(read(dir.path(), "card").unwrap(), cleared);
        rename(dir.path(), "card", "new").unwrap();
        assert!(read(dir.path(), "card").unwrap().is_empty());
        assert_eq!(read(dir.path(), "new").unwrap(), cleared);
        rename(dir.path(), "none", "other").unwrap();
    }
}
