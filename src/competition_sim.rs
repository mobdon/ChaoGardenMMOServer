//! Bounded, deterministic MMO competition simulations. These are adaptations,
//! not native race path, rival-range or Karate engine implementations.
use crate::activities::KarateOpponent;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const RULES_VERSION: &str = "adapted-ai-competition-v1";
const STEP_MS: u32 = 200;
const RACE_STEP_MS: u32 = 500;
const SEGMENT_WORK: u32 = 16_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SimulationKind {
    Race,
    Karate,
}

/// Stats are canonical-equivalent levels, in Swim/Fly/Run/Power/Stamina/Luck/
/// Intelligence order. Frozen at entry, never read from a later client message.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Competitor {
    pub id: String,
    pub name: String,
    pub stats: [u16; 7],
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
}

fn normalized(raw: f64, scale: f64) -> u16 {
    if raw.is_finite() {
        (raw.max(0.) * 99. / scale).round().clamp(0., 99.) as u16
    } else {
        0
    }
}

/// Explicit MMO interpretation: mode 0 uses sharedStat; mode 1 uses each
/// signed-short pair's arithmetic mean. Native random-range semantics remain
/// unverified. No native appearance bytes or executable content is embedded.
pub fn rival_from_native(value: &Value, index: usize) -> Option<Competitor> {
    if index > 4095 || value["index"].as_u64()? != index as u64 {
        return None;
    }
    let mode = value["statMode"].as_u64()?;
    let mut stats = [0; 7];
    let names = [
        "swim",
        "fly",
        "run",
        "power",
        "stamina",
        "luck",
        "intelligence",
    ];
    match mode {
        0 => {
            let raw = value["sharedStat"].as_i64()?;
            if !(-32768..=32767).contains(&raw) {
                return None;
            }
            stats.fill(normalized(raw as f64, 30_000.));
        }
        1 => {
            for (slot, name) in names.iter().enumerate() {
                let pair = value["perStatPairs"][name].as_array()?;
                if pair.len() != 2 {
                    return None;
                }
                let a = pair[0].as_i64()?;
                let b = pair[1].as_i64()?;
                if !(-32768..=32767).contains(&a) || !(-32768..=32767).contains(&b) {
                    return None;
                }
                stats[slot] = normalized((a + b) as f64 / 2., 30_000.);
            }
        }
        _ => return None,
    }
    Some(Competitor {
        id: format!("native-race-bot-{index}"),
        name: format!("Race rival {index}"),
        stats,
        source: json!({"nativeIndex":index,"address":value["address"],
            "statMode":mode,"mapping":"adapted-shared-or-pair-mean-over-30000"}),
    })
}

pub fn competitor_from_karate(opponent: &KarateOpponent) -> Competitor {
    let mut stats = [0; 7];
    for (index, stat) in stats.iter_mut().enumerate() {
        *stat = normalized(opponent.skills.get(index).copied().unwrap_or(0.), 2500.);
    }
    Competitor {
        id: opponent.id.clone(),
        name: opponent.name.clone(),
        stats,
        source: json!({"nativeIndex":opponent.native_index,"appearance":opponent.appearance,
            "mapping":"adapted-skill-over-2500"}),
    }
}

fn clamp_stats(mut competitor: Competitor) -> Competitor {
    competitor
        .stats
        .iter_mut()
        .for_each(|stat| *stat = (*stat).min(99));
    competitor
}

struct Random(u64);
impl Random {
    fn next(&mut self, bound: u32) -> u32 {
        // SplitMix64 has a defined integer sequence, including seed zero.
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((z ^ (z >> 31)) % u64::from(bound)) as u32
    }
}

