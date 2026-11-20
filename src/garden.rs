//! Server-owned private garden trees: seeds, planting, watering, growth,
//! fruit and shaking, ported from the GameCube ChaoMain module (GSNE8P).
//!
//! Native sources (ChaoMain section-relative unless noted; times are 60 fps
//! frames; ChaoGlobal "CG" is ChaoMain .data 0x2A50):
//! - ALO_GrowTreeExecutor 0x2CE9C (states 0-10), ALO_CreateGrowTree 0x2BF68,
//!   AL_ClearFruitNum 0x2DFD4, stage models .data 0xB6358 + kind * 12.
//! - Seedling growth +100 every CG+0x112 (60) frames to 10,000; watering sets
//!   it to 10,000. 180 + 200 frame sprout transition.
//! - Young tree: water starts at 10,000 and drains CG+0x118 (14) every CG+0x114
//!   (15) frames; size +5e-5 per frame while water > 5,000, else +2e-5; adult at 1.0.
//! - Adult: life 6,000 (15,000 for Strong, 0x2E1F4); every CG+0x116 (110)
//!   frames life -1 / -2 / -3 for water > 5,000 / > 2,000 / else, and water -14.
//!   life <= 0 on a planted tree: dying 100 frames, dead 18,000, shrink 100,
//!   then the spot is empty.
//! - Fruit: 3 slots; growth +CG+0x11C (5e-5) per frame, capped at 2.0. A slot
//!   over 0.9 counts down one per shake from 11 + rand * 14; at zero the fruit
//!   drops and the slot restarts at -0.3 * rand. Fruit kind = tree kind + 2.
//! - Watering an adult raises each fruit by 0.6 (capped at 1.0).
//! - Permanent garden tree (kind 8, stage 5) never dies and bears fruit
//!   0 / 1 / 2 by garden (fn_60_27FA0).
//!
//! Not ported, and therefore absent rather than substituted: fruit falling on
//! its own above 1.9 (no loose ground fruit exists here), the CG+0x120 cap of
//! 10 garden fruit and the fn_60_2E2B0 object count that gate regrowth, and
//! the visual shake/grow-in animations, and the random starting fruit growth
//! of the permanent trees (0-80 / 0-100 / 0-50 %). Seed prices are not traced
//! yet, so seeds are not sold.
//!
//! Mutations happen only inside a durable care transaction (see `care.rs`).
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const TREE_RULES: &str = "native-gc-chaomain-tree-v1";
pub const GARDEN_VERSION: u32 = 2;
pub const FRAME_MS: f64 = 1000. / 60.;

const SEED_TICK: u64 = 60;
const SEED_STEP: i32 = 100;
const SEED_FULL: i32 = 10_000;
const SPROUT_FRAMES: u64 = 180 + 200;
const YOUNG_TICK: u64 = 15;
const ADULT_TICK: u64 = 110;
const WATER_FULL: i32 = 10_000;
const WATER_DRAIN: i32 = 14;
const FAST_WATER: i32 = 5_000;
const MID_WATER: i32 = 2_000;
const YOUNG_FAST: f32 = 5e-5;
const YOUNG_SLOW: f32 = 2e-5;
const FRUIT_STEP: f32 = 5e-5;
const FRUIT_CAP: f32 = 2.;
const FRUIT_RIPE: f32 = 0.9;
const WATER_FRUIT_BOOST: f32 = 0.6;
const DYING_FRAMES: u64 = 100;
const DEAD_FRAMES: u64 = 18_000;
const SHRINK_FRAMES: u64 = 100;
pub const FRUIT_SLOTS: usize = 3;
/// Server proximity for tending a tree (same 2 m reach as other care points).
pub const PLOT_RANGE: f64 = 2.;

/// Seed families in native tree-kind order 1-7. Fruit kind = tree kind + 2.
pub const SEEDS: [&str; 7] = [
    "strong", "tasty", "hero", "dark", "round", "triangle", "square",
];

