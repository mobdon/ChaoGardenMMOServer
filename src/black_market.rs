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
//! - Eggs (Kinder .rodata 0x990): 10 rows {A, B, id[3]}; a row is a candidate
//!   when emblems E >= A, with id[v], v = ((GardenID[0] + GardenID[1]) &
//!   0x7FFFFFFF) % 3; past B a draw f < 0.3 makes it the shiny id + 27 (10x
//!   price). Then k = int(5 f) + 1 eggs: all if n <= k, else k without
//!   replacement (pick int(n f), move the last into the gap), in ascending id.
//! - Hats (ChaoMain .rodata 0x16D0 + 16 id): ids 3-12 with E >= emblems. Five
//!   or fewer are all added. Otherwise picks int(n f) with replacement; a Red
//!   wool pick (12) draws r = int(31 f): r < 5 -> 14 if E >= 117, 5 <= r < 15
//!   -> 13 if E >= 107; the final id must meet its emblems and be new. Added in
//!   ascending id. Pacifier (15, 999 emblems) is never stocked.
//! - Themes: Omochao (E >= 100) and Amy (E >= 140), unowned, f < 0.1 each.
//!   Maria also needs a linked Game Boy Advance, which does not exist here,
//!   so it is never offered.
//!
//! The emblem count comes from [`crate::emblems`]; every native source is still
//! unported, so emblem-gated rows stay locked until a native stage awards one.
//! The shop stands at the existing hub shop point because this world has no
//! Kindergarten.
use crate::garden::{FRAME_MS, Rand, SEEDS};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MARKET_RULES: &str = "native-gc-black-market-v2";
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

/// Egg colours by plain egg id 1-13: (name, buy). Shiny id + 27 costs 10x.
pub const EGG_COLOURS: [(&str, u32); 13] = [
    ("Yellow", 500),
    ("White", 400),
    ("Brown", 800),
    ("Sky blue", 600),
    ("Pink", 600),
    ("Blue", 500),
    ("Grey", 1000),
    ("Green", 800),
    ("Red", 500),
    ("Lime green", 1500),
    ("Purple", 600),
    ("Orange", 600),
    ("Black", 2000),
];
pub const SHINY_OFFSET: u8 = 27;
/// Kinder .rodata 0x990: (A, B, id by GardenID column).
const EGG_ROWS: [(u32, u32, [u8; 3]); 10] = [
    (5, 40, [2, 2, 2]),
    (10, 50, [6, 9, 1]),
    (15, 60, [12, 4, 5]),
    (20, 70, [8, 3, 11]),
    (30, 80, [7, 7, 7]),
    (90, 140, [10, 10, 10]),
    (100, 150, [9, 1, 6]),
    (110, 160, [4, 5, 12]),
    (120, 170, [3, 11, 8]),
    (130, 180, [13, 13, 13]),
];
/// Hats 3-15: (id, name, buy, sell, emblems).
pub const HATS: [(u8, &str, u32, u32, u32); 13] = [
    (3, "Apple", 2000, 500, 7),
    (4, "Bucket", 6000, 1500, 77),
    (5, "Empty can", 4000, 1000, 47),
    (6, "Cardboard box", 2000, 500, 17),
    (7, "Flower pot", 6000, 1500, 67),
    (8, "Paper bag", 4000, 1000, 37),
    (9, "Pan", 2000, 500, 27),
    (10, "Stump", 4000, 1000, 57),
    (11, "Watermelon", 6000, 1500, 87),
    (12, "Red wool hat", 8000, 2000, 97),
    (13, "Blue wool hat", 10000, 2500, 107),
    (14, "Black wool hat", 20000, 5000, 117),
    (15, "Pacifier", 3000, 700, 999),
];
const RED_WOOL: u8 = 12;
const BLUE_WOOL: u8 = 13;
const BLACK_WOOL: u8 = 14;
/// Menu themes: (key, buy, emblems). Theme index = save flag order here.
pub const THEMES: [(&str, u32, u32); 3] = [
    ("omochao", 10000, 100),
    ("amy", 20000, 140),
    ("maria", 30000, 180),
];

pub fn hat(id: u8) -> Option<(u8, &'static str, u32, u32, u32)> {
    HATS.iter().copied().find(|h| h.0 == id)
}
fn hat_emblems(id: u8) -> u32 {
    hat(id).map_or(u32::MAX, |h| h.4)
}
/// Plain egg id 1-13 and shiny flag of an egg item id 1-13 / 28-40.
pub fn egg_colour(id: u8) -> Option<(u8, bool)> {
    match id {
        1..=13 => Some((id, false)),
        28..=40 => Some((id - SHINY_OFFSET, true)),
        _ => None,
    }
}
fn keyed(item_id: &str, prefix: &str) -> Option<u8> {
    let id = item_id.strip_prefix(prefix)?.parse::<u8>().ok()?;
    (item_id == format!("{prefix}{id}")).then_some(id)
}
pub fn egg_id(item_id: &str) -> Option<u8> {
    keyed(item_id, "egg:").filter(|id| egg_colour(*id).is_some())
}
pub fn hat_id(item_id: &str) -> Option<u8> {
    keyed(item_id, "hat:").filter(|id| hat(*id).is_some())
}
pub fn theme_index(item_id: &str) -> Option<usize> {
    let key = item_id.strip_prefix("theme:")?;
    THEMES.iter().position(|t| t.0 == key)
}

