//! MMO layer for away Chao Races and Karate bouts.
//!
//! Every rule that decides an outcome comes from `competition_native` (ported
//! from the GameCube modules). This module only supplies what the native game
//! gets from things the server does not run: the 3D course and its geometry,
//! the motion lengths that time each karate state, the movement integrator,
//! the order tasks run in, the random seed, replay sampling and the ring
//! payout. Each of those is a field of [`MmoConfig`], labelled MMO, so mob can
//! tune them in `CHAO_DATA_DIR/activities/competition-mmo.json` without touching
//! the native rules.
use crate::competition_native::{
    MslRand, NativeTables,
    karate::{self, Fighter, StateRecord, flag, state},
    race::{self, Class},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const RULES_VERSION: &str = "native-gsne8p+mmo-v1";
/// Results saved by the previous adapted simulation keep loading.
pub const LEGACY_RULES_VERSION: &str = "adapted-ai-competition-v1";

/// A race section kind: the native movement routine that drives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Section {
    Run,
    Swim,
    Fly,
    Climb,
}

/// MMO: one stretch of an abstract course (the native course geometry and
/// path following are not ported).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stretch {
    pub kind: Section,
    /// MMO: length in native position units.
    pub length: f32,
}

/// Tunables. Everything here is an MMO choice; none of it is native data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct MmoConfig {
    /// MMO: replay milliseconds per native frame (the GameCube runs ~59.94 fields/s).
    pub frame_ms: f64,
    /// MMO: one replay frame every N native frames.
    pub sample_frames: u32,
    /// MMO: which native race (class, course, level) away races run.
    pub race_class: Class,
    pub race_course: u8,
    pub race_level: u8,
    /// MMO: stretches per native section set 0..9 (BegSwm .. JwlLng, course row +8).
    pub race_sections: Vec<Vec<Stretch>>,
    /// MMO: per-frame velocity drag while running (native run sets acceleration
    /// only; main.dol MOV_Control is not ported).
    pub race_run_drag: f32,
    /// MMO: frames a tripped racer (fn_76_D008) stands still.
    pub race_trip_frames: u32,
    /// MMO: give up on racers still running after this many frames.
    pub race_max_frames: u32,
    /// MMO: other players' Chao that replace native rivals, lanes 1.. (0 = all native).
    pub race_player_rivals: u8,
    /// MMO: the karate class away bouts use (0..=3).
    pub karate_class: u8,
    /// MMO: frames of the attack-choice motion (fn_72_FCC8 waits for its end).
    pub karate_choose_frames: u32,
    /// MMO: attack motion frames when the hit check (fn_72_EA20) runs, when the
    /// blow lands, and when the motion ends (fn_72_FE9C steps 1, 2, 3).
    pub karate_hit_frame: u32,
    pub karate_impact_frame: u32,
    pub karate_attack_frames: u32,
    /// MMO: hurt reaction and stagger motion lengths (states 6 and 7).
    pub karate_hurt_frames: u32,
    pub karate_stagger_frames: u32,
    /// MMO: fighter distance passed to the hit check (fn_72_13258 is not ported;
    /// the native check never hits beyond 5.0).
    pub karate_distance: f32,
    /// MMO: rings per finishing place (index 0 = first), and for a karate win.
    pub race_rings: Vec<u32>,
    pub karate_rings: u32,
}

