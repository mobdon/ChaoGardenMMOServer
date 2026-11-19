//! Mechanics from the A3VE Tiny Garden's second game. See NATIVE_BALL.md.
//! Each method advances its named native callback, not the GBA task scheduler.

use crate::native_growth::NativeRng;
use sha2::{Digest, Sha256};

pub const COLUMNS: usize = 10;
pub const ROWS: usize = 10;
/// Rows 1..=10 and columns 1..=10 of the native 12-byte-stride grid.
pub type Board = [[u8; COLUMNS]; ROWS];
pub const CEILING: u8 = 0xf0;
pub const PAYLOAD_SHA256: &str = "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb";

#[derive(Debug, Clone, Copy)]
pub struct BallSource {
    quotas: [[u8; 3]; 4],
    markers: [u8; 3],
    rays: [(i8, i8); 7],
}

impl BallSource {
    pub fn from_verified_payload(payload: &[u8]) -> Option<Self> {
        if payload.len() != 188_716 || format!("{:x}", Sha256::digest(payload)) != PAYLOAD_SHA256 {
            return None;
        }
        Some(Self {
            quotas: std::array::from_fn(|stage| {
                std::array::from_fn(|i| payload[0x2e108 + 3 * stage + i])
            }),
            markers: std::array::from_fn(|i| payload[0x2e114 + i]),
            rays: std::array::from_fn(|i| {
                (
                    payload[0x2e11b + 2 * i] as i8,
                    payload[0x2e11c + 2 * i] as i8,
                )
            }),
        })
    }

    /// Stage 0..=3. The native final color run is written in full, even
    /// when it extends beyond the last row that the board can consume.
    pub fn create_stage(&self, stage: u8, rng: &mut NativeRng) -> Option<BallStage> {
        let quotas = *self.quotas.get(usize::from(stage))?;
        let rows = 20 + 10 * usize::from(stage);
        let mut feed = Vec::with_capacity(rows * COLUMNS + 3);
        let mut color = (rng.next_u15() as u8 & 1) + 1;
        while feed.len() < rows * COLUMNS {
            let count = usize::from(((rng.next_u15() >> 4) & 3) + 1);
            feed.extend(std::iter::repeat_n(color, count));
            color = 3 - color;
        }
        let remaining = quotas.iter().sum::<u8>();
        let mut cursor = rows << 8;
        let spacing = cursor / usize::from(remaining);
        for (marker, count) in self.markers.iter().zip(quotas) {
            for _ in 0..count {
                let offset = (cursor / 256 - 1) * COLUMNS + 1;
                feed[offset + usize::from((rng.next_u15() >> 4) & 7)] = *marker;
                cursor -= spacing;
            }
        }
        Some(BallStage {
            feed,
            rows,
            board: [[0; COLUMNS]; ROWS],
            remaining,
            falling: false,
            flags: 0,
            scroll: 0,
            timer: 30,
            period: [116, 96, 76, 57][usize::from(stage)],
            next_row: 0,
        })
    }

    /// The seven directions at 0202E11B are visited in native order. There
    /// is no straight-down ray. A low-nibble zero stops traversal, including
    /// a ring. Return the cells that receive the native flip effect.
    pub fn flip(&self, board: &mut Board, row: usize, column: usize) -> Option<Board> {
        let &color = board.get(row)?.get(column)?;
        if !matches!(color, 1 | 2) {
            return None;
        }
        let mut effects = [[0; COLUMNS]; ROWS];
        for &(dy, dx) in &self.rays {
            let mut run: Vec<(usize, usize)> = Vec::new();
            let (mut y, mut x) = (row as isize, column as isize);
            loop {
                y += dy as isize;
                x += dx as isize;
                let Some(&cell) = board.get(y as usize).and_then(|r| r.get(x as usize)) else {
                    break;
                };
                match cell & 15 {
                    0 => break,
                    v if v == color => {
                        for (r, c) in run {
                            board[r][c] = color;
                            effects[r][c] = 0x18;
                        }
                        break;
                    }
                    _ => run.push((y as usize, x as usize)),
                }
            }
        }
        Some(effects)
    }
}

