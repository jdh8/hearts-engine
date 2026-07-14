//! An `#[ignore]`d strength tripwire: the Monte Carlo bot must keep
//! beating the greedy baseline it deviates from.
//!
//! Run it on demand (release mode, or budget a few minutes):
//!
//! ```console
//! cargo test --release --test strength -- --ignored
//! ```

#![cfg(feature = "rand")]

use hearts_engine::hearts::{PassDirection, Rules, Seat};
use hearts_engine::{HeuristicBot, MonteCarloBot, Strategy, Table};
use rand::SeedableRng as _;
use rand::rngs::StdRng;

/// Rounds per measurement; the bar below was calibrated at this count.
const ROUNDS: u64 = 200;

#[test]
#[ignore = "strength measurement: minutes of rollouts, run on demand"]
fn mc_outscores_greedy() {
    let mut mc_total: f64 = 0.0;
    let mut greedy_total: f64 = 0.0;

    for trial in 0..ROUNDS {
        // The MC bot rotates through the seats so no seat bias leaks in.
        let mc_seat = Seat::ALL[(trial % 4) as usize];
        let mut deal_rng = StdRng::seed_from_u64(trial);
        let direction = PassDirection::from_deal_index(trial as u32);
        let mut table = Table::deal(Rules::new(), direction, &mut deal_rng);

        let mut mc = MonteCarloBot::new(StdRng::seed_from_u64(trial ^ 0xDEC0)).samples(128);
        let mut greedy = HeuristicBot::new();
        let result = loop {
            let seat = table.turn().expect("an unfinished round has a mover");
            let strategy: &mut dyn Strategy = if seat == mc_seat {
                &mut mc
            } else {
                &mut greedy
            };
            if let Some(result) = table.step(strategy).unwrap() {
                break result;
            }
        };

        let scores = result.scores(&Rules::new());
        mc_total += f64::from(scores[mc_seat as usize]);
        greedy_total += Seat::ALL
            .iter()
            .filter(|&&s| s != mc_seat)
            .map(|&s| f64::from(scores[s as usize]))
            .sum::<f64>()
            / 3.0;
    }

    let mc_mean = mc_total / ROUNDS as f64;
    let greedy_mean = greedy_total / ROUNDS as f64;
    println!("mc:128 mean {mc_mean:.2} vs greedy mean {greedy_mean:.2} over {ROUNDS} rounds");

    // Tripwire bar: the MC bot must average at least two points per
    // round better than the greedy field.  First calibration (2026-07)
    // measured 3.86 vs 7.73 — a 3.9-point edge — so 2.0 trips on a real
    // regression while riding out sampling noise.  Re-calibrate against
    // the printout whenever sampling or policy logic changes materially.
    assert!(
        mc_mean + 2.0 <= greedy_mean,
        "mc:128 lost its edge: {mc_mean:.2} vs {greedy_mean:.2}"
    );
}
