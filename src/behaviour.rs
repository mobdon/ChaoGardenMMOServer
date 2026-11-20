//! Native garden Chao behaviour, ported from GameCube SA2B (GSNE8P) `ChaoMain.rel`.
//!
//! Source: a read-only trace of the owner's own disc. The REL's SHA-1 matches
//! the roeming/SA2B decomp config (commit 874a4803), so the addresses below are
//! ChaoMain `.text` offsets in that reference. The motion engine is main.dol
//! `MotionControl` 0x8002D214. Nothing here is tuned: every constant, branch
//! and random draw follows the traced code, in program order.
//!
//! Ported: AL_BehaviorControl, the intention pass for Yawn, Groggy, Sleep and
//! Rest, the ALBHV_Move fallback, Think, Sleep, SitSleep, the wake test,
//! Groggy, Yawn, SitDown, SitIdle, StandUp, the petting interrupt and petting,
//! and Walk/Run/RunFast/Crawl with AL_MoveCommon's arrival test.
//!
//! Not ported (left out, not approximated): the other intention scorers
//! (illness, hunger, thirst, music, dance, sing, toys, boredom, fear, sorrow,
//! pain, pleasure, cocoon, anger, watering can, greeting, nest), tree finding
//! for GoToSleep, climbing, bumping, slope and height exits, Chao movement
//! physics (AL_ForwardAcc, turning, friction), sounds and icons. Random draws
//! that only pick a sound are still made, so the draw order stays native.
//!
//! Three things are adapted for the MMO and are flagged in the snapshot:
//! - `rand()` is a per-resident 15-bit generator. Native uses one global
//!   main.dol `rand` stream, which an MMO cannot share.
//! - Walking uses the server's existing authoritative movement toward the
//!   native aim point, because native Chao acceleration is not traced.
//! - Petting: one `pet` intent is one full touch, held until the native
//!   behaviour disables touch.
//!
//! The motion table and the per-garden aim lists are game data. They are
//! loaded from local files and never committed. Without the motion table the
//! native routine is unavailable. Without an aim list, wandering is
//! unavailable and Move ends at once.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, sync::Arc};

use crate::game::Player;
use crate::life::{self, Mood, Need};

/// One logic tick per video field: NTSC GSNE8P runs at 59.94 Hz
/// (main 0x800D3AD0 → fn_80117DF8 with N = 1).
pub const FRAMES_PER_SECOND: f64 = 60_000. / 1001.;
pub const RULES: &str = "native-gsne8p-chaomain";

// ChaoGlobal (.data 0x2A50) values used here.
const INTENTION_MIN_SCORE: f32 = 0.2;
const INTENTION_RANDOM_RATIO: f32 = 0.4;
const STATE_LIMIT: f32 = 10000.;
const SLEEP_TRIGGER: u32 = 8000;
const AKUBI_TRIGGER: [u32; 3] = [6000, 8000, 9000];
const REST_TRIGGER: [u32; 2] = [2000, 6000];
const REST_END_TRIGGER: u32 = 8000;
const MAX_SCORE: f32 = 0.99;
const SKILL_WALK: u16 = 150;
const SKILL_RUN: u16 = 2000;
const SKILL_RUN2: u16 = 4000;
const ADD_SLEEP_DEPTH: i32 = 1500;
const SUB_SLEEPY: i32 = 1500;
const SUB_SLEEP_DEPTH: i32 = 3500;
const SUB_TIRE_SLEEP: i32 = 300;
const SUB_TIRE_REST: i32 = 100;
const ADD_CONDITION_REST: i32 = 30;
const SUB_TEDIOUS_WALK: i32 = 100;
const ADD_TIRE_WALK: i32 = 300;
const WAKE_SLEEPY: i32 = 1000;
const WAKE_SLEEP_DEPTH: i32 = 2000;
const NADE_ADD_ATTR: f32 = 0.05;
const DEFAULT_LINK_FRAMES: f32 = 15.;

/// Native motion table row (ChaoMain .data 0x3F870, 622 × 0x1C).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MotionRow {
    pub mode: u8,
    pub start: f32,
    pub end: f32,
    pub spd: f32,
    pub next: i16,
    pub link_step: i16,
    pub posture: u8,
}

/// The 622-state table, loaded from a local file built from the owner's
/// extracted files (`chao-motion-table.json`).
#[derive(Clone, Debug)]
pub struct MotionTable(Vec<MotionRow>);

