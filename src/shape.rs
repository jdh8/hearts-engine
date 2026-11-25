//! Suit-shape vocabulary: what one seat can say about a suit from its own
//! cards and the cards already played
//!
//! Everything here reads two [`Holding`]s of one suit — `mine`, and the
//! **outstanding** cards, those neither mine nor played — so it is legal
//! knowledge by construction and cheap enough for the rollouts.

#[cfg(any(feature = "rand", test))]
use hearts::{Card, Hand, Suit};
use hearts::{Holding, Rank};

/// The ranks strictly below `rank`, as a holding mask.
pub(crate) fn below(rank: Rank) -> Holding {
    Holding::from_bits_truncate((1u16 << rank.get()) - 1)
}

/// My cards that are each higher than every outstanding card of the suit
///
/// At the pass this is the unbroken run down from the ace.
pub(crate) fn top_run(mine: Holding, out: Holding) -> Holding {
    out.iter().next_back().map_or(mine, |top| mine - below(top))
}

/// Expected rounds a suit can be led before the opponents have none left,
/// indexed by how many cards of it are outstanding
///
/// Derived, not tabulated by hand: the mean of the longest of the three
/// opponents' holdings when the outstanding cards fall at random among
/// their 39 cards (a multivariate hypergeometric, summed exactly).  It
/// runs 1, 1.32, 1.84, ... 5.9 — slower than a round per three cards,
/// because holdings are uneven.
// ponytail: deal-time hands of 13.  Only the pass reads it today; a
// mid-round caller wants the opponents' real hand sizes instead.
#[cfg(any(feature = "rand", test))]
const ROUNDS: [f64; 14] = {
    const fn choose(n: usize, k: usize) -> f64 {
        let mut out = 1.0;
        let mut i = 0;
        while i < k {
            out = out * (n - i) as f64 / (i + 1) as f64;
            i += 1;
        }
        out
    }
    let mut table = [0.0; 14];
    let mut n = 0;
    while n < 14 {
        let mut a = 0;
        while a <= n {
            let mut b = 0;
            while a + b <= n {
                let c = n - a - b;
                let longest = if a >= b && a >= c {
                    a
                } else if b >= c {
                    b
                } else {
                    c
                };
                table[n] += choose(13, a) * choose(13, b) * choose(13, c) * longest as f64;
                b += 1;
            }
            a += 1;
        }
        table[n] /= choose(39, n);
        n += 1;
    }
    table
};

/// The cards of a suit that win tricks for a seat trying to win them all:
/// the top run, or the whole suit once the run is at least as long as the
/// expected rounds — it draws every outstanding card, so the small cards
/// win too
#[cfg(any(feature = "rand", test))]
pub(crate) fn winners(mine: Holding, out: Holding) -> Holding {
    let run = top_run(mine, out);
    if run.len() as f64 >= ROUNDS[out.len()] {
        mine
    } else {
        run
    }
}

/// The low cards of a suit that will very probably lose the trick when the
/// suit is led, judged from the bottom up
///
/// Both tests come from there being three opponents.  My card takes the
/// trick only if all three play under it, so with at most two outstanding
/// cards below it somebody has to be void for that to happen.  And each
/// duck already spent costs a round of up to three outstanding cards, so
/// the k-th duck needs more than `3k` cards still above it to be sure one
/// is left to lose to.
pub(crate) fn ducking(mine: Holding, out: Holding) -> Holding {
    let mut ducks = Holding::EMPTY;
    for (spent, rank) in mine.iter().enumerate() {
        let under = (out & below(rank)).len();
        if under > 2 || out.len() - under <= 3 * spent {
            break;
        }
        ducks.insert(rank);
    }
    ducks
}

/// How exposed a suit is: its cards that must win tricks sooner or later,
/// less the ducking cards beneath them
///
/// Signed.  `A K J` is 3, `A K 3 2` is 0, and an ace over three small
/// cards is −2: more escapes than the suit will need.
pub(crate) fn exposure(mine: Holding, out: Holding) -> i32 {
    mine.len() as i32 - 2 * ducking(mine, out).len() as i32
}

/// The spare length of a stopper, or `None` when the suit is not one
///
/// A suit stops a player leading it from the top when I hold more cards in
/// it than there are outstanding cards above my highest, and the opponents
/// still hold some.  The spare length is how many I can discard and keep
/// it.
pub(crate) fn spare(mine: Holding, out: Holding) -> Option<usize> {
    let top = mine.iter().next_back()?;
    if out.is_empty() {
        return None;
    }
    mine.len().checked_sub((out - below(top)).len() + 1)
}

/// From this many hearts up, a heart suit is *long*: its high cards are
/// guards rather than liabilities.  Four, because nine outstanding hearts
/// average three to an opponent — at four I outlast the average holder.
pub(crate) const LONG_HEARTS: usize = 4;

