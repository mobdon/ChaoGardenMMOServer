//! Trusted button and time adapter around the separately verified cartridge rules.
//! Every projection is an allowlist; original buffers, future cells and RNG are private.
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, Read},
    path::Path,
};
use tiny_garden_core::{
    native_ball::{BallSession, BallSource, Finish, Screen},
    native_bios_math::ReferenceBiosMath,
    native_growth::NativeRng,
    native_memory::{A2neMemorySource, NativeButtons, NativeMemoryPhase, NativeMemoryRound},
    native_rps_finish::NativeRpsSession,
    native_rps_game::{ASOE_LENGTH, NativeRpsAssets},
};
use tiny_garden_games::{TinyGame, TinyOutcome};

const ADAPTED: &str = "adapted-v1";
const MEMORY: &str = "a2ne-memory-v1";
const RPS: &str = "asoe-rps-v1";
const BALL: &str = "a3ve-ball-v1";
const GBA_FRAMES_NUM: u64 = 262144;
const GBA_FRAMES_DEN: u64 = 4389;
const FRAME_COST: u64 = GBA_FRAMES_DEN * 1_000_000_000;
const HOLD_TIMEOUT_MS: f64 = 350.;
const MAX_QUEUE: usize = 8;

#[derive(Clone, Debug, Default)]
pub struct Sources {
    memory: Option<A2neMemorySource>,
    ball: Option<BallSource>,
    rps: Option<NativeRpsAssets>,
}

