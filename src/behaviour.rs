//! Server-owned Chao behaviour selection for opted-in garden residents.
//!
//! This is **placeholder tuning** until native values replace it, not a port of the retail behaviour
//! controllers. The native controllers and activity timers are mapped in the
//! research notes (client `docs/MECHANICS.md`, CAT-LIFE-03) but their priority
//! tables and timers are not yet decoded. Every weight and duration below is
//! a placeholder, published with `rules:"placeholder-tuning"`.
//!
//! The server picks a behaviour kind, a variant and a phase. Clients map the
//! kind and variant onto native Chao motion clips and faces. The server never
//! names clips, so all observers see the same behaviour and the renderer can
//! never grant movement.
use serde_json::{Value, json};

/// Behaviour kinds. `legacy()` keeps the existing `watch.activity` values
/// (`idle`, `wander`, `rest`) so older clients keep working.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Standing still, occasionally looking around.
    Idle,
    /// Walking to a terrain-checked target.
    Wander,
    /// Sitting down for a while.
    Sit,
    /// Lying down and sleeping.
    Sleep,
    /// Playing on the spot (dance, clap, spin and similar solo play).
    Play,
    /// Short happy reaction after being petted.
    Happy,
    /// Short startled reaction after being woken.
    Startled,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Idle => "idle",
            Kind::Wander => "wander",
            Kind::Sit => "sit",
            Kind::Sleep => "sleep",
            Kind::Play => "play",
            Kind::Happy => "happy",
            Kind::Startled => "startled",
        }
    }
    /// The coarse activity that clients before this change understand.
    pub fn legacy(self) -> &'static str {
        match self {
            Kind::Wander => "wander",
            Kind::Sit | Kind::Sleep => "rest",
            _ => "idle",
        }
    }
    /// Number of distinct presentation variants the server chooses between.
    /// Clients map `variant % available` onto their native clip list.
    pub fn variants(self) -> u8 {
        match self {
            Kind::Idle => 3,
            Kind::Play => 4,
            Kind::Sit => 2,
            _ => 1,
        }
    }
    /// Kinds with a lead-in and lead-out phase (sit down/stand up, lie
    /// down/wake up). Others only use `loop`.
    fn phased(self) -> bool {
        matches!(self, Kind::Sit | Kind::Sleep)
    }
}

/// Phase of a phased behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Enter,
    Loop,
    Exit,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Enter => "enter",
            Phase::Loop => "loop",
            Phase::Exit => "exit",
        }
    }
}

/// PLACEHOLDER tuning (milliseconds), not native timers. Replace with decoded native values.
pub mod tuning {
    pub const IDLE_MS: f64 = 1800.;
    pub const WANDER_MS: f64 = 5500.;
    pub const SIT_MS: f64 = 6000.;
    pub const SLEEP_MS: f64 = 16000.;
    pub const PLAY_MS: f64 = 4200.;
    pub const HAPPY_MS: f64 = 1800.;
    pub const STARTLED_MS: f64 = 1400.;
    /// Lead-in/lead-out for sit and sleep.
    pub const TRANSITION_MS: f64 = 1200.;
    /// Below this energy (0–100 prototype scale) a rest choice becomes sleep.
    pub const SLEEPY_ENERGY: f64 = 35.;
}

/// Read-only view of the needs the picker consults, in 0..=1 where 1 means
/// "strong need". Filled from the prototype `Player` fields until the life
/// cycle module's `life::needs_view` lands; then this adapter switches to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeedsView {
    pub sleepiness: f64,
    pub boredom: f64,
    pub joy: f64,
}

impl NeedsView {
    pub const NEUTRAL: NeedsView = NeedsView {
        sleepiness: 0.3,
        boredom: 0.3,
        joy: 0.5,
    };
    /// Adapter over the existing prototype fields (energy and happiness on a
    /// 0–100 scale). Non-finite values fall back to neutral.
    pub fn from_prototype(energy: f64, happiness: f64) -> NeedsView {
        let unit = |v: f64, fallback: f64| {
            if v.is_finite() {
                (v / 100.).clamp(0., 1.)
            } else {
                fallback
            }
        };
        let energy = unit(energy, 1. - Self::NEUTRAL.sleepiness);
        let joy = unit(happiness, Self::NEUTRAL.joy);
        NeedsView {
            sleepiness: 1. - energy,
            // No native boredom yet: tie it loosely to low joy.
            boredom: (1. - joy) * 0.6 + 0.2,
            joy,
        }
    }
}

