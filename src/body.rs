//! Chao body, evolution and appearance (`AL_BODY_INFO` in X-Hax SA2Structs.h).
//!
//! Ported from SA2B GameCube `ChaoMain.rel` (GSNE8P, SHA-1 e7e24b8b…, decomp
//! symbols). Addresses are ChaoMain.rel section offsets, traced from the owner's
//! own disc. Anything the trace did not establish is left out and reported as
//! unresolved in [`BodyState::public`]; no value here is MMO tuning.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const BODY_VERSION: u32 = 2;

/// `ChaoType` from SA2Enums.h.
pub mod chao_type {
    pub const CHILD: u8 = 0x02;
    pub const NEUTRAL_NORMAL: u8 = 0x05;
    pub const NEUTRAL_SWIM: u8 = 0x08;
    pub const NEUTRAL_FLY: u8 = 0x0B;
    pub const NEUTRAL_RUN: u8 = 0x0E;
    pub const NEUTRAL_POWER: u8 = 0x11;
    pub const NEUTRAL_CHAOS: u8 = 0x14;
    pub const HERO_CHAOS: u8 = 0x15;
    pub const DARK_CHAOS: u8 = 0x16;
}

/// ChaoGlobal: DeformLimit (AddHPos/AddVPos clamp).
pub const DEFORM_LIMIT: f32 = 1.0;
/// ChaoGlobal: GrowthLimit (AddGrowth clamp upper bound).
pub const GROWTH_LIMIT: f32 = 1.2;
/// ChaoGlobal: GrowthMayuPoint (child cocoon threshold, fn_60_1531C).
pub const GROWTH_MAYU_POINT: f32 = 1.0;
/// ChaoGlobal: NormalBoundary / NeutralBoundary (fn_60_3B5C0, strict comparisons).
pub const NORMAL_BOUNDARY: f32 = 0.5;
pub const NEUTRAL_BOUNDARY: f32 = 0.5;
/// AL_CalcParameter (0x213CC) growth per 120-call step.
pub const CHILD_TIME_GROWTH: f32 = 0.016_666_7;
pub const ADULT_TIME_GROWTH: f32 = 0.004_166_67;
/// ChaoTimerBaseInterval (3) x nbMaxEntry[0] (32 Chao slots, rodata 0x81E0).
pub const FRAMES_PER_PARAMETER_CALL: f64 = 96.;
/// NTSC game logic rate (ALW_Control runs once per frame).
pub const GAME_FRAMES_PER_SECOND: f64 = 60.;
/// ChaoTimerLifeInterval: AL_CalcParameter runs its growth step once every this many calls.
pub const PARAMETER_STEP_CALLS: u32 = 120;
/// ALBHV_Capture (0x33A2C) MinimalAddForm: child / adult.
pub const CAPTURE_FORM_CHILD: f32 = 0.1;
pub const CAPTURE_FORM_ADULT: f32 = 0.05;
/// Fruit growth: child / adult.
pub const FRUIT_GROWTH_CHILD: f32 = 0.005;
pub const FRUIT_GROWTH_ADULT: f32 = 0.001_25;
/// ALO_FruitHeldC Hero/Dark Fruit alignment change.
pub const FRUIT_ALIGNMENT: f32 = 0.15;
/// AL_Deform: a zero |HPos| or |VPos| is replaced by this before the blend.
pub const DEFORM_EPSILON: f32 = 1e-6;
/// Native SA2BColour runs 0..=13; SA2BTexture 0..=16; SA2BEggColour 0..=0x36.
pub const MAX_COLOR: u8 = 27;
pub const MAX_JEWEL: u8 = 16;
pub const MAX_EGG_COLOR: u8 = 0x36;

/// Expressed appearance alleles (genes live in life.rs). ColorNum indexes the
/// 28-entry native colour table (rodata 0x1D60), not the PC SA2BColour enum.
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

