//! Bot-vs-bot Hearts tournaments over duplicate deals.
//!
//! ```console
//! cargo run --release --example arena -- --blocks 2000 mc:128 greedy greedy greedy
//! cargo run --release --example arena -- --games 200 --ab mc:1024 mc:128 greedy greedy greedy
//! cargo run --release --example arena -- --blocks 500 --csv mc:64 greedy greedy greedy > a.csv
//! ```
//!
//! The unit of measurement is a **block**: one deal — or one game seed —
//! played four times with the lineup rotated, so every bot plays all four
//! hands of it.  That is duplicate bridge in the only form a four-player
//! free-for-all admits, and it is what turns a 26-point deal swing into a
//! contrast small enough to see a skill edge through.
//!
//! Deal seeds are a pure function of `(--seed, block)` and every bot is
//! built inside its trial, seeded from the deal rather than from the deal
//! stream.  So two runs at the same seed are paired card for card — even
//! across rebuilds — blocks are independent enough to carry a standard
//! error, and they can be played in parallel.

use anyhow::{Context as _, Result, bail};
use hearts_engine::hearts::{Game, PassDirection, RoundResult, Rules};
use hearts_engine::{HeuristicBot, HeuristicConfig, MonteCarloBot, Strategy, Table};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rayon::prelude::*;
use std::time::Instant;

/// SplitMix64's stride, decorrelating adjacent seeds (as in `examples/tune.rs`).
const STRIDE: u64 = 0x9E37_79B9_7F4A_7C15;

/// The reported columns, each with the value a block's four entries must
/// sum to — `None` for the one column that is not constant-sum.
///
/// The three rank columns are the payoffs a four-player game leaves open
/// where a two-player one does not: with two players "seek the win" and
/// "avoid the loss" are the same sentence, with four they are different
/// objectives with different optimal play.  They cost nothing to report
/// once the cards are played, and their disagreement is the interesting
/// part — a change that lifts `points` while dropping `win` is reducing
/// variance in a way the game does not reward.
const COLS: [(&str, Option<f64>); 5] = [
    ("points", Some(0.0)),   // mean(others) − own, the search signal
    ("win", Some(1.0)),      // 1-0-0-0, the shipped rule; 1/k on a shared win
    ("not-last", Some(3.0)), // 1-1-1-0, loss aversion
    ("rank", Some(6.0)),     // 3-2-1-0 matchpoints, tied ranks averaged
    ("moons", None),         // moons shot per round
];
/// Index of the `points` column: the paired zero-sum search signal.
const POINTS: usize = 0;

struct Config {
    blocks: u32,
    games: bool,
    bots: [String; 4],
    /// Spec to substitute into slot 1, run over the same blocks.
    ab: Option<String>,
    csv: bool,
    seed: u64,
}

fn usage() {
    println!(
        "Usage: arena [--blocks N | --games N] [--seed N] [--ab SPEC] [--csv] [BOT BOT BOT BOT]\n\
         BOT is greedy[:V,H,G], newbie or mc[:samples[,knob=value...]].\n\
         greedy:V,H,G sets the pass knobs void_weight,heart_weight,spade_guards.\n\
         mc knobs void/heart/guards set the pass model the search ranks,\n\
         samples its rollout opponents, and reads the incoming pass with.\n\
         N counts blocks, each one deal (or game seed) played four times rotated.\n\
         --ab reruns the same blocks with SPEC in slot 1 and reports the paired delta.\n\
         --csv writes one row per block per bot instead of the summary."
    );
}

fn parse_args() -> Result<Option<Config>> {
    let mut config = Config {
        blocks: 200,
        games: false,
        bots: std::array::from_fn(|_| "greedy".into()),
        ab: None,
        csv: false,
        seed: 0,
    };
    let mut positional = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--blocks" => {
                config.blocks = value()?.parse()?;
                config.games = false;
            }
            "--games" => {
                config.blocks = value()?.parse()?;
                config.games = true;
            }
            "--seed" => config.seed = value()?.parse()?,
            "--ab" => config.ab = Some(value()?),
            "--csv" => config.csv = true,
            "--help" | "-h" => return Ok(None),
            other if other.starts_with('-') => {
                bail!("unknown flag {other:?} (--blocks/--games/--seed/--ab/--csv)")
            }
            spec => positional.push(spec.to_string()),
        }
    }
    if positional.len() > 4 {
        bail!("expected at most four bot specs, got {}", positional.len());
    }
    if config.blocks == 0 {
        bail!("--blocks/--games must be positive");
    }
    for (slot, spec) in config.bots.iter_mut().zip(positional) {
        *slot = spec;
    }
    Ok(Some(config))
}

