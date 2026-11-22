//! Player-versus-player Chao Karate: the bout rules the server decides.
//!
//! Ported from the GameCube ChaoStgKarate module (GSNE8P module 72), read from
//! its disassembly; each item names the function it comes from. Offsets are
//! fields of the fighter's work area as that module uses them. Only the rules
//! below are ported so far: fighter values, the strong-move gate, the hit
//! roll, damage, the time-up judge, the attack gauge and each attack's frame
//! schedule. Hurt and dodge reactions, movement and ring-out are not ported
//! yet, so nothing here runs a bout on its own.
//!
//! Rolls are passed in as numbers in 0..1 (the game uses rand() / 32768), so
//! the caller owns the dice.

/// Skill points as the game's AL_ParameterGetSkill returns them (0 Swim,
/// 1 Fly, 2 Run, 3 Power, 4 Stamina).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skills {
    pub swim: u16,
    pub fly: u16,
    pub run: u16,
    pub power: u16,
    pub stamina: u16,
}

/// A fighter's bout values (fn_72_11038). A player's Chao gets no random
/// spread: the game applies its 0.95..1.05 spread only to computer opponents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fighter {
    /// +0x20 = 1000 + Swim.
    pub defence: f32,
    /// +0x24 = 1000 + Fly.
    pub evade: f32,
    /// +0x28 = (1000 + 1.5 * Run) / 1000: how fast the attack gauge fills.
    pub speed: f32,
    /// +0x2C = 1200 + Power.
    pub attack: f32,
    /// +0x30 = 1000 + Run / 2 - Power / 3.
    pub accuracy: f32,
    /// +0x3C, starting at +0x44.
    pub health: f32,
    /// +0x44 = 1000 + Stamina.
    pub max_health: f32,
}

impl Fighter {
    pub fn new(s: Skills) -> Self {
        let max_health = 1000.0 + f32::from(s.stamina);
        Self {
            defence: 1000.0 + f32::from(s.swim),
            evade: 1000.0 + f32::from(s.fly),
            speed: (1000.0 + 1.5 * f32::from(s.run)) / 1000.0,
            attack: 1200.0 + f32::from(s.power),
            accuracy: 1000.0 + f32::from(s.run) / 2.0 - f32::from(s.power) / 3.0,
            health: max_health,
            max_health,
        }
    }

    pub fn knocked_out(&self) -> bool {
        self.health <= 0.0
    }
}

/// The five attacks, numbered as the module's action table (sec5 0x168A0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attack {
    HardKick = 4,
    Roundhouse = 5,
    BeatingUp = 6,
    Punch = 7,
    WindupPunch = 8,
}

impl Attack {
    pub fn from_id(id: u64) -> Option<Self> {
        Some(match id {
            4 => Self::HardKick,
            5 => Self::Roundhouse,
            6 => Self::BeatingUp,
            7 => Self::Punch,
            8 => Self::WindupPunch,
            _ => return None,
        })
    }

    /// fn_72_FCC8 picks the windup punch or roundhouse only when the fighter's
    /// attack is above 1600 (lbl_72_data_16874), which is Power above 400.
    pub fn allowed(self, fighter: &Fighter) -> bool {
        !matches!(self, Self::Roundhouse | Self::WindupPunch) || fighter.attack > 1600.0
    }
}

/// A player's chosen move. The game lets a fighter attack only when its
/// approach ends (fn_72_FCC8), so a press never adds an attack. Once a move is
/// chosen it is seen through: no other press is taken until that attack has
/// finished. A strong move the fighter may not use is refused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Chosen {
    #[default]
    Nothing,
    Waiting(Attack),
    Attacking,
}

impl Chosen {
    pub fn press(&mut self, attack: Attack, fighter: &Fighter) -> bool {
        let ok = *self == Self::Nothing && attack.allowed(fighter);
        if ok {
            *self = Self::Waiting(attack);
        }
        ok
    }

    /// The fighter starts an attack now: the chosen move, if one is waiting.
    pub fn start(&mut self) -> Option<Attack> {
        let Self::Waiting(attack) = *self else {
            return None;
        };
        *self = Self::Attacking;
        Some(attack)
    }

    /// The fighter's attack is over; the next press is taken again.
    pub fn finish(&mut self) {
        if *self == Self::Attacking {
            *self = Self::Nothing;
        }
    }
}

/// Gauge a fighter needs before it attacks (+0x48, fn_72_FAB8): 400
/// (lbl_72_data_1681C), or 0.87 of that (lbl_72_data_16860) once its attack
/// is 1600 or more. The gauge is set to 0 when an attack connects or ends.
pub fn gauge_needed(fighter: &Fighter) -> f32 {
    if fighter.attack < 1600.0 {
        400.0
    } else {
        400.0 * 0.87
    }
}

/// Gauge gained in one frame while waiting to attack (fn_72_FAB8), with one
/// roll: speed * (1 + 0.15 * roll - 0.3).
pub fn gauge_step(fighter: &Fighter, roll: f32) -> f32 {
    fighter.speed * (1.0 + 0.15 * roll - 0.3)
}

