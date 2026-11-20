use serde::{Deserialize, Serialize};

// A2NE task 02004418, save bytes 73h (Mood) and 74h (Belly).
// These are eligible garden frames, not wall time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedsClock {
    belly: u16,
    full: u16,
    hungry: u16,
    weeds: u16,
}
// A2NE 02007E80: offering checks signed rating x10 against remaining
// capacity. Eating later applies x20; the two multipliers are distinct.
pub fn accepts_fruit(belly: u8, rating: i8) -> bool {
    i16::from(rating) * 10 <= 200 - i16::from(belly)
}
fn hungry_period(belly: u8) -> u16 {
    60 * (4 + u16::from(belly / 20))
}
fn weed_period(count: usize) -> u16 {
    60 * (12 - count.min(16) as u16 / 2)
}
fn elapsed(timer: &mut u16, period: u16) -> bool {
    *timer -= 1;
    if *timer == 0 {
        *timer = period;
        true
    } else {
        false
    }
}
impl NeedsClock {
    pub fn new(belly: u8, weeds: usize) -> Self {
        Self {
            belly: 180,
            full: 360,
            hungry: hungry_period(belly),
            weeds: weed_period(weeds),
        }
    }
    pub fn valid(&self) -> bool {
        (1..=180).contains(&self.belly)
            && (1..=360).contains(&self.full)
            && (1..=840).contains(&self.hungry)
            && (1..=720).contains(&self.weeds)
    }
    pub fn tick(&mut self, mood: &mut u8, belly: &mut u8, weeds: usize) {
        if elapsed(&mut self.belly, 180) {
            *belly = belly.saturating_sub(1);
        }
        if *belly >= 190 && elapsed(&mut self.full, 360) {
            *mood = mood.saturating_add(1).min(200);
        }
        if *belly >= 110 {
            if weeds < 4 {
                self.hungry = hungry_period(*belly);
                self.weeds = weed_period(weeds);
            }
        } else if elapsed(&mut self.hungry, hungry_period(*belly)) {
            *mood = mood.saturating_sub(1);
        }
        if weeds >= 4 && elapsed(&mut self.weeds, weed_period(weeds)) {
            *mood = mood.saturating_sub(1);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fruit_offer_uses_the_original_half_effect_threshold() {
        assert!(accepts_fruit(180, 2));
        assert!(!accepts_fruit(181, 2));
        assert!(accepts_fruit(200, -1));
    }
    #[test]
    fn fourth_weed_starts_decay_and_pulling_stops_it() {
        let (mut mood, mut belly) = (100, 160);
        let mut clock = NeedsClock::new(belly, 4);
        for _ in 0..599 {
            clock.tick(&mut mood, &mut belly, 4);
        }
        assert_eq!((mood, belly), (100, 157));
        clock.tick(&mut mood, &mut belly, 4);
        assert_eq!(mood, 99);
        for _ in 0..720 {
            clock.tick(&mut mood, &mut belly, 3);
        }
        assert_eq!(mood, 99);
    }
    #[test]
    fn hunger_and_weeds_stack_and_fullness_can_raise_mood() {
        let (mut mood, mut belly) = (1, 0);
        let mut c = NeedsClock::new(belly, 16);
        for _ in 0..240 {
            c.tick(&mut mood, &mut belly, 16);
        }
        assert_eq!((mood, belly), (0, 0));
        belly = 200;
        for _ in 0..360 {
            c.tick(&mut mood, &mut belly, 0);
        }
        assert_eq!((mood, belly), (1, 198));
    }
}
