//! Exact A2NE Memory board, staged reveal, Chao traversal and matching rules.
//!
//! This is a source-backed gameplay slice, not the complete original minigame:
//! instruction/pause presentation and native sound/rendering remain separate.
//! No hidden board is serialized by this module.

use crate::{native_growth::NativeRng, shop};
use std::fmt;

pub const COLUMNS: usize = 6;
pub const ROWS: usize = 5;
pub const CELLS: usize = COLUMNS * ROWS;
pub const CARD_COUNT: usize = 14;
pub const PAIR_COUNT: u8 = 7;
pub const EMPTY: i8 = -1;
pub const BOARD_INITIALIZE_ADDRESS: u32 = 0x0200_a554;
pub const MATCH_PHASE_ADDRESS: u32 = 0x0200_b6a4;
pub const NATIVE_FIELDS_BASE: u32 = 0x0300_1970;
pub const NATIVE_FIELDS_LEN: usize = 0x89;
pub const MAX_PLACEMENT_RANDOM_CALLS: u16 = 4096;
pub const FLIP_FRAMES: i8 = 30;
pub const MISMATCH_FRAMES: u32 = 84;
pub const RING_CAP: u8 = 99;
/// Fourteen cards can occupy at most two5-ring, ten3-ring and two1-ring cells.
pub const MAX_MATCH_RINGS: u8 = 42;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Source(shop::Error),
    RandomBudgetExceeded,
    InvalidBoard,
    InvalidState,
    InvalidCursor,
    TruncatedFields,
    UnsupportedPause,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// An exact version gate for this translated code; contains no original art.
#[derive(Clone, Copy, Debug)]
pub struct A2neMemorySource(());

impl A2neMemorySource {
    pub fn from_verified_payload(payload: &[u8]) -> Result<Self, Error> {
        shop::TinyShopCatalog::from_verified_payload(payload).map_err(Error::Source)?;
        Ok(Self(()))
    }

    /// Native sub_0200A554. Rejected occupied positions consume BOTH draws.
    /// Rolls use the high eight bits of the native 15-bit sample before Mod.
    pub fn create_board(self, rng: &mut NativeRng) -> Result<NativeMemoryBoard, Error> {
        create_board_with_limit(rng, MAX_PLACEMENT_RANDOM_CALLS)
    }