/// Gauge gained in one frame while reeling from a hit (fn_72_1010C), with one
/// roll: speed * (1 + 0.1 * roll - 0.2) * 0.5 (lbl_72_data_16870).
pub fn gauge_step_hurt(fighter: &Fighter, roll: f32) -> f32 {
    fighter.speed * (1.0 + 0.1 * roll - 0.2) * 0.5
}

/// When an attack's steps fall, in frames after the frame it was picked
/// (fn_72_FCC8). `roll` is the frame the attacker takes the hit roll
/// (fn_72_FE9C step 1), `contact` the frame a landed hit is handed to the
/// defender and the attacker's gauge is cleared (step 2), `done` the frame the
/// attacker goes back to filling its gauge (step 3).
/// Unverified: whether the Chao's motion advances before or after the Karate
/// fighter's own step within a frame; this takes after, so every frame here
/// may be one too late.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeline {
    pub roll: u32,
    pub contact: u32,
    pub done: u32,
}

/// Frames the engine takes to blend into a motion over `step` link frames
/// (main.dol 0x8002D224): it adds 1 / step each frame until the sum is 1.
fn link_frames(step: u16) -> u32 {
    let (rate, mut sum, mut frames) = (1.0 / f32::from(step), 0.0_f32, 0);
    while sum < 1.0 {
        sum += rate;
        frames += 1;
    }
    frames
}

/// Frames a motion playing from frame 0 at `speed` takes to reach `mark`
/// (same function: the frame grows by the motion's speed each game frame).
fn frames_to(speed: f32, mark: f32) -> u32 {
    let (mut at, mut frames) = (0.0_f32, 0);
    while at < mark {
        at += speed;
        frames += 1;
    }
    frames
}

impl Attack {
    /// The attack's schedule, from its action row (ChaoStgKarate .data
    /// 0x168A0 + id * 0x2C: link step 15 for all five, frame marks at +0x18)
    /// and its motion row (ChaoMain motion table, row 0x1C, speed at +0x18,
    /// last frame at +0x14, all five start at frame 0; GameCube ChaoMain
    /// .data 0x3F870, the Steam ChaoMain.rel holds the same rows).
    pub fn timeline(self) -> Timeline {
        let link = link_frames(15);
        // (motion speed, last frame, frame the hit roll opens, contact frame)
        let (speed, last, opens, contact) = match self {
            // fn_72_BDBC runs on its own 15-frame counter, not on the motion.
            Self::BeatingUp => {
                return Timeline {
                    roll: link + 1,
                    contact: link + 4,
                    done: link + 10,
                };
            }
            // fn_72_C0C4: the roll opens when the blend ends. Motions 579, 582.
            Self::HardKick => (0.18, 9.0, None, 4.0),
            Self::Punch => (0.15, 8.0, None, 2.5),
            // fn_72_C3B4: the roll opens at a mark of its own. Motions 580, 583.
            Self::Roundhouse => (0.14, 12.0, Some(0.0), 6.3),
            Self::WindupPunch => (0.19, 16.0, Some(0.0), 10.5),
        };
        // A strong attack spends one more handler step before each mark.
        let late = u32::from(opens.is_some());
        Timeline {
            roll: link + 1 + late,
            contact: link + frames_to(speed, contact).max(1 + late) + 1,
            done: link + frames_to(speed, last) + 1,
        }
    }
}

/// Chance that an attack in reach lands (fn_72_EA20): certain against a
/// defender who is attacking or has given up (its phases 4 and 7), otherwise
/// 0.1 + 1.5 * accuracy / (accuracy + evade), at most 1. The attack lands when
/// the roll is at or below the chance. Reach (distance at most 5.0) is the
/// caller's check. The module's other formula, for opponent kind 5, never
/// applies to a player's Chao and is not ported.
pub fn hit_chance(accuracy: f32, evade: f32, defender_committed: bool) -> f32 {
    if defender_committed {
        return 1.0;
    }
    (0.1 + 1.5 * accuracy / (accuracy + evade)).min(1.0)
}

/// Damage of a landed attack (fn_72_1010C). `critical`, `second` and `spread`
/// are the three rolls in the order the game takes them: a critical (x1.3)
/// needs the first below 0.1 and the second above 0.9; otherwise the attack
/// is scaled by 1.1 - 0.2 * spread. Defence is subtracted and the result is
/// at least 5% of the defender's full health.
pub fn damage(attack: f32, defender: &Fighter, critical: f32, second: f32, spread: f32) -> f32 {
    let scaled = if critical < 0.1 && second > 0.9 {
        attack * 1.3
    } else {
        attack * (1.1 - 0.2 * spread)
    };
    (scaled - defender.defence).max(defender.max_health * (50.0 / 1000.0))
}

/// Take `amount` off a fighter's health; it stops at 0 (a knock-out).
pub fn hurt(defender: &mut Fighter, amount: f32) {
    defender.health = (defender.health - amount).max(0.0);
}

/// Seconds a bout lasts (fn_72_A390 sets 0x5A).
pub const BOUT_SECONDS: u32 = 90;

