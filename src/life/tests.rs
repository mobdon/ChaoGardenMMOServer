use super::*;
use crate::game::Game;
use serde_json::json;

fn game_with(names: &[&str], owner: bool) -> (Game, Vec<String>) {
    let mut game = Game::new(vec![], BTreeMap::new(), owner, 1000.).unwrap();
    let ids = names
        .iter()
        .map(|name| {
            let session = game.create_session(&json!({ "name": name })).unwrap();
            let id = session["player"]["id"].as_str().unwrap().to_owned();
            game.connect(&id);
            id
        })
        .collect();
    (game, ids)
}
fn place(game: &mut Game, id: &str, x: f64, z: f64) {
    let mut p = game.players.remove(id).unwrap();
    game.teleport(&mut p, "garden", x, z).unwrap();
    game.players.insert(id.into(), p);
}
fn make_ready(p: &mut Player) {
    p.life.stage = Stage::Adult;
    p.happiness = 80.;
    p.life.emotion.state[3] = MATE_DESIRE_READY;
}
fn request(sequence: u32, revision: u64, intent: Value) -> Value {
    json!({"type":"life","requestId":format!("{sequence:08x}-0000-4000-8000-000000000002"),
        "expectedRevision":revision,"intent":intent})
}

#[test]
fn stat_progress_levels_by_grade_and_negative_removes_points_only() {
    let (mut game, ids) = game_with(&["Grower"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    p.stats.get_mut("swim").unwrap().grade = 5;
    let (level, points) = (p.stats["swim"].level, p.stats["swim"].points);
    let gained = grant_stat_progress(p, "swim", 250);
    assert_eq!(gained.levels, 2);
    assert_eq!(p.life.exp[0], 50);
    assert_eq!(p.stats["swim"].level, level + 2);
    // S grade: 27 + 0..=3 + level / 2 for each of the two levels.
    let low = 2 * 27 + u32::from(level + 1) / 2 + u32::from(level + 2) / 2;
    let gain = p.stats["swim"].points - points;
    assert!((low..=low + 6).contains(&gain), "{gain}");
    assert_eq!(gained.points as u32, gain);
    let lost = grant_stat_progress(p, "swim", -10_000);
    assert_eq!(p.stats["swim"].points, 0);
    assert_eq!(lost.points, -(gain as i32 + points as i32));
    assert_eq!(p.stats["swim"].level, level + 2);
    assert_eq!(
        grant_stat_progress(p, "charm", 500),
        StatProgress::default()
    );
    p.stats.get_mut("run").unwrap().level = 99;
    let capped = grant_stat_progress(p, "run", 1000);
    assert_eq!(capped.levels, 0);
    assert_eq!(p.life.exp[2], 99);
}

#[test]
fn owner_perfect_stats_never_change() {
    let (mut game, ids) = game_with(&["Owner"], true);
    let p = game.players.get_mut(&ids[0]).unwrap();
    assert!(p.admin);
    let before = p.stats.clone();
    assert_eq!(
        grant_stat_progress(p, "power", 900),
        StatProgress::default()
    );
    assert_eq!(
        grant_stat_progress(p, "power", -900),
        StatProgress::default()
    );
    reincarnate(p);
    assert_eq!(p.stats, before);
}

#[test]
fn needs_rise_with_time_and_feeding_projects_into_satiety() {
    let (mut game, ids) = game_with(&["Hungry"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    p.hunger = 100.;
    for _ in 0..600 {
        tick(p, 1., Policy::default());
    }
    let view = needs_view(p);
    assert!(view.sleepiness > 500 && view.boredom > 500 && view.lonely > 300);
    assert!(view.hunger > 0 && p.hunger < 100.);
    assert_eq!(view.hunger, hunger_state(p));
    assert_eq!(
        adjust_need(p, Need::Hunger, 10_000 * 2),
        10_000 - i32::from(view.hunger)
    );
    assert_eq!(p.hunger, 0.);
    let taste = p.life.emotion.taste as u8;
    let fed = feed(
        p,
        Food {
            fruit: Some(taste),
            nourish: 3000,
            joy: 5,
        },
    );
    assert_eq!(fed.hunger, -3000);
    assert!(fed.favorite);
    assert!((p.hunger - 30.).abs() < 1e-9);
    let other = (taste + 1) % NATIVE_FRUIT_KINDS;
    assert!(
        !feed(
            p,
            Food {
                fruit: Some(other),
                nourish: 0,
                joy: 0
            }
        )
        .favorite
    );
    let joy = i32::from(p.life.emotion.mood[0]);
    assert_eq!(adjust_mood(p, Mood::Joy, 1000), 100 - joy);
    assert_eq!(p.life.emotion.mood[0], 100);
    let energy = i32::from(needs_view(p).energy);
    assert_eq!(adjust_need(p, Need::Energy, -20_000), -energy);
    assert_eq!(p.energy, 0.);
}

#[test]
fn neglect_can_cause_illness_and_good_condition_cures_it() {
    let (mut game, ids) = game_with(&["Sick"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    let mut ill = false;
    for _ in 0..400 {
        p.hunger = 0.;
        p.life.emotion.state[7] = NEED_MAX;
        p.life.emotion.state[8] = 0;
        tick(p, ILL_CHECK_SECONDS, Policy::default());
        if is_ill(p) {
            ill = true;
            break;
        }
    }
    assert!(ill, "a starving, stressed Chao should eventually fall ill");
    for _ in 0..30 {
        p.hunger = 100.;
        p.life.emotion.state[7] = 0;
        p.life.emotion.state[8] = NEED_MAX;
        tick(p, ILL_CHECK_SECONDS, Policy::default());
    }
    assert!(!is_ill(p));
}

#[test]
fn child_cocoons_evolves_then_reincarnates_with_happiness() {
    let (mut game, ids) = game_with(&["Lifer"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    assert_eq!(p.life.stage, Stage::Child);
    for _ in 0..CHILD_ROLLOVERS {
        tick(p, ROLLOVER_SECONDS, Policy::default());
    }
    assert_eq!(p.life.stage, Stage::Cocoon);
    assert!(p.life.immobile());
    tick(p, COCOON_SECONDS, Policy::default());
    assert_eq!(p.life.stage, Stage::Adult);
    assert!(p.life.events.contains(&LifeEvent::EvolutionDue));
    assert!(p.life.events.contains(&LifeEvent::Evolved));
    p.stats.get_mut("run").unwrap().level = 40;
    p.stats.get_mut("run").unwrap().points = 2000;
    p.life.life = 1;
    p.happiness = 90.;
    tick(p, ROLLOVER_SECONDS, Policy::default());
    assert_eq!(p.life.cocoon.unwrap().kind, CocoonKind::Reincarnate);
    p.happiness = 90.;
    tick(p, COCOON_SECONDS, Policy::default());
    assert_eq!(p.life.stage, Stage::Child);
    assert_eq!(p.life.reincarnations, 1);
    assert_eq!(p.life.age, 0);
    assert_eq!(p.life.life, p.life.life_max);
    assert_eq!(p.stats["run"].level, 0);
    assert_eq!(p.stats["run"].points, 200);
}

#[test]
fn unhappy_end_waits_without_death_and_dies_only_behind_the_switch() {
    let (mut game, ids) = game_with(&["Grey", "Dead"], false);
    for (index, id) in ids.iter().enumerate() {
        let p = game.players.get_mut(id).unwrap();
        p.life.stage = Stage::Adult;
        p.life.life = 0;
        p.happiness = 10.;
        let policy = Policy { death: index == 1 };
        tick(p, 0.1, policy);
        assert_eq!(p.life.cocoon.unwrap().kind, CocoonKind::End);
        tick(p, COCOON_SECONDS * 5., policy);
        if policy.death {
            assert_eq!(p.life.stage, Stage::Dead);
        } else {
            assert_eq!(p.life.stage, Stage::Cocoon);
            p.happiness = 60.;
            tick(p, 0.1, policy);
            assert_eq!(p.life.stage, Stage::Child);
            assert_eq!(p.life.reincarnations, 1);
        }
    }
    // A dead Chao can hatch a fresh egg as a new generation.
    let dead = ids[1].clone();
    let response = action(&mut game, &dead, &request(1, 0, json!({"kind":"hatch"})));
    assert_eq!(response["code"], "hatched", "{response}");
    let p = &game.players[&dead];
    assert_eq!(p.life.stage, Stage::Child);
    assert_eq!(p.life.generation, 2);
    // Not allowed for a living child.
    let response = action(&mut game, &dead, &request(2, 1, json!({"kind":"hatch"})));
    assert_eq!(response["code"], "stage-conflict");
}

#[test]
fn chaos_chao_do_not_age_out() {
    let (mut game, ids) = game_with(&["Chaos"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    p.form = "chaos".into();
    p.life.stage = Stage::Adult;
    p.life.life = 1;
    for _ in 0..5 {
        tick(p, ROLLOVER_SECONDS, Policy { death: true });
    }
    assert_eq!(p.life.stage, Stage::Adult);
    assert_eq!(p.life.life, 1);
}

#[test]
fn consensual_breeding_lays_one_egg_with_parent_alleles_and_replays_safely() {
    let (mut game, ids) = game_with(&["Mother", "Father"], false);
    let (mother, father) = (ids[0].clone(), ids[1].clone());
    place(&mut game, &mother, -2., 2.);
    place(&mut game, &father, -1., 2.);
    for id in &ids {
        make_ready(game.players.get_mut(id).unwrap());
    }
    game.players.get_mut(&mother).unwrap().life.gene.abl[0] = [5, 5];
    game.players.get_mut(&father).unwrap().life.gene.abl[0] = [0, 0];
    let offer = request(1, 0, json!({"kind":"breed","partnerId":father}));
    let response = action(&mut game, &mother, &offer);
    assert_eq!(response["code"], "offer-sent", "{response}");
    assert!(game.players[&mother].life.nest.is_empty());
    let accept = request(1, 0, json!({"kind":"breed","partnerId":mother}));
    let response = action(&mut game, &father, &accept);
    assert_eq!(response["code"], "egg-laid", "{response}");
    let nest = &game.players[&mother].life.nest;
    assert_eq!(nest.len(), 1);
    assert_eq!(nest[0].gene.abl[0], [5, 0]);
    assert_eq!(nest[0].gene.mother_name.as_deref(), Some("Mother"));
    assert_eq!(nest[0].gene.father_name.as_deref(), Some("Father"));
    assert!(game.players[&father].life.nest.is_empty());
    for id in &ids {
        assert_eq!(game.players[id].life.emotion.state[3], 0);
        assert!(game.players[id].life.breed_offer.is_none());
    }
    // Retrying the accept replays; it never lays a second egg.
    let replay = action(&mut game, &father, &accept);
    assert_eq!(replay["replayed"], true);
    assert_eq!(game.players[&mother].life.nest.len(), 1);
    let reused = request(1, 0, json!({"kind":"greet","partnerId":mother}));
    assert_eq!(
        action(&mut game, &father, &reused)["code"],
        "request-id-conflict"
    );
    let stale = request(2, 0, json!({"kind":"greet","partnerId":mother}));
    assert_eq!(action(&mut game, &father, &stale)["code"], "stale-revision");
    // Cooldown: both parents must wait before breeding again.
    for id in &ids {
        game.players.get_mut(id).unwrap().life.emotion.state[3] = MATE_DESIRE_READY;
    }
    let again = request(
        3,
        game.players[&mother].life.revision,
        json!({"kind":"breed","partnerId":father}),
    );
    assert_eq!(action(&mut game, &mother, &again)["code"], "not-ready");
}

#[test]
fn breeding_rejects_children_strangers_far_partners_and_forged_fields() {
    let (mut game, ids) = game_with(&["A", "B"], false);
    place(&mut game, &ids[0], -2., 2.);
    place(&mut game, &ids[1], 6., -6.);
    let breed = |n: u32, partner: &str| request(n, 0, json!({"kind":"breed","partnerId":partner}));
    assert_eq!(
        action(&mut game, &ids[0], &breed(1, &ids[1]))["code"],
        "out-of-range"
    );
    place(&mut game, &ids[1], -1., 2.);
    assert_eq!(
        action(&mut game, &ids[0], &breed(1, &ids[1]))["code"],
        "not-ready"
    );
    assert_eq!(
        action(&mut game, &ids[0], &breed(1, &ids[0]))["code"],
        "invalid-partner"
    );
    assert_eq!(
        action(&mut game, &ids[0], &breed(1, "nobody"))["code"],
        "partner-unavailable"
    );
    let mut forged = breed(1, &ids[1]);
    forged["intent"]["egg"] = json!({"abl":[[5,5]]});
    assert_eq!(
        action(&mut game, &ids[0], &forged)["code"],
        "invalid-request"
    );
    assert_eq!(game.players[&ids[0]].life.revision, 0);
}

#[test]
fn greeting_builds_mutual_knowledge_with_cooldown() {
    let (mut game, ids) = game_with(&["Amy", "Bob"], false);
    place(&mut game, &ids[0], -2., 2.);
    place(&mut game, &ids[1], -1., 2.);
    let greet = request(1, 0, json!({"kind":"greet","partnerId":ids[1]}));
    assert_eq!(action(&mut game, &ids[0], &greet)["code"], "greeted");
    let a = &game.players[&ids[0]].life.knowledge[&ids[1]];
    assert_eq!((a.meet, a.name.as_str()), (1, "Bob"));
    assert!(a.like > 0);
    assert_eq!(game.players[&ids[1]].life.knowledge[&ids[0]].meet, 1);
    let again = request(2, 1, json!({"kind":"greet","partnerId":ids[1]}));
    assert_eq!(action(&mut game, &ids[0], &again)["code"], "cooldown");
}

#[test]
fn knowledge_keeps_at_most_twenty_chao() {
    let (mut game, ids) = game_with(&["Social"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    for n in 0..30 {
        meet(p, &format!("chao-{n}"), "Other", f64::from(n));
    }
    assert_eq!(p.life.knowledge.len(), MAX_KNOWN_CHAO);
    assert!(p.life.knowledge.contains_key("chao-29"));
}

#[test]
fn life_round_trips_and_invalid_saves_fail_closed() {
    let (mut game, ids) = game_with(&["Saver"], false);
    {
        let p = game.players.get_mut(&ids[0]).unwrap();
        grant_stat_progress(p, "fly", 150);
        p.life.stage = Stage::Adult;
        p.life.nest.push(Egg {
            id: Uuid::new_v4().to_string(),
            gene: p.life.gene.clone(),
            laid_at: 5.,
        });
        p.life.events.clear();
    }
    let profiles = game.export_profiles();
    let restored = Game::new(profiles.clone(), BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(restored.players[&ids[0]].life, game.players[&ids[0]].life);
    let mut bad = profiles.clone();
    bad[0]["life"]["exp"][0] = json!(100);
    assert!(Game::new(bad, BTreeMap::new(), false, 1000.).is_err());
    let mut newer = profiles.clone();
    newer[0]["life"]["version"] = json!(2);
    assert!(Game::new(newer, BTreeMap::new(), false, 1000.).is_err());
    let mut unknown = profiles.clone();
    unknown[0]["life"]["immortal"] = json!(true);
    assert!(Game::new(unknown, BTreeMap::new(), false, 1000.).is_err());
    // A legacy profile without life state migrates as a child with a seeded gene.
    let mut legacy = profiles;
    legacy[0].as_object_mut().unwrap().remove("life");
    let migrated = Game::new(legacy, BTreeMap::new(), false, 1000.).unwrap();
    let life = &migrated.players[&ids[0]].life;
    assert_eq!(life.stage, Stage::Child);
    assert_eq!(life.revision, 0);
    assert!(life.valid());
}

#[test]
fn snapshots_split_private_detail_from_public_summary() {
    let (game, ids) = game_with(&["Viewer"], false);
    let snapshot = game.snapshot(&ids[0]);
    assert_eq!(snapshot["life"]["stage"], "child");
    assert_eq!(snapshot["life"]["deathPolicy"], "disabled");
    assert_eq!(
        snapshot["life"]["provenance"]["rates"],
        "mmo-tuning-unverified"
    );
    assert!(snapshot["life"]["needs"]["sleepiness"].is_number());
    let public = &snapshot["players"][0]["life"];
    assert_eq!(public["stage"], "child");
    assert!(public.get("nest").is_none() && public.get("knowledge").is_none());
}