#[derive(Clone, Debug)]
pub struct BallStage {
    pub board: Board,
    /// Generated upcoming cells, including the unused tail of the last run.
    feed: Vec<u8>,
    rows: usize,
    next_row: usize,
    pub remaining: u8,
    /// Set by the ring task even on a paused frame; consumed by descent.
    pub falling: bool,
    /// Flight (1), scrolling (2), resolution (4), loss (64), clear (128).
    pub flags: u8,
    pub scroll: i16,
    pub timer: u8,
    pub period: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingSpawn {
    pub column: u8,
    pub x: i16,
    pub y: i16,
    pub delay: u8,
}

impl BallStage {
    pub fn generated_feed(&self) -> &[u8] {
        &self.feed
    }

    /// Actor 0200E1F0. Spawns are in the cartridge's column, row and
    /// reverse-delay order. The caller schedules emitted ring actors.
    pub fn descend(&mut self, scene_flags: u8) -> Vec<RingSpawn> {
        if scene_flags & 0x83 != 0 {
            return Vec::new();
        }
        if self.scroll != 0 {
            self.scroll -= 2;
            if self.scroll == 0 {
                self.flags &= !2;
                return self.release_rings(1);
            }
            return Vec::new();
        }
        if self.flags == 0 && self.board[8].iter().any(|v| v & 15 != 0) {
            self.flags |= 0x40;
        } else if self.remaining == 0 && !self.falling {
            self.flags |= 0x80;
        } else {
            self.falling = false;
            if self.flags & !1 == 0 && self.remaining > 0 {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    self.timer = self.period;
                    self.board.copy_within(0..ROWS - 1, 1);
                    if self.next_row < self.rows {
                        let offset = self.next_row * COLUMNS;
                        self.board[0].copy_from_slice(&self.feed[offset..offset + COLUMNS]);
                        self.next_row += 1;
                    } else {
                        self.board[0].fill(CEILING);
                    }
                    self.scroll = 14;
                    self.flags |= 2;
                }
            }
        }
        Vec::new()
    }

    /// 0200F2DC. Descent excludes native row 1. Shot resolution includes it.
    fn release_rings(&mut self, excluded_rows: usize) -> Vec<RingSpawn> {
        let mut spawn = Vec::new();
        for c in 0..COLUMNS {
            for r in (excluded_rows..9).rev() {
                let cell = self.board[r][c];
                if cell & 15 != 0 {
                    break;
                }
                if cell == 0 || cell == CEILING {
                    continue;
                }
                self.board[r][c] = 0;
                self.remaining = self.remaining.wrapping_sub(1);
                for index in (0..cell >> 4).rev() {
                    spawn.push(RingSpawn {
                        column: c as u8 + 1,
                        x: 24 + 16 * c as i16,
                        y: 8 + 16 * r as i16,
                        delay: 3 * index,
                    });
                }
            }
        }
        spawn
    }

