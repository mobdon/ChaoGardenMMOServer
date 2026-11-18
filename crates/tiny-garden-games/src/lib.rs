//! Pure host-authoritative minigames. Only sanitized views are serializable.
mod ball;
mod bounce;
mod memory;
mod rps;
mod search;

pub use ball::{BallColor, BallView};
pub use bounce::BounceView;
pub use memory::MemoryView;
pub use rps::{Hand, RpsView};
pub use search::{SearchConfig, SearchView};
use serde::{Deserialize, Serialize};

pub const STEP_MS: u32 = 20;
pub const MAX_ADVANCE_MS: u32 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TinyGame {
    Memory,
    RockPaperScissors,
    Bounce,
    Ball,
    Search,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(from = "WireTinyInput")]
pub enum TinyInput {
    Move { x: i8, y: i8 },
    MemoryFlip { cell: u8 },
    MemoryTrip,
    RpsSelect { slot: u8 },
    RpsShoot,
    BounceSpring,
    BallShoot { color: BallColor },
    SearchPickUp { chao: u8 },
    SearchReturn,
    SearchHint { orb: u8 },
}

// Empty struct variants reject surplus fields; serde's tagged *unit* variants
// otherwise accept fields such as a forged score despite deny_unknown_fields.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WireTinyInput {
    Move { x: i8, y: i8 },
    MemoryFlip { cell: u8 },
    MemoryTrip {},
    RpsSelect { slot: u8 },
    RpsShoot {},
    BounceSpring {},
    BallShoot { color: BallColor },
    SearchPickUp { chao: u8 },
    SearchReturn {},
    SearchHint { orb: u8 },
}
impl From<WireTinyInput> for TinyInput {
    fn from(value: WireTinyInput) -> Self {
        match value {
            WireTinyInput::Move { x, y } => Self::Move { x, y },
            WireTinyInput::MemoryFlip { cell } => Self::MemoryFlip { cell },
            WireTinyInput::MemoryTrip {} => Self::MemoryTrip,
            WireTinyInput::RpsSelect { slot } => Self::RpsSelect { slot },
            WireTinyInput::RpsShoot {} => Self::RpsShoot,
            WireTinyInput::BounceSpring {} => Self::BounceSpring,
            WireTinyInput::BallShoot { color } => Self::BallShoot { color },
            WireTinyInput::SearchPickUp { chao } => Self::SearchPickUp { chao },
            WireTinyInput::SearchReturn {} => Self::SearchReturn,
            WireTinyInput::SearchHint { orb } => Self::SearchHint { orb },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TinyError {
    Ended,
    WrongGame,
    InvalidInput,
    NotReady,
    Cooldown,
    TooFar,
    AlreadyClaimed,
    NotEnded,
    InvalidDelta,
}
impl std::fmt::Display for TinyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TinyError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TinyOutcome {
    pub game: TinyGame,
    pub rings: u32,
    pub reason: String,
    pub completed: bool,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "game", content = "state", rename_all = "snake_case")]
pub enum TinyView {
    Memory(MemoryView),
    RockPaperScissors(RpsView),
    Bounce(BounceView),
    Ball(BallView),
    Search(SearchView),
}

// Deliberately no Serialize/Deserialize or derived Debug: private boards, PRNG
// and target order cannot enter a response or a diagnostic log. Custom Debug
// below only prints the same sanitized view a player is allowed to receive.
#[derive(Clone)]
pub struct TinySession {
    game: Game,
    kind: TinyGame,
    elapsed_ms: u64,
    remainder_ms: u32,
    result: Option<TinyOutcome>,
    claimed: bool,
}
#[derive(Clone)]
enum Game {
    Memory(memory::Memory),
    Rps(rps::Rps),
    Bounce(bounce::Bounce),
    Ball(ball::Ball),
    Search(search::Search),
}
impl std::fmt::Debug for TinySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TinySession")
            .field("view", &self.view())
            .field("outcome", &self.result)
            .field("claimed", &self.claimed)
            .finish()
    }
}

