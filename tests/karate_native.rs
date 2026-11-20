//! Native Chao Karate rounds against the owner's local data (never committed).
use chao_game_server::{behaviour::MotionTable, competition_native::MslRand, karate};
use std::path::PathBuf;

#[test]
#[ignore = "requires the owner's local imports (CHAO_KARATE_IMPORTS)"]
fn native_karate_tournament_runs_end_to_end() {
    let dir = PathBuf::from(std::env::var_os("CHAO_KARATE_IMPORTS").expect("CHAO_KARATE_IMPORTS"));
    let data = karate::Data::load(&dir).unwrap().expect("karate data");
    let motion = MotionTable::load(&dir.join("chao-motion-table.json")).unwrap();
    let mut rng = MslRand::default();
    for (skills, class) in [
        ([0u16; 7], 0u8),
        ([999; 7], 0),
        ([999; 7], 3),
        ([400; 7], 1),
    ] {
        let t = karate::tournament(&data, &motion, &mut rng, skills, false, class);
        println!("{}", serde_json::to_string(&t).unwrap());
        assert!(!t.rounds.is_empty());
        assert!(t.rounds.iter().all(|r| r.frames > 0));
    }
}