    pub fn release_after_resolution(&mut self) -> Vec<RingSpawn> {
        self.release_rings(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingStep {
    Falling,
    Caught,
    Missed,
}

#[derive(Debug, Clone, Copy)]
pub struct FallingRing {
    pub x: i16,
    /// Signed 16.16 position at native task offset 0xC.
    pub y: i32,
    /// Signed 8.8 velocity at offset 0x20.
    pub velocity: i16,
    pub delay: u8,
}
impl From<RingSpawn> for FallingRing {
    fn from(spawn: RingSpawn) -> Self {
        Self {
            x: spawn.x,
            y: i32::from(spawn.y) << 16,
            velocity: 0,
            delay: spawn.delay,
        }
    }
}
impl FallingRing {
    /// Caller removes or replaces this task immediately on Caught/Missed.
    pub fn step(&mut self, player_x: i16, scene_flags: u8, board_flags: u8) -> RingStep {
        if scene_flags & 0x80 != 0 {
            return RingStep::Falling;
        }
        if self.delay > 0 {
            self.delay -= 1;
            return RingStep::Falling;
        }
        self.velocity = self.velocity.wrapping_add(0x20);
        self.y = self.y.wrapping_add(i32::from(self.velocity) << 8);
        let y = self.y >> 16;
        if y > 167 {
            return RingStep::Missed;
        }
        if y >= 136 && board_flags & 0x40 == 0 && self.x.abs_diff(player_x) < 12 {
            RingStep::Caught
        } else {
            RingStep::Falling
        }
    }
}

/// Native cartridge wallet arithmetic at 0200F64C. Usable for a cartridge
/// reconstruction; canonical MMO wallet mutation and receipts belong to the server.
pub fn cartridge_wallet(wallet: u32, round_rings: u16) -> u32 {
    wallet.wrapping_add(u32::from(round_rings)).min(99_999)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    /// Native column, 1..=10.
    pub column: u8,
    pub color: u8,
    pub y: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotStep {
    Flying,
    /// Native row and column in 1..=10. The resolution task writes the cell.
    Landed {
        row: u8,
        column: u8,
        color: u8,
    },
}

impl Shot {
    pub fn launch(column: u8, pressed: u16, board: &Board, flags: u8) -> Option<Self> {
        if !(1..=10).contains(&column)
            || flags & !2 != 0
            || pressed & 3 == 0
            || board[7][usize::from(column - 1)] != 0
        {
            return None;
        }
        // B takes priority when both keys have an edge. The spawn callback
        // immediately calls the flight callback, so the caller ticks it once.
        Some(Self {
            column,
            color: if pressed & 2 != 0 { 1 } else { 2 },
            y: 155,
        })
    }

    /// 0200E08C. Invalid coordinates or an unreachable shot below the
    /// playfield return None instead of addressing unrelated GBA memory.
    pub fn step(&mut self, board: &Board, scroll: i16, scene_flags: u8) -> Option<ShotStep> {
        if !(1..=10).contains(&self.column) || !matches!(self.color, 1 | 2) {
            return None;
        }
        if scene_flags & 0x80 != 0 {
            return Some(ShotStep::Flying);
        }
        let y = self.y.wrapping_sub(4);
        let row = (i32::from(y) + i32::from(scroll) + 8) >> 4;
        let hit = if row == 0 {
            true
        } else {
            *board
                .get(usize::try_from(row - 1).ok()?)?
                .get(usize::from(self.column - 1))?
                != 0
        };
        if hit && row >= 10 {
            return None;
        }
        self.y = y;
        Some(if hit {
            ShotStep::Landed {
                row: row as u8 + 1,
                column: self.column,
                color: self.color,
            }
        } else {
            ShotStep::Flying
        })
    }
}

/// Gameplay fields from the parent task (0200DBA8) and the two animation
/// counters it shares with its child. Call control before the child update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Launcher {
    pub x: i16,
    pub column: u8,
    pub phase: u8,
    pub facing_right: bool,
    pub timer: u8,
    pub anim_frame: u8,
    pub anim_timer: u8,
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            x: 104,
            column: 6,
            phase: 2,
            facing_right: false,
            timer: 0,
            anim_frame: 0,
            anim_timer: 0,
        }
    }
}

impl Launcher {
    fn reset_animation(&mut self) {
        self.anim_frame = 0;
        self.anim_timer = 0;
    }

    fn start_move(&mut self, held: u16) {
        self.facing_right = held & 0x20 == 0;
        if self.facing_right {
            self.column += 1;
        } else {
            self.column -= 1;
        }
        self.phase = 3;
        self.timer = 8;
    }

