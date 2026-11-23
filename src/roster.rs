use std::{collections::BTreeMap, sync::LazyLock};
const NAMES: [&str; 17] = [
    "sonic",
    "terios",
    "miles",
    "egg",
    "knuck",
    "rouge",
    "twalk",
    "ewalk",
    "amy",
    "ssonic",
    "sshadow",
    "",
    "metalsonic",
    "cwalk",
    "dwalk",
    "tical",
    "chaos0",
];
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub max: f64,
    pub height: f64,
    pub bands: [f64; 4],
    pub jump: f64,
    pub gravity: f64,
    pub accel: f64,
    pub air: f64,
    pub decel: f64,
    pub brake: f64,
    pub air_brake: f64,
    pub air_decel: f64,
}
fn load() -> Option<BTreeMap<&'static str, Params>> {
    let text = std::fs::read_to_string(std::env::var_os("CHAO_ROSTER_TABLE")?).ok()?;
    let mut result = BTreeMap::new();
    for line in text.lines().filter(|l| l.starts_with("PhysicsArray\t")) {
        let f: Vec<_> = line.split('\t').collect();
        if f.len() != 35 {
            return None;
        }
        let id = f[1].parse::<usize>().ok()?;
        let Some(&name) = NAMES.get(id) else { continue };
        if name.is_empty() {
            continue;
        }
        let val = |i: usize| {
            f[2 + i]
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite() && x.abs() < 100.)
        };
        let p = Params {
            max: val(4)?,
            height: val(29)?,
            bands: [val(8)?, val(9)?, val(10)?, val(11)?],
            jump: val(6)?,
            gravity: val(30)?,
            accel: val(15)?,
            air: val(16)?,
            decel: -val(17)?,
            brake: -val(18)?,
            air_brake: -val(19)?,
            air_decel: -val(20)?,
        };
        if [
            p.max,
            p.jump,
            p.gravity,
            p.accel,
            p.air,
            p.decel,
            p.brake,
            p.air_brake,
            p.air_decel,
        ]
        .iter()
        .any(|v| *v <= 0.)
        {
            return None;
        }
        if result.insert(name, p).is_some() {
            return None;
        }
    }
    if result.len() != 16 {
        return None;
    }
    result.insert("metalsonicblack", result["metalsonic"]);
    Some(result)
}
static DATA: LazyLock<Option<BTreeMap<&'static str, Params>>> = LazyLock::new(load);
pub fn enabled() -> bool {
    DATA.is_some()
}
pub fn get(key: &str) -> Option<Params> {
    DATA.as_ref()?.get(key).copied()
}
// Original speeds use model units/frame. The garden displays 0.1 world
// units per model unit. Engine-specific steering is an adaptation here.
pub fn velocity(
    p: Params,
    v: (f64, f64),
    input: (f64, f64),
    airborne: bool,
    dt: f64,
) -> (f64, f64) {
    // Integrate in native frame units even when the room publishes at 20 Hz.
    // Braking ends at zero; the remaining time accelerates the new direction.
    let desired = (input.0 * p.max * 6., input.1 * p.max * 6.);
    let mut v = v;
    let mut frames = (dt * 60.).clamp(0., 60.);
    while frames > 1e-9 {
        let mut step = frames.min(1.);
        frames -= step;
        if v.0 * input.0 + v.1 * input.1 < 0. {
            let speed = v.0.hypot(v.1);
            let brake = 6. * if airborne { p.air_brake } else { p.brake };
            let used = (speed / brake).min(step);
            let factor = (1. - used * brake / speed).max(0.);
            v = (v.0 * factor, v.1 * factor);
            step -= used;
        }
        let rate = match (input == (0., 0.), airborne) {
            (true, true) => p.air_decel,
            (true, false) => p.decel,
            (false, true) => p.air,
            (false, false) => p.accel,
        };
        let d = (desired.0 - v.0, desired.1 - v.1);
        let gap = d.0.hypot(d.1);
        if gap > 0. {
            let gain = (step * 6. * rate / gap).min(1.);
            v = (v.0 + d.0 * gain, v.1 + d.1 * gain);
        }
    }
    v
}

pub fn gait(p: Params, v: (f64, f64)) -> usize {
    let s = v.0.hypot(v.1) / 6.;
    if s < 0.01 {
        0
    } else {
        1 + p.bands.iter().filter(|b| s >= **b).count()
    }
}
pub fn walker(k: Option<&str>) -> bool {
    matches!(k, Some("twalk" | "ewalk" | "dwalk" | "cwalk"))
}
#[cfg(test)]
mod tests {
    use super::*;
    // Fixture uses the PC table fields in model units/frame, not garden velocity.
    fn shadow() -> Params {
        Params {
            max: 3.,
            height: 10.,
            bands: [0.23, 0.46, 1.39, 2.3],
            jump: 1.66,
            gravity: 0.08,
            accel: 0.05,
            air: 0.031,
            decel: 0.06,
            brake: 0.18,
            air_brake: 0.17,
            air_decel: 0.028,
        }
    }
    #[test]
    fn roster_acceleration_stopping_and_tick_partitions() {
        let p = shadow();
        let forward = (1., 0.);
        let from_rest = |sec| velocity(p, (0., 0.), forward, false, sec);
        assert!((from_rest(0.25).0 - 4.5).abs() < 1e-10);
        assert!((from_rest(1.).0 - 18.).abs() < 1e-10);
        let fast = Params { accel: 0.2, ..p };
        assert!((velocity(fast, (0., 0.), forward, false, 0.25).0 - 18.).abs() < 1e-10);
        let start = (4.5, 0.);
        let back = (-1., 0.);
        let one = velocity(p, start, back, false, 0.25);
        let many = (0..5).fold(start, |v, _| velocity(p, v, back, false, 0.05));
        assert!((one.0 - many.0).abs() < 1e-10 && one.0 < 0.);
        assert!(
            velocity(p, start, (0., 0.), true, 0.1).0 > velocity(p, start, (0., 0.), false, 0.1).0
        );
    }
}