impl MotionTable {
    pub const STATES: usize = 622;
    pub fn from_rows(rows: Vec<MotionRow>) -> Result<Self, String> {
        if rows.len() != Self::STATES {
            return Err(format!(
                "expected {} motion states, found {}",
                Self::STATES,
                rows.len()
            ));
        }
        for (index, row) in rows.iter().enumerate() {
            let valid = row.mode <= 7
                && row.posture <= 5
                && [row.start, row.end, row.spd].iter().all(|v| v.is_finite())
                && (row.next == -1 || (0..Self::STATES as i16).contains(&row.next));
            if !valid {
                return Err(format!("motion state {index} is malformed"));
            }
        }
        Ok(Self(rows))
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let rows: Vec<MotionRow> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        Self::from_rows(rows)
    }
    pub fn rows(&self) -> &[MotionRow] {
        &self.0
    }
    /// Read `AL_motion_table` (.data 0x3F870, 622 × 0x1C, big-endian) from the
    /// owner's GSNE8P `ChaoMain.rel`. Layout per row (MOTION_TABLE): motion
    /// pointer (relocated, ignored), s16 mode, s16 posture, s32 next,
    /// s32 link_step, f32 start, f32 end, f32 spd.
    pub fn from_rel(rel: &[u8]) -> Result<Self, String> {
        const MODULE_ID: u32 = 0x3C;
        const DATA_SECTION: usize = 5;
        const TABLE: usize = 0x3F870;
        const ROW: usize = 0x1C;
        let u32_at = |at: usize| -> Result<u32, String> {
            rel.get(at..at + 4)
                .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or_else(|| format!("REL truncated at {at:#x}"))
        };
        if u32_at(0)? != MODULE_ID {
            return Err("not ChaoMain.rel (module id is not 0x3C)".into());
        }
        let sections = u32_at(0x0C)? as usize;
        let info = u32_at(0x10)? as usize;
        if DATA_SECTION >= sections {
            return Err("REL has no .data section".into());
        }
        let offset = u32_at(info + DATA_SECTION * 8)?;
        let length = u32_at(info + DATA_SECTION * 8 + 4)? as usize;
        if offset & 1 != 0 || offset == 0 || length < TABLE + Self::STATES * ROW {
            return Err("REL .data section does not hold the motion table".into());
        }
        let base = offset as usize + TABLE;
        let mut rows = Vec::with_capacity(Self::STATES);
        for index in 0..Self::STATES {
            let at = base + index * ROW;
            let word = |o: usize| u32_at(at + o);
            let mode = (word(4)? >> 16) as i16;
            let posture = word(4)? as u16 as i16;
            let next = word(8)? as i32;
            let link_step = word(12)? as i32;
            let float = |o: usize| word(o).map(f32::from_bits);
            let narrow =
                |v: i32| i16::try_from(v).map_err(|_| format!("motion state {index} out of range"));
            rows.push(MotionRow {
                mode: u8::try_from(mode).map_err(|_| format!("motion state {index} mode"))?,
                start: float(16)?,
                end: float(20)?,
                spd: float(24)?,
                next: narrow(next)?,
                link_step: narrow(link_step)?,
                posture: u8::try_from(posture)
                    .map_err(|_| format!("motion state {index} posture"))?,
            });
        }
        Self::from_rows(rows)
    }
    fn row(&self, num: u16) -> MotionRow {
        self.0[usize::from(num).min(Self::STATES - 1)]
    }
}

/// Per-zone aim cells (ChaoMain bss 0x1D3F8 list 1, from a 32×40 map per stage).
#[derive(Clone, Debug, Default)]
pub struct AimLists(BTreeMap<String, Vec<(i32, i32)>>);

impl AimLists {
    pub fn from_map(map: BTreeMap<String, Vec<(i32, i32)>>) -> Result<Self, String> {
        for (zone, cells) in &map {
            if cells.len() > 512
                || cells
                    .iter()
                    .any(|(x, z)| !(0..32).contains(x) || !(0..40).contains(z))
            {
                return Err(format!("aim list for {zone} is malformed"));
            }
        }
        Ok(Self(map))
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        Self::from_map(serde_json::from_slice(&bytes).map_err(|e| e.to_string())?)
    }
}

/// Local native data shared by every resident.
#[derive(Clone, Debug, Default)]
pub struct NativeData {
    pub motion: Option<Arc<MotionTable>>,
    pub aim: Option<Arc<AimLists>>,
}

// ---------------------------------------------------------------- motion

#[derive(Clone, Copy, Debug, PartialEq)]
struct Link {
    target: u16,
    ratio: f32,
    spd: f32,
    /// LKLP end→start blend of the same clip.
    self_loop: bool,
}

/// main.dol `MotionControl` state for one Chao.
#[derive(Clone, Debug, PartialEq)]
pub struct Motion {
    pub num: u16,
    pub frame: f32,
    start: f32,
    end: f32,
    spd: f32,
    mode: u8,
    link: Option<Link>,
    multi_spd: f32,
    end_flag: bool,
    stop_flag: bool,
}

impl Motion {
    fn init(table: &MotionTable, num: u16) -> Self {
        let row = table.row(num);
        Self {
            num,
            frame: row.start,
            start: row.start,
            end: row.end,
            spd: row.spd,
            mode: row.mode,
            link: None,
            multi_spd: 1.,
            end_flag: false,
            stop_flag: false,
        }
    }
    fn switch(&mut self, table: &MotionTable, num: u16) {
        let row = table.row(num);
        self.num = num;
        self.frame = row.start;
        self.start = row.start;
        self.end = row.end;
        self.spd = row.spd;
        self.mode = row.mode;
    }
    fn link(&mut self, num: u16, frames: f32) {
        self.link = Some(Link {
            target: num,
            ratio: 0.,
            spd: 1. / frames.max(1.),
            self_loop: false,
        });
    }
    /// `AL_GetMotionPosture`: the current clip's posture (old clip during a link).
    pub fn posture(&self, table: &MotionTable) -> u8 {
        table.row(self.num).posture
    }
    /// One frame of `MotionControl`.
    fn control(&mut self, table: &MotionTable) {
        self.end_flag = false;
        self.stop_flag = false;
        if let Some(mut link) = self.link {
            link.ratio += link.spd;
            if link.ratio >= 1. {
                self.link = None;
                if link.self_loop {
                    self.frame = self.start;
                } else {
                    self.switch(table, link.target);
                }
            } else {
                self.link = Some(link);
            }
            return;
        }
        self.frame += self.spd * self.multi_spd;
        self.multi_spd = 1.;
        let forward = self.spd >= 0.;
        let limit = match (self.mode, forward) {
            (1, true) => self.end + 1.,
            (1, false) => self.end - 1.,
            _ => self.end,
        };
        let ended = if forward {
            self.frame >= limit
        } else {
            self.frame <= limit
        };
        if !ended {
            return;
        }
        self.end_flag = true;
        let row = table.row(self.num);
        let next = u16::try_from(row.next).ok();
        match self.mode {
            0 => self.frame -= self.end - self.start,
            1 => self.frame -= self.end + 1. - self.start,
            2 => {
                self.frame = self.end;
                self.link = Some(Link {
                    target: self.num,
                    ratio: 0.,
                    spd: self.spd.abs(),
                    self_loop: true,
                });
            }
            3 => {
                self.frame = self.end;
                self.stop_flag = true;
            }
            4 => {
                self.frame = self.end;
                if let Some(next) = next {
                    let frames = f32::from(row.link_step.unsigned_abs());
                    self.link(next, frames);
                }
            }
            5 => {
                if let Some(next) = next {
                    self.switch(table, next);
                }
            }
            6 => {
                let over = self.frame - self.end;
                if let Some(next) = next {
                    self.switch(table, next);
                    self.frame += over;
                }
            }
            7 => {
                std::mem::swap(&mut self.start, &mut self.end);
                self.spd = -self.spd;
            }
            _ => {}
        }
    }
    /// A held STOP clip raises End and Stop on every frame.
    fn held(&self) -> bool {
        self.mode == 3 && self.link.is_none() && self.frame == self.end
    }
    pub fn public(&self) -> Value {
        json!({"num":self.num,"frame":self.frame,"spd":self.spd,
            "link":self.link.map(|l| json!({"target":l.target,"ratio":l.ratio,"spd":l.spd,"selfLoop":l.self_loop}))})
    }
}