/// Native (buy, sell) price of a market item key, if it has one. A sell
/// price of 0 means it cannot be sold.
pub fn prices(item_id: &str) -> Option<(u32, u32)> {
    if let Some(family) = item_id.strip_prefix("seed:") {
        let id = SEEDS.iter().position(|s| *s == family)?;
        return Some(SEED_PRICES[id]);
    }
    if let Some(id) = egg_id(item_id) {
        let (plain, shiny) = egg_colour(id)?;
        let buy = EGG_COLOURS[usize::from(plain - 1)].1;
        return Some((if shiny { buy * 10 } else { buy }, 0));
    }
    if let Some(id) = hat_id(item_id) {
        let (_, _, buy, sell, _) = hat(id)?;
        return Some((buy, sell));
    }
    if let Some(i) = theme_index(item_id) {
        return Some((THEMES[i].1, 0));
    }
    let id = keyed(item_id, "sa2-fruit:")?;
    fruit_price(id)
}

/// Removed from stock when bought: eggs, hats, mushrooms and themes.
pub fn removed_when_bought(item_id: &str) -> bool {
    item_id == format!("sa2-fruit:{MUSHROOM}")
        || item_id == format!("sa2-fruit:{MUSHROOM_ALT}")
        || egg_id(item_id).is_some()
        || hat_id(item_id).is_some()
        || theme_index(item_id).is_some()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Market {
    pub stock: Vec<String>,
    /// Frame at or after which entering the shop rebuilds the stock. The
    /// native timer is not saved (it counts from power-on); here it is kept
    /// so a restart does not restock early. `None` restocks on first entry.
    pub restock_at: Option<u64>,
    /// GardenID[0..2], set once when the garden is created (native: OSGetTick
    /// and chRandomU32); picks the egg colour column.
    #[serde(default)]
    pub garden_id: Option<[u32; 2]>,
    /// Owned menu themes (Omochao, Amy, Maria); owned themes are not offered.
    #[serde(default)]
    pub themes: [bool; 3],
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

/// The hat step of the restock (Kinder 0x32F0-0x36FC), at most `room` picks.
fn pick_hats(emblems: u32, room: usize, rand: &mut dyn Rand) -> Vec<u8> {
    let candidates: Vec<u8> = (3..=RED_WOOL)
        .filter(|id| emblems >= hat_emblems(*id))
        .collect();
    if candidates.len() <= 5 {
        return candidates;
    }
    let mut hats: Vec<u8> = Vec::new();
    while hats.len() < 5 && hats.len() < room {
        let mut id = candidates[pick(rand, candidates.len())];
        if id == RED_WOOL {
            let r = (31. * draw(rand)) as u32;
            if r < 5 {
                if emblems >= hat_emblems(BLACK_WOOL) {
                    id = BLACK_WOOL;
                }
            } else if r < 15 && emblems >= hat_emblems(BLUE_WOOL) {
                id = BLUE_WOOL;
            }
        }
        if emblems >= hat_emblems(id) && !hats.contains(&id) {
            hats.push(id);
        }
    }
    hats.sort_unstable();
    hats
}

impl Market {
    pub fn valid(&self) -> bool {
        self.stock.len() <= STOCK_LIMIT && self.stock.iter().all(|i| prices(i).is_some())
    }
    pub fn due(&self, frame: u64) -> bool {
        self.restock_at.is_none_or(|at| frame >= at)
    }

    /// The egg colour column v.
    fn egg_column(&self) -> usize {
        let [a, b] = self.garden_id.unwrap_or_default();
        ((a.wrapping_add(b) & 0x7FFF_FFFF) % 3) as usize
    }

    /// Kinder fn 0x2D14 for `emblems` (the native emblem count), then the
    /// timer reset. The garden id must be set first.
    pub fn restock(&mut self, frame: u64, emblems: u32, rand: &mut dyn Rand) {
        let mut stock: Vec<String> = Vec::new();
        let room = |stock: &Vec<String>| stock.len() < STOCK_LIMIT;
        // Eggs.
        let column = self.egg_column();
        let mut eggs: Vec<u8> = Vec::new();
        for (a, b, ids) in EGG_ROWS {
            if emblems < a {
                continue;
            }
            let mut id = ids[column];
            if emblems >= b && draw(rand) < 0.3 {
                id += SHINY_OFFSET;
            }
            eggs.push(id);
        }
        let count = (5. * draw(rand)) as usize + 1;
        let mut chosen = if eggs.len() <= count {
            eggs
        } else {
            let mut pool = eggs;
            let mut taken = Vec::new();
            for _ in 0..count {
                let i = pick(rand, pool.len());
                taken.push(pool.swap_remove(i));
            }
            taken.sort_unstable();
            taken
        };
        chosen.truncate(STOCK_LIMIT);
        stock.extend(chosen.iter().map(|id| format!("egg:{id}")));
        let percent = |rand: &mut dyn Rand, p: u32| ((100. * draw(rand)) as u32) < p;
        for (fruit, chance) in [(HERO, 70), (DARK, 70), (HEART, 50), (CHAO, 50)] {
            if percent(rand, chance) && room(&stock) {
                stock.push(format!("sa2-fruit:{fruit}"));
            }
        }
        for i in two_of_three(rand) {
            if room(&stock) {
                stock.push(format!("sa2-fruit:{}", ROUND + i as u8));
            }
        }
        if room(&stock) {
            stock.push(format!("seed:{}", SEEDS[0]));
        }
        if draw(rand) < 0.7 {
            let seed = SEEDS[1 + pick(rand, 3)];
            if room(&stock) {
                stock.push(format!("seed:{seed}"));
            }
        }
        for i in two_of_three(rand) {
            if room(&stock) {
                stock.push(format!("seed:{}", SEEDS[4 + i]));
            }
        }
        let hats = pick_hats(emblems, STOCK_LIMIT.saturating_sub(stock.len()), rand);
        for id in hats {
            if room(&stock) {
                stock.push(format!("hat:{id}"));
            }
        }
        let alternate = draw(rand) < 0.3;
        if percent(rand, 70) && room(&stock) {
            let id = if alternate { MUSHROOM_ALT } else { MUSHROOM };
            stock.push(format!("sa2-fruit:{id}"));
        }
        // Omochao and Amy themes; Maria needs a linked GBA, absent here.
        for (i, (key, _, need)) in THEMES.iter().enumerate().take(2) {
            if !self.themes[i] && emblems >= *need && draw(rand) < 0.1 && room(&stock) {
                stock.push(format!("theme:{key}"));
            }
        }
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
            "location":"adapted-hub-shop-point","themesOwned":self.themes,
            "notPorted":["maria-theme-gba-link","kindergarten-door"]})
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
        m.restock(100, 0, &mut Scripted(draws.to_vec(), 0));
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
        m.restock(0, 0, &mut Scripted(draws.to_vec(), 0));
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

    #[test]
    fn eggs_use_the_garden_column_and_draw_without_replacement() {
        let mut m = Market {
            garden_id: Some([0, 1]),
            ..Market::default()
        };
        // 20 emblems: rows 0-3 in column 1 = eggs 2, 9, 4, 3 (no shiny rolls).
        // Count int(5 * 0.5) + 1 = 3 of 4: picks 0 (2), 0 (3 moved in), 1 (9).
        let draws = [
            0.5, 0.0, 0.0, 0.5, // eggs
            0.9, 0.9, 0.9, 0.9, 0.0, 0.5, // fruit
            0.9, 0.0, 0.5, // seeds
            0.9, 0.9, // mushroom
            0.0, // timer
        ];
        m.restock(0, 20, &mut Scripted(draws.to_vec(), 0));
        assert_eq!(
            m.stock,
            [
                "egg:2",
                "egg:3",
                "egg:9",
                "sa2-fruit:7",
                "sa2-fruit:8",
                "seed:strong",
                "seed:round",
                "seed:triangle",
                "hat:3",
                "hat:6",
            ],
            "two hats qualify at 20 emblems and are all added"
        );
        assert_eq!(prices("egg:9"), Some((500, 0)));
        assert_eq!(prices("egg:36"), Some((5000, 0)), "shiny red is 10x");
        assert!(removed_when_bought("egg:2") && removed_when_bought("hat:3"));
    }

    #[test]
    fn hat_picks_swap_red_wool_and_check_the_final_id() {
        // 117 emblems: candidates 3-12. Pick index 9 (Red wool), r = int(31 * 0.1) = 3
        // -> Black wool; pick 9 again, r = 10 -> Blue wool; pick 9, r = 20 -> Red
        // wool; then pick 0 (Apple) and 0 again (duplicate, rejected), then 1 (Bucket).
        // Draws are quantised to k / 32768, so 0.1 * 10 truncates to 0.
        let draws = [0.95, 0.1, 0.95, 0.35, 0.95, 0.66, 0.0, 0.0, 0.15];
        let hats = pick_hats(117, 32, &mut Scripted(draws.to_vec(), 0));
        assert_eq!(hats, [3, 4, 12, 13, 14]);
        // Below 107 emblems a swap fails and the pick stays Red wool.
        let hats = pick_hats(
            100,
            32,
            &mut Scripted(vec![0.95, 0.4, 0.0, 0.15, 0.25, 0.35], 0),
        );
        assert_eq!(hats, [3, 4, 5, 6, 12]);
    }
}