/// One abstract four-segment circuit, not an available native geometry route.
/// Each entrant traverses swim/run/climb/fly segments and a final finish line.
/// All seven abilities affect performance; endurance depletes and recovers.
pub fn simulate_race(seed: u64, owner: Competitor, rivals: Vec<Competitor>) -> Simulation {
    let mut competitors = vec![clamp_stats(owner)];
    let mut ids = BTreeSet::from([competitors[0].id.clone()]);
    for rival in rivals {
        if competitors.len() == 4 {
            break;
        }
        if ids.insert(rival.id.clone()) {
            competitors.push(clamp_stats(rival));
        }
    }
    let mut rng = Random(seed);
    let mut work = vec![0_u32; competitors.len()];
    let mut stamina = vec![100_u32; competitors.len()];
    let mut finishes = vec![None; competitors.len()];
    let mut frames = Vec::new();
    let actions = ["swim", "run", "climb", "fly"];
    let skills = [0, 2, 3, 1];
    let mut elapsed = 0;
    loop {
        frames.push(SimFrame {
            elapsed_ms: elapsed,
            competitors: competitors
                .iter()
                .enumerate()
                .map(|(index, competitor)| FrameCompetitor {
                    id: competitor.id.clone(),
                    progress: work[index] * 10_000 / (4 * SEGMENT_WORK),
                    hp: 0,
                    stamina: stamina[index],
                    action: if finishes[index].is_some() {
                        "finished"
                    } else {
                        actions[(work[index] / SEGMENT_WORK).min(3) as usize]
                    }
                    .into(),
                })
                .collect(),
        });
        if finishes.iter().all(Option::is_some) {
            break;
        }
        elapsed += RACE_STEP_MS;
        for (index, competitor) in competitors.iter().enumerate() {
            if finishes[index].is_some() {
                continue;
            }
            let segment = (work[index] / SEGMENT_WORK).min(3) as usize;
            let s = competitor.stats.map(u32::from);
            let rate = 1500 + 22 * s[skills[segment]] + 4 * s[4] + 2 * s[6];
            let fatigue = if stamina[index] < 20 { 75 } else { 100 };
            let luck = 95 + rng.next(11) + s[5] / 25;
            let advance = rate * RACE_STEP_MS / 1000 * fatigue / 100 * luck / 100;
            let remaining = 4 * SEGMENT_WORK - work[index];
            if advance >= remaining {
                // Within-step interpolation preserves meaningful photo finishes.
                finishes[index] = Some(elapsed - RACE_STEP_MS + remaining * RACE_STEP_MS / advance);
                work[index] = 4 * SEGMENT_WORK;
                stamina[index] = stamina[index].saturating_sub(1);
            } else {
                work[index] += advance;
                let drain = if s[4] >= 60 { 1 } else { 2 };
                stamina[index] = if stamina[index] < 20 {
                    (stamina[index] + 3).min(100)
                } else {
                    stamina[index].saturating_sub(drain)
                };
            }
        }
    }
    let mut order: Vec<_> = (0..competitors.len()).collect();
    order.sort_by(|&a, &b| {
        finishes[a]
            .cmp(&finishes[b])
            .then_with(|| competitors[a].id.cmp(&competitors[b].id))
    });
    let placements = order
        .iter()
        .enumerate()
        .map(|(rank, &index)| Placement {
            id: competitors[index].id.clone(),
            rank: rank as u8 + 1,
            finish_ms: finishes[index],
            remaining_hp: None,
        })
        .collect();
    Simulation {
        kind: SimulationKind::Race,
        rules_version: RULES_VERSION.into(),
        duration_ms: elapsed,
        competitors,
        frames,
        placements,
        reason: "finished".into(),
    }
}

#[derive(Clone)]
struct Fighter {
    hp: u32,
    max_hp: u32,
    stamina: u32,
    max_stamina: u32,
    position: i32,
    ready_at: u32,
    guard_until: u32,
    dodge_until: u32,
    focus_until: u32,
    pending: Option<(u32, bool)>,
    action: String,
}
impl Fighter {
    fn new(stats: &[u16; 7], position: i32) -> Self {
        let max_hp = 100 + u32::from(stats[4]);
        let max_stamina = 100 + u32::from(stats[4]) / 2;
        Self {
            hp: max_hp,
            max_hp,
            stamina: max_stamina,
            max_stamina,
            position,
            ready_at: 0,
            guard_until: 0,
            dodge_until: 0,
            focus_until: 0,
            pending: None,
            action: "stance".into(),
        }
    }
}

