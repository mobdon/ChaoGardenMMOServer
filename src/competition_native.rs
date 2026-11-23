//! Native Chao Race and Chao Karate rules (GameCube SA2B, GSNE8P).
//!
//! Ported read-only from the owner's own disc: `ChaoStgKarate.rel` (module 72,
//! SHA-1 7379b87d…), `ChaoStgRace.rel` (module 76, SHA-1 2a13767b…) and
//! `ChaoMain.rel` (module 60, `CM`, SHA-1 e7e24b8b…). Addresses are `.text`
//! offsets of those modules. The same routines live in the client crate
//! (`asset-tools/crates/sa2/src/chao_karate.rs`, `chao_race.rs`), where each is
//! checked bit for bit against the module's PowerPC under Unicorn; this file is
//! a line-for-line copy of that logic for the authority.
//!
//! Tables (karate opponents, race courses) are read at runtime from the
//! owner's install via the client CLI `chao-competition-tables`, never from git.
//! Nothing here is an MMO choice; those live in `competition_mmo`.
use serde::{Deserialize, Serialize};

/// main 0x8010DC88 `rand` (MSL): next = next * 0x41C64E6D + 0x3039,
/// result (next >> 16) & 0x7FFF. Power-on `next` is 1 (.sdata 0x803AD6A0).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MslRand {
    pub next: u32,
}

impl Default for MslRand {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl MslRand {
    pub fn rand(&mut self) -> i32 {
        self.next = self.next.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
        ((self.next >> 16) & 0x7FFF) as i32
    }
}

/// rand() / 32768 (rodata 0x45C karate, 0xC88 race: 3.0517578125e-05).
fn unit(r: i32) -> f32 {
    r as f32 * 3.051_757_8e-5
}

pub mod karate {
    use super::{MslRand, unit};
    use serde::{Deserialize, Serialize};

    /// CM ChaoGlobal+0x126 `SkillWalk`.
    pub const SKILL_WALK: u16 = 150;
    /// fn_72_A390 0xA3A0: 90-second round clock, one second per 60 frames (fn_72_A22C).
    pub const ROUND_SECONDS: u32 = 90;
    pub const FRAMES_PER_SECOND: u32 = 60;
    /// fn_72_5020 0x50B8: five rounds per class, descriptor row = class * 5 + round + 2.
    pub const ROUNDS: u8 = 5;
    /// Emblem bytes save+0x3231 + class (fn_72_5D48 0x5E4C).
    pub const CLASSES: u8 = 4;

    pub mod state {
        pub const READY: u8 = 2;
        pub const CHOOSE: u8 = 3;
        pub const ATTACK: u8 = 4;
        pub const HURT: u8 = 6;
        pub const STAGGER: u8 = 7;
    }
    pub mod flag {
        /// Set by a cheer press until its gain lands (fn_72_EDE8 0xF1DC).
        pub const CHEERED: u16 = 0x2000;
        pub const KNOCKED_OUT: u16 = 0x1000;
        pub const GIVE_UP: u16 = 0x800;
        pub const ATTACK_HIT: u16 = 0x400;
        pub const HIT: u16 = 0x200;
        pub const CHECKED: u16 = 0x100;
        pub const MOTION_END: u16 = 0x8000;
    }

    /// Work +0x20..+0x48, fn_72_11038.
    #[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Fighter {
        pub defence: f32,
        pub evasion: f32,
        pub speed: f32,
        pub power: f32,
        pub accuracy: f32,
        pub intellect: f32,
        pub zeal: f32,
        pub hp: f32,
        pub luck: f32,
        pub hp_max: f32,
        pub charge: f32,
        pub flags: u16,
    }

