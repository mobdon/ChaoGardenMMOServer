use crate::{Rng, STEP_MS, TinyError, TinyInput, distance};
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct BouncePoint {
    pub id: u8,
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct BounceView {
    pub spring_x: f32,
    pub chao_x: f32,
    pub chao_y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub rings: Vec<BouncePoint>,
    pub balloons: Vec<BouncePoint>,
    pub earned_rings: u32,
    pub level: u8,
    pub lives: u8,
    pub compression_ms: u32,
    pub boost_level: u8,
    pub waiting: bool,
    pub ended: bool,
}
#[derive(Clone)]
pub(crate) struct Bounce {
    spring_x: f32,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    move_x: i8,
    age: u32,
    wait_ms: u32,
    compression_ms: u32,
    boosted: bool,
    power: u8,
    level: u8,
    lives: u8,
    ring_points: Vec<(f32, f32)>,
    balloons: Vec<(f32, f32)>,
    phase: f32,
    pub rings: u32,
    pub ending: Option<(bool, &'static str)>,
}
impl Bounce {
    pub fn new(mut rng: Rng) -> Self {
        let mut game = Self {
            spring_x: 50.,
            x: 50.,
            y: 6.,
            vx: 12.,
            vy: 0.,
            move_x: 0,
            age: 0,
            wait_ms: 3000,
            compression_ms: 0,
            boosted: false,
            power: 0,
            level: 1,
            lives: 3,
            ring_points: Vec::new(),
            balloons: Vec::new(),
            phase: rng.index(1000) as f32 / 100.,
            rings: 0,
            ending: None,
        };
        game.make_level();
        game
    }
    fn make_level(&mut self) {
        let height = [29., 46., 63., 82.][usize::from(self.level - 1)];
        self.ring_points = (0..8)
            .map(|i| (12. + i as f32 * 10.5, height + (i % 2) as f32 * 3.))
            .collect();
        self.balloons = if self.level >= 3 {
            vec![(30., height - 16.), (72., height - 20.)]
        } else {
            Vec::new()
        };
    }
    fn launch(&mut self) {
        self.wait_ms = 0;
        self.compression_ms = 0;
        self.vy = 60. + f32::from(self.power) * 10.;
        self.y = 6.;
    }
    pub fn input(&mut self, input: TinyInput) -> Result<(), TinyError> {
        match input {
            TinyInput::Move { x, y: 0 } => {
                self.move_x = x;
                Ok(())
            }
            TinyInput::Move { .. } => Err(TinyError::InvalidInput),
            TinyInput::BounceSpring => {
                if self.wait_ms > 0 {
                    self.x = self.spring_x;
                    self.launch();
                    Ok(())
                } else if (40..=80).contains(&self.compression_ms) && !self.boosted {
                    self.boosted = true;
                    self.power = (self.power + 1).min(4);
                    Ok(())
                } else {
                    Err(TinyError::NotReady)
                }
            }
            _ => Err(TinyError::WrongGame),
        }
    }
    pub fn step(&mut self) {
        self.age += STEP_MS;
        self.phase += 0.02;
        self.spring_x = (self.spring_x + f32::from(self.move_x) * 0.8).clamp(4., 96.);
        if self.wait_ms > 0 {
            self.x = self.spring_x;
            self.wait_ms = self.wait_ms.saturating_sub(STEP_MS);
            if self.wait_ms == 0 {
                self.launch();
            }
            return;
        }
        if self.compression_ms > 0 {
            self.compression_ms = self.compression_ms.saturating_sub(STEP_MS);
            if self.compression_ms == 0 {
                self.launch();
            }
            return;
        }
        self.vy -= 1.;
        self.x += self.vx * 0.02;
        self.y += self.vy * 0.02;
        if self.x < 2. {
            self.x = 2.;
            self.vx = self.vx.abs();
        }
        if self.x > 98. {
            self.x = 98.;
            self.vx = -self.vx.abs();
        }
        for (index, balloon) in self.balloons.iter().enumerate() {
            let pos = (balloon.0 + self.phase.sin() * 5., balloon.1);
            let d = distance((self.x, self.y), pos);
            if d < 6. && d > 0.01 {
                let normal = ((self.x - pos.0) / d, (self.y - pos.1) / d);
                let dot = self.vx * normal.0 + self.vy * normal.1;
                if dot < 0. {
                    self.vx -= 2. * dot * normal.0;
                    self.vy -= 2. * dot * normal.1;
                }
                self.vx = (self.vx + if index % 2 == 0 { 4. } else { -4. }).clamp(-45., 45.);
                self.x = pos.0 + normal.0 * 6.1;
                self.y = pos.1 + normal.1 * 6.1;
            }
        }
        let before = self.ring_points.len();
        let phase = self.phase;
        let x = self.x;
        let y = self.y;
        self.ring_points
            .retain(|point| distance((x, y), (point.0 + phase.sin() * 3., point.1)) > 3.8);
        self.rings += (before - self.ring_points.len()) as u32;
        if self.ring_points.is_empty() {
            if self.level == 4 {
                self.ending = Some((true, "four_levels"));
                return;
            }
            self.level += 1;
            self.make_level();
        }
        if self.y <= 6. && self.vy < 0. {
            if (self.x - self.spring_x).abs() <= 6. {
                self.y = 6.;
                self.vy = 0.;
                self.compression_ms = 120;
                self.boosted = false;
                self.vx = (self.x - self.spring_x) * 2.5;
                if self.vx.abs() < 5. {
                    self.vx = if self.age.is_multiple_of(40) {
                        12.
                    } else {
                        -12.
                    };
                }
            } else if (self.x - (self.spring_x - 9.)).abs() <= 3. {
                self.y = 8.;
                self.vy = 40.;
            } else {
                self.lives -= 1;
                if self.lives == 0 {
                    self.ending = Some((false, "three_misses"));
                } else {
                    self.wait_ms = 1200;
                    self.y = 6.;
                    self.vy = 0.;
                    self.x = self.spring_x;
                }
            }
        }
    }
    pub fn view(&self) -> BounceView {
        BounceView {
            spring_x: self.spring_x,
            chao_x: self.x,
            chao_y: self.y,
            velocity_x: self.vx,
            velocity_y: self.vy,
            rings: self
                .ring_points
                .iter()
                .enumerate()
                .map(|(i, p)| BouncePoint {
                    id: i as u8,
                    x: p.0 + self.phase.sin() * 3.,
                    y: p.1,
                })
                .collect(),
            balloons: self
                .balloons
                .iter()
                .enumerate()
                .map(|(i, p)| BouncePoint {
                    id: i as u8,
                    x: p.0 + self.phase.sin() * 5.,
                    y: p.1,
                })
                .collect(),
            earned_rings: self.rings,
            level: self.level,
            lives: self.lives,
            compression_ms: self.compression_ms,
            boost_level: self.power,
            waiting: self.wait_ms > 0,
            ended: self.ending.is_some(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spring_timing_boost_and_missed_catch() {
        let mut game = Bounce::new(Rng::new(1));
        game.input(TinyInput::BounceSpring).unwrap();
        game.x = game.spring_x;
        game.y = 6.1;
        game.vy = -10.;
        game.step();
        assert_eq!(game.compression_ms, 120);
        assert_eq!(
            game.input(TinyInput::BounceSpring),
            Err(TinyError::NotReady)
        );
        for _ in 0..3 {
            game.step();
        }
        game.input(TinyInput::BounceSpring).unwrap();
        assert_eq!(game.power, 1);
        assert_eq!(
            game.input(TinyInput::BounceSpring),
            Err(TinyError::NotReady)
        );
        for _ in 0..3 {
            game.step();
        }
        assert_eq!(game.vy, 70.);
        for _ in 0..3 {
            game.x = 3.;
            game.spring_x = 90.;
            game.y = 5.;
            game.vy = -20.;
            game.wait_ms = 0;
            game.compression_ms = 0;
            game.step();
        }
        assert_eq!(game.ending, Some((false, "three_misses")));
    }
    #[test]
    fn all_four_levels_collect_rings_and_finish() {
        let mut game = Bounce::new(Rng::new(8));
        game.wait_ms = 0;
        for level in 1..=4 {
            assert_eq!(game.level, level);
            while game.level == level && game.ending.is_none() {
                let p = game.ring_points[0];
                game.x = p.0 + (game.phase + 0.02).sin() * 3.;
                game.y = p.1;
                game.vx = 0.;
                game.vy = 1.;
                game.step();
            }
        }
        assert_eq!(game.rings, 32);
        assert_eq!(game.ending, Some((true, "four_levels")));
    }
    #[test]
    fn balloons_reflect_incoming_motion() {
        let mut game = Bounce::new(Rng::new(1));
        game.level = 3;
        game.make_level();
        game.wait_ms = 0;
        let b = game.balloons[0];
        game.x = b.0 + (game.phase + 0.02).sin() * 5.;
        game.y = b.1 + 5.;
        game.vx = 0.;
        game.vy = -20.;
        game.step();
        assert!(game.vy > 0.);
    }
}