/// One autonomous bout with server-timed approach, windup, range, recovery,
/// guard, evasion, focus and stamina. Simultaneous hits resolve together. Exact
/// HP-ratio ties are draws: both entrants rank second and neither wins.
pub fn simulate_karate(seed: u64, owner: Competitor, opponent: Competitor) -> Simulation {
    let competitors = vec![clamp_stats(owner), clamp_stats(opponent)];
    let mut fighters = [
        Fighter::new(&competitors[0].stats, 550),
        Fighter::new(&competitors[1].stats, 2450),
    ];
    let mut rng = Random(seed);
    let mut frames = Vec::new();
    let mut elapsed = 0;
    loop {
        frames.push(SimFrame {
            elapsed_ms: elapsed,
            competitors: fighters
                .iter()
                .enumerate()
                .map(|(index, fighter)| FrameCompetitor {
                    id: competitors[index].id.clone(),
                    progress: 0,
                    hp: fighter.hp,
                    stamina: fighter.stamina,
                    action: fighter.action.clone(),
                })
                .collect(),
        });
        if fighters.iter().any(|fighter| fighter.hp == 0) || elapsed == 60_000 {
            break;
        }
        elapsed += STEP_MS;
        let distance = (fighters[0].position - fighters[1].position).unsigned_abs();
        let mut damages = [0; 2];
        for index in 0..2 {
            let other = 1 - index;
            let s = competitors[index].stats.map(u32::from);
            if let Some((due, kick)) = fighters[index].pending
                && due <= elapsed
            {
                fighters[index].pending = None;
                fighters[index].action = "recover".into();
                fighters[index].ready_at = elapsed + if kick { 600 } else { 400 };
                if distance <= 1500 && fighters[other].dodge_until <= elapsed {
                    let target = competitors[other].stats.map(u32::from);
                    let evade = (target[1] + target[2] + target[5]) / 2;
                    if rng.next(1000) >= evade {
                        let base = if kick { 13 + s[3] / 5 } else { 8 + s[3] / 8 };
                        let critical = if rng.next(1000) < s[5] { 2 } else { 1 };
                        damages[other] = if fighters[other].guard_until > elapsed {
                            fighters[other].stamina = fighters[other].stamina.saturating_sub(8);
                            (base * critical / 4).max(1)
                        } else {
                            base * critical
                        };
                    } else {
                        fighters[other].action = "evade".into();
                    }
                }
            }
        }
        for index in 0..2 {
            fighters[index].hp = fighters[index].hp.saturating_sub(damages[index]);
        }
        if fighters.iter().any(|fighter| fighter.hp == 0) {
            continue;
        }
        for index in 0..2 {
            let s = competitors[index].stats.map(u32::from);
            let fighter = &mut fighters[index];
            let recovery = if fighter.focus_until > elapsed { 8 } else { 2 };
            fighter.stamina = (fighter.stamina + recovery).min(fighter.max_stamina);
            if fighter.pending.is_some() || fighter.ready_at > elapsed {
                continue;
            }
            if distance > 1400 {
                let step = (80 + s[2] * 2) as i32;
                fighter.position =
                    (fighter.position + if index == 0 { step } else { -step }).clamp(0, 3000);
                fighter.action = "approach".into();
            } else if fighter.stamina < 20 {
                fighter.focus_until = elapsed + 1000;
                fighter.ready_at = elapsed + 1000;
                fighter.action = "focus".into();
            } else {
                let choice = rng.next(100);
                if choice < 8 + s[0] / 20 {
                    fighter.stamina = fighter.stamina.saturating_sub(6);
                    fighter.guard_until = elapsed + 800;
                    fighter.ready_at = elapsed + 800;
                    fighter.action = "guard".into();
                } else if choice < 16 + s[1] / 20 {
                    fighter.stamina = fighter.stamina.saturating_sub(18);
                    fighter.dodge_until = elapsed + 400;
                    fighter.ready_at = elapsed + 1000;
                    fighter.action = "dodge".into();
                } else {
                    let kick = choice >= 65 - s[6] / 10 && fighter.stamina >= 20;
                    fighter.stamina -= if kick { 20 } else { 10 };
                    fighter.pending = Some((elapsed + if kick { 400 } else { 200 }, kick));
                    fighter.action = if kick { "kick-windup" } else { "punch-windup" }.into();
                }
            }
        }
    }
    let a = u64::from(fighters[0].hp) * u64::from(fighters[1].max_hp);
    let b = u64::from(fighters[1].hp) * u64::from(fighters[0].max_hp);
    let draw = a == b;
    let winner = usize::from(b > a);
    let mut placements = (0..2)
        .map(|index| Placement {
            id: competitors[index].id.clone(),
            rank: if draw || index != winner { 2 } else { 1 },
            finish_ms: None,
            remaining_hp: Some(fighters[index].hp),
        })
        .collect::<Vec<_>>();
    placements.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.id.cmp(&b.id)));
    let reason = if draw {
        "draw"
    } else if fighters.iter().any(|fighter| fighter.hp == 0) {
        "knockout"
    } else {
        "timeout"
    };
    Simulation {
        kind: SimulationKind::Karate,
        rules_version: RULES_VERSION.into(),
        duration_ms: elapsed,
        competitors,
        frames,
        placements,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entrant(id: &str, level: u16) -> Competitor {
        Competitor {
            id: id.into(),
            name: id.into(),
            stats: [level; 7],
            source: json!({"fixture":true}),
        }
    }

    #[test]
    fn one_race_has_ordered_segments_placements_and_deterministic_replay() {
        let race = simulate_race(57, entrant("owner", 50), vec![entrant("rival", 25)]);
        assert_eq!(
            race,
            simulate_race(57, entrant("owner", 50), vec![entrant("rival", 25)])
        );
        assert_eq!(
            race,
            serde_json::from_value::<Simulation>(serde_json::to_value(&race).unwrap()).unwrap()
        );
        assert_eq!(race.placements[0].id, "owner");
        assert!(race.duration_ms <= 60_000);
        let actions: Vec<_> = race
            .frames
            .iter()
            .map(|f| f.competitors[0].action.as_str())
            .fold(Vec::new(), |mut actions, action| {
                if actions.last().copied() != Some(action) {
                    actions.push(action);
                }
                actions
            });
        assert_eq!(actions, vec!["swim", "run", "climb", "fly", "finished"]);
        for frames in race.frames.windows(2) {
            assert!(frames[0].elapsed_ms < frames[1].elapsed_ms);
            for (before, after) in frames[0].competitors.iter().zip(&frames[1].competitors) {
                assert!(after.progress >= before.progress && after.progress <= 10_000);
            }
        }
        assert!(
            race.frames
                .last()
                .unwrap()
                .competitors
                .iter()
                .all(|c| c.progress == 10_000)
        );
    }

    #[test]
    fn race_skill_scaling_and_bounded_roster() {
        let slow = simulate_race(1, entrant("owner", 0), vec![entrant("rival", 30)]);
        let fast = simulate_race(1, entrant("owner", 99), vec![entrant("rival", 30)]);
        let time = |race: &Simulation| {
            race.placements
                .iter()
                .find(|p| p.id == "owner")
                .unwrap()
                .finish_ms
                .unwrap()
        };
        assert!(time(&fast) < time(&slow));
        let entrants = vec![
            entrant("owner", 0),
            entrant("one", 0),
            entrant("one", 0),
            entrant("two", 0),
            entrant("three", 0),
            entrant("four", 0),
        ];
        let race = simulate_race(0, entrant("owner", u16::MAX), entrants);
        assert_eq!(race.competitors.len(), 4);
        assert!(race.competitors[0].stats.iter().all(|stat| *stat == 99));
    }

    #[test]
    fn one_karate_bout_is_bounded_stat_scaled_and_replayable() {
        let bout = simulate_karate(42, entrant("owner", 99), entrant("opponent", 0));
        assert_eq!(
            bout,
            simulate_karate(42, entrant("owner", 99), entrant("opponent", 0))
        );
        assert_eq!(
            bout,
            serde_json::from_value::<Simulation>(serde_json::to_value(&bout).unwrap()).unwrap()
        );
        assert_eq!(bout.placements[0].id, "owner");
        assert_eq!(bout.placements[0].rank, 1);
        assert!((1000..=60_000).contains(&bout.duration_ms));
        assert!(bout.frames.len() <= 301);
        assert!(bout.frames.iter().any(|f| {
            f.competitors
                .iter()
                .any(|c| c.action == "punch-windup" || c.action == "kick-windup")
        }));
        assert!(
            bout.frames
                .windows(2)
                .all(|pair| pair[0].elapsed_ms + STEP_MS == pair[1].elapsed_ms)
        );
        let reverse = simulate_karate(42, entrant("owner", 0), entrant("opponent", 99));
        assert_eq!(reverse.placements[0].id, "opponent");
    }

    #[test]
    fn native_rivals_have_explicit_adapted_mapping_and_reject_invalid_rows() {
        let shared = json!({"index":1,"statMode":0,"sharedStat":15000,"address":"fixture"});
        assert_eq!(rival_from_native(&shared, 1).unwrap().stats, [50; 7]);
        assert!(rival_from_native(&shared, 2).is_none());
        let mut paired = json!({"index":0,"statMode":1,"perStatPairs":{}});
        for name in [
            "swim",
            "fly",
            "run",
            "power",
            "stamina",
            "luck",
            "intelligence",
        ] {
            paired["perStatPairs"][name] = json!([0, 30000]);
        }
        assert_eq!(rival_from_native(&paired, 0).unwrap().stats, [50; 7]);
        paired["perStatPairs"]["run"] = json!([0, 999999]);
        assert!(rival_from_native(&paired, 0).is_none());
    }

    #[test]
    fn simultaneous_knockout_is_a_draw_with_no_winning_placement() {
        let bout = simulate_karate(4, entrant("owner", 0), entrant("opponent", 0));
        assert_eq!(bout.reason, "draw");
        assert_eq!(bout.duration_ms, 10_800);
        assert!(bout.placements.iter().all(|placement| placement.rank == 2));
        assert!(
            bout.placements
                .iter()
                .all(|placement| placement.remaining_hp == Some(0))
        );
    }
}