    /// Task+0x34 state record.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    pub struct StateRecord {
        pub state: u8,
        pub step: u8,
        pub frame: u8,
        pub flags: u16,
        pub timer: u16,
    }

    /// fn_72_11038 0x112DC..0x11A3C. `skills` = AL_ParameterGetSkill(0..=6),
    /// raw points in the karate stage (CM 0x20934).
    pub fn fighter(
        skills: [u16; 7],
        cpu: bool,
        child: bool,
        d6: bool,
        rng: &mut MslRand,
    ) -> Fighter {
        let mut jitter = |s: u16| -> u16 {
            if !cpu {
                return s;
            }
            let f2 = 1.0f32 + (0.05f32 - 0.1f32 * unit(rng.rand()));
            (s as f32 * f2) as i32 as u16
        };
        let swim = jitter(skills[0]);
        let fly = jitter(skills[1]);
        let run = jitter(skills[2]);
        let power = jitter(skills[3]);
        let stamina = jitter(skills[4]);
        let intellect = jitter(skills[6]);
        let hp = 1000.0f32 + stamina as f32 * 1.0f32;
        let mut flags = if cpu { 4 } else { 0 };
        if child && run < SKILL_WALK {
            flags |= 1;
        }
        if d6 {
            flags |= 8;
        }
        Fighter {
            defence: 1000.0f32 + swim as f32 * 1.0f32,
            evasion: 1000.0f32 + fly as f32 * 1.0f32,
            speed: (1000.0f32 + run as f32 * 1.5f32) / 1000.0f32,
            power: 1200.0f32 + power as f32 * 1.0f32,
            accuracy: (1000.0f32 + run as f32 / 2.0f32) - power as f32 / 3.0f32,
            intellect: intellect as f32 * 1.0f32,
            zeal: 1000.0,
            hp,
            luck: skills[5] as f32,
            hp_max: hp,
            charge: 0.0,
            flags,
        }
    }

    /// fn_72_1010C step 0 (falls into step 1). Returns the damage.
    pub fn take_hit(
        me: &mut Fighter,
        rec: &mut StateRecord,
        attack: f32,
        kind: i16,
        fighting: bool,
        rng: &mut MslRand,
    ) -> f32 {
        rec.flags &= !0x2000;
        let mut f30 = attack;
        if unit(rng.rand()) < 0.1f32 && unit(rng.rand()) > 0.9f32 {
            f30 *= 1.3f32;
        } else {
            f30 *= 1.1f32 - 0.2f32 * unit(rng.rand());
        }
        let mut damage = f30 - me.defence;
        let floor = if kind == 6 {
            0.0f32
        } else {
            me.hp_max * (50.0f32 / 1000.0f32)
        };
        if damage < floor {
            damage = floor;
        }
        me.hp -= damage;
        if me.hp < 0.0 {
            me.hp = 0.0;
            if fighting {
                rec.flags |= flag::KNOCKED_OUT;
            }
        }
        me.zeal -= damage / 3.0f32;
        if me.zeal < 0.0 {
            me.zeal = 0.0;
        }
        me.flags &= !2;
        rec.flags &= !flag::GIVE_UP;
        rec.step = 1;
        hurt_step(me, rec, rng);
        damage
    }

    /// fn_72_1010C step 1 (0x10528..0x1069C).
    pub fn hurt_step(me: &mut Fighter, rec: &mut StateRecord, rng: &mut MslRand) {
        let f1 = (1.0f32 + 0.1f32 * unit(rng.rand())) - 0.2f32;
        me.charge += 0.5f32 * (me.speed * f1);
        if rec.flags & flag::MOTION_END == 0 {
            return;
        }
        rec.step = 0;
        rec.state = if me.power < 1600.0 {
            if me.charge >= 400.0 {
                state::CHOOSE
            } else {
                state::READY
            }
        } else if me.zeal == 0.0 {
            state::STAGGER
        } else if me.charge >= 400.0f32 * 0.87f32 {
            state::CHOOSE
        } else {
            state::READY
        };
    }

    /// fn_72_EA20 (distance beyond 5.0 never hits).
    pub fn hit_check(
        rec: &mut StateRecord,
        evasion: f32,
        kind: i16,
        accuracy: f32,
        distance: f32,
        rng: &mut MslRand,
    ) -> bool {
        if distance > 5.0 {
            return false;
        }
        rec.flags |= flag::CHECKED;
        let mut p = if rec.state == state::STAGGER || rec.state == state::ATTACK {
            1.0f32
        } else if kind == 5 {
            0.5f32 * 0.1f32 + 0.6f32 * (accuracy / (accuracy + evasion))
        } else {
            0.1f32 + 1.5f32 * (accuracy / (accuracy + evasion))
        };
        if p > 1.0 {
            p = 1.0;
        }
        if unit(rng.rand()) <= p {
            rec.flags |= flag::HIT;
            true
        } else {
            rec.flags &= !flag::HIT;
            false
        }
    }

    /// fn_72_EDE8. `cheer` is the 2-player cheering branch (0xF1B8..0xF318):
    /// `Some(pressed)` when the fighter can be cheered this frame (its action
    /// record's +5 bit 0x10 is set) and whether its player newly pressed A.
    /// `None` leaves cheering out; the 1-player branch (0xEF10) is not ported.
    pub fn zeal_step(
        me: &mut Fighter,
        rec: &mut StateRecord,
        side: i16,
        leader: i16,
        rng: &mut MslRand,
        cheer: Option<bool>,
    ) {
        if matches!(rec.state, 1 | 0xA | 0xB) {
            return;
        }
        me.zeal -= 1000.0f32 / (1800.0f32 + 1800.0f32 * (me.intellect / 1000.0f32));
        if let (state::STAGGER, Some(pressed)) = (rec.state, cheer) {
            if pressed {
                rec.flags |= flag::CHEERED;
            }
            // The gain lands on frame 5 of the 0..=60 counter (0xF220).
            if rec.frame == 5 && rec.flags & flag::CHEERED != 0 {
                // lbl_72_data_16844, _16848, _1684C; capped by lbl_72_rodata_480.
                let gain = 100.0f32 + 300.0f32 * (me.luck / 1000.0f32);
                let _ = rng.rand(); // 0xF270: taken, result not used
                me.zeal += gain.min(200.0);
                rec.flags &= !flag::CHEERED;
            }
        }
        if me.zeal > 1000.0 {
            me.zeal = 1000.0;
        } else if me.zeal < 0.0 {
            me.zeal = 0.0;
            rec.flags |= flag::GIVE_UP;
        }
        if rec.state != state::READY {
            return;
        }
        if rec.frame == 0 && me.zeal < 500.0 {
            let span = if side == leader { 2000.0f32 } else { 4000.0f32 };
            if unit(rng.rand()) > 1.0f32 - (500.0f32 - me.zeal) / span {
                rec.flags |= flag::GIVE_UP;
            }
        }
        if rec.flags & flag::GIVE_UP != 0 {
            rec.state = state::STAGGER;
            rec.step = 0;
        }
    }

    /// fn_72_FAB8 step 1.
    pub fn ready_step(me: &mut Fighter, rec: &mut StateRecord, rng: &mut MslRand) {
        let f2 = (1.0f32 + 0.15f32 * unit(rng.rand())) - 0.3f32;
        me.charge += me.speed * 1.0f32 * f2;
        let threshold = if me.power < 1600.0 {
            400.0f32
        } else {
            400.0f32 * 0.87f32
        };
        if me.charge >= threshold {
            rec.state = state::CHOOSE;
            rec.step = 0;
        }
    }

    /// fn_72_FE9C step 3 at the end of the attack motion.
    pub fn attack_end(me: &mut Fighter, rec: &mut StateRecord) {
        rec.timer = 0x1E;
        if rec.flags & flag::ATTACK_HIT != 0 {
            me.zeal += 50.0;
            if me.zeal > 1000.0 {
                me.zeal = 1000.0;
            }
            rec.flags &= !flag::ATTACK_HIT;
        } else {
            let mut cost = 100.0f32 - -50.0f32 * (me.luck / 1000.0f32);
            if cost < 0.0 {
                cost = 0.0;
            }
            me.zeal -= cost;
            if me.zeal < 0.0 {
                me.zeal = 0.0;
            }
        }
        me.charge = 0.0;
        rec.state = state::READY;
        rec.step = 0;
    }

    /// fn_72_133CC (njRoundOff truncates).
    pub fn leader(hp: [(f32, f32); 2]) -> i16 {
        let pct = |(h, m): (f32, f32)| 100.0f32 * (h / m);
        let (a, b) = (pct(hp[0]), pct(hp[1]));
        if a.trunc() == b.trunc() {
            2
        } else if a > b {
            0
        } else {
            1
        }
    }

    /// One opponent descriptor (ChaoStgKarate .data 0x1008 = sonic2app.exe 0x01296940).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct Descriptor {
        pub skills: u16,
        pub name: u16,
        pub appearance: i8,
        pub kind: i16,
        pub alternate: i8,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Tables {
        pub descriptors: Vec<Descriptor>,
        pub skills: Vec<[u16; 8]>,
    }

    impl Tables {
        pub fn valid(&self) -> bool {
            self.descriptors.len() == 27
                && self.skills.len() == 28
                && self.descriptors.iter().all(|d| {
                    usize::from(d.skills) < self.skills.len()
                        && (d.alternate < 0 || (d.alternate as usize) < self.descriptors.len())
                })
        }
    }

    /// fn_72_5020 0x50B0..0x514C.
    pub fn opponent_row(
        rows: &[Descriptor],
        class: u8,
        round: u8,
        rng: &mut MslRand,
    ) -> Option<usize> {
        let row = 2 + usize::from(class) * 5 + usize::from(round);
        let d = rows.get(row)?;
        if d.alternate >= 0 && unit(rng.rand()) > 0.5 {
            let alt = d.alternate as usize;
            return rows.get(alt).map(|_| alt);
        }
        Some(row)
    }
}