/// Native tree spots: fn_60_D3A8 creates seven trees per garden from ChaoMain
/// .data 0x2878 + (garden - 1) * 0x78 + i * 12. AL_InitTreeSaveInfo (0xDD78)
/// makes spots 0-2 permanent garden trees (template 0x2868, kind 8, stage 5)
/// and spots 3-6 empty plantable spots (template 0x2870). Positions are the
/// native ones in server world units (0.1 scale, X mirrored, as the client's
/// glTF root); they match the converted terrain within about 0.1.
///
/// Permanent garden trees: (id, zone, x, z, fruit kind by garden, fn_60_27FA0).
pub const GROVES: [(&str, &str, f64, f64, u8); 9] = [
    ("garden-grove-1", "garden", -9.10, 0.13, 0),
    ("garden-grove-2", "garden", -7.50, 3.83, 0),
    ("garden-grove-3", "garden", 1.20, 5.83, 0),
    ("hero-grove-1", "hero", -0.50, 13.14, 1),
    ("hero-grove-2", "hero", 5.40, 16.84, 1),
    ("hero-grove-3", "hero", -15.10, 19.14, 1),
    ("dark-grove-1", "dark", 1.06, -11.31, 2),
    ("dark-grove-2", "dark", 3.06, 10.69, 2),
    ("dark-grove-3", "dark", -8.14, 6.79, 2),
];

/// Plantable native spots. Hero spots 2-4 lie beyond the current ±22 terrain
/// field and stay unreachable until that field covers them.
pub const PLOTS: [(&str, &str, f64, f64); 12] = [
    ("garden-1", "garden", -1.10, -5.97),
    ("garden-2", "garden", -2.07, 3.53),
    ("garden-3", "garden", -2.50, 12.03),
    ("garden-4", "garden", 2.90, 9.23),
    ("hero-1", "hero", 9.00, -0.06),
    ("hero-2", "hero", -0.80, 25.84),
    ("hero-3", "hero", -9.40, 27.14),
    ("hero-4", "hero", -14.90, 24.04),
    ("dark-1", "dark", 5.46, -3.51),
    ("dark-2", "dark", -3.64, 11.99),
    ("dark-3", "dark", -0.34, -2.81),
    ("dark-4", "dark", -3.94, -10.31),
];

/// Native tree kind (1-7) for a seed family.
pub fn seed_kind(family: &str) -> Option<u8> {
    SEEDS.iter().position(|f| *f == family).map(|i| i as u8 + 1)
}
pub fn seed_item(item_id: &str) -> Option<(&'static str, u8)> {
    let family = item_id.strip_prefix("seed:")?;
    let kind = seed_kind(family)?;
    Some((SEEDS[usize::from(kind - 1)], kind))
}
pub fn plot(id: &str) -> Option<(&'static str, &'static str, f64, f64)> {
    PLOTS.iter().copied().find(|(p, _, _, _)| *p == id)
}
pub fn grove(id: &str) -> Option<(&'static str, &'static str, f64, f64, u8)> {
    GROVES.iter().copied().find(|(g, _, _, _, _)| *g == id)
}
pub fn position(id: &str) -> Option<(&'static str, f64, f64)> {
    plot(id)
        .map(|(_, zone, x, z)| (zone, x, z))
        .or_else(|| grove(id).map(|(_, zone, x, z, _)| (zone, x, z)))
}
pub fn frame_at(ms: f64) -> u64 {
    if ms.is_finite() && ms > 0. {
        (ms / FRAME_MS).floor() as u64
    } else {
        0
    }
}

