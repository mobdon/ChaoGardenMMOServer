//! Chao life cycle, stat growth, needs, moods, illness, genes, owner knowledge and eggs.
//!
//! Ported from the GameCube SA2B (GSNE8P) `ChaoMain.rel` code, SHA-1
//! `e7e24b8b6f4305d95531c9d898d566ead79ddc8d`, traced read-only from the owner's own disc on
//! November 20, 2026. Addresses below are ChaoMain `.text` offsets; `CG+x` is a `ChaoGlobal`
//! field (`.data` 0x2A50). See `LIFE.md` for the evidence table and the remaining unknowns.
//! Server-only policies (connected-time aging, the death switch, consent and the egg nest) are
//! marked `MMO policy`; everything else is native.
use crate::game::{ALL_STATS, Game, Player};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const VERSION: u32 = 2;
pub const RULES: &str = "native-gsne8p-chaomain-v1";
pub const MAX_RECEIPTS: usize = 256;

/// Native constants. Each names its source.
pub mod native {
    /// One logic tick per NTSC video field (GSNE8P main 0x800D3AD0 waits in
    /// fn_80117DF8 with N = 1): 60000/1001 = 59.94 Hz.
    pub const FRAMES_PER_SECOND: f64 = 60000. / 1001.;
    /// `AL_ParameterControl` 0x20CCC: one of 32 slots every `ChaoTimerBaseInterval` 3 frames.
    pub const CALC_FRAMES: f64 = 96.;
    /// `AL_CalcParameter` 0x213CC timers fire when the old value >= interval (period = +1).
    pub const LIFE_INTERVAL: u16 = 120; // CG ChaoTimerLifeInterval
    pub const MOOD_INTERVAL: u16 = 7; // CG+0x1A0
    pub const NEED_INTERVAL: u16 = 10; // CG ChaoTimerEmotionInterval
    pub const ILL_INTERVAL: u16 = 80; // CG+0x1A4
    pub const KW_INTERVAL: u16 = 10; // CG ChaoTimerKnowledgeInterval
    pub const STATE_LIMIT: i16 = 10000; // EmotionStateLimit
    pub const MOOD_LIMIT: i16 = 200; // EmotionMoodLimit
    pub const PERSONALITY_LIMIT: i8 = 100; // EmotionPersonalityLimit
    // Needs per needs tick (CG+0x152..0x196).
    pub const ADD_SLEEPY: i32 = 90;
    pub const SLEEPY_TIRE_FACTOR: f32 = 0.00006; // .rodata 0x65F0
    pub const ADD_HUNGER: i32 = 70;
    pub const ADD_BREED: i32 = 12;
    pub const ADD_TEDIOUS: [i32; 3] = [60, 30, 30];
    pub const TEDIOUS_RESET: i16 = 6000;
    pub const ADD_LONELY: i32 = 60;
    pub const ADD_TIRE: i32 = 10;
    pub const SUB_NOURISHMENT: i32 = 36;
    pub const ADD_THIRSTY: i32 = 150;
    pub const CONDITION_HIGH_NOURISH: (i16, i32) = (9000, 80);
    pub const CONDITION_LOW_NOURISH: (i16, i32) = (1000, -120);
    pub const CONDITION_STRESS: (i16, i32) = (8000, -60);
    /// Mood pain > 100 (hard-coded) costs condition 300.
    pub const CONDITION_PAIN: (i16, i32) = (100, -300);
    pub const SUB_MOOD: i32 = 4; // CG+0x144..0x150
    /// Sleep (`fn_60_467E0`) and eating (`ALBHV_Eat`).
    pub const ADD_SLEEP_DEPTH: i32 = 1500;
    pub const SUB_SLEEPY: i32 = 1500;
    pub const SUB_SLEEP_DEPTH: i32 = 3500;
    pub const SUB_TIRE_SLEEP: i32 = 300;
    pub const WAKE_SLEEPY: i16 = 1000;
    pub const WAKE_SLEEP_DEPTH: i16 = 2000;
    pub const SUB_HUNGER_BITE: i32 = 2400;
    // Stat growth (`AL_ParameterGrow` 0x21148, `fn_60_2131C` 0x2131C).
    pub const LEVEL_BASE: u32 = 11;
    pub const LEVEL_PER_GRADE: u32 = 3;
    pub const LEVEL_RANDOM: f64 = 4.9999;
    pub const LEVEL_SKILL_CAP: u32 = 4000;
    pub const MAX_LEVEL: u16 = 99;
    pub const MAX_GRADE: u8 = 5;
    pub const FRUIT_BITES: u8 = 4;
    pub const FRUIT_ADD_LIFE: i32 = 3; // CG+0xB8
    pub const FRUIT_ADD_LIFE_MAX_CHILD: i32 = 5; // CG+0xBC
    pub const LIFE_MAX_LIMIT: i32 = 9999;
    // Lifespan (`AL_GeneAnalyzeCommon` 0x1002C, `AL_CalcParameter`).
    pub const INIT_BASE_LIFE: i32 = 3000;
    pub const INIT_MUL_LIFE: i32 = 100;
    pub const LIFE_DECAY_OFFSET: i32 = 100;
    pub const LIFE_DECAY_DIVISOR: i32 = 15;
    // End of life (`fn_60_3ADE0` 0x3ADE0, `ALBHV_Dead` 0x3AABC).
    pub const SUCCEED_USER_LIKE: i16 = 50;
    pub const SUCCEED_STRESS: i16 = 700;
    pub const MAYU_WAIT: f64 = 200.;
    pub const MAYU_FADE_IN: f64 = 1800.;
    pub const MAYU_CHANGE: f64 = 900.;
    pub const MAYU_FADE_OUT: f64 = 900.;
    pub const MAYU_RELAX: f64 = 100.;
    pub const DEAD_TAIL: f64 = 930.;
    pub const SUCCEED_SKILL_CAP: u32 = 5000;
    pub const SUCCEED_SKILL_DIVISOR: u32 = 10;
    // Illness (life tick 0x2153C; illness tick 0x21C68).
    pub const ILL_CHANCE: f64 = 1. / 6000.;
    pub const ILL_SLOTS: f64 = 5.9999;
    pub const ILL_NEVER_RANDOM: usize = 3;
    pub const ILL_START: i8 = -100;
    pub const IMMUNE_START: i8 = 100;
    // Breeding (`fn_60_15214`, `fn_60_150FC`, `fn_60_40130`).
    pub const BREED_INVITE: i16 = 10000;
    pub const BREED_ANSWER: i16 = 5000;
    pub const GARDEN_CHAO_LIMIT: usize = 8;
    // Knowledge and happiness (`AL_KW_*`, `AL_ParameterAddUserLike` 0x20D7C).
    pub const USER_LIKE_LIMIT: i16 = 100;
    pub const FEAR_LIMIT: i16 = 200;
    pub const KW_SUB_FEAR: i32 = 10;
    pub const KW_ADD_DISTANCE: u16 = 16;
    pub const KW_DISTANCE_LIMIT: u16 = 50000;
    pub const PET_USER_LIKE: i16 = 1;
    pub const FRUIT_USER_LIKE: i16 = 1;
    pub const ANIMAL_USER_LIKE: i16 = 1;
    // Genes (`AL_GeneCreate` 0x1218C, `AL_GeneAnalyzeCommon` 0x1002C).
    pub const GRADE_HIGH_CHANCE: f64 = 0.7;
}
use native::*;

/// MMO policy switches. The defaults match the November 20, 2026 decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    /// `CHAO_LIFE_DEATH=enabled`: a failed end-of-life check ends the Chao (native `KillChao`).
    /// Off (default): the grey cocoon waits until the native pink check passes.
    pub death: bool,
    /// Resident (Watch) Chao keep running needs, moods and illness recovery while offline.
    /// Aging and the life tick always need a connected owner.
    pub resident_needs: bool,
    /// Copy native bugs: `AL_EmotionStateInit` writes the wrong State slots and rebirth
    /// clobbers taste/tv/music.
    pub native_bugs: bool,
    /// Native `fn_60_15214`: no mate call when the garden holds 8 or more Chao.
    pub garden_limit: bool,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            death: false,
            resident_needs: false,
            native_bugs: false,
            garden_limit: true,
        }
    }
}
impl Policy {
    /// `CHAO_LIFE_DEATH` unset or `disabled` keeps death off; `enabled` turns it on.
    pub fn from_env() -> Result<Self, String> {
        let death = match std::env::var("CHAO_LIFE_DEATH").ok().as_deref() {
            None | Some("disabled") => false,
            Some("enabled") => true,
            Some(_) => return Err("CHAO_LIFE_DEATH must be enabled or disabled".into()),
        };
        Ok(Self {
            death,
            ..Self::default()
        })
    }
}

