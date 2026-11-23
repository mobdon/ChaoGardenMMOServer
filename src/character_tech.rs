use crate::game::{Event, Game, Player};
use serde_json::json;
use std::sync::LazyLock;
pub type Point = [f64; 3];
#[derive(Clone, Debug, Default)]
pub struct Tech {
    pub held: bool,
    charging: bool,
    locks: Vec<String>,
    volley: Vec<String>,
    next: f64,
    rockets: Vec<(String, Point, f64)>,
    target: Option<String>,
    until: f64,
    pub mode: &'static str,
    rail: Option<(f64, f64)>,
    flying: bool,
    ring_time: f64,
}
impl Tech {
    pub fn locks(&self) -> &[String] {
        &self.locks
    }
}
fn speed(k: &str) -> bool {
    matches!(k, "sonic" | "terios" | "metalsonic" | "metalsonicblack")
}
fn hunter(k: &str) -> bool {
    matches!(k, "knuck" | "rouge" | "tical" | "chaos0")
}
fn norm(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}
fn interpolate(a: Point, b: Point, t: f64) -> Point {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
pub fn rail_sample(path: &[Point], mut d: f64) -> Option<Point> {
    for pair in path.windows(2) {
        let l = norm(pair[0], pair[1]);
        if l > 0. && d <= l {
            return Some(interpolate(pair[0], pair[1], (d / l).max(0.)));
        }
        d -= l;
    }
    None
}
fn nearest(path: &[Point], p: Point) -> Option<(f64, f64, Point)> {
    let mut length = 0.;
    let mut best = None;
    for line in path.windows(2) {
        let (a, b) = (line[0], line[1]);
        let l = norm(a, b);
        if l == 0. {
            continue;
        }
        let t =
            ((0..3).map(|i| (p[i] - a[i]) * (b[i] - a[i])).sum::<f64>() / (l * l)).clamp(0., 1.);
        let dist = norm(interpolate(a, b, t), p);
        if best.as_ref().is_none_or(|x: &(f64, f64, Point)| dist < x.0) {
            best = Some((
                dist,
                length + l * t,
                std::array::from_fn(|i| (b[i] - a[i]) / l),
            ))
        }
        length += l;
    }
    best
}
pub static MISSION_RAIL: LazyLock<Vec<Point>> = LazyLock::new(|| {
    let Some(raw) =
        std::env::var_os("CHAO_SKY_RAILS").and_then(|p| std::fs::read_to_string(p).ok())
    else {
        return vec![];
    };
    let mut points: Vec<Point> = vec![];
    for l in raw.lines().filter(|l| l.starts_with("14\t")) {
        let f: Vec<_> = l.split('\t').collect();
        if f.len() != 5 || f[1].parse::<usize>().ok() != Some(points.len()) {
            return vec![];
        }
        let Ok(p) = f[2..]
            .iter()
            .map(|n| n.parse::<f64>())
            .collect::<Result<Vec<_>, _>>()
        else {
            return vec![];
        };
        if !p.iter().all(|n| n.is_finite()) {
            return vec![];
        }
        points.push([p[0], p[1], p[2]]);
    }
    if points.len() != 9 {
        return vec![];
    }
    let o = points[0];
    points
        .iter()
        .map(|p| {
            [
                -2. + (p[0] - o[0]) * 0.06,
                0.32 + (p[1] - o[1]) * 0.06,
                12. + (p[2] - o[2]) * 0.06,
            ]
        })
        .collect()
});
// Mission-specific surface adapter, matching the courtyard buildings and roof.
// Other zones keep the original sampled garden terrain.
pub fn roof(x: f64, z: f64) -> Option<f64> {
    (0..8).find_map(|i| {
        let side = if i % 2 == 0 { -1. } else { 1. };
        let cx = side * (18. + (i % 3) as f64 * 2.);
        let cz = 12. - (i / 2) as f64 * 10.;
        ((x - cx).abs() <= 2.55 && (z - cz).abs() <= 2.7).then_some(4.4 + (i % 3) as f64 * 2.)
    })
}
fn near_wall(p: &Player) -> Option<f64> {
    if p.zone != "mission" {
        let s = crate::native_land::stage(&p.zone)?;
        let level = p.y + 0.25;
        return s
            .wall(
                [p.x, level, p.z],
                [
                    p.x + p.facing.sin() * 0.5,
                    level,
                    p.z + p.facing.cos() * 0.5,
                ],
            )
            .filter(|flags| flags & 0x80 == 0)
            .map(|_| p.y + 0.6);
    }
    roof(p.x + p.facing.sin() * 0.45, p.z + p.facing.cos() * 0.45).filter(|h| p.y < *h + 0.02)
}
fn targets(g: &Game, p: &Player, range: f64) -> Vec<String> {
    let Some(m) = g
        .mission
        .as_ref()
        .filter(|m| p.zone == "mission" && m.members.contains(&p.id) && !m.completed)
    else {
        return vec![];
    };
    let mut found: Vec<_> = m
        .enemies
        .iter()
        .filter(|e| e.hp > 0.)
        .filter_map(|e| {
            let dx = e.x - p.x;
            let dz = e.z - p.z;
            let d = norm([p.x, p.y, p.z], [e.x, 0.35, e.z]);
            (d <= range && (dx * p.facing.sin() + dz * p.facing.cos() >= dx.hypot(dz) * 0.35))
                .then_some((d, e.id.clone()))
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().map(|v| v.1).collect()
}
fn hit(g: &mut Game, p: &mut Player, id: &str, damage: f64, kind: &str) {
    let Some(e) = g
        .mission
        .as_mut()
        .filter(|m| !m.completed && p.zone == "mission" && m.members.contains(&p.id))
        .and_then(|m| m.enemies.iter_mut().find(|e| e.hp > 0. && e.id == id))
    else {
        return;
    };
    e.hp = (e.hp - damage).max(0.);
    let dead = e.hp == 0.;
    g.events.push(Event{target:None,message:json!({"type":"event","kind":kind,"playerId":p.id,"zone":p.zone,"target":id,"x":p.x,"y":p.y+0.4,"z":p.z,"hitX":e.x,"hitY":0.35,"hitZ":e.z})});
    if dead {
        p.rings = p.rings.saturating_add(5);
        p.gain("power", 1);
        g.dirty = true;
    }
}
pub fn homing(g: &mut Game, p: &mut Player) -> bool {
    if !p.admin || p.jump_offset <= 0.02 || !speed(p.dev_actor.as_deref().unwrap_or("")) {
        return false;
    }
    if let Some(id) = targets(g, p, 8.).into_iter().next() {
        if p.cooldown("homing", g.time, 500.) {
            p.tech.target = Some(id);
            p.tech.until = g.time + 550.;
        }
    }
    true
}
pub fn dig(g: &mut Game, p: &mut Player) {
    if !p.admin
        || !hunter(p.dev_actor.as_deref().unwrap_or(""))
        || p.jump_offset > 0.02
        || !(if p.zone == "mission" {
            roof(p.x, p.z).is_none() && !(p.x.abs() <= 15. && (-18.5..=14.5).contains(&p.z))
        } else {
            crate::native_land::stage(&p.zone)
                .and_then(|s| s.floor(p.x, p.z, p.ground_y))
                .is_some_and(|(_, flags)| flags & 0x20 != 0)
        })
    {
        return;
    }
    if p.cooldown("dig", g.time, 1000.) {
        p.tech.mode = "dig";
        p.tech.until = g.time + 700.;
    }
}
// Return authoritative displacement overrides; the normal world collision sweep still runs.
pub fn motion(
    g: &mut Game,
    p: &mut Player,
    dt: f64,
    jump: bool,
    vx: &mut f64,
    vz: &mut f64,
    y: &mut f64,
) {
    let Some(k) = p.dev_actor.clone().filter(|_| p.admin && p.watch.is_none()) else {
        return;
    };
    let held = p.tech.held;
    if p.tech.mode == "dig" && g.time < p.tech.until {
        *vx = 0.;
        *vz = 0.;
        *y = p.ground_y;
        return;
    }
    if p.tech.mode == "dig" && p.zone == "mission" && (p.x - 16.).hypot(p.z - 6.) < 1.3 {
        if let Some(r) = g.mission.as_mut().and_then(|m| {
            m.rings
                .iter_mut()
                .find(|r| r.id.ends_with("-buried") && r.available_at == f64::MAX)
        }) {
            r.available_at = 0.;
            g.notice(&p.id, "You uncovered a buried ring.", "success");
        }
    }
    p.tech.mode = "";
    if crate::roster::walker(Some(&k)) {
        if held && p.tech.volley.is_empty() {
            if !p.tech.charging {
                p.tech.locks.clear();
                p.tech.next = g.time;
            }
            p.tech.charging = true;
            if g.time >= p.tech.next && p.tech.locks.len() < 5 {
                if let Some(id) = targets(g, p, 12.)
                    .into_iter()
                    .find(|id| !p.tech.locks.contains(id))
                {
                    p.tech.locks.push(id);
                    p.tech.next = g.time + 180.;
                }
            }
            p.tech.mode = "lock";
        } else if p.tech.charging {
            p.tech.volley = std::mem::take(&mut p.tech.locks);
            p.tech.volley.reverse();
            p.tech.next = g.time;
            p.tech.charging = false;
        }
        if !p.tech.volley.is_empty() && g.time >= p.tech.next {
            let id = p.tech.volley.pop().unwrap();
            p.tech.next = g.time + 160.;
            if let Some(e) = g
                .mission
                .as_ref()
                .filter(|m| p.zone == "mission" && !m.completed)
                .and_then(|m| {
                    m.enemies
                        .iter()
                        .find(|e| e.id == id && e.hp > 0. && (e.x - p.x).hypot(e.z - p.z) < 16.)
                })
            {
                let pos = [p.x, p.y + 0.45, p.z];
                p.tech.rockets.push((id.clone(), pos, g.time + 1200.));
                g.events.push(Event{target:None,message:json!({"type":"event","kind":"roster-missile","playerId":p.id,"zone":p.zone,"target":id,"x":pos[0],"y":pos[1],"z":pos[2],"hitX":e.x,"hitY":0.35,"hitZ":e.z})});
            }
        }
        for (id, pos, end) in std::mem::take(&mut p.tech.rockets) {
            let Some(e) = g
                .mission
                .as_ref()
                .filter(|m| p.zone == "mission" && !m.completed && end >= g.time)
                .and_then(|m| m.enemies.iter().find(|e| e.id == id && e.hp > 0.))
            else {
                continue;
            };
            let goal = [e.x, 0.35, e.z];
            let d = norm(pos, goal);
            let b = interpolate(pos, goal, (20. * dt / d.max(0.001)).min(1.));
            if roof(b[0], b[2]).is_some_and(|h| b[1] < h) {
                continue;
            }
            if norm(b, goal) < 0.25 {
                hit(g, p, &id, 75., "missile-hit");
            } else {
                p.tech.rockets.push((id, b, end));
            }
        }
    }
    if let Some(id) = p.tech.target.clone() {
        let target = g
            .mission
            .as_ref()
            .filter(|_| p.zone == "mission")
            .and_then(|m| m.enemies.iter().find(|e| e.id == id && e.hp > 0.))
            .map(|e| [e.x, 0.35, e.z]);
        if let Some(t) = target.filter(|_| g.time <= p.tech.until) {
            let a = [p.x, p.y, p.z];
            let d = norm(a, t);
            if d < 0.65 {
                hit(g, p, &id, 100., "homing-hit");
                p.tech.target = None;
                p.vy = 4.;
                *y = p.y + p.vy * dt;
            } else {
                let b = interpolate(a, t, (24. * dt / d).min(1.));
                *vx = (b[0] - p.x) / dt;
                *vz = (b[2] - p.z) / dt;
                *y = b[1];
                p.vy = 0.;
                p.tech.mode = "homing";
            }
            return;
        }
        p.tech.target = None;
    }
    if hunter(&k) && held {
        if let Some(top) = near_wall(p) {
            p.tech.mode = "climb";
            *y = (p.y + dt * 3.).min(top + 0.06);
            p.vy = 0.;
            return;
        }
        if p.jump_offset > 0.02 {
            p.tech.mode = "glide";
            p.vy = p.vy.max(-0.8);
            *y = p.y + p.vy * dt;
            if vx.hypot(*vz) < 4. {
                *vx = 4. * p.facing.sin();
                *vz = 4. * p.facing.cos();
            }
            return;
        }
    }
    if matches!(k.as_str(), "ssonic" | "sshadow") {
        if held && p.rings > 0 {
            p.tech.flying = true;
        }
        if p.tech.flying && p.rings > 0 {
            p.tech.ring_time += dt;
            if p.tech.ring_time >= 1. {
                p.rings -= 1;
                p.tech.ring_time -= 1.;
                g.dirty = true;
            }
            p.tech.mode = "flight";
            p.vy = if held {
                3.
            } else if jump {
                -3.
            } else {
                0.
            };
            *y = (p.y + dt * p.vy).clamp(p.ground_y, p.ground_y + 12.);
            if !held && jump && *y <= p.ground_y + 0.02 {
                p.tech.flying = false;
            }
            return;
        }
        p.tech.flying = false;
    }
    if speed(&k) && p.zone == "mission" {
        if p.tech.rail.is_some() && jump {
            p.tech.rail = None;
            p.vy = 7.;
            *y = p.y + p.vy * dt;
            return;
        }
        if p.tech.rail.is_none() && held {
            if let Some((d, s, t)) = nearest(&MISSION_RAIL, [p.x, p.y, p.z]) {
                if d < 0.8 {
                    let sign = if t[0] * p.facing.sin() + t[2] * p.facing.cos() < 0. {
                        -1.
                    } else {
                        1.
                    };
                    p.tech.rail = Some((s, sign));
                }
            }
        }
        if let Some((s, sign)) = p.tech.rail {
            let next = s + sign * 8. * dt;
            if let Some(pos) = rail_sample(&MISSION_RAIL, next).filter(|_| next >= 0.) {
                p.tech.mode = "grind";
                p.tech.rail = Some((next, sign));
                *vx = (pos[0] - p.x) / dt;
                *vz = (pos[2] - p.z) / dt;
                *y = pos[1];
                p.vy = 0.;
                return;
            }
            p.tech.rail = None;
        }
    }
}
