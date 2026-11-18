use crate::{Rng, STEP_MS, TinyError, TinyInput};
use serde::Serialize;

const CELLS: usize = 24;
#[derive(Clone, Debug, Serialize)]
pub struct MemoryCard {
    pub face: Option<u8>,
    pub matched: bool,
    pub occupied: bool,
    pub value: u8,
}
#[derive(Clone, Debug, Serialize)]
pub struct MemoryView {
    pub columns: u8,
    pub rows: u8,
    pub phase: &'static str,
    pub phase_remaining_ms: u32,
    pub cards: Vec<MemoryCard>,
    pub mistakes: u8,
    pub matches: u8,
    pub rings: u32,
    pub trip_available: bool,
    pub moving_card: Option<(u8, u8)>,
    pub ended: bool,
}
#[derive(Clone)]
pub(crate) struct Memory {
    board: [Option<u8>; CELLS],
    matched: [bool; CELLS],
    selected: Vec<usize>,
    age: u32,
    shuffle_ms: u32,
    mismatch_ms: u32,
    trip_ms: u32,
    tripped: bool,
    moving: Option<(usize, usize)>,
    moves: u8,
    rng: Rng,
    mistakes: u8,
    matches: u8,
    pub rings: u32,
    pub ending: Option<(bool, &'static str)>,
}
impl Memory {
    pub fn new(rng: &mut Rng) -> Self {
        let mut board = [None; CELLS];
        for (index, cell) in board.iter_mut().take(14).enumerate() {
            *cell = Some((index / 2) as u8);
        }
        rng.shuffle(&mut board);
        Self {
            board,
            matched: [false; CELLS],
            selected: Vec::new(),
            age: 0,
            shuffle_ms: 0,
            mismatch_ms: 0,
            trip_ms: 0,
            tripped: false,
            moving: None,
            moves: 0,
            rng: *rng,
            mistakes: 0,
            matches: 0,
            rings: 0,
            ending: None,
        }
    }
    fn ready(&self) -> bool {
        self.age >= 2000 && self.moves >= 5 && self.mismatch_ms == 0
    }
    pub fn input(&mut self, input: TinyInput) -> Result<(), TinyError> {
        match input {
            TinyInput::MemoryTrip => {
                if self.age < 2000 || self.moves >= 5 || self.tripped {
                    return Err(TinyError::NotReady);
                }
                self.tripped = true;
                self.trip_ms = 300;
                Ok(())
            }
            TinyInput::MemoryFlip { cell } => {
                let at = usize::from(cell);
                if at >= CELLS {
                    return Err(TinyError::InvalidInput);
                }
                if !self.ready() {
                    return Err(TinyError::NotReady);
                }
                if self.board[at].is_none() || self.matched[at] || self.selected.contains(&at) {
                    return Err(TinyError::InvalidInput);
                }
                self.selected.push(at);
                if self.selected.len() == 2 {
                    let a = self.selected[0];
                    let b = self.selected[1];
                    if self.board[a] == self.board[b] {
                        self.matched[a] = true;
                        self.matched[b] = true;
                        self.matches += 1;
                        self.rings += u32::from(value(a) + value(b));
                        self.selected.clear();
                        if self.matches == 7 {
                            self.rings += match self.mistakes {
                                0 => 60,
                                1 => 30,
                                _ => 10,
                            };
                            self.ending = Some((true, "all_pairs"));
                        }
                    } else {
                        self.mistakes += 1;
                        self.mismatch_ms = 600;
                    }
                }
                Ok(())
            }
            _ => Err(TinyError::WrongGame),
        }
    }
    pub fn step(&mut self) {
        self.age += STEP_MS;
        self.trip_ms = self.trip_ms.saturating_sub(STEP_MS);
        if self.age >= 2000 && self.moves < 5 {
            self.shuffle_ms += STEP_MS;
            if self.shuffle_ms >= 300 {
                self.shuffle_ms = 0;
                let mut choices = Vec::new();
                for i in 0..CELLS {
                    if self.board[i].is_some() {
                        for j in [
                            i.checked_sub(6),
                            i.checked_add(6).filter(|v| *v < CELLS),
                            if i % 6 > 0 { Some(i - 1) } else { None },
                            if i % 6 < 5 { Some(i + 1) } else { None },
                        ]
                        .into_iter()
                        .flatten()
                        {
                            if self.board[j].is_none() {
                                choices.push((i, j));
                            }
                        }
                    }
                }
                if !choices.is_empty() {
                    let (a, b) = choices[self.rng.index(choices.len())];
                    self.board.swap(a, b);
                    self.moving = Some((a, b));
                }
                self.moves += 1;
            }
        }
        if self.mismatch_ms > 0 {
            self.mismatch_ms = self.mismatch_ms.saturating_sub(STEP_MS);
            if self.mismatch_ms == 0 {
                self.selected.clear();
                if self.mistakes >= 3 {
                    self.ending = Some((false, "three_mistakes"));
                }
            }
        }
    }
    pub fn view(&self) -> MemoryView {
        let phase = if self.ending.is_some() {
            "ended"
        } else if self.age < 2000 {
            "peek"
        } else if self.moves < 5 {
            "shuffle"
        } else if self.mismatch_ms > 0 {
            "mismatch"
        } else {
            "choose"
        };
        MemoryView {
            columns: 6,
            rows: 4,
            phase,
            phase_remaining_ms: match phase {
                "peek" => 2000 - self.age,
                "shuffle" => u32::from(5 - self.moves) * 300 - self.shuffle_ms,
                "mismatch" => self.mismatch_ms,
                _ => 0,
            },
            cards: (0..CELLS)
                .map(|i| MemoryCard {
                    face: if self.age < 2000
                        || self.matched[i]
                        || self.selected.contains(&i)
                        || (self.trip_ms > 0 && self.moving.is_some_and(|(_, to)| to == i))
                    {
                        self.board[i]
                    } else {
                        None
                    },
                    matched: self.matched[i],
                    occupied: self.board[i].is_some(),
                    value: value(i),
                })
                .collect(),
            mistakes: self.mistakes,
            matches: self.matches,
            rings: self.rings,
            trip_available: !self.tripped && phase == "shuffle",
            // Retain the last push on the transition to choose: otherwise a
            // final card swap would happen invisibly between public views.
            moving_card: self.moving.map(|(a, b)| (a as u8, b as u8)),
            ended: self.ending.is_some(),
        }
    }
}
fn value(cell: usize) -> u8 {
    let (x, y) = (cell % 6, cell / 6);
    if x == 0 || x == 5 || y == 0 || y == 3 {
        1
    } else if x == 1 || x == 4 {
        3
    } else {
        5
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready(game: &mut Memory) {
        for _ in 0..180 {
            game.step();
        }
    }
    #[test]
    fn board_hidden_after_peek_and_one_trip_only() {
        let mut game = Memory::new(&mut Rng::new(2));
        assert_eq!(
            game.view()
                .cards
                .iter()
                .filter(|c| c.face.is_some())
                .count(),
            14
        );
        for _ in 0..115 {
            game.step();
        }
        game.input(TinyInput::MemoryTrip).unwrap();
        assert_eq!(game.input(TinyInput::MemoryTrip), Err(TinyError::NotReady));
        ready(&mut game);
        assert!(game.view().cards.iter().all(|c| c.face.is_none()));
    }
    #[test]
    fn perfect_full_round_and_all_bonus_branches() {
        for mistakes in 0..=2 {
            let mut game = Memory::new(&mut Rng::new(5));
            ready(&mut game);
            game.mistakes = mistakes;
            let mut position_points = 0;
            for face in 0..7 {
                let pair: Vec<_> = game
                    .board
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| **v == Some(face))
                    .map(|(i, _)| i)
                    .collect();
                position_points += u32::from(value(pair[0]) + value(pair[1]));
                for cell in pair {
                    game.input(TinyInput::MemoryFlip { cell: cell as u8 })
                        .unwrap();
                }
            }
            assert_eq!(
                game.rings,
                position_points
                    + match mistakes {
                        0 => 60,
                        1 => 30,
                        _ => 10,
                    }
            );
            assert_eq!(game.ending, Some((true, "all_pairs")));
        }
    }
    #[test]
    fn third_mismatch_ends_and_duplicate_flip_cannot_score() {
        let mut game = Memory::new(&mut Rng::new(4));
        ready(&mut game);
        let a = game.board.iter().position(Option::is_some).unwrap();
        let b = game
            .board
            .iter()
            .position(|v| v.is_some() && *v != game.board[a])
            .unwrap();
        for _ in 0..3 {
            game.input(TinyInput::MemoryFlip { cell: a as u8 }).unwrap();
            assert_eq!(
                game.input(TinyInput::MemoryFlip { cell: a as u8 }),
                Err(TinyError::InvalidInput)
            );
            game.input(TinyInput::MemoryFlip { cell: b as u8 }).unwrap();
            for _ in 0..30 {
                game.step();
            }
        }
        assert_eq!(game.ending, Some((false, "three_mistakes")));
        assert_eq!(game.rings, 0);
    }
}