pub const MOOD_NAMES: [&str; 8] = [
    "pleasure", "anger", "sorrow", "fear", "surprise", "pain", "relax", "total",
];
pub const NEED_NAMES: [&str; 11] = [
    "sleepy",
    "sleepDepth",
    "hunger",
    "breed",
    "tedious",
    "lonely",
    "tire",
    "stress",
    "nourishment",
    "condition",
    "thirsty",
];
pub const PERSONALITY_NAMES: [&str; 13] = [
    "curiosity",
    "kindness",
    "aggressive",
    "sleepyHead",
    "solitude",
    "vitality",
    "glutton",
    "regain",
    "skillful",
    "charm",
    "chatty",
    "calm",
    "fickle",
];
pub const ILLNESS_NAMES: [&str; 6] = [
    "cough",
    "sneeze",
    "itch",
    "runnyNose",
    "hiccups",
    "stomachAche",
];

/// AL_EMOTION `State[11]` (enum 8..18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Sleepy,
    SleepDepth,
    Hunger,
    Breed,
    Tedious,
    Lonely,
    Tire,
    Stress,
    Nourishment,
    Condition,
    Thirsty,
}
/// AL_EMOTION `Mood[8]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Pleasure,
    Anger,
    Sorrow,
    Fear,
    Surprise,
    Pain,
    Relax,
    Total,
}
const CURIOSITY: usize = 0;
const KINDNESS: usize = 1;
const AGGRESSIVE: usize = 2;
const SLEEPY_HEAD: usize = 3;
const SOLITUDE: usize = 4;
const VITALITY: usize = 5;
const GLUTTON: usize = 6;
const REGAIN: usize = 7;
const CALM: usize = 11;
const FICKLE: usize = 12;

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
    /// `ALBHV_Mayu`: child to adult.
    Evolve,
    /// `ALBHV_Dead`: end of life. `succeed` holds the latest pink/grey check.
    End,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cocoon {
    pub kind: CocoonKind,
    /// Frames elapsed in the cocoon.
    pub frames: f64,
    /// Evolve: the type decision has run. End: the second check has run.
    pub decided: bool,
    pub succeed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Emotion {
    pub mood: [i16; 8],
    pub state: [i16; 11],
    pub personality: [i8; 13],
    pub taste: i8,
    pub tv: i8,
    pub music: i8,
    /// 0 healthy, negative ill (counts up to 0), positive immune (counts down to 0).
    pub ill_state: [i8; 6],
}

/// `AL_KNOWLEDGE_PLAYER`. The MMO has one character per Chao: its owner.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerKnowledge {
    pub like: i16,
    pub fear: i16,
    pub distance: u16,
    pub meet: u16,
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
    /// Abl alleles (0 = E .. 5 = S) in ALL_STATS order.
    pub abl: [[u8; 2]; 7],
    pub life_time: [i8; 2],
    pub personality: [[i8; 2]; 13],
    pub taste: [i8; 2],
    pub tv: [i8; 2],
    pub music: [i8; 2],
    pub color: [u8; 2],
    pub non_tex: [bool; 2],
    pub jewel: [u8; 2],
    pub multi: [bool; 2],
}

/// `AL_GeneAnalyze` output, fixed when the egg is created.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Expressed {
    pub abl: [u8; 7],
    pub life_max: i16,
    pub personality: [i8; 13],
    pub taste: i8,
    pub tv: i8,
    pub music: i8,
    pub look: Look,
}

