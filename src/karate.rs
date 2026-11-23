//! A native Chao Karate round (ChaoStgKarate fn_72_10E0C per fighter per
//! frame), assembled from the ported routines in `competition_native::karate`
//! and the client crate's verified ports (motion handlers, MOV_Control,
//! fn_8002F1C0, fn_72_F5B0). Addresses are ChaoStgKarate `.text` offsets.
//!
//! Known shortcuts (marked `shortcut:` below) are the parts not ported yet:
//! the floor collision (main.dol MOV_DetectCollision is replaced by landing on
//! the arena land mesh), the stagger motion fn_72_DBE8 and fn_72_D440 (run as
//! their plain clips), the Chao BodyPos (the fighter position is used) and who
//! moves a 0x4000-flagged fighter into state 6 (done at once).
use crate::behaviour::{Motion, MotionTable};
use crate::competition_native::{
    MslRand,
    karate::{self, Descriptor, Fighter, StateRecord, Tables, flag, state},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// ChaoStgKarate .data 0x168A0: link step, Chao motion number, event frames.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionRow {
    pub link_step: u16,
    pub motion: i32,
    pub events: Vec<f32>,
}

/// The karate arena floor: triangles of the arena land table
/// (objLandTableChaoKarate) in native units.
#[derive(Clone, Debug, Default)]
pub struct Arena {
    pub triangles: Vec<[[f32; 3]; 3]>,
}

impl Arena {
    /// The local arena collision export (native ×0.1 units).
    pub fn from_value(v: &Value) -> Result<Self, String> {
        let scale = v["scale"]
            .as_f64()
            .filter(|s| *s > 0.)
            .ok_or("arena scale")? as f32;
        let mut triangles = vec![];
        for mesh in v["meshes"].as_array().ok_or("arena meshes")? {
            if mesh["flags"]["sf_solid"] != true {
                continue;
            }
            let verts: Vec<[f32; 3]> = mesh["vertices"]
                .as_array()
                .ok_or("arena vertices")?
                .iter()
                .map(|p| {
                    let c = |k: usize| p[k].as_f64().unwrap_or(0.) as f32 / scale;
                    [c(0), c(1), c(2)]
                })
                .collect();
            for t in mesh["triangles"].as_array().ok_or("arena triangles")? {
                let idx = |k: usize| t[k].as_u64().unwrap_or(0) as usize;
                let (a, b, c) = (idx(0), idx(1), idx(2));
                if a < verts.len() && b < verts.len() && c < verts.len() {
                    triangles.push([verts[a], verts[b], verts[c]]);
                }
            }
        }
        Ok(Self { triangles })
    }

    /// Highest upward-facing floor under (x, z), if any.
    fn floor(&self, x: f32, z: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for [a, b, c] in &self.triangles {
            let d = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
            if d.abs() < 1e-6 {
                continue;
            }
            let u = ((x - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (z - a[2])) / d;
            let v = ((b[0] - a[0]) * (z - a[2]) - (x - a[0]) * (b[2] - a[2])) / d;
            if u < 0. || v < 0. || u + v > 1. {
                continue;
            }
            let y = a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1]);
            best = Some(best.map_or(y, |m| m.max(y)));
        }
        best
    }
}

#[derive(Clone, Debug, Default)]
struct Aux {
    motion: i8,
    step: i8,
    event: i8,
    bits: u8,
    row: i32,
    timer: u16,
    knock: [f32; 3],
}

const HIT_CHECK: u8 = 8;
const IMPACT: u8 = 0x10;
const TAKE_HIT: u16 = 0x4000;

struct Side {
    me: Fighter,
    rec: StateRecord,
    aux: Aux,
    motion: Motion,
    pos: [f32; 3],
    yaw: i32,
    vel: [f32; 3],
    acc: [f32; 3],
    mov_flags: u16,
    kind: i16,
}

fn sin_cos(angle: i32) -> (f32, f32) {
    // shortcut: njSin/njCos table lookups replaced by the exact functions.
    let r = angle as f64 * std::f64::consts::TAU / 65536.;
    (r.sin() as f32, r.cos() as f32)
}

