//! Bot-vs-bot Hearts tournaments with score and win-rate statistics.
//!
//! ```console
//! cargo run --release --example arena -- --rounds 1000 greedy greedy mc:32 mc:32
//! cargo run --release --example arena -- --games 100 mc:16 mc:32 mc:64 mc:128 --seed 7
//! ```
//!
//! Every bot rotates through every compass seat.  `--rounds` plays
//! independent single rounds and reports mean penalty points; `--games`
//! plays whole games to the target score and reports game wins with Wilson
//! intervals.  Moon shots are tallied in either mode.

use anyhow::{Context as _, Result, bail};
use hearts_engine::hearts::{FinalScore, Game, PassDirection, RoundResult, Rules, Seat};
use hearts_engine::{HeuristicBot, MonteCarloBot, Strategy, Table};
use rand::rngs::StdRng;
use rand::{RngExt as _, SeedableRng};
use std::time::Instant;

struct Config {
    count: u32,
    games: bool,
    bots: [String; 4],
    seed: Option<u64>,
}

fn usage() {
    println!(
        "Usage: arena [--rounds N | --games N] [--seed N] [BOT BOT BOT BOT]\n\
         BOT is greedy or mc[:samples]; defaults are four greedy bots."
    );
}

fn parse_args() -> Result<Option<Config>> {
    let mut config = Config {
        count: 200,
        games: false,
        bots: std::array::from_fn(|_| "greedy".into()),
        seed: None,
    };
    let mut positional = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--rounds" => {
                config.count = value()?.parse()?;
                config.games = false;
            }
            "--games" => {
                config.count = value()?.parse()?;
                config.games = true;
            }
            "--seed" => config.seed = Some(value()?.parse()?),
            "--help" | "-h" => return Ok(None),
            other if other.starts_with('-') => {
                bail!("unknown flag {other:?} (--rounds/--games/--seed)")
            }
            spec => positional.push(spec.to_string()),
        }
    }
    if positional.len() > 4 {
        bail!("expected at most four bot specs, got {}", positional.len());
    }
    for (slot, spec) in config.bots.iter_mut().zip(positional) {
        *slot = spec;
    }
    Ok(Some(config))
}

fn make_bot(spec: &str, rng: &mut StdRng) -> Result<Box<dyn Strategy>> {
    let (kind, samples) = match spec.split_once(':') {
        Some((kind, samples)) => (kind, Some(samples.parse::<u32>()?)),
        None => (spec, None),
    };
    match kind {
        "greedy" if samples.is_none() => Ok(Box::new(HeuristicBot::new())),
        "mc" => Ok(Box::new(
            MonteCarloBot::new(StdRng::seed_from_u64(rng.random())).samples(samples.unwrap_or(32)),
        )),
        "greedy" => bail!("greedy does not take a sample count"),
        other => bail!("unknown bot {other:?} (greedy | mc[:samples])"),
    }
}

#[derive(Default)]
struct Tally {
    points: [u64; 4],
    game_wins: [u32; 4],
    moons: [u32; 4],
}

impl Tally {
    /// Record a round result; `bot_of_seat` maps each seat to a bot index.
    fn record_round(&mut self, result: RoundResult, rules: &Rules, bot_of_seat: [usize; 4]) {
        let scores = result.scores(rules);
        for seat in Seat::ALL {
            let bot = bot_of_seat[seat as usize];
            self.points[bot] += u64::from(scores[seat as usize]);
        }
        if let Some(shooter) = result.shooter() {
            self.moons[bot_of_seat[shooter as usize]] += 1;
        }
    }

    /// Record every (possibly shared) winner of a settled game.
    fn record_game(&mut self, score: FinalScore, bot_of_seat: [usize; 4]) {
        for winner in score.winners() {
            self.game_wins[bot_of_seat[winner as usize]] += 1;
        }
    }
}

/// The 95% Wilson score interval for `wins` out of `n` trials.
fn wilson(wins: u32, n: u32) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    let (w, n) = (f64::from(wins), f64::from(n));
    let z = 1.96;
    let p = w / n;
    let denom = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denom;
    let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denom;
    (center - half, center + half)
}