// ------------------------------------------------------------- behaviour

/// Behaviour functions, named and addressed as in ChaoMain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bhv {
    Think,
    GoToSleep,
    Sleep,
    SitSleep,
    Groggy,
    Akubi,
    SitDown,
    SitIdle,
    StandUp,
    Nade,
    Move,
    Walk,
    Run,
    RunFast,
    Crawl,
}

impl Bhv {
    pub fn native(self) -> (&'static str, &'static str) {
        match self {
            Bhv::Think => ("ALBHV_Think", "0x47830"),
            Bhv::GoToSleep => ("Bhv_GoToSleep", "0x46684"),
            Bhv::Sleep => ("ALBHV_Sleep", "0x45CA8"),
            Bhv::SitSleep => ("ALBHV_SitSleep", "0x45A58"),
            Bhv::Groggy => ("Bhv_Groggy", "0x45468"),
            Bhv::Akubi => ("Bhv_Akubi", "0x455E4"),
            Bhv::SitDown => ("Bhv_SitDown", "0x4213C"),
            Bhv::SitIdle => ("Bhv_SitIdle", "0x42308"),
            Bhv::StandUp => ("Bhv_StandUp", "0x42220"),
            Bhv::Nade => ("Bhv_Nade", "0x3FCA4"),
            Bhv::Move => ("ALBHV_Move", "0x3DF88"),
            Bhv::Walk => ("Bhv_Walk", "0x4B924"),
            Bhv::Run => ("Bhv_Run", "0x4B7A8"),
            Bhv::RunFast => ("Bhv_RunFast", "0x4B6AC"),
            Bhv::Crawl => ("Bhv_Crawl", "0x4BA6C"),
        }
    }
    /// Coarse legacy `watch.activity` for older clients.
    pub fn legacy(self) -> &'static str {
        match self {
            Bhv::Move | Bhv::Walk | Bhv::Run | Bhv::RunFast | Bhv::Crawl => "wander",
            Bhv::Sleep | Bhv::SitSleep | Bhv::SitDown | Bhv::SitIdle | Bhv::GoToSleep => "rest",
            _ => "idle",
        }
    }
    /// Behaviours that poll the need tick (the 960-frame cadence).
    fn polls_needs(self) -> bool {
        matches!(
            self,
            Bhv::Sleep
                | Bhv::SitSleep
                | Bhv::SitIdle
                | Bhv::Walk
                | Bhv::Run
                | Bhv::RunFast
                | Bhv::Crawl
        )
    }
}

/// Face set by the native call sites: eye, mouth and the hold timer (−1 holds).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Face {
    pub eye: Option<(u8, i32)>,
    pub mouth: Option<(u8, i32)>,
}

/// Per-resident behaviour work area (chaowk+0x1B4) plus motion and RNG.
#[derive(Clone, Debug)]
pub struct Brain {
    mode: u16,
    sub_mode: u16,
    timer: i32,
    sub_timer: i32,
    free_work: i32,
    interval: [u32; 16],
    queue: Vec<Bhv>,
    current: usize,
    pub motion: Motion,
    pub face: Face,
    rng: u32,
    /// Native aim point in world units (native × 0.1).
    pub aim: Option<(f64, f64)>,
    /// One `pet` intent is one touch, held until the behaviour disables it.
    touch: bool,
    touch_enabled: bool,
    frames: f64,
    pub frame_count: u64,
}

/// What the Chao asks the server's movement to do this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drive {
    Stop,
    Toward { x: f64, z: f64, gait: Gait },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gait {
    Crawl,
    Walk,
    Run,
}

/// The per-resident interface to the server state the native code reads.
pub struct Body<'a> {
    pub player: &'a mut Player,
    pub zone: &'a str,
}

impl Brain {
    pub fn new(table: &MotionTable, seed: u32) -> Self {
        Self {
            mode: 0,
            sub_mode: 0,
            timer: 0,
            sub_timer: 0,
            free_work: 0,
            interval: [0; 16],
            queue: vec![Bhv::Think],
            current: 0,
            motion: Motion::init(table, 0),
            face: Face::default(),
            rng: seed,
            aim: None,
            touch: false,
            touch_enabled: true,
            frames: 0.,
            frame_count: 0,
        }
    }
    pub fn behaviour(&self) -> Bhv {
        self.queue.get(self.current).copied().unwrap_or(Bhv::Think)
    }
    /// 15-bit `rand()` (0..32767). Per-resident stream; see the module note.
    fn rand(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (self.rng >> 16) & 0x7FFF
    }
    /// RAND01 = (float)rand() × 3.0517578125e-05f.
    fn rand01(&mut self) -> f32 {
        self.rand() as f32 * 3.051_757_8e-5
    }
    fn reset_work(&mut self) {
        self.mode = 0;
        self.sub_mode = 0;
        self.timer = 0;
        self.sub_timer = 0;
    }
    /// AL_SetBehavior: clears the queue.
    fn set(&mut self, f: Bhv) {
        self.reset_work();
        self.queue.clear();
        self.queue.push(f);
        self.current = 0;
        self.free_work = 0;
    }
    /// AL_SetNextBehavior.
    fn set_next(&mut self, f: Bhv) {
        if self.queue.len() - self.current < 15 {
            self.queue.push(f);
        }
    }
    fn pop_next(&mut self) -> bool {
        self.reset_work();
        self.current += 1;
        self.current < self.queue.len()
    }
    /// Start a touch (one `pet` intent).
    pub fn touch(&mut self) {
        if self.touch_enabled {
            self.touch = true;
        }
    }
    fn eye(&mut self, n: u8) {
        self.face.eye = Some((n, -1));
    }
    fn mouth(&mut self, n: u8) {
        self.face.mouth = Some((n, -1));
    }

