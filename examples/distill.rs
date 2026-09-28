//! P2 stage 1: distill `mc:N`'s play into a card scorer, offline.
//!
//! ```console
//! cargo run --release --example distill -- gen --blocks 10000 > s0.tsv
//! cargo run --release --example distill -- fit s0.tsv
//! python examples/distill.py s0.tsv 100 31   # the tree-ensemble rung
//! ```
//!
//! `gen` seats one `mc:N` among three greedy players over arena-style
//! blocks (one deal, the MC seat rotated through all four hands) and logs
//! every play decision the search actually weighs: its gated pick, as the
//! equivalence class `play_candidates` collapses it into, next to greedy's
//! pick.  Forced plays, fully collapsed legal sets, the moon-defense
//! overlay and moon plans are skipped — the rollout policy's ordinary
//! branches are what P2 replaces.  The label comes from `assess`, a
//! separate search at the same width as the one that plays the card.
//!
//! `fit` trains a conditional logit over legal cards on four blocks in five
//! and reports held-out agreement with the label against greedy's, the
//! stage-1 bar, next to an independent search's agreement for scale.
//! Every feature reads only what a rollout seat may: its own
//! hand, the legal set, the trick, `played`, and points taken.  One feature
//! is greedy's own pick, so the scorer is greedy plus learned corrections.

use anyhow::{Context as _, Result, bail};
use hearts_engine::hearts::{Card, Hand, PassDirection, Rank, Rules, Seat, Suit, Trick};
use hearts_engine::{HeuristicBot, HeuristicConfig, MonteCarloBot, Strategy, Table, View};
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use rayon::prelude::*;
use std::fmt::Write as _;

/// SplitMix64's stride, as in `examples/arena.rs`.
const STRIDE: u64 = 0x9E37_79B9_7F4A_7C15;

/// The pure greedy play policy: the heuristic with its moon overlay off.
fn greedy() -> HeuristicBot {
    let mut config = HeuristicConfig::default();
    config.moon_defense = u8::MAX;
    HeuristicBot::with_config(config)
}

fn bit(card: Card) -> u32 {
    Hand::from_card(card).to_bits().trailing_zeros()
}

fn from_bit(index: u32) -> Card {
    Hand::from_bits_truncate(1 << index)
        .into_iter()
        .next()
        .expect("one bit is one card")
}

/// The legal cards `play_candidates` would collapse into `card`'s class:
/// same suit and penalty value, every rank between them already seen.
fn class(card: Card, legal: Hand, hand: Hand, trick: Trick, played: Hand) -> Hand {
    let seen = hand | (played - trick.cards());
    legal
        .into_iter()
        .filter(|other| {
            let (lo, hi) = (card.rank.min(other.rank), card.rank.max(other.rank));
            other.suit == card.suit
                && other.points() == card.points()
                && (lo.get() + 1..hi.get()).all(|gap| seen[card.suit].contains(Rank::new(gap)))
        })
        .collect()
}

/// The search's gated pick, or `None` for a tie-everything read or a
/// moon plan — `shoot_play`'s business, not the scorer's.
fn gated_pick(mc: &mut MonteCarloBot<StdRng>, view: &View<'_>) -> Option<Card> {
    let assessed = mc.assess(view);
    let picks: Vec<_> = assessed.iter().filter(|a| a.recommended).collect();
    let [pick] = picks[..] else { return None };
    let card = pick.action.strip_prefix("play ")?;
    Some(card.parse().expect("assess labels a card"))
}

/// Wraps the measured `mc:N` seat and logs its weighed play decisions.
struct Logger {
    mc: MonteCarloBot<StdRng>,
    /// An independent search of the same width.  Its agreement with the
    /// label is the scale of the label's noise — a reference, not a bound:
    /// a policy that always plays the modal pick can beat it.
    twin: MonteCarloBot<StdRng>,
    block: u32,
    shooting: bool,
    lines: Vec<String>,
}

impl Logger {
    fn log(&mut self, view: &View<'_>) {
        let me = view.seat();
        let points = [me, me.left(), me.across(), me.right()].map(|seat| view.points_taken(seat));
        // The live moon-defense overlay pre-empts the search; skip its
        // whole trigger region, not just the tricks it fires on.
        let scorers: Vec<_> = Seat::ALL
            .into_iter()
            .filter(|&seat| view.points_taken(seat) > 0)
            .collect();
        if let [sole] = scorers[..]
            && sole != me
            && view.points_taken(sole) >= 8
        {
            return;
        }
        let Some(card) = gated_pick(&mut self.mc, view) else {
            return;
        };
        let trick = view.current_trick().expect("a play decision has a trick");
        let (hand, legal, played) = (view.hand(), view.legal_plays(), view.played());
        let label = class(card, legal, hand, trick, played);
        let twin = gated_pick(&mut self.twin, view).map_or(Hand::EMPTY, Hand::from_card);
        let greedy = greedy().play_card(view);
        let mut line = format!(
            "{}\t{:x}\t{:x}\t{:x}\t",
            self.block,
            hand.to_bits(),
            legal.to_bits(),
            played.to_bits(),
        );
        let order: Vec<_> = trick
            .plays()
            .map(|(_, card)| bit(card).to_string())
            .collect();
        let _ = write!(
            line,
            "{}\t{}\t{:x}\t{}\t{:x}",
            order.join(","),
            points.map(|p| p.to_string()).join(","),
            label.to_bits(),
            bit(greedy),
            twin.to_bits(),
        );
        self.lines.push(line);
    }
}

