//! Care evidence is independent of inventory, appearance, lessons, and native lifecycle gates.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const EVIDENCE_VERSION: u32 = 1;
pub const ALL_SPECIES_MASK: u32 = (1 << 21) - 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvolutionEvidence {
    pub version: u32,
    pub species_history: u32,
    pub animal_uses: u64,
    pub drive_uses: u64,
}

impl Default for EvolutionEvidence {
    fn default() -> Self {
        Self {
            version: EVIDENCE_VERSION,
            species_history: 0,
            animal_uses: 0,
            drive_uses: 0,
        }
    }
}

impl EvolutionEvidence {
    /// Called only after authoritative ownership, consumption and effects validate.
    /// Lifetime history is an MMO evidence policy; native reset timing is unresolved.
    pub fn record_use(&mut self, native_id: u8) -> bool {
        match native_id {
            0..=20 => {
                self.species_history |= 1 << native_id;
                self.animal_uses = self.animal_uses.saturating_add(1);
            }
            21..=24 => self.drive_uses = self.drive_uses.saturating_add(1),
            _ => return false,
        }
        true
    }

    pub fn valid(&self) -> bool {
        self.version == EVIDENCE_VERSION && self.species_history & !ALL_SPECIES_MASK == 0
    }

    pub fn public(&self) -> Value {
        json!({"version":self.version,"speciesHistory":self.species_history,
            "speciesUsed":(self.species_history & ALL_SPECIES_MASK).count_ones(),
            "speciesTotal":21,"animalUses":self.animal_uses,"driveUses":self.drive_uses,
            "historyPolicy":"adapted-lifetime-care-evidence-v1",
            "nativeHistoryReset":"unknown",
            "nativeEvolution":{"enabled":false,"ready":false,"rules":"unknown",
                "reason":"Native lifecycle, alignment and full Chaos conditions require verification"},
            "learnedBehavior":{"enabled":false,"rules":"unknown","unlocked":[],
                "reason":"Animal motion resources do not establish learned Chao behavior"},
            "animalAppearance":{"enabled":false,"rules":"unknown",
                "reason":"Part selection, replacement and form compatibility require verification"}})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_distinct_from_drives_uses_and_native_evolution() {
        let mut evidence = EvolutionEvidence::default();
        for id in 0..25 {
            assert!(evidence.record_use(id));
        }
        assert_eq!(evidence.species_history, ALL_SPECIES_MASK);
        assert_eq!(evidence.animal_uses, 21);
        assert_eq!(evidence.drive_uses, 4);
        assert!(evidence.record_use(0));
        assert_eq!(evidence.animal_uses, 22);
        assert_eq!(evidence.public()["nativeEvolution"]["ready"], false);
        assert_eq!(evidence.public()["learnedBehavior"]["unlocked"], json!([]));
        let before = evidence.clone();
        assert!(!evidence.record_use(25));
        assert!(!evidence.record_use(255));
        assert_eq!(evidence, before);
    }
    #[test]
    fn round_trip_preserves_history_without_granting_behavior() {
        let mut evidence = EvolutionEvidence::default();
        evidence.record_use(7);
        evidence.record_use(21);
        let restored: EvolutionEvidence =
            serde_json::from_value(serde_json::to_value(&evidence).unwrap()).unwrap();
        assert_eq!(restored, evidence);
        assert!(restored.valid());
        assert_eq!(restored.public()["animalAppearance"]["enabled"], false);
        assert_eq!(restored.public()["learnedBehavior"]["enabled"], false);
        let invalid = EvolutionEvidence {
            species_history: 1 << 25,
            ..Default::default()
        };
        assert!(!invalid.valid());
    }
    #[test]
    fn counters_cannot_overflow_into_a_new_lifecycle() {
        let mut evidence = EvolutionEvidence {
            animal_uses: u64::MAX,
            drive_uses: u64::MAX,
            ..Default::default()
        };
        evidence.record_use(0);
        evidence.record_use(21);
        assert_eq!(evidence.animal_uses, u64::MAX);
        assert_eq!(evidence.drive_uses, u64::MAX);
        assert_eq!(evidence.species_history, 1);
    }
}