impl Default for MmoConfig {
    fn default() -> Self {
        let set = |kind| {
            vec![
                Stretch {
                    kind: Section::Run,
                    length: 100.0,
                },
                Stretch {
                    kind,
                    length: if kind == Section::Climb { 20.0 } else { 60.0 },
                },
                Stretch {
                    kind: Section::Run,
                    length: 100.0,
                },
            ]
        };
        Self {
            frame_ms: 1000.0 / 60.0,
            sample_frames: 12,
            race_class: Class::Beginner,
            race_course: 0,
            race_level: 0,
            race_sections: vec![
                set(Section::Swim),
                set(Section::Fly),
                set(Section::Run),
                set(Section::Climb),
                set(Section::Swim),
                set(Section::Fly),
                set(Section::Run),
                set(Section::Climb),
                set(Section::Run),
                vec![
                    Stretch {
                        kind: Section::Run,
                        length: 100.0,
                    },
                    Stretch {
                        kind: Section::Swim,
                        length: 60.0,
                    },
                    Stretch {
                        kind: Section::Fly,
                        length: 60.0,
                    },
                    Stretch {
                        kind: Section::Climb,
                        length: 20.0,
                    },
                    Stretch {
                        kind: Section::Run,
                        length: 100.0,
                    },
                ],
            ],
            race_run_drag: 0.06,
            race_trip_frames: 60,
            race_max_frames: 60 * 60 * 5,
            race_player_rivals: 0,
            karate_class: 0,
            karate_choose_frames: 20,
            karate_hit_frame: 15,
            karate_impact_frame: 20,
            karate_attack_frames: 40,
            karate_hurt_frames: 40,
            karate_stagger_frames: 60,
            karate_distance: 0.0,
            race_rings: vec![25, 15, 5, 0, 0, 0, 0, 0],
            karate_rings: 50,
        }
    }
}

impl MmoConfig {
    pub fn from_value(v: &Value) -> Result<Self, String> {
        if v.is_null() || v.as_object().is_some_and(|o| o.is_empty()) {
            return Ok(Self::default());
        }
        let c: Self = serde_json::from_value(v.clone())
            .map_err(|e| format!("MMO competition config: {e}"))?;
        c.valid()
            .then_some(c)
            .ok_or_else(|| "MMO competition config is out of range".into())
    }

    pub fn valid(&self) -> bool {
        let fin = |x: f32| x.is_finite();
        self.frame_ms.is_finite()
            && self.frame_ms > 0.0
            && (1..=600).contains(&self.sample_frames)
            && self
                .race_class
                .row(self.race_course, self.race_level)
                .is_some()
            && self.race_sections.len() == 10
            && self.race_sections.iter().all(|s| {
                !s.is_empty()
                    && s.len() <= 32
                    && s.iter()
                        .all(|t| fin(t.length) && t.length > 0.0 && t.length <= 100_000.0)
            })
            && fin(self.race_run_drag)
            && (0.0..1.0).contains(&self.race_run_drag)
            && self.race_trip_frames <= 6000
            && (60..=60 * 60 * 30).contains(&self.race_max_frames)
            && usize::from(self.race_player_rivals) < race::LANES
            && self.karate_class < karate::CLASSES
            && self.karate_choose_frames <= 600
            && 1 <= self.karate_hit_frame
            && self.karate_hit_frame <= self.karate_impact_frame
            && self.karate_impact_frame <= self.karate_attack_frames
            && self.karate_attack_frames <= 600
            && (1..=600).contains(&self.karate_hurt_frames)
            && (1..=600).contains(&self.karate_stagger_frames)
            && fin(self.karate_distance)
            && self.race_rings.len() == race::LANES
            && self.race_rings.iter().all(|r| *r <= 1000)
            && self.karate_rings <= 1000
    }