impl Strategy for Logger {
    fn pass_cards(&mut self, view: &View<'_>) -> [Card; 3] {
        self.mc.pass_cards(view)
    }

    fn play_card(&mut self, view: &View<'_>) -> Card {
        // Mirror the bot's own plan latch: the first trick clears it and
        // another seat scoring kills it.
        if view.tricks().is_empty()
            || Seat::ALL
                .into_iter()
                .any(|seat| seat != view.seat() && view.points_taken(seat) > 0)
        {
            self.shooting = false;
        }
        if !self.shooting && view.legal_plays().len() > 1 {
            self.log(view);
        }
        let attempts = self.mc.moon_attempts();
        let card = self.mc.play_card(view);
        self.shooting |= self.mc.moon_attempts() > attempts;
        card
    }
}

fn gen_block(seed: u64, samples: u32, index: u32) -> Vec<String> {
    let deal_seed = seed ^ u64::from(index).wrapping_mul(STRIDE);
    let direction = PassDirection::from_deal_index(index);
    let mut lines = Vec::new();
    for rotation in 0..4 {
        let mut rng = StdRng::seed_from_u64(deal_seed);
        let mut table = Table::deal(Rules::new(), direction, &mut rng);
        let bot = |salt: u64| {
            MonteCarloBot::new(StdRng::seed_from_u64(
                deal_seed ^ (rotation as u64 + salt).wrapping_mul(STRIDE),
            ))
            .samples(samples)
        };
        let mut logger = Logger {
            mc: bot(1),
            twin: bot(5),
            block: index,
            shooting: false,
            lines: Vec::new(),
        };
        let mut others = [HeuristicBot::new(); 3];
        let [a, b, c] = &mut others;
        let mut seats: [&mut dyn Strategy; 4] = [a, b, c, &mut logger];
        seats.rotate_left(3 - rotation);
        table.play(seats).expect("bots play legally");
        lines.append(&mut logger.lines);
    }
    lines
}

/// Features per legal card.  Every group is gated on the situation, since
/// a conditional logit only sees what varies across the legal cards.
const F: usize = 45;

struct State {
    hand: Hand,
    legal: Hand,
    played: Hand,
    trick: Trick,
    points: [u8; 4],
    greedy: Card,
}