    /// Advance by `dt` seconds of server time, one native frame at a time.
    /// Returns the drive for the server's movement.
    pub fn tick(&mut self, data: &NativeData, body: &mut Body, dt: f64) -> Drive {
        let Some(table) = data.motion.as_deref() else {
            return Drive::Stop;
        };
        self.frames += dt * FRAMES_PER_SECOND;
        let mut drive = Drive::Stop;
        while self.frames >= 1. {
            self.frames -= 1.;
            self.frame_count += 1;
            drive = self.frame(table, data.aim.as_deref(), body);
            self.motion.control(table);
            if self.motion.held() {
                self.motion.end_flag = true;
                self.motion.stop_flag = true;
            }
        }
        life::set_need_cadence_busy(body.player, self.behaviour().polls_needs());
        drive
    }

    /// AL_BehaviorControl 0x0F8 for one frame.
    fn frame(&mut self, table: &MotionTable, aim: Option<&AimLists>, body: &mut Body) -> Drive {
        for timer in &mut self.interval {
            *timer = timer.saturating_sub(1);
        }
        self.check_nade(table);
        let (result, drive) = self.run(table, aim, body);
        if result == 2 {
            self.calc_intention(body.player);
        } else if result == 1 && !self.pop_next() {
            self.set(Bhv::Think);
        }
        drive
    }

    /// AL_CheckNade 0x3FBD0.
    fn check_nade(&mut self, _table: &MotionTable) {
        if matches!(self.behaviour(), Bhv::Nade | Bhv::Groggy) || !self.touch {
            return;
        }
        self.set(Bhv::Nade);
    }

    /// fn_60_14E8C, limited to the ported scorers, plus the Move fallback.
    fn calc_intention(&mut self, p: &Player) {
        let n = life::needs_view(p);
        let mut best = INTENTION_MIN_SCORE;
        // Illness: not ported.
        self.score_akubi(i32::from(n.sleepy), &mut best);
        self.score_groggy(i32::from(n.sleep_depth), &mut best);
        self.score_sleep(i32::from(n.sleepy), &mut best);
        // Hunger, thirst, music, dance, sing, toys, boredom: not ported.
        self.score_rest(&n, &mut best);
        // Fear, sorrow, pain, pleasure, cocoon and the rest: not ported.
        if best <= INTENTION_MIN_SCORE {
            self.set(Bhv::Move);
        }
    }
    /// AL_CalcScoreTypeA 0x158DC.
    fn type_a(v: i32, trig: i32) -> f32 {
        if v <= trig {
            return 0.;
        }
        (v as f32 - trig as f32) / (STATE_LIMIT - trig as f32)
    }
    /// AL_ScoreRandomize 0x1576C (one rand, not fused).
    fn randomize(&mut self, s: f32) -> f32 {
        let a = s * INTENTION_RANDOM_RATIO;
        let r = self.rand01();
        let b = s * (1. - INTENTION_RANDOM_RATIO);
        a + b * r
    }
    /// Akubi_Score 0x46B24.
    fn score_akubi(&mut self, sleepy: i32, best: &mut f32) {
        let mut s = 0.;
        let sl = sleepy as u32;
        let [t1, t2, t3] = AKUBI_TRIGGER;
        if self.interval[15] == 0 && sl > t1 {
            s = if sl < t2 {
                (sl - t1) as f32 / (t2 - t1) as f32
            } else if sl < t3 {
                1. - (sl - t2) as f32 / (t3 - t2) as f32
            } else {
                0.
            };
            s = self.randomize(s * MAX_SCORE);
        }
        let f31 = (10000 - sleepy) as f32 / 10000.;
        let x = 3.051_757_8e-5_f32.mul_add(self.rand() as f32, f31);
        self.interval[15] = (0.5 * 2400.0_f32).mul_add(x, 600.) as u32;
        if s > *best {
            self.set(Bhv::Akubi);
            *best = s;
        }
    }
    /// Groggy_Score 0x46A7C.
    fn score_groggy(&mut self, depth: i32, best: &mut f32) {
        let mut s = 0.;
        if depth as u32 > 500 {
            s = 0.5_f32.mul_add(Self::type_a(depth, 500), 0.5);
            s = self.randomize(s);
        }
        if s > *best {
            self.set(Bhv::Groggy);
            *best = s;
        }
    }
    /// Sleep_Score 0x469A4. The water-height skip (mwp f_DC above the Chao) is
    /// not ported because garden water height is not traced.
    fn score_sleep(&mut self, sleepy: i32, best: &mut f32) {
        let mut s = 0.;
        if sleepy as u32 > SLEEP_TRIGGER {
            s = self.randomize(Self::type_a(sleepy, SLEEP_TRIGGER as i32) * MAX_SCORE);
        }
        if s > *best {
            self.set(Bhv::GoToSleep);
            *best = s;
        }
    }
    /// Rest_Score 0x153D8.
    fn score_rest(&mut self, n: &life::NeedsView, best: &mut f32) {
        if *best >= 1. {
            return;
        }
        let mut s = 0.;
        let tire = i32::from(n.tire);
        let curiosity = i32::from(n.personality[0]);
        let calm = i32::from(n.personality[11]);
        let k = (curiosity - calm + 200) as f32 / 400.;
        let t = REST_TRIGGER[0] + (k * (REST_TRIGGER[1] - REST_TRIGGER[0]) as f32) as u32;
        if tire as u32 > t {
            s = self.randomize(Self::type_a(tire, t as i32) * MAX_SCORE);
        }
        if s > *best {
            self.set(Bhv::SitDown);
            self.set_next(Bhv::SitIdle);
            *best = s;
        }
    }