/// Native Chaos inputs read at the cocoon decision (fn_60_3B5C0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChaosInputs {
    /// All 21 animal species flags set (animals thread, PartsBTL MinimalFlag).
    pub all_animals: bool,
    /// UserLike toward the owner (life thread, AL_KNOWLEDGE).
    pub user_like: i32,
    /// CHAO_PARAM_GC byte at +142 (life thread maps the field).
    pub param_142: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BodyState {
    pub version: u32,
    /// Run (-) : Power (+).
    pub h_pos: f32,
    /// Swim (-) : Fly (+).
    pub v_pos: f32,
    /// Dark (-) : Hero (+). The server's percentage alignment is a projection of this.
    pub a_pos: f32,
    /// Native: written only by AL_SucceedGeneParam; never eased toward.
    pub aim_h_pos: f32,
    pub aim_v_pos: f32,
    pub aim_a_pos: f32,
    /// 0..=GrowthLimit.
    pub growth: f32,
    pub chao_type: u8,
    pub look: GeneLook,
    /// AL_CalcParameter call counter toward the next 120-call step.
    pub parameter_calls: u32,
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
            parameter_calls: 0,
        }
    }
}

fn clamp(value: f32, limit: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-limit, limit)
    } else {
        0.
    }
}

impl BodyState {
    /// Restore a saved body, or migrate the pre-body profile (form + percent alignment).
    /// A version-1 body (tuned prototype) migrates to a child keeping APos and look.
    pub fn restore(saved: Option<&Value>, form: &str, alignment: f64) -> Result<Self, String> {
        if let Some(value) = saved.filter(|v| !v.is_null()) {
            if value["version"] == 1 {
                let a = clamp(value["aPos"].as_f64().unwrap_or(0.) as f32, 1.);
                let look = serde_json::from_value(value["look"].clone()).unwrap_or_default();
                return Ok(Self {
                    a_pos: a,
                    look,
                    ..Self::migrated(form)
                });
            }
            let body: Self =
                serde_json::from_value(value.clone()).map_err(|e| format!("Invalid body: {e}"))?;
            if !body.valid() {
                return Err("Invalid body values".into());
            }
            return Ok(body);
        }
        Ok(Self {
            a_pos: clamp((alignment / 100.) as f32, 1.),
            ..Self::migrated(form)
        })
    }

    fn migrated(form: &str) -> Self {
        let mut body = Self::default();
        if form == "chaos" {
            body.chao_type = chao_type::NEUTRAL_CHAOS;
        }
        body
    }

    pub fn valid(&self) -> bool {
        let axes = [self.h_pos, self.v_pos, self.aim_h_pos, self.aim_v_pos];
        let aligns = [self.a_pos, self.aim_a_pos];
        self.version == BODY_VERSION
            && axes
                .iter()
                .all(|v| v.is_finite() && v.abs() <= DEFORM_LIMIT)
            && aligns.iter().all(|v| v.is_finite() && v.abs() <= 1.)
            && self.growth.is_finite()
            && (0. ..=GROWTH_LIMIT).contains(&self.growth)
            && (self.chao_type == chao_type::CHILD
                || (chao_type::NEUTRAL_NORMAL..=chao_type::DARK_CHAOS).contains(&self.chao_type))
            && self.parameter_calls < PARAMETER_STEP_CALLS
            && self.look.valid()
    }

    pub fn is_child(&self) -> bool {
        self.chao_type == chao_type::CHILD
    }

    pub fn is_chaos(&self) -> bool {
        (chao_type::NEUTRAL_CHAOS..=chao_type::DARK_CHAOS).contains(&self.chao_type)
    }

    /// Chaos types keep their alignment in the type; others follow APos.
    pub fn chaos_alignment(&self) -> Option<f64> {
        match self.chao_type {
            chao_type::HERO_CHAOS => Some(100.),
            chao_type::DARK_CHAOS => Some(-100.),
            _ => None,
        }
    }

    /// Percent alignment (-100..100) used by garden gates and the existing UI.
    pub fn alignment_percent(&self) -> f64 {
        (f64::from(self.a_pos) * 100.).round()
    }

