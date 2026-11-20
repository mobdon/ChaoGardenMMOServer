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
//! Life signatures shared by that thread (branch claude/project-thread-3i7u22):
//! `grant_stat_progress(&mut Player, stat, amount: i32) -> StatProgress` in native
//! Exp units, and `feed(&mut Player, Food { fruit: Option<u8>, nourish: i16, joy: i8 })`
//! with nourish in State units 0..=10000 and joy in Mood units. The raw item-table
//! Belly/Mood units are not proven to be those units; the swap must state its
//! conversion as tuning.
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

/// Native Mood/Belly units map one-for-one onto the adapted 0-100 happiness and
/// satiety needs (published `adapted-care-v1` mapping).
pub fn feed(p: &mut Player, _fruit: Option<u8>, mood: i16, belly: i16) {
    p.happiness = (p.happiness + f64::from(mood)).clamp(0., 100.);
    p.hunger = (p.hunger + f64::from(belly)).clamp(0., 100.);
}

/// Body change when a Chao is given an animal or drive. The native rule is
/// ALBHV_Capture (GameCube ChaoMain 0x33A2C), ported by the body module as
/// `body::apply_capture(&mut player.body, kind)`. Until that module merges this
/// changes nothing and says so; no substitute value is applied.
pub fn capture_body(_p: &mut Player, _kind: u8) -> &'static str {
    "pending-body-module"
}

/// Body growth from eating fruit: `body::apply_fruit(&mut player.body)` once the
/// body module merges. No native fruit alignment change has been traced, so none
/// is applied.
pub fn fruit_body(_p: &mut Player) -> &'static str {
    "pending-body-module"
}