fn payload(path: &Path, expected: usize) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() != expected as u64 {
        return Err(io::Error::other("Unsupported native Tiny payload size"));
    }
    let mut bytes = Vec::with_capacity(expected);
    file.take(expected as u64 + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

impl Sources {
    pub(crate) fn load_memory(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.memory =
            Some(A2neMemorySource::from_verified_payload(bytes).map_err(io::Error::other)?);
        Ok(())
    }
    pub(crate) fn load_ball(&mut self, path: &Path) -> io::Result<()> {
        self.ball = Some(
            BallSource::from_verified_payload(&payload(path, 188_716)?)
                .ok_or_else(|| io::Error::other("Unsupported A3VE Tiny Ball payload"))?,
        );
        Ok(())
    }
    pub(crate) fn load_rps(&mut self, path: &Path) -> io::Result<()> {
        self.rps = Some(
            NativeRpsAssets::from_verified_payload(&payload(path, ASOE_LENGTH)?)
                .map_err(io::Error::other)?,
        );
        Ok(())
    }
    fn info(&self, id: &str) -> (&'static str, u32) {
        match id {
            "rock_paper_scissors" if self.rps.is_some() => (RPS, 99),
            "ball" if self.ball.is_some() => (BALL, 128),
            "memory" if self.memory.is_some() => (MEMORY, 99),
            _ => (ADAPTED, crate::phone::MAX_RING_REWARD),
        }
    }
    pub fn capabilities(&self) -> Value {
        Value::Object(
            crate::phone::GAME_IDS
                .into_iter()
                .map(|id| {
                    let (rules, max) = self.info(id);
                    (
                        id.to_owned(),
                        json!({"native":rules!=ADAPTED,"rulesVersion":rules,"maxRingReward":max}),
                    )
                })
                .collect(),
        )
    }
    pub fn rules(&self) -> &'static str {
        if crate::phone::GAME_IDS
            .iter()
            .any(|id| self.info(id).0 != ADAPTED)
        {
            "per-game"
        } else {
            "adapted"
        }
    }
    pub fn max_ring_reward(&self) -> u32 {
        crate::phone::GAME_IDS
            .iter()
            .map(|id| self.info(id).1)
            .max()
            .unwrap()
    }
    pub(crate) fn begin(&self, game: TinyGame, seed: u32) -> Result<Option<NativeRun>, String> {
        let engine = match game {
            TinyGame::Memory => {
                let Some(source) = self.memory else {
                    return Ok(None);
                };
                Engine::Memory(source.begin_round(seed).map_err(|e| e.to_string())?)
            }
            TinyGame::Ball => {
                let Some(source) = self.ball else {
                    return Ok(None);
                };
                Engine::Ball(BallSession::new(source, seed))
            }
            TinyGame::RockPaperScissors => {
                let Some(assets) = &self.rps else {
                    return Ok(None);
                };
                let mut rng = NativeRng::new(seed);
                let mut round = NativeRpsSession::default();
                round.initialize(&mut rng);
                Engine::Rps {
                    round,
                    rng,
                    assets: assets.clone(),
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(NativeRun {
            engine,
            queue: VecDeque::new(),
            held: 0,
            last_input: 0.,
            budget: 0,
            frames: 0,
            result: None,
            halted: false,
        }))
    }
}

#[derive(Clone, Debug)]
enum Engine {
    Ball(BallSession),
    Memory(NativeMemoryRound),
    Rps {
        round: NativeRpsSession,
        rng: NativeRng,
        assets: NativeRpsAssets,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct NativeRun {
    engine: Engine,
    queue: VecDeque<u16>,
    held: u16,
    last_input: f64,
    budget: u64,
    frames: u64,
    result: Option<TinyOutcome>,
    halted: bool,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Intent {
    NativeButtons { held: u16 },
}

impl NativeRun {
    pub(crate) fn rules(&self) -> &'static str {
        match self.engine {
            Engine::Ball(_) => BALL,
            Engine::Memory(_) => MEMORY,
            Engine::Rps { .. } => RPS,
        }
    }
    pub(crate) fn max_ring_reward(&self) -> u32 {
        match self.engine {
            Engine::Ball(_) => 128,
            Engine::Memory(_) | Engine::Rps { .. } => 99,
        }
    }
    pub(crate) fn buttons(&mut self, intent: &Value, now: f64) -> Result<(), ()> {
        let Intent::NativeButtons { held } =
            serde_json::from_value(intent.clone()).map_err(|_| ())?;
        let mask = match &self.engine {
            Engine::Ball(_) => 0xfb,
            // Only Ball implements the pause menu. Use A for result dismissal in all games.
            Engine::Memory(_) => 0xf3,
            Engine::Rps { .. } => 0x31,
        };
        if self.halted || held & !mask != 0 {
            return Err(());
        }
        if self.queue.back().copied().unwrap_or(self.held) != held {
            if self.queue.len() >= MAX_QUEUE {
                // A release always wins, even under a flood; never backlog old inputs.
                if held == 0 {
                    self.release();
                } else {
                    return Err(());
                }
            } else {
                self.queue.push_back(held);
            }
        }
        self.last_input = now;
        Ok(())
    }
    pub(crate) fn release(&mut self) {
        self.queue.clear();
        self.held = 0;
    }
    pub(crate) fn halted(&self) -> bool {
        self.halted
    }
    pub(crate) fn take_outcome(&mut self) -> Option<TinyOutcome> {
        self.result.take()
    }
    pub(crate) fn advance(&mut self, dt: f64, now: f64) {
        if self.halted {
            return;
        }
        if now - self.last_input > HOLD_TIMEOUT_MS {
            self.release();
        }
        self.budget += (dt * 1e9).round() as u64 * GBA_FRAMES_NUM;
        while self.budget >= FRAME_COST {
            self.budget -= FRAME_COST;
            self.frames += 1;
            let held = self.queue.pop_front().unwrap_or(self.held);
            // A2NE/A3VE 02000C28 and ASOE 02000BA8 write gInput+4 as
            // current held AND previous inverted keys. Repeat has its own +6 field.
            let pressed = held & !self.held;
            self.held = held;
            let result = match &mut self.engine {
                Engine::Rps { round, assets, rng } => {
                    if round
                        .tick(pressed, assets, &ReferenceBiosMath, rng)
                        .is_err()
                    {
                        self.halted = true;
                        break;
                    }
                    round.result.map(|rings| {
                        (
                            TinyGame::RockPaperScissors,
                            u16::from(rings),
                            if round.round.launcher.shots == 0 {
                                "shots_exhausted"
                            } else {
                                "time_up"
                            },
                            true,
                        )
                    })
                }
                Engine::Ball(round) => round.step(held).map(|(end, rings)| {
                    (TinyGame::Ball, rings, finish(end), end == Finish::Cleared)
                }),
                Engine::Memory(round) => {
                    if round.step(NativeButtons { held, pressed }).is_err() {
                        self.halted = true;
                        break;
                    }
                    round.finished().map(|end| {
                        (
                            TinyGame::Memory,
                            u16::from(end.rings),
                            if end.won { "cleared" } else { "lost" },
                            end.won,
                        )
                    })
                }
            };
            if let Some((game, rings, reason, completed)) = result {
                self.result = Some(TinyOutcome {
                    game,
                    rings: u32::from(rings),
                    reason: reason.into(),
                    completed,
                    elapsed_ms: self.frames * GBA_FRAMES_DEN * 1000 / GBA_FRAMES_NUM,
                });
                break;
            }
        }
    }
    pub(crate) fn view(&self) -> Value {
        if self.halted {
            return json!({"screen":"error"});
        }
        match &self.engine {
            Engine::Ball(round) => ball_view(round),
            Engine::Memory(round) => memory_view(round),
            Engine::Rps { round, .. } => rps_view(round),
        }
    }
}

fn finish(value: Finish) -> &'static str {
    match value {
        Finish::Quit => "quit",
        Finish::Lost => "lost",
        Finish::Cleared => "cleared",
    }
}
fn ball_view(round: &BallSession) -> Value {
    let p = &round.play;
    let player = &p.player;
    let (screen, quit, outcome) = match round.screen() {
        Screen::Intro => ("intro", None, None),
        Screen::Playing => ("playing", None, None),
        Screen::Pause { quit_selected } => ("pause", Some(quit_selected), None),
        Screen::StageClear => ("stageClear", None, None),
        Screen::NextStage => ("nextStage", None, None),
        Screen::Result { outcome } => ("result", None, Some(finish(outcome))),
        Screen::Fade => ("fade", None, None),
        Screen::Garden { outcome } => ("garden", None, Some(finish(outcome))),
    };
    json!({"screen":screen,"quitSelected":quit,"outcome":outcome,"stage":round.stage_number()+1,
        "board":p.stage.board,"scroll":p.stage.scroll,"remaining":p.stage.remaining,
        "player":{"x":player.x,"column":player.column,"phase":player.phase,"facingRight":player.facing_right,"animFrame":player.anim_frame},
        "shot":p.shot.map(|s|json!({"column":s.column,"color":s.color,"y":s.y})),
        "resolution":p.resolution.as_ref().map(|r|json!({"phase":r.phase,"timer":r.timer,"effects":r.effects})),
        "fallingRings":p.falling_rings().filter(|r|r.delay==0).map(|r|json!({"x":r.x,"y":r.y>>16})).collect::<Vec<_>>(),"rings":p.rings})
}
fn memory_view(round: &NativeMemoryRound) -> Value {
    let v = round.view();
    let phase = match v.phase {
        NativeMemoryPhase::Reveal => "reveal",
        NativeMemoryPhase::Shuffle => "shuffle",
        NativeMemoryPhase::Matching => "matching",
        NativeMemoryPhase::Result => "result",
        NativeMemoryPhase::Ended => "ended",
    };
    json!({"phase":phase,"columns":v.columns,"rows":v.rows,"cells":v.cells.map(|c|json!({"occupied":c.occupied,"face":c.face,"ringValue":c.ring_value,"flip":c.flip})),
        "cursor":v.cursor,"selected":v.selected.map(|c|c.map(|s|json!({"column":s.column,"row":s.row,"face":s.face,"flip":s.flip}))),
        "walker":v.walker.map(|w|json!({"x":w.x,"y":w.y,"direction":w.direction,"trip":w.trip,"movingPosition":w.moving_position})),
        "mistakes":v.mistakes,"matchedPairs":v.matched_pairs,"rings":v.rings})
}

fn rps_view(session: &NativeRpsSession) -> Value {
    let r = &session.round;
    let l = &r.launcher;
    let w = &r.waves;
    let screen = if session.result.is_some() {
        "ended"
    } else if r.finishing {
        "result"
    } else {
        "playing"
    };
    let targets: Vec<_> = w
        .targets
        .iter()
        .enumerate()
        .filter(|(_, t)| t.route() >= 0 && t.state() != 4)
        .map(|(index, t)| {
            let (x, y) = t.position();
            json!({"index":index,"state":t.state(),"gesture":t.gesture(),"x":x,"y":y})
        })
        .collect();
    let cards:Vec<_>=l.cards.iter().enumerate().map(|(lane,c)|{
        let (raw_x,y)=c.position();let x=if c.state()==5 {raw_x}else{12+64*lane as i16};
        json!({"lane":lane,"state":c.state(),"gesture":(c.state()<7).then(||c.gesture()),"x":x,"y":y})
    }).collect();
    json!({"screen":screen,"wave":u16::from(w.wave)+1,"rings":w.round_rings,"seconds":w.clock.seconds,
        "subframes":w.clock.subframes,"targets":targets,"cards":cards,"lane":l.lane,"markerX":l.marker_x,
        "shots":l.shots,"displayedShots":l.displayed_shots,"actor":{"x":r.actor.x,"y":47,"mode":r.actor.mode},
        "finishMode":session.finish.as_ref().map(|f|f.mode)})
}