/// Borrow the four persistent bots in the seat order for this rotation.
fn rotated_bots(bots: &mut [Box<dyn Strategy>; 4], rotation: usize) -> [&mut dyn Strategy; 4] {
    let [zero, one, two, three] = bots;
    match rotation {
        0 => [&mut **zero, &mut **one, &mut **two, &mut **three],
        1 => [&mut **one, &mut **two, &mut **three, &mut **zero],
        2 => [&mut **two, &mut **three, &mut **zero, &mut **one],
        3 => [&mut **three, &mut **zero, &mut **one, &mut **two],
        _ => unreachable!("rotation is modulo four"),
    }
}

/// Play a whole game while retaining its round results for the moon tally.
fn play_counted_game(
    rules: Rules,
    bots: &mut [Box<dyn Strategy>; 4],
    rotation: usize,
    bot_of_seat: [usize; 4],
    rng: &mut StdRng,
    tally: &mut Tally,
) -> Result<FinalScore> {
    let mut game = Game::new(rules);
    while !game.is_over() {
        let mut table = Table::new(game.deal(rng)).scores(game.scores());
        let result = table.play(rotated_bots(bots, rotation))?;
        tally.record_round(result, &rules, bot_of_seat);
        game.record(result)?;
    }
    Ok(game.final_score().expect("a game that is over settles"))
}

fn main() -> Result<()> {
    let Some(config) = parse_args()? else {
        usage();
        return Ok(());
    };
    let mut rng = match config.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_rng(&mut rand::rng()),
    };
    let mut bots = [
        make_bot(&config.bots[0], &mut rng)?,
        make_bot(&config.bots[1], &mut rng)?,
        make_bot(&config.bots[2], &mut rng)?,
        make_bot(&config.bots[3], &mut rng)?,
    ];
    let names: [String; 4] = std::array::from_fn(|i| format!("b{}={}", i + 1, config.bots[i]));
    let rules = Rules::new();
    let mut tally = Tally::default();
    let start = Instant::now();

    for index in 0..config.count {
        // Shift every bot one compass point per trial, cancelling any fixed
        // seat advantage over each block of four.
        let rotation = index as usize % 4;
        let bot_of_seat = std::array::from_fn(|seat| (seat + rotation) % 4);

        if config.games {
            let score = play_counted_game(
                rules,
                &mut bots,
                rotation,
                bot_of_seat,
                &mut rng,
                &mut tally,
            )?;
            tally.record_game(score, bot_of_seat);
        } else {
            let direction = PassDirection::from_deal_index(index);
            let mut table = Table::deal(rules, direction, &mut rng);
            let result = table.play(rotated_bots(&mut bots, rotation))?;
            tally.record_round(result, &rules, bot_of_seat);
        }
    }

    let elapsed = start.elapsed();
    let unit = if config.games { "game" } else { "round" };
    println!(
        "{} {unit}s in {:.2?} ({:.1} {unit}s/s)",
        config.count,
        elapsed,
        f64::from(config.count) / elapsed.as_secs_f64(),
    );

    if config.games {
        for (bot, name) in names.iter().enumerate() {
            let wins = tally.game_wins[bot];
            let (lo, hi) = wilson(wins, config.count);
            println!(
                "{name}: {wins} game wins / {} ({:.1}%, 95% CI {:.1}%–{:.1}%)",
                config.count,
                100.0 * f64::from(wins) / f64::from(config.count.max(1)),
                100.0 * lo,
                100.0 * hi,
            );
        }
    } else {
        for (bot, name) in names.iter().enumerate() {
            println!(
                "{name}: {:.2} points/round",
                tally.points[bot] as f64 / f64::from(config.count.max(1)),
            );
        }
    }
    let total_moons: u32 = tally.moons.iter().sum();
    println!(
        "moons: {total_moons} total (b1 {}, b2 {}, b3 {}, b4 {})",
        tally.moons[0], tally.moons[1], tally.moons[2], tally.moons[3],
    );
    Ok(())
}