fn features(s: &State, card: Card) -> [f64; F] {
    let mut f = [0.0; F];
    let suit = card.suit;
    let rank = f64::from(card.rank.get()) / 14.0;
    let out = !(s.hand | s.played)[suit];
    let higher = out.iter().filter(|&r| r > card.rank).count() as f64 / 12.0;
    let lower = out.iter().filter(|&r| r < card.rank).count() as f64 / 12.0;
    let queen_out = !(s.hand | s.played).contains(Card::QUEEN_OF_SPADES);
    let high_spade = suit == Suit::Spades && card.rank > Rank::Q;
    let len = s.hand[suit].len();
    let pts = f64::from(card.points()) / 13.0;
    let flag = |b: bool| f64::from(u8::from(b));
    let scorers = s.points.iter().filter(|&&p| p > 0).count();
    let sole_other = scorers == 1 && s.points[0] == 0;
    let sole_me = scorers == 1 && s.points[0] > 0;
    // Trick 1 carries no points under the shipped rules, and a suit's first
    // round rarely does: both are where high cards are shed for free.
    let first_trick = (s.played - s.trick.cards()).is_empty();
    let round = s.played[suit].len() as f64 / 13.0;

    f[0] = flag(card == s.greedy);
    match s.trick.suit() {
        None => {
            f[1] = rank;
            f[2] = len as f64 / 13.0;
            f[3] = higher;
            f[4] = lower;
            f[5] = flag(!out.is_empty() && higher == 0.0);
            f[6] = flag(suit == Suit::Spades && queen_out);
            f[7] = flag(card == Card::QUEEN_OF_SPADES);
            f[8] = flag(high_spade && queen_out);
            f[9] = flag(suit == Suit::Hearts);
            f[10] = flag(out.is_empty());
            f[11] = flag(len == 1);
            f[33] = flag(suit == Suit::Spades && queen_out) * rank;
            f[34] = round;
            f[35] = round * rank;
            f[36] = out.len() as f64 / 13.0;
        }
        Some(led) if !s.legal[led].is_empty() => {
            let winner = s
                .trick
                .winner()
                .and_then(|seat| s.trick.card_from(seat))
                .expect("a led trick has a winner");
            let wins = card.rank > winner.rank;
            let last = s.trick.len() == 3;
            f[12] = flag(wins);
            f[13] = flag(wins && s.trick.points() > 0);
            f[14] = flag(wins && last);
            f[15] = flag(wins) * rank;
            f[16] = flag(!wins) * rank;
            f[17] = flag(card == Card::QUEEN_OF_SPADES);
            f[18] = flag(wins && led == Suit::Spades && queen_out && !last);
            f[19] = flag(wins) * higher;
            f[20] = flag(wins) * (3 - s.trick.len()) as f64 / 3.0;
            f[21] = pts;
            f[22] = flag(wins && sole_other);
            f[23] = flag(wins) * f64::from(s.trick.points()) / 13.0;
            f[37] = flag(first_trick) * rank;
            f[38] = flag(first_trick && wins);
            f[39] = round * rank;
            f[40] = flag(wins) * round;
            f[41] = flag(wins && sole_me);
            f[42] = flag(wins && high_spade && queen_out);
        }
        Some(_) => {
            f[24] = pts;
            f[25] = flag(card == Card::QUEEN_OF_SPADES);
            f[26] = flag(high_spade && queen_out);
            f[27] = flag(suit == Suit::Hearts) * rank;
            f[28] = rank;
            f[29] = flag(len == 1);
            f[30] = higher;
            f[31] = lower;
            f[32] = pts * flag(sole_other);
            f[43] = pts * flag(sole_me);
            f[44] = flag(first_trick) * rank;
        }
    }
    f
}

struct Decision {
    block: u32,
    cards: Vec<[f64; F]>,
    label: Vec<bool>,
    greedy: usize,
    /// The independent search's pick, when it made an ordinary one.
    twin: Option<usize>,
}

fn parse(line: &str) -> Result<Decision> {
    let fields: Vec<_> = line.split('\t').collect();
    let [
        block,
        hand,
        legal,
        played,
        trick,
        points,
        label,
        greedy,
        twin,
    ] = fields[..]
    else {
        bail!("expected 9 fields: {line:?}");
    };
    let hex = |s: &str| u64::from_str_radix(s, 16).map(Hand::from_bits_truncate);
    let order: Vec<Card> = trick
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().map(from_bit))
        .collect::<Result<_, _>>()?;
    let mut t = Trick::new(Seat::North);
    for card in order {
        t.play(card).ok().context("a trick holds four cards")?;
    }
    let points: Vec<u8> = points
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let state = State {
        hand: hex(hand)?,
        legal: hex(legal)?,
        played: hex(played)?,
        trick: t,
        points: points[..].try_into()?,
        greedy: from_bit(greedy.parse()?),
    };
    let label = hex(label)?;
    let twin = hex(twin)?;
    let legal: Vec<Card> = state.legal.into_iter().collect();
    Ok(Decision {
        block: block.parse()?,
        cards: legal.iter().map(|&c| features(&state, c)).collect(),
        label: legal.iter().map(|&c| label.contains(c)).collect(),
        twin: legal.iter().position(|&c| twin.contains(c)),
        greedy: legal
            .iter()
            .position(|&c| c == state.greedy)
            .context("greedy plays a legal card")?,
    })
}

fn dot(w: &[f64; F], x: &[f64; F]) -> f64 {
    w.iter().zip(x).map(|(a, b)| a * b).sum()
}

fn softmax(w: &[f64; F], d: &Decision) -> Vec<f64> {
    let s: Vec<f64> = d.cards.iter().map(|x| dot(w, x)).collect();
    let max = s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let e: Vec<f64> = s.iter().map(|v| (v - max).exp()).collect();
    let z: f64 = e.iter().sum();
    e.into_iter().map(|v| v / z).collect()
}

