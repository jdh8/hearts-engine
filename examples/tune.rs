//! Tune `HeuristicBot`'s moon defense and passing policy by self-play.
//!
//! ```console
//! cargo run --release --example tune -- --games 2000 --seed 1 \
//!   --moon-defense 4,8,12,255 --void-weight 0,1,2,4
//! ```
//!
//! Each arm is a candidate `HeuristicConfig` — a `(moon_defense,
//! void_weight)` pair — seated against three copies of the shipped
//! default.  Every arm replays the *same* per-game seeds (common random
//! numbers), and the candidate rotates through all four seats.  The table
//! reports the paired game-win-rate delta from an all-default baseline.
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
}

/// Build a fresh candidate `HeuristicBot` from a swept knob pair.
fn candidate(moon_defense: u8, void_weight: u8) -> HeuristicBot {
    // `HeuristicConfig` is non-exhaustive, so start from Default and adjust.
    let mut config = HeuristicConfig::default();
    config.moon_defense = moon_defense;
    config.void_weight = void_weight;
    HeuristicBot::with_config(config)
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
        "Usage: tune [--games N] [--seed N] \\\n+         [--moon-defense N,N,...] [--void-weight N,N,...]"
    );
}

fn parse_args() -> Result<Option<Config>> {
    let mut config = Config {
        games: 1000,
        seed: 1,
        moon_defense: vec![4, 8, 12, u8::MAX],
        void_weight: vec![0, 1, 2, 4],
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--games" => config.games = value()?.parse()?,
            "--seed" => config.seed = value()?.parse()?,
            "--moon-defense" => config.moon_defense = parse_list(&value()?)?,
            "--void-weight" => config.void_weight = parse_list(&value()?)?,
            "--help" | "-h" => return Ok(None),
            other => bail!(
                "unknown flag {other:?} \
                 (--games/--seed/--moon-defense/--void-weight)"
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
fn play_one(
    moon_defense: u8,
    void_weight: u8,
    rules: Rules,
    seed: u64,
    index: u32,
) -> Result<bool> {
    // SplitMix64's stride decorrelates adjacent per-game seeds.
    let mixed = u64::from(index).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut rng = StdRng::seed_from_u64(seed ^ mixed);
    let candidate_seat = Seat::ALL[index as usize % 4];
    let mut bots = [HeuristicBot::new(); 4];
    bots[candidate_seat as usize] = candidate(moon_defense, void_weight);
    let [north, east, south, west] = &mut bots;
    let seats: [&mut dyn Strategy; 4] = [north, east, south, west];
    let mut game = Game::new(rules);
    let score = play_game(&mut game, seats, &mut rng)?;
    Ok(score.winners().any(|seat| seat == candidate_seat))
}

/// The candidate's game wins over the common indexed game seeds.
fn evaluate(moon_defense: u8, void_weight: u8, config: &Config) -> Result<Vec<bool>> {
    let rules = Rules::new();
    (0..config.games)
        .map(|index| play_one(moon_defense, void_weight, rules, config.seed, index))
        .collect()
}

fn main() -> Result<()> {
    let Some(config) = parse_args()? else {
        usage();
        return Ok(());
    };
    let start = Instant::now();
    let default = HeuristicConfig::default();
    let baseline = evaluate(default.moon_defense, default.void_weight, &config)?;
    let baseline_wins = baseline.iter().filter(|&&won| won).count() as u32;

    let mut deltas = vec![vec![0.0; config.void_weight.len()]; config.moon_defense.len()];
    for (row, &moon_defense) in config.moon_defense.iter().enumerate() {
        for (column, &void_weight) in config.void_weight.iter().enumerate() {
            let outcomes = evaluate(moon_defense, void_weight, &config)?;
            let paired: i32 = outcomes
                .iter()
                .zip(&baseline)
                .map(|(&arm, &base)| i32::from(arm) - i32::from(base))
                .sum();
            deltas[row][column] = 100.0 * f64::from(paired) / f64::from(config.games.max(1));
            eprintln!(
                "  arm {}/{}: moon={moon_defense} void={void_weight} -> {:+.2} pp",
                row * config.void_weight.len() + column + 1,
                config.moon_defense.len() * config.void_weight.len(),
                deltas[row][column],
            );
        }
    }

    let elapsed = start.elapsed();
    let arms = config.moon_defense.len() * config.void_weight.len();
    let total = u64::from(config.games) * (arms as u64 + 1);
    println!(
        "{total} games in {:.1?} ({:.0} games/s); default {baseline_wins}/{} ({:.1}%)",
        elapsed,
        total as f64 / elapsed.as_secs_f64(),
        config.games,
        100.0 * f64::from(baseline_wins) / f64::from(config.games.max(1)),
    );
    println!("win-rate delta vs default (percentage points; higher is better)");
    print!("moon \\ void");
    for void_weight in &config.void_weight {
        print!(" | {void_weight:>7}");
    }
    println!();
    for (row, moon_defense) in config.moon_defense.iter().enumerate() {
        print!("{moon_defense:>11}");
        for delta in &deltas[row] {
            print!(" | {delta:>+7.2}");
        }
        println!();
    }
    Ok(())
}
