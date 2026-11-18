use tiny_garden_games::{TinyError, TinyGame, TinyInput, TinySession, TinyView};

#[test]
fn memory_round_earns_a_once_only_result_using_public_information() {
    let mut game = TinySession::new(TinyGame::Memory, 825);
    let TinyView::Memory(peek) = game.view() else {
        unreachable!()
    };
    let mut remembered: Vec<_> = peek.cards.iter().map(|card| card.face).collect();
    let mut previous_move = None;
    for _ in 0..200 {
        game.advance(20).unwrap();
        let TinyView::Memory(view) = game.view() else {
            unreachable!()
        };
        if view.moving_card != previous_move {
            if let Some((from, to)) = view.moving_card {
                remembered.swap(usize::from(from), usize::from(to));
            }
            previous_move = view.moving_card;
        }
        // The final shuffle push also appears on the phase transition. The
        // public state must make that movement observable to the player.
        if view.phase == "choose" {
            break;
        }
    }
    for face in 0..7 {
        for (index, value) in remembered.iter().enumerate() {
            if *value == Some(face) {
                game.input(TinyInput::MemoryFlip { cell: index as u8 })
                    .unwrap();
            }
        }
    }
    let result = game.claim_result().unwrap();
    assert!(result.completed);
    assert!(result.rings >= 74);
    assert_eq!(result.reason, "all_pairs");
    assert_eq!(game.claim_result(), Err(TinyError::AlreadyClaimed));
}

#[test]
fn idle_games_end_from_server_time_or_physics_and_do_not_mint_rings() {
    for kind in [
        TinyGame::RockPaperScissors,
        TinyGame::Bounce,
        TinyGame::Ball,
        TinyGame::Search,
    ] {
        let mut game = TinySession::new(kind, 555);
        for _ in 0..600 {
            game.advance(1000).unwrap();
            if game.outcome().is_some() {
                break;
            }
        }
        let result = game
            .claim_result()
            .expect("idle round must reach its real terminal state");
        assert!(!result.completed);
        // Bounce can legitimately touch a ring during its automatic launch.
        if kind != TinyGame::Bounce {
            assert_eq!(result.rings, 0);
        }
        assert!(result.elapsed_ms > 0);
    }
}
