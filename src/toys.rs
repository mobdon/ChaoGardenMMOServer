//! Native shop costs, unlock order and toy pictures; requesting play and the
//! fixed drop spots are MMO adaptations. No toy awards wallet or skill gains.
use crate::{collection::EggSource, game::Player};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiny_garden_core::{native_growth::NativeRng, native_shop_state::*};

const FRAME: u64 = 4389 * 1_000_000;
const SPOTS: [[u8; 2]; 3] = [[94, 100], [64, 159], [126, 91]];
#[derive(Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Toys {
    count: u8,
    wait: u32,
    clock: u8,
    sub: u64,
    practice: u8,
    playing: Option<Play>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Play {
    id: u8,
    frame: u16,
    length: u16,
}
impl Toys {
    pub fn valid(&self) -> bool {
        self.count <= 3
            && self.practice <= 3
            && self.sub < FRAME
            && self.wait <= TOY_UNLOCK_TICKS
            && (self.wait == 0 || self.count == 2)
            && (self.count > 0 || self.practice == 0)
            && self.playing.as_ref().is_none_or(|p| {
                p.id < self.count
                    && p.frame < p.length
                    && match p.id {
                        0 => matches!(p.length, 80 | 144),
                        1 => p.length == 180,
                        2 => p.length == 240,
                        _ => false,
                    }
            })
    }
    fn fields(&self) -> NativeShopFields {
        NativeShopFields {
            toy_unlock_ticks: self.wait,
            toy_positions: std::array::from_fn(|i| {
                if i < self.count as usize {
                    NativePosition {
                        x: SPOTS[i][0],
                        y: SPOTS[i][1],
                    }
                } else {
                    NativePosition::default()
                }
            }),
            ..Default::default()
        }
    }
    pub fn advance(&mut self, dt: f64) {
        self.sub += (dt * 1e6).round() as u64 * 262144;
        let frames = (self.sub / FRAME) as u32;
        self.sub %= FRAME;
        self.clock = self.clock.wrapping_add(frames as u8);
        self.wait = self.wait.saturating_sub(frames);
        if let Some(p) = &mut self.playing {
            p.frame = p.frame.saturating_add(frames as u16);
            if p.frame >= p.length {
                self.playing = None;
            }
        }
    }
    pub fn public(&self, src: &EggSource) -> Value {
        let next = self.fields().next_toy();
        let phase = usize::from(self.clock);
        let items:Vec<_>=["Trumpet","Rubber Duck","Television"].iter().enumerate().map(|(i,name)|{
 let mut x=i16::from(SPOTS[i][0]);let mut y=i16::from(SPOTS[i][1]);
 if i==1{x+=src.wave[phase]/129;y+=src.wave[(phase*2+0xc0)&255]/129;}
 json!({"id":i,"name":name,"owned":i<usize::from(self.count),"offered":next==Some(i as u8),"price":src.prices.toy[i],"x":x,"y":y,"frame":u8::from(i==2 && self.clock&8!=0)})
 }).collect();
        let play=self.playing.as_ref().map(|p|{
 let phase=if p.frame<16 {0}else{1+(p.frame-16)/20};
 let (pose,dx,dy)=match p.id {0=>(if phase==0{0x25}else{0x3a+((phase-1)%2)},if phase==0 || phase%2==0{15}else{14},if phase==0{1}else{3}),1=>(0x1c,18,-6),_=>(0x1c,22,3)};
 json!({"id":p.id,"pose":pose,"x":i16::from(SPOTS[p.id as usize][0])+dx,"y":i16::from(SPOTS[p.id as usize][1])+dy})
 });
        json!({"rulesVersion":"tiny-toys-v1","items":items,"play":play,"tvWaitTicks":self.wait})
    }
}
pub fn apply(p: &mut Player, src: &EggSource, id: u8, buy: bool) -> Result<Value, &'static str> {
    if !p.care.tiny_resident || id >= 3 {
        return Err("toy-unavailable");
    }
    let toys = p.care.toys.get_or_insert_with(Toys::default);
    if toys.playing.is_some() {
        return Err("toy-busy");
    }
    if buy {
        let mut store =
            NativeShopState::restore(toys.fields(), p.rings).map_err(|_| "toy-state-invalid")?;
        store
            .open_stock(0, 1, &mut NativeRng::new(0))
            .map_err(|_| "toy-state-invalid")?;
        let receipt = store
            .purchase(
                &src.prices,
                NativeShopItem::Toy(id),
                NativePurchaseInput {
                    cursor_x: 32,
                    cursor_y: 0x9a - 8,
                    pressed_keys: 1,
                    ..Default::default()
                },
            )
            .map_err(|e| match e {
                Error::InsufficientRings => "insufficient-rings",
                Error::NotOffered => "toy-not-offered",
                _ => "toy-state-invalid",
            })?;
        p.rings = receipt.rings_after;
        toys.count += 1;
        toys.wait = store.fields.toy_unlock_ticks;
        Ok(json!({"toyId":id,"rings":-i64::from(receipt.price)}))
    } else {
        if id >= toys.count {
            return Err("toy-not-owned");
        }
        let length = match id {
            0 => {
                let n = if toys.practice < 3 { 80 } else { 144 };
                toys.practice = (toys.practice + 1).min(3);
                n
            }
            1 => 180,
            _ => 240,
        };
        toys.playing = Some(Play {
            id,
            frame: 0,
            length,
        });
        Ok(json!({"toyId":id}))
    }
}