    fn can_wake(p: &Player) -> bool {
        let n = life::needs_view(p);
        i32::from(n.sleepy) < WAKE_SLEEPY && i32::from(n.sleep_depth) < WAKE_SLEEP_DEPTH
    }
    fn link(&mut self, n: u16) {
        self.motion.link(n, DEFAULT_LINK_FRAMES);
    }
    fn link_step(&mut self, n: u16, s: u16) {
        self.motion.link(n, f32::from(s));
    }
    fn end(&self) -> bool {
        self.motion.end_flag
    }
    fn stop(&self) -> bool {
        self.motion.stop_flag
    }

    fn run(&mut self, t: &MotionTable, aim: Option<&AimLists>, body: &mut Body) -> (i32, Drive) {
        let p = &mut *body.player;
        let posture = self.motion.posture(t);
        let r = match self.behaviour() {
            Bhv::Think => self.think(posture),
            Bhv::GoToSleep => {
                // Bhv_GoToSleep 0x46684: AL_GetFoundTree is not ported, so no tree.
                self.set(Bhv::Sleep);
                0
            }
            Bhv::Sleep => self.sleep(posture, p),
            Bhv::SitSleep => self.sit_sleep(p),
            Bhv::Groggy => self.groggy(posture, p),
            Bhv::Akubi => self.akubi(posture),
            Bhv::SitDown => self.sit_down(posture),
            Bhv::SitIdle => self.sit_idle(p),
            Bhv::StandUp => self.stand_up(posture),
            Bhv::Nade => self.nade(posture, p),
            Bhv::Move => {
                if self.pick_aim(aim, body.zone) {
                    let run = body.player.stats.get("run").map_or(0, |s| s.points);
                    self.choose_move(run.min(u32::from(u16::MAX)) as u16);
                    0
                } else {
                    // No local aim list: wandering is unavailable.
                    1
                }
            }
            Bhv::Walk | Bhv::Run | Bhv::RunFast | Bhv::Crawl => {
                return self.move_step(body);
            }
        };
        (r, Drive::Stop)
    }

    /// ALBHV_Think 0x47830.
    fn think(&mut self, posture: u8) -> i32 {
        if self.mode == 0 {
            if (1..=3).contains(&posture) {
                self.link_step(62, 35);
            } else {
                let r = self.rand01();
                if r < 0.15 {
                    self.link(3);
                } else if r < 0.5 {
                    self.link(0);
                } else if r < 0.6 {
                    self.link(238);
                } else if r < 0.7 {
                    self.link(239);
                } else {
                    self.link(1);
                }
            }
            self.eye(0);
            self.mouth(0);
            self.timer = i32::from(((121. * self.rand01()) as i32 + 60) as u16);
            self.mode += 1;
        }
        let old = self.timer;
        self.timer = old - 1;
        if old <= 0 {
            return 2;
        }
        0
    }

    /// SleepNeeds 0x467E0, once per need tick.
    fn sleep_needs(&mut self, p: &mut Player) {
        if !life::take_need_update(p) {
            return;
        }
        match self.sub_mode {
            0 => {
                life::adjust_need(p, Need::SleepDepth, ADD_SLEEP_DEPTH);
                if life::needs_view(p).sleep_depth >= 10000 {
                    self.sub_mode += 1;
                }
            }
            1 => {
                life::adjust_need(p, Need::Sleepy, -SUB_SLEEPY);
                let regain = i32::from(life::needs_view(p).personality[7]);
                life::adjust_need(p, Need::Tire, -SUB_TIRE_SLEEP * (regain + 300) / 300);
                if life::needs_view(p).sleepy <= 0 {
                    self.sub_mode += 1;
                }
            }
            _ => {
                life::adjust_need(p, Need::Sleepy, -SUB_SLEEPY);
                life::adjust_need(p, Need::SleepDepth, -SUB_SLEEP_DEPTH);
            }
        }
    }

    /// ALBHV_Sleep 0x45CA8.
    fn sleep(&mut self, posture: u8, p: &mut Player) -> i32 {
        let n = life::needs_view(p);
        let mut fall = false;
        match self.mode {
            0 => {
                if posture == 0 {
                    self.link(37);
                    self.mode = 1;
                } else if posture == 1 {
                    if n.tire > 7000 && self.rand01() < 0.5 {
                        self.set(Bhv::SitSleep);
                        return 0;
                    }
                    self.mode = 2;
                } else {
                    self.mode = 2;
                }
                self.free_work = 0;
            }
            1 => {
                if self.end() {
                    self.mode = 2;
                }
            }
            2 => fall = true,
            3 => fall = false,
            4 | 6 | 8 => {
                let old = self.timer;
                self.timer = old - 1;
                if old <= 0 || Self::can_wake(p) {
                    self.link(match self.mode {
                        4 => 26,
                        6 => 54,
                        _ => 57,
                    });
                    self.mode += 1;
                }
            }
            5 | 7 | 9 | 11 | 13 => {
                if self.end() {
                    self.mode = 2;
                }
            }
            10 | 12 => {
                let (m, nx) = if self.mode == 10 { (36, 11) } else { (33, 13) };
                if self.end() {
                    let old = self.timer;
                    self.timer = old - 1;
                    if old <= 0 {
                        self.link(m);
                        self.mode = nx;
                    }
                }
                if Self::can_wake(p) {
                    self.link(m);
                    self.mode = nx;
                }
            }
            15 => {
                self.link_step(30, 40);
                self.mode = 16;
                self.eye(1);
                self.mouth(3);
            }
            16 if self.stop() => return 1,
            _ => {}
        }
        if fall {
            match self.free_work {
                0 => {
                    let t = 100. * self.rand01();
                    if f32::from(n.personality[2]) > t {
                        self.link_step(27, 40);
                        self.free_work = 1;
                    } else {
                        self.link_step(18, 40);
                        self.free_work = 2;
                    }
                }
                1 => self.link(28),
                _ => self.link(19),
            }
            self.timer = 1000;
            self.eye(7);
            self.mode = 3;
        }
        if self.mode == 3 {
            let snore = match self.free_work {
                1 => 0.5,
                2 => 0.4,
                _ => 0.,
            };
            if snore > 0. && self.timer % 120 == 0 && self.rand01() < snore {
                self.rand01(); // which snore sound
            }
            let old = self.timer;
            self.timer = old - 1;
            if old <= 0 {
                let r = self.rand01();
                if r < 0.3 {
                    self.timer = 1000;
                    self.link(24);
                    self.mode = 4;
                } else if r < 0.6 {
                    self.timer = 1000;
                    if self.rand01() < 0.5 {
                        self.link(52);
                        self.mode = 6;
                    } else {
                        self.link(55);
                        self.mode = 8;
                    }
                } else if r < 0.9 {
                    let left = self.rand01() < 0.5;
                    self.timer = (4. * self.rand01()) as u32 as i32 + 2;
                    if left {
                        self.link(34);
                        self.mode = 10;
                    } else {
                        self.link(31);
                        self.mode = 12;
                    }
                } else {
                    self.timer = 1000;
                }
                if self.rand01() < 0.25 {
                    self.rand01(); // which rustle sound
                }
            }
            if Self::can_wake(p) {
                self.mode = 15;
            }
        }
        self.sleep_needs(p);
        0
    }