    pub fn begin_round(self, seed: u32) -> Result<NativeMemoryRound, Error> {
        let mut rng = NativeRng::new(seed);
        let board = self.create_board(&mut rng)?;
        Ok(NativeMemoryRound {
            rng,
            stage: RoundStage::Reveal {
                board,
                reveal: NativeMemoryReveal::default(),
            },
            shuffle_flip: [0; CARD_COUNT],
            finished: None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeMemoryPhase {
    Reveal,
    Shuffle,
    Matching,
    Result,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleMemoryCell {
    pub occupied: bool,
    pub face: Option<u8>,
    pub ring_value: u8,
    pub flip: i8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleMemorySelection {
    pub column: u8,
    pub row: u8,
    pub face: Option<u8>,
    pub flip: i8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleMemoryWalker {
    pub x: i16,
    pub y: i16,
    pub direction: u32,
    pub trip: u8,
    pub moving_position: Option<(i16, i16)>,
}

/// Deliberately contains no native card IDs, future choices, seed or RNG state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMemoryView {
    pub phase: NativeMemoryPhase,
    pub columns: u8,
    pub rows: u8,
    pub cells: [VisibleMemoryCell; CELLS],
    pub cursor: Option<(u8, u8)>,
    pub selected: [Option<VisibleMemorySelection>; 2],
    pub walker: Option<VisibleMemoryWalker>,
    pub mistakes: u8,
    pub matched_pairs: u8,
    pub rings: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeMemoryFinished {
    pub won: bool,
    pub rings: u8,
}

#[derive(Clone, Debug)]
enum RoundStage {
    Reveal {
        board: NativeMemoryBoard,
        reveal: NativeMemoryReveal,
    },
    StartShuffle {
        board: NativeMemoryBoard,
    },
    Shuffle(NativeMemoryShuffle),
    Matching(NativeMemoryMatching),
    Result {
        board: NativeMemoryBoard,
        result: NativeMemoryResult,
        matched_pairs: u8,
    },
}

/// Complete frame-based gameplay after the native instruction screen. A trusted
/// server advances this core; there is no client time/score or durable-save API.
#[derive(Clone)]
pub struct NativeMemoryRound {
    rng: NativeRng,
    stage: RoundStage,
    shuffle_flip: [i8; CARD_COUNT],
    finished: Option<NativeMemoryFinished>,
}

impl fmt::Debug for NativeMemoryRound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeMemoryRound")
            .field("phase", &self.view().phase)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl NativeMemoryRound {
    pub fn finished(&self) -> Option<NativeMemoryFinished> {
        self.finished
    }

    /// One native video frame. Start/pause remains explicitly unsupported
    /// during gameplay; A/Start may dismiss the original result screen.
    pub fn step(&mut self, buttons: NativeButtons) -> Result<(), Error> {
        if self.finished.is_some() {
            return Ok(());
        }
        let mut next = self.clone();
        next.step_inner(buttons)?;
        *self = next;
        Ok(())
    }

    fn step_inner(&mut self, buttons: NativeButtons) -> Result<(), Error> {
        match &mut self.stage {
            RoundStage::Reveal { board, reveal } => {
                if buttons.pressed & 8 != 0 {
                    return Err(Error::UnsupportedPause);
                }
                if reveal.step()? {
                    self.stage = RoundStage::StartShuffle {
                        board: board.clone(),
                    };
                }
            }
            RoundStage::StartShuffle { board } => {
                if buttons.pressed & 8 != 0 {
                    return Err(Error::UnsupportedPause);
                }
                let mut shuffle = NativeMemoryShuffle::begin(board.clone(), &mut self.rng);
                shuffle.step(buttons, &mut self.rng)?;
                self.stage = RoundStage::Shuffle(shuffle);
            }
            RoundStage::Shuffle(shuffle) => {
                if shuffle.trip == 1 && shuffle.moving_card >= 0 {
                    let flip = match shuffle.timer {
                        10..=39 => Some((shuffle.timer - 10) as i8),
                        150..=179 => Some((179 - shuffle.timer) as i8),
                        _ => None,
                    };
                    if let Some(flip) = flip {
                        self.shuffle_flip[shuffle.moving_card as usize] = flip;
                    }
                }
                shuffle.step(buttons, &mut self.rng)?;
                if shuffle.finished {
                    let mut matching =
                        NativeMemoryMatching::begin_after_native_shuffle(shuffle.board.clone());
                    matching.timer = shuffle.timer;
                    self.stage = RoundStage::Matching(matching);
                }
            }
            RoundStage::Matching(game) => {
                game.step(buttons)?;
                if let Some(won) = game.ending {
                    let mut result = NativeMemoryResult::begin(won, game.mistakes, game.rings)?;
                    result.timer = game.timer;
                    self.stage = RoundStage::Result {
                        board: game.board.clone(),
                        result,
                        matched_pairs: game.matched_pairs,
                    };
                }
            }
            RoundStage::Result { result, .. } => {
                result.step(buttons)?;
                if result.done {
                    self.finished = Some(NativeMemoryFinished {
                        won: result.won,
                        rings: result.rings,
                    });
                }
            }
        }
        Ok(())
    }

    pub fn view(&self) -> NativeMemoryView {
        let (phase, board) = match &self.stage {
            RoundStage::Reveal { board, .. } => (NativeMemoryPhase::Reveal, board),
            RoundStage::StartShuffle { board } => (NativeMemoryPhase::Shuffle, board),
            RoundStage::Shuffle(game) => (NativeMemoryPhase::Shuffle, &game.board),
            RoundStage::Matching(game) => (NativeMemoryPhase::Matching, &game.board),
            RoundStage::Result { board, .. } => (
                if self.finished.is_some() {
                    NativeMemoryPhase::Ended
                } else {
                    NativeMemoryPhase::Result
                },
                board,
            ),
        };
        let mut view = NativeMemoryView {
            phase,
            columns: COLUMNS as u8,
            rows: ROWS as u8,
            cells: std::array::from_fn(|cell| VisibleMemoryCell {
                occupied: board.cells[cell] >= 0,
                face: None,
                ring_value: cell_ring_value((cell % COLUMNS) as u8, (cell / COLUMNS) as u8)
                    .expect("fixed native cell"),
                flip: 0,
            }),
            cursor: None,
            selected: [None; 2],
            walker: None,
            mistakes: 0,
            matched_pairs: 0,
            rings: 0,
        };
        match &self.stage {
            RoundStage::Reveal { reveal, .. } => {
                for (index, cell) in view.cells.iter_mut().enumerate() {
                    let group = match cell.ring_value {
                        5 => 1,
                        3 => 3,
                        _ => 5,
                    };
                    cell.flip = if reveal.stage > group {
                        0
                    } else if reveal.stage == group && reveal.timer > 0 {
                        (30 - reveal.timer) as i8
                    } else {
                        29
                    };
                    if cell.occupied && reveal.visible[index] {
                        cell.face = Some(board.cells[index] as u8 >> 1);
                    }
                }
            }
            RoundStage::Shuffle(game) => {
                view.walker = Some(VisibleMemoryWalker {
                    x: game.x,
                    y: game.y,
                    direction: game.direction,
                    trip: game.trip,
                    moving_position: if game.moving_card >= 0 {
                        Some((game.moving_x, game.moving_y))
                    } else {
                        None
                    },
                });
                for (index, cell) in view.cells.iter_mut().enumerate() {
                    if cell.occupied {
                        cell.flip = self.shuffle_flip[board.cells[index] as usize];
                        if cell.flip > 14 {
                            cell.face = Some(board.cells[index] as u8 >> 1);
                        }
                    }
                }
            }
            RoundStage::Matching(game) => {
                view.cursor = Some((game.cursor_column, game.cursor_row));
                view.mistakes = game.mistakes;
                view.matched_pairs = game.matched_pairs;
                view.rings = game.rings;
                let count = if game.state == 0 {
                    0
                } else if game.state == 1 {
                    1
                } else {
                    2
                };
                for (index, selection) in view.selected.iter_mut().enumerate().take(count) {
                    let selected = game.selected[index];
                    let flip = if game.state == 4 {
                        selected.flip
                    } else {
                        (selected.flip - 1).max(0)
                    };
                    let face = if flip > 14 {
                        Some(selected.id as u8 >> 1)
                    } else {
                        None
                    };
                    *selection = Some(VisibleMemorySelection {
                        column: selected.column as u8,
                        row: selected.row as u8,
                        face,
                        flip,
                    });
                    let position = selected.row as usize * COLUMNS + selected.column as usize;
                    if view.cells[position].occupied {
                        view.cells[position].face = face;
                        view.cells[position].flip = flip;
                    }
                }
            }
            RoundStage::Result {
                result,
                matched_pairs,
                ..
            } => {
                view.mistakes = result.mistakes;
                view.matched_pairs = *matched_pairs;
                view.rings = result.rings;
                if !result.won {
                    let flip = if result.timer <= 28 {
                        result.timer
                    } else {
                        58 - result.timer
                    } as i8;
                    for (index, cell) in view.cells.iter_mut().enumerate() {
                        cell.flip = flip;
                        if cell.occupied && flip > 14 {
                            cell.face = Some(board.cells[index] as u8 >> 1);
                        }
                    }
                }
            }
            RoundStage::StartShuffle { .. } => {}
        }
        view
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMemoryBoard {
    cells: [i8; CELLS],
    pub placement_random_calls: u16,
}

impl NativeMemoryBoard {
    /// Trusted-core inspection only. These distinct native card IDs reveal
    /// every pair; a server must construct a sanitized public view separately.
    pub fn native_cells(&self) -> &[i8; CELLS] {
        &self.cells
    }

    pub fn from_native_cells(cells: [i8; CELLS]) -> Result<Self, Error> {
        let mut seen = [false; CARD_COUNT];
        for &card in &cells {
            if card == EMPTY {
                continue;
            }
            let index = usize::try_from(card).map_err(|_| Error::InvalidBoard)?;
            let present = seen.get_mut(index).ok_or(Error::InvalidBoard)?;
            if *present {
                return Err(Error::InvalidBoard);
            }
            *present = true;
        }
        Ok(Self {
            cells,
            placement_random_calls: 0,
        })
    }
}

fn create_board_with_limit(rng: &mut NativeRng, limit: u16) -> Result<NativeMemoryBoard, Error> {
    let mut candidate_rng = *rng;
    let mut board = NativeMemoryBoard {
        cells: [EMPTY; CELLS],
        placement_random_calls: 0,
    };
    for card in 0..CARD_COUNT {
        loop {
            if board.placement_random_calls + 2 > limit {
                return Err(Error::RandomBudgetExceeded);
            }
            let row = usize::from(candidate_rng.next_u15() >> 7) % ROWS;
            let column = usize::from(candidate_rng.next_u15() >> 7) % COLUMNS;
            board.placement_random_calls += 2;
            let slot = &mut board.cells[row * COLUMNS + column];
            if *slot == EMPTY {
                *slot = card as i8;
                break;
            }
        }
    }
    *rng = candidate_rng;
    Ok(board)
}

pub fn cell_ring_value(column: u8, row: u8) -> Result<u8, Error> {
    if usize::from(column) >= COLUMNS || usize::from(row) >= ROWS {
        return Err(Error::InvalidCursor);
    }
    Ok(if (1..=4).contains(&column) && (1..=3).contains(&row) {
        if (2..=3).contains(&column) && row == 2 {
            5
        } else {
            3
        }
    } else {
        1
    })
}

/// Source-backed staged clear additions: no mistakes30+20+10, one20+10,
/// two10. Native scoring caps99 after EACH addition; host credit is separate.
pub fn rings_after_clear_bonus(mut rings: u8, mistakes: u8) -> Result<u8, Error> {
    if rings > MAX_MATCH_RINGS || mistakes >= 3 {
        return Err(Error::InvalidState);
    }
    for &addition in &[30, 20, 10][usize::from(mistakes)..] {
        rings = (rings + addition).min(RING_CAP);
    }
    Ok(rings)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeButtons {
    /// Native gInput+4 edge/repeat mask. A1, B2, Start8, R16,L32,U64,D128.
    pub pressed: u16,
    /// Native gInput held mask. Used by the cursor presentation outside scope.
    pub held: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectedCard {
    pub id: i8,
    pub flip: i8,
    pub column: i8,
    pub row: i8,
    pub ring_value: i8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMemoryMatching {
    pub board: NativeMemoryBoard,
    pub cursor_column: u8,
    pub cursor_row: u8,
    /// Exact native substate0..8 at030019DC, not a new wire protocol.
    pub state: u8,
    pub timer: u32,
    pub selected: [SelectedCard; 2],
    pub mistakes: u8,
    pub matched_pairs: u8,
    pub rings: u8,
    /// Win/loss transition only. Native result UI/bonus and durable host credit
    /// remain separate; a win here does not acknowledge a saved reward.
    pub ending: Option<bool>,
}

impl NativeMemoryMatching {
    pub fn begin_after_native_shuffle(board: NativeMemoryBoard) -> Self {
        Self {
            board,
            cursor_column: 2,
            cursor_row: 2,
            state: 0,
            timer: 0,
            selected: [SelectedCard::default(); 2],
            mistakes: 0,
            matched_pairs: 0,
            rings: 0,
            ending: None,
        }
    }

    fn validate(&self) -> Result<(), Error> {
        NativeMemoryBoard::from_native_cells(self.board.cells)?;
        if usize::from(self.cursor_column) >= COLUMNS || usize::from(self.cursor_row) >= ROWS {
            return Err(Error::InvalidCursor);
        }
        if self.state > 8
            || self.mistakes > 3
            || self.matched_pairs > PAIR_COUNT
            || self.rings > RING_CAP
            || self.timer > 1_000_000
        {
            return Err(Error::InvalidState);
        }
        for card in self.selected {
            if !(0..CARD_COUNT as i8).contains(&card.id)
                || !(-1..=FLIP_FRAMES).contains(&card.flip)
                || !(0..COLUMNS as i8).contains(&card.column)
                || !(0..ROWS as i8).contains(&card.row)
                || ![0, 1, 3, 5].contains(&card.ring_value)
            {
                return Err(Error::InvalidState);
            }
        }
        Ok(())
    }

    /// One ORIGINAL video-frame matching update. No floating point or client
    /// wall clock. Invalid imported state fails before any mutation.
    pub fn step(&mut self, buttons: NativeButtons) -> Result<(), Error> {
        self.validate()?;
        if buttons.pressed & 8 != 0 {
            return Err(Error::UnsupportedPause);
        }
        let mut candidate = self.clone();
        candidate.step_unchecked(buttons)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    fn step_unchecked(&mut self, buttons: NativeButtons) -> Result<(), Error> {
        if self.ending.is_some() {
            return Ok(());
        }
        match self.state {
            0 | 1 => {
                // Preserve original order when more than one direction arrives.
                if buttons.pressed & 64 != 0 {
                    self.cursor_row = (self.cursor_row + ROWS as u8 - 1) % ROWS as u8;
                }
                if buttons.pressed & 128 != 0 {
                    self.cursor_row = (self.cursor_row + 1) % ROWS as u8;
                }
                if buttons.pressed & 32 != 0 {
                    self.cursor_column = (self.cursor_column + COLUMNS as u8 - 1) % COLUMNS as u8;
                }
                if buttons.pressed & 16 != 0 {
                    self.cursor_column = (self.cursor_column + 1) % COLUMNS as u8;
                }
                if buttons.pressed & 1 != 0 {
                    let id = self.board.cells
                        [usize::from(self.cursor_row) * COLUMNS + usize::from(self.cursor_column)];
                    if id >= 0 && (self.state == 0 || id != self.selected[0].id) {
                        self.selected[usize::from(self.state)] = SelectedCard {
                            id,
                            flip: 0,
                            column: self.cursor_column as i8,
                            row: self.cursor_row as i8,
                            ring_value: self.selected[usize::from(self.state)].ring_value,
                        };
                        self.state += 1;
                    }
                }
            }
            2 => {
                if self.selected[1].flip == FLIP_FRAMES {
                    self.state = if self.selected[0].id >> 1 == self.selected[1].id >> 1 {
                        5
                    } else {
                        3
                    };
                    self.timer = 0;
                }
            }
            3 => {
                self.timer += 1;
                if self.timer == MISMATCH_FRAMES {
                    self.mistakes += 1;
                    self.state = 4;
                }
            }
            4 => {
                if self.selected[0].flip == 0 {
                    if self.mistakes == 3 {
                        self.ending = Some(false);
                        self.timer = 0;
                    } else {
                        self.state = 0;
                    }
                }
            }
            5 => {
                if self.timer == 0 {
                    for card in &mut self.selected {
                        card.ring_value = cell_ring_value(card.column as u8, card.row as u8)? as i8;
                        self.board.cells[card.row as usize * COLUMNS + card.column as usize] =
                            EMPTY;
                    }
                }
                self.timer += 1;
                if self.timer == 17 {
                    self.timer = 0;
                    self.state = 6;
                }
            }
            6 => {
                self.timer += 1;
                if self.timer == 21 {
                    self.state = 7;
                }
            }
            7 => {
                self.rings = (self.rings
                    + self.selected[0].ring_value as u8
                    + self.selected[1].ring_value as u8)
                    .min(RING_CAP);
                self.state = 8;
            }
            8 => {
                self.matched_pairs += 1;
                if self.matched_pairs == PAIR_COUNT {
                    self.ending = Some(true);
                } else {
                    self.state = 0;
                }
            }
            _ => unreachable!("validated native matching substate"),
        }
        match self.state {
            2 => {
                if self.selected[1].flip < FLIP_FRAMES {
                    self.selected[1].flip += 1;
                }
                if self.selected[0].flip < FLIP_FRAMES {
                    self.selected[0].flip += 1;
                }
            }
            1 => {
                if self.selected[0].flip < FLIP_FRAMES {
                    self.selected[0].flip += 1;
                }
            }
            4 => {
                for card in &mut self.selected {
                    card.flip -= 1;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Bounded native RAM mapping for independent source tools/parity tests.
    /// The caller supplies the region beginning03001970; no save/identity role.
    pub fn read_native_fields(raw: &[u8]) -> Result<Self, Error> {
        if raw.len() < NATIVE_FIELDS_LEN {
            return Err(Error::TruncatedFields);
        }
        let main_state = u32::from_le_bytes(raw[0x68..0x6c].try_into().unwrap());
        let native_state = u32::from_le_bytes(raw[0x6c..0x70].try_into().unwrap());
        let ending = match main_state {
            6 => None,
            7 => Some(true),
            8 => Some(false),
            _ => return Err(Error::InvalidState),
        };
        let result = Self {
            board: NativeMemoryBoard::from_native_cells(std::array::from_fn(|index| {
                raw[0x20 + index] as i8
            }))?,
            cursor_column: raw[0x3e],
            cursor_row: raw[0x3f],
            state: u8::try_from(native_state).map_err(|_| Error::InvalidState)?,
            timer: u32::from_le_bytes(raw[0x74..0x78].try_into().unwrap()),
            selected: std::array::from_fn(|index| {
                let offset = 0x78 + index * 8;
                SelectedCard {
                    id: raw[offset] as i8,
                    flip: raw[offset + 1] as i8,
                    column: raw[offset + 2] as i8,
                    row: raw[offset + 3] as i8,
                    ring_value: raw[offset + 4] as i8,
                }
            }),
            mistakes: raw[0x46],
            matched_pairs: raw[0x88],
            rings: raw[0],
            ending,
        };
        result.validate()?;
        Ok(result)
    }

    /// Preserves all opaque RAM bytes. Does not serialize a sanitized view.
    pub fn write_native_fields(&self, raw: &mut [u8]) -> Result<(), Error> {
        if raw.len() < NATIVE_FIELDS_LEN {
            return Err(Error::TruncatedFields);
        }
        self.validate()?;
        for (index, &card) in self.board.cells.iter().enumerate() {
            raw[0x20 + index] = card as u8;
        }
        raw[0x3e] = self.cursor_column;
        raw[0x3f] = self.cursor_row;
        raw[0x46] = self.mistakes;
        raw[0x88] = self.matched_pairs;
        raw[0] = self.rings;
        let main_state: u32 = match self.ending {
            None => 6,
            Some(true) => 7,
            Some(false) => 8,
        };
        raw[0x68..0x6c].copy_from_slice(&main_state.to_le_bytes());
        if self.ending.is_some() {
            raw[0x70..0x74].fill(0);
        }
        raw[0x6c..0x70].copy_from_slice(&u32::from(self.state).to_le_bytes());
        raw[0x74..0x78].copy_from_slice(&self.timer.to_le_bytes());
        for (index, card) in self.selected.iter().enumerate() {
            let offset = 0x78 + index * 8;
            raw[offset..offset + 5].copy_from_slice(&[
                card.id as u8,
                card.flip as u8,
                card.column as u8,
                card.row as u8,
                card.ring_value as u8,
            ]);
        }
        Ok(())
    }
}

/// Native gameplay reveal after instructions: three60-frame waits alternating
/// with30-frame flips of the two center, ten middle and eighteen outer cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMemoryReveal {
    pub stage: u8,
    pub timer: u32,
    pub visible: [bool; CELLS],
}

/// Original win/loss, staged clear additions, result wait and fade counters.
/// Graphics/audio remain adapters. The native wallet write is represented by
/// `done`; an MMO host still owns its separate durable once-only transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMemoryResult {
    pub won: bool,
    pub mistakes: u8,
    pub rings: u8,
    pub main_state: u32,
    pub stage: u32,
    pub timer: u32,
    pub ui_kind: u8,
    pub ui_stage: u8,
    pub ui_timer: i16,
    pub done: bool,
}

impl NativeMemoryResult {
    pub fn begin(won: bool, mistakes: u8, rings: u8) -> Result<Self, Error> {
        if rings > MAX_MATCH_RINGS || mistakes > 3 || (won && mistakes >= 3) {
            return Err(Error::InvalidState);
        }
        Ok(Self {
            won,
            mistakes,
            rings,
            main_state: if won { 7 } else { 8 },
            stage: 0,
            timer: 0,
            ui_kind: 1,
            ui_stage: 0,
            ui_timer: 17,
            done: false,
        })
    }

    fn update_ui(&mut self, buttons: NativeButtons) -> bool {
        match self.ui_kind {
            0 => return true,
            1 | 5 => {
                let complete = self.ui_timer == 16;
                self.ui_timer += 1;
                return complete;
            }
            4 => {
                if buttons.pressed & 9 != 0 {
                    self.ui_kind = 5;
                    self.ui_timer = 0;
                } else {
                    self.ui_timer += 1;
                    if self.ui_timer == 180 {
                        self.ui_kind = 5;
                        self.ui_timer = 0;
                    }
                }
            }
            2 | 3 => match self.ui_stage {
                0 => {
                    self.ui_stage = 1;
                    self.ui_timer = 0;
                }
                1 => {
                    let t = i32::from(self.ui_timer);
                    self.ui_timer += 1;
                    let finished = if self.ui_kind == 2 {
                        280 - t * t / 4 <= 183
                    } else {
                        96 + t * t / 4 > 184
                    };
                    if finished {
                        self.ui_stage = 2;
                        self.ui_timer = 0;
                    }
                }
                2 => {
                    self.ui_timer += 1;
                    if self.ui_timer == 8 {
                        self.ui_stage = 3;
                        self.ui_timer = 0;
                    }
                }
                3 => {
                    self.ui_timer += 1;
                    if self.ui_timer == 16 {
                        self.ui_stage = 4;
                    }
                }
                4 => {
                    self.ui_timer -= 1;
                    if self.ui_timer < 0 {
                        if self.ui_kind == 3 {
                            return true;
                        }
                        self.ui_kind = 4;
                        self.ui_timer = 0;
                    }
                }
                _ => unreachable!("validated result UI stage"),
            },
            _ => unreachable!("validated result UI kind"),
        }
        false
    }

    fn validate(&self) -> Result<(), Error> {
        if self.rings > RING_CAP
            || self.mistakes > 3
            || self.main_state > 9
            || self.stage > 5
            || self.ui_kind > 5
            || self.ui_stage > 4
            || !(-1..=180).contains(&self.ui_timer)
            || self.timer > 1_000_000
        {
            return Err(Error::InvalidState);
        }
        Ok(())
    }

    pub fn step(&mut self, buttons: NativeButtons) -> Result<(), Error> {
        self.validate()?;
        let mut candidate = self.clone();
        candidate.step_unchecked(buttons)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    fn step_unchecked(&mut self, buttons: NativeButtons) -> Result<(), Error> {
        if self.done {
            return Ok(());
        }
        match self.main_state {
            7 => {
                if self.stage == 0 {
                    self.stage = 1;
                    self.ui_kind = 3;
                    self.ui_stage = 0;
                }
                if self.stage == 1 {
                    if self.update_ui(buttons) {
                        self.stage = u32::from(self.mistakes) + 2;
                        self.timer = 0;
                    }
                } else if (2..=4).contains(&self.stage) {
                    let previous = self.timer;
                    self.timer += 1;
                    if previous == 0 {
                        let addition = match self.stage {
                            2 => 30,
                            3 => 20,
                            _ => 10,
                        };
                        self.rings = (self.rings + addition).min(RING_CAP);
                    }
                    if previous == 40 {
                        self.timer = 0;
                        self.stage += 1;
                    }
                } else if self.stage == 5 {
                    self.ui_kind = 4;
                    self.ui_timer = 0;
                    self.main_state = 9;
                }
            }
            8 => {
                if self.stage == 0 {
                    self.stage = 1;
                    self.ui_kind = 2;
                    self.ui_stage = 0;
                }
                self.timer += 1;
                if self.timer == 58 {
                    self.timer = 0;
                }
                if self.update_ui(buttons) {
                    self.done = true;
                    self.main_state = 0;
                }
            }
            9 => {
                if self.update_ui(buttons) {
                    self.done = true;
                    self.main_state = 0;
                }
            }
            _ => return Err(Error::InvalidState),
        }
        Ok(())
    }
}

impl Default for NativeMemoryReveal {
    fn default() -> Self {
        Self {
            stage: 0,
            timer: 0,
            visible: [true; CELLS],
        }
    }
}

impl NativeMemoryReveal {
    pub fn step(&mut self) -> Result<bool, Error> {
        if self.stage > 6
            || self.timer >= 60
            || (matches!(self.stage, 1 | 3 | 5) && self.timer >= 30)
        {
            return Err(Error::InvalidState);
        }
        if self.stage == 6 {
            return Ok(true);
        }
        if matches!(self.stage, 0 | 2 | 4) {
            self.timer += 1;
            if self.timer == 60 {
                self.timer = 0;
                self.stage += 1;
            }
        } else {
            let value = match self.stage {
                1 => 5,
                3 => 3,
                5 => 1,
                _ => unreachable!(),
            };
            for row in 0..ROWS {
                for col in 0..COLUMNS {
                    if cell_ring_value(col as u8, row as u8)? == value {
                        self.visible[row * COLUMNS + col] = 29 - self.timer > 14;
                    }
                }
            }
            self.timer += 1;
            if self.timer == 30 {
                self.timer = 0;
                self.stage += 1;
            }
        }
        Ok(self.stage == 6)
    }
}

/// The source Chao traversal/card-push operation. Direction0 up,1 down,2 left,
///3 right; all counters advance in original video frames, not milliseconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMemoryShuffle {
    pub board: NativeMemoryBoard,
    pub column: i8,
    pub row: i8,
    pub remaining_steps: u8,
    pub segment_remaining: u8,
    pub step_timer: i16,
    pub animation_frame: u8,
    pub trip: u8,
    pub direction: u32,
    pub x: i16,
    pub y: i16,
    pub moving_card: i8,
    pub moving_remaining: u8,
    pub moving_x: i16,
    pub moving_y: i16,
    pub moving_direction: u32,
    pub substate: u32,
    pub timer: u32,
    pub finished: bool,
}

impl NativeMemoryShuffle {
    /// Exact sub_0200A7AC initialization, before its first walk update.
    pub fn begin(board: NativeMemoryBoard, rng: &mut NativeRng) -> Self {
        let entry = (rng.next_u15() >> 7) % 22;
        let (column, row, direction) = match entry {
            0..=5 => (entry as i8, 0, 1),
            6..=11 => ((entry - 6) as i8, 4, 0),
            12..=16 => (0, (entry - 12) as i8, 3),
            _ => (5, (entry - 17) as i8, 2),
        };
        let mut result = Self {
            board,
            column,
            row,
            remaining_steps: 15,
            segment_remaining: 0,
            step_timer: 32,
            animation_frame: 0,
            trip: 0,
            direction,
            x: 0,
            y: 0,
            moving_card: -1,
            moving_remaining: 0,
            moving_x: 0,
            moving_y: 0,
            moving_direction: 0,
            substate: 1,
            timer: 0,
            finished: false,
        };
        result.next_segment(rng);
        result
    }

    fn next_segment(&mut self, rng: &mut NativeRng) {
        let sample = i32::from(rng.next_u15() >> 7);
        let available = match self.direction {
            0 => self.row,
            1 => 4 - self.row,
            2 => self.column,
            3 => 5 - self.column,
            _ => unreachable!("validated direction"),
        };
        self.segment_remaining = ((i32::from(available) * sample) >> 8) as u8 + 1;
    }

    fn validate(&self) -> Result<(), Error> {
        NativeMemoryBoard::from_native_cells(self.board.cells)?;
        if !(0..6).contains(&self.column)
            || !(0..5).contains(&self.row)
            || self.direction > 3
            || self.moving_direction > 3
            || self.trip > 2
            || !(1..=2).contains(&self.substate)
            || self.remaining_steps > 15
            || self.segment_remaining > 5
            || !(-512..=32).contains(&self.step_timer)
            || self.moving_remaining > 32
            || !(-1..14).contains(&self.moving_card)
            || self.timer > 1_000_000
            || !(-2048..=2048).contains(&self.x)
            || !(-2048..=2048).contains(&self.y)
            || !(-2048..=2048).contains(&self.moving_x)
            || !(-2048..=2048).contains(&self.moving_y)
            || (self.moving_card >= 0 && self.moving_remaining == 0)
            || (self.substate == 1 && (self.remaining_steps == 0 || self.segment_remaining == 0))
        {
            return Err(Error::InvalidState);
        }
        Ok(())
    }

    pub fn step(&mut self, buttons: NativeButtons, rng: &mut NativeRng) -> Result<(), Error> {
        self.validate()?;
        if buttons.pressed & 8 != 0 {
            return Err(Error::UnsupportedPause);
        }
        let mut candidate = self.clone();
        let mut candidate_rng = *rng;
        candidate.step_unchecked(buttons, &mut candidate_rng)?;
        candidate.validate()?;
        *self = candidate;
        *rng = candidate_rng;
        Ok(())
    }

    fn step_unchecked(&mut self, buttons: NativeButtons, rng: &mut NativeRng) -> Result<(), Error> {
        if self.finished {
            return Ok(());
        }
        if self.trip == 1 {
            if self.timer == 190 {
                self.trip = 2;
            }
            self.timer += 1;
            // Native _0200B25E branches to the frame epilogue: walking and
            // an existing pushed card both remain frozen during the trip.
            return Ok(());
        } else {
            self.step_timer -= 1;
            if self.substate == 2 {
                if !(0..=222).contains(&(self.x + 31)) || self.y <= -32 || self.y > 159 {
                    self.finished = true;
                }
            } else {
                if self.step_timer == 0 {
                    self.remaining_steps -= 1;
                    if self.remaining_steps == 0 {
                        self.substate = 2;
                    } else {
                        self.segment_remaining -= 1;
                        if self.segment_remaining == 0 {
                            let sample = i32::from(rng.next_u15());
                            self.direction = if self.direction <= 1 {
                                if sample * 5 < i32::from(self.column) * 32768 {
                                    2
                                } else {
                                    3
                                }
                            } else if sample * 4 < i32::from(self.row) * 32768 {
                                0
                            } else {
                                1
                            };
                            self.next_segment(rng);
                        }
                        match self.direction {
                            0 => self.row -= 1,
                            1 => self.row += 1,
                            2 => self.column -= 1,
                            3 => self.column += 1,
                            _ => unreachable!(),
                        }
                        self.step_timer = 32;
                    }
                }
                if self.step_timer == 20 {
                    let position = self.row as usize * COLUMNS + self.column as usize;
                    let card = self.board.cells[position];
                    if card >= 0 && rng.next_u15() <= 0x5fff {
                        let (column, row) = match self.direction {
                            0 => (self.column, self.row - 1),
                            1 => (self.column, self.row + 1),
                            2 => (self.column - 1, self.row),
                            3 => (self.column + 1, self.row),
                            _ => unreachable!(),
                        };
                        if (0..6).contains(&column) && (0..5).contains(&row) {
                            let target = row as usize * COLUMNS + column as usize;
                            if self.board.cells[target] < 0 {
                                self.moving_card = card;
                                self.moving_x = i16::from(self.column) * 32;
                                self.moving_y = i16::from(self.row) * 32;
                                self.moving_remaining = 32;
                                self.moving_direction = self.direction;
                                self.board.cells[target] = card;
                                self.board.cells[position] = EMPTY;
                            }
                        }
                    }
                }
            }
        }
        self.x = i16::from(self.column) * 32 + 4;
        self.y = i16::from(self.row) * 32 + 4;
        match self.direction {
            0 => self.y += self.step_timer,
            1 => self.y -= self.step_timer,
            2 => self.x += self.step_timer,
            3 => self.x -= self.step_timer,
            _ => unreachable!(),
        }
        self.animation_frame = self.animation_frame.wrapping_add(1);
        if self.moving_card >= 0 {
            match self.moving_direction {
                0 => self.moving_y -= 1,
                1 => self.moving_y += 1,
                2 => self.moving_x -= 1,
                3 => self.moving_x += 1,
                _ => unreachable!(),
            }
            self.moving_remaining -= 1;
            if self.moving_remaining == 0 {
                self.moving_card = EMPTY;
                self.timer = 0;
            }
        }
        // Native trip is accepted only in horizontal traversal, once per round.
        if buttons.pressed & 3 != 0 && self.direction >= 2 && self.trip == 0 {
            self.trip = 1;
            self.timer = 0;
        }
        Ok(())
    }

    pub fn read_native_fields(raw: &[u8]) -> Result<Self, Error> {
        if raw.len() < NATIVE_FIELDS_LEN {
            return Err(Error::TruncatedFields);
        }
        let word = |offset| u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap());
        let half = |offset| i16::from_le_bytes(raw[offset..offset + 2].try_into().unwrap());
        let main = word(0x68);
        if main != 5 && main != 6 {
            return Err(Error::InvalidState);
        }
        let value = Self {
            board: NativeMemoryBoard::from_native_cells(std::array::from_fn(|index| {
                raw[0x20 + index] as i8
            }))?,
            column: raw[0x47] as i8,
            row: raw[0x48] as i8,
            remaining_steps: raw[0x49],
            segment_remaining: raw[0x4a],
            step_timer: half(0x4c),
            animation_frame: raw[0x4e],
            trip: raw[0x4f],
            direction: word(0x50),
            x: half(0x54),
            y: half(0x56),
            moving_card: raw[0x58] as i8,
            moving_remaining: raw[0x59],
            moving_x: half(0x5a),
            moving_y: half(0x5c),
            moving_direction: word(0x60),
            substate: word(0x70),
            timer: word(0x74),
            finished: main == 6,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn write_native_fields(&self, raw: &mut [u8]) -> Result<(), Error> {
        if raw.len() < NATIVE_FIELDS_LEN {
            return Err(Error::TruncatedFields);
        }
        self.validate()?;
        for (index, &card) in self.board.cells.iter().enumerate() {
            raw[0x20 + index] = card as u8;
        }
        raw[0x47] = self.column as u8;
        raw[0x48] = self.row as u8;
        raw[0x49] = self.remaining_steps;
        raw[0x4a] = self.segment_remaining;
        raw[0x4c..0x4e].copy_from_slice(&self.step_timer.to_le_bytes());
        raw[0x4e] = self.animation_frame;
        raw[0x4f] = self.trip;
        raw[0x50..0x54].copy_from_slice(&self.direction.to_le_bytes());
        raw[0x54..0x56].copy_from_slice(&self.x.to_le_bytes());
        raw[0x56..0x58].copy_from_slice(&self.y.to_le_bytes());
        raw[0x58] = self.moving_card as u8;
        raw[0x59] = self.moving_remaining;
        raw[0x5a..0x5c].copy_from_slice(&self.moving_x.to_le_bytes());
        raw[0x5c..0x5e].copy_from_slice(&self.moving_y.to_le_bytes());
        raw[0x60..0x64].copy_from_slice(&self.moving_direction.to_le_bytes());
        raw[0x70..0x74].copy_from_slice(&self.substate.to_le_bytes());
        raw[0x74..0x78].copy_from_slice(&self.timer.to_le_bytes());
        raw[0x68..0x6c].copy_from_slice(&(if self.finished { 6_u32 } else { 5 }).to_le_bytes());
        if self.finished {
            raw[0x3e] = 2;
            raw[0x3f] = 2;
            raw[0x40] = 0;
            raw[0x42..0x44].copy_from_slice(&84_i16.to_le_bytes());
            raw[0x44..0x46].copy_from_slice(&56_i16.to_le_bytes());
            raw[0x46] = 0;
            raw[0x88] = 0;
            raw[0x65] = 1;
            raw[0x6c..0x70].fill(0);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordered_board() -> NativeMemoryBoard {
        let mut cells = [EMPTY; CELLS];
        for (index, slot) in cells[..CARD_COUNT].iter_mut().enumerate() {
            *slot = index as i8;
        }
        NativeMemoryBoard::from_native_cells(cells).unwrap()
    }

    #[test]
    fn native_cpu_seed_zero_fixture_includes_five_rows_and_two_draw_rejections() {
        // Observed from the actual A2NE sub_0200A554 instruction execution.
        let mut rng = NativeRng::new(0);
        let board = A2neMemorySource(()).create_board(&mut rng).unwrap();
        assert_eq!(
            *board.native_cells(),
            [
                9, 5, 12, 8, 3, 0, -1, -1, -1, -1, -1, -1, 2, -1, -1, -1, 13, -1, -1, -1, -1, -1,
                1, 7, -1, 10, 11, -1, 6, 4
            ]
        );
        assert_eq!(rng.state(), 0x473a_208c);
        assert!(board.placement_random_calls > 28);
        assert_eq!(board.placement_random_calls % 2, 0);
        let mut rng = NativeRng::new(91);
        assert_eq!(
            create_board_with_limit(&mut rng, 2),
            Err(Error::RandomBudgetExceeded)
        );
        assert_eq!(rng.state(), 91);
    }

    #[test]
    fn native_ring_zones_are_eighteen_outer_ten_middle_and_two_center_cells() {
        let mut counts = [0; 6];
        for row in 0..ROWS as u8 {
            for column in 0..COLUMNS as u8 {
                counts[usize::from(cell_ring_value(column, row).unwrap())] += 1;
            }
        }
        assert_eq!((counts[1], counts[3], counts[5]), (18, 10, 2));
        assert_eq!(cell_ring_value(2, 2), Ok(5));
        assert_eq!(cell_ring_value(2, 3), Ok(3));
        assert!(cell_ring_value(6, 4).is_err());
        assert_eq!(rings_after_clear_bonus(24, 0), Ok(84));
        assert_eq!(rings_after_clear_bonus(42, 0), Ok(99));
        assert_eq!(rings_after_clear_bonus(24, 1), Ok(54));
        assert_eq!(rings_after_clear_bonus(24, 2), Ok(34));
        assert!(rings_after_clear_bonus(43, 0).is_err());
        assert!(rings_after_clear_bonus(24, 3).is_err());
    }

    fn select(game: &mut NativeMemoryMatching, cell: usize) {
        game.cursor_column = (cell % COLUMNS) as u8;
        game.cursor_row = (cell / COLUMNS) as u8;
        game.step(NativeButtons {
            pressed: 1,
            held: 1,
        })
        .unwrap();
    }

    #[test]
    fn seven_native_pairs_remove_cards_after_flip_and_award_after_native_delays() {
        let mut game = NativeMemoryMatching::begin_after_native_shuffle(ordered_board());
        for pair in 0..7 {
            select(&mut game, pair * 2);
            select(&mut game, pair * 2 + 1);
            assert_eq!(game.state, 2);
            assert_eq!(game.matched_pairs, pair as u8);
            for _ in 0..256 {
                if game.state == 0 || game.ending.is_some() {
                    break;
                }
                game.step(NativeButtons::default()).unwrap();
            }
            assert_eq!(game.matched_pairs, (pair + 1) as u8);
            assert_eq!(game.board.cells[pair * 2], EMPTY);
            assert_eq!(game.board.cells[pair * 2 + 1], EMPTY);
        }
        assert_eq!(game.ending, Some(true));
        assert_eq!(game.mistakes, 0);
        assert_eq!(game.rings, 24);
        assert_eq!(game.timer, 21); // Native travel counter survives the win.
    }

    #[test]
    fn native_mismatch_has_eighty_four_frame_hold_then_reverse_and_three_mistakes_lose() {
        let mut game = NativeMemoryMatching::begin_after_native_shuffle(ordered_board());
        for mistake in 1..=3 {
            select(&mut game, 0);
            select(&mut game, 2);
            for _ in 0..40 {
                if game.state == 3 {
                    break;
                }
                game.step(NativeButtons::default()).unwrap();
            }
            assert_eq!(game.state, 3);
            assert_eq!(game.timer, 0);
            for _ in 0..83 {
                game.step(NativeButtons::default()).unwrap();
            }
            assert_eq!(game.mistakes, mistake - 1);
            assert_eq!(game.state, 3);
            game.step(NativeButtons::default()).unwrap();
            assert_eq!(game.mistakes, mistake);
            assert_eq!(game.state, 4);
            assert_eq!(game.selected[0].flip, 29);
            for _ in 0..40 {
                if game.state == 0 || game.ending.is_some() {
                    break;
                }
                game.step(NativeButtons::default()).unwrap();
            }
        }
        assert_eq!(game.ending, Some(false));
        assert_eq!(game.rings, 0);
        assert_eq!(game.selected[0].flip, -1); // Original same-frame reverse quirk.
        assert_eq!(game.board.cells, ordered_board().cells);
    }

    #[test]
    fn cursor_wraps_and_second_press_on_same_native_id_does_not_select_it_again() {
        let mut game = NativeMemoryMatching::begin_after_native_shuffle(ordered_board());
        game.cursor_column = 0;
        game.cursor_row = 0;
        game.step(NativeButtons {
            pressed: 32 | 64,
            held: 0,
        })
        .unwrap();
        assert_eq!((game.cursor_column, game.cursor_row), (5, 4));
        select(&mut game, 0);
        select(&mut game, 0);
        assert_eq!(game.state, 1);
        assert_eq!(game.selected[0].flip, 2);
    }

    #[test]
    fn raw_fields_preserve_unknowns_and_invalid_states_cannot_partially_mutate() {
        let game = NativeMemoryMatching::begin_after_native_shuffle(ordered_board());
        let mut raw = [0x5a; NATIVE_FIELDS_LEN + 8];
        game.write_native_fields(&mut raw).unwrap();
        let restored = NativeMemoryMatching::read_native_fields(&raw).unwrap();
        assert_eq!(restored, game);
        for offset in [
            1,
            2,
            3,
            0x40,
            0x41,
            0x42,
            0x43,
            0x44,
            0x45,
            0x70,
            0x71,
            0x72,
            0x73,
            0x7d,
            0x7e,
            0x7f,
            0x85,
            0x86,
            0x87,
            NATIVE_FIELDS_LEN,
        ] {
            assert_eq!(raw[offset], 0x5a);
        }
        let mut corrupt = game.clone();
        corrupt.board.cells[0] = 14;
        let before = corrupt.clone();
        assert!(
            corrupt
                .step(NativeButtons {
                    pressed: 1,
                    held: 1
                })
                .is_err()
        );
        assert_eq!(corrupt, before);
        let mut short = [0x5a; NATIVE_FIELDS_LEN - 1];
        let before = short;
        assert!(game.write_native_fields(&mut short).is_err());
        assert_eq!(short, before);
        assert!(NativeMemoryMatching::read_native_fields(&short).is_err());
        assert!(A2neMemorySource::from_verified_payload(&[]).is_err());
    }

    #[test]
    fn original_reveal_takes_two_hundred_seventy_frames_with_three_ring_zones() {
        let mut reveal = NativeMemoryReveal::default();
        for _ in 0..60 {
            assert!(!reveal.step().unwrap());
        }
        assert_eq!((reveal.stage, reveal.timer), (1, 0));
        for _ in 0..30 {
            reveal.step().unwrap();
        }
        assert_eq!(reveal.visible.iter().filter(|&&value| value).count(), 28);
        for _ in 0..90 {
            reveal.step().unwrap();
        }
        assert_eq!(reveal.visible.iter().filter(|&&value| value).count(), 18);
        for _ in 0..89 {
            assert!(!reveal.step().unwrap());
        }
        assert!(reveal.step().unwrap());
        assert!(reveal.visible.iter().all(|&value| !value));
    }

    #[test]
    fn native_shuffle_finishes_preserves_every_card_and_is_rng_deterministic() {
        for seed in 0..128 {
            let mut rng = NativeRng::new(seed);
            let board = A2neMemorySource(()).create_board(&mut rng).unwrap();
            let mut shuffle = NativeMemoryShuffle::begin(board, &mut rng);
            let mut replay = shuffle.clone();
            let mut replay_rng = rng;
            for frame in 0..1500 {
                if shuffle.finished {
                    break;
                }
                let buttons = NativeButtons {
                    pressed: if frame == 20 { 1 } else { 0 },
                    held: 0,
                };
                shuffle.step(buttons, &mut rng).unwrap();
                replay.step(buttons, &mut replay_rng).unwrap();
                assert_eq!(shuffle, replay);
                assert_eq!(rng, replay_rng);
            }
            assert!(shuffle.finished, "seed{seed}");
            let mut cards: Vec<_> = shuffle
                .board
                .native_cells()
                .iter()
                .copied()
                .filter(|&card| card >= 0)
                .collect();
            cards.sort();
            assert_eq!(cards, (0..14).collect::<Vec<_>>());
            let mut raw = [0; NATIVE_FIELDS_LEN];
            shuffle.write_native_fields(&mut raw).unwrap();
            let restored = NativeMemoryShuffle::read_native_fields(&raw).unwrap();
            assert_eq!(restored.board.cells, shuffle.board.cells);
            assert_eq!(restored.direction, shuffle.direction);
            assert!(restored.finished);
        }
    }

    #[test]
    fn unsupported_pause_and_invalid_shuffle_fail_without_rng_or_state_mutation() {
        let mut rng = NativeRng::new(2);
        let mut shuffle = NativeMemoryShuffle::begin(ordered_board(), &mut rng);
        let before = shuffle.clone();
        let seed = rng;
        assert_eq!(
            shuffle.step(
                NativeButtons {
                    pressed: 8,
                    held: 0
                },
                &mut rng
            ),
            Err(Error::UnsupportedPause)
        );
        assert_eq!(shuffle, before);
        assert_eq!(rng, seed);
        shuffle.moving_card = 0;
        shuffle.moving_remaining = 0;
        let before = shuffle.clone();
        assert!(shuffle.step(NativeButtons::default(), &mut rng).is_err());
        assert_eq!(shuffle, before);
        assert_eq!(rng, seed);
    }

    #[test]
    fn native_result_screen_stages_bonus_and_finishes_once_after_wait_and_fade() {
        for (mistakes, expected) in [(0, 84), (1, 54), (2, 34)] {
            let mut result = NativeMemoryResult::begin(true, mistakes, 24).unwrap();
            for _ in 0..61 {
                result.step(NativeButtons::default()).unwrap();
                assert_eq!(result.rings, 24);
            }
            result.step(NativeButtons::default()).unwrap();
            assert_eq!(result.stage, u32::from(mistakes) + 2);
            for _ in 0..500 {
                if result.done {
                    break;
                }
                result.step(NativeButtons::default()).unwrap();
            }
            assert!(result.done);
            assert_eq!(result.rings, expected);
            let before = result.clone();
            result
                .step(NativeButtons {
                    pressed: 1,
                    held: 1,
                })
                .unwrap();
            assert_eq!(result, before);
        }
        let mut loss = NativeMemoryResult::begin(false, 3, 12).unwrap();
        for _ in 0..500 {
            if loss.done {
                break;
            }
            loss.step(NativeButtons::default()).unwrap();
        }
        assert!(loss.done);
        assert_eq!(loss.rings, 12);
        assert!(NativeMemoryResult::begin(true, 3, 12).is_err());

        // Inspection fields are mutable for CPU tools. Impossible imported UI
        // state must fail without leaving a partially advanced result.
        let mut corrupt = NativeMemoryResult::begin(true, 0, 24).unwrap();
        corrupt.stage = 1;
        corrupt.ui_kind = 1;
        corrupt.ui_timer = 180;
        let before = corrupt.clone();
        assert_eq!(
            corrupt.step(NativeButtons::default()),
            Err(Error::InvalidState)
        );
        assert_eq!(corrupt, before);
    }

    #[test]
    fn complete_round_is_playable_from_public_peek_and_visible_pushes_without_hidden_ids() {
        let mut round = A2neMemorySource(()).begin_round(825).unwrap();
        let before = round.view();
        let seed = round.rng;
        assert_eq!(
            round.step(NativeButtons {
                pressed: 8,
                held: 0
            }),
            Err(Error::UnsupportedPause)
        );
        assert_eq!(round.view(), before);
        assert_eq!(round.rng, seed);
        let mut knowledge = round.view().cells.map(|cell| cell.face);
        let mut old = round.view();
        for _ in 0..2000 {
            if round.view().phase == NativeMemoryPhase::Matching {
                break;
            }
            round.step(NativeButtons::default()).unwrap();
            let current = round.view();
            let from =
                (0..CELLS).find(|&cell| old.cells[cell].occupied && !current.cells[cell].occupied);
            let to =
                (0..CELLS).find(|&cell| !old.cells[cell].occupied && current.cells[cell].occupied);
            if let (Some(from), Some(to)) = (from, to) {
                knowledge[to] = knowledge[from];
                knowledge[from] = None;
            }
            for (index, cell) in current.cells.iter().enumerate() {
                if let Some(face) = cell.face {
                    knowledge[index] = Some(face);
                }
            }
            old = current;
        }
        assert_eq!(round.view().phase, NativeMemoryPhase::Matching);
        assert!(round.view().cells.iter().all(|cell| cell.face.is_none()));
        assert!(!format!("{round:?}").contains("rng"));
        let mut earned = 0;
        for face in 0..7 {
            let positions: Vec<_> = (0..CELLS)
                .filter(|&cell| knowledge[cell] == Some(face))
                .collect();
            assert_eq!(positions.len(), 2);
            for cell in positions {
                earned += round.view().cells[cell].ring_value;
                let (column, row) = ((cell % COLUMNS) as u8, (cell / COLUMNS) as u8);
                while round.view().cursor.unwrap().0 != column {
                    round
                        .step(NativeButtons {
                            pressed: 16,
                            held: 0,
                        })
                        .unwrap();
                }
                while round.view().cursor.unwrap().1 != row {
                    round
                        .step(NativeButtons {
                            pressed: 128,
                            held: 0,
                        })
                        .unwrap();
                }
                round
                    .step(NativeButtons {
                        pressed: 1,
                        held: 1,
                    })
                    .unwrap();
            }
            for _ in 0..256 {
                let view = round.view();
                if view.phase != NativeMemoryPhase::Matching
                    || view.selected.iter().all(Option::is_none)
                {
                    break;
                }
                round.step(NativeButtons::default()).unwrap();
            }
        }
        assert_eq!(round.view().phase, NativeMemoryPhase::Result);
        assert!(round.finished().is_none());
        for _ in 0..500 {
            if round.finished().is_some() {
                break;
            }
            round.step(NativeButtons::default()).unwrap();
        }
        assert_eq!(
            round.finished(),
            Some(NativeMemoryFinished {
                won: true,
                rings: rings_after_clear_bonus(earned, 0).unwrap()
            })
        );
        assert_eq!(round.view().phase, NativeMemoryPhase::Ended);
        let before = round.view();
        round
            .step(NativeButtons {
                pressed: 1,
                held: 1,
            })
            .unwrap();
        assert_eq!(round.view(), before);
    }
}
