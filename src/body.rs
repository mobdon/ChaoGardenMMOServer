//! Chao body, evolution and appearance (`AL_BODY_INFO` in X-Hax SA2Structs.h).
//!
//! Field meanings, enum values and the 6-model-per-type `AL_RootObject` layout are
//! native (SA2Structs.h, SA2Enums.h, SA Tools `PC_SA2/Chao_DLL.ini`). Every rate and
//! threshold marked `TUNED` is our own MMO tuning: the native shape and evolution
//! formulas have not been traced yet, and the public view says so.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const BODY_VERSION: u32 = 1;

/// `ChaoType` from SA2Enums.h. Only values this module can produce are listed.
pub mod chao_type {
    pub const CHILD: u8 = 0x02;
    pub const NEUTRAL_NORMAL: u8 = 0x05;
    pub const NEUTRAL_CHAOS: u8 = 0x14;
    pub const HERO_CHAOS: u8 = 0x15;
    pub const DARK_CHAOS: u8 = 0x16;
}

/// TUNED: hero/dark class boundary on APos for evolution and child model family.
pub const ALIGN_CLASS: f32 = 0.6;
/// TUNED: adults stay Normal unless one shape axis has moved at least this far.
pub const NORMAL_LIMIT: f32 = 0.3;
/// TUNED: Aim axis movement per native Exp unit (100 Exp = one level, life.rs units).
pub const SHAPE_PER_EXP: f32 = 0.0005;
/// TUNED: visible axes ease toward their Aim targets at this many units per second.
pub const EASE_PER_SECOND: f32 = 0.01;
/// TUNED: seconds of connected play for growth (child or adult morph depth) to reach 1.
pub const GROWTH_SECONDS: f32 = 3.0 * 3600.0;
/// Native SA2BColour runs 0 (Normal) ..= 13 (Black); SA2BTexture 0 (None) ..= 16 (Moon).
pub const MAX_COLOR: u8 = 13;
pub const MAX_JEWEL: u8 = 16;
/// Native SA2BEggColour runs 0 ..= 0x36 in SA2Enums.h.
pub const MAX_EGG_COLOR: u8 = 0x36;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneLook {
    pub color: u8,
    pub jewel: u8,
    pub shiny: bool,
    pub monotone: bool,
    pub egg_color: u8,
}

impl GeneLook {
    pub fn valid(&self) -> bool {
        self.color <= MAX_COLOR && self.jewel <= MAX_JEWEL && self.egg_color <= MAX_EGG_COLOR
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BodyState {
    pub version: u32,
    /// Run (-1) : Power (+1).
    pub h_pos: f32,
    /// Swim (-1) : Fly (+1).
    pub v_pos: f32,
    /// Dark (-1) : Hero (+1). The server's percentage alignment is a projection of this.
    pub a_pos: f32,
    pub aim_h_pos: f32,
    pub aim_v_pos: f32,
    pub aim_a_pos: f32,
    /// 0..1 morph depth within the current stage.
    pub growth: f32,
    pub chao_type: u8,
    pub look: GeneLook,
}

impl Default for BodyState {
    fn default() -> Self {
        Self {
            version: BODY_VERSION,
            h_pos: 0.,
            v_pos: 0.,
            a_pos: 0.,
            aim_h_pos: 0.,
            aim_v_pos: 0.,
            aim_a_pos: 0.,
            growth: 0.,
            chao_type: chao_type::CHILD,
            look: GeneLook::default(),
        }
    }
}

fn axis(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1., 1.)
    } else {
        0.
    }
}

fn ease(current: f32, aim: f32, step: f32) -> f32 {
    if (aim - current).abs() <= step {
        aim
    } else {
        current + step * (aim - current).signum()
    }
}

impl BodyState {
    /// Restore a saved body, or migrate the pre-body profile (form + percent alignment).
    pub fn restore(saved: Option<&Value>, form: &str, alignment: f64) -> Result<Self, String> {
        if let Some(value) = saved.filter(|v| !v.is_null()) {
            let body: Self =
                serde_json::from_value(value.clone()).map_err(|e| format!("Invalid body: {e}"))?;
            if !body.valid() {
                return Err("Invalid body values".into());
            }
            return Ok(body);
        }
        let a = axis((alignment / 100.) as f32);
        let mut body = Self {
            a_pos: a,
            aim_a_pos: a,
            ..Self::default()
        };
        if form == "chaos" {
            body.chao_type = chaos_for(a);
            body.growth = 1.;
        }
        Ok(body)
    }