    /// Existing garden choices (an MMO action, not a native event) set APos directly.
    pub fn set_alignment_percent(&mut self, percent: f64) {
        self.a_pos = clamp((percent / 100.) as f32, 1.);
    }

    /// AddHPos / AddVPos / AddAPos / AddGrowth with the native clamps.
    pub fn add(&mut self, h: f32, v: f32, a: f32, growth: f32) {
        let finite = |x: f32| if x.is_finite() { x } else { 0. };
        self.h_pos = clamp(self.h_pos + finite(h), DEFORM_LIMIT);
        self.v_pos = clamp(self.v_pos + finite(v), DEFORM_LIMIT);
        self.a_pos = clamp(self.a_pos + finite(a), 1.);
        self.growth = (self.growth + finite(growth)).clamp(0., GROWTH_LIMIT);
    }

    /// One AL_CalcParameter call. Every 120th call adds the time growth
    /// (child 1/60, adult 1/240). The garden-stage alignment drift is unresolved
    /// (its sign source was not traced) and is not applied.
    pub fn parameter_call(&mut self) {
        self.parameter_calls += 1;
        if self.parameter_calls < PARAMETER_STEP_CALLS {
            return;
        }
        self.parameter_calls = 0;
        if self.is_chaos() {
            return;
        }
        let growth = if self.is_child() {
            CHILD_TIME_GROWTH
        } else {
            ADULT_TIME_GROWTH
        };
        self.add(0., 0., 0., growth);
    }

