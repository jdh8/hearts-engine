//! [`HeuristicBot`]: a deterministic knowledge-based player
//!
//! The knowledge-free greedy core in this module — duck under the winner,
//! dump the most dangerous card when void, smoke out the Q♠ — doubles as
//! the rollout policy of the Monte Carlo bot.  The bot itself layers a
//! knowledge-based moon-defense overlay on top: when a single opponent has
//! swept every point so far, it stops ducking and takes a trick to kill
//! the moon.

use crate::{Strategy, View};
use hearts::{Card, Hand, Holding, Rank, Seat, Suit, Trick};

/// The ranks strictly below `rank`, as a holding mask.
fn below(rank: Rank) -> Holding {
    Holding::from_bits_truncate((1u16 << rank.get()) - 1)
}

/// How badly `card` wants to be passed away, higher first
///
/// The knowledge-free pass policy shared with the Monte Carlo rollouts:
/// an unprotected Q♠ tops the list, bare A♠/K♠ follow (they catch the
/// queen), hearts weigh double their rank, and cards of a short non-spade
/// suit earn a void bonus scaled by `void_weight`.
pub(crate) fn pass_score(hand: Hand, card: Card, void_weight: u8) -> i32 {
    let mut score = i32::from(card.rank.get());

    let guards = (hand[Suit::Spades] & below(Rank::Q)).len();
    if card.suit == Suit::Spades && guards < 3 {
        if card == Card::QUEEN_OF_SPADES {
            score += 100;
        } else if card.rank == Rank::A {
            score += 90;
        } else if card.rank == Rank::K {
            score += 80;
        }
    }

    if card.suit == Suit::Hearts {
        score += 2 * i32::from(card.rank.get());
    }

    // A short side suit is a void in the making; spades keep their guards.
    let len = hand[card.suit].len() as i32;
    if card.suit != Suit::Spades && len <= 3 {
        score += i32::from(void_weight) * (8 - 2 * len);
    }
    score
}

/// The knowledge-free greedy pass: the three highest-scoring cards
pub(crate) fn greedy_pass(hand: Hand, void_weight: u8) -> [Card; 3] {
    let mut cards: Vec<Card> = hand.into_iter().collect();
    cards.sort_by_key(|&card| -pass_score(hand, card, void_weight));
    [cards[0], cards[1], cards[2]]
}

/// The card currently winning `trick`
fn winning_card(trick: Trick) -> Option<Card> {
    trick.winner().and_then(|seat| trick.card_from(seat))
}

/// The most dangerous card to be rid of when free to discard anything
///
/// The Q♠ first, then a bare A♠/K♠ while the queen is at large, then the
/// highest heart, then the highest card overall.
fn best_dump(legal: Hand, played: Hand) -> Card {
    if legal.contains(Card::QUEEN_OF_SPADES) {
        return Card::QUEEN_OF_SPADES;
    }
    if !played.contains(Card::QUEEN_OF_SPADES)
        && let Some(rank) = [Rank::A, Rank::K]
            .into_iter()
            .find(|&rank| legal[Suit::Spades].contains(rank))
    {
        return Card {
            suit: Suit::Spades,
            rank,
        };
    }
    if let Some(rank) = legal[Suit::Hearts].iter().next_back() {
        return Card {
            suit: Suit::Hearts,
            rank,
        };
    }
    legal
        .into_iter()
        .max_by_key(|card| card.rank)
        .expect("legal plays are never empty on turn")
}

/// The knowledge-free greedy play policy, shared with the Monte Carlo
/// rollouts
///
/// `legal` must be the acting seat's non-empty legal set, `trick` the
/// trick in progress, and `played` every card played so far.
pub(crate) fn greedy_play(legal: Hand, trick: Trick, played: Hand) -> Card {
    let lowest = |cards: Hand| {
        cards
            .into_iter()
            .min_by_key(|card| card.rank)
            .expect("the set was checked non-empty")
    };

    let Some(led) = trick.suit() else {
        // Leading.  While the Q♠ is at large and not ours, smoke it out
        // with low spades; otherwise lead our lowest card.
        if !played.contains(Card::QUEEN_OF_SPADES) && !legal.contains(Card::QUEEN_OF_SPADES) {
            let low_spades = legal[Suit::Spades] & below(Rank::Q);
            if let Some(rank) = low_spades.iter().next() {
                return Card {
                    suit: Suit::Spades,
                    rank,
                };
            }
        }
        return lowest(legal);
    };

    let follow = legal[led];
    if follow.is_empty() {
        // Void: dump the most dangerous card.
        return best_dump(legal, played);
    }

    let winner = winning_card(trick).expect("a led trick has a winning card");

    // A dead queen: the A♠ or K♠ already sits in the trick, so the Q♠
    // rides for free.
    if led == Suit::Spades && follow.contains(Rank::Q) && winner.rank > Rank::Q {
        return Card::QUEEN_OF_SPADES;
    }

    // Duck with the highest card under the winner.
    if let Some(rank) = (follow & below(winner.rank)).iter().next_back() {
        return Card { suit: led, rank };
    }

    // Forced above the winner; never hand ourselves the Q♠ if avoidable.
    let safe = if led == Suit::Spades {
        let others = follow - Holding::from_rank(Rank::Q);
        if others.is_empty() { follow } else { others }
    } else {
        follow
    };

    // Last to play takes the trick anyway, so shed the highest; earlier,
    // play the lowest and hope someone overtakes.
    let rank = if trick.len() == 3 {
        safe.iter().next_back().expect("checked non-empty")
    } else {
        safe.iter().next().expect("checked non-empty")
    };
    Card { suit: led, rank }
}

