//! Chao life cycle, stat growth, needs, mood, illness, personality, knowledge and genes.
//!
//! Field layout and names follow the reverse-engineered `CHAO_PARAM_GC`, `AL_EMOTION`,
//! `AL_KNOWLEDGE` and `AL_GENE` structures in X-Hax `SA2Structs.h`. Those names say *where*
//! the original game keeps a value. They do not establish its formula. Every rate, threshold,
//! range scale and inheritance rule in [`tuning`] is labelled MMO tuning until the retail
//! executable is traced. See `LIFE.md`.
use crate::game::{ALL_STATS, Game, Player};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const VERSION: u32 = 1;
pub const RULES: &str = "adapted-life-v1";
pub const MAX_RECEIPTS: usize = 256;

/// Every value here is MMO tuning, not a verified native constant.
pub mod tuning {
    /// One `ClockRollovers` (age) step per this many connected seconds.
    pub const ROLLOVER_SECONDS: f64 = 1800.;
    /// Child age, in rollovers, at which the evolution cocoon starts.
    pub const CHILD_ROLLOVERS: u16 = 4;
    pub const COCOON_SECONDS: f64 = 60.;
    /// `LifeMax` in rollovers before the gene's LifeTime alleles are added.
    pub const BASE_LIFE: i16 = 200;
    /// Happiness needed for a pink (reincarnation) cocoon. Community-documented as 50 for the
    /// original; not verified against the executable.
    pub const REINCARNATE_HAPPINESS: f64 = 50.;
    /// Reincarnation keeps points / this divisor and resets levels to zero.
    pub const REINCARNATE_POINT_DIVISOR: u32 = 10;
    /// Points added per level-up by grade E, D, C, B, A, S, plus 0..=3 random and level / 2.
    pub const GRADE_POINT_BASE: [u32; 6] = [12, 15, 18, 21, 24, 27];
    pub const NEED_MAX: i16 = 10000;
    pub const MOOD_MAX: i8 = 100;
    // Per connected second, in State units (0..=10000).
    pub const SLEEPINESS_PER_SEC: f64 = 1.4;
    pub const TIREDNESS_PER_SEC: f64 = 0.8;
    pub const TIREDNESS_MOVING_PER_SEC: f64 = 2.4;
    pub const BOREDOM_PER_SEC: f64 = 1.6;
    pub const LONELY_PER_SEC: f64 = 1.0;
    pub const MATE_DESIRE_PER_SEC: f64 = 0.7;
    pub const NOURISH_DECAY_PER_SEC: f64 = 0.9;
    pub const STRESS_RISE_PER_SEC: f64 = 2.0;
    pub const STRESS_FALL_PER_SEC: f64 = 1.5;
    /// Satiety (Player.hunger, 0..100) lost per connected second, applied once a minute:
    /// full to empty in two hours.
    pub const SATIETY_DECAY_PER_SEC: f64 = 100. / 7200.;
    /// Mood points per second that transient moods fall back toward rest.
    pub const MOOD_DECAY_PER_SEC: f64 = 0.5;
    pub const FAVORITE_FRUIT_JOY: i32 = 10;
    pub const ILL_CHECK_SECONDS: f64 = 60.;
    pub const ILL_START_LEVEL: i8 = 30;
    pub const MATE_HAPPINESS: f64 = 50.;
    pub const MATE_DESIRE_READY: i16 = 5000;
    pub const BREED_OFFER_MS: f64 = 60_000.;
    pub const BREED_COOLDOWN_MS: f64 = 30. * 60_000.;
    pub const NEST_CAPACITY: usize = 3;
    pub const SOCIAL_RANGE: f64 = 4.;
    pub const GREET_COOLDOWN_MS: f64 = 10_000.;
    pub const GREET_LIKE: i32 = 4;
    pub const GREET_LONELY: i32 = -1500;
    pub const GREET_JOY: i32 = 5;
    /// Native AL_KNOWLEDGE keeps 20 Chao slots.
    pub const MAX_KNOWN_CHAO: usize = 20;
    pub const NATIVE_FRUIT_KINDS: u8 = 24;
}
use tuning::*;

pub const MOOD_NAMES: [&str; 8] = [
    "joy",
    "anger",
    "urgeToCry",
    "fear",
    "surprise",
    "dizziness",
    "relax",
    "total",
];
pub const NEED_NAMES: [&str; 11] = [
    "sleepiness",
    "tiredness",
    "hunger",
    "mateDesire",
    "boredom",
    "lonely",
    "tire",
    "stress",
    "nourish",
    "condition",
    "energy",
];
pub const PERSONALITY_NAMES: [&str; 13] = [
    "normalCuriosity",
    "kindness",
    "cryBabyEnergetic",
    "naiveNormal",
    "solitude",
    "vitality",
    "glutton",
    "regain",
    "skillful",
    "charm",
    "chatty",
    "normalCarefree",
    "fickle",
];
pub const ILLNESS_NAMES: [&str; 6] = [
    "cough",
    "cold",
    "rash",
    "runnyNose",
    "hiccups",
    "stomachAche",
];