    /// fn_60_1531C: a child at GrowthMayuPoint enters the cocoon (ALBHV_Mayu).
    /// The native call also requires the Chao to stand still on the ground; the
    /// life cycle thread checks that before starting the cocoon.
    pub fn cocoon_due(&self) -> bool {
        self.is_child() && self.growth >= GROWTH_MAYU_POINT
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

    /// First `AL_RootObject` index of the six native models (z, n, s, f, r, p).
    /// Children blend the neutral set (0) with the hero (6) or dark (12) set.
    pub fn model_base_index(&self) -> u16 {
        match self.chao_type {
            chao_type::CHILD => 0,
            chao_type::NEUTRAL_CHAOS => 108,
            chao_type::HERO_CHAOS => 114,
            chao_type::DARK_CHAOS => 120,
            t => 18 + u16::from(t - chao_type::NEUTRAL_NORMAL) * 6,
        }
    }

    /// AL_Deform weights as glTF morph weights on the z model.
    ///
    /// Native per vertex: `z + g*((v/(h+v))*lerp(n,V,v) + (h/(h+v))*lerp(n,H,h) - z)`,
    /// H = p if HPos >= 0 else r, V = f if VPos >= 0 else s, zeros -> 1e-6, growth
    /// unclamped up to GrowthLimit (AL_DeformObject 0x70D4). Children run the same
    /// blend on the neutral and on the hero (APos >= 0) or dark set, then
    /// `lerp(neutral, aligned, |APos|)` (AL_DeformObjectChild 0x6328).
    /// Chaos and special models have no morph set.
    pub fn morph(&self) -> (Vec<&'static str>, Vec<f32>) {
        if self.is_chaos() {
            return (vec![], vec![]);
        }
        let h = self.h_pos.abs().max(DEFORM_EPSILON);
        let v = self.v_pos.abs().max(DEFORM_EPSILON);
        let g = self.growth;
        let c_n = (v * (1. - v) + h * (1. - h)) / (h + v);
        let c_v = v * v / (h + v);
        let c_h = h * h / (h + v);
        let v_model = if self.v_pos >= 0. { 1 } else { 0 }; // f : s
        let h_model = if self.h_pos >= 0. { 1 } else { 0 }; // p : r
        let mut set = [0.; 5]; // n, s, f, r, p
        set[0] = g * c_n;
        set[1 + v_model] = g * c_v;
        set[3 + h_model] = g * c_h;
        if !self.is_child() {
            return (vec!["n", "s", "f", "r", "p"], set.to_vec());
        }
        let a = self.a_pos.abs();
        let names = vec![
            "n", "s", "f", "r", "p", "hz", "hn", "hs", "hf", "hr", "hp", "dz", "dn", "ds", "df",
            "dr", "dp",
        ];
        let mut weights = vec![0.; 17];
        for i in 0..5 {
            weights[i] = (1. - a) * set[i];
        }
        let aligned = if self.a_pos >= 0. { 5 } else { 11 };
        // Σc = 1, so the aligned z model keeps a*(1-g) and its targets a*g*c.
        weights[aligned] = a * (1. - g);
        for i in 0..5 {
            weights[aligned + 1 + i] = a * set[i];
        }
        (names, weights)
    }

    pub fn public(&self) -> Value {
        let (targets, weights) = self.morph();
        json!({"version":self.version,"type":self.chao_type,"typeName":self.type_name(),
            "stage":if self.is_child() {"child"} else {"adult"},
            "runPower":self.h_pos,"swimFly":self.v_pos,"heroDark":self.a_pos,
            "growth":self.growth,"cocoonDue":self.cocoon_due(),
            "modelBaseIndex":self.model_base_index(),
            "morph":{"targets":targets,"weights":weights},
            "look":self.look,
            "rules":{"source":"SA2B GC ChaoMain.rel GSNE8P",
                "unresolved":["garden-stage alignment drift sign","capture alignment sign",
                    "capture species 12-20 random pick","jewel, shiny and monotone drawing",
                    "60 Hz game rate assumed for ALW_Control"]}})
    }
}

/// ALBHV_Capture (0x33A2C) shape change for a captured animal or drive.
/// Kinds 0-11 and 21-24 are traced; kinds 12-20 use a random pick and the
/// alignment change uses a per-character table, both unresolved, so they are
/// not applied. Returns whether a traced shape change was applied.
pub fn apply_capture(body: &mut BodyState, kind: u8) -> bool {
    if body.is_chaos() {
        return false;
    }
    let f = if body.is_child() {
        CAPTURE_FORM_CHILD
    } else {
        CAPTURE_FORM_ADULT
    };
    let (h, v) = match kind {
        0..=2 => (0., -f),
        3..=5 => (-f, 0.),
        6..=8 => (f, 0.),
        9..=11 => (0., f),
        21 => (0., -f / 2.),
        22 => (-f / 2., 0.),
        23 => (f / 2., 0.),
        24 => (0., f / 2.),
        _ => return false,
    };
    body.add(h, v, 0., 0.);
    true
}

/// ALO_FruitHeldC (0x2B6A4): Hero Fruit (kind 5) adds +0.15 to APos, Dark Fruit
/// (kind 6) -0.15. Growth: AL_ParameterGrow (0x21148) reads FruitGrowParam by kind;
/// the traced child/adult growth applies until the per-kind table is reconciled.
/// Stats and needs are applied by life.rs.
pub fn apply_fruit(body: &mut BodyState, kind: u8) {
    let alignment = match kind {
        5 => FRUIT_ALIGNMENT,
        6 => -FRUIT_ALIGNMENT,
        _ => 0.,
    };
    if body.is_chaos() {
        body.add(0., 0., alignment, 0.);
        return;
    }
    let growth = if body.is_child() {
        FRUIT_GROWTH_CHILD
    } else {
        FRUIT_GROWTH_ADULT
    };
    body.add(0., 0., alignment, growth);
}

/// fn_60_3B5C0: cocoon type decision. Resets growth, HPos and VPos; keeps APos.
/// Only children evolve here.
pub fn evolve(body: &mut BodyState, chaos: ChaosInputs) -> u8 {
    if !body.is_child() {
        return body.chao_type;
    }
    let (h, v, a) = (body.h_pos, body.v_pos, body.a_pos);
    let family = if h.abs() < NORMAL_BOUNDARY && v.abs() < NORMAL_BOUNDARY {
        if chaos.all_animals && chaos.user_like > 80 && chaos.param_142 >= 2 {
            chao_type::NEUTRAL_CHAOS
        } else {
            chao_type::NEUTRAL_NORMAL
        }
    } else if v > h {
        if v > -h {
            chao_type::NEUTRAL_FLY
        } else {
            chao_type::NEUTRAL_RUN
        }
    } else if v > -h {
        chao_type::NEUTRAL_POWER
    } else {
        chao_type::NEUTRAL_SWIM
    };
    let align = if a.abs() < NEUTRAL_BOUNDARY {
        0
    } else if a > 0. {
        1
    } else {
        2
    };
    body.chao_type = family + align;
    body.growth = 0.;
    body.h_pos = 0.;
    body.v_pos = 0.;
    body.chao_type
}

/// The existing owner/milestone Chaos path (server MMO gate, not a native rule)
/// keeps alignment in the native Chaos type.
pub fn become_chaos(body: &mut BodyState) -> u8 {
    body.chao_type = if body.a_pos.abs() < NEUTRAL_BOUNDARY {
        chao_type::NEUTRAL_CHAOS
    } else if body.a_pos > 0. {
        chao_type::HERO_CHAOS
    } else {
        chao_type::DARK_CHAOS
    };
    body.chao_type
}

/// Reincarnation: a fresh child body. Expressed look is kept (genes live in life.rs).
/// AL_SucceedGeneParam's Aim* carry-over is not ported yet.
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