/// fn_8002F1C0: turn toward the opponent by at most `step`.
fn turn_toward(s: &mut Side, target: [f32; 3], step: i32) {
    let a = (target[0] - s.pos[0]).atan2(target[2] - s.pos[2]);
    let aim = (10430.38043493439f64 * f64::from(a)) as i32;
    let (y, b) = (s.yaw & 0xffff, aim & 0xffff);
    let d = (b - y) as i16 as i32;
    s.yaw = if (-step..=step).contains(&d) {
        b
    } else if d & 0x8000 != 0 {
        (y - step) & 0xffff
    } else {
        (y + step) & 0xffff
    };
}

/// fn_72_B620: start motion `id`.
fn set_motion(s: &mut Side, id: i8) {
    s.rec.flags &= !0x8000;
    s.aux.bits &= !(HIT_CHECK | IMPACT);
    s.aux.motion = id;
    s.aux.step = 0;
    s.aux.event = 0;
}

fn link(s: &mut Side, rows: &[MotionRow], row: usize, step: u16) {
    if let Some(r) = rows.get(row) {
        s.motion.link_step(r.motion.max(0) as u16, step);
    }
}

fn reached(rows: &[MotionRow], aux: &Aux, frame: f32) -> bool {
    frame
        >= rows
            .get(aux.row as usize)
            .and_then(|r| r.events.get(aux.event as usize).copied())
            .unwrap_or(-1.0)
}

