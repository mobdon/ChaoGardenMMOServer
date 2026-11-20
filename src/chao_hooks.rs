//! Single integration seam between care/garden effects and the Chao state
//! owned by other modules.
//!
//! Agreed ownership (server coordination, November 20, 2026):
//! - `life` owns stat experience/level/grade curves and needs. When it lands,
//!   `grant_stat` calls `life::grant_stat_progress` and `feed` calls
//!   `life::feed`.
//! - `body` owns the run/power, swim/fly and hero/dark axes. When it lands,
//!   `shape` calls `body::apply_shape_delta(&mut player.body, ..)`.
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

/// Body-axis deltas in native AL_BODY_INFO scale (-1..1 per axis; run -1 /
/// power +1, swim -1 / fly +1, dark -1 / hero +1).
///
/// Until `body.rs` lands only the hero/dark axis has a home: the existing
/// -100..100 alignment, which is the same axis scaled by 100. Run/power and
/// swim/fly are reported in the effect but not stored yet.
pub fn shape(p: &mut Player, _run_power: f32, _swim_fly: f32, hero_dark: f32) -> f64 {
    let before = p.alignment;
    // f32 axis units carry float noise (0.05f32 != 0.05); alignment keeps 4 decimals.
    let step = (f64::from(hero_dark) * 1_000_000.).round() / 10_000.;
    p.alignment = (p.alignment + step).clamp(-100., 100.);
    p.alignment - before
}