    fn child(h: f32, v: f32, a: f32) -> BodyState {
        BodyState {
            h_pos: h,
            v_pos: v,
            a_pos: a,
            growth: 1.,
            ..Default::default()
        }
    }

    #[test]
    fn native_type_decision() {
        let none = ChaosInputs::default();
        assert_eq!(
            evolve(&mut child(0.49, -0.49, 0.), none),
            chao_type::NEUTRAL_NORMAL
        );
        assert_eq!(evolve(&mut child(0., 0.6, 0.6), none), 0x0C); // Hero Fly
        assert_eq!(
            evolve(&mut child(-0.7, 0.2, 0.), none),
            chao_type::NEUTRAL_RUN
        );
        assert_eq!(evolve(&mut child(0.7, 0.2, -0.5), none), 0x13); // Dark Power
        assert_eq!(
            evolve(&mut child(0.2, -0.7, -0.49), none),
            chao_type::NEUTRAL_SWIM
        );
        // Ties on the diagonals follow the strict native comparisons.
        assert_eq!(
            evolve(&mut child(0.6, 0.6, 0.), none),
            chao_type::NEUTRAL_POWER
        );
        assert_eq!(
            evolve(&mut child(-0.6, 0.6, 0.), none),
            chao_type::NEUTRAL_RUN
        );
        let chaos = ChaosInputs {
            all_animals: true,
            user_like: 81,
            param_142: 2,
        };
        assert_eq!(
            evolve(&mut child(0., 0., 0.9), chaos),
            chao_type::HERO_CHAOS
        );
        assert_eq!(
            evolve(
                &mut child(0., 0., 0.),
                ChaosInputs {
                    user_like: 80,
                    ..chaos
                }
            ),
            5
        );
        let mut b = child(0.9, 0.1, 0.7);
        evolve(&mut b, none);
        assert_eq!((b.h_pos, b.v_pos, b.growth, b.a_pos), (0., 0., 0., 0.7));
        assert_eq!(b.model_base_index(), 18 + 13 * 6); // al_hpz
        assert_eq!(evolve(&mut b, none), 0x12);
    }

