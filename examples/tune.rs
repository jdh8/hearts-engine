//! Tune `HeuristicBot`'s knobs by self-play.
//!
//! ```console
//! cargo run --release --example tune -- --games 2000 --seed 1 \
//!   --moon-defense 8 --void-weight 0,1,2 --heart-weight 1,2,3 --spade-guards 2,3,4
//! ```
//!
//! Each arm is a candidate `HeuristicConfig` from the Cartesian product of
//! the knob lists, seated against three copies of the shipped default.
//! Every arm replays the *same* per-game seeds (common random numbers), and
//! the candidate rotates through all four seats.  The report is one row per
//! arm: the paired game-win-rate delta from an all-default baseline, ± its
//! paired standard error.  An arm equal to the default plays literally the
//! same games and must print `+0.00 ± 0.00` exactly — the harness's own
//! self-check.
//!
//! Comparing many arms on one seed and keeping the maximum overstates the
//! winner.  Search on one seed, then re-confirm the single best arm on
//! another before trusting it.

use anyhow::{Context as _, Result, bail};
use hearts_engine::hearts::{Game, Rules, Seat};
use hearts_engine::{HeuristicBot, HeuristicConfig, Strategy, play_game};
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use std::time::Instant;

struct Config {
    games: u32,
    seed: u64,
    moon_defense: Vec<u8>,
    void_weight: Vec<u8>,
    heart_weight: Vec<u8>,
    two_of_clubs_bonus: Vec<u8>,
    spade_guards: Vec<u8>,
}

/// The Cartesian product of the knob lists, one candidate config per arm.
fn arms(config: &Config) -> Vec<HeuristicConfig> {
    let mut arms = Vec::new();
    for &moon_defense in &config.moon_defense {
        for &void_weight in &config.void_weight {
            for &heart_weight in &config.heart_weight {
                for &two_of_clubs_bonus in &config.two_of_clubs_bonus {
                    for &spade_guards in &config.spade_guards {
                        // `HeuristicConfig` is non-exhaustive: start from Default.
                        let mut arm = HeuristicConfig::default();
                        arm.moon_defense = moon_defense;
                        arm.void_weight = void_weight;
                        arm.heart_weight = heart_weight;
                        arm.two_of_clubs_bonus = two_of_clubs_bonus;
                        arm.spade_guards = spade_guards;
                        arms.push(arm);
                    }
                }
            }
        }
    }
    arms
}

/// Parse a comma-separated list of small integers, e.g. `4,8,12`.
fn parse_list(text: &str) -> Result<Vec<u8>> {
    let values: Vec<u8> = text
        .split(',')
        .map(|item| item.trim().parse().map_err(anyhow::Error::from))
        .collect::<Result<_>>()?;
    if values.is_empty() {
        bail!("a tuning grid may not be empty");
    }
    Ok(values)
}

fn usage() {
    println!(
        "Usage: tune [--games N] [--seed N] [--moon-defense N,N,...] \\\n\
         [--void-weight N,N,...] [--heart-weight N,N,...] \\\n\
         [--two-of-clubs-bonus N,N,...] \\\n\
         [--spade-guards N,N,...]"
    );
}

fn parse_args() -> Result<Option<Config>> {
    let default = HeuristicConfig::default();
    let mut config = Config {
        games: 1000,
        seed: 1,
        moon_defense: vec![4, 8, 12, u8::MAX],
        void_weight: vec![0, 1, 2, 4],
        heart_weight: vec![default.heart_weight],
        two_of_clubs_bonus: vec![default.two_of_clubs_bonus],
        spade_guards: vec![default.spade_guards],
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--games" => config.games = value()?.parse()?,
            "--seed" => config.seed = value()?.parse()?,
            "--moon-defense" => config.moon_defense = parse_list(&value()?)?,
            "--void-weight" => config.void_weight = parse_list(&value()?)?,
            "--heart-weight" => config.heart_weight = parse_list(&value()?)?,
            "--two-of-clubs-bonus" => config.two_of_clubs_bonus = parse_list(&value()?)?,
            "--spade-guards" => config.spade_guards = parse_list(&value()?)?,
            "--help" | "-h" => return Ok(None),
            other => bail!(
                "unknown flag {other:?} (--games/--seed/--moon-defense\
                 /--void-weight/--heart-weight\
                 /--two-of-clubs-bonus/--spade-guards)"
            ),
        }
    }
    Ok(Some(config))
}

