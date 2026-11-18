use crate::{Rng, STEP_MS, TinyError, TinyInput, distance};
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct SearchConfig {
    rewards: [u16; 7],
}
impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            rewards: [5, 10, 15, 20, 25, 30, 35],
        }
    }
}
impl SearchConfig {
    /// The guide's claimed 105 total conflicts with seven increments of five
    /// (140). Hosts can supply a verified table later; no retail total assumed.
    pub fn with_rewards(rewards: [u16; 7]) -> Result<Self, TinyError> {
        if rewards.iter().any(|r| *r > 1000) {
            return Err(TinyError::InvalidInput);
        }
        Ok(Self { rewards })
    }
    pub fn rewards(&self) -> [u16; 7] {
        self.rewards
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct SearchChao {
    pub id: u8,
    pub x: f32,
    pub y: f32,
    pub activity: &'static str,
    pub terrain: &'static str,
    pub carried: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct SearchOrb {
    pub id: u8,
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct SearchView {
    pub player_x: f32,
    pub player_y: f32,
    pub entrance_x: f32,
    pub entrance_y: f32,
    pub chao: Vec<SearchChao>,
    pub orbs: Vec<SearchOrb>,
    pub carrying: Option<u8>,
    pub request_name: &'static str,
    pub hints: Vec<String>,
    pub found: u8,
    pub rings: u32,
    pub remaining_ms: u32,
    pub reward_schedule: [u16; 7],
    pub native_reward_total_unresolved: bool,
    pub ended: bool,
}
#[derive(Clone)]
struct Chao {
    id: u8,
    x: f32,
    y: f32,
    activity: &'static str,
    returned: bool,
}
const ORBS: [(f32, f32); 4] = [(20., 75.), (80., 75.), (20., 25.), (80., 25.)];
const NAMES: [&str; 7] = ["Pip", "Momo", "Sky", "Bubbles", "Clover", "Peach", "Nova"];
#[derive(Clone)]
pub(crate) struct Search {
    chao: Vec<Chao>,
    targets: [u8; 7],
    player: (f32, f32),
    movement: (i8, i8),
    carrying: Option<u8>,
    found: usize,
    hints: Vec<String>,
    hint_cooldowns: [u32; 4],
    interaction_ms: u32,
    config: SearchConfig,
    remaining_ms: u32,
    pub rings: u32,
    pub ending: Option<(bool, &'static str)>,
}
impl Search {
    pub fn new(mut rng: Rng, config: SearchConfig) -> Self {
        let mut cells: Vec<_> = (0..16).collect();
        rng.shuffle(&mut cells);
        let chao = (0..16)
            .map(|i| {
                let cell = cells[i];
                Chao {
                    id: i as u8,
                    x: 12. + (cell % 4) as f32 * 25. + rng.index(6) as f32,
                    y: 12. + (cell / 4) as f32 * 19. + rng.index(5) as f32,
                    activity: ["napping", "dancing", "walking", "sitting"][rng.index(4)],
                    returned: false,
                }
            })
            .collect();
        let mut ids: Vec<_> = (0..16).collect();
        rng.shuffle(&mut ids);
        Self {
            chao,
            targets: std::array::from_fn(|i| ids[i]),
            player: (50., 95.),
            movement: (0, 0),
            carrying: None,
            found: 0,
            hints: Vec::new(),
            hint_cooldowns: [0; 4],
            interaction_ms: 0,
            config,
            remaining_ms: 120000,
            rings: 0,
            ending: None,
        }
    }
    fn target(&self) -> &Chao {
        &self.chao[usize::from(self.targets[self.found.min(6)])]
    }
    pub fn input(&mut self, input: TinyInput) -> Result<(), TinyError> {
        match input {
            TinyInput::Move { x, y } => {
                self.movement = (x, y);
                Ok(())
            }
            TinyInput::SearchPickUp { chao } => {
                if self.interaction_ms > 0 {
                    return Err(TinyError::Cooldown);
                }
                if self.carrying.is_some() {
                    return Err(TinyError::NotReady);
                }
                let candidate = self
                    .chao
                    .get(usize::from(chao))
                    .ok_or(TinyError::InvalidInput)?;
                if candidate.returned {
                    return Err(TinyError::InvalidInput);
                }
                if distance(self.player, (candidate.x, candidate.y)) > 4. {
                    return Err(TinyError::TooFar);
                }
                self.carrying = Some(chao);
                self.interaction_ms = 200;
                Ok(())
            }
            TinyInput::SearchReturn => {
                if self.interaction_ms > 0 {
                    return Err(TinyError::Cooldown);
                }
                if distance(self.player, (50., 95.)) > 5. {
                    return Err(TinyError::TooFar);
                }
                let carried = self.carrying.take().ok_or(TinyError::NotReady)?;
                self.interaction_ms = 200;
                if carried == self.targets[self.found] {
                    self.chao[usize::from(carried)].returned = true;
                    self.rings += u32::from(self.config.rewards[self.found]);
                    self.found += 1;
                    self.remaining_ms = self.remaining_ms.saturating_add(10000);
                    self.hints.clear();
                    self.hint_cooldowns = [0; 4];
                    if self.found == 7 {
                        self.ending = Some((true, "seven_found"));
                    }
                } else {
                    self.remaining_ms = self.remaining_ms.saturating_sub(30000);
                    if self.remaining_ms == 0 {
                        self.ending = Some((false, "time_up"));
                    }
                }
                Ok(())
            }
            TinyInput::SearchHint { orb } => {
                let i = usize::from(orb);
                let pos = *ORBS.get(i).ok_or(TinyError::InvalidInput)?;
                if distance(self.player, pos) > 5. {
                    return Err(TinyError::TooFar);
                }
                if self.hint_cooldowns[i] > 0 {
                    return Err(TinyError::Cooldown);
                }
                let t = self.target();
                let hint = match i {
                    0 => format!(
                        "The Chao is {}{} of you.",
                        if t.y < self.player.1 {
                            "north"
                        } else {
                            "south"
                        },
                        if t.x < self.player.0 { "west" } else { "east" }
                    ),
                    1 => format!("The Chao is on {}.", terrain(t.x, t.y)),
                    2 => format!("The Chao is {}.", t.activity),
                    _ => {
                        if distance(self.player, (t.x, t.y)) < 35. {
                            "The Chao can see you from here.".into()
                        } else {
                            "The Chao is too far away to see you.".into()
                        }
                    }
                };
                if self.hints.len() == 8 {
                    self.hints.remove(0);
                }
                self.hints.push(hint);
                self.hint_cooldowns[i] = 1000;
                Ok(())
            }
            _ => Err(TinyError::WrongGame),
        }
    }
    pub fn step(&mut self) {
        self.remaining_ms = self.remaining_ms.saturating_sub(STEP_MS);
        self.interaction_ms = self.interaction_ms.saturating_sub(STEP_MS);
        for c in &mut self.hint_cooldowns {
            *c = c.saturating_sub(STEP_MS);
        }
        let (x, y) = (f32::from(self.movement.0), f32::from(self.movement.1));
        let length = (x * x + y * y).sqrt().max(1.);
        let speed = if self.carrying.is_some() { 0.38 } else { 0.50 };
        self.player.0 = (self.player.0 + x / length * speed).clamp(2., 98.);
        self.player.1 = (self.player.1 + y / length * speed).clamp(2., 98.);
        if self.remaining_ms == 0 {
            self.ending = Some((false, "time_up"));
        }
    }
    pub fn view(&self) -> SearchView {
        SearchView {
            player_x: self.player.0,
            player_y: self.player.1,
            entrance_x: 50.,
            entrance_y: 95.,
            chao: self
                .chao
                .iter()
                .filter(|chao| !chao.returned)
                .map(|chao| {
                    let carried = self.carrying == Some(chao.id);
                    SearchChao {
                        id: chao.id,
                        x: if carried { self.player.0 } else { chao.x },
                        y: if carried { self.player.1 } else { chao.y },
                        activity: chao.activity,
                        terrain: terrain(chao.x, chao.y),
                        carried,
                    }
                })
                .collect(),
            orbs: ORBS
                .iter()
                .enumerate()
                .map(|(i, p)| SearchOrb {
                    id: i as u8,
                    x: p.0,
                    y: p.1,
                })
                .collect(),
            carrying: self.carrying,
            request_name: NAMES[self.found.min(6)],
            hints: self.hints.clone(),
            found: self.found as u8,
            rings: self.rings,
            remaining_ms: self.remaining_ms,
            reward_schedule: self.config.rewards,
            native_reward_total_unresolved: true,
            ended: self.ending.is_some(),
        }
    }
}
fn terrain(x: f32, y: f32) -> &'static str {
    if (42.0..=58.0).contains(&x) || y > 85. {
        "pavement"
    } else {
        "grass"
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seven_target_pickup_return_loop_and_configurable_rewards() {
        let config = SearchConfig::with_rewards([1, 2, 3, 4, 5, 6, 7]).unwrap();
        let mut game = Search::new(Rng::new(7), config);
        for i in 0..7 {
            let target = game.targets[i];
            game.player = (
                game.chao[usize::from(target)].x,
                game.chao[usize::from(target)].y,
            );
            game.interaction_ms = 0;
            game.input(TinyInput::SearchPickUp { chao: target })
                .unwrap();
            game.player = (50., 95.);
            for _ in 0..10 {
                game.step();
            }
            game.input(TinyInput::SearchReturn).unwrap();
            assert_eq!(game.found, i + 1);
        }
        assert_eq!(game.rings, 28);
        assert_eq!(game.ending, Some((true, "seven_found")));
        assert_eq!(game.remaining_ms, 188600);
        assert_eq!(SearchConfig::default().rewards.iter().sum::<u16>(), 140);
    }
    #[test]
    fn remote_pickups_and_returns_rejected_wrong_return_costs_thirty_seconds() {
        let mut game = Search::new(Rng::new(3), SearchConfig::default());
        assert_eq!(
            game.input(TinyInput::SearchPickUp { chao: 0 }),
            Err(TinyError::TooFar)
        );
        let wrong = game
            .chao
            .iter()
            .find(|c| c.id != game.targets[0])
            .unwrap()
            .id;
        game.player = (
            game.chao[usize::from(wrong)].x,
            game.chao[usize::from(wrong)].y,
        );
        game.input(TinyInput::SearchPickUp { chao: wrong }).unwrap();
        for _ in 0..10 {
            game.step();
        }
        assert_eq!(game.input(TinyInput::SearchReturn), Err(TinyError::TooFar));
        game.player = (50., 95.);
        game.input(TinyInput::SearchReturn).unwrap();
        assert_eq!(game.remaining_ms, 89800);
        assert_eq!(game.rings, 0);
    }
    #[test]
    fn hints_require_proximity_and_public_view_never_contains_target_ids() {
        let mut game = Search::new(Rng::new(1), SearchConfig::default());
        assert_eq!(
            game.input(TinyInput::SearchHint { orb: 0 }),
            Err(TinyError::TooFar)
        );
        game.player = ORBS[0];
        game.input(TinyInput::SearchHint { orb: 0 }).unwrap();
        assert!(!game.hints.is_empty());
        let view = serde_json::to_value(game.view()).unwrap();
        assert!(view.get("targets").is_none());
        assert!(view.get("target_id").is_none());
        assert!(view.get("seed").is_none());
        assert!(view.get("rng").is_none());
        assert_eq!(
            game.input(TinyInput::SearchHint { orb: 0 }),
            Err(TinyError::Cooldown)
        );
    }
}