/// Tuning knobs for [`HeuristicBot`]
///
/// Like [`hearts::Rules`], the struct is non-exhaustive: start from
/// [`HeuristicConfig::default`] and adjust fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct HeuristicConfig {
    /// Take a trick to kill a suspected moon once a single opponent has
    /// swept this many points; `u8::MAX` effectively disables the defense
    pub moon_defense: u8,
    /// Weight of the void bonus when scoring passes
    ///
    /// Zero passes purely on card danger; the default is 1.
    pub void_weight: u8,
}

impl Default for HeuristicConfig {
    fn default() -> Self {
        Self {
            moon_defense: 8,
            void_weight: 1,
        }
    }
}

/// A deterministic knowledge-based player
///
/// Fast enough for tournaments at any scale: every decision is a handful
/// of bit operations.
#[derive(Debug, Clone, Copy, Default)]
pub struct HeuristicBot {
    config: HeuristicConfig,
}

impl HeuristicBot {
    /// A bot with the default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A bot with custom tuning
    #[must_use]
    pub const fn with_config(config: HeuristicConfig) -> Self {
        Self { config }
    }

    /// The opponent who has swept every point so far, once the sweep
    /// weighs enough to smell like a moon attempt
    fn moon_threat(&self, view: &View<'_>) -> Option<Seat> {
        let mut sweeper = None;
        for seat in Seat::ALL {
            if view.points_taken(seat) > 0 {
                if sweeper.is_some() {
                    return None;
                }
                sweeper = Some(seat);
            }
        }
        sweeper.filter(|&seat| {
            seat != view.seat() && view.points_taken(seat) >= self.config.moon_defense
        })
    }
}

impl Strategy for HeuristicBot {
    fn pass_cards(&mut self, view: &View<'_>) -> [Card; 3] {
        greedy_pass(view.hand(), self.config.void_weight)
    }

    fn play_card(&mut self, view: &View<'_>) -> Card {
        let legal = view.legal_plays();
        let trick = view.current_trick().expect("a play decision has a trick");

        // Moon defense: when the sweeper is winning this trick, stop
        // ducking — beat them as cheaply as possible (but never with our
        // own Q♠ if avoidable), and when we cannot beat them, at least
        // gift them no penalty card toward the sweep.
        if let Some(threat) = self.moon_threat(view)
            && let Some(led) = trick.suit()
            && trick.winner() == Some(threat)
        {
            let winner = winning_card(trick).expect("a led trick has a winning card");
            let follow = legal[led];
            if follow.is_empty() {
                // Void: dump the most dangerous card that carries no
                // points into the sweeper's trick.
                let harmless = legal - Hand::PENALTIES;
                if let Some(card) = harmless.into_iter().max_by_key(|card| card.rank) {
                    return card;
                }
            } else {
                // Beat them as cheaply as possible — taking a trick with
                // our own Q♠ costs more than the sweep it denies.
                let mut beat = follow - below(winner.rank) - Holding::from_rank(winner.rank);
                if led == Suit::Spades {
                    beat -= Holding::from_rank(Rank::Q);
                }
                if let Some(rank) = beat.iter().next() {
                    return Card { suit: led, rank };
                }
                // Can't beat the sweep: duck plainly — and never ride the
                // dead queen into the sweeper's trick.
                let mut duck = follow & below(winner.rank);
                if led == Suit::Spades {
                    let harmless = duck - Holding::from_rank(Rank::Q);
                    if !harmless.is_empty() {
                        duck = harmless;
                    }
                }
                if let Some(rank) = duck.iter().next_back() {
                    return Card { suit: led, rank };
                }
            }
        }

        greedy_play(legal, trick, view.played())
    }

    fn name(&self) -> &str {
        "greedy"
    }
}

