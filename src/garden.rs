//! Server-owned private garden trees: seeds, planting, watering, growth stages,
//! fruit and harvest.
//!
//! Native evidence: seven seed/tree families (Strong, Tasty, Hero, Dark, Round,
//! Triangle, Square) with young/mature/dying stage models, and the 24-entry
//! native fruit enum (client docs/MECHANICS.md CAT-TREE-01, CAT-FOOD-01).
//! Native growth timers, watering rules, fruit counts and plot layout are not
//! decoded. Every number below is labelled MMO tuning (`TREE_RULES`) and is
//! reported to clients so it is never presented as a retail rule.
//!
//! Mutations happen only inside a durable care transaction (see `care.rs`), so
//! planting, watering and harvesting share its revision, receipt and save path.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const TREE_RULES: &str = "adapted-tree-growth-v1";
pub const GARDEN_VERSION: u32 = 1;

// MMO tuning, milliseconds of wall-clock time. Not decoded native values.
pub const SPROUT_MS: f64 = 30. * 60. * 1000.;
pub const MATURE_MS: f64 = 3. * 60. * 60. * 1000.;
pub const FRUITING_MS: f64 = 3. * 24. * 60. * 60. * 1000.;
pub const DYING_MS: f64 = 12. * 60. * 60. * 1000.;
pub const FRUIT_MS: f64 = 20. * 60. * 1000.;
pub const MAX_FRUIT: u8 = 3;
pub const WATER_BONUS_MS: f64 = 15. * 60. * 1000.;
pub const WATER_COOLDOWN_MS: f64 = 10. * 60. * 1000.;
pub const PLOT_RANGE: f64 = 2.;

/// Seed families in native tree-family order, with the native fruit ID each
/// mature tree bears and the adapted hub price in rings.
pub const SEEDS: [(&str, u8, u32); 7] = [
    ("strong", 3, 10),
    ("tasty", 4, 10),
    ("hero", 5, 15),
    ("dark", 6, 15),
    ("round", 7, 10),
    ("triangle", 8, 10),
    ("square", 9, 10),
];

/// Permanent garden trees. Each garden's own tree bears that garden's native
/// fruit (Chao Garden 0, Hero Garden 1, Dark Garden 2). Positions: the Neutral
/// grove reuses the existing grove interaction point; the others are tuning.
pub const GROVES: [(&str, &str, f64, f64, u8); 3] = [
    ("garden-grove", "garden", -7., -5., 0),
    ("hero-grove", "hero", -6., -6., 1),
    ("dark-grove", "dark", -6., -6., 2),
];

/// Plantable plots per garden zone (MMO layout tuning, inside the ±22 bounds).
pub const PLOTS: [(&str, &str, f64, f64); 12] = [
    ("garden-1", "garden", 10., 2.),
    ("garden-2", "garden", 13., 2.),
    ("garden-3", "garden", 10., -2.),
    ("garden-4", "garden", 13., -2.),
    ("garden-5", "garden", 2., 12.),
    ("garden-6", "garden", 6., 12.),
    ("hero-1", "hero", 10., 2.),
    ("hero-2", "hero", 13., 2.),
    ("hero-3", "hero", 10., -2.),
    ("dark-1", "dark", 10., 2.),
    ("dark-2", "dark", 13., 2.),
    ("dark-3", "dark", 10., -2.),
];