impl TinySession {
    pub fn new(game: TinyGame, seed: u64) -> Self {
        Self::with_search_config(game, seed, SearchConfig::default())
    }
    pub fn with_search_config(kind: TinyGame, seed: u64, config: SearchConfig) -> Self {
        let mut rng = Rng::new(seed);
        let game = match kind {
            TinyGame::Memory => Game::Memory(memory::Memory::new(&mut rng)),
            TinyGame::RockPaperScissors => Game::Rps(rps::Rps::new(rng)),
            TinyGame::Bounce => Game::Bounce(bounce::Bounce::new(rng)),
            TinyGame::Ball => Game::Ball(ball::Ball::new(rng)),
            TinyGame::Search => Game::Search(search::Search::new(rng, config)),
        };
        Self {
            game,
            kind,
            elapsed_ms: 0,
            remainder_ms: 0,
            result: None,
            claimed: false,
        }
    }
    pub fn input(&mut self, input: TinyInput) -> Result<(), TinyError> {
        if self.result.is_some() {
            return Err(TinyError::Ended);
        }
        if let TinyInput::Move { x, y } = input
            && (!(-1..=1).contains(&x) || !(-1..=1).contains(&y))
        {
            return Err(TinyError::InvalidInput);
        }
        match &mut self.game {
            Game::Memory(game) => game.input(input),
            Game::Rps(game) => game.input(input),
            Game::Bounce(game) => game.input(input),
            Game::Ball(game) => game.input(input),
            Game::Search(game) => game.input(input),
        }?;
        self.capture_result();
        Ok(())
    }
    /// Call only with elapsed time measured by the trusted host. Never pass a
    /// client's timestamp/delta, even though the value is bounded here.
    pub fn advance(&mut self, dt_ms: u32) -> Result<(), TinyError> {
        if dt_ms > MAX_ADVANCE_MS {
            return Err(TinyError::InvalidDelta);
        }
        if self.result.is_some() {
            return Ok(());
        }
        self.remainder_ms += dt_ms;
        while self.remainder_ms >= STEP_MS && self.result.is_none() {
            self.remainder_ms -= STEP_MS;
            self.elapsed_ms += u64::from(STEP_MS);
            match &mut self.game {
                Game::Memory(game) => game.step(),
                Game::Rps(game) => game.step(),
                Game::Bounce(game) => game.step(),
                Game::Ball(game) => game.step(),
                Game::Search(game) => game.step(),
            }
            self.capture_result();
        }
        Ok(())
    }
    pub fn view(&self) -> TinyView {
        match &self.game {
            Game::Memory(game) => TinyView::Memory(game.view()),
            Game::Rps(game) => TinyView::RockPaperScissors(game.view()),
            Game::Bounce(game) => TinyView::Bounce(game.view()),
            Game::Ball(game) => TinyView::Ball(game.view()),
            Game::Search(game) => TinyView::Search(game.view()),
        }
    }
    pub fn outcome(&self) -> Option<&TinyOutcome> {
        self.result.as_ref()
    }
    /// This local latch is NOT a durable currency transaction. The owning
    /// service must journal a server-generated session ID and ring award once.
    pub fn claim_result(&mut self) -> Result<TinyOutcome, TinyError> {
        let result = self.result.as_ref().ok_or(TinyError::NotEnded)?;
        if self.claimed {
            return Err(TinyError::AlreadyClaimed);
        }
        self.claimed = true;
        Ok(result.clone())
    }
    fn capture_result(&mut self) {
        if self.result.is_some() {
            return;
        }
        let (rings, ending) = match &self.game {
            Game::Memory(game) => (game.rings, game.ending),
            Game::Rps(game) => (game.rings, game.ending),
            Game::Bounce(game) => (game.rings, game.ending),
            Game::Ball(game) => (game.rings, game.ending),
            Game::Search(game) => (game.rings, game.ending),
        };
        if let Some((completed, reason)) = ending {
            self.result = Some(TinyOutcome {
                game: self.kind,
                rings,
                reason: reason.into(),
                completed,
                elapsed_ms: self.elapsed_ms,
            });
        }
    }
}

#[derive(Clone, Copy)]
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x6a09_e667_f3bc_c909)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn index(&mut self, length: usize) -> usize {
        (self.next() % length as u64) as usize
    }
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.index(i + 1);
            items.swap(i, j);
        }
    }
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_forged_results_and_large_host_delta() {
        assert!(serde_json::from_str::<TinyInput>(r#"{"kind":"rps_shoot","score":999}"#).is_err());
        assert!(serde_json::from_str::<TinyInput>(r#"{"kind":"finish","rings":999}"#).is_err());
        let mut game = TinySession::new(TinyGame::Ball, 42);
        assert_eq!(
            game.input(TinyInput::Move { x: 2, y: 0 }),
            Err(TinyError::InvalidInput)
        );
        assert_eq!(game.advance(1001), Err(TinyError::InvalidDelta));
        assert_eq!(game.claim_result(), Err(TinyError::NotEnded));
    }
    #[test]
    fn fixed_steps_are_independent_of_host_tick_partition() {
        for kind in [
            TinyGame::Memory,
            TinyGame::RockPaperScissors,
            TinyGame::Bounce,
            TinyGame::Ball,
            TinyGame::Search,
        ] {
            let mut a = TinySession::new(kind, 16);
            let mut b = TinySession::new(kind, 16);
            for _ in 0..100 {
                a.advance(20).unwrap();
            }
            for _ in 0..10 {
                b.advance(200).unwrap();
            }
            assert_eq!(
                serde_json::to_value(a.view()).unwrap(),
                serde_json::to_value(b.view()).unwrap()
            );
        }
    }
    #[test]
    fn terminal_results_are_immutable_and_claimed_once() {
        let mut game = TinySession::new(TinyGame::RockPaperScissors, 1);
        for _ in 0..30 {
            game.advance(1000).unwrap();
        }
        let outcome = game.claim_result().unwrap();
        assert_eq!(outcome.reason, "time_up");
        assert_eq!(game.claim_result(), Err(TinyError::AlreadyClaimed));
        assert_eq!(game.input(TinyInput::RpsShoot), Err(TinyError::Ended));
        game.advance(1000).unwrap();
        assert_eq!(game.outcome(), Some(&outcome));
    }
}