pub mod race {
    use super::unit;
    use serde::{Deserialize, Serialize};

    /// CM ChaoGlobal+0x126 `SkillWalk`.
    pub const SKILL_WALK: u16 = 150;
    /// CM ChaoGlobal+0x274 `RaceIntervalSubStamina`, +0x280 `RaceSubStaminaRun`.
    /// The other RaceSubStamina*/RaceTurboRatio* fields have no reader in
    /// ChaoMain or ChaoStgRace, so only running costs stamina.
    pub const INTERVAL_SUB_STAMINA: u32 = 12;
    pub const SUB_STAMINA_RUN: i32 = 20;
    pub const LANES: usize = 8;

    /// AL_ParameterGetSkill (CM 0x20934) in the race stage: points * 3, as u16.
    pub fn race_skill(points: u32) -> u16 {
        (points.wrapping_mul(3) & 0xFFFF) as u16
    }

    fn condition(a: f32, stamina: i32, turbo: bool, ratio: f32) -> f32 {
        if stamina == 0 {
            a * 0.7f32
        } else if turbo {
            a * ratio
        } else {
            a
        }
    }
    fn base(skill: u16, base: f32, ratio: f32) -> f32 {
        base + ratio * (1.0e-4f32 * skill as f32)
    }

    /// fn_76_C3BC run acceleration (no drag term).
    pub fn run_accel(skill: u16, stamina: i32, turbo: bool) -> f32 {
        condition(base(skill, 0.095, 0.055), stamina, turbo, 1.3)
    }
    /// fn_76_C500 fly: forward accel and the per-frame horizontal drag 0.1.
    pub fn fly_accel(skill: u16, stamina: i32, turbo: bool) -> f32 {
        condition(base(skill, 0.018, 0.067), stamina, turbo, 1.2)
    }
    pub const FLY_DRAG: f32 = 0.1;
    /// fn_76_C500: vertical sink 0.01 + 0.005 * (10000 - skill) / 10000.
    pub fn fly_sink(skill: u16) -> f32 {
        0.01f32 + 0.005f32 * (1.0e-4f32 * (10000 - i32::from(skill)) as f32)
    }
    /// fn_76_C6EC swim: accel and the horizontal drag 0.06.
    pub fn swim_accel(skill: u16, stamina: i32, turbo: bool) -> f32 {
        condition(base(skill, 0.0027, 0.018), stamina, turbo, 1.3)
    }
    pub const SWIM_DRAG: f32 = 0.06;
    /// fn_76_C864 climb: the position rises by this much per frame.
    pub fn climb_step(skill: u16, stamina: i32, turbo: bool) -> f32 {
        condition(base(skill, 0.04, 0.1), stamina, turbo, 1.3)
    }

