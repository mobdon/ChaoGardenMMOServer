// Static SA2B landtable collision reader (SA2BLVL v3). Reads only the collision
// tail, native BASIC meshsets, surface bits, and the transform subset in the
// three imported gardens. Unsupported layouts fail instead of becoming solids.
use std::{collections::BTreeMap, path::Path, sync::OnceLock};
type V = [f64; 3];
fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: V, b: V) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[derive(Clone, Copy)]
pub struct Face {
    v: [V; 3],
    flags: u32,
    n: V,
}
pub struct Stage {
    faces: Vec<Face>,
    grid: BTreeMap<(i32, i32), Vec<usize>>,
}
struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn bytes<const N: usize>(&self, o: usize) -> Result<[u8; N], String> {
        self.0
            .get(o..o + N)
            .and_then(|b| b.try_into().ok())
            .ok_or("truncated landtable".into())
    }
    fn u(&self, o: usize) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.bytes(o)?))
    }
    fn h(&self, o: usize) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.bytes(o)?))
    }
    fn f(&self, o: usize) -> Result<f64, String> {
        let f = f32::from_le_bytes(self.bytes(o)?) as f64;
        if f.is_finite() {
            Ok(f)
        } else {
            Err("nonfinite vertex".into())
        }
    }
}
impl Stage {
    pub fn read(b: &[u8], origin: V) -> Result<Self, String> {
        let r = Reader(b);
        if b.get(..8) != Some(b"SA2BLVL\x03") {
            return Err("expected SA2BLVL v3".into());
        }
        let t = r.u(8)? as usize;
        let entries = r.u(t + 16)? as usize;
        let mut s = Self {
            faces: vec![],
            grid: BTreeMap::new(),
        };
        for i in r.h(t + 2)?..r.h(t)? {
            let e = entries + i as usize * 32;
            let flags = r.u(e + 28)?;
            if flags & 1 == 0 {
                continue;
            }
            let a = r.u(e + 16)? as usize;
            let m = r.u(a + 4)? as usize;
            if r.u(a + 20)? != 0 || r.u(a + 28)? != 0 || r.u(a + 44)? != 0 {
                return Err("unsupported rotated or nested collision node".into());
            }
            let yaw = (r.u(a + 24)? as i32 as f64) * std::f64::consts::TAU / 65536.;
            let (sn, cs) = yaw.sin_cos();
            let tr = [r.f(a + 8)?, r.f(a + 12)?, r.f(a + 16)?];
            let sc = [r.f(a + 32)?, r.f(a + 36)?, r.f(a + 40)?];
            let verts = r.u(m)? as usize;
            let sets = r.u(m + 12)? as usize;
            for j in 0..r.h(m + 20)? as usize {
                let set = sets + 24 * j;
                if r.h(set)? >> 14 != 3 {
                    return Err("expected native triangle strips".into());
                }
                let mut off = r.u(set + 4)? as usize;
                for _ in 0..r.h(set + 2)? {
                    let hdr = r.h(off)?;
                    off += 2;
                    let count = (hdr & 0x3fff) as usize;
                    let mut prev = [[0.; 3]; 2];
                    for k in 0..count {
                        let idx = r.h(off)? as usize;
                        off += 2;
                        if idx >= r.u(m + 8)? as usize {
                            return Err("bad vertex index".into());
                        }
                        let at = verts + 12 * idx;
                        let (x, y, z) =
                            (r.f(at)? * sc[0], r.f(at + 4)? * sc[1], r.f(at + 8)? * sc[2]);
                        let v = [
                            origin[0] - (tr[0] + cs * x + sn * z) * 0.1,
                            origin[1] + (tr[1] + y) * 0.1,
                            origin[2] + (tr[2] - sn * x + cs * z) * 0.1,
                        ];
                        if v.iter().any(|q| q.abs() > 5000.) {
                            return Err("out of range land vertex".into());
                        }
                        if k >= 2 {
                            let mut tri = [prev[0], prev[1], v];
                            if (k % 2 == 1) ^ (hdr & 0x8000 != 0) {
                                tri.swap(0, 1);
                            }
                            let n = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
                            let len = dot(n, n).sqrt();
                            if len > 1e-8 {
                                let index = s.faces.len();
                                s.faces.push(Face {
                                    v: tri,
                                    flags,
                                    n: n.map(|a| a / len),
                                });
                                let cells = |axis: usize| {
                                    let a = tri.map(|v| v[axis].floor() as i32);
                                    *a.iter().min().unwrap()..=*a.iter().max().unwrap()
                                };
                                for x in cells(0).filter(|x| (-24..=24).contains(x)) {
                                    for z in cells(2).filter(|z| (-24..=24).contains(z)) {
                                        s.grid.entry((x, z)).or_default().push(index);
                                    }
                                }
                            }
                        }
                        prev = [prev[1], v];
                    }
                }
            }
        }
        if s.faces.is_empty() {
            return Err("empty collision table".into());
        }
        Ok(s)
    }
    fn cast(&self, start: V, end: V, accept: impl Fn(&Face) -> bool) -> Option<(f64, u32)> {
        let dir = sub(end, start);
        let mut hit = None;
        let x0 = start[0].min(end[0]).floor() as i32;
        let x1 = start[0].max(end[0]).floor() as i32;
        let z0 = start[2].min(end[2]).floor() as i32;
        let z1 = start[2].max(end[2]).floor() as i32;
        for x in x0..=x1 {
            for z in z0..=z1 {
                for &idx in self.grid.get(&(x, z)).into_iter().flatten() {
                    let f = &self.faces[idx];
                    if !accept(f) {
                        continue;
                    }
                    let [a, b, c] = f.v;
                    let e = sub(b, a);
                    let h = cross(dir, sub(c, a));
                    let det = dot(e, h);
                    if det.abs() < 1e-9 {
                        continue;
                    }
                    let s = sub(start, a);
                    let u = dot(s, h) / det;
                    let q = cross(s, e);
                    let v = dot(dir, q) / det;
                    let d = dot(sub(c, a), q) / det;
                    if u >= -1e-6
                        && v >= -1e-6
                        && u + v <= 1.000001
                        && (0.0..=1.).contains(&d)
                        && hit.is_none_or(|(t, _)| d < t)
                    {
                        hit = Some((d, f.flags));
                    }
                }
            }
        }
        hit
    }
    pub fn floor(&self, x: f64, z: f64, reference: f64) -> Option<(f64, u32)> {
        self.cast([x, reference + 0.35, z], [x, reference - 0.75, z], |f| {
            f.flags & (2 | 0x1000) == 0 && f.n[1].abs() > 0.45
        })
        .map(|(t, f)| (reference + 0.35 - 1.1 * t, f))
    }
    pub fn wall(&self, a: V, b: V) -> Option<u32> {
        self.cast(a, b, |f| f.n[1].abs() < 0.45 && f.flags & 2 == 0)
            .map(|(_, flags)| flags)
    }
    pub fn ceiling(&self, a: V, b: V) -> Option<f64> {
        self.cast(a, b, |f| f.n[1].abs() >= 0.45 && f.flags & 2 == 0)
            .map(|(t, _)| a[1] + (b[1] - a[1]) * t)
    }
}
static STAGES: OnceLock<BTreeMap<String, Stage>> = OnceLock::new();
pub fn stage(zone: &str) -> Option<&'static Stage> {
    STAGES.get()?.get(zone)
}
pub fn load(root: &Path) -> Result<(), String> {
    if STAGES.get().is_some() {
        return Ok(());
    }
    let mut stages = BTreeMap::new();
    for (zone, file, origin) in [
        (
            "garden",
            "stg_neut/objLandTableNeut.sa2blvl",
            [-2.0968506171830086, 0.060817185301374366, 8.529567845606323],
        ),
        (
            "hero",
            "stg_hero/objLandTableHero.sa2blvl",
            [-3.6033796135917697, -3.3800047945335905, 15.041726085593204],
        ),
        (
            "dark",
            "stg_dark/objLandTableDark.sa2blvl",
            [-5.737651115485207, 0., 0.987035526051784],
        ),
    ] {
        let bytes = std::fs::read(root.join(file)).map_err(|e| format!("{file}: {e}"))?;
        let s = Stage::read(&bytes, origin).map_err(|e| format!("{file}: {e}"))?;
        eprintln!("{zone}: {} native solid triangles", s.faces.len());
        stages.insert(zone.into(), s);
    }
    STAGES.set(stages).map_err(|_| "already loaded".into())
}
