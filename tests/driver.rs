//! The [`Table`] driver end to end: legality, retries, and whole games.

use hearts_engine::hearts::round::RoundError;
use hearts_engine::hearts::{Card, Hand, PassDirection, Phase, Round, Rules, Seat};
use hearts_engine::{EngineError, HeuristicBot, Strategy, Table, View, play_round};

/// The sorted deck dealt round-robin.
fn round_robin() -> [Hand; 4] {
    let mut hands = [Hand::EMPTY; 4];
    for (i, card) in Hand::ALL.into_iter().enumerate() {
        hands[i % 4].insert(card);
    }
    hands
}

/// A strategy that always tries the same illegal move.
struct Cheater;

impl Strategy for Cheater {
    fn pass_cards(&mut self, _: &View<'_>) -> [Card; 3] {
        // Three copies of one card collapse to a one-card set.
        [Card::QUEEN_OF_SPADES; 3]
    }

    fn play_card(&mut self, _: &View<'_>) -> Card {
        Card::QUEEN_OF_SPADES
    }

    fn name(&self) -> &str {
        "cheater"
    }
}

#[test]
fn a_full_round_of_greedy_bots() {
    let round = Round::from_deal(Rules::new(), PassDirection::Hold, round_robin()).unwrap();
    let [mut n, mut e, mut s, mut w] = [HeuristicBot::new(); 4];
    let result = play_round(round, [&mut n, &mut e, &mut s, &mut w]).unwrap();
    assert_eq!(
        result.points().iter().map(|&p| u32::from(p)).sum::<u32>(),
        26
    );
}

#[test]
fn passing_turn_serializes_in_seat_order() {
    let round = Round::from_deal(Rules::new(), PassDirection::Left, round_robin()).unwrap();
    let mut table = Table::new(round);
    let mut bot = HeuristicBot::new();

    assert_eq!(table.turn(), Some(Seat::North));
    table.step(&mut bot).unwrap();
    assert_eq!(table.turn(), Some(Seat::East));
    table.step(&mut bot).unwrap();
    table.step(&mut bot).unwrap();
    assert_eq!(table.turn(), Some(Seat::West));
    table.step(&mut bot).unwrap();
    assert_eq!(table.round().phase(), Phase::Playing);
}

#[test]
fn illegal_actions_leave_the_table_retryable() {
    let round = Round::from_deal(Rules::new(), PassDirection::Left, round_robin()).unwrap();
    let mut table = Table::new(round);
    let before = table.round().clone();

    // A degenerate pass (one distinct card) is rejected...
    let error = table.step(&mut Cheater).unwrap_err();
    let EngineError::IllegalAction { seat, source } = error else {
        panic!("a rejected step is an IllegalAction");
    };
    assert_eq!(seat, Seat::North);
    assert_eq!(source, RoundError::WrongPassCount(1));
    assert_eq!(table.round(), &before, "the table is untouched");

    // ...and an honest retry succeeds.
    let mut honest = HeuristicBot::new();
    table.step(&mut honest).unwrap();
    assert_eq!(table.turn(), Some(Seat::East));

    // Same again mid-play: whoever cheats is named, nothing moves.
    let mut hold =
        Table::new(Round::from_deal(Rules::new(), PassDirection::Hold, round_robin()).unwrap());
    let leader = hold.turn().unwrap();
    let error = hold.step(&mut Cheater).unwrap_err();
    let EngineError::IllegalAction { seat, source } = error else {
        panic!("a rejected step is an IllegalAction");
    };
    assert_eq!(seat, leader);
    assert!(matches!(
        source,
        RoundError::NotInHand(_) | RoundError::MustLeadTwoOfClubs
    ));
    hold.step(&mut honest).unwrap();
    assert_eq!(hold.round().played().len(), 1);
}

#[cfg(feature = "rand")]
mod seeded {
    use super::*;
    use hearts_engine::hearts::Game;
    use hearts_engine::play_game;
    use rand::SeedableRng as _;
    use rand::rngs::StdRng;

    #[test]
    fn seeded_deals_replay_identically() {
        let deal = |seed| {
            let mut rng = StdRng::seed_from_u64(seed);
            Table::deal(Rules::new(), PassDirection::Left, &mut rng)
                .round()
                .clone()
        };
        assert_eq!(deal(42), deal(42));
        assert_ne!(deal(42), deal(43));
    }

    #[test]
    fn a_whole_game_settles() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut game = Game::new(Rules::new());
        let [mut n, mut e, mut s, mut w] = [HeuristicBot::new(); 4];
        let settled = play_game(&mut game, [&mut n, &mut e, &mut s, &mut w], &mut rng).unwrap();

        assert!(game.is_over());
        assert_eq!(settled.totals, game.scores());
        assert!(settled.winners().count() >= 1);
        assert!(settled.totals.iter().any(|&t| t >= 100));
    }
}
