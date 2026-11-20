//! Private resident garden adapter; the core owns every growth and pull step.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::num::NonZeroU16;
use tiny_garden_core::{native_growth::NativeRng, native_weeds::*};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    native: NativeWeeds,
    seed: u32,
    generations: [u32; 16],
    remainder: u64,
    #[serde(default)]
    needs: Option<tiny_garden_core::native_needs::NeedsClock>,
}
const FRAME: u64 = 4389 * 1_000_000;
impl Patch {
    pub(crate) fn new() -> Self {
        // MMO admission policy: introduce one weed on the first active frame;
        // subsequent spawn intervals and all placements come from the cartridge.
        let fields = NativeWeedFields {
            spawn_ticks: 1,
            ..Default::default()
        };
        Self {
            native: NativeWeeds::restore(fields),
            seed: uuid::Uuid::new_v4().as_fields().0,
            generations: [0; 16],
            remainder: 0,
            needs: None,
        }
    }
    pub(crate) fn valid(&self) -> bool {
        self.remainder < FRAME
            && self.needs.as_ref().is_none_or(|n| n.valid())
            && self.native.fields.spawn_ticks <= 89 * 60
            && self.native.fields.spawn_ticks > 0
            && self.native.pull.is_none_or(|p| {
                p.slot < 16 && p.phase <= 7 && self.native.weeds[usize::from(p.slot)].present
            })
            && self.native.weeds.iter().enumerate().all(|(i, w)| {
                let pos = self.native.fields.positions[i];
                (pos.x != 0) == w.present
                    && (!w.present
                        || (self.generations[i] > 0
                            && (8..168).contains(&pos.x)
                            && (24..160).contains(&pos.y)
                            && w.x == pos.x
                            && w.base_y == pos.y
                            && w.growth_frame <= 64
                            && w.interaction <= 4))
            })
    }
    pub(crate) fn advance(
        &mut self,
        dt: f64,
        assets: &NativeWeedAssets,
        happiness: &mut f64,
        satiety: &mut f64,
    ) -> bool {
        self.remainder += (dt * 1_000_000.).round() as u64 * 262144;
        let mut rng = NativeRng::new(self.seed);
        let mut changed = false;
        while self.remainder >= FRAME {
            self.remainder -= FRAME;
            let count = self.native.fields.count();
            let (mut mood, mut belly) = (
                (*happiness * 2.).round().clamp(0., 200.) as u8,
                (*satiety * 2.).round().clamp(0., 200.) as u8,
            );
            let before = (mood, belly);
            self.needs
                .get_or_insert_with(|| {
                    tiny_garden_core::native_needs::NeedsClock::new(belly, count)
                })
                .tick(&mut mood, &mut belly, count);
            *happiness =
                (*happiness + (f64::from(mood) - f64::from(before.0)) / 2.).clamp(0., 100.);
            *satiety = (*satiety + (f64::from(belly) - f64::from(before.1)) / 2.).clamp(0., 100.);
            changed = true; // Persist timer progress even before the next visible need changes.
            let update = self
                .native
                .spawn_tick(0, assets, &mut rng, NonZeroU16::new(32).unwrap())
                .expect("valid native spawner");
            if update.event == SpawnEvent::Pending {
                continue;
            }
            if let SpawnEvent::Spawned { slot, .. } = update.event {
                let g = &mut self.generations[usize::from(slot)];
                *g = g.saturating_add(1);
                changed = true;
            }
            if self.native.pull.is_some() {
                self.native.update_pull(0, assets).expect("validated pull");
            }
            for i in 0..16 {
                if self.native.weeds[i].present {
                    changed |= self
                        .native
                        .update_weed(i as u8, 0)
                        .expect("valid slot")
                        .removed;
                }
            }
        }
        self.seed = rng.state();
        changed
    }
    pub(crate) fn pull(
        &mut self,
        slot: u8,
        generation: u32,
        assets: &NativeWeedAssets,
    ) -> Result<(), &'static str> {
        let i = usize::from(slot);
        let weed = self.native.weeds.get(i).ok_or("weed-unavailable")?;
        if !weed.present || self.generations[i] != generation {
            return Err("weed-unavailable");
        }
        // A touch targets the centre of the original class-3 hit rectangle.
        let [left, width, top, height] = assets.hitbox;
        self.native
            .begin_pull(
                slot,
                NativePullInput {
                    cursor_x: i16::from(weed.x) + 8 + i16::from(left) + i16::from(width) / 2,
                    cursor_y: weed.y - 8 + i16::from(top) + i16::from(height) / 2,
                    pressed_keys: 1,
                    ..Default::default()
                },
                assets,
            )
            .map_err(|_| "weed-busy")?;
        Ok(())
    }
    pub(crate) fn public(&self) -> Value {
        let items:Vec<_>=self.native.weeds.iter().enumerate().filter(|(_,w)|w.present).map(|(i,w)|json!({
            "slot":i,"generation":self.generations[i],"x":w.x,"y":w.y,"scale64":if w.emerging{w.growth_frame}else{64},
            "ready":!w.emerging && self.native.pull.is_none(),"pulling":self.native.pull.is_some_and(|p|usize::from(p.slot)==i)
        })).collect();
        json!({"rulesVersion":"a2ne-weeds-v1","items":items})
    }
}