    /// fn_76_D2B4 one frame after its first step: trip roll every 50 frames
    /// above SkillWalk (p = 0.001 + 1e-6 * (10000 - skill); never when Chao
    /// param +0xDD == 3), frame counter, and the run stamina drain every 12.
    pub fn run_frame(
        skill: u16,
        counter: &mut i32,
        stamina: &mut i32,
        param_dd: u8,
        rng: &mut super::MslRand,
    ) -> bool {
        let mut trip = false;
        if skill > SKILL_WALK && *counter % 50 == 0 {
            let u = unit(rng.rand());
            let p = 0.001f32 + 1.0e-6f32 * (10000 - i32::from(skill)) as f32;
            trip = u < p && param_dd != 3;
        }
        *counter = counter.wrapping_add(1);
        if (*counter as u32).is_multiple_of(INTERVAL_SUB_STAMINA) {
            *stamina = sub_stamina(*stamina, SUB_STAMINA_RUN);
        }
        trip
    }

    /// CM fn_60_2105C: work +0x88 += -amount, clamped at 0.
    pub fn sub_stamina(stamina: i32, amount: i32) -> i32 {
        stamina.wrapping_add(i32::from(-(amount as i16))).max(0)
    }

    /// fn_76_1764 0x1988: GetSkill(stamina) * 5 + 10000.
    pub fn start_stamina(skill: u16) -> i32 {
        i32::from(skill) * 5 + 10000
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub enum Class {
        Beginner,
        Jewel,
        Challenge,
        Hero,
        Dark,
    }

    impl Class {
        /// CM fn_60_24798 / fn_60_24ABC: course numbers and levels per course.
        pub fn courses(self) -> (std::ops::Range<u8>, u8) {
            match self {
                Class::Beginner => (0..4, 3),
                Class::Jewel => (4..10, 5),
                Class::Challenge => (10..11, 12),
                Class::Hero => (11..12, 4),
                Class::Dark => (12..13, 4),
            }
        }
        pub fn row(self, course: u8, level: u8) -> Option<usize> {
            let (courses, levels) = self.courses();
            if !courses.contains(&course) || level >= levels {
                return None;
            }
            let level = usize::from(level);
            Some(match self {
                Class::Beginner => usize::from(course) * 3 + level,
                Class::Jewel => 12 + usize::from(course - 4) * 5 + level,
                Class::Challenge => 42 + level,
                Class::Hero => 54 + level,
                Class::Dark => 58 + level,
            })
        }
    }

    /// CM .data 0x939C0 = sonic2app.exe 0x01354828, 62 rows of 20 bytes.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct CourseRow {
        pub index: i32,
        /// +4: land 0 neutral, 1 hero, 2 dark.
        pub land: i32,
        /// +8: section set 0..9 (BegSwm BegFly BegRun BegPow JwlSwm JwlFly JwlRun JwlPow JwlIL JwlLng).
        pub sections: i32,
        pub e010: i32,
        /// +0x10: toy or garden flag unlocked by a win, or -1.
        pub unlock: i32,
    }

