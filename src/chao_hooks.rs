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
//! The life module merged in PR #27 and is wired here. The body module has not
//! merged; its hooks still change nothing and say so.
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
use crate::{
    game::{ALL_STATS, Player},
    life::{self, Need, Stage, StatProgress},
};
use serde_json::{Value, json};

/// Native fruit index of the Heart Fruit (breeding).
const HEART_FRUIT: u8 = 10;
/// Breed need added by a Heart Fruit on its last bite.
const HEART_FRUIT_BREED: i32 = 10_000;
/// ALO_FruitHeldC eats a fruit in four bites.
const FRUIT_BITES: u8 = 4;

/// Adds signed skill points with the published adapted clamp (0-9999). Used
/// only by the pre-existing adapted `garden-fruit`. Owner perfect stats never change.
pub fn grant_stat(p: &mut Player, stat: &str, delta: i16) -> i64 {
    let stat = p.stats.get_mut(stat).expect("seven canonical stats");
    let before = stat.points;
    if !p.admin {
        stat.points = (i64::from(before) + i64::from(delta)).clamp(0, 9999) as u32;
    }
    i64::from(stat.points) - i64::from(before)
}

/// Per-stat summary of native `AL_ParameterGrow` results.
fn progress(gains: &[[StatProgress; 7]]) -> Value {
    let mut points = serde_json::Map::new();
    let mut exp = serde_json::Map::new();
    let mut levels = 0u32;
    for (i, stat) in ALL_STATS.iter().enumerate() {
        let p: i64 = gains.iter().map(|g| i64::from(g[i].points)).sum();
        let e: i64 = gains.iter().map(|g| i64::from(g[i].exp_delta)).sum();
        levels += gains.iter().map(|g| u32::from(g[i].levels)).sum::<u32>();
        points.insert((*stat).into(), json!(p));
        exp.insert((*stat).into(), json!(e));
    }
    json!({"points":points,"exp":exp,"levelUps":levels,"levelsChanged":levels > 0})
}
fn seven(v: Vec<StatProgress>) -> [StatProgress; 7] {
    v.try_into().unwrap_or_default()
}

/// ALBHV_Capture: an animal or drive's MinimalGrowParam row applied once
/// through `life::capture_animal` (Exp, at most one level per stat, like +1).
pub fn capture_stats(p: &mut Player, row: [i16; 7]) -> Value {
    let gains = seven(life::capture_animal(p, row));
    progress(&[gains])
}

/// ALO_FruitHeldC: four bites through `life::eat_fruit_bite` (row share,
/// hunger, life, LifeMax), and Heart Fruit breed +10000 on the last bite.
pub fn fruit_bites(p: &mut Player, kind: u8, row: [i16; 7]) -> Value {
    let gains: Vec<_> = (1..=FRUIT_BITES)
        .map(|bite| seven(life::eat_fruit_bite(p, kind, row, bite)))
        .collect();
    let mut out = progress(&gains);
    out["bites"] = json!(FRUIT_BITES);
    if kind == HEART_FRUIT {
        out["breed"] = json!(life::adjust_need(p, Need::Breed, HEART_FRUIT_BREED));
    }
    out
}

/// fn_60_2B8FC: the owner handing over a fruit, like +1.
pub fn fruit_handed_over(p: &mut Player) {
    life::fruit_handed_over(p);
}

/// fn_60_3FCA4: petting, like +1.
pub fn pet(p: &mut Player) {
    life::pet(p);
}

/// Child or adult, for choosing the native child or adult part list.
pub fn is_adult(p: &Player) -> bool {
    p.life.stage == Stage::Adult
}

/// Body change when a Chao is given an animal or drive. The native rule is
/// ALBHV_Capture (GameCube ChaoMain 0x33A2C), ported by the body module as
/// `body::apply_capture(&mut player.body, kind)`. Until that module merges this
/// changes nothing and says so; no substitute value is applied.
pub fn capture_body(_p: &mut Player, _kind: u8) -> &'static str {
    "pending-body-module"
}

/// Body growth from eating fruit: `body::apply_fruit(&mut player.body, kind,
/// last_bite)` per bite once the body module merges (it can read the life
/// module's `FruitBite` events). Until then nothing changes.
pub fn fruit_body(_p: &mut Player, _kind: u8) -> &'static str {
    "pending-body-module"
}