    /// ALBHV_SitSleep 0x45A58.
    fn sit_sleep(&mut self, p: &mut Player) -> i32 {
        match self.mode {
            0 => {
                let r = self.rand01();
                let t = f64::from(life::needs_view(p).tire) * 0.0001;
                if t > f64::from(r) {
                    self.link(20);
                    self.mode = 1;
                } else {
                    self.link(22);
                    self.mode = 2;
                    self.timer = 10.0_f32.mul_add(self.rand01(), 3.) as i32;
                }
                self.eye(7);
            }
            1 => {
                if Self::can_wake(p) {
                    self.link(62);
                    return 1;
                }
            }
            _ => {
                if self.end() {
                    let old = self.timer;
                    self.timer = old - 1;
                    if old <= 0 {
                        self.link(23);
                        self.timer = 10.0_f32.mul_add(self.rand01(), 3.) as i32;
                    }
                }
                if Self::can_wake(p) {
                    self.link(62);
                    return 1;
                }
            }
        }
        self.sleep_needs(p);
        0
    }

    /// Bhv_Groggy 0x45468.
    fn groggy(&mut self, posture: u8, p: &mut Player) -> i32 {
        if self.mode == 0 {
            match posture {
                1 => self.link(63),
                2 => self.link(18),
                _ => self.link(0),
            }
            self.eye(9);
            if self.rand01() < 0.3 {
                self.mouth(8);
            }
            self.mode += 1;
        }
        let old = self.timer;
        self.timer = old + 1;
        if old > 400 {
            self.timer = 0;
        }
        life::adjust_need(p, Need::SleepDepth, -2);
        if life::needs_view(p).sleep_depth == 0 {
            return 1;
        }
        0
    }

    /// Bhv_Akubi 0x455E4.
    fn akubi(&mut self, posture: u8) -> i32 {
        match self.mode {
            0 => {
                if posture == 0 {
                    if self.rand01() < 0.5 {
                        self.timer = 6.0_f32.mul_add(self.rand01(), 3.) as i32;
                        self.link(39);
                        self.mode = 1;
                        self.eye(3);
                        self.mouth(2);
                    } else {
                        self.link(84);
                        self.mode = 3;
                        self.eye(1);
                        self.mouth(2);
                    }
                } else {
                    let r = self.rand01();
                    if r < 0.3 {
                        self.timer = 6.0_f32.mul_add(self.rand01(), 2.) as i32;
                        self.link(15);
                        self.mode = 4;
                        self.eye(3);
                        self.mouth(2);
                    } else if r < 0.6 {
                        self.link(30);
                        self.mode = 6;
                        self.eye(1);
                        self.mouth(2);
                    } else {
                        self.link(21);
                        self.mode = 7;
                        self.eye(3);
                        self.mouth(8);
                    }
                }
            }
            1 | 4 => {
                if self.end() {
                    let old = self.timer;
                    self.timer = old - 1;
                    if old <= 0 {
                        let (m, next) = if self.mode == 1 { (41, 2) } else { (17, 5) };
                        self.link(m);
                        self.mode = next;
                    }
                }
            }
            2 | 3 | 5 | 6 | 7 => {
                if self.end() {
                    self.mode = 8;
                }
            }
            _ => return 1,
        }
        0
    }

    /// Bhv_SitDown 0x4213C.
    fn sit_down(&mut self, posture: u8) -> i32 {
        if self.mode == 0 {
            match posture {
                0 => self.link_step(47, 40),
                2 => self.link_step(42, 40),
                3 => self.link_step(42, 35),
                _ => return 1,
            }
            self.mode = 1;
            return 0;
        }
        i32::from(self.end())
    }

    /// Bhv_StandUp 0x42220.
    fn stand_up(&mut self, posture: u8) -> i32 {
        if self.mode == 0 {
            match posture {
                1 => self.link_step(48, 40),
                2 => self.link_step(38, 40),
                3 => self.link_step(38, 35),
                _ => return 1,
            }
            self.mode = 1;
            return 0;
        }
        i32::from(self.end())
    }