    pub fn valid(&self) -> bool {
        let axes = [
            self.h_pos,
            self.v_pos,
            self.a_pos,
            self.aim_h_pos,
            self.aim_v_pos,
            self.aim_a_pos,
        ];
        self.version == BODY_VERSION
            && axes
                .iter()
                .all(|v| v.is_finite() && (-1. ..=1.).contains(v))
            && self.growth.is_finite()
            && (0. ..=1.).contains(&self.growth)
            && (self.chao_type == chao_type::CHILD
                || (chao_type::NEUTRAL_NORMAL..=chao_type::DARK_CHAOS).contains(&self.chao_type))
            && self.look.valid()
    }

    pub fn is_child(&self) -> bool {
        self.chao_type == chao_type::CHILD
    }

    pub fn is_chaos(&self) -> bool {
        (chao_type::NEUTRAL_CHAOS..=chao_type::DARK_CHAOS).contains(&self.chao_type)
    }

    /// Percent alignment (-100..100) used by garden gates and the existing UI.
    pub fn alignment_percent(&self) -> f64 {
        (f64::from(self.a_pos) * 100.).round()
    }

    /// Alignment changes from garden choices apply immediately (existing gameplay contract).
    pub fn set_alignment_percent(&mut self, percent: f64) {
        let a = axis((percent / 100.) as f32);
        self.a_pos = a;
        self.aim_a_pos = a;
    }

    /// Advance the visible axes toward their Aim values and grow. `dt` is in seconds.
    pub fn tick(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        let step = EASE_PER_SECOND * dt;
        self.h_pos = ease(self.h_pos, self.aim_h_pos, step);
        self.v_pos = ease(self.v_pos, self.aim_v_pos, step);
        self.a_pos = ease(self.a_pos, self.aim_a_pos, step);
        if !self.is_chaos() {
            self.growth = (self.growth + dt / GROWTH_SECONDS).min(1.);
        }
    }

    pub fn class(&self) -> &'static str {
        if self.a_pos >= ALIGN_CLASS {
            "hero"
        } else if self.a_pos <= -ALIGN_CLASS {
            "dark"
        } else {
            "neutral"
        }
    }

    pub fn type_name(&self) -> String {
        match self.chao_type {
            chao_type::CHILD => "child".into(),
            chao_type::NEUTRAL_CHAOS => "neutral-chaos".into(),
            chao_type::HERO_CHAOS => "hero-chaos".into(),
            chao_type::DARK_CHAOS => "dark-chaos".into(),
            t => {
                let offset = t - chao_type::NEUTRAL_NORMAL;
                let align = ["neutral", "hero", "dark"][usize::from(offset % 3)];
                let family = ["normal", "swim", "fly", "run", "power"][usize::from(offset / 3)];
                format!("{align}-{family}")
            }
        }
    }

    /// First `AL_RootObject` index of this body's six native morph models
    /// (z, n, s, f, r, p) in SA Tools `PC_SA2/Chao_DLL.ini`.
    pub fn model_base_index(&self) -> u16 {
        match self.chao_type {
            chao_type::CHILD => match self.class() {
                "hero" => 6,
                "dark" => 12,
                _ => 0,
            },
            t => 18 + u16::from(t - chao_type::NEUTRAL_NORMAL) * 6,
        }
    }

    /// TUNED morph weights relative to the `z` model: `n` is reached as growth rises and
    /// the axes pull toward `s`/`f`/`r`/`p`. Native blend formula not yet traced.
    pub fn morph_weights(&self) -> [f32; 5] {
        if self.is_chaos() {
            return [0.; 5];
        }
        let g = self.growth;
        let s = (-self.v_pos).max(0.);
        let f = self.v_pos.max(0.);
        let r = (-self.h_pos).max(0.);
        let p = self.h_pos.max(0.);
        [g * (1. - s - f - r - p), g * s, g * f, g * r, g * p]
    }

    pub fn public(&self) -> Value {
        json!({"version":self.version,"type":self.chao_type,"typeName":self.type_name(),
            "stage":if self.is_child() {"child"} else {"adult"},"class":self.class(),
            "runPower":self.h_pos,"swimFly":self.v_pos,"heroDark":self.a_pos,
            "aim":{"runPower":self.aim_h_pos,"swimFly":self.aim_v_pos,"heroDark":self.aim_a_pos},
            "growth":self.growth,"modelBaseIndex":self.model_base_index(),
            "morph":{"targets":["n","s","f","r","p"],"weights":self.morph_weights()},
            "look":self.look,
            "rules":{"fields":"native AL_BODY_INFO","models":"native AL_RootObject layout",
                "evolution":"tuned-body-evolution-v1","morph":"tuned-body-morph-v1",
                "nativeFormulas":"unknown"}})
    }
}