/// A source of `rand() / 32768`-style values in 0..1.
pub trait Rand {
    fn next(&mut self) -> f32;
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Seed,
    Sprouting,
    Young,
    Adult,
    Dying,
    Dead,
    Shrinking,
    Gone,
}
impl Phase {
    /// Model stage for the client: young / mature / dying stage models.
    pub fn name(self) -> &'static str {
        match self {
            Phase::Seed | Phase::Sprouting => "seed",
            Phase::Young => "young",
            Phase::Adult => "mature",
            Phase::Dying | Phase::Dead | Phase::Shrinking => "dying",
            Phase::Gone => "empty",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Slot {
    pub growth: f32,
    pub shakes_left: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tree {
    /// Native tree kind: 1-7 planted, 8 permanent.
    pub kind: u8,
    /// Native fruit kind this tree bears.
    pub fruit: u8,
    pub phase: Phase,
    /// Seedling growth (0-10,000).
    pub seed_growth: i32,
    /// Young-tree size factor (0-1).
    pub size: f32,
    pub water: i32,
    pub life: i32,
    /// Frames already spent inside the current tick or timed phase.
    pub elapsed: u64,
    /// Absolute frame this state is simulated up to.
    pub frame: u64,
    pub slots: [Slot; FRUIT_SLOTS],
    pub harvested: u32,
}

fn shake_count(rand: &mut dyn Rand) -> u8 {
    (11. + rand.next() * 14.).floor().clamp(11., 24.) as u8
}

impl Tree {
    pub fn planted(kind: u8, frame: u64) -> Self {
        Self {
            kind,
            fruit: kind + 2,
            phase: Phase::Seed,
            seed_growth: 0,
            size: 0.,
            water: WATER_FULL,
            life: 0,
            elapsed: 0,
            frame,
            slots: [Slot {
                growth: 0.,
                shakes_left: 0,
            }; FRUIT_SLOTS],
            harvested: 0,
        }
    }
    pub fn permanent(fruit: u8, frame: u64, rand: &mut dyn Rand) -> Self {
        let mut tree = Self::planted(8, frame);
        tree.fruit = fruit;
        tree.phase = Phase::Adult;
        tree.size = 1.;
        for slot in &mut tree.slots {
            slot.shakes_left = shake_count(rand);
        }
        tree
    }
    fn is_permanent(&self) -> bool {
        self.kind == 8
    }
    fn become_adult(&mut self, rand: &mut dyn Rand) {
        self.phase = Phase::Adult;
        self.life = if self.kind == 1 { 15_000 } else { 6_000 };
        self.elapsed = 0;
        for slot in &mut self.slots {
            *slot = Slot {
                growth: 0.,
                shakes_left: shake_count(rand),
            };
        }
    }
    fn grow_fruit(&mut self, frames: u64) {
        for slot in &mut self.slots {
            slot.growth = (slot.growth + FRUIT_STEP * frames as f32).min(FRUIT_CAP);
        }
    }

    /// Simulates up to `to`. Bounded: every phase ends or reaches a fixed point.
    pub fn advance(&mut self, to: u64, rand: &mut dyn Rand) {
        while self.frame < to {
            let left = to - self.frame;
            match self.phase {
                Phase::Seed => {
                    let step = (SEED_TICK - self.elapsed).min(left);
                    self.frame += step;
                    self.elapsed += step;
                    if self.elapsed == SEED_TICK {
                        self.elapsed = 0;
                        self.seed_growth = (self.seed_growth + SEED_STEP).min(SEED_FULL);
                    }
                    if self.seed_growth >= SEED_FULL {
                        self.phase = Phase::Sprouting;
                        self.elapsed = 0;
                    }
                }
                Phase::Sprouting | Phase::Dying | Phase::Dead | Phase::Shrinking => {
                    let length = match self.phase {
                        Phase::Sprouting => SPROUT_FRAMES,
                        Phase::Dying => DYING_FRAMES,
                        Phase::Dead => DEAD_FRAMES,
                        _ => SHRINK_FRAMES,
                    };
                    let step = (length - self.elapsed).min(left);
                    self.frame += step;
                    self.elapsed += step;
                    if self.elapsed == length {
                        self.elapsed = 0;
                        self.phase = match self.phase {
                            Phase::Sprouting => {
                                self.water = WATER_FULL;
                                Phase::Young
                            }
                            Phase::Dying => Phase::Dead,
                            Phase::Dead => Phase::Shrinking,
                            _ => Phase::Gone,
                        };
                    }
                }
                Phase::Young => {
                    let step = (YOUNG_TICK - self.elapsed).min(left);
                    let rate = if self.water > FAST_WATER {
                        YOUNG_FAST
                    } else {
                        YOUNG_SLOW
                    };
                    self.size = (self.size + rate * step as f32).min(1.);
                    self.frame += step;
                    self.elapsed += step;
                    if self.elapsed == YOUNG_TICK {
                        self.elapsed = 0;
                        self.water = (self.water - WATER_DRAIN).max(0);
                    }
                    if self.size >= 1. {
                        self.become_adult(rand);
                    }
                }
                Phase::Adult => {
                    if self.is_permanent()
                        && self.slots.iter().all(|s| s.growth >= FRUIT_CAP)
                        && self.water == 0
                    {
                        // Fixed point: nothing observable changes any more.
                        self.frame = to;
                        break;
                    }
                    let step = (ADULT_TICK - self.elapsed).min(left);
                    self.grow_fruit(step);
                    self.frame += step;
                    self.elapsed += step;
                    if self.elapsed == ADULT_TICK {
                        self.elapsed = 0;
                        if !self.is_permanent() {
                            self.life -= if self.water > FAST_WATER {
                                1
                            } else if self.water > MID_WATER {
                                2
                            } else {
                                3
                            };
                        }
                        self.water = (self.water - WATER_DRAIN).max(0);
                        if !self.is_permanent() && self.life <= 0 {
                            self.phase = Phase::Dying;
                        }
                    }
                }
                Phase::Gone => {
                    self.frame = to;
                }
            }
        }
    }

    /// Watering (fn_60_19E64(task, 4, 0xAD) and the adult +0xB0 flag).
    pub fn water(&mut self) -> bool {
        match self.phase {
            Phase::Seed => self.seed_growth = SEED_FULL,
            Phase::Young => self.water = WATER_FULL,
            Phase::Adult => {
                self.water = WATER_FULL;
                for slot in &mut self.slots {
                    if slot.growth < 1. {
                        slot.growth = (slot.growth.max(0.) + WATER_FRUIT_BOOST).min(1.);
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// One shake. Returns the number of fruit that dropped.
    pub fn shake(&mut self, rand: &mut dyn Rand) -> u8 {
        if self.phase != Phase::Adult {
            return 0;
        }
        let mut dropped = 0;
        for slot in &mut self.slots {
            if slot.growth > FRUIT_RIPE {
                slot.shakes_left = slot.shakes_left.saturating_sub(1);
                if slot.shakes_left == 0 {
                    dropped += 1;
                    slot.growth = -0.3 * rand.next();
                    slot.shakes_left = shake_count(rand);
                }
            }
        }
        self.harvested = self.harvested.saturating_add(u32::from(dropped));
        dropped
    }

    pub fn ripe(&self) -> usize {
        if self.phase == Phase::Adult {
            self.slots.iter().filter(|s| s.growth > FRUIT_RIPE).count()
        } else {
            0
        }
    }

    fn valid(&self) -> bool {
        ((1..=7).contains(&self.kind) && self.fruit == self.kind + 2
            || self.kind == 8 && self.fruit <= 2)
            && (0..=SEED_FULL).contains(&self.seed_growth)
            && (0..=WATER_FULL).contains(&self.water)
            && self.size.is_finite()
            && (0. ..=1.).contains(&self.size)
            && self.life <= 15_000
            && self.elapsed <= DEAD_FRAMES
            && self.slots.iter().all(|s| {
                s.growth.is_finite()
                    && (-0.3..=FRUIT_CAP).contains(&s.growth)
                    && s.shakes_left <= 24
            })
    }

    pub fn public(&self) -> Value {
        json!({"kind":self.kind,"family":SEEDS.get(usize::from(self.kind.wrapping_sub(1))),
            "stage":self.phase.name(),"phase":self.phase,"fruitId":format!("sa2-fruit:{}",self.fruit),
            "fruitReady":self.ripe(),"fruitGrowth":self.slots.iter().map(|s|s.growth).collect::<Vec<_>>(),
            "seedGrowth":self.seed_growth,"size":self.size,"water":self.water,"life":self.life,"harvested":self.harvested})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GardenState {
    pub version: u32,
    pub trees: BTreeMap<String, Tree>,
    pub groves: BTreeMap<String, Tree>,
}

impl Default for GardenState {
    fn default() -> Self {
        Self {
            version: GARDEN_VERSION,
            trees: BTreeMap::new(),
            groves: BTreeMap::new(),
        }
    }
}

impl GardenState {
    /// Absent data starts empty. Earlier (tuned, unreleased) versions or invalid
    /// data stop loading rather than being reinterpreted.
    pub fn restore(value: Option<&Value>) -> Result<Self, String> {
        let Some(value) = value.filter(|v| !v.is_null()) else {
            return Ok(Self::default());
        };
        let state: Self =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid saved garden state")?;
        if state.version != GARDEN_VERSION
            || state
                .trees
                .iter()
                .any(|(id, t)| plot(id).is_none() || t.kind > 7 || !t.valid())
            || state.groves.iter().any(|(id, t)| {
                grove(id).is_none_or(|(_, _, _, _, fruit)| t.kind != 8 || t.fruit != fruit)
                    || !t.valid()
            })
        {
            return Err("Invalid or unsupported saved garden state".into());
        }
        Ok(state)
    }

    /// Adds any missing permanent grove tree, starting at `frame`.
    pub fn seed_groves(&mut self, frame: u64, rand: &mut dyn Rand) {
        for (id, _, _, _, fruit) in GROVES {
            self.groves
                .entry(id.into())
                .or_insert_with(|| Tree::permanent(fruit, frame, rand));
        }
    }

    /// Brings every tree up to `frame`. Called inside transactions only.
    pub fn advance(&mut self, frame: u64, rand: &mut dyn Rand) {
        self.seed_groves(frame, rand);
        for tree in self.groves.values_mut() {
            tree.advance(frame, rand);
        }
        for tree in self.trees.values_mut() {
            tree.advance(frame, rand);
        }
        self.trees.retain(|_, t| t.phase != Phase::Gone);
    }

    /// Tree at a plot or grove, if any.
    pub fn tree_mut(&mut self, place: &str) -> Option<&mut Tree> {
        if grove(place).is_some() {
            self.groves.get_mut(place)
        } else {
            self.trees.get_mut(place)
        }
    }

    /// View of the saved state. Clients see stages as of the last server step.
    pub fn public(&self) -> Value {
        let groves: Vec<Value> = GROVES
            .iter()
            .map(|(id, zone, x, z, fruit)| {
                json!({"id":id,"zone":zone,"x":x,"z":z,"fruitId":format!("sa2-fruit:{fruit}"),
                    "tree":self.groves.get(*id).map(Tree::public)})
            })
            .collect();
        let plots: Vec<Value> = PLOTS
            .iter()
            .map(|(id, zone, x, z)| json!({"id":id,"zone":zone,"x":x,"z":z,"tree":self.trees.get(*id).map(Tree::public)}))
            .collect();
        json!({"version":self.version,"rules":TREE_RULES,"plots":plots,"groves":groves,
            "positions":"native-gc-tree-spots","seedSale":"not-traced",
            "notPorted":["fruit-auto-fall-above-1.9","garden-fruit-cap-10","regrow-object-count"]})
    }
}

/// Ripeness view only: drops never happen while projecting, so no random
/// value is ever used for anything that is kept.
struct Fixed;
impl Rand for Fixed {
    fn next(&mut self) -> f32 {
        0.
    }
}

/// For Chao behaviour: the nearest tree in the Chao's zone with ripe fruit
/// at game time `now` (ms). Reads a projection; nothing is stored.
pub fn nearest_fruit(p: &crate::game::Player, now: f64) -> Option<(String, f64, f64)> {
    let mut garden = p.garden.clone();
    garden.advance(frame_at(now), &mut Fixed);
    let mut best: Option<(f64, String, f64, f64)> = None;
    let places = GROVES
        .iter()
        .map(|(id, zone, x, z, _)| (*id, *zone, *x, *z, garden.groves.get(*id)))
        .chain(
            PLOTS
                .iter()
                .map(|(id, zone, x, z)| (*id, *zone, *x, *z, garden.trees.get(*id))),
        );
    for (id, zone, x, z, tree) in places {
        if zone == p.zone && tree.is_some_and(|t| t.ripe() > 0) {
            let d = (p.x - x).hypot(p.z - z);
            if best.as_ref().is_none_or(|(bd, _, _, _)| d < *bd) {
                best = Some((d, id.into(), x, z));
            }
        }
    }
    best.map(|(_, id, x, z)| (id, x, z))
}

#[cfg(test)]
pub(crate) struct Scripted(pub Vec<f32>, pub usize);
#[cfg(test)]
impl Rand for Scripted {
    fn next(&mut self) -> f32 {
        let v = self.0[self.1 % self.0.len()];
        self.1 += 1;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r() -> Scripted {
        Scripted(vec![0.], 0)
    }

    #[test]
    fn seedling_takes_6000_frames_or_sprouts_when_watered() {
        let mut t = Tree::planted(3, 0);
        t.advance(5_999, &mut r());
        assert_eq!(t.phase, Phase::Seed);
        assert_eq!(t.seed_growth, 9_900);
        t.advance(6_000, &mut r());
        assert_eq!(t.phase, Phase::Sprouting);
        t.advance(6_000 + SPROUT_FRAMES, &mut r());
        assert_eq!(t.phase, Phase::Young);
        let mut w = Tree::planted(3, 0);
        assert!(w.water());
        w.advance(1, &mut r());
        assert_eq!(w.phase, Phase::Sprouting);
    }

    #[test]
    fn young_growth_is_fast_while_watered_and_slow_when_dry() {
        let mut t = Tree::planted(2, 0);
        t.phase = Phase::Young;
        t.water = WATER_FULL;
        // Water stays above 5,000 for 357 drains (5,355 frames).
        t.advance(5_355, &mut r());
        assert!((t.size - 5_355. * YOUNG_FAST).abs() < 1e-4);
        assert!(t.water > FAST_WATER);
        let mut kept = t.clone();
        t.advance(5_355 + 1_500, &mut r());
        assert!(
            (t.size - (5_355. * YOUNG_FAST + 15. * YOUNG_FAST + 1_485. * YOUNG_SLOW)).abs() < 1e-3
        );
        kept.water();
        kept.advance(5_355 + 1_500, &mut r());
        assert!(kept.size > t.size);
        // Fully watered growth reaches adulthood at 20,000 frames.
        let mut fast = Tree::planted(2, 0);
        fast.phase = Phase::Young;
        let mut frame = 0;
        while fast.phase == Phase::Young {
            frame += 3_000;
            fast.water();
            fast.advance(frame, &mut r());
        }
        assert_eq!(fast.phase, Phase::Adult);
        // Life starts at 6,000 and drains for the frames past adulthood.
        assert!(fast.life <= 6_000 && fast.life > 5_950, "{}", fast.life);
        assert!(frame <= 21_000);
    }

    #[test]
    fn adult_life_drains_by_water_and_strong_trees_live_longer() {
        let mut t = Tree::planted(5, 0);
        t.become_adult(&mut r());
        t.water = WATER_FULL;
        t.advance(110, &mut r());
        assert_eq!((t.life, t.water), (5_999, 9_986));
        t.water = 3_000;
        t.advance(220, &mut r());
        assert_eq!(t.life, 5_997);
        t.water = 0;
        t.advance(330, &mut r());
        assert_eq!(t.life, 5_994);
        let mut s = Tree::planted(1, 0);
        s.become_adult(&mut r());
        assert_eq!(s.life, 15_000);
    }

    #[test]
    fn dead_trees_pass_dying_dead_and_shrink_then_free_the_spot() {
        let mut g = GardenState::default();
        let mut t = Tree::planted(4, 0);
        t.become_adult(&mut r());
        t.life = 1;
        t.water = 0;
        g.trees.insert("garden-1".into(), t);
        g.advance(110, &mut r());
        assert_eq!(g.trees["garden-1"].phase, Phase::Dying);
        g.advance(110 + DYING_FRAMES + DEAD_FRAMES, &mut r());
        assert_eq!(g.trees["garden-1"].phase, Phase::Shrinking);
        g.advance(110 + DYING_FRAMES + DEAD_FRAMES + SHRINK_FRAMES, &mut r());
        assert!(!g.trees.contains_key("garden-1"));
    }

    #[test]
    fn fruit_ripens_then_drops_after_its_shake_count() {
        let mut t = Tree::planted(5, 0);
        t.become_adult(&mut Scripted(vec![0.5], 0));
        assert_eq!(t.slots[0].shakes_left, 18);
        // 0.9 needs 18,000 frames at 5e-5.
        t.advance(18_040, &mut r());
        assert_eq!(t.ripe(), 3);
        for _ in 0..17 {
            assert_eq!(t.shake(&mut r()), 0);
        }
        assert_eq!(t.shake(&mut Scripted(vec![0.5], 0)), 3);
        assert_eq!(t.harvested, 3);
        assert!(t.slots.iter().all(|s| s.growth < 0.));
        assert_eq!(t.fruit, 7);
    }

    #[test]
    fn watering_an_adult_near_ripens_fruit_and_permanent_trees_never_die() {
        let mut t = Tree::planted(6, 0);
        t.become_adult(&mut r());
        t.slots[0].growth = 0.5;
        t.slots[1].growth = -0.2;
        t.water();
        assert_eq!(t.slots[0].growth, 1.);
        assert!((t.slots[1].growth - 0.6).abs() < 1e-6);
        let mut p = Tree::permanent(1, 0, &mut r());
        p.advance(60 * 60 * 60 * 24 * 30, &mut r());
        assert_eq!(p.phase, Phase::Adult);
        assert_eq!(p.ripe(), 3);
    }

    #[test]
    fn restore_rejects_unknown_spots_kinds_and_old_versions() {
        let mut g = GardenState::default();
        g.trees.insert("garden-1".into(), Tree::planted(2, 5));
        g.advance(10, &mut r());
        let v = serde_json::to_value(&g).unwrap();
        assert_eq!(GardenState::restore(Some(&v)).unwrap(), g);
        let mut bad = v.clone();
        bad["version"] = json!(1);
        assert!(GardenState::restore(Some(&bad)).is_err());
        let mut bad = v.clone();
        bad["trees"]["nowhere"] = bad["trees"]["garden-1"].clone();
        assert!(GardenState::restore(Some(&bad)).is_err());
        let mut bad = v.clone();
        bad["trees"]["garden-1"]["kind"] = json!(9);
        assert!(GardenState::restore(Some(&bad)).is_err());
        let mut bad = v;
        bad["groves"]["hero-grove-1"] = bad["groves"]["garden-grove-1"].clone();
        assert!(GardenState::restore(Some(&bad)).is_err());
    }
}