/// Who wins when time runs out (fn_72_133CC): each side's health share as a
/// rounded percentage; the larger wins and equal shares are a draw.
/// Unverified: how the game's njRoundOff treats an exact half.
pub fn judge(first: &Fighter, second: &Fighter) -> Option<usize> {
    let share = |f: &Fighter| (100.0 * f.health / f.max_health).round();
    let (a, b) = (share(first), share(second));
    if a == b {
        None
    } else {
        Some(usize::from(a < b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skills(power: u16) -> Skills {
        Skills {
            swim: 300,
            fly: 200,
            run: 600,
            power,
            stamina: 500,
        }
    }

    #[test]
    fn fighter_values_follow_the_setup_formulas() {
        let f = Fighter::new(skills(450));
        assert_eq!((f.defence, f.evade, f.attack), (1300.0, 1200.0, 1650.0));
        assert_eq!(f.accuracy, 1000.0 + 300.0 - 150.0);
        assert_eq!((f.health, f.max_health), (1500.0, 1500.0));
    }

    #[test]
    fn strong_moves_need_attack_above_1600() {
        let at = Fighter::new(skills(400));
        let above = Fighter::new(skills(401));
        for a in [Attack::Roundhouse, Attack::WindupPunch] {
            assert!(!a.allowed(&at));
            assert!(a.allowed(&above));
        }
        for a in [Attack::HardKick, Attack::BeatingUp, Attack::Punch] {
            assert!(a.allowed(&at));
        }
        assert_eq!(Attack::from_id(6), Some(Attack::BeatingUp));
        assert_eq!(Attack::from_id(9), None);
    }

    #[test]
    fn a_chosen_move_is_seen_through() {
        let weak = Fighter::new(skills(0));
        let mut c = Chosen::default();
        assert!(!c.press(Attack::Roundhouse, &weak));
        assert!(c.press(Attack::Punch, &weak));
        // Waiting and attacking both refuse another move.
        assert!(!c.press(Attack::HardKick, &weak));
        c.finish(); // nothing is attacking yet: the choice stays
        assert_eq!(c.start(), Some(Attack::Punch));
        assert!(!c.press(Attack::HardKick, &weak));
        assert_eq!(c.start(), None);
        c.finish();
        assert!(c.press(Attack::HardKick, &weak));
    }

    #[test]
    fn the_gauge_fills_by_speed_and_strong_fighters_need_less() {
        let mut s = skills(0);
        s.run = 1000;
        let quick = Fighter::new(s);
        assert_eq!(quick.speed, 2.5);
        assert_eq!(gauge_step(&quick, 0.0), 2.5 * 0.7);
        assert_eq!(gauge_step_hurt(&quick, 0.0), 2.5 * 0.8 * 0.5);
        assert_eq!(gauge_needed(&Fighter::new(skills(399))), 400.0);
        assert_eq!(gauge_needed(&Fighter::new(skills(400))), 400.0 * 0.87);
    }

    #[test]
    fn attack_schedules_follow_the_action_and_motion_rows() {
        let at = |a: Attack| {
            let t = a.timeline();
            (t.roll, t.contact, t.done)
        };
        assert_eq!(at(Attack::HardKick), (16, 39, 67));
        assert_eq!(at(Attack::Roundhouse), (17, 62, 102));
        assert_eq!(at(Attack::BeatingUp), (16, 19, 25));
        assert_eq!(at(Attack::Punch), (16, 33, 70));
        assert_eq!(at(Attack::WindupPunch), (17, 72, 101));
    }

    #[test]
    fn hit_chance_is_capped_and_certain_against_a_committed_defender() {
        assert_eq!(hit_chance(1000.0, 1000.0, false), 0.1 + 0.75);
        assert_eq!(hit_chance(3000.0, 1000.0, false), 1.0);
        assert_eq!(hit_chance(1.0, 9000.0, true), 1.0);
    }

    #[test]
    fn damage_takes_the_critical_only_on_both_rolls_and_keeps_the_floor() {
        let d = Fighter::new(skills(0));
        assert_eq!(damage(2000.0, &d, 0.05, 0.95, 0.0), 2000.0 * 1.3 - 1300.0);
        // First roll alone is not a critical: the spread roll applies.
        assert_eq!(
            damage(2000.0, &d, 0.05, 0.5, 0.5),
            2000.0 * (1.1 - 0.1) - 1300.0
        );
        // A weak attack still takes 5% of full health.
        assert_eq!(damage(1200.0, &d, 0.5, 0.5, 1.0), 75.0);
    }

    #[test]
    fn health_stops_at_zero_and_the_judge_compares_rounded_shares() {
        let mut a = Fighter::new(skills(0));
        let mut b = a;
        hurt(&mut a, 600.0);
        assert_eq!(judge(&a, &b), Some(1));
        hurt(&mut b, 603.0); // 59.8% rounds to the same 60 as 60.0%
        assert_eq!(judge(&a, &b), None);
        hurt(&mut b, 5000.0);
        assert!(b.knocked_out());
        assert_eq!(judge(&a, &b), Some(0));
    }
}