    /// Output is a pending projectile task; the pool activates its callback
    /// according to the native task order. Terminal animation phases are 5/6.
    pub fn control(
        &mut self,
        held: u16,
        pressed: u16,
        scene_flags: u8,
        board_flags: u8,
        board: &Board,
    ) -> Option<Shot> {
        if scene_flags & 0x93 != 0 || !(1..=10).contains(&self.column) {
            return None;
        }
        if self.phase < 5 {
            if board_flags & 0x40 != 0 {
                self.phase = 5;
                self.reset_animation();
            } else if board_flags & 0x84 == 0x80 {
                self.phase = 6;
                self.reset_animation();
            }
        }
        if self.phase == 0 {
            self.timer = 20;
            self.phase = 1;
        }
        if self.phase == 1 {
            self.timer = self.timer.wrapping_sub(1);
            if self.timer == 0 {
                self.anim_frame = 1;
                self.anim_timer = 0;
                self.phase = 2;
            }
        }
        if self.phase <= 2 {
            if let Some(shot) = Shot::launch(self.column, pressed, board, board_flags) {
                self.facing_right = shot.color == 2;
                self.phase = 4;
                self.reset_animation();
                return Some(shot);
            }
            let left = held & 0x20 != 0;
            if held & 0x30 != 0 && ((left && self.column > 1) || (!left && self.column < 10)) {
                self.reset_animation();
                self.start_move(held);
            }
        } else if self.phase == 3 {
            self.x = self.x.wrapping_add(if self.facing_right { 2 } else { -2 });
            self.timer = self.timer.wrapping_sub(1);
            if self.timer == 0 {
                if (2..=9).contains(&self.column) && held & 0x30 != 0 {
                    self.start_move(held);
                } else {
                    self.phase = 0;
                }
            }
        } else if self.phase == 4 && self.anim_timer > 4 {
            self.anim_timer = 0;
            self.anim_frame = self.anim_frame.wrapping_add(1);
            if self.anim_frame > 2 {
                self.phase = 0;
            }
        }
        None
    }
}

/// Game logic sequenced by the shot-resolution task at 0200E430. Coordinates
/// in this Rust type are zero-based. Graphics, audio and interpolation use
/// distinct presentation fields.
#[derive(Clone, Debug)]
pub struct Resolution {
    pub phase: u8,
    pub timer: u8,
    pub effects: Board,
    flipped: bool,
    row: usize,
    column: usize,
    color: u8,
    gaps: [usize; COLUMNS],
    offset: u8,
    candidates: Vec<(usize, usize, u8)>,
}

impl Resolution {
    pub fn new(row: u8, column: u8, color: u8) -> Option<Self> {
        if !(1..=10).contains(&row) || !(1..=10).contains(&column) || !matches!(color, 1 | 2) {
            return None;
        }
        Some(Self {
            phase: 0,
            timer: 0,
            effects: [[0; COLUMNS]; ROWS],
            flipped: false,
            row: usize::from(row - 1),
            column: usize::from(column - 1),
            color,
            gaps: [ROWS; COLUMNS],
            offset: 0,
            candidates: Vec::new(),
        })
    }

    fn mark_groups(&mut self, board: &mut Board) -> bool {
        // Copy the low nibbles of native rows 1..9. The bottom row belongs
        // to the loss boundary. Native clears are orthogonal, with 10 minimum.
        let mut work = [[0; COLUMNS]; ROWS];
        for (dst, src) in work[..9].iter_mut().zip(&board[..9]) {
            for (cell, &value) in dst.iter_mut().zip(src) {
                *cell = value & 15;
            }
        }
        let mut removed = false;
        while let Some((row, column, color)) = self.candidates.pop() {
            if work[row][column] != color {
                continue;
            }
            // Match the scanline fill's native 64-entry circular queue, even
            // if many adjacent cells enqueue the same span.
            let mut queue = [(0usize, 0usize); 64];
            queue[0] = (row, column);
            let (mut read, mut write) = (0, 1);
            let mut cells = Vec::new();
            while read != write {
                let (r, mut c) = queue[read];
                read = (read + 1) & 63;
                if work[r][c] == 0 {
                    continue;
                }
                while c > 0 && work[r][c - 1] == color {
                    c -= 1;
                }
                while c < COLUMNS && work[r][c] == color {
                    for adjacent in [r.checked_add(1).filter(|&i| i < ROWS), r.checked_sub(1)]
                        .into_iter()
                        .flatten()
                    {
                        if work[adjacent][c] == color {
                            queue[write] = (adjacent, c);
                            write = (write + 1) & 63;
                        }
                    }
                    work[r][c] = 0;
                    cells.push((r, c));
                    c += 1;
                }
            }
            if cells.len() >= 10 {
                for (r, c) in cells {
                    board[r][c] = 0x18;
                }
                removed = true;
            }
        }
        if removed {
            self.effects = [[0; COLUMNS]; ROWS];
            for (r, row) in board.iter_mut().enumerate() {
                for (c, v) in row.iter_mut().enumerate() {
                    if *v == 0x18 {
                        self.effects[r][c] = *v;
                        *v = 0;
                    }
                }
            }
            for c in 0..COLUMNS {
                self.gaps[c] = (0..9).find(|&r| board[r][c] == 0).unwrap_or(ROWS);
            }
        }
        removed
    }