fn chaos_for(a: f32) -> u8 {
    if a >= ALIGN_CLASS {
        chao_type::HERO_CHAOS
    } else if a <= -ALIGN_CLASS {
        chao_type::DARK_CHAOS
    } else {
        chao_type::NEUTRAL_CHAOS
    }
}

/// Stat gains reshape the body. `amount` is native Exp (100 = one level).
/// Swim/Fly/Run/Power only; other stats and negative amounts do not move the axes.
pub fn on_stat_progress(body: &mut BodyState, stat: &str, amount: i32) {
    let delta = SHAPE_PER_EXP * amount.clamp(0, 100_000) as f32;
    match stat {
        "swim" => apply_shape_delta(body, 0., -delta, 0.),
        "fly" => apply_shape_delta(body, 0., delta, 0.),
        "run" => apply_shape_delta(body, -delta, 0., 0.),
        "power" => apply_shape_delta(body, delta, 0., 0.),
        _ => {}
    }
}

/// Animal, drive and fruit influence, in native AL_BODY_INFO units.
pub fn apply_shape_delta(body: &mut BodyState, run_power: f32, swim_fly: f32, hero_dark: f32) {
    let finite = |v: f32| if v.is_finite() { v } else { 0. };
    body.aim_h_pos = axis(body.aim_h_pos + finite(run_power));
    body.aim_v_pos = axis(body.aim_v_pos + finite(swim_fly));
    body.aim_a_pos = axis(body.aim_a_pos + finite(hero_dark));
}

/// Cocoon finished: choose the adult type from the axes (TUNED rule), reset morph depth.
/// Chaos and existing adults are unchanged (this module has no second evolution yet).
pub fn evolve(body: &mut BodyState) -> u8 {
    if !body.is_child() {
        return body.chao_type;
    }
    body.h_pos = body.aim_h_pos;
    body.v_pos = body.aim_v_pos;
    body.a_pos = body.aim_a_pos;
    let align = match body.class() {
        "hero" => 1,
        "dark" => 2,
        _ => 0,
    };
    let (h, v) = (body.h_pos, body.v_pos);
    let family = if h.abs().max(v.abs()) < NORMAL_LIMIT {
        0
    } else if v.abs() >= h.abs() {
        if v < 0. { 1 } else { 2 }
    } else if h < 0. {
        3
    } else {
        4
    };
    body.chao_type = chao_type::NEUTRAL_NORMAL + family * 3 + align;
    body.growth = 0.;
    body.chao_type
}

/// Separate milestone/owner path for Chaos forms (CAT-GROW-05 gate stays custom).
pub fn become_chaos(body: &mut BodyState) -> u8 {
    body.chao_type = chaos_for(body.a_pos);
    body.growth = 1.;
    body.chao_type
}

/// Reincarnation: a fresh child body. Expressed look is kept (genes live in life.rs).
pub fn reincarnate(body: &mut BodyState) {
    *body = BodyState {
        look: body.look,
        ..BodyState::default()
    };
}