/// Full-batch Adam on the class likelihood `−log Σ_{c ∈ label} p(c)`.
fn fit(train: &[Decision]) -> [f64; F] {
    const L2: f64 = 1e-4;
    let (mut w, mut m, mut v) = ([0.0; F], [0.0; F], [0.0; F]);
    for step in 1..=600 {
        let grad = train
            .par_iter()
            .map(|d| {
                let p = softmax(&w, d);
                let z: f64 = p
                    .iter()
                    .zip(&d.label)
                    .filter(|(_, l)| **l)
                    .map(|(p, _)| p)
                    .sum();
                let mut g = [0.0; F];
                for ((x, &p), &l) in d.cards.iter().zip(&p).zip(&d.label) {
                    let q = if l { p / z } else { 0.0 };
                    for (g, x) in g.iter_mut().zip(x) {
                        *g += (p - q) * x;
                    }
                }
                g
            })
            .reduce(
                || [0.0; F],
                |mut a, b| {
                    a.iter_mut().zip(b).for_each(|(a, b)| *a += b);
                    a
                },
            );
        let n = train.len() as f64;
        for i in 0..F {
            let g = grad[i] / n + L2 * w[i];
            m[i] = 0.9 * m[i] + 0.1 * g;
            v[i] = 0.999 * v[i] + 0.001 * g * g;
            let (mh, vh) = (
                m[i] / (1.0 - 0.9_f64.powi(step)),
                v[i] / (1.0 - 0.999_f64.powi(step)),
            );
            w[i] -= 0.05 * mh / (vh.sqrt() + 1e-8);
        }
    }
    w
}

fn pick(w: &[f64; F], d: &Decision) -> usize {
    d.cards
        .iter()
        .map(|x| dot(w, x))
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a decision has legal cards")
        .0
}

/// Mean and block-clustered SE of per-decision values.
fn clustered(values: &[(u32, f64)]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().map(|v| v.1).sum::<f64>() / n;
    let mut by_block = std::collections::BTreeMap::<u32, f64>::new();
    for &(block, v) in values {
        *by_block.entry(block).or_default() += v - mean;
    }
    (
        mean,
        by_block.values().map(|s| s * s).sum::<f64>().sqrt() / n,
    )
}

fn report(name: &str, w: &[f64; F], set: &[Decision]) {
    let agree = |i: usize, d: &Decision| f64::from(u8::from(d.label[i]));
    let rows = |f: &dyn Fn(&Decision) -> f64| -> Vec<(u32, f64)> {
        set.iter().map(|d| (d.block, f(d))).collect()
    };
    let (g, gse) = clustered(&rows(&|d| agree(d.greedy, d)));
    let (m, mse) = clustered(&rows(&|d| agree(pick(w, d), d)));
    let (diff, dse) = clustered(&rows(&|d| agree(pick(w, d), d) - agree(d.greedy, d)));
    let twins: Vec<_> = set
        .iter()
        .filter_map(|d| d.twin.map(|t| (d.block, agree(t, d))))
        .collect();
    let (t, tse) = clustered(&twins);
    println!(
        "{name}: twin search agrees {t:.4}±{tse:.4} over {}",
        twins.len()
    );
    let deviating: Vec<_> = set.iter().filter(|d| !d.label[d.greedy]).collect();
    let caught = deviating.iter().filter(|d| d.label[pick(w, d)]).count();
    let false_moves = set
        .iter()
        .filter(|d| d.label[d.greedy] && !d.label[pick(w, d)])
        .count();
    println!(
        "{name}: {} decisions; greedy {g:.4}±{gse:.4}, model {m:.4}±{mse:.4}, \
         diff {diff:+.4}±{dse:.4}; label deviates {} ({:.2}%), model catches {caught}, \
         breaks {false_moves}",
        set.len(),
        deviating.len(),
        100.0 * deviating.len() as f64 / set.len() as f64,
    );
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str, default: u64| -> Result<u64> {
        args.iter()
            .position(|a| a == name)
            .map_or(Ok(default), |i| {
                Ok(args.get(i + 1).context("flag value")?.parse()?)
            })
    };
    match args.first().map(String::as_str) {
        Some("gen") => {
            let (blocks, seed) = (flag("--blocks", 1000)?, flag("--seed", 0)?);
            let samples = u32::try_from(flag("--samples", 256)?)?;
            let lines: Vec<Vec<String>> = (0..u32::try_from(blocks)?)
                .into_par_iter()
                .map(|index| gen_block(seed, samples, index))
                .collect();
            for line in lines.iter().flatten() {
                println!("{line}");
            }
        }
        Some("fit") => {
            let path = args.get(1).context("fit FILE")?;
            let data: Vec<Decision> = std::fs::read_to_string(path)?
                .lines()
                .map(parse)
                .collect::<Result<_>>()?;
            let (test, train): (Vec<_>, Vec<_>) = data.into_iter().partition(|d| d.block % 5 == 0);
            let w = fit(&train);
            report("train", &w, &train);
            report("test ", &w, &test);
            println!("weights: {w:.3?}");
        }
        _ => bail!("usage: distill gen [--blocks N] [--seed N] [--samples N] | distill fit FILE"),
    }
    Ok(())
}
