use crate::{Rng, STEP_MS, TinyError, TinyInput, distance};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hand {
    Rock,
    Paper,
    Scissors,
}
impl Hand {
    fn random(rng: &mut Rng) -> Self {
        [Self::Rock, Self::Paper, Self::Scissors][rng.index(3)]
    }
    fn beats(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Rock, Self::Scissors)
                | (Self::Paper, Self::Rock)
                | (Self::Scissors, Self::Paper)
        )
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct RpsTarget {
    pub id: u8,
    pub x: f32,
    pub y: f32,
    pub hand: Hand,
}
#[derive(Clone, Debug, Serialize)]
pub struct RpsShot {
    pub x: f32,
    pub y: f32,
    pub hand: Hand,
}
#[derive(Clone, Debug, Serialize)]
pub struct RpsView {
    pub targets: Vec<RpsTarget>,
    pub cards: [Option<Hand>; 3],
    pub selected: u8,
    pub shots: Vec<RpsShot>,
    pub replacements: u8,
    pub remaining_ms: u32,
    pub rings: u32,
    pub clears: u32,
    pub ended: bool,
}
#[derive(Clone)]
struct Shot {
    slot: usize,
    hand: Hand,
    y: f32,
}
#[derive(Clone)]
pub(crate) struct Rps {
    targets: [Option<Hand>; 10],
    cards: [Option<Hand>; 3],
    selected: usize,
    shots: Vec<Shot>,
    angle: f32,
    cooldown: u32,
    rng: Rng,
    pub rings: u32,
    pub ending: Option<(bool, &'static str)>,
    remaining_ms: u32,
    replacements: u8,
    clears: u32,
}
impl Rps {
    pub fn new(mut rng: Rng) -> Self {
        Self {
            targets: std::array::from_fn(|_| Some(Hand::random(&mut rng))),
            cards: std::array::from_fn(|_| Some(Hand::random(&mut rng))),
            selected: 1,
            shots: Vec::new(),
            angle: 0.,
            cooldown: 0,
            rng,
            rings: 0,
            ending: None,
            remaining_ms: 30000,
            replacements: 5,
            clears: 0,
        }
    }
    pub fn input(&mut self, input: TinyInput) -> Result<(), TinyError> {
        match input {
            TinyInput::RpsSelect { slot } => {
                if slot >= 3 {
                    return Err(TinyError::InvalidInput);
                }
                self.selected = usize::from(slot);
                Ok(())
            }
            TinyInput::RpsShoot => {
                if self.cooldown > 0 {
                    return Err(TinyError::Cooldown);
                }
                let hand = self.cards[self.selected]
                    .take()
                    .ok_or(TinyError::NotReady)?;
                self.shots.push(Shot {
                    slot: self.selected,
                    hand,
                    y: 94.,
                });
                self.cooldown = 200;
                Ok(())
            }
            _ => Err(TinyError::WrongGame),
        }
    }
    fn point(&self, index: usize) -> (f32, f32) {
        let a = self.angle + index as f32 * std::f32::consts::TAU / 10.;
        (50. + 24. * a.cos(), 40. + 24. * a.sin())
    }
    fn replace(&mut self, slot: usize, free: bool) {
        if free || self.replacements > 0 {
            if !free {
                self.replacements -= 1;
            }
            self.cards[slot] = Some(Hand::random(&mut self.rng));
        }
    }
    pub fn step(&mut self) {
        self.remaining_ms = self.remaining_ms.saturating_sub(STEP_MS);
        self.cooldown = self.cooldown.saturating_sub(STEP_MS);
        if self.remaining_ms == 0 {
            self.ending = Some((false, "time_up"));
            return;
        }
        let alive = self.targets.iter().flatten().count();
        self.angle += (0.6 + (10 - alive) as f32 * 0.09 + self.clears as f32 * 0.05) * 0.02;
        let shots = std::mem::take(&mut self.shots);
        for mut shot in shots {
            shot.y -= 1.6;
            // Shot displacement is shorter than the target diameter. This is
            // authoritative collision, not an accepted client target ID.
            let hit = (0..10).find(|i| {
                self.targets[*i].is_some() && distance((50., shot.y), self.point(*i)) <= 4.
            });
            if let Some(index) = hit {
                let target = self.targets[index].unwrap();
                let free = shot.hand == target || shot.hand.beats(target);
                if free {
                    self.targets[index] = None;
                    if shot.hand.beats(target) {
                        self.rings = self.rings.saturating_add(1);
                    }
                }
                self.replace(shot.slot, free);
            } else if shot.y < 0. {
                self.replace(shot.slot, false);
            } else {
                self.shots.push(shot);
            }
        }
        if self.targets.iter().all(Option::is_none) {
            self.clears += 1;
            self.remaining_ms = self.remaining_ms.saturating_add(10000);
            self.targets = std::array::from_fn(|_| Some(Hand::random(&mut self.rng)));
        }
        if self.cards.iter().all(Option::is_none) && self.shots.is_empty() {
            self.ending = Some((false, "out_of_cards"));
        }
    }
    pub fn view(&self) -> RpsView {
        RpsView {
            targets: self
                .targets
                .iter()
                .enumerate()
                .filter_map(|(i, hand)| {
                    hand.map(|hand| {
                        let (x, y) = self.point(i);
                        RpsTarget {
                            id: i as u8,
                            x,
                            y,
                            hand,
                        }
                    })
                })
                .collect(),
            cards: self.cards,
            selected: self.selected as u8,
            shots: self
                .shots
                .iter()
                .map(|s| RpsShot {
                    x: 50.,
                    y: s.y,
                    hand: s.hand,
                })
                .collect(),
            replacements: self.replacements,
            remaining_ms: self.remaining_ms,
            rings: self.rings,
            clears: self.clears,
            ended: self.ending.is_some(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn collision(hand: Hand, target: Hand) -> Rps {
        let mut game = Rps::new(Rng::new(1));
        game.targets = [None; 10];
        game.targets[0] = Some(target);
        game.targets[1] = Some(Hand::Rock);
        game.angle = std::f32::consts::FRAC_PI_2 - 0.0264;
        game.shots.push(Shot {
            slot: 0,
            hand,
            y: 65.6,
        });
        game.cards[0] = None;
        game.step();
        game
    }
    #[test]
    fn actual_shot_collisions_distinguish_win_tie_loss() {
        let win = collision(Hand::Rock, Hand::Scissors);
        assert_eq!(win.rings, 1);
        assert_eq!(win.replacements, 5);
        assert_eq!(win.targets[0], None);
        let tie = collision(Hand::Rock, Hand::Rock);
        assert_eq!(tie.rings, 0);
        assert_eq!(tie.replacements, 5);
        assert_eq!(tie.targets[0], None);
        let loss = collision(Hand::Rock, Hand::Paper);
        assert_eq!(loss.replacements, 4);
        assert_eq!(loss.targets[0], Some(Hand::Paper));
    }
    #[test]
    fn miss_uses_stock_and_eventually_exhausts_all_cards() {
        let mut game = Rps::new(Rng::new(1));
        game.replacements = 0;
        game.cards = [None; 3];
        game.shots.push(Shot {
            slot: 0,
            hand: Hand::Paper,
            y: 0.,
        });
        game.step();
        assert_eq!(game.ending, Some((false, "out_of_cards")));
    }
    #[test]
    fn cleared_targets_add_time_and_refill() {
        let mut game = Rps::new(Rng::new(1));
        game.targets = [None; 10];
        game.step();
        assert_eq!(game.remaining_ms, 39980);
        assert_eq!(game.targets.iter().flatten().count(), 10);
        assert_eq!(game.clears, 1);
    }
    #[test]
    fn shooting_is_rate_limited_and_cannot_reuse_inflight_card() {
        let mut game = Rps::new(Rng::new(1));
        game.input(TinyInput::RpsShoot).unwrap();
        assert_eq!(game.input(TinyInput::RpsShoot), Err(TinyError::Cooldown));
        for _ in 0..10 {
            game.step();
        }
        assert_eq!(game.input(TinyInput::RpsShoot), Err(TinyError::NotReady));
    }
}