    fn lift(&mut self, board: &mut Board) -> bool {
        self.effects = [[0; COLUMNS]; ROWS];
        let mut any = false;
        for c in 0..COLUMNS {
            let mut gap = false;
            for (r, row) in board.iter_mut().enumerate() {
                if row[c] == 0 {
                    gap = true;
                } else if gap {
                    self.effects[r][c] = row[c];
                    row[c] = 0;
                    any = true;
                }
            }
        }
        self.offset = 4;
        any
    }

    fn seed_cascade(&mut self, board: &Board) -> bool {
        self.candidates.clear();
        for c in 0..COLUMNS {
            for (r, row) in board.iter().enumerate().take(9).skip(self.gaps[c]) {
                if row[c] & 15 != 0 {
                    self.candidates.push((r, c, row[c]));
                }
            }
        }
        !self.candidates.is_empty()
    }

    /// Returns Some on task retirement. The emitted rings must be scheduled
    /// before testing stage completion.
    pub fn step(
        &mut self,
        source: &BallSource,
        stage: &mut BallStage,
        scene_flags: u8,
    ) -> Option<Vec<RingSpawn>> {
        if scene_flags & 0x80 != 0 || self.phase > 5 {
            return None;
        }
        stage.flags |= 4;
        match self.phase {
            0 => {
                stage.board[self.row][self.column] = self.color;
                if stage.remaining != 0 {
                    self.phase = 1;
                    self.timer = 15;
                    return None;
                }
                stage.flags &= !4;
                self.phase = 6;
                return Some(Vec::new());
            }
            1 => {
                self.timer -= 1;
                if self.timer == 0 {
                    self.effects = source
                        .flip(&mut stage.board, self.row, self.column)
                        .expect("placed native shot");
                    self.flipped = self.effects.iter().flatten().any(|&v| v != 0);
                    self.timer = if self.flipped { 31 } else { 0 };
                    self.phase = 2;
                }
            }
            2 => {
                if self.flipped {
                    self.timer -= 1;
                }
                if self.timer == 0 {
                    self.candidates.push((self.row, self.column, self.color));
                    if self.mark_groups(&mut stage.board) {
                        self.phase = 3;
                        self.timer = 36;
                    } else {
                        self.phase = 6;
                        stage.flags &= !4;
                        return Some(Vec::new());
                    }
                }
            }
            3 if self.timer != 0 => {
                self.timer -= 1;
            }
            3 => {
                if self.lift(&mut stage.board) {
                    self.phase = 4;
                } else if self.seed_cascade(&stage.board) {
                    self.phase = 5;
                } else {
                    self.phase = 6;
                    stage.flags &= !4;
                    return Some(stage.release_after_resolution());
                }
            }
            4 => {
                self.offset += 4;
                if self.offset >= 16 {
                    for r in (1..ROWS).rev() {
                        for c in 0..COLUMNS {
                            let value = self.effects[r][c];
                            if value != 0 {
                                stage.board[r - 1][c] = value;
                            }
                        }
                    }
                    self.phase = 3;
                }
            }
            5 => {
                if self.mark_groups(&mut stage.board) {
                    self.phase = 3;
                    self.timer = 36;
                } else {
                    self.phase = 6;
                    stage.flags &= !4;
                    return Some(stage.release_after_resolution());
                }
            }
            _ => unreachable!(),
        }
        None
    }
}

/// Active gameplay between the intro fade and the result banner. Buttons use
/// GBA masks (A=1, B=2, Right=16, Left=32). The caller keeps time in frames.
#[derive(Clone, Debug)]
pub struct BallPlay {
    pub stage: BallStage,
    pub player: Launcher,
    pub shot: Option<Shot>,
    pub resolution: Option<Resolution>,
    pub rings: u16,
    /// (ring, column), in the original update order.
    rings_falling: Vec<(FallingRing, u8)>,
    caught_timers: Vec<u8>,
    catch_pose: u8,
    occupied: [bool; COLUMNS],
    last_buttons: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayOutcome {
    Playing,
    Lost,
    Cleared,
}

impl BallPlay {
    pub fn begin(
        source: &BallSource,
        stage: u8,
        rng: &mut NativeRng,
        round_rings: u16,
    ) -> Option<Self> {
        Some(Self {
            stage: source.create_stage(stage, rng)?,
            player: Launcher {
                anim_timer: 15,
                ..Launcher::default()
            },
            shot: None,
            resolution: None,
            rings: round_rings,
            rings_falling: Vec::new(),
            caught_timers: Vec::new(),
            catch_pose: 0,
            occupied: [false; COLUMNS],
            last_buttons: 0,
        })
    }

