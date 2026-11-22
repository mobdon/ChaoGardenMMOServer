//! Player-versus-player Chao Karate: the bout rules the server decides.
//!
//! Ported from the GameCube ChaoStgKarate module (GSNE8P module 72), read from
//! its disassembly; each item names the function it comes from. Offsets are
//! fields of the fighter's work area as that module uses them. Only the rules
//! below are ported so far: fighter values, the strong-move gate, the hit
//! roll, damage and the time-up judge. Move timing, movement and ring-out are
//! not ported yet, so nothing here runs a bout on its own.
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