pub fn seed(family: &str) -> Option<(&'static str, u8, u32)> {
    SEEDS.iter().copied().find(|(f, _, _)| *f == family)
}
pub fn seed_item(item_id: &str) -> Option<(&'static str, u8, u32)> {
    seed(item_id.strip_prefix("seed:")?)
}
pub fn plot(id: &str) -> Option<(&'static str, &'static str, f64, f64)> {
    PLOTS.iter().copied().find(|(p, _, _, _)| *p == id)
}
pub fn grove(id: &str) -> Option<(&'static str, &'static str, f64, f64, u8)> {
    GROVES.iter().copied().find(|(g, _, _, _, _)| *g == id)
}
/// Any plot or grove position, for proximity checks.
pub fn position(id: &str) -> Option<(&'static str, f64, f64)> {
    plot(id)
        .map(|(_, zone, x, z)| (zone, x, z))
        .or_else(|| grove(id).map(|(_, zone, x, z, _)| (zone, x, z)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Seed,
    Young,
    Mature,
    Dying,
    Dead,
}
impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Seed => "seed",
            Stage::Young => "young",
            Stage::Mature => "mature",
            Stage::Dying => "dying",
            Stage::Dead => "dead",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tree {
    pub family: String,
    pub planted_at: f64,
    /// Total growth time granted by watering.
    pub water_bonus: f64,
    pub last_watered_at: Option<f64>,
    pub fruit_ready: u8,
    /// Fruit growth is accounted up to this wall-clock time.
    pub fruit_clock_at: f64,
    pub harvested: u32,
}

impl Tree {
    pub fn new(family: &str, now: f64) -> Self {
        Self {
            family: family.into(),
            planted_at: now,
            water_bonus: 0.,
            last_watered_at: None,
            fruit_ready: 0,
            fruit_clock_at: now,
            harvested: 0,
        }
    }
    /// Wall-clock time the tree became (or becomes) mature.
    fn mature_at(&self) -> f64 {
        self.planted_at + MATURE_MS - self.water_bonus
    }
    fn dying_at(&self) -> f64 {
        self.mature_at() + FRUITING_MS
    }
    pub fn stage(&self, now: f64) -> Stage {
        let age = now - self.planted_at + self.water_bonus;
        if age < SPROUT_MS {
            Stage::Seed
        } else if now < self.mature_at() {
            Stage::Young
        } else if now < self.dying_at() {
            Stage::Mature
        } else if now < self.dying_at() + DYING_MS {
            Stage::Dying
        } else {
            Stage::Dead
        }
    }
    pub fn fruit_id(&self) -> Option<u8> {
        seed(&self.family).map(|(_, fruit, _)| fruit)
    }
    /// Lazily grows fruit while mature. Dying trees keep hanging fruit but grow
    /// no more; a dead tree drops everything.
    pub fn advance(&mut self, now: f64) {
        let (start, end) = (self.mature_at(), self.dying_at());
        advance_fruit(
            &mut self.fruit_ready,
            &mut self.fruit_clock_at,
            start,
            end,
            now,
        );
        if self.stage(now) == Stage::Dead {
            self.fruit_ready = 0;
        }
    }
    fn valid(&self) -> bool {
        seed(&self.family).is_some()
            && [self.planted_at, self.water_bonus, self.fruit_clock_at]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.)
            && self.water_bonus <= MATURE_MS
            && self.fruit_ready <= MAX_FRUIT
            && self
                .last_watered_at
                .is_none_or(|at| at.is_finite() && at >= 0.)
    }
}

fn advance_fruit(ready: &mut u8, clock: &mut f64, start: f64, end: f64, now: f64) {
    if *ready >= MAX_FRUIT {
        // Growth pauses while full and restarts from the next release.
        *clock = clock.max(now);
        return;
    }
    let from = clock.max(start);
    let until = now.min(end);
    if until <= from {
        return;
    }
    let grown = ((until - from) / FRUIT_MS).floor() as u64;
    let add = grown.min(u64::from(MAX_FRUIT - *ready)) as u8;
    *ready += add;
    *clock = if *ready >= MAX_FRUIT {
        now.max(from)
    } else {
        from + f64::from(add) * FRUIT_MS
    };
}

/// A permanent garden tree's hanging fruit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grove {
    pub fruit_ready: u8,
    pub fruit_clock_at: f64,
    pub harvested: u32,
}

