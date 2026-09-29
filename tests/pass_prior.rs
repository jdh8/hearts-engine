#![cfg(feature = "rand")]

//! An `#[ignore]`d P1 instrument for the incoming cards produced by the
//! shipped greedy pass policy.
//!
//! This deliberately deals all four hands instead of sampling a lone
//! 13-card giver hand. Our hand and the giver's are dependent: cards we hold,
//! the giver cannot, and that dependence moves the giver's shape, not just
//! where the queen is. Holding five spades drops the giver's expected spade
//! count from 3.25 to `8 * 13 / 39 = 2.67`, and that count gates the entire
//! Q♠/A♠/K♠ bonus tier in `pass_score`. A marginal single-hand deal gets the
//! refill curve and the guard distribution wrong.
//!
//! Predictions, recorded before the run so the run can kill them:
//!
//! 1. **P(receive Q♠ | we lack her) ≈ 0.30.** She sits with the giver at
//!    exactly 1/3 when we lack her; an unguarded Q♠ scores `100 + 12 = 112`,
//!    the global maximum of `pass_score` (A♠ = 104, K♠ = 93, the best heart =
//!    20), so the model passes her whenever fewer than `spade_guards = 5`
//!    below-queen spades sit in the giver's other twelve cards. This is a
//!    large, currently unpriced arrival.
//! 2. **The rank histogram is hard high-skewed.** Base score is bare rank and
//!    the only bonuses ride honors and short suits.
//! 3. **Expected received spades below the queen ≈ 0.** They score bare rank
//!    and take no void bonus (spades are excluded from it), so the model
//!    essentially never sends them. If confirmed, proposal P3(iii) is a
//!    recorded no-build. The soft spot is J♠ = 11, hence the per-rank
//!    breakdown.
//! 4. **The refill curve is skewed off the uniform null.** It is pulled up in
//!    short suits by the void bonus and pulled down by the giver's own length
//!    in the suits we are short of. Which way it nets is the number the
//!    sibling doc needs, and there is no prediction for the sign.
//!
//! `--nocapture` is load-bearing: libtest swallows a passing test's stdout,
//! and the table is the result. Run it on demand with:
//!
//! ```console
//! cargo test --release --test pass_prior -- --ignored --nocapture
//! ```

use hearts_engine::hearts::{Card, PassDirection, Rank, Rules, Seat, Suit};
use hearts_engine::{HeuristicBot, Strategy, Table};
use rand::SeedableRng as _;
use rand::rngs::StdRng;

const SEED: u64 = 0x261;
const DEALS: usize = 1_000_000;
const RANKS: [&str; 13] = [
    "2", "3", "4", "5", "6", "7", "8", "9", "T", "J", "Q", "K", "A",
];

