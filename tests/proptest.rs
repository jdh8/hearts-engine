//! Property tests: every driven round terminates legally and conserves the
//! 26 points, whatever the seed.

#![cfg(feature = "rand")]

use hearts_engine::hearts::{Game, PassDirection, Rules, Seat};
use hearts_engine::{HeuristicBot, MonteCarloBot, Table};
use proptest::prelude::*;
use rand::SeedableRng as _;
use rand::rngs::StdRng;

proptest! {
    // Whole rounds are slow; a few dozen seeds catch drift.
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn greedy_rounds_terminate_and_conserve_points(seed in any::<u64>(), deal in 0u32..4) {
        let mut rng = StdRng::seed_from_u64(seed);
        let direction = PassDirection::from_deal_index(deal);
        let mut table = Table::deal(Rules::new(), direction, &mut rng);
        let mut bot = HeuristicBot::new();

        let mut steps = 0;
        let result = loop {
            if let Some(result) = table.step(&mut bot).unwrap() {
                break result;
            }
            steps += 1;
            prop_assert!(steps <= 56, "a round is at most 4 passes + 52 plays");
        };

        prop_assert_eq!(table.round().tricks().len(), 13);
        let points = result.points();
        prop_assert_eq!(points.iter().map(|&p| u32::from(p)).sum::<u32>(), 26);
        for seat in Seat::ALL {
            prop_assert_eq!(table.round().points_taken(seat), points[seat as usize]);
        }
    }

    #[test]
    fn recorded_rounds_charge_26_or_a_moons_78(seed in any::<u64>()) {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut game = Game::new(Rules::new());
        let mut bot = HeuristicBot::new();

        let mut table = Table::deal(Rules::new(), game.next_direction(), &mut rng);
        let result = loop {
            if let Some(result) = table.step(&mut bot).unwrap() {
                break result;
            }
        };
        let before: u32 = game.scores().iter().copied().map(u32::from).sum();
        game.record(result).unwrap();
        let after: u32 = game.scores().iter().copied().map(u32::from).sum();

        let charged = after - before;
        match result.shooter() {
            Some(_) => prop_assert_eq!(charged, 78),
            None => prop_assert_eq!(charged, 26),
        }
    }
}

#[test]
fn a_small_mc_bot_survives_a_full_round() {
    // One seeded round with the Monte Carlo bot in every seat: the
    // reconstruction, sampling, and rollouts hold up under real play.
    let mut rng = StdRng::seed_from_u64(99);
    let mut table = Table::deal(Rules::new(), PassDirection::Left, &mut rng);
    let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(1)).samples(8);
    let result = loop {
        if let Some(result) = table.step(&mut bot).unwrap() {
            break result;
        }
    };
    assert_eq!(
        result.points().iter().map(|&p| u32::from(p)).sum::<u32>(),
        26
    );
}