/// Build one bot, seeded so its search noise is a function of the deal and
/// its slot alone — never of the deal stream or of who else is at the table.
fn make_bot(spec: &str, deal_seed: u64, slot: usize) -> Result<Box<dyn Strategy>> {
    let (kind, suffix) = match spec.split_once(':') {
        Some((kind, suffix)) => (kind, Some(suffix)),
        None => (spec, None),
    };
    match kind {
        "greedy" => {
            let Some(suffix) = suffix else {
                return Ok(Box::new(HeuristicBot::new()));
            };
            // greedy:V,H,G — the pass knobs; moon defense stays default.
            let knobs: Vec<u8> = suffix
                .split(',')
                .map(|knob| knob.trim().parse().map_err(anyhow::Error::from))
                .collect::<Result<_>>()?;
            let &[void_weight, heart_weight, spade_guards] = knobs.as_slice() else {
                bail!("greedy takes three knobs V,H,G (void,heart,guards), got {spec:?}");
            };
            // `HeuristicConfig` is non-exhaustive, so start from Default.
            let mut heuristic = HeuristicConfig::default();
            heuristic.void_weight = void_weight;
            heuristic.heart_weight = heart_weight;
            heuristic.spade_guards = spade_guards;
            Ok(Box::new(HeuristicBot::with_config(heuristic)))
        }
        "newbie" if suffix.is_none() => {
            // The web UI's Easy tier: no moon defense, no void bonus.
            // `HeuristicConfig` is non-exhaustive, so start from Default.
            let mut heuristic = HeuristicConfig::default();
            heuristic.moon_defense = u8::MAX;
            heuristic.void_weight = 0;
            Ok(Box::new(HeuristicBot::with_config(heuristic)))
        }
        "mc" => {
            // mc:SAMPLES[,knob=value...] — the bare count keeps its old
            // meaning; named knobs configure the pass model the search
            // ranks, samples opponents and reads the incoming pass with.
            // Named rather than positional because a sweep wants to move
            // one knob without restating the rest, and because the set is
            // open — the sample count is a `u32` beside `u8` pass weights.
            let mut samples = 32;
            // `HeuristicConfig` is non-exhaustive, so start from Default.
            let mut pass = HeuristicConfig::default();
            for (index, knob) in suffix
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|knob| !knob.is_empty())
                .enumerate()
            {
                match knob.split_once('=') {
                    // Only the leading component may be bare.  A second one
                    // would silently reset the width — `mc:128,64` quietly
                    // running 64 worlds invalidates the measurement it was
                    // typed for, so it is an error, not a last-one-wins.
                    None if index == 0 => samples = knob.parse()?,
                    None => bail!(
                        "mc takes one sample count, then named knobs; got a bare {knob:?} in {spec:?}"
                    ),
                    Some(("void", value)) => pass.void_weight = value.parse()?,
                    Some(("heart", value)) => pass.heart_weight = value.parse()?,
                    Some(("guards", value)) => pass.spade_guards = value.parse()?,
                    Some((key, _)) => {
                        bail!("unknown mc knob {key:?} in {spec:?} (void/heart/guards)")
                    }
                }
            }
            let seed = deal_seed ^ (slot as u64 + 1).wrapping_mul(STRIDE);
            Ok(Box::new(
                MonteCarloBot::new(StdRng::seed_from_u64(seed))
                    .samples(samples)
                    .pass_model(pass),
            ))
        }
        "newbie" => bail!("newbie does not take a suffix"),
        other => {
            bail!("unknown bot {other:?} (greedy[:V,H,G] | newbie | mc[:samples[,knob=value]])")
        }
    }
}

/// The four constant-sum payoffs of one score vector, in [`COLS`] order:
/// the zero-sum margin `mean(others) − own`, win-seeking `1-0-0-0`,
/// loss-averse `1-1-1-0`, and matchpoints `3-2-1-0`.
///
/// Lowest wins, matching [`FinalScore::winners`](hearts::FinalScore).  Ties
/// share their band — `1/k` for a k-way win in `win`, tied ranks averaged
/// in `rank` (the crate's own equity payoff) — so every column keeps its
/// constant sum exactly.
fn payoffs(scores: [u16; 4]) -> [[f64; 4]; 4] {
    let sum: u16 = scores.iter().sum();
    let low = *scores.iter().min().expect("four scores");
    let high = *scores.iter().max().expect("four scores");
    let share = |value: u16| scores.iter().filter(|&&s| s == value).count() as f64;
    [
        scores.map(|own| f64::from(sum - own) / 3.0 - f64::from(own)),
        scores.map(|own| if own == low { 1.0 / share(low) } else { 0.0 }),
        scores.map(|own| {
            if own == high {
                1.0 - 1.0 / share(high)
            } else {
                1.0
            }
        }),
        scores.map(|own| {
            let ahead = scores.iter().filter(|&&other| other < own).count() as f64;
            3.0 - ahead - (share(own) - 1.0) / 2.0
        }),
    ]
}