    pub fn falling_rings(&self) -> impl Iterator<Item = &FallingRing> {
        self.rings_falling.iter().map(|(r, _)| r)
    }

    fn spawn_rings(&mut self, spawn: Vec<RingSpawn>) {
        self.rings_falling
            .extend(spawn.into_iter().map(|d| (d.into(), d.column)));
    }

    /// Advances through the first loss or clear signal. Caller owns the
    /// result banner, next stage, and the one-time durable host settlement.
    pub fn step(&mut self, source: &BallSource, held: u16) -> PlayOutcome {
        let pressed = held & !self.last_buttons;
        self.last_buttons = held;
        self.step_keys(source, held, pressed)
    }

    fn step_keys(&mut self, source: &BallSource, held: u16, pressed: u16) -> PlayOutcome {
        let spawned = self.stage.descend(0);
        self.spawn_rings(spawned);
        if let Some(shot) =
            self.player
                .control(held, pressed, 0, self.stage.flags, &self.stage.board)
        {
            self.stage.flags |= 1;
            self.shot = Some(shot);
        }
        let occupied = self.occupied[self.player.column as usize - 1];
        self.occupied.fill(false);
        if let Some(shot) = self.shot.as_mut()
            && let Some(ShotStep::Landed { row, column, color }) =
                shot.step(&self.stage.board, self.stage.scroll, 0)
        {
            self.shot = None;
            self.stage.flags &= !1;
            self.resolution = Resolution::new(row, column, color);
        }
        // The display child also owns the animation timer used by control.
        let p = &mut self.player;
        p.anim_timer = p.anim_timer.wrapping_add(1);
        match p.phase {
            0..=2 if self.catch_pose > 0 => self.catch_pose -= 1,
            2 if !occupied => {
                if p.anim_timer > 30 {
                    p.anim_timer = 0;
                    p.anim_frame = p.anim_frame.wrapping_add(1);
                }
                if p.anim_frame > 3 && p.anim_frame & 1 == 0 && p.anim_timer > 5 {
                    p.anim_frame = 0;
                }
            }
            3 if p.anim_timer > 6 => {
                p.anim_timer = 0;
                p.anim_frame = p.anim_frame.wrapping_add(1);
            }
            _ => {}
        }
        for t in &mut self.caught_timers {
            *t -= 1;
            if *t != 0 {
                self.stage.falling = true;
            }
        }
        self.caught_timers.retain(|&t| t != 0);
        if let Some(op) = self.resolution.as_mut()
            && let Some(spawned) = op.step(source, &mut self.stage, 0)
        {
            self.resolution = None;
            self.spawn_rings(spawned);
        }
        self.rings_falling.retain_mut(|(r, column)| {
            let step = r.step(self.player.x, 0, self.stage.flags);
            if step != RingStep::Missed {
                self.stage.falling = true;
                self.occupied[*column as usize - 1] = true;
            }
            if step == RingStep::Caught {
                self.rings = self.rings.wrapping_add(1);
                self.catch_pose = 30;
                self.caught_timers.push(28);
            }
            step == RingStep::Falling
        });
        match self.player.phase {
            5 => PlayOutcome::Lost,
            6 => PlayOutcome::Cleared,
            _ => PlayOutcome::Playing,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    Quit,
    Lost,
    Cleared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Intro,
    Playing,
    Pause { quit_selected: bool },
    StageClear,
    NextStage,
    Result { outcome: Finish },
    Fade,
    Garden { outcome: Finish },
}

/// Cartridge round from the first frame of D7CC to return to Garden.
/// Success emits a ring amount only; credential, wallet and receipt ownership
/// belong to the calling authority.
#[derive(Clone, Debug)]
pub struct BallSession {
    source: BallSource,
    rng: NativeRng,
    stage_number: u8,
    pub play: BallPlay,
    screen: Screen,
    timer: u16,
    menu_lock: u8,
    previous: u16,
    ending: Option<Finish>,
}

impl BallSession {
    pub fn new(source: BallSource, seed: u32) -> Self {
        let mut rng = NativeRng::new(seed);
        let play = BallPlay::begin(&source, 0, &mut rng, 0).expect("native stage zero");
        Self {
            source,
            rng,
            play,
            stage_number: 0,
            screen: Screen::Intro,
            timer: 16,
            menu_lock: 4,
            previous: 0,
            ending: None,
        }
    }

    pub fn stage_number(&self) -> u8 {
        self.stage_number
    }
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Edges are derived from held buttons exactly once on each frame.
    /// The result is emitted only on the transition back to Garden.
    pub fn step(&mut self, held: u16) -> Option<(Finish, u16)> {
        let mut pressed = held & !self.previous;
        self.previous = held;
        match self.screen {
            Screen::Intro => {
                self.timer -= 1;
                if self.timer > 0 {
                    return None;
                }
                self.screen = Screen::Playing;
                self.menu_lock = self.menu_lock.saturating_sub(1);
            }
            Screen::Fade => {
                if self.timer > 0 {
                    self.timer -= 1;
                }
                if self.timer == 0 {
                    if let Some(outcome) = self.ending {
                        self.screen = Screen::Garden { outcome };
                        return Some((outcome, self.play.rings));
                    }
                    // F624 installs F8F8; on the next frame the latter
                    // generates the stage and installs the active callback.
                    self.screen = Screen::NextStage;
                }
                return None;
            }
            Screen::NextStage => {
                self.stage_number += 1;
                self.play = BallPlay::begin(
                    &self.source,
                    self.stage_number,
                    &mut self.rng,
                    self.play.rings,
                )
                .expect("four cartridge stages");
                self.screen = Screen::Intro;
                self.timer = 16;
                self.menu_lock = 4;
                return None;
            }
            Screen::Garden { .. } => return None,
            Screen::StageClear => {
                self.play.step_keys(&self.source, 0, 0);
                self.timer -= 1;
                if self.timer == 0 {
                    self.screen = Screen::Fade;
                    self.timer = 16;
                }
                return None;
            }
            Screen::Result { outcome } => {
                self.play.step_keys(&self.source, 0, 0);
                self.timer += 1;
                let ready = if outcome == Finish::Lost { 77 } else { 76 };
                if self.timer >= ready + 180 || (self.timer > ready && pressed & 9 != 0) {
                    self.screen = Screen::Fade;
                    self.timer = 16;
                }
                return None;
            }
            Screen::Pause { mut quit_selected } => {
                self.menu_lock = self.menu_lock.saturating_sub(1);
                if self.menu_lock == 0 && pressed & 0xb != 0 {
                    self.menu_lock = 4;
                    if held & 2 == 0 && quit_selected {
                        self.screen = Screen::Fade;
                        self.timer = 16;
                        self.ending = Some(Finish::Quit);
                        return None;
                    }
                    self.screen = Screen::Playing;
                    pressed &= !3;
                } else {
                    if pressed & 0x40 != 0 && quit_selected {
                        quit_selected = false;
                    } else if pressed & 0x80 != 0 && !quit_selected {
                        quit_selected = true;
                    }
                    self.screen = Screen::Pause { quit_selected };
                    return None;
                }
            }
            Screen::Playing => {
                self.menu_lock = self.menu_lock.saturating_sub(1);
                if self.menu_lock == 0 && pressed & 8 != 0 {
                    self.screen = Screen::Pause {
                        quit_selected: false,
                    };
                    self.menu_lock = 4;
                    return None;
                }
            }
        }
        let outcome = self.play.step_keys(&self.source, held, pressed);
        match outcome {
            PlayOutcome::Playing => {}
            PlayOutcome::Cleared if self.stage_number < 3 => {
                self.screen = Screen::StageClear;
                self.timer = 105;
            }
            _ => {
                let ending = if outcome == PlayOutcome::Lost {
                    Finish::Lost
                } else {
                    Finish::Cleared
                };
                self.ending = Some(ending);
                self.screen = Screen::Result { outcome: ending };
                self.timer = 1;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_wrong_cartridge_and_decoded_length() {
        assert!(BallSource::from_verified_payload(&[]).is_none());
        assert!(BallSource::from_verified_payload(&vec![0; 188716]).is_none());
    }

    #[test]
    fn simultaneous_keys_obey_cartridge_priority_and_reserved_lane() {
        let mut board = [[0; COLUMNS]; ROWS];
        let mut p = Launcher::default();
        let shot = p.control(0x30, 3, 0, 0, &board).unwrap();
        assert_eq!((shot.column, shot.color, p.phase), (6, 1, 4));
        board[7][5] = 0x10;
        let mut p = Launcher::default();
        assert_eq!(p.control(0x30, 3, 0, 0, &board), None);
        assert_eq!((p.column, p.x, p.timer), (5, 104, 8));
        for _ in 0..8 {
            p.control(0, 0, 0, 0, &board);
        }
        assert_eq!((p.column, p.x, p.phase), (5, 88, 0));
    }

    #[test]
    fn shots_hit_markers_as_well_as_colored_balls_and_ceiling() {
        let mut b = [[0; COLUMNS]; ROWS];
        let mut s = Shot {
            column: 1,
            color: 2,
            y: 28,
        };
        b[1][0] = 0x50;
        assert_eq!(
            s.step(&b, 0, 0),
            Some(ShotStep::Landed {
                row: 3,
                column: 1,
                color: 2
            })
        );
        let mut s = Shot {
            column: 10,
            color: 1,
            y: 5,
        };
        assert_eq!(s.step(&b, 0, 0x80), Some(ShotStep::Flying));
        assert_eq!(s.y, 5);
        assert_eq!(
            s.step(&b, 0, 0),
            Some(ShotStep::Landed {
                row: 1,
                column: 10,
                color: 1
            })
        );
    }

    #[test]
    fn ring_catch_window_has_strict_horizontal_edges() {
        for (x, y, flags, result) in [
            (88, 136, 0, RingStep::Falling),
            (89, 136, 0, RingStep::Caught),
            (111, 167, 0, RingStep::Caught),
            (112, 167, 0, RingStep::Falling),
            (100, 168, 0, RingStep::Missed),
            (100, 150, 64, RingStep::Falling),
        ] {
            let mut ring = FallingRing {
                x,
                y: y << 16,
                velocity: -32,
                delay: 0,
            };
            assert_eq!(ring.step(100, 0, flags), result);
        }
    }

    #[test]
    fn cartridge_wallet_cap_is_applied_after_unsigned_addition() {
        assert_eq!(cartridge_wallet(99998, 2), 99999);
        assert_eq!(cartridge_wallet(u32::MAX, 2), 1);
    }
}
