//! Native activity catalog data with explicitly adapted, server-owned bouts.
use crate::game::{Event, Game, Player, RACE_CHECKPOINTS, Race, Terrain};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRecord {
    pub completions: u32,
    pub best_ms: Option<f64>,
}

pub fn restore_records(value: &Value) -> BTreeMap<String, ActivityRecord> {
    value
        .as_object()
        .into_iter()
        .flat_map(|map| map.iter())
        .filter(|(id, _)| valid_id(id))
        .map(|(id, row)| {
            (
                id.clone(),
                ActivityRecord {
                    completions: row["completions"].as_u64().unwrap_or(0).min(99999) as u32,
                    best_ms: row["bestMs"]
                        .as_f64()
                        .filter(|x| x.is_finite() && *x > 0. && *x <= 86_400_000.),
                },
            )
        })
        .take(256)
        .collect()
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub z: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub min_x: f64,
    pub max_x: f64,
    pub min_z: f64,
    pub max_z: f64,
}
impl Default for Bounds {
    fn default() -> Self {
        Self {
            min_x: -22.,
            max_x: 22.,
            min_z: -22.,
            max_z: 22.,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gate {
    pub x: f64,
    pub z: f64,
    #[serde(default = "gate_radius")]
    pub radius: f64,
}
fn gate_radius() -> f64 {
    0.8
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RaceCourse {
    pub id: String,
    pub name: String,
    pub group: String,
    #[serde(default)]
    pub level: Option<u8>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub world_id: Option<String>,
    #[serde(default)]
    pub gaps: Vec<String>,
    #[serde(default)]
    pub source: Value,
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub geometry_validated: bool,
    #[serde(default)]
    pub checkpoints: Vec<Gate>,
    #[serde(default)]
    pub bounds: Bounds,
    #[serde(default)]
    pub spawn: Option<Position>,
    #[serde(default = "race_reward")]
    pub reward_rings: u32,
}
fn race_reward() -> u32 {
    25
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarateOpponent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub native_index: usize,
    pub skills: Vec<f64>,
    #[serde(default)]
    pub appearance: Value,
}
impl KarateOpponent {
    fn skill(&self, index: usize) -> f64 {
        (self.skills.get(index).copied().unwrap_or(0.) / 2500.).clamp(0., 1.)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KarateTier {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub source: Value,
    pub opponents: Vec<KarateOpponent>,
    #[serde(default = "karate_reward")]
    pub reward_rings: u32,
}
fn karate_reward() -> u32 {
    50
}
#[derive(Clone, Debug, Default)]
pub struct ActivityCatalog {
    pub courses: Vec<RaceCourse>,
    pub karate: Vec<KarateTier>,
    pub terrains: BTreeMap<String, Terrain>,
}
impl ActivityCatalog {
    pub fn from_values(races: Value, karate: Value) -> Result<Self, String> {
        let courses: Vec<RaceCourse> =
            serde_json::from_value(races.get("courses").cloned().unwrap_or_else(|| json!([])))
                .map_err(|e| format!("Race catalog: {e}"))?;
        let tiers: Vec<KarateTier> =
            serde_json::from_value(karate.get("tiers").cloned().unwrap_or_else(|| json!([])))
                .map_err(|e| format!("Karate catalog: {e}"))?;
        if courses.len() > 256 || tiers.len() > 32 {
            return Err("Activity catalog is too large".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for course in &courses {
            if !valid_id(&course.id)
                || !ids.insert(course.id.clone())
                || course.name.is_empty()
                || course.reward_rings > 1000
            {
                return Err("Invalid race metadata".into());
            }
            let b = &course.bounds;
            if ![b.min_x, b.max_x, b.min_z, b.max_z]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 10000.)
                || b.min_x >= b.max_x
                || b.min_z >= b.max_z
            {
                return Err("Invalid race bounds".into());
            }
            let inside = |x: f64, z: f64| {
                x.is_finite()
                    && z.is_finite()
                    && (b.min_x..=b.max_x).contains(&x)
                    && (b.min_z..=b.max_z).contains(&z)
            };
            if course.checkpoints.len() > 2048
                || course.checkpoints.iter().any(|g| {
                    !inside(g.x, g.z) || !g.radius.is_finite() || !(0.1..=5.).contains(&g.radius)
                })
            {
                return Err("Invalid race gates".into());
            }
            if course.available
                && (!course.geometry_validated
                    || course.checkpoints.len() < 2
                    || course.spawn.as_ref().is_none_or(|p| !inside(p.x, p.z)))
            {
                return Err("An available native race needs validated gates and spawn".into());
            }
        }
        ids.clear();
        for tier in &tiers {
            if !valid_id(&tier.id)
                || !ids.insert(tier.id.clone())
                || tier.name.is_empty()
                || tier.opponents.is_empty()
                || tier.opponents.len() > 32
                || tier.reward_rings > 1000
            {
                return Err("Invalid karate tier".into());
            }
            for opponent in &tier.opponents {
                if !valid_id(&opponent.id)
                    || opponent.name.is_empty()
                    || opponent.skills.len() < 7
                    || opponent.skills.len() > 8
                    || opponent
                        .skills
                        .iter()
                        .any(|v| !v.is_finite() || !(0. ..=9999.).contains(v))
                {
                    return Err("Invalid native karate opponent".into());
                }
            }
        }
        Ok(Self {
            courses,
            karate: tiers,
            terrains: BTreeMap::new(),
        })
    }
    pub fn details(&self) -> Value {
        let courses: Vec<_> = self
            .courses
            .iter()
            .map(|course| {
                let mut value = serde_json::to_value(course).unwrap();
                value["available"] = json!(
                    course.available
                        && course.geometry_validated
                        && self
                            .terrains
                            .contains_key(course.world_id.as_deref().unwrap_or(&course.id))
                );
                value
            })
            .collect();
        json!({"raceCourses":courses,"karateTiers":self.karate,"rules":"Adapted browser controls and combat; original catalog/opponent data. Native engine routines are not ported."})
    }
    pub fn public(&self) -> Value {
        let courses: Vec<_> = self.courses.iter().map(|c| json!({"id":c.id,"name":c.name,"group":c.group,"level":c.level,"model":c.model,"worldId":c.world_id,
            "available":c.available && c.geometry_validated && self.terrains.contains_key(c.world_id.as_deref().unwrap_or(&c.id)),"gaps":c.gaps})).collect();
        let tiers: Vec<_> = self.karate.iter().map(|t| json!({"id":t.id,"name":t.name,"rounds":t.opponents.len(),"opponents":t.opponents.iter().map(|o|json!({"id":o.id,"name":o.name,"nativeIndex":o.native_index})).collect::<Vec<_>>()})).collect();
        json!({"raceCourses":courses,"karateTiers":tiers,"rules":"Adapted browser controls and combat; original catalog/opponent data. Native engine routines are not ported."})
    }
}

pub(crate) fn native_course<'a>(game: &'a Game, player: &Player) -> Option<&'a RaceCourse> {
    let id = player.race.as_ref()?.course_id.as_deref()?;
    game.activities
        .courses
        .iter()
        .find(|course| course.id == id)
}
pub(crate) fn enter_race(game: &mut Game, player: &mut Player, message: &Value) {
    if player.race.is_some() {
        game.notice(
            &player.id,
            "Finish or leave your current race first.",
            "error",
        );
        return;
    }
    if player.vehicle.is_some() {
        game.notice(
            &player.id,
            "Step out of your vehicle before entering a native Chao race.",
            "error",
        );
        return;
    }
    let course = message["courseId"]
        .as_str()
        .and_then(|id| game.activities.courses.iter().find(|c| c.id == id))
        .cloned();
    let Some(course) = course else {
        game.notice(&player.id, "Unknown native race course.", "error");
        return;
    };
    let ground = course.spawn.as_ref().and_then(|spawn| {
        game.activities
            .terrains
            .get(course.world_id.as_deref().unwrap_or(&course.id))
            .and_then(|t| t.sample(spawn.x, spawn.z))
    });
    if !course.available || !course.geometry_validated || ground.is_none() {
        game.notice(
            &player.id,
            "This original course is catalogued; its route and collision still need validation.",
            "error",
        );
        return;
    }
    let spawn = course.spawn.unwrap();
    let _ = game.teleport(player, "race", 0., 12.);
    player.x = spawn.x;
    player.z = spawn.z;
    player.y = ground.unwrap();
    player.ground_y = player.y;
    player.activity_result = None;
    player.race = Some(Race {
        started_at: game.time + 3000.,
        next_checkpoint: 0,
        course_id: Some(course.id),
    });
}
pub(crate) fn public_race(game: &Game, player: &Player) -> Option<Value> {
    let race = player.race.as_ref()?;
    if let Some(course) = native_course(game, player) {
        Some(
            json!({"active":true,"native":true,"courseId":course.id,"name":course.name,"group":course.group,"model":course.model,"worldId":course.world_id,
            "phase":if game.time<race.started_at{"countdown"}else{"racing"},"startedAt":race.started_at,"nextCheckpoint":race.next_checkpoint,"checkpoints":course.checkpoints,
            "bestMs":player.race_records.get(&course.id).and_then(|r|r.best_ms)}),
        )
    } else {
        Some(
            json!({"active":true,"native":false,"courseId":"prototype","name":"Garden Circuit (prototype)","phase":"racing","startedAt":race.started_at,"nextCheckpoint":race.next_checkpoint,
        "checkpoints":RACE_CHECKPOINTS.iter().map(|(x,z)|json!({"x":x,"z":z,"radius":0.8})).collect::<Vec<_>>(),"bestMs":player.best_race_ms}),
        )
    }
}
pub(crate) fn tick_native_race(game: &mut Game, player: &mut Player) {
    let Some(course) = native_course(game, player).cloned() else {
        return;
    };
    let race = player.race.as_mut().unwrap();
    if game.time < race.started_at {
        return;
    }
    let Some(gate) = course.checkpoints.get(race.next_checkpoint) else {
        return;
    };
    let ground = game
        .activities
        .terrains
        .get(course.world_id.as_deref().unwrap_or(&course.id))
        .and_then(|t| t.sample(gate.x, gate.z));
    if (player.x - gate.x).hypot(player.z - gate.z) > gate.radius
        || ground.is_none_or(|height| (player.y - height).abs() > 1.)
    {
        return;
    }
    race.next_checkpoint += 1;
    if race.next_checkpoint < course.checkpoints.len() {
        return;
    }
    let elapsed = (game.time - race.started_at).round();
    let record = player.race_records.entry(course.id.clone()).or_default();
    record.completions = (record.completions + 1).min(99999);
    record.best_ms = Some(record.best_ms.map_or(elapsed, |best| best.min(elapsed)));
    player.races_completed += 1;
    player.rings += course.reward_rings;
    player.gain("run", 1);
    player.happiness = (player.happiness + 8.).min(100.);
    player.activity_result = Some(
        json!({"kind":"race","courseId":course.id,"name":course.name,"native":true,"elapsedMs":elapsed,"rings":course.reward_rings,"rules":"adapted"}),
    );
    let _ = game.teleport(player, "hub", 0., 6.);
    game.dirty = true;
}

#[derive(Clone, Debug)]
pub struct KarateBout {
    pub id: String,
    pub tier: KarateTier,
    pub opponent_index: usize,
    pub phase: String,
    pub started_at: f64,
    pub round_started_at: f64,
    pub ends_at: f64,
    pub player_hp: f64,
    pub player_max_hp: f64,
    pub player_stamina: f64,
    pub opponent_hp: f64,
    pub opponent_max_hp: f64,
    pub opponent_stamina: f64,
    pub opponent_x: f64,
    pub opponent_z: f64,
    pub player_action: String,
    pub player_until: f64,
    pub player_hit_at: Option<f64>,
    pub opponent_action: String,
    pub opponent_until: f64,
    pub opponent_hit_at: Option<f64>,
    pub next_ai_at: f64,
    pub decision: usize,
}
impl KarateBout {
    fn new(tier: KarateTier, player: &Player, now: f64) -> Self {
        let mut bout = Self {
            id: Uuid::new_v4().to_string(),
            tier,
            opponent_index: 0,
            phase: "fighting".into(),
            started_at: now,
            round_started_at: now,
            ends_at: now + 60000.,
            player_hp: 0.,
            player_max_hp: 0.,
            player_stamina: 100.,
            opponent_hp: 0.,
            opponent_max_hp: 0.,
            opponent_stamina: 100.,
            opponent_x: 0.,
            opponent_z: 2.7,
            player_action: "idle".into(),
            player_until: 0.,
            player_hit_at: None,
            opponent_action: "idle".into(),
            opponent_until: 0.,
            opponent_hit_at: None,
            next_ai_at: now + 800.,
            decision: 0,
        };
        bout.reset_round(player, now);
        bout
    }
    fn opponent(&self) -> &KarateOpponent {
        &self.tier.opponents[self.opponent_index]
    }
    fn reset_round(&mut self, player: &Player, now: f64) {
        self.phase = "fighting".into();
        self.round_started_at = now;
        self.ends_at = now + 60000.;
        self.player_max_hp = 100. + f64::from(player.stats["stamina"].level) / 99. * 60.;
        self.player_hp = self.player_max_hp;
        self.player_stamina = 100.;
        self.opponent_max_hp = 85. + self.opponent().skill(4) * 60.;
        self.opponent_hp = self.opponent_max_hp;
        self.opponent_stamina = 100.;
        self.opponent_x = 0.;
        self.opponent_z = 2.7;
        self.player_action = "idle".into();
        self.opponent_action = "idle".into();
        self.player_hit_at = None;
        self.opponent_hit_at = None;
        self.player_until = 0.;
        self.opponent_until = 0.;
        self.next_ai_at = now + 800.;
    }
    pub fn public(&self) -> Value {
        json!({"id":self.id,"tierId":self.tier.id,"tierName":self.tier.name,"round":self.opponent_index+1,"rounds":self.tier.opponents.len(),"phase":self.phase,
            "startedAt":self.started_at,"roundEndsAt":self.ends_at,"playerHp":self.player_hp,"playerMaxHp":self.player_max_hp,"playerStamina":self.player_stamina,
            "playerAction":self.player_action,"playerActionUntil":self.player_until,
            "opponent":{"id":self.opponent().id,"name":self.opponent().name,"nativeIndex":self.opponent().native_index,"appearance":self.opponent().appearance,
                "x":self.opponent_x,"y":0.75,"z":self.opponent_z,"hp":self.opponent_hp,"maxHp":self.opponent_max_hp,"stamina":self.opponent_stamina,"action":self.opponent_action,"actionUntil":self.opponent_until,"hitAt":self.opponent_hit_at}})
    }
}

pub(crate) fn karate_action(game: &mut Game, player: &mut Player, message: &Value) {
    let action = message["action"].as_str().unwrap_or_default();
    if action == "karate-leave" {
        let _ = game.teleport(player, "hub", 0., 6.);
        return;
    }
    if action == "karate-enter" {
        if player.vehicle.is_some() {
            game.notice(
                &player.id,
                "Step out of your vehicle before entering Chao Karate.",
                "error",
            );
            return;
        }
        if player.karate.is_some() {
            game.notice(
                &player.id,
                "Finish or leave your current tournament first.",
                "error",
            );
            return;
        }
        let tier = message["tier"]
            .as_str()
            .and_then(|id| game.activities.karate.iter().find(|t| t.id == id))
            .cloned();
        let Some(tier) = tier else {
            game.notice(
                &player.id,
                "That native karate tier is not available.",
                "error",
            );
            return;
        };
        let _ = game.teleport(player, "karate", 0., 0.8);
        player.facing = 0.;
        player.activity_result = None;
        player.karate = Some(KarateBout::new(tier, player, game.time));
        return;
    }
    let Some(mut bout) = player.karate.take() else {
        game.notice(&player.id, "Enter a Chao Karate tournament first.", "error");
        return;
    };
    if action == "karate-next" && bout.phase == "round-won" {
        bout.opponent_index += 1;
        let _ = game.teleport(player, "karate", 0., 0.8);
        player.facing = 0.;
        bout.reset_round(player, game.time);
    } else if bout.phase == "fighting" && game.time >= bout.player_until {
        match action {
            "karate-attack" => {
                let kick = message["move"] == "kick";
                let cost = if kick { 20. } else { 10. };
                if bout.player_stamina >= cost {
                    bout.player_stamina -= cost;
                    bout.player_action = if kick { "kick" } else { "punch" }.into();
                    let windup = if kick { 320. } else { 180. };
                    bout.player_hit_at = Some(game.time + windup);
                    bout.player_until = game.time + windup + if kick { 650. } else { 400. };
                    player.facing = (bout.opponent_x - player.x).atan2(bout.opponent_z - player.z);
                }
            }
            "karate-block" if bout.player_stamina >= 6. => {
                bout.player_stamina -= 6.;
                bout.player_action = "block".into();
                bout.player_until = game.time + 850.;
            }
            "karate-dodge"
                if bout.player_stamina >= 18.
                    && player.cooldown("karate-dodge", game.time, 1100.) =>
            {
                bout.player_stamina -= 18.;
                bout.player_action = "dodge".into();
                bout.player_until = game.time + 450.;
                player.x = (player.x
                    + if message["direction"] == "right" {
                        0.65
                    } else {
                        -0.65
                    })
                .clamp(-1.35, 1.35);
            }
            "karate-focus" => {
                bout.player_action = "focus".into();
                bout.player_until = game.time + 900.;
            }
            _ => {}
        }
    }
    player.karate = Some(bout);
}

pub(crate) fn tick_karate(game: &mut Game, player: &mut Player, dt: f64) {
    let Some(mut bout) = player.karate.take() else {
        return;
    };
    if player.zone != "karate" {
        return;
    }
    if bout.phase != "fighting" {
        player.karate = Some(bout);
        return;
    }
    if game.time >= bout.player_until {
        bout.player_action = "idle".into();
    }
    if game.time >= bout.opponent_until {
        bout.opponent_action = "idle".into();
    }
    let regen = if bout.player_action == "focus" {
        32.
    } else if bout.player_action == "idle" || bout.player_action == "recover" {
        9.
    } else {
        0.
    };
    bout.player_stamina = (bout.player_stamina + regen * dt).min(100.);
    bout.opponent_stamina = (bout.opponent_stamina + dt * 10.).min(100.);
    let separation = (player.x - bout.opponent_x).hypot(player.z - bout.opponent_z);
    if bout.opponent_action == "idle" && separation > 1.05 {
        let speed = 0.75 + bout.opponent().skill(2) * 0.6;
        bout.opponent_x += (player.x - bout.opponent_x) / separation * speed * dt;
        bout.opponent_z += (player.z - bout.opponent_z) / separation * speed * dt;
    }
    if bout.player_hit_at.is_some_and(|at| game.time >= at) {
        bout.player_hit_at = None;
        let kick = bout.player_action == "kick";
        bout.player_action = "recover".into();
        if separation <= 1.5 {
            let damage = if kick { 16. } else { 10. }
                + f64::from(player.stats["power"].level) / 99. * if kick { 15. } else { 12. };
            bout.opponent_hp = (bout.opponent_hp - damage).max(0.);
            emit_hit(game, player, &bout, "opponent", damage);
        }
    }
    if bout.opponent_hit_at.is_some_and(|at| game.time >= at) {
        bout.opponent_hit_at = None;
        let kick = bout.opponent_action == "kick";
        bout.opponent_action = "recover".into();
        if separation <= 1.5 && bout.player_action != "dodge" {
            let mut damage = if kick { 14. } else { 9. }
                + bout.opponent().skill(3) * if kick { 14. } else { 12. };
            if bout.player_action == "block" && bout.player_stamina >= 8. {
                damage *= 0.25;
                bout.player_stamina -= 8.;
            }
            bout.player_hp = (bout.player_hp - damage).max(0.);
            emit_hit(game, player, &bout, "player", damage);
        }
    }
    if bout.opponent_action == "idle" && game.time >= bout.next_ai_at && separation <= 1.5 {
        bout.decision += 1;
        let kick = bout.decision.is_multiple_of(3);
        let cost = if kick { 20. } else { 10. };
        if bout.opponent_stamina >= cost {
            bout.opponent_stamina -= cost;
            bout.opponent_action = if kick { "kick" } else { "punch" }.into();
            let windup = 650. - bout.opponent().skill(6) * 250. + if kick { 150. } else { 0. };
            bout.opponent_hit_at = Some(game.time + windup);
            bout.opponent_until = game.time + windup + 650.;
            bout.next_ai_at = bout.opponent_until + 200.;
        }
    }
    let timeout = game.time >= bout.ends_at;
    if bout.player_hp <= 0. || bout.opponent_hp <= 0. || timeout {
        let won = bout.opponent_hp <= 0.
            || (timeout
                && bout.player_hp / bout.player_max_hp > bout.opponent_hp / bout.opponent_max_hp);
        if won && bout.opponent_index + 1 < bout.tier.opponents.len() {
            bout.phase = "round-won".into();
        } else {
            bout.phase = if won { "won" } else { "lost" }.into();
            let elapsed = (game.time - bout.started_at).round();
            if won {
                let record = player
                    .karate_records
                    .entry(bout.tier.id.clone())
                    .or_default();
                record.completions = (record.completions + 1).min(99999);
                record.best_ms = Some(record.best_ms.map_or(elapsed, |v| v.min(elapsed)));
                player.rings += bout.tier.reward_rings;
                player.gain("power", 2);
                player.gain("stamina", 1);
                player.happiness = (player.happiness + 8.).min(100.);
                game.dirty = true;
            }
            player.activity_result = Some(
                json!({"kind":"karate","id":bout.id,"tierId":bout.tier.id,"won":won,"elapsedMs":elapsed,"rings":if won{bout.tier.reward_rings}else{0},"rules":"adapted"}),
            );
        }
    }
    player.karate = Some(bout);
}
fn emit_hit(game: &mut Game, player: &Player, bout: &KarateBout, target: &str, damage: f64) {
    game.events.push(Event{target:Some(player.id.clone()),message:json!({"type":"event","kind":"karate-hit","boutId":bout.id,"target":target,"damage":damage})});
}