/// Seat the lineup for one rotation: seat `s` is played by bot `(s + r) % 4`.
fn seated(bots: &mut [Box<dyn Strategy>; 4], rotation: usize) -> [&mut dyn Strategy; 4] {
    let [zero, one, two, three] = bots;
    let (zero, one, two, three) = (&mut **zero, &mut **one, &mut **two, &mut **three);
    match rotation {
        0 => [zero, one, two, three],
        1 => [one, two, three, zero],
        2 => [two, three, zero, one],
        _ => [three, zero, one, two],
    }
}

/// Undo [`seated`]: reindex a per-seat vector by bot slot.
fn by_bot(per_seat: [f64; 4], rotation: usize) -> [f64; 4] {
    std::array::from_fn(|bot| per_seat[(bot + 4 - rotation) % 4])
}

/// One rotation's running tallies, indexed by seat.
#[derive(Default)]
struct Rotation {
    margin: [f64; 4],
    moons: [f64; 4],
    rounds: u32,
}

impl Rotation {
    fn fold(&mut self, result: RoundResult, rules: &Rules) {
        let round = payoffs(result.scores(rules));
        for (margin, seat) in self.margin.iter_mut().zip(round[POINTS]) {
            *margin += seat;
        }
        if let Some(shooter) = result.shooter() {
            self.moons[shooter as usize] += 1.0;
        }
        self.rounds += 1;
    }
}

/// One block's per-bot column values, in [`COLS`] order.
#[derive(Clone, Copy, Default)]
struct Block {
    cols: [[f64; 4]; COLS.len()],
    rounds: u32,
}

/// Play one block: the same deal — or the same game seed — four times with
/// the lineup rotated, so every bot plays every seat.
///
/// The pass direction is fixed for the whole block and varies with the
/// block index, so it cannot share a period with the rotation the way
/// `index % 4` once did.  In games mode the direction rotates within each
/// game as the rules dictate, and the block index only picks the deals.
fn play_block(
    specs: &[String; 4],
    rules: Rules,
    seed: u64,
    index: u32,
    games: bool,
) -> Result<Block> {
    let deal_seed = seed ^ u64::from(index).wrapping_mul(STRIDE);
    let direction = PassDirection::from_deal_index(index);
    let mut block = Block::default();

    for rotation in 0..4 {
        // Re-seeding from the same deal seed hands all four rotations
        // identical cards; only the seating changes.
        let mut rng = StdRng::seed_from_u64(deal_seed);
        let mut bots = [
            make_bot(&specs[0], deal_seed, 0)?,
            make_bot(&specs[1], deal_seed, 1)?,
            make_bot(&specs[2], deal_seed, 2)?,
            make_bot(&specs[3], deal_seed, 3)?,
        ];
        let mut tally = Rotation::default();

        // The score vector the rank columns read: final totals in games
        // mode, the single deal's points otherwise — where they are the
        // bridge matchpoint reading of that deal rather than of a game.
        let totals = if games {
            let mut game = Game::new(rules);
            while !game.is_over() {
                let mut table = Table::new(game.deal(&mut rng)).scores(game.scores());
                let result = table.play(seated(&mut bots, rotation))?;
                tally.fold(result, &rules);
                game.record(result)?;
            }
            game.final_score()
                .expect("a game that is over settles")
                .totals
        } else {
            let mut table = Table::deal(rules, direction, &mut rng);
            let result = table.play(seated(&mut bots, rotation))?;
            tally.fold(result, &rules);
            result.scores(&rules)
        };

        let per_round = f64::from(tally.rounds);
        let rank = payoffs(totals);
        let per_seat = [
            tally.margin.map(|value| value / per_round),
            rank[1],
            rank[2],
            rank[3],
            tally.moons.map(|value| value / per_round),
        ];
        for (accumulator, values) in block.cols.iter_mut().zip(per_seat) {
            for (total, value) in accumulator.iter_mut().zip(by_bot(values, rotation)) {
                *total += value / 4.0;
            }
        }
        block.rounds += tally.rounds;
    }

    // Every reported column but `moons` is constant-sum by construction, so
    // a wrong tie convention or a rotation that lost a bot shows up here
    // rather than as a plausible number three hours later.
    for (col, (name, expected)) in COLS.into_iter().enumerate() {
        if let Some(expected) = expected {
            let sum: f64 = block.cols[col].iter().sum();
            assert!(
                (sum - expected).abs() < 1e-9,
                "block {index} column {name} sums to {sum}, not {expected}"
            );
        }
    }
    Ok(block)
}