    /// Bhv_SitIdle 0x42308.
    fn sit_idle(&mut self, p: &mut Player) -> i32 {
        if self.mode == 0 {
            self.timer = i32::from(((501. * self.rand01()) as i32 + 120) as u16);
            let r = self.rand01();
            if r < 0.2 {
                self.link_step(62, 40);
            } else if r < 0.3 {
                self.link_step(65, 40);
            } else if r < 0.4 {
                self.link_step(63, 40);
            } else if r < 0.5 {
                self.link_step(64, 40);
            } else if r < 0.6 {
                self.link_step(65, 40);
            } else if r < 0.7 {
                self.link_step(66, 40);
            } else if r < 0.8 {
                self.timer = i32::from(((121. * self.rand01()) as i32 + 120) as u16);
                let pose = if self.rand01() < 0.5 { 67 } else { 68 };
                self.link_step(pose, 40);
            } else {
                self.timer = i32::from(((121. * self.rand01()) as i32 + 60) as u16);
                self.link_step(71, 40);
            }
            self.mode += 1;
            return 0;
        }
        let old = self.timer;
        self.timer = old - 1;
        if old <= 0 {
            let n = life::needs_view(p);
            let end = REST_END_TRIGGER;
            if n.hunger as u32 > end
                || n.sleepy as u32 > end
                || n.tedious as u32 > end
                || n.lonely as u32 > end
                || n.tire < 100
                || self.rand01() < 0.3
            {
                return 1;
            }
            self.mode = 0;
        }
        if life::take_need_update(p) {
            let regain = i32::from(life::needs_view(p).personality[7]) + 300;
            life::adjust_need(p, Need::Tire, -SUB_TIRE_REST * regain / 300);
            life::adjust_need(p, Need::Condition, ADD_CONDITION_REST * regain / 300);
        }
        0
    }

    /// Bhv_Nade 0x3FCA4.
    fn nade(&mut self, posture: u8, p: &mut Player) -> i32 {
        let relax = |p: &Player| i32::from(life::needs_view(p).mood[Mood::Relax as usize]);
        match self.mode {
            0 => {
                match posture {
                    1 | 3 => self.link_step(62, 20),
                    2 => self.link_step(42, 20),
                    _ => self.link_step(0, 20),
                }
                if relax(p) < 10 {
                    // AL_ParameterAddUserLike(tp, 1, NadeAddAttr): the like step
                    // is life::pet; the attribute shift belongs to the body module.
                    let _ = NADE_ADD_ATTR;
                    life::pet(p);
                }
                self.mode += 1;
            }
            1 => {
                if !self.end() {
                    return 0;
                }
                if life::needs_view(p).sleep_depth > 500 {
                    self.set(Bhv::Groggy);
                    return 0;
                }
                let pose = if posture == 1 || posture == 2 {
                    if self.rand01() < 0.5 { 65 } else { 66 }
                } else if self.rand01() < 0.5 {
                    250
                } else {
                    252
                };
                self.link_step(pose, 20);
                self.mode += 1;
                self.timer = 10;
                if self.rand01() < 0.3 {
                    self.rand01(); // which voice
                }
            }
            2 => {
                if self.end() {
                    self.timer -= 1;
                    if self.timer <= 0 {
                        self.touch_enabled = false;
                        self.touch = false;
                        self.mode += 1;
                    }
                }
                if relax(p) > 100 {
                    self.eye(3);
                    self.mouth(3);
                }
                let old = self.sub_timer;
                self.sub_timer = old + 1;
                if old >= 3 {
                    life::adjust_mood(p, Mood::Relax, 1);
                }
                if self.mode == 2 && !self.touch {
                    if posture == 1 {
                        self.link_step(12, 20);
                    } else if posture == 0 {
                        self.link_step(1, 20);
                    }
                    self.mode = 4;
                    self.timer = 0;
                    self.sub_timer = 0;
                }
            }
            3 => {
                let old = self.timer;
                self.timer = old + 1;
                if old > 10 {
                    self.touch_enabled = true;
                    return 1;
                }
            }
            _ => {
                let old = self.sub_timer;
                self.sub_timer = old + 1;
                if old >= 15 {
                    life::adjust_mood(p, Mood::Relax, -1);
                }
                if relax(p) < 10 {
                    self.touch_enabled = true;
                    return 1;
                }
            }
        }
        0
    }

    /// PickAim 0x16AE4 with list 1 of the Chao's zone.
    fn pick_aim(&mut self, aim: Option<&AimLists>, zone: &str) -> bool {
        let Some(cells) = aim
            .and_then(|lists| lists.0.get(zone))
            .filter(|c| !c.is_empty())
        else {
            return false;
        };
        let count = cells.len() as f32;
        let i = ((count - 1.0e-5) * (self.rand() as f32 * 3.051_757_8e-5)) as usize;
        let (cx, cz) = cells[i.min(cells.len() - 1)];
        let x = 10. * 3.051_757_8e-5_f32.mul_add(self.rand() as f32, (cx - 16) as f32);
        let z = 10. * 3.051_757_8e-5_f32.mul_add(self.rand() as f32, (cz - 20) as f32);
        // Native units to world units (× 0.1).
        self.aim = Some((f64::from(x) * 0.1, f64::from(z) * 0.1));
        true
    }
    /// AL_ChooseMove 0x4B5C0.
    fn choose_move(&mut self, run: u16) {
        if run < SKILL_WALK {
            self.set(Bhv::SitDown);
            self.set_next(Bhv::Crawl);
        } else if run < SKILL_RUN {
            self.set(Bhv::StandUp);
            self.set_next(Bhv::Walk);
        } else if run < SKILL_RUN2 {
            self.set(Bhv::StandUp);
            self.set_next(Bhv::Run);
        } else {
            self.set(Bhv::StandUp);
            self.set_next(Bhv::RunFast);
        }
    }
    /// Bhv_Walk / Bhv_Run / Bhv_RunFast / Bhv_Crawl and AL_MoveCommon 0x4BC10.
    fn move_step(&mut self, body: &mut Body) -> (i32, Drive) {
        let bhv = self.behaviour();
        let (motion, gait) = match bhv {
            Bhv::Walk => (100, Gait::Walk),
            Bhv::Run => (101, Gait::Run),
            Bhv::RunFast => (102, Gait::Run),
            _ => (115, Gait::Crawl),
        };
        if self.mode == 0 {
            self.link(motion);
            self.mode += 1;
        }
        match bhv {
            Bhv::Walk => {
                let old = self.timer;
                self.timer = old + 1;
                // A trip (fn_60_3DDE8) is not ported; the draw is kept.
                if old % 85 == 1 {
                    self.rand01();
                }
            }
            Bhv::Run => {
                let old = self.timer;
                self.timer = old + 1;
                if old % 100 == 0 {
                    self.rand01();
                }
            }
            Bhv::Crawl if self.frame_count.is_multiple_of(120) && self.rand01() < 0.3 => {
                self.rand01();
            }
            _ => {}
        }
        let p = &mut *body.player;
        let Some((x, z)) = self.aim else {
            return (1, Drive::Stop);
        };
        // AL_Dist2FromAim < 36 in native units = 0.36 in world units.
        let (dx, dz) = ((x - p.x) * 10., (z - p.z) * 10.);
        if dx * dx + dz * dz < 36. {
            return (1, Drive::Stop);
        }
        if life::take_need_update(p) {
            life::adjust_need(p, Need::Tedious, -SUB_TEDIOUS_WALK);
            life::adjust_need(p, Need::Tire, ADD_TIRE_WALK);
        }
        (0, Drive::Toward { x, z, gait })
    }