/// Live behaviour of one resident.
#[derive(Clone, Debug, PartialEq)]
pub struct Behaviour {
    pub kind: Kind,
    pub variant: u8,
    pub phase: Phase,
    pub started_at: f64,
    pub until: f64,
    pub phase_started_at: f64,
    pub phase_until: f64,
}

impl Behaviour {
    pub fn new(kind: Kind, variant: u8, now: f64, duration: f64) -> Behaviour {
        let variant = variant % kind.variants().max(1);
        let until = now + duration;
        let (phase, phase_until) = if kind.phased() && duration > 3. * tuning::TRANSITION_MS {
            (Phase::Enter, now + tuning::TRANSITION_MS)
        } else {
            (Phase::Loop, until)
        };
        Behaviour {
            kind,
            variant,
            phase,
            started_at: now,
            until,
            phase_started_at: now,
            phase_until,
        }
    }
    /// Advance enter → loop → exit by time. Returns true when the behaviour
    /// has finished.
    pub fn advance(&mut self, now: f64) -> bool {
        if now >= self.until {
            return true;
        }
        if now < self.phase_until {
            return false;
        }
        match self.phase {
            Phase::Enter => {
                self.phase = Phase::Loop;
                self.phase_started_at = self.phase_until;
                self.phase_until = if self.kind.phased() {
                    self.until - tuning::TRANSITION_MS
                } else {
                    self.until
                };
            }
            Phase::Loop if self.kind.phased() => {
                self.phase = Phase::Exit;
                self.phase_started_at = self.phase_until;
                self.phase_until = self.until;
            }
            _ => {}
        }
        false
    }
    /// End a phased behaviour early through its exit phase (for example a
    /// wake request), so the stand-up motion still plays.
    pub fn exit_now(&mut self, now: f64) {
        if self.kind.phased() && self.phase != Phase::Exit {
            self.phase = Phase::Exit;
            self.phase_started_at = now;
            self.phase_until = now + tuning::TRANSITION_MS;
            self.until = self.phase_until;
        } else {
            self.until = now;
        }
    }
    pub fn public(&self) -> Value {
        json!({"kind":self.kind.name(),"variant":self.variant,"phase":self.phase.name(),
            "startedAt":self.started_at,"until":self.until,
            "phaseStartedAt":self.phase_started_at,"phaseUntil":self.phase_until,
            "rules":"placeholder-tuning"})
    }
}

/// Deterministic 64-bit mixer (SplitMix64 finaliser) so a resident's choices
/// depend only on its session salt and decision count.
pub fn mix(salt: u64, decision: u32) -> u64 {
    let mut z = salt ^ u64::from(decision).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn unit(roll: u64) -> f64 {
    (roll >> 11) as f64 / (1u64 << 53) as f64
}

/// What the picker wants next after an idle beat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Wander,
    Rest(Kind),
    Play,
    Idle,
}

/// Choose the next behaviour after an idle beat.
///
/// PLACEHOLDER tuning: decisions cycle in threes so every resident visibly rests,
/// walks and has one free choice within a few seconds. The rest slot becomes
/// sleep when sleepy, the free slot leans toward play when bored or happy.
pub fn choose(needs: NeedsView, salt: u64, decision: u32) -> (Choice, u8) {
    let roll = mix(salt, decision);
    let variant = (roll >> 56) as u8;
    let r = unit(roll);
    let choice = match decision % 3 {
        0 => {
            let sleep_chance =
                (needs.sleepiness - tuning::SLEEPY_ENERGY / 100.).max(0.) * 1.6 + 0.1;
            if r < sleep_chance.min(0.9) {
                Choice::Rest(Kind::Sleep)
            } else {
                Choice::Rest(Kind::Sit)
            }
        }
        1 => Choice::Wander,
        _ => {
            let play = 0.25 + needs.boredom * 0.3 + needs.joy * 0.2 - needs.sleepiness * 0.3;
            if r < play.clamp(0.05, 0.8) {
                Choice::Play
            } else if r < play.clamp(0.05, 0.8) + 0.15 {
                Choice::Idle
            } else {
                Choice::Wander
            }
        }
    };
    (choice, variant)
}