/// fn_72_E868: the motion handler for the current motion id.
fn motion_handler(
    s: &mut Side,
    rows: &[MotionRow],
    table: &MotionTable,
    distance: f32,
    opponent: [f32; 3],
) {
    let linking = s.motion.linking();
    let frame = s.motion.frame;
    let ended = s.motion.ended();
    let stopped = s.motion.stopped();
    match s.aux.motion {
        // fn_72_C0C4 (motions 4, 7) and fn_72_C3B4 (5, 8).
        4 | 7 | 5 | 8 => {
            let light = matches!(s.aux.motion, 4 | 7);
            match (s.aux.step, light) {
                (0, _) => {
                    s.vel = [0.; 3];
                    s.acc = [0.; 3];
                    s.aux.row = i32::from(s.aux.motion);
                    let r = s.aux.row as usize;
                    link(s, rows, r, rows[r].link_step);
                    s.rec.flags &= !0x8000;
                    if !light {
                        s.aux.bits &= !IMPACT;
                    }
                    s.aux.step = 1;
                }
                (1, _) if !linking => {
                    if light {
                        s.aux.bits |= HIT_CHECK;
                    }
                    s.aux.step = 2;
                }
                (2, false) if reached(rows, &s.aux, frame) => {
                    s.aux.bits |= HIT_CHECK;
                    s.aux.event += 1;
                    s.aux.step = 3;
                }
                (2, true) | (3, false) if reached(rows, &s.aux, frame) => {
                    s.aux.bits &= !HIT_CHECK;
                    s.aux.bits |= IMPACT;
                    s.aux.event += 1;
                    s.aux.step += 1;
                }
                (3, true) | (4, false) if reached(rows, &s.aux, frame) => {
                    s.aux.bits &= !IMPACT;
                    s.aux.event += 1;
                    s.aux.step += 1;
                }
                (4, true) | (5, false) if stopped => {
                    if s.aux.row == 5 {
                        s.aux.row = 1;
                        if let Some(r) = rows.get(1) {
                            s.motion.skip(table, r.motion.max(0) as u16);
                        }
                    }
                    s.rec.flags |= 0x8000;
                    s.aux.step += 1;
                }
                _ => {}
            }
        }
        // fn_72_D110 (3). shortcut: fn_72_D440 (37) runs the same walk.
        3 | 37 => match s.aux.step {
            0 => {
                s.aux.row = i32::from(s.aux.motion);
                let r = s.aux.row as usize;
                link(s, rows, r, rows[r].link_step);
                s.rec.flags &= !0x8000;
                s.aux.step = 1;
            }
            1 if !linking => {
                s.aux.timer = 0x14;
                s.aux.step = 2;
            }
            2 => {
                turn_toward(s, opponent, 0x100);
                let (sin, cos) = sin_cos(s.yaw);
                s.acc[0] += 0.05 * sin;
                s.acc[2] += 0.05 * cos;
                s.vel[0] *= 0.95;
                s.vel[2] *= 0.95;
                if distance < 3.5 {
                    s.acc[0] = 0.;
                    s.acc[2] = 0.;
                    s.vel[0] = 0.;
                    s.vel[2] = 0.;
                    s.aux.step = 3;
                }
                s.aux.timer = s.aux.timer.saturating_sub(1);
            }
            3 => {
                if s.aux.timer == 0 {
                    s.rec.flags |= 0x8000;
                    s.aux.step = 4;
                } else {
                    s.aux.timer -= 1;
                }
            }
            _ => {}
        },
        // fn_72_C7A8 (hurt 9-11, 29-31).
        9..=11 | 29..=31 => {
            let damp = |v: &mut f32, k: f32| {
                if *v != 0. {
                    *v *= k;
                    if v.abs() < 0.05 {
                        *v = 0.;
                    }
                }
            };
            if s.aux.step == 0 {
                s.aux.row = i32::from(s.aux.motion);
                let r = s.aux.row as usize;
                link(s, rows, r, rows[r].link_step);
                s.rec.flags &= !0x8000;
                s.vel = [0.; 3];
                s.aux.timer = 5;
                s.aux.step = 1;
            }
            match s.aux.step {
                1 if !linking => s.aux.step = 2,
                2 => {
                    let k = f32::from(s.aux.timer) / 5.;
                    let (h, v) = match s.aux.motion {
                        9 | 29 => (0.2 * k, 0.16 * k),
                        10 | 30 => (0.35 * k, 0.2 * k),
                        _ => (0.55 * k, 0.2 * k),
                    };
                    s.acc[0] += s.aux.knock[0] * h;
                    s.acc[1] += v;
                    s.acc[2] += s.aux.knock[2] * h;
                    damp(&mut s.vel[0], 0.85);
                    damp(&mut s.vel[2], 0.85);
                    let old = s.aux.timer;
                    s.aux.timer = s.aux.timer.wrapping_sub(1);
                    if old == 0 {
                        s.aux.timer = 0x78;
                        s.aux.step = 3;
                    }
                }
                3 => {
                    damp(&mut s.vel[0], 0.75);
                    damp(&mut s.vel[2], 0.75);
                    if ended {
                        s.vel[0] = 0.;
                        s.vel[2] = 0.;
                        if s.rec.flags & flag::KNOCKED_OUT != 0 {
                            s.aux.step += 2;
                        } else {
                            s.aux.row = match s.aux.motion {
                                9 => 1,
                                29 => 0x13,
                                10 | 30 => 0x22,
                                _ => 0x23,
                            };
                            let r = s.aux.row as usize;
                            link(s, rows, r, 0x14);
                            s.aux.step = 4;
                        }
                    }
                }
                4 if s.rec.flags & 0x8000 == 0 && !linking => {
                    let done = match s.aux.row {
                        1 | 0x13 => ended,
                        _ => stopped,
                    };
                    if done {
                        s.rec.flags |= 0x8000;
                    }
                }
                _ => {}
            }
        }
        // fn_72_B720 (ready/idle) and, as a shortcut, the stagger clips
        // (fn_72_DBE8 for 12/38) and any other id: link, then end with the clip.
        id => match s.aux.step {
            0 => {
                s.aux.row = i32::from(id);
                let r = (s.aux.row as usize).min(rows.len().saturating_sub(1));
                link(s, rows, r, rows[r].link_step);
                s.rec.flags &= !0x8000;
                s.aux.step = 1;
            }
            1 if !linking => s.aux.step = 2,
            2 if matches!(id, 12 | 38) && (ended || stopped) => {
                s.rec.flags |= 0x8000;
                s.aux.step = 3;
            }
            _ => {}
        },
    }
}

/// main.dol MOV_Control (0x8002F448).
fn mov_control(s: &mut Side) {
    s.acc[1] += -0.05; // MOV_SetGravity(-0.05), fn_72_11038 0x1112C
    for k in 0..3 {
        s.vel[k] += s.acc[k];
    }
    s.acc = [0.; 3];
    let f = s.mov_flags;
    if f & 8 != 0 {
        if f & 0x10 != 0 {
            if (s.vel[0] * s.vel[0] + s.vel[1] * s.vel[1] + s.vel[2] * s.vel[2]).sqrt() <= 0.01 {
                return;
            }
            s.mov_flags &= !0x10;
        } else if f & 0x400 != 0
            && f & 0x800 == 0
            && (s.vel[0] * s.vel[0] + s.vel[2] * s.vel[2]).sqrt() < 0.01
        {
            s.vel = [0.; 3];
            s.mov_flags |= 0x10;
            return;
        }
    }
    for k in 0..3 {
        s.pos[k] += s.vel[k];
    }
}