impl Grove {
    pub fn advance(&mut self, now: f64) {
        advance_fruit(
            &mut self.fruit_ready,
            &mut self.fruit_clock_at,
            0.,
            f64::INFINITY,
            now,
        );
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GardenState {
    pub version: u32,
    pub trees: BTreeMap<String, Tree>,
    pub groves: BTreeMap<String, Grove>,
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
    /// Absent legacy data starts empty. Present but invalid data stops loading.
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
                .any(|(id, tree)| plot(id).is_none() || !tree.valid())
            || state.groves.iter().any(|(id, g)| {
                grove(id).is_none()
                    || g.fruit_ready > MAX_FRUIT
                    || !g.fruit_clock_at.is_finite()
                    || g.fruit_clock_at < 0.
            })
        {
            return Err("Invalid or unsupported saved garden state".into());
        }
        Ok(state)
    }

    /// A grove the profile has never touched starts with full fruit.
    pub fn grove_mut(&mut self, id: &str, now: f64) -> &mut Grove {
        let grove = self.groves.entry(id.into()).or_insert(Grove {
            fruit_ready: MAX_FRUIT,
            fruit_clock_at: now,
            harvested: 0,
        });
        grove.advance(now);
        grove
    }

    /// Read-only view at `now`; never mutates saved state.
    pub fn public(&self, now: f64) -> Value {
        let groves: Vec<Value> = GROVES
            .iter()
            .map(|(id, zone, x, z, fruit)| {
                let mut g = self.groves.get(*id).cloned().unwrap_or(Grove {
                    fruit_ready: MAX_FRUIT,
                    fruit_clock_at: now,
                    harvested: 0,
                });
                g.advance(now);
                json!({"id":id,"zone":zone,"x":x,"z":z,"fruitId":format!("sa2-fruit:{fruit}"),
                    "fruitReady":g.fruit_ready,"nextFruitAt":(g.fruit_ready<MAX_FRUIT).then(|| g.fruit_clock_at.max(0.)+FRUIT_MS)})
            })
            .collect();
        let plots: Vec<Value> = PLOTS
            .iter()
            .map(|(id, zone, x, z)| {
                let tree = self.trees.get(*id).map(|t| {
                    let mut t = t.clone();
                    t.advance(now);
                    let stage = t.stage(now);
                    json!({"family":t.family,"stage":stage.name(),"plantedAt":t.planted_at,
                        "matureAt":t.mature_at(),"dyingAt":t.dying_at(),"deadAt":t.dying_at()+DYING_MS,
                        "fruitId":t.fruit_id().map(|f|format!("sa2-fruit:{f}")),"fruitReady":t.fruit_ready,
                        "waterReadyAt":t.last_watered_at.map(|at|at+WATER_COOLDOWN_MS),"harvested":t.harvested})
                });
                json!({"id":id,"zone":zone,"x":x,"z":z,"tree":tree})
            })
            .collect();
        json!({"version":self.version,"rules":TREE_RULES,"classification":"adapted-mmo-tuning",
            "nativeTimers":"unknown","plots":plots,"groves":groves,
            "seeds":SEEDS.iter().map(|(f,fruit,price)|json!({"itemId":format!("seed:{f}"),"family":f,"fruitId":format!("sa2-fruit:{fruit}"),"price":price})).collect::<Vec<_>>(),
            "tuning":{"sproutMs":SPROUT_MS,"matureMs":MATURE_MS,"fruitingMs":FRUITING_MS,"dyingMs":DYING_MS,
                "fruitMs":FRUIT_MS,"maxFruit":MAX_FRUIT,"waterBonusMs":WATER_BONUS_MS,"waterCooldownMs":WATER_COOLDOWN_MS,"plotRange":PLOT_RANGE}})
    }
}

/// For Chao behaviour: the nearest hanging fruit in the Chao's zone, as
/// `(plot or grove id, x, z)`. Read-only; take it with `care::eat_garden_fruit`.
pub fn nearest_fruit(p: &crate::game::Player, now: f64) -> Option<(String, f64, f64)> {
    let mut best: Option<(f64, String, f64, f64)> = None;
    let mut consider = |id: &str, x: f64, z: f64| {
        let d = (p.x - x).hypot(p.z - z);
        if best.as_ref().is_none_or(|(bd, _, _, _)| d < *bd) {
            best = Some((d, id.into(), x, z));
        }
    };
    for (id, zone, x, z, _) in GROVES {
        if zone == p.zone {
            let mut g = p.garden.groves.get(id).cloned().unwrap_or(Grove {
                fruit_ready: MAX_FRUIT,
                fruit_clock_at: now,
                harvested: 0,
            });
            g.advance(now);
            if g.fruit_ready > 0 {
                consider(id, x, z);
            }
        }
    }
    for (id, zone, x, z) in PLOTS {
        if zone == p.zone
            && let Some(t) = p.garden.trees.get(id)
        {
            let mut t = t.clone();
            t.advance(now);
            if t.fruit_ready > 0 {
                consider(id, x, z);
            }
        }
    }
    best.map(|(_, id, x, z)| (id, x, z))
}

#[cfg(test)]
mod tests {
    use super::*;
    const H: f64 = 60. * 60. * 1000.;