/// Play game `index` of an arm and report whether the candidate won.
///
/// The deal is seeded from `index` alone, not from the arm's knobs, so every
/// arm plays game `index` on the same random stream.  This is the common
/// random number pairing that keeps arm-to-baseline deltas quiet.
fn play_one(arm: HeuristicConfig, rules: Rules, seed: u64, index: u32) -> Result<bool> {
    // SplitMix64's stride decorrelates adjacent per-game seeds.
    let mixed = u64::from(index).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut rng = StdRng::seed_from_u64(seed ^ mixed);
    let candidate_seat = Seat::ALL[index as usize % 4];
    let mut bots = [HeuristicBot::new(); 4];
    bots[candidate_seat as usize] = HeuristicBot::with_config(arm);
    let [north, east, south, west] = &mut bots;
    let seats: [&mut dyn Strategy; 4] = [north, east, south, west];
    let mut game = Game::new(rules);
    let score = play_game(&mut game, seats, &mut rng)?;
    Ok(score.winners().any(|seat| seat == candidate_seat))
}

/// The candidate's game wins over the common indexed game seeds.
fn evaluate(arm: HeuristicConfig, config: &Config) -> Result<Vec<bool>> {
    let rules = Rules::new();
    (0..config.games)
        .map(|index| play_one(arm, rules, config.seed, index))
        .collect()
}

/// The paired win-rate delta from the baseline and its standard error, both
/// in percentage points.
fn paired_delta(outcomes: &[bool], baseline: &[bool]) -> (f64, f64) {
    let diffs: Vec<f64> = outcomes
        .iter()
        .zip(baseline)
        .map(|(&arm, &base)| f64::from(i8::from(arm) - i8::from(base)))
        .collect();
    let n = diffs.len().max(1) as f64;
    let mean = diffs.iter().sum::<f64>() / n;
    let variance = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    (100.0 * mean, 100.0 * (variance / n).sqrt())
}

fn describe(arm: HeuristicConfig) -> String {
    format!(
        "moon={:>3} void={} heart={} two={:>2} guards={}",
        arm.moon_defense,
        arm.void_weight,
        arm.heart_weight,
        arm.two_of_clubs_bonus,
        arm.spade_guards,
    )
}

fn main() -> Result<()> {
    let Some(config) = parse_args()? else {
        usage();
        return Ok(());
    };
    let start = Instant::now();
    let baseline = evaluate(HeuristicConfig::default(), &config)?;
    let baseline_wins = baseline.iter().filter(|&&won| won).count() as u32;

    let arms = arms(&config);
    let mut results = Vec::with_capacity(arms.len());
    for (index, &arm) in arms.iter().enumerate() {
        let outcomes = evaluate(arm, &config)?;
        let (delta, error) = paired_delta(&outcomes, &baseline);
        eprintln!(
            "  arm {}/{}: {} -> {delta:+.2} ± {error:.2} pp",
            index + 1,
            arms.len(),
            describe(arm),
        );
        results.push((arm, delta, error));
    }

    let elapsed = start.elapsed();
    let total = u64::from(config.games) * (arms.len() as u64 + 1);
    println!(
        "{total} games in {:.1?} ({:.0} games/s); default {baseline_wins}/{} ({:.1}%)",
        elapsed,
        total as f64 / elapsed.as_secs_f64(),
        config.games,
        100.0 * f64::from(baseline_wins) / f64::from(config.games.max(1)),
    );
    println!("paired game-win-rate delta vs default (percentage points; higher is better)");
    for &(arm, delta, error) in &results {
        println!("{} | {delta:>+6.2} ± {error:.2}", describe(arm));
    }
    if let Some(&(arm, delta, error)) = results
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).expect("deltas are finite"))
    {
        println!(
            "best: --moon-defense {} --void-weight {} --heart-weight {} \
             --two-of-clubs-bonus {} --spade-guards {} \
             ({delta:+.2} ± {error:.2} pp)",
            arm.moon_defense,
            arm.void_weight,
            arm.heart_weight,
            arm.two_of_clubs_bonus,
            arm.spade_guards,
        );
    }
    Ok(())
}
