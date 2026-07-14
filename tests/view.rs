//! Information hygiene: what a [`View`] shows, and what it never leaks.

#![cfg(feature = "rand")]

use hearts_engine::hearts::{Hand, PassDirection, Phase, Rules, Seat, Suit};
use hearts_engine::{HeuristicBot, Table};
use rand::SeedableRng as _;
use rand::rngs::StdRng;

/// Drive `steps` decisions with the greedy bot.
fn advance(table: &mut Table, steps: usize) {
    let mut bot = HeuristicBot::new();
    for _ in 0..steps {
        table.step(&mut bot).expect("greedy decisions are legal");
    }
}

#[test]
fn passing_reveals_nothing_incoming() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut table = Table::deal(Rules::new(), PassDirection::Left, &mut rng);

    // North has passed; East has not.  Nobody sees incoming cards yet.
    advance(&mut table, 1);
    let north = table.view(Seat::North);
    assert_eq!(north.phase(), Phase::Passing);
    assert!(north.passed().is_some());
    assert_eq!(north.received(), None);
    assert_eq!(north.known_cards(Seat::East), Hand::EMPTY);
    let east = table.view(Seat::East);
    assert_eq!(east.passed(), None);
    assert_eq!(east.received(), None);
    // North's 10 remaining cards plus its own 3 passed are located; the
    // other 39 are not.
    assert_eq!(north.unseen().len(), 39);

    // After the exchange, everyone knows what they gave and got.
    advance(&mut table, 3);
    for seat in Seat::ALL {
        let view = table.view(seat);
        let passed = view.passed().expect("everyone has passed");
        let received = view.received().expect("the exchange happened");
        assert_eq!(view.phase(), Phase::Playing);
        assert_eq!(passed.len(), 3);
        assert_eq!(received.len(), 3);
        assert_eq!(view.hand() & received, received);
        // What I know the receiver holds is exactly my unplayed passes.
        let receiver = PassDirection::Left.receiver(seat);
        assert_eq!(view.known_cards(receiver), passed);
        for other in Seat::ALL {
            if other != receiver {
                assert_eq!(view.known_cards(other), Hand::EMPTY);
            }
        }
    }
}

#[test]
fn unseen_identity_holds_all_round() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut table = Table::deal(Rules::new(), PassDirection::Right, &mut rng);
    let mut bot = HeuristicBot::new();

    loop {
        for seat in Seat::ALL {
            let view = table.view(seat);
            let known: usize = Seat::ALL
                .iter()
                .map(|&other| view.known_cards(other).len())
                .sum();
            let others: usize = Seat::ALL
                .iter()
                .filter(|&&other| other != seat)
                .map(|&other| view.hand_len(other))
                .sum();
            // During passing the identity is offset by the hidden passes
            // in flight; once play starts it is exact.
            if view.phase() == Phase::Playing {
                assert_eq!(view.unseen().len(), others - known);
            }
        }
        if table
            .step(&mut bot)
            .expect("greedy decisions are legal")
            .is_some()
        {
            break;
        }
    }
}

#[test]
fn voids_are_sound_and_common_knowledge() {
    let mut rng = StdRng::seed_from_u64(3);
    let mut table = Table::deal(Rules::new(), PassDirection::Hold, &mut rng);
    let mut bot = HeuristicBot::new();

    while table.round().result().is_none() {
        table.step(&mut bot).expect("greedy decisions are legal");
        for observer in Seat::ALL {
            let view = table.view(observer);
            for seat in Seat::ALL {
                for suit in Suit::ASC {
                    if view.is_void(seat, suit) {
                        // Sound: a seat shown void really is void.
                        assert!(table.round().hand(seat)[suit].is_empty());
                        // Common knowledge: every observer agrees.
                        assert!(table.view(seat.left()).is_void(seat, suit));
                    }
                }
            }
        }
    }
}

#[test]
fn possible_cards_cover_the_truth() {
    let mut rng = StdRng::seed_from_u64(4);
    let mut table = Table::deal(Rules::new(), PassDirection::Across, &mut rng);
    let mut bot = HeuristicBot::new();

    while table.round().result().is_none() {
        table.step(&mut bot).expect("greedy decisions are legal");
        for observer in Seat::ALL {
            let view = table.view(observer);
            if view.phase() != Phase::Playing {
                continue;
            }
            for seat in Seat::ALL {
                let truth = table.round().hand(seat);
                assert_eq!(
                    truth - view.possible_cards(seat),
                    Hand::EMPTY,
                    "{observer} ruled out a card {seat} actually holds"
                );
            }
        }
    }
}

#[test]
fn game_scores_rotate_clockwise_from_the_seat() {
    let mut rng = StdRng::seed_from_u64(5);
    let table = Table::deal(Rules::new(), PassDirection::Hold, &mut rng).scores([1, 2, 3, 4]);
    assert_eq!(table.view(Seat::North).game_scores(), [1, 2, 3, 4]);
    assert_eq!(table.view(Seat::East).game_scores(), [2, 3, 4, 1]);
    assert_eq!(table.view(Seat::South).game_scores(), [3, 4, 1, 2]);
    assert_eq!(table.view(Seat::West).game_scores(), [4, 1, 2, 3]);
}
