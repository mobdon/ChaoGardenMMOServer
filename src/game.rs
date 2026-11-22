//! Deterministic, server-owned gameplay. Incoming messages carry intent only.
use crate::activities::{self, ActivityCatalog, ActivityRecord, KarateBout};
use crate::competition::{self, CompetitionCatalog, CompetitionState};
use crate::phone::{self, TinyRecord, TinyRun};
use crate::watch::{self, WatchState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub const MAIN_STATS: [&str; 5] = ["swim", "fly", "run", "power", "stamina"];
pub const ALL_STATS: [&str; 7] = [
    "swim",
    "fly",
    "run",
    "power",
    "stamina",
    "luck",
    "intelligence",
];
/// Rooms for the game's own Chao World areas (ChaoMain area numbers 0 lobby,
/// 1 Neutral, 2 Hero, 3 Dark, 4 race, 5 race entrance, 6 Kindergarten,
/// 9 Stadium, 10 Karate). The lobby is the MMO hub.
pub const CHAO_WORLD_ZONES: [&str; 9] = [
    "hub",
    "garden",
    "hero",
    "dark",
    "chao-race",
    "race-entrance",
    "kindergarten",
    "stadium",
    "chao-karate",
];
pub const TERRAIN_ZONES: [&str; 4] = ["hub", "garden", "hero", "dark"];
pub const RACE_CHECKPOINTS: [(f64, f64); 5] =
    [(0., 12.), (-12., 0.), (0., -12.), (12., 0.), (0., 12.)];
const MECH_FIRE_RANGE: f64 = 9.;
const MECH_FIRE_DAMAGE: f64 = 60.;
const MECH_FIRE_COOLDOWN: f64 = 350.;
const MECH_BOOST_HEIGHT: f64 = 3.;
const MECH_BOOST_SPEED: f64 = 1.8;
/// The Metal Sonic Chao's top speed: his physics row 12 SpeedMaxH (3.0 game units per frame, Steam
/// PhysicsArray 0x17391E8) x 60 frames x one game unit in this world (0.3866 Chao-model units, from
/// scripts/metal-chao/skin.py, x the client's 0.1 Chao visual scale).
const METAL_GAME_UNIT: f64 = 0.3866 * 0.1;
const METAL_CHAO_TOP_SPEED: f64 = 3.0 * 60. * METAL_GAME_UNIT;
/// His row 12 jmp_y_spd (2.52 units/frame) and weight, the per-frame gravity (0.08), in world units.
const METAL_CHAO_JUMP_SPEED: f64 = 2.52 * 60. * METAL_GAME_UNIT;
const METAL_CHAO_GRAVITY: f64 = 0.08 * 3600. * METAL_GAME_UNIT;
const MECH_BOOST_DRAIN: f64 = 40.;
const MECH_BOOST_RECOVERY: f64 = 25.;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Stat {
    pub grade: u16,
    pub level: u16,
    pub points: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    pub dx: f64,
    pub dz: f64,
    pub jump: bool,
    pub boost: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Gear {
    pub footwear: Option<String>,
    pub hands: Option<String>,
    /// Owner-only whole look: "metal-sonic-chao" (Metal Sonic's own parts on the Chao).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Race {
    pub started_at: f64,
    pub next_checkpoint: usize,
    pub course_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Player {
    /// Preserve future/unmodeled save metadata through care's additive migration.
    pub(crate) profile_extra: Value,
    pub id: String,
    pub token_hash: String,
    /// The memory card this Chao is on (hash of the card code). None: a Chao
    /// from before cards, whose own token is its card code.
    pub card_hash: Option<String>,
    pub name: String,
    pub admin: bool,
    pub stats: BTreeMap<String, Stat>,
    pub rings: u32,
    pub alignment: f64,
    pub form: String,
    pub owned_cosmetics: Vec<String>,
    pub equipped: Option<String>,
    pub vehicle: Option<String>,
    pub gear: Gear,
    pub race_records: BTreeMap<String, ActivityRecord>,
    pub karate_records: BTreeMap<String, ActivityRecord>,
    pub karate: Option<KarateBout>,
    pub activity_result: Option<Value>,
    pub tiny_records: BTreeMap<String, TinyRecord>,
    pub tiny_game: Option<TinyRun>,
    /// The player's Chao as its game draws it in a Chao World area (draw-only
    /// for others; not saved).
    pub chao_pose: Value,
    pub tiny_result: Option<Value>,
    pub watch: Option<WatchState>,
    pub care: crate::care::CareState,
    pub garden: crate::garden::GardenState,
    pub body: crate::body::BodyState,
    pub life: crate::life::LifeState,
    pub competition: CompetitionState,
    pub activity_lease: Option<Value>,
    pub tiny_collection: Option<crate::collection::CollectionState>,
    pub boosting: bool,
    pub boost_energy: f64,
    pub happiness: f64,
    pub energy: f64,
    pub hunger: f64,
    pub hp: f64,
    pub max_hp: f64,
    pub missions_completed: u32,
    pub races_completed: u32,
    pub best_race_ms: Option<f64>,
    pub x: f64,
    pub z: f64,
    pub y: f64,
    pub ground_y: f64,
    pub jump_offset: f64,
    pub vy: f64,
    pub facing: f64,
    pub zone: String,
    pub connections: usize,
    pub input: Input,
    pub last_input_at: f64,
    pub cooldowns: BTreeMap<String, f64>,
    pub race: Option<Race>,
}

impl Player {
    /// The memory card this Chao is on: keys its game save and lease.
    pub fn card(&self) -> &str {
        self.card_hash.as_deref().unwrap_or(&self.token_hash)
    }
    pub(crate) fn cooldown(&mut self, name: &str, now: f64, duration: f64) -> bool {
        if self.cooldowns.get(name).is_some_and(|until| *until > now) {
            return false;
        }
        self.cooldowns.insert(name.into(), now + duration);
        true
    }
    pub(crate) fn update_form(&mut self) {
        // APos is the alignment authority; percent alignment is its projection.
        self.alignment = self
            .body
            .chaos_alignment()
            .unwrap_or_else(|| self.body.alignment_percent());
        if self.body.is_chaos() {
            self.form = "chaos".into();
        } else {
            self.form = if self.alignment >= 60. {
                "hero"
            } else if self.alignment <= -60. {
                "dark"
            } else {
                "normal"
            }
            .into();
        }
    }
    pub(crate) fn gain(&mut self, name: &str, levels: u16) {
        if let Some(stat) = self.stats.get_mut(name) {
            stat.level = (stat.level + levels).min(99);
            stat.points = (stat.points + u32::from(levels) * 25).min(9999);
            if MAIN_STATS.contains(&name) && !self.admin {
                stat.grade = (1 + stat.level / 20).min(5);
            }
        }
    }
    pub fn evolution(&self) -> Value {
        let checks = [
            ("missions", f64::from(self.missions_completed), 3.),
            ("races", f64::from(self.races_completed), 1.),
            ("run", f64::from(self.stats["run"].level), 10.),
            ("swim", f64::from(self.stats["swim"].level), 10.),
            ("power", f64::from(self.stats["power"].level), 10.),
            ("happiness", self.happiness, 80.),
        ];
        let requirements: BTreeMap<_, _> = checks
            .iter()
            .map(|(key, current, required)| {
                (*key, json!({"current": current, "required": required}))
            })
            .collect();
        json!({"ready": checks.iter().all(|(_, current, required)| current >= required), "requirements": requirements,
            "rules":"adapted-prototype-evolution-v1","nativeConditions":"unknown"})
    }
    pub fn public(&self) -> Value {
        json!({
            "id": self.id, "name": self.name, "x": self.x, "y": self.y, "z": self.z,
            "groundY": self.ground_y, "jumpOffset": self.jump_offset, "facing": self.facing,
            "zone": self.zone, "alignment": self.alignment, "form": self.form, "admin": self.admin,
            "stats": self.stats, "ownedCosmetics": self.owned_cosmetics, "equipped": self.equipped, "vehicle": self.vehicle, "gear": self.gear,
            "raceRecords": self.race_records, "karateRecords": self.karate_records,
            "tinyRecords": self.tiny_records,
            "competition":competition::public(self),
            "watch":self.watch.as_ref().map(|watch|watch.public(self.connections>0)),"connected":self.connections>0,
            "mech": if self.vehicle.as_deref() == Some("dark-chao-walker") { json!({"boosting": self.boosting, "boostEnergy": self.boost_energy.round(), "grounded": self.jump_offset <= 0. && self.vy <= 0.}) } else { Value::Null },
            "rings": self.rings, "hp": self.hp, "maxHp": self.max_hp, "happiness": self.happiness,
            "energy": self.energy.round(), "hunger": self.hunger.round(),
            "missionsCompleted": self.missions_completed, "racesCompleted": self.races_completed,
            "bestRaceMs": self.best_race_ms, "evolution": self.evolution(), "body": self.body.public(),
            "life": crate::life::public_summary(self),
            "animalParts": self.care.animal_parts, "hat": self.care.hat,
            "chaoPose": if self.connections > 0 { self.chao_pose.clone() } else { Value::Null },
        })
    }
    pub fn profile(&self) -> Value {
        let mut profile = json!({"id": self.id, "tokenHash": self.token_hash, "cardHash": self.card_hash, "name": self.name, "admin": self.admin,
            "stats": self.stats, "rings": self.rings, "alignment": self.alignment, "form": self.form,
            "happiness": self.happiness, "ownedCosmetics": self.owned_cosmetics, "equipped": self.equipped, "vehicle": self.vehicle, "gear": self.gear,
            "energy":self.energy,"hunger":self.hunger,"care":self.care,"body":self.body,"life":self.life,"garden":self.garden,"tinyCollection":self.tiny_collection,
            "raceRecords": self.race_records, "karateRecords": self.karate_records,
            "tinyRecords": self.tiny_records,
            "resident":self.watch.as_ref().map(|_|json!({"enabled":true,"zone":self.zone,"x":self.x,"z":self.z})),
            "missionsCompleted": self.missions_completed, "racesCompleted": self.races_completed, "bestRaceMs": self.best_race_ms});
        phone::project_profile(self, &mut profile);
        competition::project_profile(self, &mut profile);
        let mut preserved = self.profile_extra.clone();
        merge_profile(&mut preserved, profile);
        preserved
    }
}

// Canonical known values win; unmodeled object fields survive a round trip.
fn merge_profile(preserved: &mut Value, canonical: Value) {
    match (preserved, canonical) {
        (Value::Object(existing), Value::Object(current)) => {
            for (key, value) in current {
                // Strict/normalized ledgers are complete canonical replacements.
                // Merging an empty inventory would resurrect consumed stacks.
                if [
                    "care",
                    "garden",
                    "body",
                    "life",
                    "competition",
                    "activityLease",
                    "tinyCollection",
                    "tinyRecords",
                    "raceRecords",
                    "karateRecords",
                ]
                .contains(&key.as_str())
                {
                    existing.insert(key, value);
                } else if let Some(previous) = existing.get_mut(&key) {
                    merge_profile(previous, value);
                } else {
                    existing.insert(key, value);
                }
            }
        }
        (previous, current) => *previous = current,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Terrain {
    pub min_x: f64,
    pub min_z: f64,
    pub step: f64,
    pub width: usize,
    pub depth: usize,
    pub heights: Vec<Option<f64>>,
}
impl Terrain {
    pub fn validate(&self) -> Result<(), String> {
        if !self.min_x.is_finite()
            || !self.min_z.is_finite()
            || !self.step.is_finite()
            || self.step <= 0.
            || self.width == 0
            || self.depth == 0
            || self.width > 4096
            || self.depth > 4096
            || self.heights.len() != self.width * self.depth
            || !self.heights.iter().any(Option::is_some)
            || self
                .heights
                .iter()
                .flatten()
                .any(|height| !height.is_finite())
        {
            return Err("Invalid authoritative terrain".into());
        }
        Ok(())
    }
    pub fn sample(&self, x: f64, z: f64) -> Option<f64> {
        let gx = (x - self.min_x) / self.step;
        let gz = (z - self.min_z) / self.step;
        if !gx.is_finite()
            || !gz.is_finite()
            || gx < 0.
            || gz < 0.
            || gx > (self.width - 1) as f64
            || gz > (self.depth - 1) as f64
        {
            return None;
        }
        let at = |column: usize, row: usize| self.heights[row * self.width + column];
        let nearest = at(gx.round() as usize, gz.round() as usize)?;
        let x0 = gx.floor() as usize;
        let z0 = gz.floor() as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let z1 = (z0 + 1).min(self.depth - 1);
        let corners = [at(x0, z0), at(x1, z0), at(x0, z1), at(x1, z1)];
        if corners.iter().any(Option::is_none) {
            return Some(nearest);
        }
        let c = corners.map(Option::unwrap);
        if c.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - c.iter().copied().fold(f64::INFINITY, f64::min)
            > 0.75
        {
            return Some(nearest);
        }
        let tx = gx - x0 as f64;
        let tz = gz - z0 as f64;
        Some((c[0] * (1. - tx) + c[1] * tx) * (1. - tz) + (c[2] * (1. - tx) + c[3] * tx) * tz)
    }
}

#[derive(Clone, Debug)]
pub struct Ring {
    pub id: String,
    pub x: f64,
    pub z: f64,
    pub zone: String,
    pub available_at: f64,
}
#[derive(Clone, Debug)]
pub struct Enemy {
    pub id: String,
    pub x: f64,
    pub z: f64,
    pub hp: f64,
    pub max_hp: f64,
    pub last_attack_at: f64,
}
#[derive(Clone, Debug)]
pub struct Mission {
    pub id: String,
    pub completed: bool,
    pub members: BTreeSet<String>,
    pub rewarded: BTreeSet<String>,
    pub enemies: Vec<Enemy>,
    pub rings: Vec<Ring>,
}
fn mutes(listener: &Player, speaker: &str) -> bool {
    listener.profile_extra["muted"]
        .as_array()
        .is_some_and(|l| l.iter().any(|v| v == speaker))
}
/// Game units around a Chao within which others hear its voice.
const VOICE_RADIUS: f64 = 150.;
#[derive(Clone, Debug)]
pub struct Event {
    pub target: Option<String>,
    pub message: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShopItem {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub price: u32,
    pub r#type: &'static str,
}
pub const SHOP_CATALOG: [ShopItem; 1] = [ShopItem {
    id: "eggshell-cap",
    name: "Eggshell Cap",
    description: "Own and equip a little eggshell hat.",
    price: 25,
    r#type: "cosmetic",
}];

pub struct Game {
    pub care_catalog: crate::care_catalog::CareCatalog,
    pub native_tiny: crate::native_tiny::Sources,
    pub tiny_shop_catalog: Option<tiny_garden_core::shop::TinyShopCatalog>,
    pub tiny_egg_source: Option<crate::collection::EggSource>,
    pub activities: ActivityCatalog,
    pub competition_catalog: CompetitionCatalog,
    pub life_policy: crate::life::Policy,
    /// Local native Chao data (never committed); empty unless installed.
    pub native_chao: crate::behaviour::NativeData,
    pub time: f64,
    pub allow_owner_bootstrap: bool,
    pub terrains: BTreeMap<String, Terrain>,
    pub players: BTreeMap<String, Player>,
    /// Chao swapped back to their memory card while still connected; their sockets close.
    pub benched: BTreeSet<String>,
    pub tokens: BTreeMap<String, String>,
    pub events: Vec<Event>,
    pub dirty: bool,
    pub tiny_save_retry_at: f64,
    pub mission: Option<Mission>,
    pub rings: Vec<Ring>,
    /// Decision-making time only; terrain/movement work remains in total tick time.
    pub last_resident_tick: Duration,
    pub last_tick_residents: usize,
    /// Games holding a player's native save (in memory: a restart frees them).
    pub game_save_leases: BTreeMap<String, crate::game_save::Lease>,
}

pub fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
/// A memory card holds a garden's worth of Chao, as the game's own save does.
pub const CARD_CHAO: usize = 8;
/// The game's own save keeps 7 bytes of a Chao's name (Chao record +0x12).
pub const NAME_MAX: usize = 7;
/// A Chao's session token on a card, recomputable from the card code alone.
fn chao_token(card: &str, id: &str) -> String {
    hash_token(&format!("chao:{card}:{id}"))
}
fn clean_name(value: Option<&str>) -> String {
    let value: String = value
        .unwrap_or("Chao")
        .chars()
        .filter(|c| !c.is_control() && *c != '<' && *c != '>')
        .collect();
    let value: String = value.trim().chars().take(20).collect();
    if value.is_empty() {
        "Chao".into()
    } else {
        value
    }
}
/// A new or changed name: what the game's own save can hold.
fn short_name(value: Option<&str>) -> String {
    let name: String = clean_name(value).chars().take(NAME_MAX).collect();
    name.trim_end().to_owned()
}
fn number(value: &Value, default: f64, min: f64, max: f64) -> f64 {
    value
        .as_f64()
        .filter(|x| x.is_finite())
        .unwrap_or(default)
        .clamp(min, max)
}
fn dist(x: f64, z: f64, x2: f64, z2: f64) -> f64 {
    (x - x2).hypot(z - z2)
}

impl Game {
    pub fn new(
        profiles: Vec<Value>,
        terrains: BTreeMap<String, Terrain>,
        allow_owner_bootstrap: bool,
        now: f64,
    ) -> Result<Self, String> {
        for (zone, terrain) in &terrains {
            if !TERRAIN_ZONES.contains(&zone.as_str()) {
                return Err(format!("Unsupported terrain zone: {zone}"));
            }
            terrain
                .validate()
                .map_err(|error| format!("{error} for {zone}"))?;
        }
        let mut game = Self {
            care_catalog: crate::care_catalog::CareCatalog::default(),
            native_tiny: Default::default(),
            tiny_shop_catalog: None,
            tiny_egg_source: None,
            activities: ActivityCatalog::default(),
            competition_catalog: CompetitionCatalog::default(),
            life_policy: crate::life::Policy::default(),
            native_chao: crate::behaviour::NativeData::default(),
            time: now,
            allow_owner_bootstrap,
            terrains,
            players: BTreeMap::new(),
            benched: BTreeSet::new(),
            tokens: BTreeMap::new(),
            events: vec![],
            dirty: false,
            tiny_save_retry_at: 0.,
            mission: None,
            rings: vec![],
            last_resident_tick: Duration::ZERO,
            last_tick_residents: 0,
            game_save_leases: BTreeMap::new(),
        };
        for (i, (x, z)) in [(-6., 6.), (6., 6.), (-6., -6.), (6., -6.), (0., -9.)]
            .into_iter()
            .enumerate()
        {
            let (x, z, _) = game.safe_position("garden", x, z)?;
            game.rings.push(Ring {
                id: format!("garden-ring-{i}"),
                x,
                z,
                zone: "garden".into(),
                available_at: 0.,
            });
        }
        for profile in profiles {
            if profile["id"].as_str().is_none() || profile["tokenHash"].as_str().is_none() {
                continue;
            }
            let player = game.make_player(&profile)?;
            game.tokens
                .insert(player.token_hash.clone(), player.id.clone());
            game.players.insert(player.id.clone(), player);
        }
        Ok(game)
    }

    fn make_player(&self, data: &Value) -> Result<Player, String> {
        let admin = data["admin"].as_bool().unwrap_or(false);
        let stats = ALL_STATS
            .into_iter()
            .map(|name| {
                let grade_max = if MAIN_STATS.contains(&name) { 5. } else { 255. };
                let stat = if admin {
                    Stat {
                        grade: grade_max as u16,
                        level: 99,
                        points: 9999,
                    }
                } else {
                    Stat {
                        grade: number(&data["stats"][name]["grade"], 1., 0., grade_max).floor()
                            as u16,
                        level: number(&data["stats"][name]["level"], 1., 0., 99.).floor() as u16,
                        points: number(&data["stats"][name]["points"], 0., 0., 9999.).floor()
                            as u32,
                    }
                };
                (name.into(), stat)
            })
            .collect();
        let owned_cosmetics = if data["ownedCosmetics"]
            .as_array()
            .is_some_and(|list| list.iter().any(|item| item == "eggshell"))
        {
            vec!["eggshell".into()]
        } else {
            vec![]
        };
        let equipped = if !owned_cosmetics.is_empty() && data["equipped"] == "eggshell" {
            Some("eggshell".into())
        } else {
            None
        };
        let mut player = Player {
            profile_extra: data.clone(),
            id: data["id"].as_str().unwrap_or_default().into(),
            token_hash: data["tokenHash"].as_str().unwrap_or_default().into(),
            card_hash: data["cardHash"].as_str().map(Into::into),
            name: clean_name(data["name"].as_str()),
            admin,
            stats,
            rings: number(&data["rings"], 20., 0., 999999.).floor() as u32,
            alignment: number(&data["alignment"], 0., -100., 100.),
            form: if data["form"] == "chaos" {
                "chaos"
            } else {
                "normal"
            }
            .into(),
            owned_cosmetics,
            equipped,
            vehicle: if admin && data["vehicle"] == "dark-chao-walker" {
                Some("dark-chao-walker".into())
            } else {
                None
            },
            boosting: false,
            boost_energy: 100.,
            gear: Gear {
                footwear: if admin && data["gear"]["footwear"] == "shadow-air-shoes" {
                    Some("shadow-air-shoes".into())
                } else {
                    None
                },
                hands: if admin && data["gear"]["hands"] == "knuckles-hammer-gloves" {
                    Some("knuckles-hammer-gloves".into())
                } else {
                    None
                },
                look: (admin && data["gear"]["look"] == "metal-sonic-chao")
                    .then(|| "metal-sonic-chao".into()),
            },
            race_records: activities::restore_records(&data["raceRecords"]),
            karate_records: activities::restore_records(&data["karateRecords"]),
            karate: None,
            activity_result: None,
            tiny_records: phone::restore_records(&data["tinyRecords"]),
            tiny_game: None,
            chao_pose: Value::Null,
            tiny_result: None,
            watch: None,
            care: crate::care::CareState::restore(data.get("care"))?,
            garden: crate::garden::GardenState::restore(data.get("garden"))?,
            body: crate::body::BodyState::restore(
                data.get("body"),
                data["form"].as_str().unwrap_or("normal"),
                number(&data["alignment"], 0., -100., 100.),
            )?,
            life: crate::life::LifeState::legacy(
                "",
                "normal",
                [0; 7],
                crate::life::Policy::default(),
            ),
            competition: competition::restore(
                &data["competition"],
                data["id"].as_str().unwrap_or_default(),
            )?,
            activity_lease: data.get("activityLease").filter(|v| !v.is_null()).cloned(),
            tiny_collection: crate::collection::CollectionState::restore(
                data.get("tinyCollection"),
            )?,
            happiness: number(&data["happiness"], 65., 0., 100.),
            energy: number(&data["energy"], 100., 0., 100.),
            hunger: number(&data["hunger"], 100., 0., 100.),
            hp: if admin { 250. } else { 100. },
            max_hp: if admin { 250. } else { 100. },
            missions_completed: number(&data["missionsCompleted"], 0., 0., 99999.).floor() as u32,
            races_completed: number(&data["racesCompleted"], 0., 0., 99999.).floor() as u32,
            best_race_ms: data["bestRaceMs"]
                .as_f64()
                .filter(|x| x.is_finite() && *x > 0.),
            x: 0.,
            z: 6.,
            y: 0.,
            ground_y: 0.,
            jump_offset: 0.,
            vy: 0.,
            facing: 0.,
            zone: "hub".into(),
            connections: 0,
            input: Input::default(),
            last_input_at: 0.,
            cooldowns: BTreeMap::new(),
            race: None,
        };
        self.teleport(&mut player, "hub", 0., 6.)?;
        // The saved percent alignment stays the legacy contract: if something wrote it
        // without the body (older tools, admin edits), the body follows it.
        if (player.alignment - player.body.alignment_percent()).abs() >= 0.5 {
            player.body.set_alignment_percent(player.alignment);
        }
        crate::training::validate_player(&player)?;
        crate::collection::validate_player(&player)?;
        player.update_form();
        player.life = crate::life::restore_for(
            data,
            &player.id,
            &player.form,
            &player.stats,
            self.life_policy,
        )?;
        watch::restore(self, &mut player, &data["resident"]);
        if let Some(plan) = &player.competition.plan {
            let expected = competition::lease(plan);
            if player
                .activity_lease
                .as_ref()
                .is_some_and(|lease| *lease != expected)
            {
                return Err("Competition conflicts with saved activity lease".into());
            }
            player.activity_lease = Some(expected);
        }
        watch::maintain_tiny(self, &mut player);
        Ok(player)
    }

    pub fn create_session(&mut self, input: &Value) -> Result<Value, String> {
        if let Some(token) = input["token"].as_str().filter(|token| token.len() <= 256)
            && let Some(player) = self.player_for_token(token)
        {
            return Ok(json!({"token": token, "player": player.public()}));
        }
        if input["resumeOnly"] == true {
            return Err("Saved Chao was not found.".into());
        }
        // A new Chao joins the card it was made with, or starts a new card.
        let card = match input["card"].as_str() {
            Some(card) if !self.card_chao(card).is_empty() => card.to_owned(),
            Some(_) => return Err("Memory card was not found.".into()),
            None => format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        };
        if self.card_chao(&card).len() >= CARD_CHAO {
            return Err("This memory card already holds 8 Chao.".into());
        }
        // An egg picked from a Chao's inventory, or a bred egg from its nest, hatches into
        // the new slot (MMO glue).
        let egg = match &input["egg"] {
            Value::Null => None,
            egg => {
                let parent = egg["from"]
                    .as_str()
                    .and_then(|token| self.player_for_token(token))
                    .filter(|p| p.card() == hash_token(&card))
                    .ok_or("That egg isn't on this memory card.")?;
                // MMO rule: eggs hatch in the hub.
                if parent.zone != "hub" {
                    return Err("Eggs hatch in the hub.".into());
                }
                if let Some(nest) = egg["eggId"].as_str() {
                    if !parent.life.nest.iter().any(|e| e.id == nest) {
                        return Err("That egg isn't in the nest any more.".into());
                    }
                    Some((parent.id.clone(), nest.to_owned(), None))
                } else {
                    let item = egg["itemId"].as_str().unwrap_or_default();
                    let egg_id = crate::black_market::egg_id(item).ok_or("That isn't an egg.")?;
                    if parent.care.inventory.get(item).copied().unwrap_or(0) == 0 {
                        return Err("That egg isn't carried any more.".into());
                    }
                    Some((parent.id.clone(), item.to_owned(), Some(egg_id)))
                }
            }
        };
        let id = Uuid::new_v4().to_string();
        let token = chao_token(&card, &id);
        let name: String = short_name(input["name"].as_str());
        let player = self.make_player(&json!({"id": id, "tokenHash": hash_token(&token), "cardHash": hash_token(&card), "name": name, "admin": self.allow_owner_bootstrap && self.players.is_empty()}))?;
        let mut player = player;
        match egg {
            Some((parent, item, Some(egg_id))) => {
                crate::life::hatch_bought_egg(&mut player, egg_id, self.time, self.life_policy)?;
                let parent = self.players.get_mut(&parent).expect("checked");
                let left = parent.care.inventory.get(&item).copied().unwrap_or(0) - 1;
                if left == 0 {
                    parent.care.inventory.remove(&item);
                } else {
                    parent.care.inventory.insert(item, left);
                }
                parent.care.revision += 1;
            }
            Some((parent, nest, None)) => {
                let parent = self.players.get_mut(&parent).expect("checked");
                let index = parent
                    .life
                    .nest
                    .iter()
                    .position(|e| e.id == nest)
                    .expect("checked");
                let egg = parent.life.nest.remove(index);
                parent.life.revision = parent.life.revision.saturating_add(1);
                crate::life::hatch(&mut player, egg, self.life_policy);
            }
            None => {}
        }
        let response = json!({"token": token, "card": card, "player": player.public()});
        self.tokens
            .insert(player.token_hash.clone(), player.id.clone());
        self.players.insert(player.id.clone(), player);
        self.dirty = true;
        Ok(response)
    }
    /// The Chao on the card whose code is `card`.
    pub fn card_chao(&self, card: &str) -> Vec<&Player> {
        if card.len() > 256 {
            return Vec::new();
        }
        let card_hash = hash_token(card);
        self.players
            .values()
            .filter(|p| p.card() == card_hash)
            .collect()
    }
    /// Each Chao on the card with the token that opens it.
    pub fn card_sessions(&self, card: &str) -> Vec<Value> {
        self.card_chao(card)
            .into_iter()
            .map(|p| {
                let token = if p.card_hash.is_none() {
                    card.to_owned()
                } else {
                    chao_token(card, &p.id)
                };
                json!({"token": token, "player": p.public()})
            })
            .collect()
    }
    /// The card a bearer credential (a Chao's token or a card code) opens.
    pub fn card_for(&self, credential: &str) -> Option<String> {
        if let Some(p) = self.player_for_token(credential) {
            return Some(p.card().to_owned());
        }
        (!self.card_chao(credential).is_empty()).then(|| hash_token(credential))
    }
    /// Local owner login (loopback + CHAO_OWNER_BOOTSTRAP only): a fresh token for the admin Chao.
    pub fn owner_token(&mut self) -> Option<String> {
        let id = self.players.values().find(|p| p.admin)?.id.clone();
        let token = hash_token(&Uuid::new_v4().to_string());
        let player = self.players.get_mut(&id)?;
        self.tokens.remove(&player.token_hash);
        player.token_hash = hash_token(&token);
        self.tokens.insert(player.token_hash.clone(), id);
        self.dirty = true;
        Some(token)
    }
    pub fn player_for_token(&self, token: &str) -> Option<&Player> {
        if token.len() > 256 {
            return None;
        }
        self.tokens
            .get(&hash_token(token))
            .and_then(|id| self.players.get(id))
    }
    pub fn connect(&mut self, id: &str) -> bool {
        // MMO rule: only the selected Chao of a memory card is out, in the garden,
        // the hub or Tiny Garden. Selecting another swaps it in; the rest go back to the card.
        if let Some(card) = self.players.get(id).map(|p| p.card().to_owned()) {
            self.benched.remove(id);
            for other in self
                .players
                .values_mut()
                .filter(|p| p.id != id && p.card() == card)
            {
                if other.watch.take().is_some() {
                    self.dirty = true;
                }
                if other.connections > 0 {
                    self.benched.insert(other.id.clone());
                }
            }
        }
        if let Some(player) = self.players.get_mut(id) {
            player.connections += 1;
            player.input = Input::default();
            if player.garden.groves.is_empty() {
                crate::care::seed_groves(player, self.time);
                self.dirty = true;
            }
            true
        } else {
            false
        }
    }
    /// MMO rule: a Chao back online from Tiny Chao Garden brings its egg along; it goes
    /// into the inventory (the nest) as the GameCube makes it, `AL_GeneCreateGBAEgg`. A full
    /// nest leaves it in Tiny Garden until there's room.
    pub fn bring_tiny_egg(&mut self, id: &str) -> Option<String> {
        let now = self.time;
        let p = self.players.get_mut(id)?;
        let v = p
            .tiny_collection
            .as_ref()?
            .pending_egg
            .as_ref()?
            .native_egg_id;
        let egg = crate::life::lay_gba_egg(p, v, now).ok()?;
        p.tiny_collection.as_mut()?.pending_egg = None;
        self.dirty = true;
        Some(egg)
    }
    pub fn disconnect(&mut self, id: &str) {
        if let Some(mut player) = self.players.remove(id) {
            player.connections = player.connections.saturating_sub(1);
            if let Some(run) = &mut player.tiny_game {
                run.release_controls();
            }
            if player.connections == 0 && player.care.tiny_resident {
                if !player.tiny_game.as_ref().is_some_and(TinyRun::saving) {
                    player.tiny_game = None;
                }
                watch::maintain_tiny(self, &mut player);
            } else if player.connections == 0
                && player.watch.is_none()
                && !competition::reserved(&player)
            {
                let _ = self.teleport(&mut player, "hub", 0., 6.);
                if !player.tiny_game.as_ref().is_some_and(TinyRun::saving) {
                    player.tiny_game = None;
                }
            }
            self.players.insert(id.into(), player);
        }
    }
    pub fn set_input(&mut self, id: &str, input: &Value) {
        if let Some(player) = self.players.get_mut(id).filter(|p| p.connections > 0) {
            if player.tiny_game.is_some()
                || player.watch.is_some()
                || competition::training_conflict(player)
                || competition::reserved(player)
                || player.care.tiny_resident
                || player.care.activity_lock.is_some()
            {
                return;
            }
            let mut dx = number(&input["dx"], 0., -1., 1.);
            let mut dz = number(&input["dz"], 0., -1., 1.);
            let length = dx.hypot(dz);
            if length > 1. {
                dx /= length;
                dz /= length;
            }
            player.input = Input {
                dx,
                dz,
                jump: input["jump"] == true,
                boost: input["boost"] == true,
            };
            player.last_input_at = self.time;
        }
    }
    pub fn request_save(&mut self, id: &str) -> bool {
        let Some(player) = self.players.get_mut(id).filter(|p| p.connections > 0) else {
            return false;
        };
        if !player.cooldown("save", self.time, 300.) {
            return false;
        }
        self.dirty = true;
        true
    }
    pub fn tiny_input(&mut self, id: &str, message: &Value) {
        if self
            .players
            .get(id)
            .is_some_and(|p| competition::reserved(p) || competition::training_conflict(p))
        {
            return;
        }
        let Some(mut player) = self.players.remove(id) else {
            return;
        };
        if player.connections > 0 {
            phone::input(self, &mut player, message);
        }
        self.players.insert(id.to_owned(), player);
    }
    pub fn notice(&mut self, id: &str, text: impl Into<String>, kind: &str) {
        self.events.push(Event {
            target: Some(id.into()),
            message: json!({"type": "notice", "text": text.into(), "kind": kind}),
        });
    }
    pub fn sample_ground(&self, zone: &str, x: f64, z: f64) -> Option<f64> {
        if zone == "karate" {
            return Some(0.75);
        }
        self.terrains
            .get(zone)
            .map_or(Some(0.), |terrain| terrain.sample(x, z))
    }
    pub fn player_terrain(&self, player: &Player) -> Option<&Terrain> {
        if let Some(course) = activities::native_course(self, player) {
            self.activities
                .terrains
                .get(course.world_id.as_deref().unwrap_or(&course.id))
        } else {
            self.terrains.get(&player.zone)
        }
    }
    pub fn player_ground(&self, player: &Player, x: f64, z: f64) -> Option<f64> {
        if player.zone == "karate" {
            return Some(0.75);
        }
        self.player_terrain(player)
            .map_or(Some(0.), |terrain| terrain.sample(x, z))
    }
    pub fn safe_position(&self, zone: &str, x: f64, z: f64) -> Result<(f64, f64, f64), String> {
        let x = x.clamp(-22., 22.);
        let z = z.clamp(-22., 22.);
        if let Some(y) = self.sample_ground(zone, x, z) {
            return Ok((x, z, y));
        }
        let terrain = self.terrains.get(zone).ok_or("No terrain")?;
        let mut nearest = None;
        let mut best = f64::INFINITY;
        for row in 0..terrain.depth {
            for column in 0..terrain.width {
                let Some(y) = terrain.heights[row * terrain.width + column] else {
                    continue;
                };
                let px = terrain.min_x + column as f64 * terrain.step;
                let pz = terrain.min_z + row as f64 * terrain.step;
                if px.abs() > 22. || pz.abs() > 22. {
                    continue;
                }
                let score = (px - x).powi(2) + (pz - z).powi(2);
                if score < best {
                    best = score;
                    nearest = Some((px, pz, y));
                }
            }
        }
        nearest.ok_or_else(|| format!("No safe spawn within world bounds for {zone}"))
    }
    pub fn teleport(&self, player: &mut Player, zone: &str, x: f64, z: f64) -> Result<(), String> {
        let (x, z, y) = self.safe_position(zone, x, z)?;
        player.zone = zone.into();
        player.x = x;
        player.z = z;
        player.y = y;
        player.ground_y = y;
        player.jump_offset = 0.;
        player.vy = 0.;
        player.input = Input::default();
        player.boosting = false;
        if zone != "race" {
            player.race = None;
        }
        if zone != "karate" {
            player.karate = None;
        }
        Ok(())
    }
    pub fn near_interaction(&self, player: &Player, name: &str) -> bool {
        let (zone, x, z, radius) = match name {
            "trainer" => ("hub", 7., 2., 2.),
            "shop" => ("hub", -4., 8., 2.),
            "fruit" => ("garden", -7., -5., 2.),
            "swim" => ("garden", -8., 0., 3.),
            _ => return false,
        };
        if player.zone != zone {
            return false;
        }
        self.sample_ground(zone, x, z).is_some_and(|y| {
            ((player.x - x).powi(2) + (player.z - z).powi(2) + (player.y - y).powi(2)).sqrt()
                <= radius
        })
    }

    pub fn action(&mut self, id: &str, message: &Value) {
        let Some(mut player) = self.players.remove(id) else {
            return;
        };
        if player.connections > 0
            && message["action"].as_str().is_some()
            // Pose updates (about 10 a second) have their own limit, so they
            // never crowd out the player's other actions.
            && if message["action"] == "chao-pose" {
                player.cooldown("pose", self.time, 80.)
            } else if message["action"] == "voice" {
                player.cooldown("voice", self.time, 40.)
            } else {
                player.cooldown("message", self.time, 80.)
            }
        {
            self.action_inner(&mut player, message);
        }
        self.players.insert(id.into(), player);
        if self
            .mission
            .as_ref()
            .is_some_and(|m| !m.completed && m.enemies.iter().all(|e| e.hp <= 0.))
        {
            self.finish_mission();
        }
    }
    fn action_inner(&mut self, p: &mut Player, message: &Value) {
        let action = message["action"].as_str().unwrap_or_default();
        let id = p.id.clone();
        if action.starts_with("competition-") {
            competition::action(self, p, message);
            return;
        }
        if competition::reserved(p) || competition::training_conflict(p) {
            self.notice(
                &id,
                "Finish your reserved activity before starting another action.",
                "error",
            );
            return;
        }
        if action == "feed"
            || (action == "buy"
                && ["garden-fruit", "run-drive", "power-drive"]
                    .iter()
                    .any(|item| message["item"] == *item))
        {
            self.notice(
                &id,
                "Use the versioned care inventory controls to purchase and consume items.",
                "info",
            );
            return;
        }
        if action.starts_with("watch-") {
            watch::action(self, p, message);
            return;
        }
        if p.care.tiny_resident && matches!(action, "tiny-start" | "tiny-stop") {
            phone::action(self, p, message);
            return;
        }
        if p.watch.is_some() {
            self.notice(
                &id,
                "Stop watching your Chao before starting another activity.",
                "error",
            );
            return;
        }
        if action.starts_with("tiny-") {
            phone::action(self, p, message);
            return;
        }
        if p.tiny_game.is_some() {
            self.notice(
                &id,
                "Stop your Tiny Garden game before returning to other activities.",
                "error",
            );
            return;
        }
        if action == "chao-pose" {
            // Relayed as is to the same area room; other games only draw it.
            let pose = &message["pose"];
            let ok = CHAO_WORLD_ZONES.contains(&p.zone.as_str())
                && pose.as_object().is_some_and(|o| {
                    o.len() <= 8 && o.values().all(|v| v.as_f64().is_some_and(f64::is_finite))
                });
            // Updates may carry only the changed keys: merged into the stored pose.
            if ok {
                let mut merged = p.chao_pose.as_object().cloned().unwrap_or_default();
                merged.extend(pose.as_object().unwrap().clone());
                if merged.len() <= 8 {
                    p.chao_pose = Value::Object(merged);
                }
            }
            return;
        }
        if action == "chao-world" {
            // The game's own Chao World area the player's game is in (client
            // sa2_boot::start_stage): one shared room per area. The game owns
            // position and gating, so this only moves the player's room.
            let Some(zone) = message["area"]
                .as_str()
                .filter(|a| CHAO_WORLD_ZONES.contains(a))
            else {
                self.notice(&id, "Unknown Chao World area.", "error");
                return;
            };
            // Hero/Dark rooms only for a Chao aligned to them (mob 2026-11-21), the
            // same line as the hub's Hero/Dark portals. Admins go anywhere.
            if !p.admin
                && ((zone == "hero" && p.alignment < 60.) || (zone == "dark" && p.alignment > -60.))
            {
                self.notice(&id, "Your Chao isn't aligned to that garden.", "error");
                return;
            }
            p.zone = zone.into();
            p.chao_pose = Value::Null;
            p.input = Input::default();
            p.race = None;
            p.karate = None;
            return;
        }
        if action.starts_with("karate-") {
            activities::karate_action(self, p, message);
            return;
        }
        if action == "race-enter" {
            activities::enter_race(self, p, message);
            return;
        }
        if action == "race-leave" {
            let _ = self.teleport(p, "hub", 0., 6.);
            return;
        }
        if action == "attack" && p.zone == "karate" {
            activities::karate_action(self, p, &json!({"action":"karate-attack","move":"punch"}));
            return;
        }
        match action {
            "mute" => {
                self.mute(p, message);
                return;
            }
            "voice" => {
                self.voice(p, message);
                return;
            }
            "chat" => {
                self.chat(p, message);
                return;
            }
            "attack" => {
                if p.vehicle.as_deref() == Some("dark-chao-walker") {
                    self.mech_fire(p);
                } else {
                    self.attack(p);
                }
                return;
            }
            "mech-fire" => {
                self.mech_fire(p);
                return;
            }
            "admin-dark-chaos" => {
                if !p.admin {
                    self.notice(
                        &id,
                        "Only the owner admin can choose this form directly.",
                        "error",
                    );
                    return;
                }
                p.body.set_alignment_percent(-100.);
                crate::body::become_chaos(&mut p.body);
                p.form = "chaos".into();
                p.alignment = -100.;
                self.dirty = true;
                self.notice(
                    &id,
                    "Your owner Chao is now a Dark Chaos Chao. Perfect stats preserved.",
                    "success",
                );
                return;
            }
            "mount-dark-walker" => {
                if p.race.as_ref().is_some_and(|r| r.course_id.is_some()) {
                    self.notice(
                        &p.id,
                        "Leave your native Chao race before mounting a vehicle.",
                        "error",
                    );
                    return;
                }
                if p.zone == "karate" {
                    self.notice(&id, "Leave Chao Karate before mounting a vehicle.", "error");
                    return;
                }
                if !p.admin {
                    self.notice(
                        &id,
                        "Only the owner admin can pilot the Dark Chao Walker.",
                        "error",
                    );
                    return;
                }
                p.vehicle = Some("dark-chao-walker".into());
                self.dirty = true;
                self.notice(&id, "You are piloting the Dark Chao Walker.", "success");
                return;
            }
            "dismount-vehicle" => {
                p.vehicle = None;
                p.boosting = false;
                self.dirty = true;
                self.notice(&id, "You stepped out of your vehicle.", "info");
                return;
            }
            "equip-owner-gear" | "unequip-owner-gear" => {
                if !p.admin {
                    self.notice(&id, "Only the owner admin can equip these extras.", "error");
                    return;
                }
                if action == "equip-owner-gear" {
                    match message["item"].as_str() {
                        Some("shadow-air-shoes") => {
                            p.gear.footwear = Some("shadow-air-shoes".into())
                        }
                        Some("knuckles-hammer-gloves") => {
                            p.gear.hands = Some("knuckles-hammer-gloves".into())
                        }
                        Some("metal-sonic-chao") => p.gear.look = Some("metal-sonic-chao".into()),
                        _ => {
                            self.notice(&id, "Unknown gear item.", "error");
                            return;
                        }
                    }
                } else {
                    match message["slot"].as_str() {
                        Some("footwear") => p.gear.footwear = None,
                        Some("hands") => p.gear.hands = None,
                        Some("look") => p.gear.look = None,
                        _ => {
                            self.notice(&id, "Unknown gear slot.", "error");
                            return;
                        }
                    }
                }
                self.dirty = true;
                self.notice(&id, "Your gear has been updated.", "success");
                return;
            }
            "rename" => {
                p.name = short_name(message["name"].as_str());
                self.dirty = true;
                self.notice(&id, format!("Your name is now {}.", p.name), "info");
                return;
            }
            "hub" | "garden" => {
                let _ = self.teleport(p, action, 0., 6.);
                p.hp = p.max_hp;
                self.notice(
                    &id,
                    if action == "hub" {
                        "Welcome back to the main hub."
                    } else {
                        "You entered the Neutral Garden."
                    },
                    "info",
                );
                return;
            }
            "hero" | "dark" => {
                if !(if action == "hero" {
                    p.alignment >= 60.
                } else {
                    p.alignment <= -60.
                }) {
                    self.notice(&id, format!("{} Garden needs {} alignment. Make garden choices to change your alignment.", if action == "hero" { "Hero" } else { "Dark" }, if action == "hero" { "+60" } else { "−60" }), "error");
                    return;
                }
                let _ = self.teleport(p, action, 0., 6.);
                self.notice(
                    &id,
                    format!(
                        "You entered the {} Garden.",
                        if action == "hero" { "Hero" } else { "Dark" }
                    ),
                    "info",
                );
                return;
            }
            "mission" => {
                self.enter_mission(p);
                return;
            }
            "race" => {
                if p.race.is_some() {
                    self.notice(
                        &id,
                        "Your race is already running. Follow the checkpoint order.",
                        "info",
                    );
                    return;
                }
                let _ = self.teleport(p, "race", 0., 12.);
                p.race = Some(Race {
                    started_at: self.time,
                    next_checkpoint: 1,
                    course_id: None,
                });
                self.notice(
                    &id,
                    "Race started! Follow the glowing checkpoints in order.",
                    "success",
                );
                return;
            }
            "evolve" => {
                if p.form == "chaos" {
                    self.notice(&id, "You have already become a Chaos Chao.", "info");
                    return;
                }
                if p.evolution()["ready"] != true {
                    self.notice(&id, "Chaos evolution: finish 3 missions and 1 race, reach Run/Swim/Power level 10, and happiness 80.", "error");
                    return;
                }
                crate::body::become_chaos(&mut p.body);
                p.form = "chaos".into();
                self.dirty = true;
                self.notice(
                    &id,
                    "Your adventures have awakened a Chaos Chao!",
                    "success",
                );
                return;
            }
            _ => {}
        }
        if !["hub", "garden", "hero", "dark"].contains(&p.zone.as_str()) {
            self.notice(
                &id,
                "Return to the hub or a garden for this activity.",
                "error",
            );
            return;
        }
        if action == "buy" {
            self.buy(p, message["item"].as_str());
            return;
        }
        if action == "align-good" || action == "align-dark" {
            if !p.cooldown("alignment", self.time, 1500.) {
                self.notice(&id, "Give your last choice a moment.", "info");
                return;
            }
            p.body.set_alignment_percent(
                (p.alignment + if action == "align-good" { 15. } else { -15. }).clamp(-100., 100.),
            );
            p.happiness = (p.happiness + 2.).min(100.);
            p.update_form();
            self.dirty = true;
            self.notice(
                &id,
                if action == "align-good" {
                    "You helped another Chao. Hero alignment +15."
                } else {
                    "You played a mischievous prank. Dark alignment +15."
                },
                "info",
            );
            return;
        }
        if let Some(stat) = action
            .strip_prefix("train-")
            .filter(|stat| MAIN_STATS.contains(stat))
        {
            if !self.near_interaction(p, if stat == "swim" { "swim" } else { "trainer" }) {
                self.notice(
                    &id,
                    if stat == "swim" {
                        "Visit the Neutral Garden pond to practice swimming."
                    } else {
                        "Visit Knuckles in the hub for skill training."
                    },
                    "info",
                );
                return;
            }
            if !p.cooldown("train", self.time, 900.) {
                return;
            }
            if p.energy < 10. {
                self.notice(&id, "You need energy. Eat some fruit first.", "error");
                return;
            }
            p.energy -= 10.;
            p.happiness = (p.happiness + 2.).min(100.);
            p.gain(stat, 2);
            self.dirty = true;
            let title = format!("{}{}", stat[..1].to_uppercase(), &stat[1..]);
            self.notice(
                &id,
                format!("{title} training complete: +2 levels."),
                "success",
            );
        }
    }
    fn buy(&mut self, p: &mut Player, item_id: Option<&str>) {
        if !self.near_interaction(p, "shop") {
            self.notice(
                &p.id,
                "Visit Amy in the main hub to buy from the Chao shop.",
                "info",
            );
            return;
        }
        let Some(item) = SHOP_CATALOG.iter().find(|item| Some(item.id) == item_id) else {
            self.notice(&p.id, "That item is not in the Chao shop.", "error");
            return;
        };
        if item.id == "eggshell-cap" && p.owned_cosmetics.iter().any(|id| id == "eggshell") {
            self.notice(&p.id, "You already own the Eggshell Cap.", "info");
            return;
        }
        if p.rings < item.price {
            self.notice(
                &p.id,
                format!(
                    "{} costs {} rings. Collect a few more first.",
                    item.name, item.price
                ),
                "error",
            );
            return;
        }
        p.rings -= item.price;
        if item.id == "eggshell-cap" {
            p.owned_cosmetics.push("eggshell".into());
            p.equipped = Some("eggshell".into());
        }
        self.dirty = true;
        self.notice(
            &p.id,
            format!("Purchased {} for {} rings!", item.name, item.price),
            "success",
        );
    }
    fn enter_mission(&mut self, p: &mut Player) {
        if p.zone == "mission" {
            self.notice(
                &p.id,
                "You are already in the arena. Defeat all five robots.",
                "info",
            );
            return;
        }
        let occupied = self
            .players
            .values()
            .any(|other| other.connections > 0 && other.zone == "mission");
        if self.mission.is_none() || !occupied {
            let id = Uuid::new_v4().to_string();
            self.mission = Some(Mission {
                enemies: [(-10., -7.), (-4., -11.), (4., -11.), (10., -7.), (0., -3.)]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (x, z))| Enemy {
                        id: format!("{id}-robot-{i}"),
                        x,
                        z,
                        hp: 100.,
                        max_hp: 100.,
                        last_attack_at: f64::NEG_INFINITY,
                    })
                    .collect(),
                rings: [(-10., 6.), (-5., 1.), (0., -4.), (5., 1.), (10., 6.)]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (x, z))| Ring {
                        id: format!("{id}-ring-{i}"),
                        x,
                        z,
                        zone: "mission".into(),
                        available_at: 0.,
                    })
                    .collect(),
                id,
                completed: false,
                members: BTreeSet::new(),
                rewarded: BTreeSet::new(),
            });
        }
        let mission = self.mission.as_mut().unwrap();
        if mission.completed {
            self.notice(
                &p.id,
                "This arena is cleared. Everyone must return to a garden before a new mission.",
                "info",
            );
            return;
        }
        mission.members.insert(p.id.clone());
        let _ = self.teleport(p, "mission", 0., 15.);
        p.hp = p.max_hp;
        self.notice(
            &p.id,
            "Mission: defeat 5 robots! Move close and attack. Clear reward: 50 rings.",
            "success",
        );
    }
    fn attack(&mut self, p: &mut Player) {
        if p.zone != "mission" || self.mission.as_ref().is_none_or(|m| m.completed) {
            self.notice(&p.id, "Attack robots inside the mission arena.", "info");
            return;
        }
        if !p.cooldown("attack", self.time, 550.) {
            return;
        }
        self.events.push(Event { target: None, message: json!({"type": "event", "kind": "attack", "playerId": p.id, "x": p.x, "z": p.z, "zone": p.zone}) });
        let mission = self.mission.as_mut().unwrap();
        let nearest = mission
            .enemies
            .iter_mut()
            .filter(|enemy| {
                enemy.hp > 0.
                    && ((p.x - enemy.x).powi(2) + (p.z - enemy.z).powi(2) + p.y.powi(2)).sqrt()
                        <= 0.8
            })
            .min_by(|a, b| dist(p.x, p.z, a.x, a.z).total_cmp(&dist(p.x, p.z, b.x, b.z)));
        let Some(enemy) = nearest else {
            self.notice(&p.id, "Get close to a robot to hit it.", "info");
            return;
        };
        enemy.hp = (enemy.hp
            - if p.admin {
                100.
            } else {
                35. + f64::from(p.stats["power"].level) * 0.3
            })
        .max(0.);
        if enemy.hp == 0. {
            crate::care::mission_drop(
                p,
                &self.care_catalog,
                &mission.id,
                &enemy.id,
                enemy.x,
                enemy.z,
            );
            p.rings += 5;
            p.gain("power", 1);
            self.dirty = true;
            self.notice(&p.id, "Robot defeated! +5 rings.", "success");
        }
    }
    fn mech_fire(&mut self, p: &mut Player) {
        if !p.admin || p.vehicle.as_deref() != Some("dark-chao-walker") {
            self.notice(
                &p.id,
                "Mount your authorized Dark Chao Walker to fire.",
                "error",
            );
            return;
        }
        if !p.cooldown("mech-fire", self.time, MECH_FIRE_COOLDOWN) {
            return;
        }
        let dx = p.facing.sin();
        let dz = p.facing.cos();
        let (x, y, z) = (p.x + dx * 0.35, p.y + 0.65, p.z + dz * 0.35);
        let target = self
            .mission
            .as_mut()
            .filter(|m| p.zone == "mission" && !m.completed)
            .and_then(|mission| {
                mission
                    .enemies
                    .iter_mut()
                    .filter(|enemy| {
                        let ex = enemy.x - p.x;
                        let ez = enemy.z - p.z;
                        let flat = ex.hypot(ez);
                        enemy.hp > 0.
                            && (ex.powi(2) + ez.powi(2) + (0.35 - y).powi(2)).sqrt()
                                <= MECH_FIRE_RANGE
                            && (flat < 0.001
                                || (ex * dx + ez * dz) / flat >= std::f64::consts::FRAC_1_SQRT_2)
                    })
                    .min_by(|a, b| dist(p.x, p.z, a.x, a.z).total_cmp(&dist(p.x, p.z, b.x, b.z)))
            });
        let (target_id, hit_x, hit_y, hit_z, killed) = if let Some(enemy) = target {
            enemy.hp = (enemy.hp - MECH_FIRE_DAMAGE).max(0.);
            (
                Some(enemy.id.clone()),
                enemy.x,
                0.35,
                enemy.z,
                enemy.hp == 0.,
            )
        } else {
            (
                None,
                x + dx * MECH_FIRE_RANGE,
                y,
                z + dz * MECH_FIRE_RANGE,
                false,
            )
        };
        self.events.push(Event { target: None, message: json!({"type":"event", "kind":"mech-shot", "playerId":p.id, "zone":p.zone,
            "x":x, "y":y, "z":z, "hitX":hit_x, "hitY":hit_y, "hitZ":hit_z, "hit":target_id.is_some(), "targetId":target_id}) });
        if killed {
            if let (Some(mission), Some(enemy_id)) = (&self.mission, &target_id) {
                crate::care::mission_drop(
                    p,
                    &self.care_catalog,
                    &mission.id,
                    enemy_id,
                    hit_x,
                    hit_z,
                );
            }
            p.rings += 5;
            p.gain("power", 1);
            self.dirty = true;
            self.notice(&p.id, "Robot defeated! +5 rings.", "success");
        }
    }
    pub fn finish_mission(&mut self) {
        let Some(mission) = self.mission.as_mut().filter(|m| !m.completed) else {
            return;
        };
        mission.completed = true;
        let mut recipients = vec![];
        for id in &mission.members {
            let Some(p) = self
                .players
                .get_mut(id)
                .filter(|p| p.connections > 0 && p.zone == "mission")
            else {
                continue;
            };
            if !mission.rewarded.insert(id.clone()) {
                continue;
            }
            p.rings += 50;
            p.missions_completed += 1;
            p.happiness = (p.happiness + 10.).min(100.);
            for stat in MAIN_STATS {
                p.gain(stat, 2);
            }
            recipients.push(id.clone());
        }
        for id in recipients {
            self.notice(
                &id,
                "Mission cleared! +50 rings and +2 to all five skill levels.",
                "success",
            );
        }
        self.dirty = true;
    }

    pub fn tick(&mut self, dt: f64) {
        self.last_resident_tick = Duration::ZERO;
        self.last_tick_residents = 0;
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        let dt = dt.min(0.1);
        self.time += dt * 1000.;
        let ids: Vec<_> = self.players.keys().cloned().collect();
        for id in ids {
            let mut p = self.players.remove(&id).unwrap();
            competition::tick(self, &mut p, dt);
            let resident = p.watch.is_some() && self.life_policy.resident_needs;
            if p.connections > 0 || resident {
                let (grounded, connected) = (p.jump_offset <= 0., p.connections > 0);
                crate::life::tick(&mut p, dt, self.life_policy, connected, grounded);
                if p.life.immobile() {
                    p.input = Input::default();
                }
            }
            if competition::reserved(&p) || p.activity_lease.is_some() {
                p.input = Input::default();
                self.players.insert(id, p);
                continue;
            }
            watch::maintain_tiny(self, &mut p);
            if p.care.tiny_resident
                && p.tiny_game.is_none()
                && p.care.activity_lock.is_none()
                && p.race.is_none()
                && p.karate.is_none()
                && p.vehicle.is_none()
                && matches!(p.zone.as_str(), "garden" | "hero" | "dark" | "hub")
                && let Some(assets) = self.native_tiny.weeds.as_ref()
            {
                let patch = p.care.weeds.get_or_insert_with(crate::weeds::Patch::new);
                self.dirty |= patch.advance(dt, assets, &mut p.happiness, &mut p.hunger);
                if let Some(t) = &mut p.care.toys {
                    t.advance(dt);
                    self.dirty = true;
                }
            }
            if p.connections == 0 && p.watch.is_none() {
                self.players.insert(id, p);
                continue;
            }
            // Body growth advances from life.rs's LifeTick (one native clock).
            p.update_form();
            if p.tiny_game.is_some() {
                phone::tick(self, &mut p, dt);
                self.players.insert(id, p);
                continue;
            }
            if self.time - p.last_input_at > 350. || p.care.activity_lock.is_some() {
                p.input = Input::default();
            }
            if p.watch.is_some() {
                let started = Instant::now();
                watch::tick(self, &mut p, dt);
                self.last_resident_tick += started.elapsed();
                self.last_tick_residents += 1;
            }
            let water = p.zone == "garden" && dist(p.x, p.z, -8., 0.) < 5.;
            let metal = p.gear.look.as_deref() == Some("metal-sonic-chao");
            let gravity = if metal { METAL_CHAO_GRAVITY } else { 12. };
            let land_speed = if metal {
                METAL_CHAO_TOP_SPEED
            } else {
                1.4 + f64::from(p.stats["run"].level) * 0.006
            };
            let mut speed = land_speed
                * if water {
                    0.65 + f64::from(p.stats["swim"].level) * 0.003
                } else {
                    1.
                };
            if p.race
                .as_ref()
                .is_some_and(|r| r.course_id.is_some() && self.time < r.started_at)
            {
                speed = 0.;
            }
            let mut ground = match self.player_ground(&p, p.x, p.z) {
                Some(height) => height,
                None => {
                    let recovery = if let Some(course) = activities::native_course(self, &p) {
                        course
                            .spawn
                            .as_ref()
                            .and_then(|spawn| {
                                self.player_ground(&p, spawn.x, spawn.z)
                                    .map(|y| (spawn.x, spawn.z, y))
                            })
                            .ok_or_else(|| "No native race spawn".to_string())
                    } else {
                        self.safe_position(&p.zone, p.x, p.z)
                    };
                    let Ok((x, z, y)) = recovery else {
                        self.players.insert(id, p);
                        continue;
                    };
                    p.x = x;
                    p.z = z;
                    p.jump_offset = 0.;
                    p.vy = 0.;
                    y
                }
            };
            let mounted = p.admin && p.vehicle.as_deref() == Some("dark-chao-walker");
            if p.input.jump && p.jump_offset <= 0. && p.vy <= 0. {
                p.vy = if mounted {
                    3.2
                } else if metal {
                    METAL_CHAO_JUMP_SPEED
                } else {
                    2.4
                };
            }
            p.input.jump = false;
            p.boosting = mounted && p.input.boost && p.boost_energy > 0.;
            let predicted_y = if p.boosting {
                p.boost_energy = (p.boost_energy - MECH_BOOST_DRAIN * dt).max(0.);
                p.vy = MECH_BOOST_SPEED;
                let height = (ground + p.jump_offset + p.vy * dt).min(ground + MECH_BOOST_HEIGHT);
                if height >= ground + MECH_BOOST_HEIGHT {
                    p.vy = 0.;
                }
                height
            } else {
                let height = ground + p.jump_offset + p.vy * dt - 0.5 * gravity * dt * dt;
                p.vy -= gravity * dt;
                height
            };
            let bounds = activities::native_course(self, &p)
                .map(|c| c.bounds.clone())
                .unwrap_or_else(|| {
                    if p.zone == "karate" {
                        activities::Bounds {
                            min_x: -1.35,
                            max_x: 1.35,
                            min_z: 0.4,
                            max_z: 3.1,
                        }
                    } else {
                        activities::Bounds::default()
                    }
                });
            let next_x = (p.x + p.input.dx * speed * dt).clamp(bounds.min_x, bounds.max_x);
            let next_z = (p.z + p.input.dz * speed * dt).clamp(bounds.min_z, bounds.max_z);
            let (start_x, start_z, start_ground) = (p.x, p.z, ground);
            let subdivisions = self.player_terrain(&p).map_or(1, |t| {
                (dist(next_x, next_z, start_x, start_z) / 0.2_f64.min(t.step * 0.4))
                    .ceil()
                    .max(1.) as usize
            });
            for part in 1..=subdivisions {
                let x = start_x + (next_x - start_x) * part as f64 / subdivisions as f64;
                let z = start_z + (next_z - start_z) * part as f64 / subdivisions as f64;
                let Some(candidate) = self.player_ground(&p, x, z) else {
                    break;
                };
                if candidate - start_ground > 0.15 && predicted_y < candidate {
                    break;
                }
                p.x = x;
                p.z = z;
                ground = candidate;
            }
            if p.input.dx != 0. || p.input.dz != 0. {
                p.facing = p.input.dx.atan2(p.input.dz);
            }
            p.ground_y = ground;
            p.y = ground.max(if p.boosting {
                predicted_y.min(ground + MECH_BOOST_HEIGHT)
            } else {
                predicted_y
            });
            p.jump_offset = p.y - ground;
            if p.jump_offset == 0. {
                p.vy = 0.;
                if !p.input.boost {
                    p.boost_energy = (p.boost_energy + MECH_BOOST_RECOVERY * dt).min(100.);
                }
            }
            if p.watch.is_none() {
                p.energy = (p.energy + dt * 1.1).min(100.);
            }
            if p.watch.is_none() {
                self.pickup_rings(&mut p);
                activities::tick_karate(self, &mut p, dt);
                self.tick_race(&mut p);
            }
            self.players.insert(id, p);
        }
        let pending: Vec<_> = self
            .players
            .iter()
            .filter(|(_, p)| !p.life.events.is_empty())
            .map(|(id, _)| id.clone())
            .collect();
        for id in pending {
            crate::life::apply_events(self, &id);
        }
        self.tick_enemies(dt);
    }
    fn pickup_rings(&mut self, p: &mut Player) {
        let nodes = if p.zone == "mission" {
            self.mission
                .as_ref()
                .map(|m| m.rings.clone())
                .unwrap_or_default()
        } else {
            self.rings.clone()
        };
        for (i, ring) in nodes.iter().enumerate() {
            if ring.zone != p.zone
                || ring.available_at > self.time
                || dist(p.x, p.z, ring.x, ring.z) > 0.55
            {
                continue;
            }
            if self
                .sample_ground(&ring.zone, ring.x, ring.z)
                .is_none_or(|height| (p.y - height).abs() > 2.5)
            {
                continue;
            }
            if p.zone == "mission" {
                self.mission.as_mut().unwrap().rings[i].available_at = f64::INFINITY;
            } else {
                self.rings[i].available_at = self.time + 20000.;
            }
            p.rings += 1;
            self.dirty = true;
            self.events.push(Event { target: Some(p.id.clone()), message: json!({"type": "event", "kind": "ring", "ringId": ring.id, "x": ring.x, "z": ring.z}) });
        }
    }
    fn tick_enemies(&mut self, dt: f64) {
        let Some(mut mission) = self.mission.take() else {
            return;
        };
        if mission.completed {
            self.mission = Some(mission);
            return;
        }
        for enemy in mission.enemies.iter_mut().filter(|enemy| enemy.hp > 0.) {
            let target_id = self
                .players
                .values()
                .filter(|p| p.connections > 0 && p.zone == "mission")
                .min_by(|a, b| {
                    dist(enemy.x, enemy.z, a.x, a.z).total_cmp(&dist(enemy.x, enemy.z, b.x, b.z))
                })
                .map(|p| p.id.clone());
            let Some(id) = target_id else {
                break;
            };
            let mut target = self.players.remove(&id).unwrap();
            let length = dist(enemy.x, enemy.z, target.x, target.z);
            if length > 0.55 {
                enemy.x = (enemy.x + (target.x - enemy.x) / length * dt * 0.8).clamp(-20., 20.);
                enemy.z = (enemy.z + (target.z - enemy.z) / length * dt * 0.8).clamp(-20., 20.);
            } else if self.time - enemy.last_attack_at > 1300. && target.y < 0.4 {
                enemy.last_attack_at = self.time;
                target.hp = (target.hp - 8.).max(0.);
                if target.hp == 0. {
                    let _ = self.teleport(&mut target, "hub", 0., 6.);
                    target.hp = target.max_hp;
                    self.notice(
                        &id,
                        "The robots overwhelmed you. You recovered safely in the main hub.",
                        "error",
                    );
                }
            }
            self.players.insert(id, target);
        }
        self.mission = Some(mission);
    }
    fn tick_race(&mut self, p: &mut Player) {
        if p.race.as_ref().is_some_and(|race| race.course_id.is_some()) {
            activities::tick_native_race(self, p);
            return;
        }
        if p.zone != "race" {
            return;
        }
        let Some(race) = p.race.as_mut() else {
            return;
        };
        let (x, z) = RACE_CHECKPOINTS[race.next_checkpoint];
        if dist(p.x, p.z, x, z) > 0.8 {
            return;
        }
        race.next_checkpoint += 1;
        if race.next_checkpoint < RACE_CHECKPOINTS.len() {
            self.notice(
                &p.id,
                format!("Checkpoint {}/4!", race.next_checkpoint - 1),
                "info",
            );
            return;
        }
        let elapsed = (self.time - race.started_at).round();
        p.races_completed += 1;
        p.rings += 25;
        p.best_race_ms = Some(p.best_race_ms.map_or(elapsed, |best| best.min(elapsed)));
        p.gain("run", 3);
        p.happiness = (p.happiness + 10.).min(100.);
        let _ = self.teleport(p, "hub", 0., 6.);
        self.dirty = true;
        self.notice(
            &p.id,
            format!("Race finished in {:.1}s! +25 rings.", elapsed / 1000.),
            "success",
        );
    }
    /// World chat goes to everyone online; room chat to players in the same room.
    fn chat(&mut self, p: &mut Player, message: &Value) {
        let channel = message["channel"].as_str().unwrap_or_default();
        if channel != "world" && channel != "room" {
            return;
        }
        let text: String = message["text"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect();
        let text = text.trim();
        if text.is_empty() || !p.cooldown("chat", self.time, 1000.) {
            return;
        }
        let msg = json!({"type":"event","kind":"chat","channel":channel,
            "fromId":p.id,"fromName":p.name,"text":text});
        let world = channel == "world";
        let mut targets: Vec<String> = self
            .players
            .values()
            .filter(|o| o.connections > 0 && (world || self.same_room(p, o)) && !mutes(o, &p.id))
            .map(|o| o.id.clone())
            .collect();
        targets.push(p.id.clone());
        for target in targets {
            self.events.push(Event {
                target: Some(target),
                message: msg.clone(),
            });
        }
    }
    /// Push-to-talk audio: relayed only to players in the same room whose Chao
    /// is within VOICE_RADIUS, so no player ever learns another's address.
    fn voice(&mut self, p: &mut Player, message: &Value) {
        let Some(data) = message["data"].as_str() else {
            return;
        };
        let pos = |q: &Player| Some((q.chao_pose["x"].as_f64()?, q.chao_pose["z"].as_f64()?));
        let Some((x, z)) = pos(p) else { return };
        if data.is_empty() || data.len() > 1400 {
            return;
        }
        let near: Vec<(String, f64)> = self
            .players
            .values()
            .filter(|o| o.connections > 0 && self.same_room(p, o) && !mutes(o, &p.id))
            .filter_map(|o| {
                let (ox, oz) = pos(o)?;
                let d = (ox - x).hypot(oz - z);
                (d < VOICE_RADIUS).then(|| (o.id.clone(), 1. - d / VOICE_RADIUS))
            })
            .collect();
        for (target, volume) in near {
            self.events.push(Event {
                target: Some(target),
                message: json!({"type":"event","kind":"voice",
                "fromId":p.id,"fromName":p.name,"volume":volume,"data":data}),
            });
        }
    }
    /// Mutes live in the player's server save; the server then never sends
    /// that player's chat or voice to them.
    fn mute(&mut self, p: &mut Player, message: &Value) {
        let Some(target) = message["playerId"].as_str() else {
            return;
        };
        if target == p.id || !p.profile_extra.is_object() {
            return;
        }
        let mut list: Vec<Value> = p.profile_extra["muted"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        list.retain(|v| v != target);
        if message["muted"] != false && list.len() < 500 {
            list.push(target.into());
        }
        let reply = json!({"type":"event","kind":"mutes","muted":list});
        p.profile_extra["muted"] = Value::Array(list);
        self.dirty = true;
        self.events.push(Event {
            target: Some(p.id.clone()),
            message: reply,
        });
    }
    fn same_room(&self, viewer: &Player, other: &Player) -> bool {
        if viewer.zone != other.zone {
            return false;
        }
        match viewer.zone.as_str() {
            // Native course variants can share scenery but have different gates.
            "race" => {
                viewer.race.as_ref().and_then(|r| r.course_id.as_deref())
                    == other.race.as_ref().and_then(|r| r.course_id.as_deref())
            }
            // Each bout has its own server-owned opponent and clock.
            "karate" => viewer.id == other.id,
            _ => true,
        }
    }
    /// Diagnostic identity for existing logical rooms, not a shard assignment.
    pub fn room_id(&self, id: &str) -> Option<String> {
        let player = self.players.get(id)?;
        Some(match player.zone.as_str() {
            "race" => format!(
                "race:{}",
                player
                    .race
                    .as_ref()
                    .and_then(|r| r.course_id.as_deref())
                    .unwrap_or("prototype")
            ),
            "karate" => format!("karate:{}", player.id),
            _ => player.zone.clone(),
        })
    }
    /// Keep reliable private events private and room effects within their room.
    pub fn event_visible(&self, self_id: &str, event: &Event) -> bool {
        if let Some(target) = &event.target {
            return target == self_id;
        }
        let Some(viewer) = self.players.get(self_id) else {
            return false;
        };
        let Some(actor) = event.message["playerId"]
            .as_str()
            .and_then(|id| self.players.get(id))
        else {
            return false;
        };
        event.message["zone"].as_str() == Some(viewer.zone.as_str())
            && self.same_room(viewer, actor)
    }
    pub fn snapshot(&self, self_id: &str) -> Value {
        let viewer = self.players.get(self_id);
        let mission_visible = viewer.is_some_and(|p| p.zone == "mission");
        let rings: Vec<_> = self
            .rings
            .iter()
            .chain(self.mission.iter().flat_map(|m| m.rings.iter()))
            .filter(|ring| {
                ring.available_at <= self.time && viewer.is_some_and(|p| p.zone == ring.zone)
            })
            .map(|ring| json!({"id": ring.id, "x": ring.x, "z": ring.z, "zone": ring.zone}))
            .collect();
        let enemies: Vec<_> = self.mission.iter().filter(|_| mission_visible).flat_map(|m| m.enemies.iter()).filter(|enemy| enemy.hp > 0.)
            .map(|enemy| json!({"id": enemy.id, "x": enemy.x, "z": enemy.z, "hp": enemy.hp, "maxHp": enemy.max_hp, "zone": "mission"})).collect();
        let race = self
            .players
            .get(self_id)
            .and_then(|p| activities::public_race(self, p));
        json!({"type": "snapshot", "selfId": self_id, "serverTime": self.time, "players": self.players.values().filter(|p| p.id == self_id || ((p.connections > 0 || p.watch.is_some() || competition::reserved(p)) && viewer.is_some_and(|v|self.same_room(v,p)))).map(Player::public).collect::<Vec<_>>(),
            "rings": rings, "enemies": enemies, "mission": self.mission.as_ref().filter(|_|mission_visible).map(|m| json!({"id": m.id, "completed": m.completed, "remaining": m.enemies.iter().filter(|e| e.hp > 0.).count()})),
            "race": race, "shop": {"catalog": SHOP_CATALOG}, "activities": self.activities.public(),
            "care":crate::care::public(self,self_id),
            "life":crate::life::public(self,self_id),
            "competitionCatalog":self.competition_catalog.public(),
            "competition":self.players.get(self_id).map(competition::public),
            "karate": self.players.get(self_id).and_then(|p|p.karate.as_ref().map(KarateBout::public)),
            "activityResult": self.players.get(self_id).and_then(|p|p.activity_result.as_ref()),
            "tinyGame":self.players.get(self_id).and_then(|p|p.tiny_game.as_ref().map(TinyRun::public)),
            "tinyResult":self.players.get(self_id).and_then(|p|p.tiny_result.as_ref())})
    }
    pub fn export_profiles(&self) -> Vec<Value> {
        self.players.values().map(Player::profile).collect()
    }
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
    pub fn online_connections(&self) -> usize {
        self.players.values().map(|p| p.connections).sum()
    }
}