    /// AI racer record (CM .data 0x93ED4 + 0x54 i = sonic2app.exe 0x011CE2A0).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct RaceBot {
        pub mode: u16,
        pub shared: u32,
        pub pairs: [[u16; 2]; 7],
    }

    /// Rival set (CM .data 0x93E98 + 0x25F8 + 0x24 i = sonic2app.exe 0x011D0888).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct RivalSet {
        pub fixed: i32,
        pub unknown4: i32,
        pub bots: [i32; 7],
    }

    /// CM fn_60_24E40: the race's rival set.
    pub fn rival_set_index(
        class: Class,
        course: u8,
        level: u8,
        rows: &[CourseRow],
    ) -> Option<usize> {
        let row = class.row(course, level)?;
        match class {
            Class::Challenge => usize::try_from(rows.get(row)?.index).ok().map(|i| 42 + i),
            _ => Some(row),
        }
    }

    /// CM fn_60_24F48 0x24F98..0x25064: 32 random lane swaps unless +0 is set.
    pub fn rival_order(set: &RivalSet, rng: &mut super::MslRand) -> [usize; 7] {
        let mut order = [0, 1, 2, 3, 4, 5, 6];
        if set.fixed == 0 {
            for _ in 0..32 {
                let a = (6.999_9f32 * unit(rng.rand())) as i32 as usize;
                let b = (6.999_9f32 * unit(rng.rand())) as i32 as usize;
                order.swap(a, b);
            }
        }
        order
    }

    /// CM fn_60_253D4 0x26198..0x263D0: rival param skills (swim..intellect),
    /// already divided by 3. Its earlier appearance draws are not ported.
    pub fn rival_skills(bot: &RaceBot, rng: &mut super::MslRand) -> [u16; 7] {
        let mut s = [0u16; 7];
        match bot.mode {
            0 | 2 => {
                let base = bot.shared / 7;
                for v in &mut s {
                    let r = (500.0f32 * unit(rng.rand())) as i32 as u16;
                    *v = (i32::from(base.wrapping_add(u32::from(r)) as u16) / 3) as u16;
                }
            }
            1 => {
                for (i, v) in s.iter_mut().enumerate() {
                    let k = 2.0f32 * unit(rng.rand()) - 1.0f32;
                    let r = (bot.pairs[i][1] as f32 * k) as i32;
                    *v = (i32::from(i32::from(bot.pairs[i][0]).wrapping_add(r) as u16) / 3) as u16;
                }
                s[4] = s[6];
                s[5] = s[6];
            }
            _ => {}
        }
        s
    }

    /// Race progress (GardenInfo+0x780+0x13; AL_InitRaceData starts at -1).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Progress(pub [i8; 13]);

    impl Default for Progress {
        fn default() -> Self {
            Self([-1; 13])
        }
    }

    impl Progress {
        /// CM fn_60_24ABC.
        pub fn record(&mut self, course: i32, level: i32) {
            let Ok(c) = usize::try_from(course) else {
                return;
            };
            let Some(old) = self.0.get(c) else { return };
            if level <= i32::from(*old) {
                return;
            }
            let cap = match c {
                0..=3 => 3,
                4..=9 => 5,
                10 => 12,
                11 | 12 => 4,
                _ => return,
            };
            if level < cap {
                self.0[c] = level as i8;
            }
        }
        /// CM fn_60_24B6C.
        pub fn get(&self, course: i32) -> i32 {
            usize::try_from(course)
                .ok()
                .and_then(|c| self.0.get(c))
                .map_or(course, |v| i32::from(*v))
        }
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Award {
        pub unlock: Option<i32>,
        /// Chao param +0xD7 medal and its +0x108 bit.
        pub medal: Option<(u8, u16)>,
        /// Emblem bytes save+0x3224 + i newly set.
        pub emblems: Vec<usize>,
    }

    /// fn_76_698 0x938..0x1024: a win (first finisher is lane 0).
    pub fn win(
        progress: &mut Progress,
        emblems: &[bool; 5],
        class: Class,
        course: u8,
        level: u8,
        row: &CourseRow,
    ) -> Award {
        let mut award = Award {
            unlock: (row.unlock != -1).then_some(row.unlock),
            ..Default::default()
        };
        progress.record(i32::from(course), i32::from(level));
        let all = |courses: std::ops::Range<i32>, at_least: i32| {
            courses.clone().all(|c| progress.get(c) >= at_least)
        };
        award.medal = match class {
            Class::Beginner if all(0..4, 2) && level == 2 => Some((7, 2)),
            Class::Jewel if level == 4 => Some((course - 3, 1 << course)),
            Class::Challenge if level == 11 => Some((8, 1)),
            Class::Hero if level == 3 => Some((9, 4)),
            Class::Dark if level == 3 => Some((10, 8)),
            _ => None,
        };
        let checks = [
            all(0..4, 2),
            all(4..10, 4),
            progress.get(10) >= 11,
            progress.get(11) >= 3,
            progress.get(12) >= 3,
        ];
        let range = if matches!(class, Class::Beginner | Class::Jewel) {
            0..2
        } else {
            2..5
        };
        for i in range {
            if checks[i] && !emblems[i] {
                award.emblems.push(i);
            }
        }
        award
    }
}