/// The expressed look handed to the body module at hatch.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Look {
    pub color: u8,
    pub jewel: u8,
    pub shiny: bool,
    pub monotone: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Egg {
    pub id: String,
    pub gene: Gene,
    pub expressed: Expressed,
    pub laid_at: f64,
    /// EggColor fixed at creation (a Tiny Chao Garden egg); otherwise the body keeps its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egg_color: Option<u8>,
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

/// Integration points for other modules, drained by [`apply_events`].
#[derive(Clone, Debug, PartialEq)]
pub enum LifeEvent {
    /// One native life tick; the body module adds child/adult growth here.
    LifeTick,
    /// The evolve cocoon reached its type decision: call body::evolve with these inputs.
    EvolutionDecision(ChaosInputs),
    Evolved,
    /// Rebirth: body resets axes and animal parts.
    Reincarnated,
    Hatched {
        look: Look,
    },
    Died,
    /// A fruit bite for the body's fruit growth; `last` marks the bite with the kind's side effects.
    FruitBite {
        kind: u8,
        child: bool,
        last: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChaosInputs {
    /// CHAO_PARAM_GC +0x82 `like` (fn_60_3B5C0 compares > 80).
    pub user_like: i16,
    /// CHAO_PARAM_GC +0x8E `nbSucceed`.
    pub reincarnations: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifeState {
    pub version: u32,
    pub revision: u64,
    pub stage: Stage,
    pub cocoon: Option<Cocoon>,
    /// Frames toward the next `AL_CalcParameter` call.
    pub calc_frames: f64,
    pub life_timer: u16,
    pub mood_timer: u16,
    pub need_timer: u16,
    pub ill_timer: u16,
    pub kw_timer: u16,
    /// age (+0x86) and old (+0x88).
    pub age: u16,
    pub adult_age: u16,
    /// life (+0x8A) and LifeMax (+0x8C).
    pub life: i16,
    pub life_max: i16,
    /// nbSucceed (+0x8E).
    pub reincarnations: u16,
    /// MMO lineage counter: increases when an egg hatches into this profile.
    pub generation: u32,
    /// like (+0x82), -100..100.
    pub like: i16,
    /// Exp (0..99) in ALL_STATS order.
    pub exp: [i16; 7],
    pub emotion: Emotion,
    pub owner: OwnerKnowledge,
    pub gene: Gene,
    pub nest: Vec<Egg>,
    pub breed_offer: Option<BreedOffer>,
    pub seed: u64,
    pub rng: u64,
    pub receipts: BTreeMap<String, Receipt>,
    #[serde(skip)]
    pub events: Vec<LifeEvent>,
    /// Set by the behaviour module while sleeping, sitting or walking (960-frame cadence).
    #[serde(skip)]
    pub busy: bool,
    #[serde(skip)]
    pub need_updated: bool,
}

fn hash_u64(parts: &[&[u8]]) -> u64 {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    u64::from_le_bytes(hasher.finalize()[..8].try_into().expect("eight bytes"))
}
/// Native `x * (300 ± P) / 300`, truncated toward zero.
fn scaled(base: i32, sign: i32, trait_value: i8) -> i32 {
    base * (300 + sign * i32::from(trait_value)) / 300
}

impl LifeState {
    /// `rand()/32768`: a 15-bit uniform value in 0..1.
    fn rand01(&mut self) -> f64 {
        let value = hash_u64(&[&self.seed.to_le_bytes(), &self.rng.to_le_bytes()]);
        self.rng = self.rng.wrapping_add(1);
        (value % 32768) as f64 / 32768.
    }
    /// `RAND_U32(n)` = `(u32)(njRandom() * (n - 0.0001))`.
    fn rand_u32(&mut self, n: u32) -> usize {
        (self.rand01() * (f64::from(n) - 0.0001)) as usize
    }
    /// `RAND_BOOL` = `(int)(njRandom() * 1.99)`.
    fn rand_bool(&mut self) -> usize {
        (self.rand01() * 1.99) as usize
    }

    /// `AL_GeneCreate` 0x1218C: genes for an egg with no parents.
    fn create_gene(&mut self) -> Gene {
        let r = self.rand01();
        let mut abl = [[0u8; 2]; 7];
        if r < 0.33 {
            // Shuffle {0..4} with 10 random swaps. The native loop then reads past the
            // 5-entry array for skills 5..7 (undefined values); those grades stay 0 here.
            let mut order = [0u8, 1, 2, 3, 4];
            for _ in 0..10 {
                let a = self.rand_u32(5);
                let b = self.rand_u32(5);
                order.swap(a, b);
            }
            for (slot, grade) in abl.iter_mut().zip(order) {
                *slot = [grade, grade];
            }
        } else if r < 0.66 {
            for slot in &mut abl {
                let grade = self.rand_u32(3) as u8 + 1;
                *slot = [grade, grade];
            }
        } else {
            let mut grades = [0u8; 8];
            for _ in 0..14 {
                let i = self.rand_u32(8);
                if grades[i] < MAX_GRADE {
                    grades[i] += 1;
                }
            }
            for (slot, grade) in abl.iter_mut().zip(grades) {
                *slot = [grade, grade];
            }
        }
        let mut personality = [[0i8; 2]; 13];
        for pair in &mut personality {
            *pair = [self.rand_u32(5) as i8, self.rand_u32(5) as i8];
        }
        let pair6 = |s: &mut Self| [s.rand_u32(6) as i8, s.rand_u32(6) as i8];
        let taste = pair6(self);
        let tv = pair6(self);
        let music = pair6(self);
        Gene {
            mother_id: None,
            father_id: None,
            mother_name: None,
            father_name: None,
            grandparents: [None, None, None, None],
            abl,
            life_time: [self.rand_u32(10) as i8, self.rand_u32(10) as i8],
            personality,
            taste,
            tv,
            music,
            color: [0, 0],
            non_tex: [false, false],
            jewel: [0, 0],
            multi: [false, false],
        }
    }

    /// `AL_GeneAnalyzeCommon` 0x1002C.
    fn analyze(&mut self, gene: &Gene) -> Expressed {
        let mut abl = [0u8; 7];
        for (grade, [a0, a1]) in abl.iter_mut().zip(gene.abl) {
            let (high, low) = if a0 >= a1 { (a0, a1) } else { (a1, a0) };
            *grade = if self.rand01() < GRADE_HIGH_CHANCE {
                high
            } else {
                low
            };
        }
        let mut personality = [0i8; 13];
        for (value, pair) in personality.iter_mut().zip(gene.personality) {
            *value = (pair[self.rand_bool()] - 2) * 40;
        }
        let taste = gene.taste[self.rand_u32(2)];
        let tv = gene.tv[self.rand_u32(2)];
        let music = gene.music[self.rand_u32(2)];
        let dominant = |s: &mut Self, pair: [u8; 2]| match pair {
            [0, b] => b,
            [a, 0] => a,
            _ => pair[s.rand_u32(2)],
        };
        let color = dominant(self, gene.color);
        let jewel = dominant(self, gene.jewel);
        let monotone = gene.non_tex[self.rand_u32(2)];
        Expressed {
            abl,
            life_max: life_max_of(gene, 0),
            personality,
            taste,
            tv,
            music,
            look: Look {
                color,
                jewel,
                shiny: gene.multi[0] || gene.multi[1],
                monotone,
            },
        }
    }

    fn blank(seed: u64, stage: Stage) -> Self {
        Self {
            version: VERSION,
            revision: 0,
            stage,
            cocoon: None,
            calc_frames: 0.,
            life_timer: 0,
            mood_timer: 0,
            need_timer: 0,
            ill_timer: 0,
            kw_timer: 0,
            age: 0,
            adult_age: 0,
            life: INIT_BASE_LIFE as i16,
            life_max: INIT_BASE_LIFE as i16,
            reincarnations: 0,
            generation: 1,
            like: 0,
            exp: [0; 7],
            emotion: Emotion {
                mood: [0; 8],
                state: [0; 11],
                personality: [0; 13],
                taste: 0,
                tv: 0,
                music: 0,
                ill_state: [0; 6],
            },
            owner: OwnerKnowledge::default(),
            gene: Gene {
                mother_id: None,
                father_id: None,
                mother_name: None,
                father_name: None,
                grandparents: [None, None, None, None],
                abl: [[0; 2]; 7],
                life_time: [0; 2],
                personality: [[0; 2]; 13],
                taste: [0; 2],
                tv: [0; 2],
                music: [0; 2],
                color: [0; 2],
                non_tex: [false; 2],
                jewel: [0; 2],
                multi: [false; 2],
            },
            nest: vec![],
            breed_offer: None,
            seed,
            rng: 0,
            receipts: BTreeMap::new(),
            events: vec![],
            busy: false,
            need_updated: false,
        }
    }

    /// Profiles saved before life state existed: a new first-generation egg's genes and
    /// emotion, keeping the profile's existing grades, levels and points. Hero, dark and Chaos
    /// forms already count as adults.
    pub fn legacy(id: &str, form: &str, grades: [u8; 7], policy: Policy) -> Self {
        let seed = hash_u64(&[b"chao-life-native-v1", id.as_bytes()]);
        let stage = if form == "normal" {
            Stage::Child
        } else {
            Stage::Adult
        };
        let mut state = Self::blank(seed, stage);
        state.gene = state.create_gene();
        for (pair, grade) in state.gene.abl.iter_mut().zip(grades) {
            *pair = [grade, grade];
        }
        let gene = state.gene.clone();
        let expressed = state.analyze(&gene);
        state.express(&expressed, policy);
        state
    }

    /// Apply an egg's expressed values to a freshly zeroed param, then `AL_EmotionStateInit`.
    fn express(&mut self, expressed: &Expressed, policy: Policy) {
        self.life = expressed.life_max;
        self.life_max = expressed.life_max;
        self.emotion.personality = expressed.personality;
        self.emotion.taste = expressed.taste;
        self.emotion.tv = expressed.tv;
        self.emotion.music = expressed.music;
        self.emotion_state_init(policy);
    }

    /// `AL_EmotionStateInit` 0x115D0. Native code stores to the wrong State slots; with
    /// `native_bugs` off the intended targets are used instead.
    fn emotion_state_init(&mut self, policy: Policy) {
        let r = |s: &mut Self, scale: f64| (s.rand01() * scale) as i16;
        if policy.native_bugs {
            let nourishment = r(self, 5000.);
            let thirsty = 3000 + r(self, 6000.);
            let state = &mut self.emotion.state;
            state[8] = nourishment;
            state[9] = 0;
            state[10] = thirsty;
        } else {
            let sleepy = r(self, 5000.);
            let hunger = 3000 + r(self, 6000.);
            let breed = r(self, 999.9);
            let tedious = r(self, 5000.);
            let tire = r(self, 8000.);
            self.emotion.state = [
                sleepy, 0, hunger, breed, tedious, 0, tire, 0, 10000, 10000, 0,
            ];
        }
    }

    pub fn restore(
        value: Option<&Value>,
        id: &str,
        form: &str,
        grades: [u8; 7],
        policy: Policy,
    ) -> Result<Self, String> {
        let Some(value) = value.filter(|v| !v.is_null()) else {
            return Ok(Self::legacy(id, form, grades, policy));
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
        let gene_ok = |g: &Gene| {
            g.abl.iter().flatten().all(|a| *a <= MAX_GRADE)
                && g.life_time.iter().all(|v| (0..=9).contains(v))
                && g.personality.iter().flatten().all(|v| (0..=4).contains(v))
                && [g.taste, g.tv, g.music]
                    .iter()
                    .flatten()
                    .all(|v| (0..=5).contains(v))
                && name_ok(&g.mother_name)
                && name_ok(&g.father_name)
                && g.grandparents.iter().all(name_ok)
        };
        let e = &self.emotion;
        self.version == VERSION
            && finite(self.calc_frames)
            && self.life_timer <= LIFE_INTERVAL
            && self.mood_timer <= MOOD_INTERVAL
            && self.need_timer <= NEED_INTERVAL
            && self.ill_timer <= ILL_INTERVAL
            && self.kw_timer <= KW_INTERVAL
            && (0..=LIFE_MAX_LIMIT as i16).contains(&self.life_max)
            && (0..=self.life_max).contains(&self.life)
            && (-USER_LIKE_LIMIT..=USER_LIKE_LIMIT).contains(&self.like)
            && self.exp.iter().all(|x| (0..100).contains(x))
            && e.mood.iter().all(|m| (0..=MOOD_LIMIT).contains(m))
            && e.state.iter().all(|s| (0..=STATE_LIMIT).contains(s))
            && e.personality
                .iter()
                .all(|p| (-PERSONALITY_LIMIT..=PERSONALITY_LIMIT).contains(p))
            && (-USER_LIKE_LIMIT..=USER_LIKE_LIMIT).contains(&self.owner.like)
            && (0..=FEAR_LIMIT).contains(&self.owner.fear)
            && self.owner.distance <= KW_DISTANCE_LIMIT
            && gene_ok(&self.gene)
            && self.nest.len() <= NEST_CAPACITY
            && self.nest.iter().all(|egg| {
                Uuid::parse_str(&egg.id).is_ok() && finite(egg.laid_at) && gene_ok(&egg.gene)
            })
            && self.receipts.len() <= MAX_RECEIPTS
            && self
                .receipts
                .iter()
                .all(|(id, r)| id == &r.request.request_id && r.revision <= self.revision)
            && match (self.stage, self.cocoon) {
                (Stage::Cocoon, Some(c)) => finite(c.frames),
                (Stage::Cocoon, None) | (_, Some(_)) => false,
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

/// MMO policy: the nest holds up to three eggs per profile.
pub const NEST_CAPACITY: usize = 3;
/// MMO policy: a mate call (offer) waits this long for the partner's answer.
pub const BREED_OFFER_MS: f64 = 60_000.;
/// MMO policy: partners must be within this distance in the same zone.
pub const SOCIAL_RANGE: f64 = 4.;

fn life_max_of(gene: &Gene, bonus: i32) -> i16 {
    let best = i32::from(gene.life_time[0].max(gene.life_time[1]));
    (INIT_BASE_LIFE + best * INIT_MUL_LIFE + bonus).min(LIFE_MAX_LIMIT) as i16
}
pub fn restore_for(
    data: &Value,
    id: &str,
    form: &str,
    stats: &BTreeMap<String, crate::game::Stat>,
    policy: Policy,
) -> Result<LifeState, String> {
    let grades = ALL_STATS.map(|name| stats.get(name).map_or(0, |s| s.grade.min(5) as u8));
    LifeState::restore(data.get("life"), id, form, grades, policy)
}

// ---------- Needs, moods, happiness ----------

fn hunger_state(p: &Player) -> i16 {
    ((100. - p.hunger.clamp(0., 100.)) * 100.).round() as i16
}
fn need_value(p: &Player, need: Need) -> i16 {
    match need {
        Need::Hunger => hunger_state(p),
        _ => p.life.emotion.state[need as usize],
    }
}
/// `AL_EmotionAdd` for one State: clamp to 0..10000. Returns the applied change. Hunger is
/// projected onto the existing Player.hunger satiety (hunger 10000 = satiety 0).
pub fn adjust_need(p: &mut Player, need: Need, delta: i32) -> i32 {
    let before = i32::from(need_value(p, need));
    let after = (before + delta).clamp(0, i32::from(STATE_LIMIT));
    match need {
        Need::Hunger => p.hunger = 100. - f64::from(after) / 100.,
        _ => p.life.emotion.state[need as usize] = after as i16,
    }
    after - before
}
/// `AL_EmotionAdd` for one Mood: clamp to 0..200. Returns the applied change.
pub fn adjust_mood(p: &mut Player, mood: Mood, delta: i32) -> i32 {
    let slot = &mut p.life.emotion.mood[mood as usize];
    let before = i32::from(*slot);
    let after = (before + delta).clamp(0, i32::from(MOOD_LIMIT));
    *slot = after as i16;
    after - before
}
/// `AL_ParameterAddUserLike` 0x20D7C plus `AL_KW_AddPlayerLike` 0x167C8 for the owner.
/// The APos attribute shift belongs to the body module.
pub fn add_user_like(p: &mut Player, add: i16) {
    let life = &mut p.life;
    life.like = (life.like + add).clamp(-USER_LIKE_LIMIT, USER_LIKE_LIMIT);
    life.owner.like = (life.owner.like + add).clamp(-USER_LIKE_LIMIT, USER_LIKE_LIMIT);
    if add > 0 {
        let personality = life.emotion.personality;
        let sub = scaled(KW_SUB_FEAR, 1, personality[AGGRESSIVE])
            + scaled(KW_SUB_FEAR, 1, personality[CALM]);
        life.owner.fear = (i32::from(life.owner.fear) - sub).clamp(0, i32::from(FEAR_LIMIT)) as i16;
        meet_owner(life);
    }
}
/// `AL_KW_AddPlayerFear` 0x166C0.
pub fn add_user_fear(p: &mut Player, add: i32) {
    let personality = p.life.emotion.personality;
    let add = scaled(add, -1, personality[AGGRESSIVE]) + scaled(add, -1, personality[CALM]);
    p.life.owner.fear = (i32::from(p.life.owner.fear) + add).clamp(0, i32::from(FEAR_LIMIT)) as i16;
}
/// `AL_KW_MeetPlayer` 0x165C4.
fn meet_owner(life: &mut LifeState) {
    life.owner.meet = life.owner.meet.saturating_add(1);
    life.owner.distance = life.owner.distance / 3 * 2;
}
/// Native petting (`fn_60_3FCA4`): like +1.
pub fn pet(p: &mut Player) {
    add_user_like(p, PET_USER_LIKE);
}

/// While true, the need tick runs every 10 calc ticks instead of 11 (sleep, sit, walk).
pub fn set_need_cadence_busy(p: &mut Player, busy: bool) {
    p.life.busy = busy;
}
/// True once after a tick in which `AL_CalcParameter` applied its needs step.
pub fn take_need_update(p: &mut Player) -> bool {
    std::mem::take(&mut p.life.need_updated)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeedsView {
    pub sleepy: i16,
    pub sleep_depth: i16,
    pub hunger: i16,
    pub breed: i16,
    pub tedious: i16,
    pub lonely: i16,
    pub tire: i16,
    pub stress: i16,
    pub nourishment: i16,
    pub condition: i16,
    pub thirsty: i16,
    pub mood: [i16; 8],
    pub personality: [i8; 13],
    pub like: i16,
    pub stage: Stage,
    pub ill: [i8; 6],
}
/// Read-only view in native ranges: State 0..10000, Mood 0..200, personality -100..100,
/// like -100..100, IllState negative = ill.
pub fn needs_view(p: &Player) -> NeedsView {
    let s = &p.life.emotion.state;
    NeedsView {
        sleepy: s[0],
        sleep_depth: s[1],
        hunger: hunger_state(p),
        breed: s[3],
        tedious: s[4],
        lonely: s[5],
        tire: s[6],
        stress: s[7],
        nourishment: s[8],
        condition: s[9],
        thirsty: s[10],
        mood: p.life.emotion.mood,
        personality: p.life.emotion.personality,
        like: p.life.like,
        stage: p.life.stage,
        ill: p.life.emotion.ill_state,
    }
}
pub fn is_ill(p: &Player) -> bool {
    p.life.emotion.ill_state.iter().any(|v| *v < 0)
}
/// `fn_60_160D8`: the first ill slot becomes immune. Its native caller is outside ChaoMain.
pub fn cure(p: &mut Player) -> bool {
    match p.life.emotion.ill_state.iter_mut().find(|v| **v < 0) {
        Some(slot) => {
            *slot = IMMUNE_START;
            true
        }
        None => false,
    }
}
/// `fn_60_46940`: may the sleeping Chao wake?
pub fn can_wake(p: &Player) -> bool {
    p.life.emotion.state[0] < WAKE_SLEEPY && p.life.emotion.state[1] < WAKE_SLEEP_DEPTH
}

// ---------- Stat growth and food ----------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StatProgress {
    pub exp_delta: i32,
    pub levels: u16,
    pub points: i32,
}
/// `AL_ParameterGrow` 0x21148 for one skill: Exp += amount (minimum 0), then at most one
/// level-up (`fn_60_2131C`: points += 11 + 3*grade + 0..4, capped at 4000). Owner perfect
/// stats never change.
pub fn grant_stat_progress(p: &mut Player, stat: &str, amount: i32) -> StatProgress {
    let Some(index) = ALL_STATS.iter().position(|s| *s == stat) else {
        return StatProgress::default();
    };
    if p.admin {
        return StatProgress::default();
    }
    let life = &mut p.life;
    let entry = p.stats.get_mut(stat).expect("seven canonical stats");
    let before_exp = i32::from(life.exp[index]);
    let before_points = entry.points as i32;
    let mut exp = (before_exp + amount).max(0);
    let mut result = StatProgress::default();
    if exp > 99 {
        if entry.level < MAX_LEVEL {
            exp -= 100;
            entry.level += 1;
            result.levels = 1;
            let grade = u32::from(entry.grade.min(u16::from(MAX_GRADE)));
            let random = (life.rand01() * LEVEL_RANDOM) as u32;
            entry.points =
                (entry.points + LEVEL_BASE + LEVEL_PER_GRADE * grade + random).min(LEVEL_SKILL_CAP);
        } else {
            exp = 99;
        }
    }
    life.exp[index] = exp as i16;
    result.exp_delta = exp - before_exp;
    result.points = entry.points as i32 - before_points;
    result
}

/// One fruit bite (`ALO_FruitHeldC` → `fn_60_2BD7C` → `AL_ParameterGrow` kind 3).
/// `kind` is the native fruit index, `row` its whole-fruit Exp row in ALL_STATS order and
/// `bite` counts 1..=4.
/// Applies the stat share, hunger −2400, life +3 and (child only) LifeMax +5.
pub fn eat_fruit_bite(p: &mut Player, kind: u8, row: [i16; 7], bite: u8) -> Vec<StatProgress> {
    let last = bite >= FRUIT_BITES;
    let gains: Vec<_> = ALL_STATS
        .iter()
        .zip(row)
        .map(|(stat, v)| {
            let v = i32::from(v);
            let share = v / 4 + if last { v - (v / 4) * 4 } else { 0 };
            grant_stat_progress(p, stat, share)
        })
        .collect();
    adjust_need(p, Need::Hunger, -SUB_HUNGER_BITE);
    let child = p.life.stage == Stage::Child;
    if child {
        p.life.life_max =
            (i32::from(p.life.life_max) + FRUIT_ADD_LIFE_MAX_CHILD).min(LIFE_MAX_LIMIT) as i16;
    }
    p.life.life = (i32::from(p.life.life) + FRUIT_ADD_LIFE).min(i32::from(p.life.life_max)) as i16;
    p.life
        .events
        .push(LifeEvent::FruitBite { kind, child, last });
    gains
}
/// A Black Market egg placed in the garden (ChaoMain item manager 0x18F7C):
/// `AL_GeneCreate`, then gene.EggColor = `egg_id`, then `CreateEgg` (0x8F20),
/// which runs `AL_GeneAnalyzeCommon` at once. For EggColor 1-13 both Color
/// alleles = id, NonTex = 1, Jewel = 0, Multi = 0; for 28-40 Color = id - 27
/// and Multi = 1 (0x10528-0x1060C). The egg goes into this profile's nest
/// (MMO policy, capacity [`NEST_CAPACITY`]). Returns the new egg's id.
pub fn lay_bought_egg(p: &mut Player, egg_id: u8, now: f64) -> Result<String, &'static str> {
    let (color, shiny) = match egg_id {
        1..=13 => (egg_id, false),
        28..=40 => (egg_id - 27, true),
        _ => return Err("invalid-request"),
    };
    if p.life.nest.len() >= NEST_CAPACITY {
        return Err("nest-full");
    }
    let mut gene = p.life.create_gene();
    gene.color = [color; 2];
    gene.non_tex = [true; 2];
    gene.jewel = [0; 2];
    gene.multi = [shiny; 2];
    let expressed = p.life.analyze(&gene);
    let id = Uuid::new_v4().hyphenated().to_string();
    p.life.nest.push(Egg {
        id: id.clone(),
        gene,
        expressed,
        laid_at: now,
        egg_color: None,
    });
    p.life.revision = p.life.revision.saturating_add(1);
    Ok(id)
}

/// `AL_GeneCreateGBAEgg` (ChaoMain GSNE8P 0x11808): the egg a Chao brings back from Tiny
/// Chao Garden. `v` is the Tiny egg id, sent unchanged as GBAEgg (BREEDING.md 6b).
/// Random genes, Jewel gene = v, EggColor = v + 0x35 (0 = a normal egg).
pub fn lay_gba_egg(p: &mut Player, v: u8, now: f64) -> Result<String, &'static str> {
    if v >= 12 {
        return Err("invalid-request");
    }
    if p.life.nest.len() >= NEST_CAPACITY {
        return Err("nest-full");
    }
    let mut gene = p.life.create_gene();
    gene.jewel = [v; 2];
    let expressed = p.life.analyze(&gene);
    let id = Uuid::new_v4().hyphenated().to_string();
    p.life.nest.push(Egg {
        id: id.clone(),
        gene,
        expressed,
        laid_at: now,
        egg_color: Some(if v == 0 { 0 } else { v + 0x35 }),
    });
    p.life.revision = p.life.revision.saturating_add(1);
    Ok(id)
}
/// `fn_60_2B8FC`: a fruit handed over by the owner, like +1.
pub fn fruit_handed_over(p: &mut Player) {
    add_user_like(p, FRUIT_USER_LIKE);
}
/// `ALBHV_Capture`: an animal's Exp row applies once, like +1.
pub fn capture_animal(p: &mut Player, row: [i16; 7]) -> Vec<StatProgress> {
    let gains = ALL_STATS
        .iter()
        .zip(row)
        .map(|(stat, v)| grant_stat_progress(p, stat, i32::from(v)))
        .collect();
    add_user_like(p, ANIMAL_USER_LIKE);
    gains
}

// ---------- The native clock ----------

fn fires(timer: &mut u16, interval: u16) -> bool {
    if *timer >= interval {
        *timer = 0;
        true
    } else {
        *timer += 1;
        false
    }
}

/// Advance one Chao by `dt` seconds. `connected` gates the life tick (MMO policy).
pub fn tick(p: &mut Player, dt: f64, policy: Policy, connected: bool, grounded: bool) {
    if !dt.is_finite() || dt <= 0. || p.life.stage == Stage::Dead {
        return;
    }
    let frames = dt * FRAMES_PER_SECOND;
    if let Some(mut cocoon) = p.life.cocoon {
        if connected {
            cocoon.frames += frames;
            p.life.cocoon = Some(cocoon);
            advance_cocoon(p, policy);
        }
        return;
    }
    p.life.calc_frames += frames;
    while p.life.calc_frames >= CALC_FRAMES {
        p.life.calc_frames -= CALC_FRAMES;
        calc_parameter(p, connected);
    }
    if connected && grounded {
        start_end_cocoon_if_due(p);
    }
}

/// One `AL_CalcParameter` 0x213CC call.
fn calc_parameter(p: &mut Player, connected: bool) {
    if connected && fires(&mut p.life.life_timer, LIFE_INTERVAL) {
        life_tick(p);
    }
    if fires(&mut p.life.mood_timer, MOOD_INTERVAL) {
        mood_tick(p);
    }
    if fires(&mut p.life.need_timer, NEED_INTERVAL) {
        need_tick(p);
        if p.life.busy {
            p.life.need_timer = 1;
        }
    }
    if fires(&mut p.life.ill_timer, ILL_INTERVAL) {
        for slot in &mut p.life.emotion.ill_state {
            *slot -= slot.signum();
        }
    }
    if fires(&mut p.life.kw_timer, KW_INTERVAL) {
        p.life.owner.distance = p
            .life
            .owner
            .distance
            .saturating_add(KW_ADD_DISTANCE)
            .min(KW_DISTANCE_LIMIT);
    }
}

fn chaos(p: &Player) -> bool {
    p.form == "chaos"
}

fn life_tick(p: &mut Player) {
    let life = &mut p.life;
    life.age = life.age.saturating_add(1);
    life.events.push(LifeEvent::LifeTick);
    if life.stage == Stage::Adult && p.form != "chaos" {
        life.adult_age = life.adult_age.saturating_add(1);
        let decay = (i32::from(life.adult_age) + LIFE_DECAY_OFFSET) / LIFE_DECAY_DIVISOR;
        life.life_max = (i32::from(life.life_max) - decay).max(0) as i16;
        life.life = life.life.min(life.life_max);
    }
    if life.rand01() < ILL_CHANCE {
        let i = (life.rand01() * ILL_SLOTS) as usize;
        if i != ILL_NEVER_RANDOM && life.emotion.ill_state[i] == 0 {
            life.emotion.ill_state[i] = ILL_START;
        }
    }
}

fn mood_tick(p: &mut Player) {
    let personality = p.life.emotion.personality;
    let sub = |sign: i32, t: usize| -scaled(SUB_MOOD, sign, personality[t]);
    let terms: [(Mood, i32); 13] = [
        (Mood::Pleasure, -SUB_MOOD),
        (Mood::Anger, sub(-1, AGGRESSIVE)),
        (Mood::Anger, sub(1, KINDNESS)),
        (Mood::Anger, sub(1, CALM)),
        (Mood::Sorrow, sub(1, AGGRESSIVE)),
        (Mood::Sorrow, sub(1, CALM)),
        (Mood::Fear, sub(1, AGGRESSIVE)),
        (Mood::Fear, sub(1, CALM)),
        (Mood::Surprise, sub(1, CALM)),
        (Mood::Pain, sub(1, REGAIN)),
        (Mood::Relax, sub(1, CURIOSITY)),
        (Mood::Relax, sub(1, FICKLE)),
        (Mood::Total, 0),
    ];
    for (mood, delta) in terms {
        adjust_mood(p, mood, delta);
    }
}

fn need_tick(p: &mut Player) {
    let personality = p.life.emotion.personality;
    let tire = f32::from(p.life.emotion.state[6]);
    let sleepy = (scaled(ADD_SLEEPY, 1, personality[SLEEPY_HEAD]) as f32
        * (1.0 + SLEEPY_TIRE_FACTOR * tire)) as i32;
    adjust_need(p, Need::Sleepy, sleepy);
    adjust_need(p, Need::Hunger, scaled(ADD_HUNGER, 1, personality[GLUTTON]));
    if p.life.stage == Stage::Adult && !chaos(p) {
        adjust_need(p, Need::Breed, scaled(ADD_BREED, 1, personality[VITALITY]));
    }
    adjust_need(
        p,
        Need::Tedious,
        scaled(ADD_TEDIOUS[0], 1, personality[CURIOSITY]),
    );
    adjust_need(
        p,
        Need::Tedious,
        scaled(ADD_TEDIOUS[1], 1, personality[AGGRESSIVE]),
    );
    adjust_need(
        p,
        Need::Tedious,
        scaled(ADD_TEDIOUS[2], -1, personality[CALM]),
    );
    if p.life.emotion.state[4] > 9999 && p.life.rand01() < 0.1 {
        p.life.emotion.state[4] = TEDIOUS_RESET;
    }
    adjust_need(
        p,
        Need::Lonely,
        scaled(ADD_LONELY, -1, personality[SOLITUDE]),
    );
    adjust_need(p, Need::Tire, scaled(ADD_TIRE, -1, personality[VITALITY]));
    adjust_need(p, Need::Nourishment, -SUB_NOURISHMENT);
    let s = p.life.emotion.state;
    if s[8] > CONDITION_HIGH_NOURISH.0 {
        adjust_need(p, Need::Condition, CONDITION_HIGH_NOURISH.1);
    }
    if s[8] < CONDITION_LOW_NOURISH.0 {
        adjust_need(p, Need::Condition, CONDITION_LOW_NOURISH.1);
    }
    if s[7] > CONDITION_STRESS.0 {
        adjust_need(p, Need::Condition, CONDITION_STRESS.1);
    }
    if p.life.emotion.mood[5] > CONDITION_PAIN.0 {
        adjust_need(p, Need::Condition, CONDITION_PAIN.1);
    }
    adjust_need(p, Need::Thirsty, ADD_THIRSTY);
    p.life.need_updated = true;
}

// ---------- Cocoons, evolution, end of life and rebirth ----------

/// `fn_60_1531C`: call when body::cocoon_due is true and the Chao stands on the ground.
pub fn start_evolution_cocoon(p: &mut Player) -> bool {
    if p.life.stage != Stage::Child {
        return false;
    }
    p.life.stage = Stage::Cocoon;
    p.life.breed_offer = None;
    p.life.cocoon = Some(Cocoon {
        kind: CocoonKind::Evolve,
        frames: 0.,
        decided: false,
        succeed: false,
    });
    true
}

/// `fn_60_3ADE0`: pink (succeed) when like > 50 and stress < 700.
fn succeed_check(p: &Player) -> bool {
    p.life.like > SUCCEED_USER_LIKE && p.life.emotion.state[7] < SUCCEED_STRESS
}

/// `fn_60_15060`: an adult at life 0 on the ground starts `ALBHV_Dead`.
fn start_end_cocoon_if_due(p: &mut Player) {
    if p.life.stage == Stage::Adult && p.life.life == 0 && !chaos(p) {
        p.life.stage = Stage::Cocoon;
        p.life.breed_offer = None;
        p.life.cocoon = Some(Cocoon {
            kind: CocoonKind::End,
            frames: 0.,
            decided: false,
            succeed: succeed_check(p),
        });
    }
}

fn advance_cocoon(p: &mut Player, policy: Policy) {
    let mut cocoon = p.life.cocoon.expect("cocoon stage");
    match cocoon.kind {
        CocoonKind::Evolve => {
            let decision = MAYU_WAIT + MAYU_FADE_IN;
            if !cocoon.decided && cocoon.frames >= decision {
                cocoon.decided = true;
                p.life
                    .events
                    .push(LifeEvent::EvolutionDecision(ChaosInputs {
                        user_like: p.life.like,
                        reincarnations: p.life.reincarnations,
                    }));
            }
            p.life.cocoon = Some(cocoon);
            if cocoon.frames >= decision + MAYU_CHANGE + MAYU_FADE_OUT + MAYU_RELAX {
                p.life.cocoon = None;
                p.life.stage = Stage::Adult;
                p.life.events.push(LifeEvent::Evolved);
            }
        }
        CocoonKind::End => {
            let second = MAYU_WAIT + MAYU_FADE_IN;
            if !cocoon.decided && cocoon.frames >= second {
                cocoon.decided = true;
                cocoon.succeed = succeed_check(p);
            }
            // MMO policy: with death off, a grey cocoon keeps checking until it turns pink.
            if cocoon.decided && !cocoon.succeed && !policy.death {
                cocoon.succeed = succeed_check(p);
            }
            p.life.cocoon = Some(cocoon);
            if cocoon.decided && cocoon.frames >= second + MAYU_CHANGE + DEAD_TAIL {
                if cocoon.succeed {
                    reincarnate(p, policy);
                } else if policy.death {
                    p.life.cocoon = None;
                    p.life.stage = Stage::Dead;
                    p.life.events.push(LifeEvent::Died);
                }
            }
        }
    }
}

fn reset_clock(life: &mut LifeState) {
    life.cocoon = None;
    life.calc_frames = 0.;
    life.life_timer = 0;
    life.age = 0;
    life.adult_age = 0;
    life.exp = [0; 7];
    life.breed_offer = None;
}

/// `CreateEggSucceed` 0x3B864 → `AL_SucceedGeneParam` 0x111C0 → `CreateEgg` → hatch.
pub fn reincarnate(p: &mut Player, policy: Policy) {
    let life = &mut p.life;
    life.reincarnations = life.reincarnations.saturating_add(1);
    if !p.admin {
        for stat in p.stats.values_mut() {
            stat.level = 1;
            stat.points = stat.points.min(SUCCEED_SKILL_CAP) / SUCCEED_SKILL_DIVISOR;
        }
    }
    for i in 0..13 {
        let copy = life.rand_bool();
        life.gene.personality[i][copy] = life.emotion.personality[i] / 40 + 2;
    }
    if policy.native_bugs {
        // The mood-reset loop writes Mood[8], the high byte of State[0].
        life.emotion.state[0] &= 0xFF;
    } else {
        life.emotion.mood = [0; 8];
    }
    life.emotion_state_init(policy);
    if policy.native_bugs {
        life.emotion.taste = 16;
        life.emotion.tv = 0;
        life.emotion.music = 0;
    } else {
        life.emotion.taste = life.gene.taste[life.rand_u32(2)];
        life.emotion.tv = life.gene.tv[life.rand_u32(2)];
        life.emotion.music = life.gene.music[life.rand_u32(2)];
    }
    for i in 0..13 {
        let copy = life.rand_bool();
        life.emotion.personality[i] = (life.gene.personality[i][copy] - 2) * 40;
    }
    let bonus = i32::from(life.life_max) / 10;
    life.life_max = life_max_of(&life.gene, bonus);
    life.life = life.life_max;
    reset_clock(life);
    life.stage = Stage::Child;
    life.events.push(LifeEvent::Reincarnated);
    if !policy.native_bugs {
        p.hunger = 100. - f64::from(p.life.emotion.state[2]) / 100.;
    }
}

/// Hatch an egg into this profile as a new generation (MMO lineage policy). The native egg
/// param starts all-zero; its genes were analysed when the egg was laid.
pub(crate) fn hatch(p: &mut Player, egg: Egg, policy: Policy) {
    if let Some(color) = egg.egg_color {
        p.body.look.egg_color = color;
    }
    let seed = hash_u64(&[b"egg", egg.id.as_bytes()]);
    let generation = p.life.generation.saturating_add(1);
    let nest = std::mem::take(&mut p.life.nest);
    let receipts = std::mem::take(&mut p.life.receipts);
    let revision = p.life.revision;
    let mut life = LifeState::blank(seed, Stage::Child);
    life.gene = egg.gene;
    life.generation = generation;
    life.nest = nest;
    life.receipts = receipts;
    life.revision = revision;
    life.express(&egg.expressed, policy);
    life.events.push(LifeEvent::Hatched {
        look: egg.expressed.look,
    });
    if !p.admin {
        for (name, grade) in ALL_STATS.iter().zip(egg.expressed.abl) {
            let stat = p.stats.get_mut(*name).expect("seven canonical stats");
            stat.level = 0;
            stat.points = 0;
            stat.grade = u16::from(grade);
        }
    }
    p.hunger = 100. - f64::from(life.emotion.state[2]) / 100.;
    p.life = life;
}

/// MMO policy: a carried Black Market egg picked from the inventory hatches straight into a
/// new Chao on the owner's card. The egg is made exactly as [`lay_bought_egg`] makes it.
pub fn hatch_bought_egg(
    p: &mut Player,
    egg_id: u8,
    now: f64,
    policy: Policy,
) -> Result<(), &'static str> {
    lay_bought_egg(p, egg_id, now)?;
    let egg = p.life.nest.pop().expect("just laid");
    hatch(p, egg, policy);
    Ok(())
}

/// `AL_CreateChildGene` / `AL_BlendGene` 0x1086C: one random allele from each parent.
pub fn blend(mother: &mut LifeState, father: &Gene, names: [&str; 2], ids: [&str; 2]) -> Gene {
    let m = mother.gene.clone();
    let f = father;
    let pick = |s: &mut LifeState| (s.rand01() * 2.) as usize % 2;
    let mut abl = [[0u8; 2]; 7];
    for (i, slot) in abl.iter_mut().enumerate() {
        *slot = [m.abl[i][pick(mother)], f.abl[i][pick(mother)]];
    }
    let mut personality = [[0i8; 2]; 13];
    for (i, slot) in personality.iter_mut().enumerate() {
        *slot = [
            m.personality[i][pick(mother)],
            f.personality[i][pick(mother)],
        ];
    }
    let i8s = |s: &mut LifeState, a: [i8; 2], b: [i8; 2]| [a[pick(s)], b[pick(s)]];
    let u8s = |s: &mut LifeState, a: [u8; 2], b: [u8; 2]| [a[pick(s)], b[pick(s)]];
    let bools = |s: &mut LifeState, a: [bool; 2], b: [bool; 2]| [a[pick(s)], b[pick(s)]];
    Gene {
        mother_id: Some(ids[0].into()),
        father_id: Some(ids[1].into()),
        mother_name: Some(names[0].chars().take(64).collect()),
        father_name: Some(names[1].chars().take(64).collect()),
        grandparents: [
            m.mother_name.clone(),
            m.father_name.clone(),
            f.mother_name.clone(),
            f.father_name.clone(),
        ],
        abl,
        life_time: i8s(mother, m.life_time, f.life_time),
        personality,
        taste: i8s(mother, m.taste, f.taste),
        tv: i8s(mother, m.tv, f.tv),
        music: i8s(mother, m.music, f.music),
        color: u8s(mother, m.color, f.color),
        non_tex: bools(mother, m.non_tex, f.non_tex),
        jewel: u8s(mother, m.jewel, f.jewel),
        multi: bools(mother, m.multi, f.multi),
    }
}

// ---------- Intents ----------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Intent {
    Breed {
        #[serde(rename = "partnerId")]
        partner_id: String,
    },
    Hatch {
        #[serde(rename = "eggId")]
        egg_id: String,
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
            Intent::Breed { partner_id } => id_ok(partner_id),
            Intent::Hatch { egg_id } | Intent::ReleaseEgg { egg_id } => id_ok(egg_id),
        }
}
/// The partner a request may also change, for transactional rollback by the service.
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
        || !["hub", "garden", "hero", "dark"].contains(&p.zone.as_str())
}
/// MMO policy: partners are picked from the breeding menu, so any Chao in the same garden counts.
fn in_range(a: &Player, b: &Player) -> bool {
    a.zone == b.zone
}
fn can_breed(p: &Player) -> bool {
    p.life.stage == Stage::Adult && !chaos(p)
}
/// MMO policy: gardens are social spaces full of other players' Chao, so the native
/// "garden under 8 Chao" check counts the inviter's own memory card (8 slots) instead.
fn card_population(game: &Game, me: &Player) -> usize {
    game.players
        .values()
        .filter(|p| p.card() == me.card())
        .count()
}

fn apply(game: &mut Game, id: &str, request: &Request) -> Result<(String, Value), &'static str> {
    let now = game.time;
    let policy = game.life_policy;
    let me = game.players.get(id).ok_or("profile-unavailable")?;
    match &request.intent {
        Intent::Breed { partner_id } => {
            if partner_id == id {
                return Err("invalid-partner");
            }
            let partner = game.players.get(partner_id).ok_or("partner-unavailable")?;
            if social_conflict(me) {
                return Err("activity-conflict");
            }
            // MMO policy: every Chao on the caller's own memory card is a partner wherever
            // it is, playing or not; nobody answers for it, so the egg is laid at once.
            let own = partner.card() == me.card();
            if !own && social_conflict(partner) {
                return Err("partner-busy");
            }
            if !can_breed(me) || !can_breed(partner) {
                return Err("stage-conflict");
            }
            if !own && !in_range(me, partner) {
                return Err("out-of-range");
            }
            let (my_name, partner_name) = (me.name.clone(), partner.name.clone());
            let answering = partner
                .life
                .breed_offer
                .as_ref()
                .is_some_and(|o| o.partner_id == id && o.expires_at > now);
            if !answering {
                // fn_60_15214: the mate call needs breed 10000 and a garden under 8 Chao.
                if me.life.emotion.state[3] < BREED_INVITE {
                    return Err("not-ready");
                }
                if policy.garden_limit && card_population(game, me) >= GARDEN_CHAO_LIMIT {
                    game.players
                        .get_mut(id)
                        .expect("checked")
                        .life
                        .emotion
                        .state[3] = 0;
                    return Ok(("garden-full".into(), json!({"breed":0})));
                }
                if !own {
                    let me = game.players.get_mut(id).expect("checked");
                    me.life.breed_offer = Some(BreedOffer {
                        partner_id: partner_id.clone(),
                        expires_at: now + BREED_OFFER_MS,
                    });
                    game.events.push(crate::game::Event {
                        message: json!({"type":"event","kind":"life-breed-offer","fromId":id,
                            "fromName":my_name,"expiresAt":now + BREED_OFFER_MS}),
                        target: Some(partner_id.clone()),
                    });
                    return Ok((
                        "offer-sent".into(),
                        json!({"offerExpiresAt":now + BREED_OFFER_MS}),
                    ));
                }
            } else if me.life.emotion.state[3] <= BREED_ANSWER {
                // fn_60_150FC: the answer needs breed above 5000.
                return Err("not-ready");
            }
            // fn_60_40130: the inviter is the mother; MMO policy: the egg goes to her nest.
            let (mother_id, mother_name, father_id, father_name) = if answering {
                (partner_id.as_str(), partner_name, id, my_name)
            } else {
                (id, my_name, partner_id.as_str(), partner_name)
            };
            if game.players[mother_id].life.nest.len() >= NEST_CAPACITY {
                return Err(if answering {
                    "partner-nest-full"
                } else {
                    "nest-full"
                });
            }
            let father = game.players[father_id].life.gene.clone();
            let mut mother = game.players.remove(mother_id).expect("checked");
            let gene = blend(
                &mut mother.life,
                &father,
                [&mother_name, &father_name],
                [mother_id, father_id],
            );
            let expressed = mother.life.analyze(&gene);
            let egg_id = Uuid::new_v4().hyphenated().to_string();
            mother.life.nest.push(Egg {
                id: egg_id.clone(),
                gene,
                expressed,
                laid_at: now,
                egg_color: None,
            });
            mother.life.breed_offer = None;
            mother.life.emotion.state[3] = 0;
            mother.life.revision = mother.life.revision.saturating_add(1);
            game.players.insert(mother_id.to_owned(), mother);
            let other = game.players.get_mut(father_id).expect("checked");
            other.life.emotion.state[3] = 0;
            other.life.breed_offer = None;
            game.events.push(crate::game::Event {
                message: json!({"type":"event","kind":"life-egg-laid","eggId":egg_id,
                    "partnerId":father_id,"partnerName":father_name}),
                target: Some(mother_id.to_owned()),
            });
            Ok((
                "egg-laid".into(),
                json!({"eggId":egg_id,"keeperId":mother_id}),
            ))
        }
        Intent::Hatch { egg_id } => {
            let ending = me.life.stage == Stage::Dead
                || me
                    .life
                    .cocoon
                    .is_some_and(|c| c.kind == CocoonKind::End && c.decided && !c.succeed);
            if !ending {
                return Err("stage-conflict");
            }
            let p = game.players.get_mut(id).expect("checked");
            let index = p
                .life
                .nest
                .iter()
                .position(|e| &e.id == egg_id)
                .ok_or("egg-unavailable")?;
            let egg = p.life.nest.remove(index);
            hatch(p, egg, policy);
            Ok(("hatched".into(), json!({"generation":p.life.generation})))
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

/// Strict `life` envelope with replayable receipts.
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
                // Safe: an evicted retry now fails the revision check instead of reapplying.
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

/// Drain life events into the body: `LifeTick` → BodyState::life_step,
/// `EvolutionDecision` → body::evolve, `Reincarnated` → body::reincarnate,
/// `Hatched` → a fresh body with body::express, `FruitBite` → body::apply_fruit.
pub fn apply_events(game: &mut Game, id: &str) {
    let Some(p) = game.players.get_mut(id) else {
        return;
    };
    let events = std::mem::take(&mut p.life.events);
    for event in events {
        let (kind, text) = match &event {
            LifeEvent::LifeTick => {
                p.body.life_step();
                continue;
            }
            LifeEvent::EvolutionDecision(inputs) => {
                // PartsBTL MinimalFlag: every one of the 21 animal species eaten.
                let all_animals = crate::care::all_species_eaten(p);
                crate::body::evolve(
                    &mut p.body,
                    crate::body::ChaosInputs {
                        all_animals,
                        user_like: i32::from(inputs.user_like),
                        param_142: i32::from(inputs.reincarnations),
                    },
                );
                p.update_form();
                game.dirty = true;
                continue;
            }
            LifeEvent::FruitBite { kind, last, .. } => {
                crate::body::apply_fruit(&mut p.body, *kind, *last);
                game.dirty = true;
                continue;
            }
            LifeEvent::Reincarnated => {
                crate::body::reincarnate(&mut p.body);
                // AL_SucceedGeneParam: MinimalFlag and animal parts.
                crate::care::rebirth(p);
                p.update_form();
                ("life-reincarnated", "Your Chao was reborn.")
            }
            LifeEvent::Hatched { look } => {
                // A new Chao: a fresh child body with the expressed gene look.
                let egg_color = p.body.look.egg_color;
                p.body = crate::body::BodyState::default();
                crate::body::express(
                    &mut p.body,
                    &crate::body::GeneLook {
                        color: look.color,
                        jewel: look.jewel,
                        shiny: look.shiny,
                        monotone: look.monotone,
                        egg_color,
                    },
                );
                p.update_form();
                ("life-hatched", "A new Chao hatched.")
            }
            LifeEvent::Evolved => ("life-evolved", "Your Chao has grown into an adult."),
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

/// Public per-player summary.
pub fn public_summary(p: &Player) -> Value {
    json!({"stage":p.life.stage,"cocoon":p.life.cocoon.map(|c|c.kind),"age":p.life.age,
        "reincarnations":p.life.reincarnations,"generation":p.life.generation,
        "ill":is_ill(p),
        // The breeding menu's check: would this Chao answer a call (fn_60_150FC, breed > 5000)?
        "inTheMood":p.life.emotion.state[3] > BREED_ANSWER,"busy":social_conflict(p)})
}

/// Owner-only `snapshot.life`.
pub fn public(game: &Game, id: &str) -> Value {
    let Some(p) = game.players.get(id) else {
        return Value::Null;
    };
    let l = &p.life;
    let mut state = l.emotion.state;
    state[2] = hunger_state(p);
    let exp = named(&ALL_STATS, &l.exp);
    let grades = Value::Object(
        ALL_STATS
            .iter()
            .zip(l.gene.abl)
            .map(|(n, a)| ((*n).into(), json!(a)))
            .collect(),
    );
    let policy = game.life_policy;
    json!({"rulesVersion":RULES,"revision":l.revision,"stage":l.stage,
        "cocoon":l.cocoon.map(|c|json!({"kind":c.kind,"frames":c.frames,"decided":c.decided,"succeed":c.succeed})),
        "age":l.age,"adultAge":l.adult_age,"life":l.life,"lifeMax":l.life_max,"immortal":chaos(p),
        "reincarnations":l.reincarnations,"generation":l.generation,"like":l.like,
        "policy":{"death":policy.death,"residentNeeds":policy.resident_needs,
            "nativeBugs":policy.native_bugs,"gardenLimit":policy.garden_limit},
        "exp":exp,"mood":named(&MOOD_NAMES,&l.emotion.mood),"needs":named(&NEED_NAMES,&state),
        "personality":named(&PERSONALITY_NAMES,&l.emotion.personality),
        "taste":l.emotion.taste,"tv":l.emotion.tv,"music":l.emotion.music,
        "illness":named(&ILLNESS_NAMES,&l.emotion.ill_state),
        "owner":l.owner,
        "gene":{"motherName":l.gene.mother_name,"fatherName":l.gene.father_name,
            "grandparents":l.gene.grandparents,"gradeAlleles":grades,"lifeTime":l.gene.life_time},
        "nest":l.nest.iter().map(|e|json!({"id":e.id,"laidAt":e.laid_at,
            "motherName":e.gene.mother_name,"fatherName":e.gene.father_name,"look":e.expressed.look,
            "gene":e.gene,"eggColor":e.egg_color})).collect::<Vec<_>>(),
        "nestCapacity":NEST_CAPACITY,
        "breed":{"offer":l.breed_offer,"canCall":l.emotion.state[3] >= BREED_INVITE,
            "canAnswer":l.emotion.state[3] > BREED_ANSWER},
        "eligibility":{"canBreed":!social_conflict(p) && can_breed(p),
            "canHatch":l.stage==Stage::Dead
                || l.cocoon.is_some_and(|c|c.kind==CocoonKind::End && c.decided && !c.succeed)},
        "provenance":{"source":"GSNE8P ChaoMain.rel e7e24b8b6f4305d95531c9d898d566ead79ddc8d",
            "frameRate":"60 Hz assumed","agingPolicy":"connected-time-only"}})
}

#[cfg(test)]
mod tests;