/// shortcut: main.dol MOV_DetectCollision is not ported; a fighter lands on
/// the arena floor under it (velocity y cleared, grounded flag 0x400).
fn land(s: &mut Side, arena: &Arena) {
    match arena.floor(s.pos[0], s.pos[2]) {
        Some(y) if s.pos[1] <= y => {
            s.pos[1] = y;
            s.vel[1] = s.vel[1].max(0.);
            s.mov_flags |= 0x400 | 8;
        }
        _ => s.mov_flags &= !0x400,
    }
}

/// The result of one round.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    /// 0 = the owner's Chao, 1 = the opponent, 2 = draw (fn_72_F4CC).
    pub winner: i16,
    pub reason: &'static str,
    pub frames: u32,
    pub hp: [f32; 2],
    pub opponent_row: usize,
}

/// One native round. `owner_skills` are the owner Chao's raw skill points
/// (karate stage AL_ParameterGetSkill), `child` is Chao type 2.
#[allow(clippy::too_many_arguments)]
pub fn run_round(
    tables: &Tables,
    rows: &[MotionRow],
    motion_table: &MotionTable,
    arena: &Arena,
    rng: &mut MslRand,
    owner_skills: [u16; 7],
    child: bool,
    class: u8,
    round: u8,
) -> Option<Round> {
    let row = karate::opponent_row(&tables.descriptors, class, round, rng)?;
    let d: Descriptor = tables.descriptors[row];
    let s = tables.skills.get(usize::from(d.skills))?;
    let opp_skills = [s[0], s[1], s[2], s[3], s[4], s[5], s[6]];
    // shortcut: the owner's work +6 kind is not traced; 0 (no descriptor kind).
    let entrants = [
        (owner_skills, false, child, 0),
        (opp_skills, true, false, d.kind),
    ];
    let mut round = fight(rows, motion_table, arena, rng, entrants);
    round.opponent_row = row;
    Some(round)
}

/// One native round between two given fighters: (skill points, computer
/// opponent, child, descriptor kind) for the left and the right side.
/// shortcut: for two players' Chao both sides are set up as the owner's side
/// (no computer spread, kind 0); the native 2-player setup is not traced.
pub fn fight(
    rows: &[MotionRow],
    motion_table: &MotionTable,
    arena: &Arena,
    rng: &mut MslRand,
    entrants: [([u16; 7], bool, bool, i16); 2],
) -> Round {
    let mut make =
        |(skills, cpu, child, kind): ([u16; 7], bool, bool, i16), x: f32, yaw: i32| Side {
            me: karate::fighter(skills, cpu, child, false, rng),
            rec: StateRecord {
                state: state::READY,
                ..Default::default()
            },
            aux: Aux::default(),
            motion: Motion::start(motion_table, 0),
            // fn_72_5020: start positions .data 0x1888 (±5, 8, 19), yaw ±0x4000.
            pos: [x, 8., 19.],
            yaw,
            vel: [0.; 3],
            acc: [0.; 3],
            mov_flags: 8,
            kind,
        };
    let mut sides = [
        make(entrants[0], -5., 0x4000),
        make(entrants[1], 5., -0x4000),
    ];
    let limit = karate::ROUND_SECONDS * karate::FRAMES_PER_SECOND;
    let mut frame = 0;
    let mut knocked_out: Option<usize> = None;
    while frame < limit && knocked_out.is_none() {
        frame += 1;
        for i in 0..2 {
            let o = 1 - i;
            let opp_pos = sides[o].pos;
            let opp_power = sides[o].me.power;
            // fn_72_13258: distance between the two fighters.
            let dist = {
                let (a, b) = (sides[i].pos, opp_pos);
                ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
            };
            state_handler(&mut sides, i, dist, opp_pos, opp_power, rng);
            let lead = karate::leader([
                (sides[0].me.hp, sides[0].me.hp_max),
                (sides[1].me.hp, sides[1].me.hp_max),
            ]);
            let s = &mut sides[i];
            resolve_hit(&mut s.rec);
            karate::zeal_step(&mut s.me, &mut s.rec, i as i16, lead, rng);
            motion_handler(s, rows, motion_table, dist, opp_pos);
            s.motion.step(motion_table);
            mov_control(s);
            land(s, arena);
            // fn_72_F5B0 tail: a fighter below height 7 is out.
            if s.pos[1] < 7. {
                s.rec.flags |= flag::KNOCKED_OUT;
            }
            if s.rec.flags & flag::KNOCKED_OUT != 0 {
                knocked_out = Some(i);
            }
            // 0x10F60: the frame counter wraps after 60.
            s.rec.frame = if s.rec.frame >= 60 {
                0
            } else {
                s.rec.frame + 1
            };
            if knocked_out.is_some() {
                break;
            }
        }
    }
    let hp = [
        (sides[0].me.hp, sides[0].me.hp_max),
        (sides[1].me.hp, sides[1].me.hp_max),
    ];
    let winner = match knocked_out {
        Some(k) => 1 - k as i16,
        None => karate::leader(hp),
    };
    Round {
        winner,
        reason: if knocked_out.is_some() {
            "knockout"
        } else {
            "time-limit"
        },
        frames: frame,
        hp: [sides[0].me.hp, sides[1].me.hp],
        opponent_row: 0,
    }
}