/// The native tables, exported from the owner's sonic2app.exe by the client
/// CLI `chao-competition-tables` into `CHAO_DATA_DIR/activities/competition-native.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTables {
    pub karate: karate::Tables,
    pub race_courses: Vec<race::CourseRow>,
    pub race_bots: Vec<race::RaceBot>,
    pub rival_sets: Vec<race::RivalSet>,
}

impl NativeTables {
    pub fn from_value(v: &serde_json::Value) -> Result<Self, String> {
        let t = NativeTables {
            karate: serde_json::from_value(v["karate"].clone())
                .map_err(|e| format!("Karate tables: {e}"))?,
            race_courses: serde_json::from_value(v["raceCourses"].clone())
                .map_err(|e| format!("Race courses: {e}"))?,
            race_bots: serde_json::from_value(v["raceBots"].clone())
                .map_err(|e| format!("Race bots: {e}"))?,
            rival_sets: serde_json::from_value(v["rivalSets"].clone())
                .map_err(|e| format!("Rival sets: {e}"))?,
        };
        if !t.karate.valid()
            || t.race_courses.len() != 62
            || t.race_bots.len() != 115
            || t.rival_sets.len() != 62
            || t.rival_sets
                .iter()
                .any(|s| s.bots.iter().any(|b| !(0..115).contains(b)))
        {
            return Err("Native competition tables have the wrong shape".into());
        }
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msl_rand_matches_the_native_sequence_start() {
        let mut r = MslRand::default();
        // The first values main.dol rand() returns from next = 1 (checked under
        // Unicorn by the client crate's tests/chao_karate.rs).
        assert_eq!(
            [r.rand(), r.rand(), r.rand(), r.rand(), r.rand()],
            [16838, 5758, 10113, 17515, 31051]
        );
    }

    #[test]
    fn a_cheer_lifts_a_discouraged_chao_on_frame_five_only() {
        let mut r = MslRand::default();
        let mut me = karate::fighter([0; 7], false, false, false, &mut r);
        me.zeal = 100.0;
        me.luck = 1000.0;
        let decay = 1000.0f32 / (1800.0f32 + 1800.0f32 * (me.intellect / 1000.0f32));
        let mut rec = karate::StateRecord {
            state: karate::state::STAGGER,
            ..Default::default()
        };
        // The press is kept, nothing is gained before frame 5.
        karate::zeal_step(&mut me, &mut rec, 0, 0, &mut r, Some(true));
        assert_eq!(me.zeal, 100.0 - decay);
        assert_ne!(rec.flags & karate::flag::CHEERED, 0);
        rec.frame = 5;
        let before = (me.zeal, r.next);
        karate::zeal_step(&mut me, &mut rec, 0, 0, &mut r, Some(false));
        // 100 + 300 * luck/1000 is 400, capped at 200.
        assert_eq!(me.zeal, before.0 - decay + 200.0);
        assert_eq!(rec.flags & karate::flag::CHEERED, 0);
        assert_ne!(r.next, before.1);
        // Outside the 2-player bout nothing is gained.
        let before = me.zeal;
        karate::zeal_step(&mut me, &mut rec, 0, 0, &mut r, None);
        assert_eq!(me.zeal, before - decay);
    }

    #[test]
    fn karate_damage_has_a_five_percent_floor() {
        let mut r = MslRand::default();
        let mut me = karate::fighter([4000, 0, 0, 0, 0, 0, 0], false, false, false, &mut r);
        let mut rec = karate::StateRecord {
            state: karate::state::HURT,
            ..Default::default()
        };
        let d = karate::take_hit(&mut me, &mut rec, 1200.0, -1, true, &mut r);
        assert_eq!(d, me.hp_max * (50.0f32 / 1000.0f32));
    }

    #[test]
    fn race_rows_and_progress_follow_the_class_layout() {
        let mut rows = vec![];
        for c in [
            race::Class::Beginner,
            race::Class::Jewel,
            race::Class::Challenge,
            race::Class::Hero,
            race::Class::Dark,
        ] {
            let (courses, levels) = c.courses();
            for course in courses {
                for l in 0..levels {
                    rows.push(c.row(course, l).unwrap());
                }
            }
        }
        assert_eq!(rows, (0..62).collect::<Vec<_>>());
        let mut p = race::Progress::default();
        p.record(0, 2);
        p.record(0, 1);
        assert_eq!(p.get(0), 2);
        assert_eq!(race::start_stamina(race::race_skill(1000)), 25000);
    }
}