    #[test]
    fn stages_follow_tuned_timers_and_watering_speeds_growth() {
        let mut t = Tree::new("hero", 1000.);
        assert_eq!(t.stage(1000.), Stage::Seed);
        assert_eq!(t.stage(1000. + SPROUT_MS), Stage::Young);
        assert_eq!(t.stage(1000. + MATURE_MS - 1.), Stage::Young);
        t.water_bonus = WATER_BONUS_MS;
        assert_eq!(t.stage(1000. + MATURE_MS - WATER_BONUS_MS), Stage::Mature);
        let dying = t.dying_at();
        assert_eq!(t.stage(dying), Stage::Dying);
        assert_eq!(t.stage(dying + DYING_MS), Stage::Dead);
        assert_eq!(t.fruit_id(), Some(5));
    }

    #[test]
    fn fruit_grows_only_while_mature_and_caps() {
        let mut t = Tree::new("round", 0.);
        t.advance(MATURE_MS - 1.);
        assert_eq!(t.fruit_ready, 0);
        t.advance(MATURE_MS + FRUIT_MS * 2. + 5.);
        assert_eq!(t.fruit_ready, 2);
        t.advance(MATURE_MS + 10. * H);
        assert_eq!(t.fruit_ready, MAX_FRUIT);
        t.fruit_ready = 0;
        // Production restarts from the moment the cap was released.
        t.advance(MATURE_MS + 10. * H + FRUIT_MS - 1.);
        assert_eq!(t.fruit_ready, 0);
        t.advance(MATURE_MS + 10. * H + FRUIT_MS);
        assert_eq!(t.fruit_ready, 1);
        t.fruit_ready = 0;
        t.advance(t.dying_at() + 5. * H);
        assert!(t.fruit_ready <= MAX_FRUIT);
        let before = t.fruit_ready;
        t.advance(t.dying_at() + 11. * H);
        assert_eq!(t.fruit_ready, before, "dying trees grow nothing");
        t.advance(t.dying_at() + DYING_MS);
        assert_eq!(t.fruit_ready, 0, "dead trees drop fruit");
    }

    #[test]
    fn offline_growth_is_deterministic_regardless_of_tick_pattern() {
        let mut a = Tree::new("square", 0.);
        let mut b = a.clone();
        for step in 0..500 {
            a.advance(f64::from(step) * 60_000.);
        }
        b.advance(499. * 60_000.);
        assert_eq!(a.fruit_ready, b.fruit_ready);
    }

    #[test]
    fn restore_rejects_unknown_plots_families_and_bad_numbers() {
        assert_eq!(GardenState::restore(None).unwrap(), GardenState::default());
        let mut s = GardenState::default();
        s.trees.insert("garden-1".into(), Tree::new("tasty", 5.));
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(GardenState::restore(Some(&v)).unwrap(), s);
        for bad in [
            json!({"version":1,"trees":{"nowhere":serde_json::to_value(Tree::new("tasty",5.)).unwrap()},"groves":{}}),
            json!({"version":1,"trees":{"garden-1":serde_json::to_value(Tree::new("banana",5.)).unwrap()},"groves":{}}),
            json!({"version":2,"trees":{},"groves":{}}),
            json!({"version":1,"trees":{},"groves":{"garden-grove":{"fruitReady":9,"fruitClockAt":0.,"harvested":0}}}),
            json!({"version":1,"trees":{},"groves":{},"extra":true}),
        ] {
            assert!(GardenState::restore(Some(&bad)).is_err(), "{bad}");
        }
    }
}