pub fn duration(kind: Kind) -> f64 {
    match kind {
        Kind::Idle => tuning::IDLE_MS,
        Kind::Wander => tuning::WANDER_MS,
        Kind::Sit => tuning::SIT_MS,
        Kind::Sleep => tuning::SLEEP_MS,
        Kind::Play => tuning::PLAY_MS,
        Kind::Happy => tuning::HAPPY_MS,
        Kind::Startled => tuning::STARTLED_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phased_behaviour_runs_enter_loop_exit_in_order() {
        let mut b = Behaviour::new(Kind::Sleep, 0, 0., tuning::SLEEP_MS);
        assert_eq!(b.phase, Phase::Enter);
        assert!(!b.advance(tuning::TRANSITION_MS));
        assert_eq!(b.phase, Phase::Loop);
        assert!(!b.advance(tuning::SLEEP_MS - tuning::TRANSITION_MS));
        assert_eq!(b.phase, Phase::Exit);
        assert!(b.advance(tuning::SLEEP_MS));
    }

    #[test]
    fn early_exit_keeps_stand_up_phase() {
        let mut b = Behaviour::new(Kind::Sit, 1, 0., tuning::SIT_MS);
        b.advance(2000.);
        b.exit_now(2000.);
        assert_eq!(b.phase, Phase::Exit);
        assert_eq!(b.until, 2000. + tuning::TRANSITION_MS);
        let mut idle = Behaviour::new(Kind::Idle, 0, 0., tuning::IDLE_MS);
        idle.exit_now(10.);
        assert!(idle.advance(10.));
    }

    #[test]
    fn choices_are_deterministic_and_need_driven() {
        assert_eq!(
            choose(NeedsView::NEUTRAL, 7, 4),
            choose(NeedsView::NEUTRAL, 7, 4)
        );
        let sleepy = NeedsView::from_prototype(0., 60.);
        let rested = NeedsView::from_prototype(100., 60.);
        let count = |needs: NeedsView, want: Choice| {
            (0..3000u32)
                .filter(|d| d % 3 == 0 && choose(needs, 99, *d).0 == want)
                .count()
        };
        assert!(count(sleepy, Choice::Rest(Kind::Sleep)) > 700);
        assert!(count(rested, Choice::Rest(Kind::Sleep)) < 150);
        let bored = NeedsView {
            sleepiness: 0.,
            boredom: 1.,
            joy: 1.,
        };
        let tired = NeedsView {
            sleepiness: 1.,
            boredom: 0.,
            joy: 0.,
        };
        let plays = |needs| {
            (0..3000u32)
                .filter(|d| d % 3 == 2 && choose(needs, 5, *d).0 == Choice::Play)
                .count()
        };
        assert!(plays(bored) > plays(tired) * 3);
    }

    #[test]
    fn variants_stay_in_range_and_legacy_names_are_stable() {
        for decision in 0..200 {
            let (_, v) = choose(NeedsView::NEUTRAL, 3, decision);
            let b = Behaviour::new(Kind::Play, v, 0., tuning::PLAY_MS);
            assert!(b.variant < Kind::Play.variants());
        }
        assert_eq!(Kind::Sleep.legacy(), "rest");
        assert_eq!(Kind::Sit.legacy(), "rest");
        assert_eq!(Kind::Play.legacy(), "idle");
        assert_eq!(Kind::Wander.legacy(), "wander");
        assert!(
            NeedsView::from_prototype(f64::NAN, f64::INFINITY)
                .sleepiness
                .is_finite()
        );
    }
}