    pub fn public(&self) -> Value {
        let (name, address) = self.behaviour().native();
        json!({"fn":name,"address":address,"mode":self.mode,"motion":self.motion.public(),
            "face":{"eye":self.face.eye.map(|(n,_)|n),"mouth":self.face.mouth.map(|(n,_)|n)},
            "aim":self.aim.map(|(x,z)|json!({"x":x,"z":z})),"frame":self.frame_count,
            "rules":RULES,"adapted":["per-resident-rand","server-movement","pet-is-one-touch"]})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic table for engine tests only (not game data): every state is a
    /// 10-frame LOOP at speed 1, standing, with no next.
    fn table_with(edits: &[(usize, MotionRow)]) -> MotionTable {
        let base = MotionRow {
            mode: 0,
            start: 0.,
            end: 10.,
            spd: 1.,
            next: -1,
            link_step: -40,
            posture: 0,
        };
        let mut rows = vec![base; MotionTable::STATES];
        for (i, row) in edits {
            rows[*i] = *row;
        }
        MotionTable::from_rows(rows).unwrap()
    }

    #[test]
    fn motion_modes_follow_motion_control() {
        let row = |mode, next| MotionRow {
            mode,
            start: 0.,
            end: 4.,
            spd: 1.,
            next,
            link_step: -10,
            posture: 0,
        };
        let t = table_with(&[
            (1, row(3, -1)),
            (2, row(5, 3)),
            (4, row(7, -1)),
            (5, row(4, 6)),
        ]);
        // STOP holds at end and raises End and Stop.
        let mut m = Motion::init(&t, 1);
        for _ in 0..4 {
            m.control(&t);
        }
        assert!(m.end_flag && m.stop_flag && m.held());
        // SKIP switches to next at once.
        let mut m = Motion::init(&t, 2);
        for _ in 0..4 {
            m.control(&t);
        }
        assert!(m.end_flag);
        assert_eq!(m.num, 3);
        // BACK ping-pongs.
        let mut m = Motion::init(&t, 4);
        for _ in 0..4 {
            m.control(&t);
        }
        assert!(m.end_flag && m.spd < 0.);
        // LINK blends to next over |link_step| frames; no End during the link.
        let mut m = Motion::init(&t, 5);
        for _ in 0..4 {
            m.control(&t);
        }
        assert!(m.end_flag && m.link.is_some());
        for _ in 0..9 {
            m.control(&t);
            assert!(!m.end_flag);
        }
        m.control(&t);
        assert_eq!(m.num, 6);
        // SetMotionLink is a 15-frame blend.
        let mut m = Motion::init(&t, 0);
        m.link(9, 15.);
        for _ in 0..14 {
            m.control(&t);
        }
        assert_eq!(m.num, 0);
        m.control(&t);
        assert_eq!(m.num, 9);
    }

    #[test]
    fn motion_table_reads_from_a_rel_layout() {
        // Synthetic REL (not game data): header, 6 sections, .data at 0x100.
        let data = 0x100usize;
        let mut rel = vec![0u8; data + 0x3F870 + MotionTable::STATES * 0x1C];
        rel[0..4].copy_from_slice(&0x3Cu32.to_be_bytes());
        rel[0x0C..0x10].copy_from_slice(&7u32.to_be_bytes());
        rel[0x10..0x14].copy_from_slice(&0x40u32.to_be_bytes());
        rel[0x40 + 40..0x40 + 44].copy_from_slice(&(data as u32).to_be_bytes());
        let len = (rel.len() - data) as u32;
        rel[0x40 + 44..0x40 + 48].copy_from_slice(&len.to_be_bytes());
        for i in 0..MotionTable::STATES {
            let at = data + 0x3F870 + i * 0x1C;
            rel[at + 4..at + 6].copy_from_slice(&3i16.to_be_bytes());
            rel[at + 6..at + 8].copy_from_slice(&1i16.to_be_bytes());
            rel[at + 8..at + 12].copy_from_slice(&(-1i32).to_be_bytes());
            rel[at + 12..at + 16].copy_from_slice(&(-40i32).to_be_bytes());
            rel[at + 20..at + 24].copy_from_slice(&30f32.to_bits().to_be_bytes());
            rel[at + 24..at + 28].copy_from_slice(&0.5f32.to_bits().to_be_bytes());
        }
        let table = MotionTable::from_rel(&rel).unwrap();
        let row = table.rows()[621];
        assert_eq!(
            (row.mode, row.posture, row.next, row.link_step),
            (3, 1, -1, -40)
        );
        assert_eq!((row.start, row.end, row.spd), (0., 30., 0.5));
        rel[3] = 0;
        assert!(MotionTable::from_rel(&rel).is_err());
    }

    #[test]
    fn score_helpers_match_native_formulas() {
        assert_eq!(Brain::type_a(8000, 8000), 0.);
        assert!((Brain::type_a(9000, 8000) - 0.5).abs() < 1e-6);
        let t = table_with(&[]);
        let mut b = Brain::new(&t, 1);
        for _ in 0..1000 {
            let s = b.randomize(1.);
            assert!((0.4..=1.).contains(&s));
        }
        const { assert!(FRAMES_PER_SECOND > 59.9 && FRAMES_PER_SECOND < 60.) };
    }
}