const SLOW_CHILD: u16 = 1;
const D6: u16 = 8;

/// fn_72_EC10: a checked blow becomes the hurt state (6) once the attacker's
/// impact sets 0x4000, a miss becomes the dodge (5). The stage-phase test at
/// 0xEC24 compares cntlzw>>5 with 11 and never passes.
fn resolve_hit(rec: &mut StateRecord) {
    if rec.flags & flag::CHECKED == 0 {
        return;
    }
    let clear = flag::CHECKED | flag::HIT | flag::ATTACK_HIT | TAKE_HIT;
    if !matches!(rec.state, 2 | 3 | 4 | 7) {
        rec.flags &= !(TAKE_HIT | flag::CHECKED | flag::HIT);
    } else if rec.flags & flag::HIT != 0 {
        if rec.flags & TAKE_HIT != 0 {
            rec.flags &= !clear;
            rec.state = state::HURT;
            rec.step = 0;
        }
    } else {
        rec.flags &= !clear;
        rec.state = 5;
        rec.step = 0;
    }
}

fn rand_unit(rng: &mut MslRand) -> f32 {
    rng.rand() as f32 * 3.051_757_8e-5
}

/// fn_72_10E0C jump table .data 0x17348: 2 FAB8, 3 FCC8, 4 FE9C, 5 10AD4,
/// 6 1010C, 7 106F0.
fn state_handler(
    sides: &mut [Side; 2],
    i: usize,
    dist: f32,
    opp_pos: [f32; 3],
    opp_power: f32,
    rng: &mut MslRand,
) {
    let o = 1 - i;
    match sides[i].rec.state {
        // fn_72_FAB8: face the opponent, ready motion, charge.
        state::READY => {
            let s = &mut sides[i];
            turn_toward(s, opp_pos, 0x100);
            if s.rec.step == 0 {
                let id = if s.me.flags & SLOW_CHILD != 0 {
                    s.me.flags |= 2;
                    0x24
                } else if s.me.luck < 500. {
                    1
                } else {
                    2
                };
                set_motion(s, id);
                s.rec.step = 1;
            }
            karate::ready_step(&mut s.me, &mut s.rec, rng);
        }
        // fn_72_FCC8: approach, then pick the attack.
        state::CHOOSE => {
            let s = &mut sides[i];
            if s.rec.step == 0 {
                set_motion(
                    s,
                    if s.me.flags & SLOW_CHILD != 0 {
                        0x25
                    } else {
                        3
                    },
                );
                s.aux.bits &= !IMPACT;
                s.rec.step = 1;
            }
            if s.rec.flags & flag::MOTION_END != 0 {
                let quick = s.me.flags & D6 == 0 && rand_unit(rng) >= 0.5;
                let id = match (quick, s.me.power > 1600.) {
                    (false, true) => 8,
                    (false, false) => 7,
                    (true, true) => 5,
                    (true, false) => 4,
                };
                s.aux.bits &= !IMPACT;
                set_motion(s, id);
                s.rec.state = state::ATTACK;
                s.rec.step = 0;
            }
        }
        // fn_72_FE9C.
        state::ATTACK => match sides[i].rec.step {
            0 => sides[i].rec.step = 1,
            1 if sides[i].aux.bits & HIT_CHECK != 0 => {
                let (acc, eva, kind) = (sides[i].me.accuracy, sides[o].me.evasion, sides[o].kind);
                let hit = karate::hit_check(&mut sides[o].rec, eva, kind, acc, dist, rng);
                if hit {
                    sides[i].rec.flags |= flag::ATTACK_HIT;
                } else {
                    sides[i].rec.flags &= !flag::ATTACK_HIT;
                }
                sides[i].rec.step = 2;
            }
            2 if sides[i].aux.bits & IMPACT != 0 => {
                sides[i].me.charge = 0.;
                if sides[i].rec.flags & flag::ATTACK_HIT != 0 {
                    sides[o].rec.flags |= TAKE_HIT;
                }
                sides[i].rec.timer = 0x1E;
                sides[i].rec.step = 3;
            }
            3 if sides[i].rec.flags & flag::MOTION_END != 0 => {
                let s = &mut sides[i];
                karate::attack_end(&mut s.me, &mut s.rec);
            }
            _ => {}
        },
        // fn_72_10AD4: dodge, sliding to a stop, back to ready at height 8.
        5 => {
            let s = &mut sides[i];
            if s.rec.step == 0 {
                set_motion(s, if s.me.flags & 2 != 0 { 0x26 } else { 0xC });
                s.rec.timer = 0x78;
                s.rec.step = 1;
            }
            if s.vel[0] != 0. {
                s.vel[0] *= 0.2;
                if s.vel[0] < 0.05 {
                    s.vel[0] = 0.;
                }
            }
            if s.vel[2] != 0. {
                s.vel[2] *= 0.2;
                if s.vel[2] < 0.05 {
                    s.vel[2] *= 0.2; // 0x10C18 multiplies again where x zeroes
                }
            }
            if s.rec.flags & flag::MOTION_END != 0 {
                s.vel[0] = 0.;
                s.vel[2] = 0.;
                s.pos[1] = 8.;
                s.rec.state = state::READY;
                s.rec.step = 0;
            }
        }
        // fn_72_1010C.
        state::HURT => {
            let s = &mut sides[i];
            match s.rec.step {
                0 => {
                    let kind = s.kind;
                    // 0x103B0 reads the give-up flag before take_hit clears it.
                    let heavy = s.rec.flags & flag::GIVE_UP != 0 || s.me.flags & 2 != 0;
                    let damage =
                        karate::take_hit(&mut s.me, &mut s.rec, opp_power, kind, true, rng);
                    let id = match (damage < 300., damage < 700., heavy) {
                        (true, _, true) => 0x1D,
                        (true, _, false) => 9,
                        (false, true, true) => 0x1E,
                        (false, true, false) => 0xA,
                        (false, false, true) => 0x1F,
                        (false, false, false) => 0xB,
                    };
                    set_motion(s, id);
                    // 0x104A0: knock = unit(me - opponent).
                    let v = [
                        s.pos[0] - opp_pos[0],
                        s.pos[1] - opp_pos[1],
                        s.pos[2] - opp_pos[2],
                    ];
                    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                    s.aux.knock = if len > 0. {
                        v.map(|c| c / len)
                    } else {
                        [0.; 3]
                    };
                }
                1 => karate::hurt_step(&mut s.me, &mut s.rec, rng),
                2 => {
                    s.rec.timer = s.rec.timer.wrapping_sub(1);
                    if s.rec.timer == 0 {
                        s.rec.state = state::READY;
                        s.rec.step = 0;
                    }
                }
                _ => {}
            }
        }
        // fn_72_106F0: out of zeal; sulk motions until zeal is back over 980
        // (only the cheering branch of fn_72_EDE8 refills it).
        state::STAGGER => {
            let s = &mut sides[i];
            match s.rec.step {
                0 => {
                    set_motion(s, 0x14);
                    s.rec.timer = (60. * (1. + rand_unit(rng))) as i32 as u16;
                    s.me.charge = 0.;
                    s.rec.step = 1;
                }
                1 => {
                    s.rec.timer = s.rec.timer.wrapping_sub(1);
                    if s.rec.timer == 0 {
                        let u = rand_unit(rng);
                        let id = if s.kind == 1 {
                            0x19
                        } else if u < 0.2 {
                            0x15
                        } else if u < 0.4 {
                            0x1C
                        } else if u < 0.5 {
                            0x18
                        } else if u < 0.6 {
                            0x1A
                        } else if s.me.flags & SLOW_CHILD != 0 {
                            0x15
                        } else if u < 0.7 {
                            0x17
                        } else if u < 0.8 {
                            0x1B
                        } else {
                            0x1C
                        };
                        set_motion(s, id);
                        s.rec.step = 2;
                    }
                }
                2 if s.me.zeal > 980. => {
                    s.me.charge = 0.;
                    s.rec.flags &= !(flag::GIVE_UP | 0x2000);
                    s.rec.state = state::READY;
                    s.rec.step = 0;
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// Local native karate data (never committed): the owner's exported tables,
/// the karate motion table and the arena land mesh.
#[derive(Clone, Debug)]
pub struct Data {
    pub tables: Tables,
    pub rows: Vec<MotionRow>,
    pub arena: Arena,
}

impl Data {
    /// `imports/competition-native.json`, `imports/karate-motions.json`,
    /// `imports/karate-arena-collision.json`; `None` when any is missing.
    pub fn load(imports: &std::path::Path) -> Result<Option<Self>, String> {
        let read = |name: &str| -> Result<Option<Value>, String> {
            let path = imports.join(name);
            if !path.is_file() {
                return Ok(None);
            }
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
            serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| format!("{name}: {e}"))
        };
        let (Some(native), Some(motions), Some(arena)) = (
            read("competition-native.json")?,
            read("karate-motions.json")?,
            read("karate-arena-collision.json")?,
        ) else {
            return Ok(None);
        };
        let tables: Tables = serde_json::from_value(native["karate"].clone())
            .map_err(|e| format!("competition-native.json karate: {e}"))?;
        if !tables.valid() {
            return Err("competition-native.json karate: wrong table shape".into());
        }
        let rows: Vec<MotionRow> = serde_json::from_value(motions["karateMotions"].clone())
            .map_err(|e| format!("karate-motions.json: {e}"))?;
        if rows.len() != 46 {
            return Err("karate-motions.json: expected 46 rows".into());
        }
        let arena =
            Arena::from_value(&arena).map_err(|e| format!("karate-arena-collision.json: {e}"))?;
        Ok(Some(Self {
            tables,
            rows,
            arena,
        }))
    }
}

/// One class: five rounds against descriptor rows class*5+round+2
/// (fn_72_5020), stopping at the first round not won.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tournament {
    pub class: u8,
    pub rounds: Vec<Round>,
    pub won: bool,
}

pub fn tournament(
    data: &Data,
    motion_table: &MotionTable,
    rng: &mut MslRand,
    owner_skills: [u16; 7],
    child: bool,
    class: u8,
) -> Tournament {
    let mut rounds = vec![];
    for round in 0..karate::ROUNDS {
        let Some(r) = run_round(
            &data.tables,
            &data.rows,
            motion_table,
            &data.arena,
            rng,
            owner_skills,
            child,
            class,
            round,
        ) else {
            break;
        };
        // shortcut: a draw (leader 2) ends the run like a loss; the native
        // draw/rematch path is not traced yet.
        let lost = r.winner != 0;
        rounds.push(r);
        if lost {
            break;
        }
    }
    let won = rounds.len() == usize::from(karate::ROUNDS) && rounds.iter().all(|r| r.winner == 0);
    Tournament { class, rounds, won }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_blow_waits_for_impact_then_hurts_and_a_miss_dodges() {
        let mut rec = StateRecord {
            state: state::READY,
            flags: flag::CHECKED | flag::HIT,
            ..Default::default()
        };
        resolve_hit(&mut rec);
        assert_eq!(rec.state, state::READY);
        rec.flags |= TAKE_HIT;
        resolve_hit(&mut rec);
        assert_eq!((rec.state, rec.flags), (state::HURT, 0));
        let mut miss = StateRecord {
            state: state::ATTACK,
            flags: flag::CHECKED,
            ..Default::default()
        };
        resolve_hit(&mut miss);
        assert_eq!(miss.state, 5);
    }

    #[test]
    fn arena_floor_is_the_highest_triangle_under_a_point() {
        let arena = Arena {
            triangles: vec![
                [[-10., 7.5, -10.], [10., 7.5, -10.], [-10., 7.5, 10.]],
                [[-10., 0., -10.], [10., 0., -10.], [-10., 0., 10.]],
            ],
        };
        assert_eq!(arena.floor(-1., -1.), Some(7.5));
        assert_eq!(arena.floor(9., 9.), None);
    }
}
