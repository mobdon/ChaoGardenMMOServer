//! The Black Market's seed and fruit stock, ported from the GameCube
//! Kindergarten module (ChaoStgKinder, GSNE8P; matches the decomp byte for
//! byte) and ChaoMain's item tables.
//!
//! Native sources (section-relative):
//! - Item info rows (buy, sell, emblems, name, description; 16 bytes each),
//!   ChaoMain .rodata; seeds at 0x14E0 + 16 * id (id 0-6 = Strong..Square).
//! - Restock: Kinder fn 0x2D14. It runs only on entering the shop with the
//!   timer expired (fn 0xB50, 0xD68); the caller then resets the timer to
//!   10800 + int(14400 f) frames. Draws use the global LCG at main.dol
//!   0x8010DC88; each draw is f = k / 32768. Chance tests are f < C
//!   (single-precision 0.3 / 0.7), percent tests int(100 f) < P, picks int(N f).
//! - Build order: eggs, egg count (one draw, 1-5), Hero 70 / Dark 70 / Heart 50
//!   / Chao 50 fruit, two of Round / Triangle / Square fruit (0x3044), Strong
//!   seed always, f < 0.7 then int(3 f) of Tasty / Hero / Dark seed (0xB64),
//!   two of Round / Triangle / Square seeds (0x31D8), hats, mushroom (f < 0.3
//!   alternate, then int(100 f) < 70), themes. "Two of" redraws duplicates and
//!   adds in ascending id. At most 32 entries.
//! - Buying (Kinder fn 0x1AD0, 0x2100 / 0x21A0): seeds and ordinary fruit stay
//!   in stock; mushrooms (and eggs, hats, themes) are removed.
//!
//! Not ported, and therefore absent: eggs, hats and menu themes, since this
//! server has no emblem count, egg item or hat item to gate or hold them. With
//! no emblems their native rows are skipped without draws, except the egg
//! count draw, which is still taken so the draw order matches. The shop stands
//! at the existing hub shop point because this world has no Kindergarten.
use crate::garden::{FRAME_MS, Rand, SEEDS};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MARKET_RULES: &str = "native-gc-black-market-v1";
pub const STOCK_LIMIT: usize = 32;
const RESTOCK_BASE: u64 = 10_800;
const RESTOCK_SPREAD: f32 = 14_400.;

/// Native fruit indices used by the stock.
const HERO: u8 = 5;
const DARK: u8 = 6;
const ROUND: u8 = 7;
const HEART: u8 = 10;
const CHAO: u8 = 11;
const MUSHROOM: u8 = 20;
const MUSHROOM_ALT: u8 = 21;

/// (buy, sell) rings for seeds by native seed id 0-6 (SEEDS order).
const SEED_PRICES: [(u32, u32); 7] = [
    (500, 150),
    (200, 50),
    (400, 100),
    (400, 100),
    (300, 80),
    (300, 80),
    (300, 80),
];

/// (buy, sell) rings for fruit the Black Market stocks.
fn fruit_price(id: u8) -> Option<(u32, u32)> {
    match id {
        HERO | DARK => Some((120, 40)),
        ROUND..=9 => Some((80, 20)),
        HEART => Some((300, 80)),
        CHAO => Some((200, 50)),
        MUSHROOM | MUSHROOM_ALT => Some((300, 50)),
        _ => None,
    }
}

/// Native (buy, sell) price of a market item key, if it has one.
pub fn prices(item_id: &str) -> Option<(u32, u32)> {
    if let Some(family) = item_id.strip_prefix("seed:") {
        let id = SEEDS.iter().position(|s| *s == family)?;
        return Some(SEED_PRICES[id]);
    }
    let id = item_id.strip_prefix("sa2-fruit:")?.parse::<u8>().ok()?;
    (item_id == format!("sa2-fruit:{id}"))
        .then(|| fruit_price(id))
        .flatten()
}