/// AL_EMOTION `State[11]` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Sleepiness,
    Tiredness,
    Hunger,
    MateDesire,
    Boredom,
    Lonely,
    Tire,
    Stress,
    Nourish,
    Condition,
    Energy,
}
/// AL_EMOTION `Mood[8]` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Joy,
    Anger,
    UrgeToCry,
    Fear,
    Surprise,
    Dizziness,
    Relax,
    Total,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    Child,
    Cocoon,
    Adult,
    Dead,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CocoonKind {
    /// Child to adult. The evolution thread's body module picks the adult type.
    Evolve,
    /// Pink cocoon: end of life with enough happiness.
    Reincarnate,
    /// Grey cocoon: end of life without enough happiness.
    End,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cocoon {
    pub kind: CocoonKind,
    /// Compared with `LifeState::active_seconds`.
    pub until: f64,
    /// Stage to return to if an evolve cocoon is interrupted by a load.
    pub from_adult: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Emotion {
    pub mood: [i8; 8],
    /// Hunger (index 2) and Energy (index 10) are projections of Player.hunger and
    /// Player.energy; the stored values are refreshed on every read and save.
    pub state: [i16; 11],
    pub personality: [i8; 13],
    /// FavoriteFruit: native fruit index.
    pub taste: i8,
    pub tv: i8,
    pub music: i8,
    pub ill_state: [i8; 6],
    pub ill_clock: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Knowledge {
    pub name: String,
    pub like: u8,
    pub fear: i8,
    pub meet: u16,
    pub last_met_at: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Gene {
    pub mother_id: Option<String>,
    pub father_id: Option<String>,
    pub mother_name: Option<String>,
    pub father_name: Option<String>,
    /// MGroundMotherName, MGroundFatherName, FGroundMotherName, FGroundFatherName.
    pub grandparents: [Option<String>; 4],
    /// Abl: two grade alleles (0 = E .. 5 = S) per stat in ALL_STATS order.
    pub abl: [[u8; 2]; 7],
    pub life_time: [i8; 2],
    pub personality: [[i8; 13]; 2],
    pub taste: [i8; 2],
    pub tv: [i8; 2],
    pub music: [i8; 2],
    pub color: [u8; 2],
    pub monotone: [bool; 2],
    pub jewel: [u8; 2],
    pub shiny: [bool; 2],
    pub egg_color: [u8; 2],
}

/// The expressed look handed to the body module at hatch (body::GeneLook has the same fields).
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    pub color: u8,
    pub jewel: u8,
    pub shiny: bool,
    pub monotone: bool,
    pub egg_color: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Egg {
    pub id: String,
    pub gene: Gene,
    pub laid_at: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BreedOffer {
    pub partner_id: String,
    pub expires_at: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub request: Request,
    pub revision: u64,
    pub code: String,
    pub effects: Value,
}

/// Integration points for other server modules. Drained by [`apply_events`].
#[derive(Clone, Debug, PartialEq)]
pub enum LifeEvent {
    StatProgress { stat: String, amount: i32 },
    EvolutionDue,
    Evolved,
    Reincarnated,
    Hatched { look: Look },
    Died,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeState {
    pub version: u32,
    pub revision: u64,
    pub stage: Stage,
    pub cocoon: Option<Cocoon>,
    /// Connected seconds lived in the current life.
    pub active_seconds: f64,
    pub rollover_clock: f64,
    /// ClockRollovers.
    pub age: u16,
    /// AdultClockRollovers.
    pub adult_age: u16,
    /// Lifespan (counts down) and Lifespan2 (maximum).
    pub life: i16,
    pub life_max: i16,
    /// nbSucceed.
    pub reincarnations: u16,
    /// MMO lineage counter: increases when an egg hatches into this profile.
    pub generation: u32,
    /// CHAO_PARAM Exp, 0..=99, in ALL_STATS order.
    pub exp: [u8; 7],
    pub emotion: Emotion,
    pub knowledge: BTreeMap<String, Knowledge>,
    pub gene: Gene,
    pub nest: Vec<Egg>,
    pub breed_offer: Option<BreedOffer>,
    pub breed_ready_at: f64,
    pub seed: u64,
    pub rng: u64,
    pub receipts: BTreeMap<String, Receipt>,
    #[serde(skip)]
    pub events: Vec<LifeEvent>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    /// When false (default) a grey cocoon waits for happiness instead of ending the Chao.
    pub death: bool,
}
impl Policy {
    /// `CHAO_LIFE_DEATH` unset or `disabled` keeps death off; `enabled` turns it on.
    pub fn from_env() -> Result<Self, String> {
        match std::env::var("CHAO_LIFE_DEATH").ok().as_deref() {
            None | Some("disabled") => Ok(Self { death: false }),
            Some("enabled") => Ok(Self { death: true }),
            Some(_) => Err("CHAO_LIFE_DEATH must be enabled or disabled".into()),
        }
    }
}

fn hash_u64(parts: &[&[u8]]) -> u64 {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    let digest = hasher.finalize();
    u64::from_le_bytes(digest[..8].try_into().expect("eight bytes"))
}

impl LifeState {
    fn roll(&mut self, below: u64) -> u64 {
        let value = hash_u64(&[&self.seed.to_le_bytes(), &self.rng.to_le_bytes()]);
        self.rng = self.rng.wrapping_add(1);
        if below == 0 { 0 } else { value % below }
    }
    fn roll_signed(&mut self, magnitude: i8) -> i8 {
        let span = u64::from(magnitude.unsigned_abs()) * 2 + 1;
        (self.roll(span) as i64 - i64::from(magnitude)) as i8
    }

    /// A random first-generation gene. Grades seed from the profile's current grades.
    fn random_gene(&mut self, grades: [u8; 7]) -> Gene {
        let mut personality = [[0i8; 13]; 2];
        for allele in &mut personality {
            for value in allele.iter_mut() {
                *value = self.roll_signed(50);
            }
        }
        let fruit = NATIVE_FRUIT_KINDS as u64;
        Gene {
            mother_id: None,
            father_id: None,
            mother_name: None,
            father_name: None,
            grandparents: [None, None, None, None],
            abl: grades.map(|g| [g.min(5), g.min(5)]),
            life_time: [self.roll_signed(20), self.roll_signed(20)],
            personality,
            taste: [self.roll(fruit) as i8, self.roll(fruit) as i8],
            tv: [self.roll_signed(50), self.roll_signed(50)],
            music: [self.roll_signed(50), self.roll_signed(50)],
            color: [0, 0],
            monotone: [false, false],
            jewel: [0, 0],
            shiny: [false, false],
            egg_color: [0, 0],
        }
    }

    fn fresh(seed: u64, grades: [u8; 7], stage: Stage) -> Self {
        let mut state = Self {
            version: VERSION,
            revision: 0,
            stage,
            cocoon: None,
            active_seconds: 0.,
            rollover_clock: 0.,
            age: 0,
            adult_age: 0,
            life: BASE_LIFE,
            life_max: BASE_LIFE,
            reincarnations: 0,
            generation: 1,
            exp: [0; 7],
            emotion: Emotion {
                mood: [0; 8],
                state: [0; 11],
                personality: [0; 13],
                taste: 0,
                tv: 0,
                music: 0,
                ill_state: [0; 6],
                ill_clock: 0.,
            },
            knowledge: BTreeMap::new(),
            gene: Gene {
                mother_id: None,
                father_id: None,
                mother_name: None,
                father_name: None,
                grandparents: [None, None, None, None],
                abl: [[0; 2]; 7],
                life_time: [0; 2],
                personality: [[0; 13]; 2],
                taste: [0; 2],
                tv: [0; 2],
                music: [0; 2],
                color: [0; 2],
                monotone: [false; 2],
                jewel: [0; 2],
                shiny: [false; 2],
                egg_color: [0; 2],
            },
            nest: vec![],
            breed_offer: None,
            breed_ready_at: 0.,
            seed,
            rng: 0,
            receipts: BTreeMap::new(),
            events: vec![],
        };
        state.gene = state.random_gene(grades);
        state.express_gene();
        state
    }

    /// Personality, taste, tv, music and lifespan expressed from both alleles (MMO tuning:
    /// the mean of the two alleles; native dominance is unverified).
    fn express_gene(&mut self) {
        let g = &self.gene;
        for i in 0..13 {
            self.emotion.personality[i] =
                ((i16::from(g.personality[0][i]) + i16::from(g.personality[1][i])) / 2) as i8;
        }
        self.emotion.taste = g.taste[0];
        self.emotion.tv = ((i16::from(g.tv[0]) + i16::from(g.tv[1])) / 2) as i8;
        self.emotion.music = ((i16::from(g.music[0]) + i16::from(g.music[1])) / 2) as i8;
        self.life_max = BASE_LIFE + (i16::from(g.life_time[0]) + i16::from(g.life_time[1])) / 2;
        self.life = self.life_max;
    }

    /// Legacy profiles: hero/dark/chaos forms are already adults; everything else is a child.
    pub fn legacy(id: &str, form: &str, grades: [u8; 7]) -> Self {
        let seed = hash_u64(&[b"chao-life-v1", id.as_bytes()]);
        let stage = if form == "normal" {
            Stage::Child
        } else {
            Stage::Adult
        };
        let mut state = Self::fresh(seed, grades, stage);
        if stage == Stage::Adult {
            state.age = CHILD_ROLLOVERS;
        }
        state
    }

    pub fn restore(
        value: Option<&Value>,
        id: &str,
        form: &str,
        grades: [u8; 7],
    ) -> Result<Self, String> {
        let Some(value) = value.filter(|v| !v.is_null()) else {
            return Ok(Self::legacy(id, form, grades));
        };
        let state: Self =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid saved life state")?;
        if !state.valid() {
            return Err("Invalid or unsupported saved life state".into());
        }
        Ok(state)
    }

    pub fn valid(&self) -> bool {
        let finite = |x: f64| x.is_finite() && x >= 0.;
        let name_ok = |s: &Option<String>| s.as_ref().is_none_or(|s| s.len() <= 64);
        self.version == VERSION
            && finite(self.active_seconds)
            && finite(self.rollover_clock)
            && finite(self.breed_ready_at)
            && finite(self.emotion.ill_clock)
            && self.life_max > 0
            && self.life <= self.life_max
            && self.exp.iter().all(|e| *e < 100)
            && self.emotion.mood.iter().all(|m| (0..=MOOD_MAX).contains(m))
            && self
                .emotion
                .state
                .iter()
                .all(|s| (0..=NEED_MAX).contains(s))
            && self.emotion.ill_state.iter().all(|s| (0..=100).contains(s))
            && (0..NATIVE_FRUIT_KINDS as i8).contains(&self.emotion.taste)
            && self.gene.abl.iter().flatten().all(|g| *g <= 5)
            && self
                .gene
                .taste
                .iter()
                .all(|t| (0..NATIVE_FRUIT_KINDS as i8).contains(t))
            && name_ok(&self.gene.mother_name)
            && name_ok(&self.gene.father_name)
            && self.gene.grandparents.iter().all(name_ok)
            && self.knowledge.len() <= MAX_KNOWN_CHAO
            && self.knowledge.iter().all(|(k, v)| {
                !k.is_empty() && k.len() <= 128 && v.name.len() <= 64 && finite(v.last_met_at)
            })
            && self.nest.len() <= NEST_CAPACITY
            && self.nest.iter().all(|egg| {
                Uuid::parse_str(&egg.id).is_ok()
                    && finite(egg.laid_at)
                    && egg.gene.abl.iter().flatten().all(|g| *g <= 5)
            })
            && self.receipts.len() <= MAX_RECEIPTS
            && self
                .receipts
                .iter()
                .all(|(id, r)| id == &r.request.request_id && r.revision <= self.revision)
            && match (self.stage, self.cocoon) {
                (Stage::Cocoon, Some(c)) => c.until.is_finite(),
                (Stage::Cocoon, None) => false,
                (_, Some(_)) => false,
                _ => true,
            }
            && self.breed_offer.as_ref().is_none_or(|o| {
                !o.partner_id.is_empty() && o.partner_id.len() <= 128 && o.expires_at.is_finite()
            })
    }

    pub fn immobile(&self) -> bool {
        matches!(self.stage, Stage::Cocoon | Stage::Dead)
    }
}

fn grades_of(p: &Player) -> [u8; 7] {
    ALL_STATS.map(|name| p.stats.get(name).map_or(0, |s| s.grade.min(5) as u8))
}
pub fn restore_for(
    data: &Value,
    id: &str,
    form: &str,
    stats: &BTreeMap<String, crate::game::Stat>,
) -> Result<LifeState, String> {
    let grades = ALL_STATS.map(|name| stats.get(name).map_or(0, |s| s.grade.min(5) as u8));
    LifeState::restore(data.get("life"), id, form, grades)
}

// ---------- Needs and mood ----------

fn need_index(need: Need) -> usize {
    need as usize
}
fn hunger_state(p: &Player) -> i16 {
    ((100. - p.hunger.clamp(0., 100.)) * 100.).round() as i16
}
fn energy_state(p: &Player) -> i16 {
    (p.energy.clamp(0., 100.) * 100.).round() as i16
}
fn need_value(p: &Player, need: Need) -> i16 {
    match need {
        Need::Hunger => hunger_state(p),
        Need::Energy => energy_state(p),
        _ => p.life.emotion.state[need_index(need)],
    }
}
/// Apply a signed change to one need, clamped to 0..=10000. Returns the applied change.
pub fn adjust_need(p: &mut Player, need: Need, delta: i32) -> i32 {
    let before = i32::from(need_value(p, need));
    let after = (before + delta).clamp(0, i32::from(NEED_MAX));
    match need {
        Need::Hunger => p.hunger = 100. - f64::from(after) / 100.,
        Need::Energy => p.energy = f64::from(after) / 100.,
        _ => p.life.emotion.state[need_index(need)] = after as i16,
    }
    after - before
}
fn adjust_need_f(p: &mut Player, need: Need, delta: f64) {
    let index = need_index(need);
    let value = (f64::from(p.life.emotion.state[index]) + delta).clamp(0., f64::from(NEED_MAX));
    p.life.emotion.state[index] = value.round() as i16;
}
/// Apply a signed change to one mood, clamped to 0..=100. Returns the applied change.
pub fn adjust_mood(p: &mut Player, mood: Mood, delta: i32) -> i32 {
    let slot = &mut p.life.emotion.mood[mood as usize];
    let before = i32::from(*slot);
    let after = (before + delta).clamp(0, i32::from(MOOD_MAX));
    *slot = after as i8;
    after - before
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeedsView {
    pub sleepiness: i16,
    pub tiredness: i16,
    pub hunger: i16,
    pub mate_desire: i16,
    pub boredom: i16,
    pub lonely: i16,
    pub tire: i16,
    pub stress: i16,
    pub nourish: i16,
    pub condition: i16,
    pub energy: i16,
    pub joy: i8,
    pub anger: i8,
    pub urge_to_cry: i8,
    pub fear: i8,
    pub surprise: i8,
    pub dizziness: i8,
    pub relax: i8,
    pub total: i8,
    pub personality: [i8; 13],
    pub stage: Stage,
    pub ill: bool,
}
/// Read-only needs for behaviour selection. Needs are 0..=10000, moods 0..=100, personality
/// -100..=100. Higher always means more of the named thing (hunger 10000 = starving).
pub fn needs_view(p: &Player) -> NeedsView {
    let s = &p.life.emotion.state;
    let m = &p.life.emotion.mood;
    NeedsView {
        sleepiness: s[0],
        tiredness: s[1],
        hunger: hunger_state(p),
        mate_desire: s[3],
        boredom: s[4],
        lonely: s[5],
        tire: s[6],
        stress: s[7],
        nourish: s[8],
        condition: s[9],
        energy: energy_state(p),
        joy: m[0],
        anger: m[1],
        urge_to_cry: m[2],
        fear: m[3],
        surprise: m[4],
        dizziness: m[5],
        relax: m[6],
        total: m[7],
        personality: p.life.emotion.personality,
        stage: p.life.stage,
        ill: is_ill(p),
    }
}
pub fn is_ill(p: &Player) -> bool {
    p.life.emotion.ill_state.iter().any(|level| *level > 0)
}

#[derive(Clone, Copy, Debug)]
pub struct Food {
    /// Native fruit index, used for taste/FavoriteFruit. None for non-fruit food.
    pub fruit: Option<u8>,
    /// State units: lowers hunger and raises nourish by this much.
    pub nourish: i16,
    /// Mood units added to joy.
    pub joy: i8,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FeedResult {
    pub hunger: i32,
    pub nourish: i32,
    pub joy: i32,
    pub favorite: bool,
}
pub fn feed(p: &mut Player, food: Food) -> FeedResult {
    let nourish = i32::from(food.nourish.clamp(0, NEED_MAX));
    let favorite = food
        .fruit
        .is_some_and(|fruit| i16::from(fruit) == i16::from(p.life.emotion.taste));
    let joy = i32::from(food.joy.max(0)) + if favorite { FAVORITE_FRUIT_JOY } else { 0 };
    FeedResult {
        hunger: adjust_need(p, Need::Hunger, -nourish),
        nourish: adjust_need(p, Need::Nourish, nourish),
        joy: adjust_mood(p, Mood::Joy, joy),
        favorite,
    }
}

// ---------- Stat growth ----------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StatProgress {
    pub exp_delta: i32,
    pub levels: u16,
    pub points: i32,
}
/// Add native-shaped Exp to one stat. 100 Exp is one level. Each level adds
/// `GRADE_POINT_BASE[grade] + 0..=3 + level / 2` points (MMO tuning), capped at 9999.
/// A negative amount removes points only. Owner perfect stats never change.
pub fn grant_stat_progress(p: &mut Player, stat: &str, amount: i32) -> StatProgress {
    let Some(index) = ALL_STATS.iter().position(|s| *s == stat) else {
        return StatProgress::default();
    };
    if p.admin || amount == 0 {
        return StatProgress::default();
    }
    let life = &mut p.life;
    let entry = p.stats.get_mut(stat).expect("seven canonical stats");
    let mut result = StatProgress::default();
    if amount < 0 {
        let before = entry.points as i32;
        entry.points = (before + amount).max(0) as u32;
        result.points = entry.points as i32 - before;
    } else {
        let before_exp = i32::from(life.exp[index]);
        let mut exp = before_exp + amount;
        let before_points = entry.points as i32;
        while exp >= 100 && entry.level < 99 {
            exp -= 100;
            entry.level += 1;
            result.levels += 1;
            let grade = usize::from(entry.grade.min(5));
            let gain = GRADE_POINT_BASE[grade] + life.roll(4) as u32 + u32::from(entry.level) / 2;
            entry.points = (entry.points + gain).min(9999);
        }
        let exp = exp.min(99);
        life.exp[index] = exp as u8;
        result.exp_delta = exp - before_exp;
        result.points = entry.points as i32 - before_points;
    }
    life.events.push(LifeEvent::StatProgress {
        stat: stat.into(),
        amount,
    });
    result
}

// ---------- Life cycle ----------

fn immortal(p: &Player) -> bool {
    // Chaos Chao do not age out (widely documented for the original; unverified trace).
    p.form == "chaos"
}

/// Advance one connected Chao. Residents and offline profiles do not age (MMO policy).
pub fn tick(p: &mut Player, dt: f64, policy: Policy) {
    if !dt.is_finite() || dt <= 0. || p.life.stage == Stage::Dead {
        return;
    }
    p.life.active_seconds += dt;
    tick_needs(p, dt);
    tick_illness(p, dt);
    p.life.rollover_clock += dt;
    while p.life.rollover_clock >= ROLLOVER_SECONDS {
        p.life.rollover_clock -= ROLLOVER_SECONDS;
        rollover(p);
    }
    tick_stage(p, policy);
}

fn tick_needs(p: &mut Player, dt: f64) {
    let personality = p.life.emotion.personality;
    let scale =
        |trait_index: usize, sign: f64| 1. + sign * f64::from(personality[trait_index]) / 200.;
    let moving = p.input.dx != 0. || p.input.dz != 0.;
    adjust_need_f(p, Need::Sleepiness, SLEEPINESS_PER_SEC * dt);
    let tired = if moving {
        TIREDNESS_MOVING_PER_SEC
    } else {
        TIREDNESS_PER_SEC
    };
    adjust_need_f(p, Need::Tiredness, tired * scale(5, -1.) * dt);
    adjust_need_f(p, Need::Tire, tired * 0.5 * dt);
    adjust_need_f(p, Need::Boredom, BOREDOM_PER_SEC * scale(0, 1.) * dt);
    adjust_need_f(p, Need::Lonely, LONELY_PER_SEC * scale(4, -1.) * dt);
    if p.life.stage == Stage::Adult {
        adjust_need_f(p, Need::MateDesire, MATE_DESIRE_PER_SEC * dt);
    }
    adjust_need_f(p, Need::Nourish, -NOURISH_DECAY_PER_SEC * dt);
    let hunger = hunger_state(p);
    let s = p.life.emotion.state;
    let pressured = hunger > 7000 || s[0] > 8000 || s[1] > 8000 || s[5] > 8000 || is_ill(p);
    adjust_need_f(
        p,
        Need::Stress,
        if pressured {
            STRESS_RISE_PER_SEC * dt
        } else {
            -STRESS_FALL_PER_SEC * dt
        },
    );
    let s = p.life.emotion.state;
    let condition = 10000 - (i32::from(s[7]) + i32::from(hunger) + (10000 - i32::from(s[8]))) / 3;
    p.life.emotion.state[9] = condition.clamp(0, 10000) as i16;
    // Transient moods relax toward zero; joy and relax track wellbeing.
    let decay = MOOD_DECAY_PER_SEC * dt;
    let condition = f64::from(p.life.emotion.state[9]) / 100.;
    let stress = f64::from(p.life.emotion.state[7]) / 100.;
    let toward = |value: i8, target: f64| -> i8 {
        let v = f64::from(value);
        let next = if v < target {
            (v + decay).min(target)
        } else {
            (v - decay).max(target)
        };
        next.round().clamp(0., 100.) as i8
    };
    let m = &mut p.life.emotion.mood;
    m[0] = toward(m[0], (p.happiness * 0.5 + condition * 0.5).clamp(0., 100.));
    for mood in &mut m[2..=5] {
        *mood = toward(*mood, 0.);
    }
    m[1] = toward(m[1], if stress > 70. { 30. } else { 0. });
    m[6] = toward(m[6], (100. - stress).clamp(0., 100.));
    let total = (i16::from(m[0]) + i16::from(m[6])) / 2
        - (i16::from(m[1]) + i16::from(m[2]) + i16::from(m[3])) / 3;
    m[7] = total.clamp(0, 100) as i8;
    if hunger > 9000 || is_ill(p) {
        p.happiness = (p.happiness - 0.01 * dt).max(0.);
    }
}

fn tick_illness(p: &mut Player, dt: f64) {
    p.life.emotion.ill_clock += dt;
    while p.life.emotion.ill_clock >= ILL_CHECK_SECONDS {
        p.life.emotion.ill_clock -= ILL_CHECK_SECONDS;
        let condition = p.life.emotion.state[9];
        let stress = p.life.emotion.state[7];
        let glutton = 1. + f64::from(p.life.emotion.personality[6]) / 200.;
        // Satiety falls in once-a-minute steps so short interactions stay exact.
        p.hunger = (p.hunger - SATIETY_DECAY_PER_SEC * ILL_CHECK_SECONDS * glutton).max(0.);
        let regain = i32::from(p.life.emotion.personality[7]);
        for level in &mut p.life.emotion.ill_state {
            if *level > 0 {
                let change: i32 = if condition > 6000 {
                    -10 - regain / 20
                } else {
                    5
                };
                *level = (i32::from(*level) + change).clamp(0, 100) as i8;
            }
        }
        if condition < 3000 && !is_ill(p) {
            // Chance per check rises with stress: up to 1 in 4.
            let chance = 1 + u64::from(stress.max(0) as u16) / 2500;
            if p.life.roll(16) < chance {
                let slot = p.life.roll(6) as usize;
                p.life.emotion.ill_state[slot] = ILL_START_LEVEL;
            }
        }
    }
}

fn rollover(p: &mut Player) {
    let life = &mut p.life;
    life.age = life.age.saturating_add(1);
    if life.stage == Stage::Adult {
        life.adult_age = life.adult_age.saturating_add(1);
        if p.form != "chaos" {
            let extra = i16::from(is_ill(p) || p.happiness < 20.);
            p.life.life = (p.life.life - 1 - extra).max(0);
        }
    }
}

fn start_cocoon(p: &mut Player, kind: CocoonKind) {
    p.life.cocoon = Some(Cocoon {
        kind,
        until: p.life.active_seconds + COCOON_SECONDS,
        from_adult: p.life.stage == Stage::Adult,
    });
    p.life.stage = Stage::Cocoon;
    p.life.breed_offer = None;
}

fn tick_stage(p: &mut Player, policy: Policy) {
    match p.life.stage {
        Stage::Child if p.life.age >= CHILD_ROLLOVERS => {
            start_cocoon(p, CocoonKind::Evolve);
            p.life.events.push(LifeEvent::EvolutionDue);
        }
        Stage::Adult if p.life.life <= 0 && !immortal(p) => {
            let kind = if p.happiness >= REINCARNATE_HAPPINESS {
                CocoonKind::Reincarnate
            } else {
                CocoonKind::End
            };
            start_cocoon(p, kind);
        }
        Stage::Cocoon => {
            let cocoon = p.life.cocoon.expect("cocoon stage has a cocoon");
            if p.life.active_seconds < cocoon.until {
                return;
            }
            match cocoon.kind {
                CocoonKind::Evolve => {
                    p.life.cocoon = None;
                    p.life.stage = Stage::Adult;
                    p.life.events.push(LifeEvent::Evolved);
                }
                CocoonKind::Reincarnate => reincarnate(p),
                CocoonKind::End if p.happiness >= REINCARNATE_HAPPINESS => reincarnate(p),
                CocoonKind::End if policy.death => {
                    p.life.cocoon = None;
                    p.life.stage = Stage::Dead;
                    p.life.events.push(LifeEvent::Died);
                }
                CocoonKind::End => {}
            }
        }
        _ => {}
    }
}

fn reset_needs(p: &mut Player) {
    let e = &mut p.life.emotion;
    e.mood = [0; 8];
    e.state = [0; 11];
    e.state[9] = NEED_MAX;
    e.ill_state = [0; 6];
    e.ill_clock = 0.;
    p.hunger = 100.;
    p.energy = 100.;
}

fn new_life(p: &mut Player) {
    let life = &mut p.life;
    life.stage = Stage::Child;
    life.cocoon = None;
    life.active_seconds = 0.;
    life.rollover_clock = 0.;
    life.age = 0;
    life.adult_age = 0;
    life.exp = [0; 7];
    life.breed_offer = None;
    reset_needs(p);
}

/// Pink cocoon outcome: same Chao, same gene, personality and knowledge, stats reset.
pub fn reincarnate(p: &mut Player) {
    new_life(p);
    p.life.reincarnations = p.life.reincarnations.saturating_add(1);
    p.life.life = p.life.life_max;
    if !p.admin {
        for stat in p.stats.values_mut() {
            stat.level = 0;
            stat.points /= REINCARNATE_POINT_DIVISOR;
        }
    }
    p.life.events.push(LifeEvent::Reincarnated);
}

fn look_of(life: &mut LifeState) -> Look {
    let pick = |life: &mut LifeState| life.roll(2) as usize;
    let (a, b, c, d, e) = (pick(life), pick(life), pick(life), pick(life), pick(life));
    let g = &life.gene;
    Look {
        color: g.color[a],
        jewel: g.jewel[b],
        shiny: g.shiny[c],
        monotone: g.monotone[d],
        egg_color: g.egg_color[e],
    }
}

/// Hatch an egg into this profile as a new generation (MMO lineage rule). The higher grade
/// allele is expressed (tuning; native dominance unverified).
fn hatch_gene(p: &mut Player, gene: Gene, seed: u64) {
    new_life(p);
    p.life.gene = gene;
    p.life.seed = seed;
    p.life.rng = 0;
    p.life.generation = p.life.generation.saturating_add(1);
    p.life.reincarnations = 0;
    p.life.knowledge.clear();
    p.life.express_gene();
    if !p.admin {
        for (index, name) in ALL_STATS.iter().enumerate() {
            let grade = p.life.gene.abl[index].into_iter().max().unwrap_or(0);
            let stat = p.stats.get_mut(*name).expect("seven canonical stats");
            stat.level = 0;
            stat.points = 0;
            stat.grade = u16::from(grade);
        }
    }
    let look = look_of(&mut p.life);
    p.life.events.push(LifeEvent::Hatched { look });
}

/// One child allele per parent, each chosen at random from that parent's pair.
pub fn cross(
    mother: &mut LifeState,
    father: &LifeState,
    mother_name: &str,
    father_name: &str,
    mother_id: &str,
    father_id: &str,
) -> Gene {
    let m = mother.gene.clone();
    let f = &father.gene;
    let mut abl = [[0u8; 2]; 7];
    for (i, slot) in abl.iter_mut().enumerate() {
        let a = mother.roll(2) as usize;
        let b = mother.roll(2) as usize;
        *slot = [m.abl[i][a], f.abl[i][b]];
    }
    let mut personality = [[0i8; 13]; 2];
    let [left, right] = &mut personality;
    for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
        *l = m.personality[mother.roll(2) as usize][i];
        *r = f.personality[mother.roll(2) as usize][i];
    }
    let mut two =
        |mv: [i8; 2], fv: [i8; 2]| [mv[mother.roll(2) as usize], fv[mother.roll(2) as usize]];
    let life_time = two(m.life_time, f.life_time);
    let taste = two(m.taste, f.taste);
    let tv = two(m.tv, f.tv);
    let music = two(m.music, f.music);
    let mut two_u =
        |mv: [u8; 2], fv: [u8; 2]| [mv[mother.roll(2) as usize], fv[mother.roll(2) as usize]];
    let color = two_u(m.color, f.color);
    let jewel = two_u(m.jewel, f.jewel);
    let egg_color = two_u(m.egg_color, f.egg_color);
    let mut two_b =
        |mv: [bool; 2], fv: [bool; 2]| [mv[mother.roll(2) as usize], fv[mother.roll(2) as usize]];
    let monotone = two_b(m.monotone, f.monotone);
    let shiny = two_b(m.shiny, f.shiny);
    Gene {
        mother_id: Some(mother_id.into()),
        father_id: Some(father_id.into()),
        mother_name: Some(mother_name.chars().take(64).collect()),
        father_name: Some(father_name.chars().take(64).collect()),
        grandparents: [
            m.mother_name.clone(),
            m.father_name.clone(),
            f.mother_name.clone(),
            f.father_name.clone(),
        ],
        abl,
        life_time,
        personality,
        taste,
        tv,
        music,
        color,
        monotone,
        jewel,
        shiny,
        egg_color,
    }
}

// ---------- Intents ----------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Intent {
    Greet {
        #[serde(rename = "partnerId")]
        partner_id: String,
    },
    Breed {
        #[serde(rename = "partnerId")]
        partner_id: String,
    },
    Hatch {
        #[serde(rename = "eggId", default, skip_serializing_if = "Option::is_none")]
        egg_id: Option<String>,
    },
    ReleaseEgg {
        #[serde(rename = "eggId")]
        egg_id: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    #[serde(rename = "type")]
    pub message_type: String,
    pub request_id: String,
    pub expected_revision: u64,
    pub intent: Intent,
}
fn valid_request(r: &Request) -> bool {
    let id_ok = |s: &str| !s.is_empty() && s.len() <= 128;
    r.message_type == "life"
        && Uuid::parse_str(&r.request_id)
            .is_ok_and(|id| id.hyphenated().to_string() == r.request_id)
        && match &r.intent {
            Intent::Greet { partner_id } | Intent::Breed { partner_id } => id_ok(partner_id),
            Intent::Hatch { egg_id } => egg_id.as_deref().is_none_or(id_ok),
            Intent::ReleaseEgg { egg_id } => id_ok(egg_id),
        }
}

/// The partner a request may also mutate (for transactional rollback by the service).
pub fn partner_of(message: &Value) -> Option<String> {
    message["intent"]["partnerId"]
        .as_str()
        .filter(|s| s.len() <= 128)
        .map(String::from)
}

fn result(
    request_id: &str,
    accepted: bool,
    replayed: bool,
    code: &str,
    revision: u64,
    effects: Value,
) -> Value {
    json!({"type":"event","kind":"life-result","requestId":request_id,"accepted":accepted,
        "replayed":replayed,"code":code,"revision":revision,"rulesVersion":RULES,"effects":effects})
}
pub fn failure(game: &Game, id: &str, message: &Value, code: &str) -> Value {
    let request_id = message["requestId"]
        .as_str()
        .filter(|s| s.len() <= 64)
        .unwrap_or_default();
    result(
        request_id,
        false,
        false,
        code,
        game.players.get(id).map_or(0, |p| p.life.revision),
        Value::Null,
    )
}

fn social_conflict(p: &Player) -> bool {
    p.connections == 0
        || crate::competition::reserved(p)
        || p.watch.is_some()
        || p.tiny_game.is_some()
        || p.race.is_some()
        || p.karate.is_some()
        || p.vehicle.is_some()
        || p.care.activity_lock.is_some()
        || !["garden", "hero", "dark", "hub"].contains(&p.zone.as_str())
}
fn in_range(a: &Player, b: &Player) -> bool {
    a.zone == b.zone && ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt() <= SOCIAL_RANGE
}
fn breed_ready(p: &Player, now: f64) -> bool {
    p.life.stage == Stage::Adult
        && p.happiness >= MATE_HAPPINESS
        && p.life.emotion.state[3] >= MATE_DESIRE_READY
        && now >= p.life.breed_ready_at
}

fn meet(p: &mut Player, other_id: &str, other_name: &str, now: f64) -> Value {
    let kindness = i32::from(p.life.emotion.personality[1]);
    let known = p.life.knowledge.contains_key(other_id);
    if !known && p.life.knowledge.len() >= MAX_KNOWN_CHAO {
        // Forget the least liked, then least recently met, acquaintance.
        if let Some(evict) = p
            .life
            .knowledge
            .iter()
            .min_by(|a, b| {
                a.1.like
                    .cmp(&b.1.like)
                    .then(a.1.last_met_at.total_cmp(&b.1.last_met_at))
            })
            .map(|(k, _)| k.clone())
        {
            p.life.knowledge.remove(&evict);
        }
    }
    let entry = p
        .life
        .knowledge
        .entry(other_id.into())
        .or_insert(Knowledge {
            name: String::new(),
            like: 0,
            fear: 0,
            meet: 0,
            last_met_at: now,
        });
    entry.name = other_name.chars().take(64).collect();
    entry.meet = entry.meet.saturating_add(1);
    let like = (i32::from(entry.like) + GREET_LIKE + kindness / 50).clamp(0, 255);
    entry.like = like as u8;
    entry.last_met_at = now;
    let like = entry.like;
    let lonely = adjust_need(p, Need::Lonely, GREET_LONELY);
    let joy = adjust_mood(p, Mood::Joy, GREET_JOY);
    json!({"like":like,"lonely":lonely,"joy":joy})
}

fn apply(game: &mut Game, id: &str, request: &Request) -> Result<(String, Value), &'static str> {
    let now = game.time;
    let me = game.players.get(id).ok_or("profile-unavailable")?;
    match &request.intent {
        Intent::Greet { partner_id } | Intent::Breed { partner_id } => {
            if partner_id == id {
                return Err("invalid-partner");
            }
            let partner = game.players.get(partner_id).ok_or("partner-unavailable")?;
            if social_conflict(me) {
                return Err("activity-conflict");
            }
            if social_conflict(partner) {
                return Err("partner-busy");
            }
            if me.life.immobile() || partner.life.immobile() {
                return Err("stage-conflict");
            }
            if !in_range(me, partner) {
                return Err("out-of-range");
            }
            let (my_name, partner_name) = (me.name.clone(), partner.name.clone());
            if let Intent::Greet { .. } = request.intent {
                if me
                    .life
                    .knowledge
                    .get(partner_id)
                    .is_some_and(|k| now < k.last_met_at + GREET_COOLDOWN_MS)
                {
                    return Err("cooldown");
                }
                let mut me = game.players.remove(id).expect("checked");
                let mine = meet(&mut me, partner_id, &partner_name, now);
                game.players.insert(id.into(), me);
                let partner = game.players.get_mut(partner_id).expect("checked");
                let theirs = meet(partner, id, &my_name, now);
                partner.life.revision = partner.life.revision.saturating_add(1);
                return Ok(("greeted".into(), json!({"self":mine,"partner":theirs})));
            }
            if !breed_ready(me, now) {
                return Err("not-ready");
            }
            if !breed_ready(partner, now) {
                return Err("partner-not-ready");
            }
            let accepted_offer = partner
                .life
                .breed_offer
                .as_ref()
                .is_some_and(|o| o.partner_id == id && o.expires_at > now);
            if !accepted_offer {
                let me = game.players.get_mut(id).expect("checked");
                me.life.breed_offer = Some(BreedOffer {
                    partner_id: partner_id.clone(),
                    expires_at: now + BREED_OFFER_MS,
                });
                game.events.push(crate::game::Event {
                    message: json!({"type":"event","kind":"life-breed-offer","fromId":id,"fromName":my_name,"expiresAt":now + BREED_OFFER_MS}),
                    target: Some(partner_id.clone()),
                });
                return Ok((
                    "offer-sent".into(),
                    json!({"offerExpiresAt":now + BREED_OFFER_MS}),
                ));
            }
            // The offerer is the mother and keeps the egg (MMO rule).
            if partner.life.nest.len() >= NEST_CAPACITY {
                return Err("partner-nest-full");
            }
            let father = me.life.clone();
            let mut mother = game.players.remove(partner_id).expect("checked");
            let gene = cross(
                &mut mother.life,
                &father,
                &partner_name,
                &my_name,
                partner_id,
                id,
            );
            let egg_id = Uuid::new_v4().hyphenated().to_string();
            mother.life.nest.push(Egg {
                id: egg_id.clone(),
                gene,
                laid_at: now,
            });
            mother.life.breed_offer = None;
            mother.life.breed_ready_at = now + BREED_COOLDOWN_MS;
            mother.life.emotion.state[3] = 0;
            mother.life.revision = mother.life.revision.saturating_add(1);
            game.players.insert(partner_id.clone(), mother);
            let me = game.players.get_mut(id).expect("checked");
            me.life.breed_offer = None;
            me.life.breed_ready_at = now + BREED_COOLDOWN_MS;
            me.life.emotion.state[3] = 0;
            game.events.push(crate::game::Event {
                message: json!({"type":"event","kind":"life-egg-laid","eggId":egg_id,"partnerId":id,"partnerName":my_name}),
                target: Some(partner_id.clone()),
            });
            Ok((
                "egg-laid".into(),
                json!({"eggId":egg_id,"keeperId":partner_id}),
            ))
        }
        Intent::Hatch { egg_id } => {
            let ending = me.life.stage == Stage::Dead
                || (me.life.stage == Stage::Cocoon
                    && me.life.cocoon.is_some_and(|c| c.kind == CocoonKind::End));
            if !ending {
                return Err("stage-conflict");
            }
            let mut p = game.players.remove(id).expect("checked");
            let (gene, seed) = match egg_id {
                Some(egg_id) => {
                    let Some(index) = p.life.nest.iter().position(|e| &e.id == egg_id) else {
                        game.players.insert(id.into(), p);
                        return Err("egg-unavailable");
                    };
                    let egg = p.life.nest.remove(index);
                    (egg.gene, hash_u64(&[b"egg", egg.id.as_bytes()]))
                }
                None if p.life.stage == Stage::Dead => {
                    let grades = grades_of(&p);
                    let seed = p.life.roll(u64::MAX);
                    let mut scratch = LifeState::fresh(seed, grades, Stage::Child);
                    (scratch.random_gene(grades), seed)
                }
                None => {
                    game.players.insert(id.into(), p);
                    return Err("egg-unavailable");
                }
            };
            hatch_gene(&mut p, gene, seed);
            let generation = p.life.generation;
            game.players.insert(id.into(), p);
            Ok(("hatched".into(), json!({"generation":generation})))
        }
        Intent::ReleaseEgg { egg_id } => {
            let p = game.players.get_mut(id).expect("checked");
            let before = p.life.nest.len();
            p.life.nest.retain(|e| &e.id != egg_id);
            if p.life.nest.len() == before {
                return Err("egg-unavailable");
            }
            Ok(("released".into(), json!({"eggId":egg_id})))
        }
    }
}

/// Strict `life` envelope. Duplicate request IDs replay; a reused ID with a different payload
/// is rejected; new requests must match the current revision.
pub fn action(game: &mut Game, id: &str, message: &Value) -> Value {
    let Ok(request) = serde_json::from_value::<Request>(message.clone()) else {
        return failure(game, id, message, "invalid-request");
    };
    if !valid_request(&request) {
        return failure(game, id, message, "invalid-request");
    }
    let Some(p) = game.players.get(id) else {
        return failure(game, id, message, "profile-unavailable");
    };
    if let Some(receipt) = p.life.receipts.get(&request.request_id) {
        return if receipt.request == request {
            result(
                &request.request_id,
                true,
                true,
                &receipt.code,
                receipt.revision,
                receipt.effects.clone(),
            )
        } else {
            failure(game, id, message, "request-id-conflict")
        };
    }
    if request.expected_revision != p.life.revision {
        return failure(game, id, message, "stale-revision");
    }
    if p.life.revision == u64::MAX {
        return failure(game, id, message, "revision-exhausted");
    }
    match apply(game, id, &request) {
        Ok((code, effects)) => {
            let p = game.players.get_mut(id).expect("profile exists");
            p.life.revision += 1;
            let revision = p.life.revision;
            if p.life.receipts.len() >= MAX_RECEIPTS
                && let Some(oldest) = p
                    .life
                    .receipts
                    .iter()
                    .min_by_key(|(_, r)| r.revision)
                    .map(|(k, _)| k.clone())
            {
                // Safe to evict: an evicted retry now fails the revision check instead of reapplying.
                p.life.receipts.remove(&oldest);
            }
            p.life.receipts.insert(
                request.request_id.clone(),
                Receipt {
                    request: request.clone(),
                    revision,
                    code: code.clone(),
                    effects: effects.clone(),
                },
            );
            game.dirty = true;
            apply_events(game, id);
            result(&request.request_id, true, false, &code, revision, effects)
        }
        Err(code) => failure(game, id, message, code),
    }
}

/// Drain life events into other modules. The evolution thread's body module hooks in here
/// (body::on_stat_progress, body::evolve, body::reincarnate, body::express).
pub fn apply_events(game: &mut Game, id: &str) {
    let Some(p) = game.players.get_mut(id) else {
        return;
    };
    let events = std::mem::take(&mut p.life.events);
    for event in events {
        let (kind, text) = match &event {
            LifeEvent::StatProgress { .. } => continue,
            LifeEvent::EvolutionDue => ("life-cocoon", "Your Chao is wrapped in a cocoon."),
            LifeEvent::Evolved => ("life-evolved", "Your Chao has grown into an adult."),
            LifeEvent::Reincarnated => (
                "life-reincarnated",
                "Your Chao was reborn from a pink cocoon.",
            ),
            LifeEvent::Hatched { .. } => ("life-hatched", "A new Chao hatched."),
            LifeEvent::Died => ("life-ended", "Your Chao's life has ended."),
        };
        game.dirty = true;
        game.events.push(crate::game::Event {
            message: json!({"type":"event","kind":kind,"text":text,"playerId":id}),
            target: Some(id.into()),
        });
    }
}

// ---------- Snapshots ----------

fn named<T: Serialize + Copy>(names: &[&str], values: &[T]) -> Value {
    Value::Object(
        names
            .iter()
            .zip(values)
            .map(|(n, v)| ((*n).into(), json!(v)))
            .collect(),
    )
}

/// Public per-player summary, safe to show to others in the room.
pub fn public_summary(p: &Player) -> Value {
    json!({"stage":p.life.stage,"cocoon":p.life.cocoon.map(|c|c.kind),"age":p.life.age,
        "reincarnations":p.life.reincarnations,"generation":p.life.generation,
        "joy":p.life.emotion.mood[0],"total":p.life.emotion.mood[7],"ill":is_ill(p)})
}

/// Owner-only `snapshot.life`.
pub fn public(game: &Game, id: &str) -> Value {
    let Some(p) = game.players.get(id) else {
        return Value::Null;
    };
    let l = &p.life;
    let mut state = l.emotion.state;
    state[2] = hunger_state(p);
    state[10] = energy_state(p);
    let exp: Value = Value::Object(
        ALL_STATS
            .iter()
            .zip(l.exp)
            .map(|(n, e)| ((*n).into(), json!(e)))
            .collect(),
    );
    let grades = Value::Object(
        ALL_STATS
            .iter()
            .zip(l.gene.abl)
            .map(|(n, a)| ((*n).into(), json!(a)))
            .collect(),
    );
    let now = game.time;
    json!({"rulesVersion":RULES,"revision":l.revision,"stage":l.stage,
        "cocoon":l.cocoon.map(|c|json!({"kind":c.kind,"remainingSeconds":(c.until-l.active_seconds).max(0.)})),
        "age":l.age,"adultAge":l.adult_age,"life":l.life,"lifeMax":l.life_max,"immortal":immortal(p),
        "reincarnations":l.reincarnations,"generation":l.generation,
        "rolloverSeconds":ROLLOVER_SECONDS,"childRollovers":CHILD_ROLLOVERS,
        "deathPolicy":if game.life_policy.death {"enabled"} else {"disabled"},
        "exp":exp,"statCurve":"adapted-grade-point-curve-v1",
        "mood":named(&MOOD_NAMES,&l.emotion.mood),"needs":named(&NEED_NAMES,&state),
        "personality":named(&PERSONALITY_NAMES,&l.emotion.personality),
        "taste":l.emotion.taste,"tv":l.emotion.tv,"music":l.emotion.music,
        "illness":named(&ILLNESS_NAMES,&l.emotion.ill_state),
        "knowledge":l.knowledge.iter().map(|(k,v)|json!({"playerId":k,"name":v.name,"like":v.like,"fear":v.fear,"meet":v.meet})).collect::<Vec<_>>(),
        "gene":{"motherName":l.gene.mother_name,"fatherName":l.gene.father_name,"grandparents":l.gene.grandparents,
            "gradeAlleles":grades,"lifeTime":l.gene.life_time},
        "nest":l.nest.iter().map(|e|json!({"id":e.id,"laidAt":e.laid_at,"motherName":e.gene.mother_name,"fatherName":e.gene.father_name,"eggColor":e.gene.egg_color[0]})).collect::<Vec<_>>(),
        "nestCapacity":NEST_CAPACITY,
        "breed":{"offer":l.breed_offer,"readyAt":l.breed_ready_at,"ready":breed_ready(p,now)},
        "eligibility":{"canSocialize":!social_conflict(p)&&!l.immobile(),
            "canHatch":l.stage==Stage::Dead||l.cocoon.is_some_and(|c|c.kind==CocoonKind::End)},
        "provenance":{"layout":"X-Hax SA2Structs.h CHAO_PARAM_GC, AL_EMOTION, AL_KNOWLEDGE, AL_GENE",
            "rates":"mmo-tuning-unverified","agingPolicy":"connected-time-only"}})
}

#[cfg(test)]
mod tests;
