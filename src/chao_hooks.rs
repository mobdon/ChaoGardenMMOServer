//! Single integration seam between care/garden effects and the Chao state
//! owned by other modules.
//!
//! Agreed ownership (server coordination, November 20, 2026):
//! - `life` owns stat experience/level/grade curves and needs. When it lands,
//!   `grant_stat` calls `life::grant_stat_progress` and `feed` calls
//!   `life::feed`.
//! - `body` owns the run/power, swim/fly and hero/dark axes. When it lands,
//!   `capture_body` calls `body::apply_capture` and `fruit_body` calls `body::apply_fruit`.
//!
//! Until then these functions keep today's published adapted behaviour so the
//! swap is a change to this file only.
//!
//! Signatures shared by those threads (November 20, 2026):
//! - life (PR #27): `life::eat_fruit_bite(&mut Player, kind: u8, row: [i16; 7],
//!   bite: u8 /*1..=4*/)` applies the fruit row per bite (trunc(v/4), remainder
//!   on bite 4) with hunger, life and LifeMax, so the swap moves the fruit skill
//!   grant out of `care::eat_fruit` into these bites. `life::capture_animal(&mut
//!   Player, row)` applies an animal row once; `life::fruit_handed_over` and
//!   `life::pet` are the native like +1 events. Heart Fruit (kind 10) gives
//!   breed +10000 via `life::adjust_need(Need::Breed, 10000)` on the last bite.
//! - body: `body::apply_fruit(&mut player.body, kind: u8, last_bite: bool)` per
//!   bite, with the Hero/Dark fruit alignment on the last bite.
use crate::game::Player;

/// Adds signed skill points with the published adapted clamp (0-9999).
/// Returns the applied delta. Owner perfect stats never change.
pub fn grant_stat(p: &mut Player, stat: &str, delta: i16) -> i64 {
    let stat = p.stats.get_mut(stat).expect("seven canonical stats");
    let before = stat.points;
    if !p.admin {
        stat.points = (i64::from(before) + i64::from(delta)).clamp(0, 9999) as u32;
    }
    i64::from(stat.points) - i64::from(before)
}

/// Fruit bites: native belly +3 per bite (CG+0xB8) and the FavoriteFruit
/// taste check belong to `life::feed`. Until the life module merges this
/// changes nothing; no substitute need value is applied.
pub fn fruit_bites(_p: &mut Player, _fruit: u8, _bites: u8) -> &'static str {
    "pending-life-module"
}

/// Child or adult, for choosing the native child or adult part list. Every
/// Chao is a child until the life module's lifecycle merges.
pub fn is_adult(_p: &Player) -> bool {
    false
}

/// Body change when a Chao is given an animal or drive. The native rule is
/// ALBHV_Capture (GameCube ChaoMain 0x33A2C), ported by the body module as
/// `body::apply_capture(&mut player.body, kind)`. Until that module merges this
/// changes nothing and says so; no substitute value is applied.
pub fn capture_body(_p: &mut Player, _kind: u8) -> &'static str {
    "pending-body-module"
}

/// Body growth from eating fruit: `body::apply_fruit(&mut player.body, kind)` once the
/// body module merges. No native fruit alignment change has been traced, so none
/// is applied.
pub fn fruit_body(_p: &mut Player, _kind: u8) -> &'static str {
    "pending-body-module"
}