/// Hatch: express the gene look. Invalid values are rejected without change.
pub fn express(body: &mut BodyState, look: &GeneLook) -> bool {
    if !look.valid() {
        return false;
    }
    body.look = *look;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evolution_picks_family_and_alignment() {
        let mut b = BodyState::default();
        on_stat_progress(&mut b, "swim", 1200);
        apply_shape_delta(&mut b, 0., 0., 0.8);
        assert_eq!(evolve(&mut b), 0x09); // Hero Swim
        assert_eq!(b.type_name(), "hero-swim");
        assert_eq!(b.model_base_index(), 42); // al_hsz
        let mut b = BodyState::default();
        on_stat_progress(&mut b, "power", 800);
        on_stat_progress(&mut b, "run", 200);
        on_stat_progress(&mut b, "luck", 5000);
        on_stat_progress(&mut b, "swim", -5000);
        apply_shape_delta(&mut b, 0., 0., -1.);
        assert_eq!(evolve(&mut b), 0x13); // Dark Power
        assert_eq!(b.model_base_index(), 102); // al_dpz
        let mut b = BodyState::default();
        assert_eq!(evolve(&mut b), chao_type::NEUTRAL_NORMAL);
        assert_eq!(b.model_base_index(), 18); // al_nnz
        assert_eq!(evolve(&mut b), chao_type::NEUTRAL_NORMAL);
    }

    #[test]
    fn child_and_chaos_model_indices_follow_native_table() {
        let mut b = BodyState::default();
        assert_eq!(b.model_base_index(), 0);
        b.set_alignment_percent(100.);
        assert_eq!(b.model_base_index(), 6);
        b.set_alignment_percent(-100.);
        assert_eq!(b.model_base_index(), 12);
        assert_eq!(become_chaos(&mut b), chao_type::DARK_CHAOS);
        assert_eq!(b.model_base_index(), 120);
        assert_eq!(b.morph_weights(), [0.; 5]);
    }

    #[test]
    fn axes_ease_and_clamp_and_ignore_bad_input() {
        let mut b = BodyState::default();
        apply_shape_delta(&mut b, 5., f32::NAN, -0.5);
        assert_eq!((b.aim_h_pos, b.aim_v_pos, b.aim_a_pos), (1., 0., -0.5));
        b.tick(10.);
        assert!((b.h_pos - 0.1).abs() < 1e-6);
        b.tick(1000.);
        assert_eq!(b.h_pos, 1.);
        assert_eq!(b.a_pos, -0.5);
        b.tick(f32::INFINITY);
        assert!(b.valid());
        assert_eq!(b.growth, (1010. / GROWTH_SECONDS).min(1.));
    }

    #[test]
    fn morph_weights_reach_targets() {
        let mut b = BodyState {
            growth: 1.,
            h_pos: -1.,
            ..Default::default()
        };
        assert_eq!(b.morph_weights(), [0., 0., 0., 1., 0.]);
        b.h_pos = 0.;
        assert_eq!(b.morph_weights(), [1., 0., 0., 0., 0.]);
        b.growth = 0.;
        assert_eq!(b.morph_weights(), [0.; 5]);
    }

    #[test]
    fn restore_migrates_and_round_trips() {
        let b = BodyState::restore(None, "chaos", -100.).unwrap();
        assert_eq!(b.chao_type, chao_type::DARK_CHAOS);
        assert_eq!(b.alignment_percent(), -100.);
        let child = BodyState::restore(None, "hero", 70.).unwrap();
        assert!(child.is_child());
        assert_eq!(child.class(), "hero");
        let saved = serde_json::to_value(&b).unwrap();
        assert_eq!(BodyState::restore(Some(&saved), "normal", 0.).unwrap(), b);
        let mut bad = saved.clone();
        bad["hPos"] = json!(3.0);
        assert!(BodyState::restore(Some(&bad), "normal", 0.).is_err());
        let mut extra = saved;
        extra["unknown"] = json!(1);
        assert!(BodyState::restore(Some(&extra), "normal", 0.).is_err());
    }

    #[test]
    fn reincarnation_keeps_look_and_express_validates() {
        let mut b = BodyState::default();
        let look = GeneLook {
            color: 9,
            jewel: 0,
            shiny: true,
            monotone: true,
            egg_color: 0x24,
        };
        assert!(express(&mut b, &look));
        assert!(!express(&mut b, &GeneLook { jewel: 17, ..look }));
        assert_eq!(b.look, look);
        apply_shape_delta(&mut b, 1., 1., 1.);
        evolve(&mut b);
        reincarnate(&mut b);
        assert!(b.is_child());
        assert_eq!(b.aim_h_pos, 0.);
        assert_eq!(b.look, look);
    }
}