    fn ms(&self, frames: u32) -> u32 {
        (f64::from(frames) * self.frame_ms).round() as u32
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SimulationKind {
    Race,
    Karate,
}

/// `stats` are the canonical levels (display only); `skills` are the native
/// skill points (Chao param +0x38) the native rules read.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Competitor {
    pub id: String,
    pub name: String,
    pub stats: [u16; 7],
    #[serde(default)]
    pub skills: [u16; 7],
    pub source: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub id: String,
    pub rank: u8,
    pub finish_ms: Option<u32>,
    pub remaining_hp: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrameCompetitor {
    pub id: String,
    /// Race progress in basis points; zero for Karate.
    pub progress: u32,
    pub hp: u32,
    /// Race stamina (Chao work +0x88) or karate zeal (+0x38).
    pub stamina: u32,
    pub action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SimFrame {
    pub elapsed_ms: u32,
    pub competitors: Vec<FrameCompetitor>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Simulation {
    pub kind: SimulationKind,
    pub rules_version: String,
    pub duration_ms: u32,
    pub competitors: Vec<Competitor>,
    pub frames: Vec<SimFrame>,
    pub placements: Vec<Placement>,
    pub reason: String,
    /// The MMO config the run used, so a saved result replays after re-tuning.
    #[serde(default)]
    pub config: Option<MmoConfig>,
    /// Native context: class/course/level/row, opponent row, round, award.
    #[serde(default)]
    pub native: Value,
}

/// Native skill points from a canonical stat's points (param skills are u16).
pub fn skill_points(points: u32) -> u16 {
    points.min(u32::from(u16::MAX)) as u16
}

/// The lane-1.. native rivals of a race: fn_60_24E40 set, fn_60_24F48 order,
/// fn_60_253D4 skills. `rng` must be the race's generator.
pub fn native_rivals(
    tables: &NativeTables,
    cfg: &MmoConfig,
    rng: &mut MslRand,
) -> Option<Vec<Competitor>> {
    let set = race::rival_set_index(
        cfg.race_class,
        cfg.race_course,
        cfg.race_level,
        &tables.race_courses,
    )?;
    let set_row = tables.rival_sets.get(set)?;
    let order = race::rival_order(set_row, rng);
    let mut out = vec![];
    for (lane, k) in order.iter().enumerate() {
        let bot_index = usize::try_from(set_row.bots[*k]).ok()?;
        let skills = race::rival_skills(tables.race_bots.get(bot_index)?, rng);
        out.push(Competitor {
            id: format!("native-race-rival-{}", lane + 1),
            name: format!("Rival {}", lane + 1),
            stats: [0; 7],
            skills,
            source: json!({"rivalSet":set,"raceBot":bot_index,"lane":lane + 1,
                "rule":"CM fn_60_24E40/24F48/253D4"}),
        });
    }
    Some(out)
}

struct Lane {
    stretch: usize,
    distance: f32,
    covered: f32,
    velocity: f32,
    stamina: i32,
    counter: i32,
    trip_wait: u32,
    finish: Option<u32>,
}

/// One native race run by the MMO integrator. `competitors[0]` is the owner
/// (lane 0); the rest are lanes 1.. in order.
pub fn simulate_race(
    seed: u64,
    tables: &NativeTables,
    cfg: &MmoConfig,
    competitors: Vec<Competitor>,
) -> Option<Simulation> {
    let row_index = cfg.race_class.row(cfg.race_course, cfg.race_level)?;
    let row = *tables.race_courses.get(row_index)?;
    let stretches = cfg
        .race_sections
        .get(usize::try_from(row.sections).ok()?)?
        .clone();
    let total: f32 = stretches.iter().map(|s| s.length).sum();
    // MMO: the native generator, seeded by the server.
    let mut rng = MslRand { next: seed as u32 };
    let skills: Vec<[u16; 7]> = competitors
        .iter()
        .map(|c| c.skills.map(|s| race::race_skill(u32::from(s))))
        .collect();
    let mut lanes: Vec<Lane> = skills
        .iter()
        .map(|s| Lane {
            stretch: 0,
            distance: 0.0,
            covered: 0.0,
            velocity: 0.0,
            stamina: race::start_stamina(s[4]),
            counter: 0,
            trip_wait: 0,
            finish: None,
        })
        .collect();
    let mut frames = vec![];
    let mut frame = 0u32;
    loop {
        if frame.is_multiple_of(cfg.sample_frames) || lanes.iter().all(|l| l.finish.is_some()) {
            frames.push(SimFrame {
                elapsed_ms: cfg.ms(frame),
                competitors: lanes
                    .iter()
                    .zip(&competitors)
                    .map(|(l, c)| FrameCompetitor {
                        id: c.id.clone(),
                        progress: ((l.covered + l.distance) / total * 10_000.0).clamp(0.0, 10_000.0)
                            as u32,
                        hp: 0,
                        stamina: l.stamina.max(0) as u32,
                        action: if l.finish.is_some() {
                            "finished".into()
                        } else if l.trip_wait > 0 {
                            "trip".into()
                        } else {
                            format!("{:?}", stretches[l.stretch].kind).to_lowercase()
                        },
                    })
                    .collect(),
            });
        }
        if lanes.iter().all(|l| l.finish.is_some()) || frame >= cfg.race_max_frames {
            break;
        }
        frame += 1;
        // MMO: lanes update in lane order each frame.
        for (l, s) in lanes.iter_mut().zip(&skills) {
            if l.finish.is_some() {
                continue;
            }
            let stretch = stretches[l.stretch];
            match stretch.kind {
                Section::Run => {
                    if l.trip_wait > 0 {
                        l.trip_wait -= 1;
                    } else {
                        // fn_76_D2B4 (param +0xDD unknown: MMO 0).
                        if race::run_frame(s[2], &mut l.counter, &mut l.stamina, 0, &mut rng) {
                            l.trip_wait = cfg.race_trip_frames;
                            l.velocity = 0.0;
                        } else {
                            let a = race::run_accel(s[2], l.stamina, false);
                            l.velocity += a - cfg.race_run_drag * l.velocity;
                        }
                    }
                }
                Section::Swim => {
                    let a = race::swim_accel(s[0], l.stamina, false);
                    l.velocity += a - race::SWIM_DRAG * l.velocity;
                }
                Section::Fly => {
                    let a = race::fly_accel(s[1], l.stamina, false);
                    l.velocity += a - race::FLY_DRAG * l.velocity;
                }
                Section::Climb => l.velocity = race::climb_step(s[3], l.stamina, false),
            }
            l.distance += l.velocity;
            while l.finish.is_none() && l.distance >= stretches[l.stretch].length {
                l.distance -= stretches[l.stretch].length;
                l.covered += stretches[l.stretch].length;
                l.stretch += 1;
                l.counter = 0; // fn_76_D2B4 step 0 resets its counter.
                if l.stretch == stretches.len() {
                    l.finish = Some(frame);
                    l.distance = 0.0;
                    l.stretch -= 1;
                }
            }
        }
    }
    let mut order: Vec<usize> = (0..lanes.len()).collect();
    order.sort_by(|&a, &b| {
        let key = |i: usize| {
            (
                lanes[i].finish.unwrap_or(u32::MAX),
                -((lanes[i].covered + lanes[i].distance) as f64 * 1000.0) as i64,
                i,
            )
        };
        key(a).cmp(&key(b))
    });
    let mut placements: Vec<Placement> = order
        .iter()
        .enumerate()
        .map(|(rank, &i)| Placement {
            id: competitors[i].id.clone(),
            rank: rank as u8 + 1,
            finish_ms: lanes[i].finish.map(|f| cfg.ms(f)),
            remaining_hp: None,
        })
        .collect();
    placements.sort_by_key(|p| p.rank);
    let duration = cfg.ms(frame).max(1);
    Some(Simulation {
        kind: SimulationKind::Race,
        rules_version: RULES_VERSION.into(),
        duration_ms: duration,
        competitors,
        frames,
        placements,
        reason: if lanes.iter().all(|l| l.finish.is_some()) {
            "finished"
        } else {
            "time-limit"
        }
        .into(),
        config: Some(cfg.clone()),
        native: json!({"class":cfg.race_class,"course":cfg.race_course,"level":cfg.race_level,
            "courseRow":row_index,"sections":row.sections}),
    })
}

/// The karate opponent for a class and round: fn_72_5020's row choice and the
/// skill row the descriptor names.
pub fn karate_opponent(
    tables: &NativeTables,
    names: &[String],
    class: u8,
    round: u8,
    rng: &mut MslRand,
) -> Option<(Competitor, karate::Descriptor)> {
    let row = karate::opponent_row(&tables.karate.descriptors, class, round, rng)?;
    let d = tables.karate.descriptors[row];
    let s = tables.karate.skills.get(usize::from(d.skills))?;
    let name = names
        .get(usize::from(d.name))
        .cloned()
        .unwrap_or_else(|| format!("Karate opponent {row}"));
    Some((
        Competitor {
            id: format!("native-karate-opponent-{row}"),
            name,
            stats: [0; 7],
            skills: [s[0], s[1], s[2], s[3], s[4], s[5], s[6]],
            source: json!({"descriptor":row,"kind":d.kind,"class":class,"round":round,
                "rule":"ChaoStgKarate fn_72_5020"}),
        },
        d,
    ))
}

struct Side {
    me: Fighter,
    rec: StateRecord,
    kind: i16,
    timer: u32,
}

/// One native karate round (fn_72_10E0C per frame per fighter) timed by the
/// MMO motion lengths. `owner_child` is Chao type 2 (body). The opponent is a
/// table fighter (skills jittered, fn_72_11038).
pub fn simulate_karate(
    seed: u64,
    cfg: &MmoConfig,
    owner: Competitor,
    owner_child: bool,
    opponent: Competitor,
    kind: i16,
    native: Value,
) -> Simulation {
    let mut rng = MslRand { next: seed as u32 };
    let mut sides = [
        Side {
            me: karate::fighter(owner.skills, false, owner_child, false, &mut rng),
            rec: StateRecord::default(),
            kind: -1,
            timer: 0,
        },
        Side {
            me: karate::fighter(opponent.skills, true, false, false, &mut rng),
            rec: StateRecord::default(),
            kind,
            timer: 0,
        },
    ];
    // fn_72_F8F8 waits for the fight to start, then both fighters are ready.
    for s in &mut sides {
        s.rec.state = state::READY;
    }
    let competitors = vec![owner, opponent];
    let limit = karate::ROUND_SECONDS * karate::FRAMES_PER_SECOND;
    let mut frames = vec![];
    let mut frame = 0u32;
    let mut knocked_out = None;
    let sample = |sides: &[Side; 2], frame: u32, frames: &mut Vec<SimFrame>| {
        frames.push(SimFrame {
            elapsed_ms: cfg.ms(frame),
            competitors: sides
                .iter()
                .zip(&competitors)
                .map(|(s, c)| FrameCompetitor {
                    id: c.id.clone(),
                    progress: 0,
                    hp: s.me.hp.max(0.0).round() as u32,
                    stamina: s.me.zeal.max(0.0).round() as u32,
                    action: match s.rec.state {
                        state::READY => "ready",
                        state::CHOOSE => "windup",
                        state::ATTACK => "attack",
                        state::HURT => "hurt",
                        state::STAGGER => "stagger",
                        _ => "stance",
                    }
                    .into(),
                })
                .collect(),
        })
    };
    sample(&sides, 0, &mut frames);
    while frame < limit && knocked_out.is_none() {
        frame += 1;
        for i in 0..2 {
            let o = 1 - i;
            match sides[i].rec.state {
                state::READY => {
                    let s = &mut sides[i];
                    karate::ready_step(&mut s.me, &mut s.rec, &mut rng);
                    s.timer = 0;
                }
                state::CHOOSE => {
                    sides[i].timer += 1;
                    if sides[i].timer >= cfg.karate_choose_frames {
                        sides[i].rec.state = state::ATTACK;
                        sides[i].rec.step = 1;
                        sides[i].timer = 0;
                    }
                }
                state::ATTACK => {
                    sides[i].timer += 1;
                    let t = sides[i].timer;
                    if t == cfg.karate_hit_frame {
                        let (accuracy, evasion, okind) =
                            (sides[i].me.accuracy, sides[o].me.evasion, sides[o].kind);
                        let hit = karate::hit_check(
                            &mut sides[o].rec,
                            evasion,
                            okind,
                            accuracy,
                            cfg.karate_distance,
                            &mut rng,
                        );
                        if hit {
                            sides[i].rec.flags |= flag::ATTACK_HIT;
                        } else {
                            sides[i].rec.flags &= !flag::ATTACK_HIT;
                        }
                    }
                    if t == cfg.karate_impact_frame && sides[i].rec.flags & flag::ATTACK_HIT != 0 {
                        // MMO: the 0x4000 flag fn_72_FE9C sets puts the target in state 6.
                        let (power, okind) = (sides[i].me.power, sides[o].kind);
                        let target = &mut sides[o];
                        target.rec.state = state::HURT;
                        target.rec.step = 0;
                        target.timer = 0;
                        karate::take_hit(
                            &mut target.me,
                            &mut target.rec,
                            power,
                            okind,
                            true,
                            &mut rng,
                        );
                        if target.rec.flags & flag::KNOCKED_OUT != 0 {
                            knocked_out = Some(o);
                        }
                    }
                    if t >= cfg.karate_attack_frames {
                        let s = &mut sides[i];
                        s.rec.flags |= flag::MOTION_END;
                        karate::attack_end(&mut s.me, &mut s.rec);
                        s.rec.flags &= !flag::MOTION_END;
                        s.timer = 0;
                    }
                }
                state::HURT => {
                    let s = &mut sides[i];
                    s.timer += 1;
                    if s.timer >= cfg.karate_hurt_frames {
                        s.rec.flags |= flag::MOTION_END;
                    }
                    karate::hurt_step(&mut s.me, &mut s.rec, &mut rng);
                    if s.rec.state != state::HURT {
                        s.rec.flags &= !flag::MOTION_END;
                        s.timer = 0;
                    }
                }
                state::STAGGER => {
                    let s = &mut sides[i];
                    s.timer += 1;
                    // fn_72_10AD4: back to ready when the motion ends.
                    if s.timer >= cfg.karate_stagger_frames {
                        s.rec.state = state::READY;
                        s.rec.step = 0;
                        s.rec.flags &= !flag::GIVE_UP;
                        s.timer = 0;
                    }
                }
                _ => {}
            }
            let lead = karate::leader([
                (sides[0].me.hp, sides[0].me.hp_max),
                (sides[1].me.hp, sides[1].me.hp_max),
            ]);
            let s = &mut sides[i];
            karate::zeal_step(&mut s.me, &mut s.rec, i as i16, lead, &mut rng);
            if knocked_out.is_some() {
                break;
            }
        }
        for s in &mut sides {
            s.rec.frame = (s.rec.frame + 1) % 60;
        }
        if frame.is_multiple_of(cfg.sample_frames) {
            sample(&sides, frame, &mut frames);
        }
    }
    if !frame.is_multiple_of(cfg.sample_frames) {
        sample(&sides, frame, &mut frames);
    }
    // fn_72_F4CC: a knock-out is won by the other fighter; at the time limit
    // the HP-percentage leader wins (2 = draw).
    let winner = match knocked_out {
        Some(k) => 1 - k as i16,
        None => karate::leader([
            (sides[0].me.hp, sides[0].me.hp_max),
            (sides[1].me.hp, sides[1].me.hp_max),
        ]),
    };
    let placements = (0..2)
        .map(|i| Placement {
            id: competitors[i].id.clone(),
            rank: if winner == i as i16 { 1 } else { 2 },
            finish_ms: None,
            remaining_hp: Some(sides[i].me.hp.max(0.0).round() as u32),
        })
        .collect();
    Simulation {
        kind: SimulationKind::Karate,
        rules_version: RULES_VERSION.into(),
        duration_ms: cfg.ms(frame).max(1),
        competitors,
        frames,
        placements,
        reason: match (knocked_out, winner) {
            (Some(_), _) => "knockout",
            (None, 2) => "draw",
            _ => "time-limit",
        }
        .into(),
        config: Some(cfg.clone()),
        native,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::competition_native::{
        karate::Descriptor,
        race::{CourseRow, RaceBot, RivalSet},
    };

    /// Synthetic tables with the native shapes (no game data).
    fn tables() -> NativeTables {
        NativeTables {
            karate: karate::Tables {
                descriptors: (0..27)
                    .map(|i| Descriptor {
                        skills: (i % 28) as u16,
                        name: i as u16,
                        appearance: 0,
                        kind: -1,
                        alternate: -1,
                    })
                    .collect(),
                skills: (0..28).map(|i| [i * 100; 8]).collect(),
            },
            race_courses: (0..62)
                .map(|_| CourseRow {
                    index: 0,
                    land: 0,
                    sections: 0,
                    e010: 0,
                    unlock: -1,
                })
                .collect(),
            race_bots: (0..115)
                .map(|i| RaceBot {
                    mode: 0,
                    shared: i * 70,
                    pairs: [[0, 0]; 7],
                })
                .collect(),
            rival_sets: (0..62)
                .map(|i| RivalSet {
                    fixed: 0,
                    unknown4: 0,
                    bots: [i % 115; 7],
                })
                .collect(),
        }
    }
    fn owner(points: u16) -> Competitor {
        Competitor {
            id: "owner".into(),
            name: "owner".into(),
            stats: [0; 7],
            skills: [points; 7],
            source: Value::Null,
        }
    }

    #[test]
    fn default_config_is_valid_and_round_trips() {
        let c = MmoConfig::default();
        assert!(c.valid());
        assert_eq!(
            MmoConfig::from_value(&serde_json::to_value(&c).unwrap()).unwrap(),
            c
        );
        assert!(MmoConfig::from_value(&json!({"raceLevel":9})).is_err());
    }

    #[test]
    fn a_race_is_deterministic_and_ranks_every_lane() {
        let t = tables();
        let cfg = MmoConfig::default();
        let mut rng = MslRand { next: 7 };
        let mut field = vec![owner(3000)];
        field.extend(native_rivals(&t, &cfg, &mut rng).unwrap());
        let a = simulate_race(7, &t, &cfg, field.clone()).unwrap();
        assert_eq!(a, simulate_race(7, &t, &cfg, field).unwrap());
        assert_eq!(a.placements.len(), 8);
        assert!(a.placements.iter().all(|p| p.finish_ms.is_some()));
        let slow = simulate_race(
            7,
            &t,
            &cfg,
            vec![owner(0), owner(4000)]
                .into_iter()
                .enumerate()
                .map(|(i, mut c)| {
                    c.id = format!("c{i}");
                    c
                })
                .collect(),
        )
        .unwrap();
        assert_eq!(slow.placements[0].id, "c1");
    }

    #[test]
    fn a_karate_round_ends_by_knockout_or_the_clock() {
        let t = tables();
        let cfg = MmoConfig::default();
        let mut rng = MslRand { next: 3 };
        let (opp, d) = karate_opponent(&t, &[], 0, 0, &mut rng).unwrap();
        let strong = simulate_karate(
            3,
            &cfg,
            owner(4000),
            false,
            opp.clone(),
            d.kind,
            Value::Null,
        );
        assert_eq!(
            strong,
            simulate_karate(
                3,
                &cfg,
                owner(4000),
                false,
                opp.clone(),
                d.kind,
                Value::Null
            )
        );
        assert_eq!(strong.placements[0].rank, 1);
        assert!(strong.duration_ms <= cfg.ms(90 * 60));
    }
}