#[test]
#[ignore = "P1 pass-prior measurement: one million deals, run on demand"]
fn measure_pass_prior() {
    let rules = Rules::new();
    let giver = Seat::North;
    let me = PassDirection::Left.receiver(giver);
    let honors = [
        Card::QUEEN_OF_SPADES,
        Card {
            suit: Suit::Spades,
            rank: Rank::A,
        },
        Card {
            suit: Suit::Spades,
            rank: Rank::K,
        },
    ];
    let mut rng = StdRng::seed_from_u64(SEED);
    let mut bot = HeuristicBot::new();

    let mut honor_eligible = [0_u64; 3];
    let mut honor_held = [0_u64; 3];
    let mut honor_received = [0_u64; 3];
    let mut rank_counts = [0_u64; 13];
    let mut suit_counts = [0_u64; 4];
    let mut refill_deals = [[0_u64; 4]; 4];
    let mut refill_hits = [[0_u64; 4]; 4];
    let mut low_spade_counts = [0_u64; 13];

    for _ in 0..DEALS {
        let table = Table::deal(rules, PassDirection::Left, &mut rng);
        let theirs = table.round().hand(giver);
        let mine = table.round().hand(me);
        let pass = bot.pass_cards(&table.view(giver));

        for (index, honor) in honors.into_iter().enumerate() {
            if !mine.contains(honor) {
                honor_eligible[index] += 1;
                if theirs.contains(honor) {
                    honor_held[index] += 1;
                }
                if pass.contains(&honor) {
                    honor_received[index] += 1;
                }
            }
        }

        for card in pass {
            let rank_index = usize::from(card.rank.get() - 2);
            rank_counts[rank_index] += 1;
            suit_counts[card.suit as usize] += 1;
            if card.suit == Suit::Spades && card.rank < Rank::Q {
                low_spade_counts[rank_index] += 1;
            }
        }

        for suit in Suit::ASC {
            let k = mine[suit].len();
            if k <= 3 {
                let suit_index = suit as usize;
                refill_deals[suit_index][k] += 1;
                if pass.iter().any(|card| card.suit == suit) {
                    refill_hits[suit_index][k] += 1;
                }
            }
        }
    }

    let probability = |count: u64, total: u64| count as f64 / total as f64;

    println!("P1 greedy incoming-pass prior ({DEALS} deals, seed {SEED:#x})");
    println!();
    println!("(1) Spade honors, conditioned on our hand lacking the card");
    println!("honor    P(giver holds) P(we receive) P(passes | holds)");
    for (index, label) in ["Q♠", "A♠", "K♠"].into_iter().enumerate() {
        println!(
            "{label:<5} {:>8.4}       {:>8.4}          {:>8.4}",
            probability(honor_held[index], honor_eligible[index]),
            probability(honor_received[index], honor_eligible[index]),
            probability(honor_received[index], honor_held[index]),
        );
    }
    println!(
        "note: P(giver holds) must read 0.3333; otherwise conditioning is wrong and every other number is garbage."
    );

    println!();
    println!("(2) Received rank histogram ({} cards)", 3 * DEALS);
    println!("rank        count    share");
    for (index, label) in RANKS.into_iter().enumerate() {
        println!(
            "{label:<4} {:>12} {:>8.4}",
            rank_counts[index],
            probability(rank_counts[index], (3 * DEALS) as u64),
        );
    }

    println!();
    println!("(3) Expected received cards per suit");
    println!("suit     mean/pass uniform");
    for suit in Suit::ASC {
        println!(
            "{suit:<4} {:>8.4} {:>8.4}",
            probability(suit_counts[suit as usize], DEALS as u64),
            0.75,
        );
    }

    // The uniform null: three cards drawn at random from the 39 we do not
    // hold, of which 13 - k are of suit s.
    let uniform = |k: usize| {
        let c3 = |n: f64| n * (n - 1.0) * (n - 2.0) / 6.0;
        1.0 - c3(26.0 + k as f64) / c3(39.0)
    };
    println!();
    println!("(4) Refill curve: P(at least one received card of suit)");
    println!("suit k        deals observed  uniform");
    for suit in Suit::ASC {
        for k in 0..=3 {
            let suit_index = suit as usize;
            println!(
                "{suit:<4} {k} {:>12} {:>8.4} {:>8.4}",
                refill_deals[suit_index][k],
                probability(refill_hits[suit_index][k], refill_deals[suit_index][k]),
                uniform(k),
            );
        }
    }

    let low_spade_total = low_spade_counts.iter().sum::<u64>();
    println!();
    println!("(5) Received spades below the queen");
    println!(
        "mean per pass {:>8.4}",
        probability(low_spade_total, DEALS as u64)
    );
    println!("rank        count mean/pass");
    for index in (0..=usize::from(Rank::J.get() - 2)).rev() {
        println!(
            "{}♠   {:>12} {:>8.4}",
            RANKS[index],
            low_spade_counts[index],
            probability(low_spade_counts[index], DEALS as u64),
        );
    }
}
