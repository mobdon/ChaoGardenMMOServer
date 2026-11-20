//! Native emblem record (GameCube GSNE8P), used by the Black Market's emblem
//! gates. fn_80057EB4 rebuilds the emblem count (save+0x0D, a u8) by counting
//! 180 non-zero bytes:
//! - 29 stages x 5 mission ranks at save + 0x38 + slot * 0xC4 + m, with the
//!   stage slot list at dol 0x801B33E8, plus Route 101 (slot 60) and Route 280
//!   (slot 61): 155;
//! - Chao Race save+0x3224..0x3228 (5) and Chao Karate +0x3231..0x3234 (4);
//! - Kart +0x2FBC / +0x2FC9 / +0x2FD6 (3);
//! - 13 special bytes at +0x348D..0x3499 (0-5 all A per character, 6-8 story
//!   clears, 12 all A on Cannon's Core; 9-11 have no traced writer).
//!
//! The server awards an emblem only when a native source is cleared. Nothing
//! in this MMO is a native stage yet, so every count starts at zero; the stage,
//! race, karate and kart ports call [`record_mission`] and [`award`].
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const EMBLEM_TOTAL: u32 = 180;
/// Mission rank byte for an A rank.
pub const RANK_A: u8 = 5;
/// dol 0x801B33E8: list index -> stage save slot (level id).
pub const STAGE_SLOTS: [(u8, &str); 29] = [
    (13, "City Escape"),
    (10, "Metal Harbor"),
    (3, "Green Forest"),
    (28, "Pyramid Cave"),
    (22, "Crazy Gadget"),
    (30, "Final Rush"),
    (9, "Prison Lane"),
    (17, "Mission Street"),
    (23, "Hidden Base"),
    (24, "Eternal Engine"),
    (16, "Wild Canyon"),
    (5, "Pumpkin Hill"),
    (7, "Aquatic Mine"),
    (25, "Death Chamber"),
    (32, "Meteor Herd"),
    (14, "Radical Highway"),
    (4, "White Jungle"),
    (6, "Sky Rail"),
    (40, "Final Chase"),
    (11, "Iron Gate"),
    (21, "Sand Ocean"),
    (27, "Lost Colony"),
    (12, "Weapons Bed"),
    (43, "Cosmic Wall"),
    (18, "Dry Lagoon"),
    (26, "Egg Quarters"),
    (8, "Security Hall"),
    (44, "Mad Space"),
    (34, "Cannon's Core"),
];
/// Route 101 and Route 280 slots.
pub const ROUTE_SLOTS: [u8; 2] = [60, 61];
const CANNONS_CORE: u8 = 34;
const SPECIAL_CANNONS_CORE: usize = 12;

fn counted_slot(slot: u8) -> bool {
    STAGE_SLOTS.iter().any(|(s, _)| *s == slot) || ROUTE_SLOTS.contains(&slot)
}

/// Emblem bytes other than stage missions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emblem {
    ChaoRace(usize),
    ChaoKarate(usize),
    Kart(usize),
    Special(usize),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmblemState {
    /// Stage save slot -> five mission rank bytes (0 = not cleared, 5 = A).
    pub ranks: BTreeMap<u8, [u8; 5]>,
    pub race: [bool; 5],
    pub karate: [bool; 4],
    pub kart: [bool; 3],
    pub special: [bool; 13],
}

impl EmblemState {
    pub fn valid(&self) -> bool {
        self.ranks
            .iter()
            .all(|(slot, ranks)| counted_slot(*slot) && ranks.iter().all(|r| *r <= RANK_A))
    }

    /// fn_80057EB4: the number of non-zero emblem bytes.
    pub fn count(&self) -> u32 {
        let missions: usize = self
            .ranks
            .values()
            .map(|r| r.iter().filter(|b| **b != 0).count())
            .sum();
        let flags = [
            &self.race[..],
            &self.karate[..],
            &self.kart[..],
            &self.special[..],
        ]
        .iter()
        .map(|f| f.iter().filter(|b| **b).count())
        .sum::<usize>();
        (missions + flags) as u32
    }

    pub fn public(&self) -> Value {
        json!({"count":self.count(),"total":EMBLEM_TOTAL,"ranks":self.ranks,"race":self.race,
            "karate":self.karate,"kart":self.kart,"special":self.special,
            "rules":"native-gc-emblem-count-v1"})
    }
}

/// Records a native stage mission rank (missions 0-4); a rank only ever rises.
/// Returns false for a slot outside the native emblem list or a bad rank.
pub fn record_mission(state: &mut EmblemState, slot: u8, mission: usize, rank: u8) -> bool {
    if !counted_slot(slot) || mission >= 5 || rank == 0 || rank > RANK_A {
        return false;
    }
    let ranks = state.ranks.entry(slot).or_default();
    ranks[mission] = ranks[mission].max(rank);
    // Special byte 12: all five Cannon's Core missions at A.
    if slot == CANNONS_CORE && ranks.iter().all(|r| *r == RANK_A) {
        state.special[SPECIAL_CANNONS_CORE] = true;
    }
    true
}

/// Sets one non-mission emblem byte. Returns false for an index out of range.
pub fn award(state: &mut EmblemState, emblem: Emblem) -> bool {
    let slot = match emblem {
        Emblem::ChaoRace(i) => state.race.get_mut(i),
        Emblem::ChaoKarate(i) => state.karate.get_mut(i),
        Emblem::Kart(i) => state.kart.get_mut(i),
        Emblem::Special(i) => state.special.get_mut(i),
    };
    slot.map(|b| *b = true).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_covers_all_180_native_bytes() {
        let mut s = EmblemState::default();
        assert_eq!(s.count(), 0);
        for (slot, _) in STAGE_SLOTS {
            for m in 0..5 {
                assert!(record_mission(&mut s, slot, m, RANK_A));
            }
        }
        for slot in ROUTE_SLOTS {
            for m in 0..5 {
                assert!(record_mission(&mut s, slot, m, 1));
            }
        }
        assert!(s.special[12], "Cannon's Core all A");
        for i in 0..5 {
            award(&mut s, Emblem::ChaoRace(i));
        }
        for i in 0..4 {
            award(&mut s, Emblem::ChaoKarate(i));
        }
        for i in 0..3 {
            award(&mut s, Emblem::Kart(i));
        }
        for i in 0..13 {
            award(&mut s, Emblem::Special(i));
        }
        assert_eq!(s.count(), EMBLEM_TOTAL);
        assert!(s.valid());
    }

    #[test]
    fn ranks_only_rise_and_unknown_sources_are_refused() {
        let mut s = EmblemState::default();
        assert!(
            !record_mission(&mut s, 1, 0, RANK_A),
            "slot 1 is not an emblem stage"
        );
        assert!(!record_mission(&mut s, 9, 5, RANK_A));
        assert!(!record_mission(&mut s, 9, 0, 6));
        assert!(record_mission(&mut s, 9, 0, 4));
        assert!(record_mission(&mut s, 9, 0, 2));
        assert_eq!(s.ranks[&9][0], 4);
        assert_eq!(s.count(), 1);
        assert!(!award(&mut s, Emblem::Kart(3)));
    }
}
