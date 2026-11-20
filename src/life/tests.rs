use super::*;
use crate::game::Game;
use serde_json::json;

const CALC_SECONDS: f64 = CALC_FRAMES / FRAMES_PER_SECOND;

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
/// A Chao with neutral personality and zeroed needs, so native steps are exact.
fn neutral(p: &mut Player) {
    p.life.emotion.personality = [0; 13];
    p.life.emotion.state = [0; 11];
    p.life.emotion.mood = [0; 8];
    p.life.emotion.ill_state = [0; 6];
    p.hunger = 100.;
}
/// Run `calls` AL_CalcParameter calls.
fn calc(p: &mut Player, calls: u32, connected: bool) {
    for _ in 0..calls {
        tick(p, CALC_SECONDS, Policy::default(), connected, true);
    }
}
fn request(sequence: u32, revision: u64, intent: Value) -> Value {
    json!({"type":"life","requestId":format!("{sequence:08x}-0000-4000-8000-000000000002"),
        "expectedRevision":revision,"intent":intent})
}

#[test]
fn level_up_is_one_per_call_with_native_points_and_caps() {
    let (mut game, ids) = game_with(&["Grower"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    p.stats.get_mut("swim").unwrap().grade = 5;
    p.stats.get_mut("swim").unwrap().points = 0;
    let level = p.stats["swim"].level;
    let gained = grant_stat_progress(p, "swim", 250);
    assert_eq!(gained.levels, 1);
    assert_eq!(p.life.exp[0], 150);
    assert_eq!(p.stats["swim"].level, level + 1);
    assert!(
        (26..=30).contains(&p.stats["swim"].points),
        "S grade gives 26..30"
    );
    let lowered = grant_stat_progress(p, "swim", -1000);
    assert_eq!(p.life.exp[0], 0);
    assert_eq!(lowered.points, 0, "negative rows lower Exp only");
    p.stats.get_mut("run").unwrap().grade = 0;
    p.stats.get_mut("run").unwrap().points = 3995;
    grant_stat_progress(p, "run", 100);
    assert_eq!(p.stats["run"].points, 4000, "per-skill cap at level-up");
    p.stats.get_mut("fly").unwrap().level = 99;
    let capped = grant_stat_progress(p, "fly", 300);
    assert_eq!((capped.levels, p.life.exp[1]), (0, 99));
    assert_eq!(
        grant_stat_progress(p, "charm", 500),
        StatProgress::default()
    );
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
    reincarnate(p, Policy::default());
    assert_eq!(p.stats, before);
}

#[test]
fn needs_tick_every_eleven_calls_with_native_steps() {
    let (mut game, ids) = game_with(&["Needs"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    neutral(p);
    calc(p, 10, true);
    assert_eq!(p.life.emotion.state, [0; 11]);
    calc(p, 1, true);
    assert!(take_need_update(p));
    assert!(!take_need_update(p));
    let v = needs_view(p);
    assert_eq!(
        (v.sleepy, v.hunger, v.breed, v.tedious, v.lonely, v.tire),
        (90, 70, 0, 120, 60, 10)
    );
    assert_eq!((v.nourishment, v.condition, v.thirsty), (0, 0, 150));
    assert!(
        (p.hunger - 99.3).abs() < 1e-9,
        "hunger projects onto satiety"
    );
    // Adults accumulate breed; personality scales by (300 ± P) / 300, truncated.
    p.life.stage = Stage::Adult;
    p.life.emotion.personality[GLUTTON] = 100;
    p.life.emotion.personality[SOLITUDE] = 100;
    p.life.emotion.state[8] = 9500;
    calc(p, 11, true);
    let v = needs_view(p);
    assert_eq!(v.breed, 12);
    assert_eq!(v.hunger, 70 + 93);
    assert_eq!(v.lonely, 60 + 40);
    assert_eq!(v.condition, 80);
    // Sleepy grows with tire: 90 * (1 + 0.00006 * tire).
    neutral(p);
    p.life.emotion.state[6] = 10000;
    p.life.need_timer = NEED_INTERVAL;
    calc(p, 1, true);
    // Single-precision float: 90 * (1 + 0.00006 * 10000) truncates to 143.
    assert_eq!(p.life.emotion.state[0], 143);
}

#[test]
fn busy_cadence_is_ten_calls() {
    let (mut game, ids) = game_with(&["Busy"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    neutral(p);
    set_need_cadence_busy(p, true);
    calc(p, 11, true);
    assert!(take_need_update(p));
    calc(p, 9, true);
    assert!(!take_need_update(p));
    calc(p, 1, true);
    assert!(take_need_update(p));
}

#[test]
fn moods_fall_by_personality_scaled_fours() {
    let (mut game, ids) = game_with(&["Moody"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    neutral(p);
    p.life.emotion.mood = [200; 8];
    p.life.emotion.personality[AGGRESSIVE] = 100;
    calc(p, 8, true);
    let m = p.life.emotion.mood;
    assert_eq!(m[0], 196);
    assert_eq!(
        m[1],
        200 - 2 - 4 - 4,
        "anger: 4(300-aggr)/300, kindness, calm"
    );
    assert_eq!(m[2], 200 - 5 - 4);
    assert_eq!(m[7], 200, "total is never touched");
}

#[test]
fn adult_lifespan_decays_only_on_connected_life_ticks() {
    let (mut game, ids) = game_with(&["Elder"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    p.life.stage = Stage::Adult;
    p.life.life = 3000;
    p.life.life_max = 3000;
    calc(p, 121 * 3, false);
    assert_eq!(
        (p.life.age, p.life.life_max),
        (0, 3000),
        "offline residents do not age"
    );
    calc(p, 121, true);
    assert_eq!(p.life.age, 1);
    assert_eq!(p.life.adult_age, 1);
    assert_eq!(p.life.life_max, 3000 - 101 / 15);
    assert_eq!(p.life.life, p.life.life_max);
    p.form = "chaos".into();
    calc(p, 121, true);
    assert_eq!(
        p.life.life_max,
        3000 - 101 / 15,
        "Chaos Chao do not age out"
    );
}

#[test]
fn illness_counts_toward_zero_and_cure_gives_immunity() {
    let (mut game, ids) = game_with(&["Sick"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    p.life.emotion.ill_state = [ILL_START, 0, 0, 0, 0, 5];
    assert!(is_ill(p));
    calc(p, 81, true);
    assert_eq!(p.life.emotion.ill_state[0], -99);
    assert_eq!(p.life.emotion.ill_state[5], 4);
    assert!(cure(p));
    assert_eq!(p.life.emotion.ill_state[0], IMMUNE_START);
    assert!(!is_ill(p));
    assert!(!cure(p));
}

#[test]
fn evolution_cocoon_follows_mayu_frames() {
    let (mut game, ids) = game_with(&["Child"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    p.life.like = 90;
    p.life.reincarnations = 2;
    assert!(start_evolution_cocoon(p));
    assert!(p.life.immobile());
    tick(p, 1999. / FRAMES_PER_SECOND, Policy::default(), true, true);
    assert!(p.life.events.is_empty());
    tick(p, 1. / FRAMES_PER_SECOND, Policy::default(), true, true);
    assert_eq!(
        p.life.events,
        vec![LifeEvent::EvolutionDecision(ChaosInputs {
            user_like: 90,
            reincarnations: 2
        })]
    );
    tick(p, 1900. / FRAMES_PER_SECOND, Policy::default(), true, true);
    assert_eq!(p.life.stage, Stage::Adult);
    assert!(!start_evolution_cocoon(p));
}

fn end_of_life(p: &mut Player, like: i16, stress: i16, policy: Policy) {
    p.life.stage = Stage::Adult;
    p.life.life = 0;
    p.life.life_max = 0;
    p.life.like = like;
    p.life.emotion.state[7] = stress;
    tick(p, 0.01, policy, true, true);
    assert_eq!(p.life.cocoon.unwrap().kind, CocoonKind::End);
    tick(p, 3831. / FRAMES_PER_SECOND, policy, true, true);
}

#[test]
fn pink_cocoon_reincarnates_with_native_carry_over() {
    let (mut game, ids) = game_with(&["Pink"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    p.stats.get_mut("run").unwrap().points = 4000;
    p.stats.get_mut("swim").unwrap().points = 2345;
    p.life.gene.life_time = [4, 7];
    p.life.emotion.mood = [50; 8];
    end_of_life(p, 60, 0, Policy::default());
    assert_eq!(p.life.stage, Stage::Child);
    assert_eq!(p.life.reincarnations, 1);
    assert_eq!(p.stats["run"].points, 400);
    assert_eq!(p.stats["swim"].points, 234);
    assert!(p.stats.values().all(|s| s.level == 1));
    assert_eq!(p.life.life_max, 3700, "old LifeMax 0 adds nothing");
    // mob's choice: the native init/rebirth bugs are fixed.
    assert!(p.life.gene.taste.contains(&p.life.emotion.taste));
    assert_eq!(p.life.emotion.mood, [0; 8]);
    assert_eq!(p.life.emotion.state[8..10], [10000, 10000]);
    assert!((p.hunger - (100. - f64::from(p.life.emotion.state[2]) / 100.)).abs() < 1e-9);
    assert_eq!(p.life.like, 60, "like is kept");
    assert!(p.life.events.contains(&LifeEvent::Reincarnated));
}

#[test]
fn grey_cocoon_waits_without_death_and_dies_behind_the_switch() {
    let (mut game, ids) = game_with(&["Grey", "Dead"], false);
    let waiting = Policy::default();
    let p = game.players.get_mut(&ids[0]).unwrap();
    end_of_life(p, 50, 0, waiting);
    assert_eq!(p.life.stage, Stage::Cocoon, "like must be above 50");
    p.life.like = 51;
    tick(p, 0.1, waiting, true, true);
    assert_eq!(p.life.stage, Stage::Child);

    let dying = Policy {
        death: true,
        ..Policy::default()
    };
    let dead = ids[1].clone();
    let p = game.players.get_mut(&dead).unwrap();
    end_of_life(p, 90, 700, dying);
    assert_eq!(p.life.stage, Stage::Dead, "stress 700 fails the pink check");
    let egg_gene = p.life.gene.clone();
    let expressed = p.life.analyze(&egg_gene);
    let egg_id = Uuid::new_v4().to_string();
    p.life.nest.push(Egg {
        id: egg_id.clone(),
        gene: egg_gene,
        expressed: expressed.clone(),
        laid_at: 1.,
    });
    let response = action(
        &mut game,
        &dead,
        &request(1, 0, json!({"kind":"hatch","eggId":egg_id})),
    );
    assert_eq!(response["code"], "hatched", "{response}");
    let p = &game.players[&dead];
    assert_eq!((p.life.stage, p.life.generation), (Stage::Child, 2));
    assert_eq!(p.life.emotion.personality, expressed.personality);
    assert_eq!(p.stats["swim"].grade, u16::from(expressed.abl[0]));
    assert_eq!(p.life.like, 0);
}

#[test]
fn fruit_bites_sum_to_the_row_and_children_gain_life_max() {
    let (mut game, ids) = game_with(&["Eater"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    neutral(p);
    p.life.life = 3000;
    p.life.life_max = 3000;
    p.life.exp = [50; 7];
    let row = [10, -7, 0, 0, 40, 0, 0];
    for bite in 1..=4 {
        eat_fruit_bite(p, 0, row, bite);
    }
    assert_eq!(p.life.exp[0], 60);
    assert_eq!(p.life.exp[1], 43);
    assert_eq!(p.life.exp[4], 90);
    assert_eq!((p.life.life, p.life.life_max), (3012, 3020));
    assert_eq!(needs_view(p).hunger, 0);
    p.hunger = 0.;
    eat_fruit_bite(p, 0, [0; 7], 1);
    assert_eq!(needs_view(p).hunger, 10000 - 2400);
}

#[test]
fn petting_raises_like_and_lowers_fear() {
    let (mut game, ids) = game_with(&["Pet"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    p.life.emotion.personality = [0; 13];
    add_user_fear(p, 50);
    assert_eq!(p.life.owner.fear, 100);
    p.life.owner.distance = 300;
    pet(p);
    assert_eq!((p.life.like, p.life.owner.like), (1, 1));
    assert_eq!(p.life.owner.fear, 80);
    assert_eq!((p.life.owner.meet, p.life.owner.distance), (1, 200));
}

#[test]
fn new_genes_follow_native_ranges_and_expression() {
    let mut life = LifeState::blank(42, Stage::Child);
    for _ in 0..200 {
        let gene = life.create_gene();
        assert!(gene.abl.iter().all(|a| a[0] == a[1] && a[0] <= 5));
        assert!(gene.life_time.iter().all(|v| (0..=9).contains(v)));
        let mut shiny = gene.clone();
        shiny.multi = [false, true];
        shiny.color = [0, 7];
        let e = life.analyze(&shiny);
        assert!(e.look.shiny);
        assert_eq!(e.look.color, 7, "non-normal colour dominates");
        assert!(
            e.personality
                .iter()
                .all(|p| [-80, -40, 0, 40, 80].contains(p))
        );
        assert_eq!(
            e.life_max,
            3000 + 100 * i16::from(gene.life_time[0].max(gene.life_time[1]))
        );
    }
    let mut mixed = life.create_gene();
    mixed.abl[0] = [5, 1];
    let high = (0..2000)
        .filter(|_| life.analyze(&mixed).abl[0] == 5)
        .count();
    assert!(
        (1300..=1500).contains(&high),
        "about 70% higher allele: {high}"
    );
}

#[test]
fn breeding_needs_native_desire_and_lays_one_egg_with_parent_alleles() {
    let (mut game, ids) = game_with(&["Mother", "Father"], false);
    let (mother, father) = (ids[0].clone(), ids[1].clone());
    place(&mut game, &mother, -2., 2.);
    place(&mut game, &father, -1., 2.);
    for id in &ids {
        game.players.get_mut(id).unwrap().life.stage = Stage::Adult;
    }
    game.players.get_mut(&mother).unwrap().life.gene.abl[0] = [5, 5];
    game.players.get_mut(&father).unwrap().life.gene.abl[0] = [0, 0];
    let offer = request(1, 0, json!({"kind":"breed","partnerId":father}));
    game.players.get_mut(&mother).unwrap().life.emotion.state[3] = 9999;
    assert_eq!(action(&mut game, &mother, &offer)["code"], "not-ready");
    game.players.get_mut(&mother).unwrap().life.emotion.state[3] = BREED_INVITE;
    assert_eq!(action(&mut game, &mother, &offer)["code"], "offer-sent");
    let accept = request(1, 0, json!({"kind":"breed","partnerId":mother}));
    game.players.get_mut(&father).unwrap().life.emotion.state[3] = BREED_ANSWER;
    assert_eq!(action(&mut game, &father, &accept)["code"], "not-ready");
    game.players.get_mut(&father).unwrap().life.emotion.state[3] = BREED_ANSWER + 1;
    let response = action(&mut game, &father, &accept);
    assert_eq!(response["code"], "egg-laid", "{response}");
    let egg = &game.players[&mother].life.nest[0];
    assert_eq!(egg.gene.abl[0], [5, 0]);
    assert_eq!(egg.gene.father_name.as_deref(), Some("Father"));
    assert!([0, 5].contains(&egg.expressed.abl[0]));
    for id in &ids {
        assert_eq!(game.players[id].life.emotion.state[3], 0);
    }
    assert_eq!(action(&mut game, &father, &accept)["replayed"], true);
    assert_eq!(game.players[&mother].life.nest.len(), 1);
    let reused = request(1, 0, json!({"kind":"release-egg","eggId":"x"}));
    assert_eq!(
        action(&mut game, &father, &reused)["code"],
        "request-id-conflict"
    );
    let stale = request(2, 0, json!({"kind":"release-egg","eggId":"x"}));
    assert_eq!(action(&mut game, &father, &stale)["code"], "stale-revision");
}

#[test]
fn a_full_garden_cancels_the_mate_call() {
    let names = ["A", "B", "C", "D", "E", "F", "G", "H"];
    let (mut game, ids) = game_with(&names, false);
    for (i, id) in ids.iter().enumerate() {
        place(&mut game, id, -2. + i as f64 * 0.1, 2.);
        game.players.get_mut(id).unwrap().life.stage = Stage::Adult;
    }
    game.players.get_mut(&ids[0]).unwrap().life.emotion.state[3] = BREED_INVITE;
    let offer = request(1, 0, json!({"kind":"breed","partnerId":ids[1]}));
    assert_eq!(action(&mut game, &ids[0], &offer)["code"], "garden-full");
    assert_eq!(game.players[&ids[0]].life.emotion.state[3], 0);
}

#[test]
fn breeding_rejects_children_far_partners_and_forged_fields() {
    let (mut game, ids) = game_with(&["A", "B"], false);
    place(&mut game, &ids[0], -2., 2.);
    place(&mut game, &ids[1], 6., -6.);
    let breed = |n: u32, partner: &str| request(n, 0, json!({"kind":"breed","partnerId":partner}));
    assert_eq!(
        action(&mut game, &ids[0], &breed(1, &ids[1]))["code"],
        "stage-conflict"
    );
    for id in &ids {
        game.players.get_mut(id).unwrap().life.stage = Stage::Adult;
    }
    assert_eq!(
        action(&mut game, &ids[0], &breed(1, &ids[1]))["code"],
        "out-of-range"
    );
    assert_eq!(
        action(&mut game, &ids[0], &breed(1, &ids[0]))["code"],
        "invalid-partner"
    );
    let mut forged = breed(1, &ids[1]);
    forged["intent"]["gene"] = json!({"abl":[[5,5]]});
    assert_eq!(
        action(&mut game, &ids[0], &forged)["code"],
        "invalid-request"
    );
    let greet = request(1, 0, json!({"kind":"greet","partnerId":ids[1]}));
    assert_eq!(
        action(&mut game, &ids[0], &greet)["code"],
        "invalid-request"
    );
    assert_eq!(game.players[&ids[0]].life.revision, 0);
}

#[test]
fn life_round_trips_and_invalid_saves_fail_closed() {
    let (mut game, ids) = game_with(&["Saver"], false);
    {
        let p = game.players.get_mut(&ids[0]).unwrap();
        grant_stat_progress(p, "fly", 50);
        calc(p, 30, true);
        p.life.events.clear();
        p.life.need_updated = false;
    }
    let profiles = game.export_profiles();
    let restored = Game::new(profiles.clone(), BTreeMap::new(), false, 1000.).unwrap();
    assert_eq!(restored.players[&ids[0]].life, game.players[&ids[0]].life);
    for (path, value) in [
        ("exp", json!([100, 0, 0, 0, 0, 0, 0])),
        ("version", json!(1)),
        ("like", json!(101)),
        ("lifeTimer", json!(121)),
    ] {
        let mut bad = profiles.clone();
        bad[0]["life"][path] = value;
        assert!(
            Game::new(bad, BTreeMap::new(), false, 1000.).is_err(),
            "{path}"
        );
    }
    let mut unknown = profiles.clone();
    unknown[0]["life"]["immortal"] = json!(true);
    assert!(Game::new(unknown, BTreeMap::new(), false, 1000.).is_err());
    let mut legacy = profiles;
    legacy[0].as_object_mut().unwrap().remove("life");
    let migrated = Game::new(legacy, BTreeMap::new(), false, 1000.).unwrap();
    let life = &migrated.players[&ids[0]].life;
    assert_eq!((life.stage, life.revision), (Stage::Child, 0));
    assert!(life.valid());
}

#[test]
fn snapshots_split_private_detail_from_public_summary() {
    let (game, ids) = game_with(&["Viewer"], false);
    let snapshot = game.snapshot(&ids[0]);
    assert_eq!(snapshot["life"]["stage"], "child");
    assert_eq!(snapshot["life"]["policy"]["death"], false);
    assert!(snapshot["life"]["needs"]["sleepDepth"].is_number());
    assert!(snapshot["life"]["mood"]["pleasure"].is_number());
    let public = &snapshot["players"][0]["life"];
    assert_eq!(public["stage"], "child");
    assert!(public.get("nest").is_none() && public.get("owner").is_none());
}

#[test]
fn native_bug_switch_reproduces_the_original_init() {
    let (mut game, ids) = game_with(&["Buggy"], false);
    let p = game.players.get_mut(&ids[0]).unwrap();
    let buggy = Policy {
        native_bugs: true,
        ..Policy::default()
    };
    p.life.emotion.mood = [50; 8];
    p.life.emotion.state[0] = 0x1234;
    reincarnate(p, buggy);
    assert_eq!(
        (
            p.life.emotion.taste,
            p.life.emotion.tv,
            p.life.emotion.music
        ),
        (16, 0, 0)
    );
    assert_eq!(p.life.emotion.mood[0], 50);
    assert_eq!(p.life.emotion.state[0], 0x34);
    assert_eq!(p.life.emotion.state[9], 0);
}

#[test]
fn life_events_drive_the_body() {
    let (mut game, ids) = game_with(&["Body"], false);
    let id = &ids[0];
    let p = game.players.get_mut(id).unwrap();
    p.body.h_pos = 0.8;
    p.body.a_pos = 0.6;
    p.life.events = vec![
        LifeEvent::LifeTick,
        LifeEvent::FruitBite {
            kind: 5,
            child: true,
            last: true,
        },
    ];
    apply_events(&mut game, id);
    let body = &game.players[id].body;
    assert!((body.growth - (1. / 60. + 0.005)).abs() < 1e-5);
    assert!((body.a_pos - 0.75).abs() < 1e-5);

    let p = game.players.get_mut(id).unwrap();
    p.life.events = vec![LifeEvent::EvolutionDecision(ChaosInputs {
        user_like: 100,
        reincarnations: 3,
    })];
    apply_events(&mut game, id);
    let p = &game.players[id];
    assert!(!p.body.is_child());
    assert_eq!(p.body.type_name(), "hero-power");
    assert_eq!(p.body.h_pos, 0.);

    let p = game.players.get_mut(id).unwrap();
    p.life.events = vec![LifeEvent::Hatched {
        look: Look {
            color: 4,
            jewel: 0,
            shiny: true,
            monotone: false,
        },
    }];
    apply_events(&mut game, id);
    let body = &game.players[id].body;
    assert!(body.is_child());
    assert_eq!(body.look.color, 4);
    assert!(body.look.shiny);
}