/// The pass a shooter would make: three rounds of shed the card that helps
/// a shot least, rescoring as the hand shrinks
///
/// While hearts are short, a heart that is not a winner goes first — it is
/// a point the shot cannot collect by force.  Otherwise the lowest loser
/// of the suit with the fewest winners (shorter suit on a tie) goes.
#[cfg(any(feature = "rand", test))]
pub(crate) fn shooter_pass(mut hand: Hand) -> [Card; 3] {
    [(); 3].map(|()| {
        let current = hand;
        let wins = |suit: Suit| winners(current[suit], Holding::ALL - current[suit]);
        let lowest_loser = |suit: Suit| {
            (current[suit] - wins(suit))
                .iter()
                .next()
                .map(|rank| Card { suit, rank })
        };
        let heart = (current[Suit::Hearts].len() < LONG_HEARTS)
            .then(|| lowest_loser(Suit::Hearts))
            .flatten();
        let pick = heart
            .or_else(|| {
                Suit::ASC
                    .into_iter()
                    .filter(|&suit| lowest_loser(suit).is_some())
                    .min_by_key(|&suit| (wins(suit).len(), current[suit].len()))
                    .and_then(lowest_loser)
            })
            // Nothing but winners: any card is a winner given away.
            .or_else(|| current.into_iter().min_by_key(|card| card.rank))
            .expect("a passing hand holds thirteen cards");
        hand.remove(pick);
        pick
    })
}

/// How many non-winners a kept hand carries into a shot, or `None` when
/// one of them is a heart — a heart that cannot be won is a point lost
#[cfg(any(feature = "rand", test))]
pub(crate) fn shot_losers(kept: Hand) -> Option<usize> {
    let losers = |suit: Suit| (kept[suit] - winners(kept[suit], Holding::ALL - kept[suit])).len();
    (losers(Suit::Hearts) == 0).then(|| Suit::ASC.into_iter().map(losers).sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding(text: &str) -> Holding {
        text.parse().expect("a valid holding")
    }

    fn out(mine: Holding) -> Holding {
        Holding::ALL - mine
    }

    #[test]
    fn expected_rounds_are_derived_and_sublinear() {
        assert_eq!(ROUNDS[0], 0.0);
        assert!((ROUNDS[1] - 1.0).abs() < 1e-12);
        // Two cards share a hand 12 times in 38.
        assert!((ROUNDS[2] - (1.0 + 12.0 / 38.0)).abs() < 1e-12);
        for n in 1..14 {
            assert!(ROUNDS[n] > ROUNDS[n - 1]);
            assert!(ROUNDS[n] >= n as f64 / 3.0 && ROUNDS[n] <= n as f64);
        }
    }

    #[test]
    fn a_long_enough_run_makes_the_small_cards_win() {
        let mine = holding("23JQKA");
        assert_eq!(top_run(mine, out(mine)), holding("JQKA"));
        // Seven outstanding last under three rounds on average; four top
        // cards draw them.
        assert_eq!(winners(mine, out(mine)), mine);

        let short = holding("23A");
        assert_eq!(winners(short, out(short)), holding("A"));
        // A gap in the run stops it.
        assert_eq!(top_run(holding("QA"), out(holding("QA"))), holding("A"));
        // Nothing outstanding: every card wins.
        assert_eq!(top_run(holding("27"), Holding::EMPTY), holding("27"));
    }

    #[test]
    fn exposure_reads_the_bottom_of_the_suit() {
        // The spec's example: club honours with nothing under them, diamond
        // honours over two sure ducks.
        let clubs = holding("JKA");
        let diamonds = holding("23KA");
        assert_eq!(ducking(clubs, out(clubs)), Holding::EMPTY);
        assert_eq!(ducking(diamonds, out(diamonds)), holding("23"));
        assert_eq!(exposure(clubs, out(clubs)), 3);
        assert_eq!(exposure(diamonds, out(diamonds)), 0);
        // An ace over three small cards is nearly safe.
        assert_eq!(exposure(holding("234A"), out(holding("234A"))), -2);
        // A fourth small card is no duck: three rounds may leave nothing
        // above it.
        assert_eq!(
            ducking(holding("2345"), out(holding("2345"))),
            holding("234")
        );
        // Three outstanding cards under the six: all three can play low.
        assert_eq!(ducking(holding("6"), out(holding("6"))), Holding::EMPTY);
    }

    #[test]
    fn a_stopper_counts_the_cards_above_it() {
        // K x x under an outstanding ace: the king survives one round, one
        // card to spare.
        assert_eq!(spare(holding("27K"), out(holding("27K"))), Some(1));
        assert_eq!(spare(holding("7K"), out(holding("7K"))), Some(0));
        assert_eq!(spare(holding("K"), out(holding("K"))), None);
        // The boss card alone stops; nothing outstanding stops nothing.
        assert_eq!(spare(holding("A"), out(holding("A"))), Some(0));
        assert_eq!(spare(holding("A"), Holding::EMPTY), None);
    }

    #[test]
    fn the_shape_gate_reads_the_kept_ten() {
        let hand = |text: &str| text.parse::<Hand>().expect("a valid hand");
        // Solid hearts, two top clubs, the top spades: the shooter sheds
        // the three small diamonds and keeps ten winners.
        let strong = hand("KA.234.9TJQKA.KA");
        let pass: Hand = shooter_pass(strong).into_iter().collect();
        assert_eq!(pass, hand(".234.."));
        assert_eq!(shot_losers(strong - pass), Some(0));

        // A losing heart goes first while hearts are short, and a kept one
        // fails the gate whatever else the hand holds.
        let thin = hand("JQKA.JQKA.3A.QKA");
        assert!(shooter_pass(thin).contains(&"3♥".parse().unwrap()));
        assert_eq!(shot_losers(hand("89TJQKA.KA.2.")), None);
        // Junk counts its losers.
        assert_eq!(shot_losers(hand("234.567.A.89T")), Some(9));
    }
}