/// The rollout flavor of the policy, for omniscient Monte Carlo worlds
#[cfg(feature = "rand")]
pub(crate) fn rollout_play(round: &hearts::Round, seat: Seat) -> Card {
    greedy_play(
        round.legal_plays(seat),
        round.current_trick().expect("a playing round has a trick"),
        round.played(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(text: &str) -> Card {
        text.parse().expect("a valid card")
    }

    fn hand(text: &str) -> Hand {
        text.parse().expect("a valid hand")
    }

    #[test]
    fn passing_dumps_the_unprotected_queen() {
        // Q♠ with one guard, the A♥, and filler.
        let dealt = hand("234567.89.A.2Q");
        let picks = greedy_pass(dealt, 1);
        assert!(picks.contains(&card("Q♠")));
        assert!(picks.contains(&card("A♥")));

        // Well guarded, the queen stays; the hearts and short suits go.
        let guarded = hand("234567.8.9A.2345Q");
        let picks = greedy_pass(guarded, 1);
        assert!(!picks.contains(&card("Q♠")));
    }

    #[test]
    fn ducking_under_the_winner() {
        let mut trick = Trick::new(Seat::North);
        trick.play(card("9♦")).unwrap();
        // Duck with the highest diamond under the nine.
        assert_eq!(
            greedy_play(hand(".2378Q.."), trick, Hand::EMPTY),
            card("8♦")
        );
    }

    #[test]
    fn void_dumps_the_queen_first() {
        let mut trick = Trick::new(Seat::North);
        trick.play(card("9♦")).unwrap();
        assert_eq!(greedy_play(hand("2..A.QA"), trick, Hand::EMPTY), card("Q♠"));
        // Queen gone: a bare A♠ goes while she is at large.
        assert_eq!(greedy_play(hand("2..A.A"), trick, Hand::EMPTY), card("A♠"));
        // Queen already played: the high heart goes instead.
        assert_eq!(
            greedy_play(hand("2..A.A"), trick, Hand::QUEEN_OF_SPADES),
            card("A♥")
        );
    }

    #[test]
    fn dead_queen_rides() {
        let mut trick = Trick::new(Seat::North);
        trick.play(card("2♠")).unwrap();
        trick.play(card("A♠")).unwrap();
        assert_eq!(greedy_play(hand("...5Q"), trick, Hand::EMPTY), card("Q♠"));
    }

    #[test]
    fn forced_above_the_winner() {
        let mut trick = Trick::new(Seat::North);
        trick.play(card("5♠")).unwrap();
        trick.play(card("2♠")).unwrap();
        trick.play(card("3♠")).unwrap();
        // Last to play and forced to take: shed the king, never the queen.
        assert_eq!(greedy_play(hand("...QK"), trick, Hand::EMPTY), card("K♠"));

        let mut early = Trick::new(Seat::North);
        early.play(card("5♠")).unwrap();
        // Second to play: the low card rides, hoping someone overtakes.
        assert_eq!(greedy_play(hand("...9K"), early, Hand::EMPTY), card("9♠"));
    }

    #[test]
    fn moon_defense_never_spends_or_gifts_the_queen() {
        use crate::Table;
        use hearts::{PassDirection, Round, Rules};

        // South holds nothing but hearts and bleeds on trick 1, arming
        // East as the sole sweeper for a bot with moon_defense = 1.
        let hands = [
            hand("23456789TJQ...QK"), // North: low clubs + Q♠ K♠
            hand("A.23456789TJQ..J"), // East: A♣, diamonds, J♠
            hand("..23456789TJQKA."), // South: all hearts
            hand("K.KA..23456789TA"), // West: the rest
        ];
        let round = Round::from_deal(Rules::new(), PassDirection::Hold, hands).unwrap();
        let mut table = Table::new(round);
        let config = HeuristicConfig {
            moon_defense: 1,
            ..HeuristicConfig::default()
        };
        let mut defender = HeuristicBot::with_config(config);

        // Trick 1: E takes A♣ over the forced 2♥ — one point, armed.
        for (seat, text) in [
            (Seat::North, "2♣"),
            (Seat::East, "A♣"),
            (Seat::South, "2♥"),
            (Seat::West, "K♣"),
        ] {
            struct Fixed(Card);
            impl Strategy for Fixed {
                fn pass_cards(&mut self, _: &View<'_>) -> [Card; 3] {
                    unreachable!()
                }
                fn play_card(&mut self, _: &View<'_>) -> Card {
                    self.0
                }
            }
            assert_eq!(table.turn(), Some(seat));
            table.step(&mut Fixed(card(text))).unwrap();
        }

        // Trick 2: the sweeper leads J♠; South and West stay under it.
        for text in ["J♠", "3♥", "2♠"] {
            struct Fixed(Card);
            impl Strategy for Fixed {
                fn pass_cards(&mut self, _: &View<'_>) -> [Card; 3] {
                    unreachable!()
                }
                fn play_card(&mut self, _: &View<'_>) -> Card {
                    self.0
                }
            }
            let seat = table.turn().unwrap();
            table.step(&mut Fixed(card(text))).unwrap();
            let _ = seat;
        }

        // North must beat the sweep with the K♠ — never its own Q♠.
        assert_eq!(table.turn(), Some(Seat::North));
        assert_eq!(defender.play_card(&table.view(Seat::North)), card("K♠"));
    }

    #[test]
    fn smoke_out_the_queen() {
        let trick = Trick::new(Seat::North);
        let played = hand("2345...");
        // Leading without the queen: low spades hunt it.
        assert_eq!(greedy_play(hand("9.9.9.29"), trick, played), card("2♠"));
        // Holding the queen: no self-hunt, but the 2♠ is still our lowest.
        assert_eq!(greedy_play(hand("9.9..29Q"), trick, played), card("2♠"));
        // Holding the queen and higher clubs: lead the lowest card, no hunt.
        assert_eq!(greedy_play(hand("4.9..9Q"), trick, played), card("4♣"));
    }
}
