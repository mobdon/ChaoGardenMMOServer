use crate::{Rng, STEP_MS, TinyError, TinyInput};
use serde::{Deserialize, Serialize};
const WIDTH: usize = 8;
const HEIGHT: usize = 12;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BallColor {
    Red,
    Green,
}
#[derive(Clone, Debug, Serialize)]
pub struct BallPoint {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct BallShotView {
    pub x: f32,
    pub y: f32,
    pub color: BallColor,
}
#[derive(Clone, Debug, Serialize)]
pub struct BallView {
    pub columns: u8,
    pub rows: u8,
    pub grid: Vec<Vec<Option<BallColor>>>,
    pub board_offset: f32,
    pub player_x: f32,
    pub shots: Vec<BallShotView>,
    pub embedded_rings: Vec<BallPoint>,
    pub falling_rings: Vec<BallPoint>,
    pub rings: u32,
    pub stage: u32,
    pub cleared_balls: u32,
    pub shot_cooldown_ms: u32,
    pub ended: bool,
}
#[derive(Clone)]
struct Shot {
    column: usize,
    y: f32,
    color: BallColor,
}
#[derive(Clone)]
struct EmbeddedRing {
    column: usize,
    support_row: usize,
}
#[derive(Clone)]
pub(crate) struct Ball {
    grid: [[Option<BallColor>; WIDTH]; HEIGHT],
    offset: f32,
    player_x: f32,
    move_x: i8,
    shots: Vec<Shot>,
    embedded: Vec<EmbeddedRing>,
    falling: Vec<(f32, f32)>,
    cooldown: u32,
    stage: u32,
    stage_ms: u32,
    cleared: u32,
    stage_cleared: u32,
    rng: Rng,
    pub rings: u32,
    pub ending: Option<(bool, &'static str)>,
}
impl Ball {
    pub fn new(rng: Rng) -> Self {
        let mut game = Self {
            grid: [[None; WIDTH]; HEIGHT],
            offset: 0.,
            player_x: 3.5,
            move_x: 0,
            shots: Vec::new(),
            embedded: Vec::new(),
            falling: Vec::new(),
            cooldown: 0,
            stage: 1,
            stage_ms: 0,
            cleared: 0,
            stage_cleared: 0,
            rng,
            rings: 0,
            ending: None,
        };
        game.fill_stage();
        game
    }
    fn color(&mut self) -> BallColor {
        if self.rng.index(2) == 0 {
            BallColor::Red
        } else {
            BallColor::Green
        }
    }
    fn fill_stage(&mut self) {
        self.grid = [[None; WIDTH]; HEIGHT];
        for row in 0..3 {
            for column in 0..WIDTH {
                self.grid[row][column] = Some(self.color());
            }
        }
        self.offset = 0.;
        self.stage_ms = 0;
        self.stage_cleared = 0;
        self.embedded = vec![
            EmbeddedRing {
                column: self.rng.index(WIDTH),
                support_row: 2,
            },
            EmbeddedRing {
                column: self.rng.index(WIDTH),
                support_row: 1,
            },
        ];
    }
    pub fn input(&mut self, input: TinyInput) -> Result<(), TinyError> {
        match input {
            TinyInput::Move { x, y: 0 } => {
                self.move_x = x;
                Ok(())
            }
            TinyInput::Move { .. } => Err(TinyError::InvalidInput),
            TinyInput::BallShoot { color } => {
                if self.cooldown > 0 {
                    return Err(TinyError::Cooldown);
                }
                if self.shots.len() >= 3 {
                    return Err(TinyError::NotReady);
                }
                self.shots.push(Shot {
                    column: self.player_x.round().clamp(0., 7.) as usize,
                    y: 12.8,
                    color,
                });
                self.cooldown = 240;
                Ok(())
            }
            _ => Err(TinyError::WrongGame),
        }
    }
    pub fn step(&mut self) {
        self.stage_ms += STEP_MS;
        self.cooldown = self.cooldown.saturating_sub(STEP_MS);
        self.player_x = (self.player_x + f32::from(self.move_x) * 0.12).clamp(0., 7.);
        self.offset += (0.32 + (self.stage - 1).min(30) as f32 * 0.04) * 0.02;
        if self.offset >= 1. {
            self.offset -= 1.;
            if self.grid[HEIGHT - 1].iter().any(Option::is_some) {
                self.ending = Some((false, "board_descended"));
                return;
            }
            for row in (1..HEIGHT).rev() {
                self.grid[row] = self.grid[row - 1];
            }
            for col in 0..WIDTH {
                self.grid[0][col] = Some(self.color());
            }
            for ring in &mut self.embedded {
                ring.support_row += 1;
            }
        }
        let shots = std::mem::take(&mut self.shots);
        for mut shot in shots {
            shot.y -= 0.24;
            let last = (0..HEIGHT)
                .rev()
                .find(|row| self.grid[*row][shot.column].is_some());
            let touch = last.map_or(0.6 + self.offset, |row| row as f32 + 1.2 + self.offset);
            if shot.y <= touch {
                let row = last.map_or(0, |row| row + 1);
                if row >= HEIGHT {
                    self.ending = Some((false, "board_descended"));
                    return;
                }
                self.place(row, shot.column, shot.color);
            } else {
                self.shots.push(shot);
            }
        }
        let mut falling = std::mem::take(&mut self.falling);
        for point in &mut falling {
            point.1 += 0.10;
        }
        for (x, y) in falling {
            if (12. ..=13.4).contains(&y) && (x - self.player_x).abs() < 0.65 {
                self.rings = self.rings.saturating_add(1);
            } else if y < 13.4 {
                self.falling.push((x, y));
            }
        }
        if self.stage_ms >= 45000 || self.stage_cleared >= 60 {
            self.stage = self.stage.saturating_add(1);
            self.fill_stage();
        }
    }
    fn place(&mut self, row: usize, column: usize, color: BallColor) {
        self.grid[row][column] = Some(color);
        // Orthogonal sandwich flips. Diagonal/precise retail adjacency remains
        // unverified and is explicitly an adaptation in this implementation.
        for (dr, dc) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (mut r, mut c) = (row as i32 + dr, column as i32 + dc);
            let mut trapped = Vec::new();
            while r >= 0 && r < HEIGHT as i32 && c >= 0 && c < WIDTH as i32 {
                match self.grid[r as usize][c as usize] {
                    Some(other) if other != color => trapped.push((r as usize, c as usize)),
                    Some(_) => {
                        for (r, c) in trapped {
                            self.grid[r][c] = Some(color);
                        }
                        break;
                    }
                    None => break,
                }
                r += dr;
                c += dc;
            }
        }
        let mut visited = [[false; WIDTH]; HEIGHT];
        let mut stack = vec![(row, column)];
        let mut group = Vec::new();
        while let Some((r, c)) = stack.pop() {
            if visited[r][c] || self.grid[r][c] != Some(color) {
                continue;
            }
            visited[r][c] = true;
            group.push((r, c));
            if r > 0 {
                stack.push((r - 1, c));
            }
            if r + 1 < HEIGHT {
                stack.push((r + 1, c));
            }
            if c > 0 {
                stack.push((r, c - 1));
            }
            if c + 1 < WIDTH {
                stack.push((r, c + 1));
            }
        }
        if group.len() < 10 {
            return;
        }
        self.cleared += group.len() as u32;
        self.stage_cleared += group.len() as u32;
        let old = self.grid;
        for (r, c) in &group {
            self.grid[*r][*c] = None;
        }
        let embedded = std::mem::take(&mut self.embedded);
        for mut ring in embedded {
            if group.contains(&(ring.support_row, ring.column)) {
                self.falling.push((
                    ring.column as f32,
                    ring.support_row as f32 + self.offset - 0.4,
                ));
            } else {
                ring.support_row = (0..ring.support_row)
                    .filter(|r| {
                        old[*r][ring.column].is_some() && !group.contains(&(*r, ring.column))
                    })
                    .count();
                self.embedded.push(ring);
            }
        }
        for col in 0..WIDTH {
            let kept: Vec<_> = (0..HEIGHT).filter_map(|r| self.grid[r][col]).collect();
            for (row, cells) in self.grid.iter_mut().enumerate() {
                cells[col] = kept.get(row).copied();
            }
        }
    }
    pub fn view(&self) -> BallView {
        BallView {
            columns: WIDTH as u8,
            rows: HEIGHT as u8,
            grid: self.grid.iter().map(|row| row.to_vec()).collect(),
            board_offset: self.offset,
            player_x: self.player_x,
            shots: self
                .shots
                .iter()
                .map(|shot| BallShotView {
                    x: shot.column as f32,
                    y: shot.y,
                    color: shot.color,
                })
                .collect(),
            embedded_rings: self
                .embedded
                .iter()
                .map(|ring| BallPoint {
                    x: ring.column as f32,
                    y: ring.support_row as f32 + self.offset - 0.4,
                })
                .collect(),
            falling_rings: self
                .falling
                .iter()
                .map(|p| BallPoint { x: p.0, y: p.1 })
                .collect(),
            rings: self.rings,
            stage: self.stage,
            cleared_balls: self.cleared,
            shot_cooldown_ms: self.cooldown,
            ended: self.ending.is_some(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sandwich_flips_then_ten_connected_clear_and_release_ring() {
        let mut game = Ball::new(Rng::new(1));
        game.grid = [[None; WIDTH]; HEIGHT];
        for (row, cells) in game.grid.iter_mut().take(9).enumerate() {
            cells[0] = Some(if row == 0 {
                BallColor::Red
            } else {
                BallColor::Green
            });
        }
        game.embedded = vec![EmbeddedRing {
            column: 0,
            support_row: 7,
        }];
        game.place(9, 0, BallColor::Red);
        assert_eq!(game.cleared, 10);
        assert!(game.grid.iter().all(|row| row[0].is_none()));
        assert_eq!(game.falling.len(), 1);
        assert_eq!(game.rings, 0);
        game.player_x = 0.;
        game.falling[0].1 = 11.95;
        game.step();
        assert_eq!(game.rings, 1);
        assert!(game.falling.is_empty());
    }
    #[test]
    fn nine_connected_do_not_clear_and_survivors_compact_upward() {
        let mut game = Ball::new(Rng::new(3));
        game.grid = [[None; WIDTH]; HEIGHT];
        game.embedded.clear();
        for cells in game.grid.iter_mut().take(8) {
            cells[0] = Some(BallColor::Red);
        }
        game.place(8, 0, BallColor::Red);
        assert_eq!(game.cleared, 0);
        game.grid[10][0] = Some(BallColor::Green);
        game.place(9, 0, BallColor::Red);
        assert_eq!(game.cleared, 10);
        assert_eq!(game.grid[0][0], Some(BallColor::Green));
        assert!(game.grid[1..].iter().all(|row| row[0].is_none()));
    }
    #[test]
    fn falling_rings_can_be_missed_and_board_descent_loses() {
        let mut game = Ball::new(Rng::new(2));
        game.falling.push((0., 13.35));
        game.player_x = 7.;
        game.step();
        assert_eq!(game.rings, 0);
        assert!(game.falling.is_empty());
        game.grid[11][2] = Some(BallColor::Green);
        game.offset = 0.999;
        game.step();
        assert_eq!(game.ending, Some((false, "board_descended")));
    }
    #[test]
    fn actual_projectile_lands_and_shots_are_rate_limited() {
        let mut game = Ball::new(Rng::new(1));
        game.grid = [[None; WIDTH]; HEIGHT];
        game.embedded.clear();
        game.player_x = 0.;
        game.input(TinyInput::BallShoot {
            color: BallColor::Red,
        })
        .unwrap();
        assert_eq!(
            game.input(TinyInput::BallShoot {
                color: BallColor::Green
            }),
            Err(TinyError::Cooldown)
        );
        for _ in 0..60 {
            game.step();
        }
        assert_eq!(game.grid[0][0], Some(BallColor::Red));
    }
}