/// Play every block of one lineup.  Blocks are self-contained, so this is
/// order-preserving parallelism: the output does not depend on the schedule.
fn run(specs: &[String; 4], rules: Rules, config: &Config) -> Result<Vec<Block>> {
    (0..config.blocks)
        .into_par_iter()
        .map(|index| play_block(specs, rules, config.seed, index, config.games))
        .collect()
}

/// Mean and standard error of a sample; the block is the independent unit.
fn mean_se(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    (mean, (var / n).sqrt())
}

fn column(blocks: &[Block], col: usize, bot: usize) -> Vec<f64> {
    blocks.iter().map(|block| block.cols[col][bot]).collect()
}

fn report(blocks: &[Block], names: &[String; 4]) {
    let header: String = COLS.iter().map(|(name, _)| format!("{name:>16}")).collect();
    println!("{:<14}{header}", "bot");
    for (bot, name) in names.iter().enumerate() {
        let cells: String = (0..COLS.len())
            .map(|col| {
                let (mean, se) = mean_se(&column(blocks, col, bot));
                format!("{:>16}", format!("{mean:.3}±{se:.3}"))
            })
            .collect();
        println!("{name:<14}{cells}");
    }
}

/// The paired per-block delta of slot 1, challenger minus incumbent.  This
/// is the gate: a change is worth keeping when it clears two standard
/// errors here, on the same cards, in the same seats.
fn report_ab(base: &[Block], challenger: &[Block], spec: &str, incumbent: &str) {
    println!("\nΔ slot 1: {spec} − {incumbent}, paired over the same blocks");
    for (col, (name, _)) in COLS.into_iter().enumerate() {
        let deltas: Vec<f64> = base
            .iter()
            .zip(challenger)
            .map(|(a, b)| b.cols[col][0] - a.cols[col][0])
            .collect();
        let (mean, se) = mean_se(&deltas);
        println!("  {name:<10}{mean:+.4} ± {se:.4}  ({:+.1} SE)", mean / se);
    }
}

fn csv(blocks: &[Block], names: &[String; 4], lineup: char) {
    for (index, block) in blocks.iter().enumerate() {
        for (bot, name) in names.iter().enumerate() {
            let cells: String = (0..COLS.len())
                .map(|col| format!(",{}", block.cols[col][bot]))
                .collect();
            println!("{lineup},{index},{bot},{name}{cells}");
        }
    }
}

fn main() -> Result<()> {
    let Some(config) = parse_args()? else {
        usage();
        return Ok(());
    };
    let rules = Rules::new();
    let names: [String; 4] = std::array::from_fn(|i| format!("b{}={}", i + 1, config.bots[i]));
    let start = Instant::now();

    let base = run(&config.bots, rules, &config)?;
    let challenger = match &config.ab {
        Some(spec) => {
            let mut specs = config.bots.clone();
            specs[0] = spec.clone();
            Some(run(&specs, rules, &config)?)
        }
        None => None,
    };
    let elapsed = start.elapsed();

    if config.csv {
        let header: String = COLS.iter().map(|(name, _)| format!(",{name}")).collect();
        println!("lineup,block,bot,spec{header}");
        csv(&base, &names, 'a');
        if let Some(challenger) = &challenger {
            let mut names = names.clone();
            names[0] = format!("b1={}", config.ab.as_deref().unwrap_or_default());
            csv(challenger, &names, 'b');
        }
        return Ok(());
    }

    let rounds: u32 = base.iter().map(|block| block.rounds).sum();
    let played = rounds + challenger.iter().flatten().map(|b| b.rounds).sum::<u32>();
    println!(
        "{} blocks ({rounds} rounds{}), seed {}, in {:.2?} — {:.0} rounds/s",
        config.blocks,
        if config.ab.is_some() { " each" } else { "" },
        config.seed,
        elapsed,
        f64::from(played) / elapsed.as_secs_f64(),
    );
    let (_, se) = mean_se(&column(&base, POINTS, 0));
    println!("resolvable at 2 SE: ±{:.3} points/deal for b1\n", 2.0 * se);
    report(&base, &names);
    if let Some(challenger) = &challenger {
        let spec = config.ab.as_deref().unwrap_or_default();
        report_ab(&base, challenger, spec, &config.bots[0]);
    }
    Ok(())
}