    #[test]
    fn capture_fruit_and_time_follow_native_amounts() {
        let mut b = BodyState::default();
        assert!(apply_capture(&mut b, 0));
        assert!(apply_capture(&mut b, 23));
        assert_eq!((b.h_pos, b.v_pos), (0.05, -0.1));
        assert!(!apply_capture(&mut b, 12));
        assert!(!apply_capture(&mut b, 25));
        for _ in 0..30 {
            apply_capture(&mut b, 9);
        }
        assert_eq!(b.v_pos, DEFORM_LIMIT);
        apply_fruit(&mut b, 5);
        assert_eq!(b.growth, FRUIT_GROWTH_CHILD);
        assert_eq!(b.a_pos, 0.15);
        apply_fruit(&mut b, 6);
        apply_fruit(&mut b, 6);
        assert!((b.a_pos + 0.15).abs() < 1e-6);
        let mut t = BodyState::default();
        for _ in 0..119 {
            t.parameter_call();
        }
        assert_eq!(t.growth, 0.);
        t.parameter_call();
        assert_eq!(t.growth, CHILD_TIME_GROWTH);
        for _ in 0..120 * 80 {
            t.parameter_call();
        }
        assert_eq!(t.growth, GROWTH_LIMIT);
        assert!(t.cocoon_due());
    }

    #[test]
    fn adult_weights_match_native_blend() {
        let mut b = child(0.5, -0.25, 0.);
        b.chao_type = chao_type::NEUTRAL_NORMAL;
        b.growth = 1.2;
        let (names, w) = b.morph();
        assert_eq!(names, ["n", "s", "f", "r", "p"]);
        let (h, v) = (0.5f32, 0.25f32);
        let c_n = (v * (1. - v) + h * (1. - h)) / (h + v);
        assert!((w[0] - 1.2 * c_n).abs() < 1e-6);
        assert!((w[1] - 1.2 * v * v / (h + v)).abs() < 1e-6);
        assert_eq!(w[2], 0.);
        assert_eq!(w[3], 0.);
        assert!((w[4] - 1.2 * h * h / (h + v)).abs() < 1e-6);
        assert!((c_n + (v * v + h * h) / (h + v) - 1.).abs() < 1e-6);
        b.h_pos = 0.;
        b.v_pos = 0.;
        let (_, w) = b.morph();
        assert!((w[0] - 1.2).abs() < 1e-4);
    }

    #[test]
    fn child_blends_neutral_with_aligned_set() {
        let b = BodyState {
            a_pos: -0.25,
            growth: 0.5,
            ..Default::default()
        };
        let (names, w) = b.morph();
        assert_eq!(names.len(), 17);
        assert!((w[0] - 0.75 * 0.5).abs() < 1e-4); // neutral n
        assert!((w[11] - 0.25 * 0.5).abs() < 1e-6); // dz keeps a*(1-g)
        assert!((w[12] - 0.25 * 0.5).abs() < 1e-4); // dn
        assert!(w[5..11].iter().all(|x| *x == 0.));
        let chaos = BodyState {
            chao_type: chao_type::DARK_CHAOS,
            ..Default::default()
        };
        assert_eq!(chaos.morph().1.len(), 0);
        assert_eq!(chaos.model_base_index(), 120);
    }

    #[test]
    fn restore_migrates_and_round_trips() {
        let b = BodyState::restore(None, "chaos", -100.).unwrap();
        assert!(b.is_chaos());
        let old = json!({"version":1,"aPos":0.7,"look":{"color":3,"jewel":0,"shiny":true,"monotone":false,"eggColor":0}});
        let migrated = BodyState::restore(Some(&old), "normal", 0.).unwrap();
        assert!(migrated.is_child());
        assert_eq!(migrated.a_pos, 0.7);
        assert!(migrated.look.shiny);
        let saved = serde_json::to_value(&migrated).unwrap();
        assert_eq!(
            BodyState::restore(Some(&saved), "normal", 0.).unwrap(),
            migrated
        );
        let mut bad = saved.clone();
        bad["growth"] = json!(1.3);
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
        b.add(1., 1., 1., 1.);
        evolve(&mut b, ChaosInputs::default());
        reincarnate(&mut b);
        assert!(b.is_child());
        assert_eq!(b.look, look);
    }
}