/// Removed from stock when bought (mushrooms; seeds and other fruit stay).
pub fn removed_when_bought(item_id: &str) -> bool {
    item_id == format!("sa2-fruit:{MUSHROOM}") || item_id == format!("sa2-fruit:{MUSHROOM_ALT}")
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Market {
    pub stock: Vec<String>,
    /// Frame at or after which entering the shop rebuilds the stock. The
    /// native timer is not saved (it counts from power-on); here it is kept
    /// so a restart does not restock early. `None` restocks on first entry.
    pub restock_at: Option<u64>,
}

/// One native draw: f = k / 32768.
fn draw(rand: &mut dyn Rand) -> f32 {
    let k = (rand.next().clamp(0., 0.999_999_9) * 32768.).floor();
    k / 32768.
}
fn pick(rand: &mut dyn Rand, n: usize) -> usize {
    ((n as f32 * draw(rand)) as usize).min(n - 1)
}
/// Two different picks of three, redrawing duplicates, in ascending order.
fn two_of_three(rand: &mut dyn Rand) -> [usize; 2] {
    let a = pick(rand, 3);
    let mut b = pick(rand, 3);
    while b == a {
        b = pick(rand, 3);
    }
    [a.min(b), a.max(b)]
}

impl Market {
    pub fn valid(&self) -> bool {
        self.stock.len() <= STOCK_LIMIT && self.stock.iter().all(|i| prices(i).is_some())
    }
    pub fn due(&self, frame: u64) -> bool {
        self.restock_at.is_none_or(|at| frame >= at)
    }

    /// Kinder fn 0x2D14 with no emblems, then the timer reset.
    pub fn restock(&mut self, frame: u64, rand: &mut dyn Rand) {
        let mut stock = Vec::new();
        // Egg count draw (no egg rows qualify without emblems).
        let _eggs = (5. * draw(rand)) as u32 + 1;
        let percent = |rand: &mut dyn Rand, p: u32| ((100. * draw(rand)) as u32) < p;
        for (fruit, chance) in [(HERO, 70), (DARK, 70), (HEART, 50), (CHAO, 50)] {
            if percent(rand, chance) {
                stock.push(format!("sa2-fruit:{fruit}"));
            }
        }
        for i in two_of_three(rand) {
            stock.push(format!("sa2-fruit:{}", ROUND + i as u8));
        }
        stock.push(format!("seed:{}", SEEDS[0]));
        if draw(rand) < 0.7 {
            stock.push(format!("seed:{}", SEEDS[1 + pick(rand, 3)]));
        }
        for i in two_of_three(rand) {
            stock.push(format!("seed:{}", SEEDS[4 + i]));
        }
        // No hats qualify without emblems.
        let alternate = draw(rand) < 0.3;
        if percent(rand, 70) {
            let id = if alternate { MUSHROOM_ALT } else { MUSHROOM };
            stock.push(format!("sa2-fruit:{id}"));
        }
        // No themes qualify without emblems.
        stock.truncate(STOCK_LIMIT);
        self.stock = stock;
        self.restock_at = Some(frame + RESTOCK_BASE + (RESTOCK_SPREAD * draw(rand)) as u64);
    }

    pub fn public(&self) -> Value {
        let stock: Vec<Value> = self
            .stock
            .iter()
            .filter_map(|id| {
                prices(id).map(|(buy, sell)| json!({"itemId":id,"price":buy,"sell":sell}))
            })
            .collect();
        json!({"rules":MARKET_RULES,"stock":stock,
            "restockAtMs":self.restock_at.map(|f| f as f64 * FRAME_MS),
            "location":"adapted-hub-shop-point",
            "notPorted":["eggs","hats","menu-themes","emblem-gates"]})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::Scripted;

    #[test]
    fn prices_match_the_native_tables() {
        assert_eq!(prices("seed:strong"), Some((500, 150)));
        assert_eq!(prices("seed:tasty"), Some((200, 50)));
        assert_eq!(prices("seed:square"), Some((300, 80)));
        assert_eq!(prices("sa2-fruit:5"), Some((120, 40)));
        assert_eq!(prices("sa2-fruit:9"), Some((80, 20)));
        assert_eq!(prices("sa2-fruit:21"), Some((300, 50)));
        assert_eq!(prices("sa2-fruit:12"), None, "Smart fruit is never stocked");
        assert_eq!(prices("sa2-fruit:05"), None);
        assert!(removed_when_bought("sa2-fruit:20"));
        assert!(!removed_when_bought("seed:strong"));
    }

    #[test]
    fn draws_are_quantized_like_the_native_rand() {
        // k = 22937 is the last value that passes f < 0.7 (22938 of 32768).
        let mut r = Scripted(vec![22_937. / 32_768., 22_938. / 32_768.], 0);
        assert!(draw(&mut r) < 0.7);
        assert!(draw(&mut r) >= 0.7);
    }

    #[test]
    fn restock_follows_the_native_draw_order() {
        let mut m = Market::default();
        assert!(m.due(0));
        // eggs, Hero, Dark, Heart, Chao, two-of (0, then 0 again redraws, 2),
        // seed gate, Tasty/Hero/Dark pick, two-of (1, 2), mushroom alt, add, timer.
        let draws = [
            0.9, 0.1, 0.9, 0.2, 0.6, 0.0, 0.1, 0.9, 0.5, 0.4, 0.4, 0.7, 0.2, 0.5, 0.5,
        ];
        m.restock(100, &mut Scripted(draws.to_vec(), 0));
        assert_eq!(
            m.stock,
            [
                "sa2-fruit:5",
                "sa2-fruit:10",
                "sa2-fruit:7",
                "sa2-fruit:9",
                "seed:strong",
                "seed:hero",
                "seed:triangle",
                "seed:square",
                "sa2-fruit:21",
            ]
        );
        assert_eq!(m.restock_at, Some(100 + 10_800 + 7_200));
        assert!(!m.due(18_099) && m.due(18_100));
        assert!(m.valid());
    }

    #[test]
    fn chance_tests_are_strict_and_failed_gates_skip_their_pick() {
        let mut m = Market::default();
        // 0.5 is exactly k = 16384, and int(100 f) = 50 is not < 50.
        let draws = [
            0.9, 0.75, 0.75, 0.5, 0.5, 0.0, 0.5, 0.75, 0.0, 0.5, 0.9, 0.75, 0.0,
        ];
        m.restock(0, &mut Scripted(draws.to_vec(), 0));
        assert_eq!(
            m.stock,
            [
                "sa2-fruit:7",
                "sa2-fruit:8",
                "seed:strong",
                "seed:round",
                "seed:triangle"
            ]
        );
    }
}
