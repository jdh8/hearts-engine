//! [`HeuristicBot`]: a deterministic knowledge-based player
//!
//! The knowledge-free greedy core in this module — duck point-bearing
//! tricks, take safe control, dump the most dangerous card when void, smoke
//! out the Q♠ — doubles as the rollout policy of the Monte Carlo bot.  The
//! bot itself layers a knowledge-based moon-defense overlay on top: when a
//! single opponent has swept every point so far, it stops ducking and takes
//! a trick to kill the moon.

use crate::shape::{LONG_HEARTS, below, ducking, exposure, spare, top_run};
use crate::{Strategy, View};
use hearts::{Card, Hand, Holding, Rank, Seat, Suit, Trick};

/// How badly `card` wants to be passed away, higher first
///
/// The knowledge-free pass policy shared with the Monte Carlo rollouts:
/// an unprotected Q♠ tops the list, bare A♠/K♠ follow (they catch the
/// queen), hearts weigh an extra `heart_weight` times their rank, the 2♣
/// takes a flat `two_of_clubs_bonus` for its forced lead, and cards of a
/// short non-spade suit earn a void bonus scaled by `void_weight`.  Lacking
/// the queen, spades below her lose `queen_insurance`.
pub(crate) fn pass_score(hand: Hand, card: Card, config: HeuristicConfig) -> i32 {
    let mut score = i32::from(card.rank.get());

    let guards = (hand[Suit::Spades] & below(Rank::Q)).len();
    if card.suit == Suit::Spades && guards < usize::from(config.spade_guards) {
        if card == Card::QUEEN_OF_SPADES {
            score += 100;
        } else if card.rank == Rank::A {
            score += 90;
        } else if card.rank == Rank::K {
            score += 80;
        }
    }

    if card.suit == Suit::Hearts {
        score += i32::from(config.heart_weight) * i32::from(card.rank.get());
    }

    // The 2♣ has no play to choose — its holder must lead it at the first
    // trick — so it is the cheapest card in the deck to give away, yet it
    // scores bare rank 2 and is otherwise all but unpassable.
    if card == Card::TWO_OF_CLUBS {
        score += i32::from(config.two_of_clubs_bonus);
    }

    // Lacking the queen, a low spade guards against her arriving in the pass.
    // `greedy_pass` rescores what is left, so passing her insures the spades
    // she leaves behind; the measured policy works that way.
    if card.suit == Suit::Spades
        && card.rank.get() < Rank::Q.get()
        && !hand.contains(Card::QUEEN_OF_SPADES)
    {
        score -= i32::from(config.queen_insurance);
    }

    // A short side suit is a void in the making; spades keep their guards.
    // Loser-aware, a club suit counts one longer: the first trick is a
    // forced club round, which takes a club for nothing and lets nobody
    // discard a point on the void it finds.
    let loser = i32::from(config.loser_weight);
    let len = hand[card.suit].len() as i32;
    let void_len = len + i32::from(loser != 0 && card.suit == Suit::Clubs);
    if card.suit != Suit::Spades && void_len <= 3 {
        score += i32::from(config.void_weight) * (8 - 2 * void_len);
    }

    if loser != 0 && card.suit != Suit::Spades {
        score += loser_term(hand[card.suit], card, loser);
    }
    score
}

/// The loser-aware part of [`pass_score`]: a high card is as dangerous as
/// its suit is exposed
///
/// Ducking cards score as before.  Every other card of a side suit takes
/// `weight` times the suit's signed [`exposure`], less the suit's length —
/// so honours over ducks are kept, bare honours go, and at equal exposure
/// the shorter suit goes first.  A long heart suit is the exception: its
/// high cards are guards, priced as if every one were covered, and its top
/// card scores a rank under its second-highest so that when the suit must
/// give something up, the top one stays.
fn loser_term(mine: Holding, card: Card, weight: i32) -> i32 {
    let out = Holding::ALL - mine;
    if ducking(mine, out).contains(card.rank) {
        return 0;
    }
    let len = mine.len() as i32;
    if card.suit != Suit::Hearts || mine.len() < LONG_HEARTS {
        return weight * exposure(mine, out) - len;
    }
    let mut tops = mine.iter().rev();
    let under_second = match (tops.next(), tops.next()) {
        (Some(top), Some(second)) if top == card.rank => {
            i32::from(top.get()) - i32::from(second.get()) + 1
        }
        _ => 0,
    };
    -weight * len - len - under_second
}

/// Which card leaves first when two [`pass_score`]s tie
///
/// Ties are common — at the shipped `heart_weight = 0` the score is exactly
/// symmetric under permuting ♣/♦/♥ — and they used to fall to whichever suit
/// a stable sort saw first, which is clubs, a bias worth 0.145 cards a pass
/// that nobody chose.  The rules break the symmetry properly: the first trick
/// is led in clubs and cannot score, so a club is the safest side-suit card
/// to keep, with diamonds dearer and hearts dearest.  ♠A/♠K/♠Q lead for the
/// reason the tier bonuses exist; the spades below her are guards and go last.
const fn danger(card: Card) -> u8 {
    match card.suit {
        Suit::Spades if card.rank.get() >= Rank::Q.get() => 4,
        Suit::Spades => 0,
        // ♣ 1, ♦ 2, ♥ 3 — `Suit` is `repr(u8)` in that order.
        suit => suit as u8 + 1,
    }
}

/// The total order the pass policy and the Monte Carlo ranking share
///
/// Total by construction: [`danger`] separates every suit but the two spade
/// bands, and ranks inside one suit are distinct.  No tie survives to be
/// settled by sort stability, so ordering never depends on iteration order.
pub(crate) fn pass_key(hand: Hand, card: Card, config: HeuristicConfig) -> (i32, u8, u8) {
    (
        pass_score(hand, card, config),
        danger(card),
        card.rank.get(),
    )
}

/// The knowledge-free greedy pass: three rounds of take the card that most
/// wants to go, then rescore what is left
///
/// Rescoring is the point.  [`pass_score`] reads the suit lengths of the hand
/// it is handed, so removing a card escalates the void bonus along the suit it
/// came from — a doubleton's two cards are worth +4 then +6 — and a triple
/// that finishes a void outbids three that merely start three.  A single sort
/// cannot see that: it scores every card against the untouched thirteen and
/// happily passes one card out of each of three short suits, emptying nothing.
pub(crate) fn greedy_pass(mut hand: Hand, config: HeuristicConfig) -> [Card; 3] {
    [(); 3].map(|()| {
        // A `Copy` snapshot, so the scan borrows nothing the loop mutates —
        // and so the score is visibly against the *current* hand.
        let current = hand;
        let pick = current
            .into_iter()
            .max_by_key(|&card| pass_key(current, card, config))
            .expect("a passing hand holds thirteen cards");
        hand.remove(pick);
        pick
    })
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

/// The knowledge-free, point-aware greedy play policy shared with the Monte
/// Carlo rollouts
///
/// `legal` must be the acting seat's non-empty legal set, `trick` the
/// trick in progress, and `played` every card played so far.
pub(crate) fn greedy_play(legal: Hand, trick: Trick, played: Hand) -> Card {
    let Some(led) = trick.suit() else {
        // Leading.  While the Q♠ is at large and not ours, smoke it out
        // with low spades — unless we hold the A♠ or K♠: each low spade
        // led spends a guard, and a bare high spade is exactly where the
        // queen lands.  Otherwise lead our lowest card.
        if !played.contains(Card::QUEEN_OF_SPADES)
            && !legal.contains(Card::QUEEN_OF_SPADES)
            && (legal[Suit::Spades] - below(Rank::Q)).is_empty()
        {
            let low_spades = legal[Suit::Spades] & below(Rank::Q);
            if let Some(rank) = low_spades.iter().next() {
                return Card {
                    suit: Suit::Spades,
                    rank,
                };
            }
        }
        // Work the shortest suit first: exhausting it creates a place to
        // shed penalties, while a global low-card ordering merely broke ties
        // toward clubs.
        return legal
            .into_iter()
            .min_by_key(|card| (legal[card.suit].len(), card.rank))
            .expect("legal plays are never empty on turn");
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

    // Last to a clean trick, take the lead with the cheapest winner.  There
    // is nobody left to dump a penalty on us, and control is worth more than
    // ducking a scoreless trick.  Do not turn Q♠ itself into the points.
    if trick.len() == 3 && trick.plays().all(|(_, card)| card.points() == 0) {
        let mut winners = follow - below(winner.rank) - Holding::from_rank(winner.rank);
        if led == Suit::Spades {
            winners -= Holding::from_rank(Rank::Q);
        }
        if let Some(rank) = winners.iter().next() {
            return Card { suit: led, rank };
        }
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
    /// Extra pass-score weight per rank for hearts, atop the base rank
    ///
    /// Zero, the default, prices hearts like any other card: high hearts
    /// are duckable on demand, unlike a caught Q♠.
    pub heart_weight: u8,
    /// High-spade pass bonuses apply while holding fewer than this many
    /// spades below the queen; the default is 4
    pub spade_guards: u8,
    /// Extra pass score for the 2♣ alone
    ///
    /// Its holder must lead it at the first trick, so the card carries no
    /// choice worth keeping — the cheapest thing in the deck to give away —
    /// yet it scores bare rank 2 and is otherwise all but unpassable.  Zero,
    /// the default, leaves it at bare rank.
    pub two_of_clubs_bonus: u8,
    /// Pass-score malus for a spade below the queen while the hand lacks
    /// her; zero disables it
    ///
    /// After the exchange such a spade guards against the queen arriving.
    /// The default, 4, already saturates: larger values change no more
    /// passes.
    pub queen_insurance: u8,
    /// Weight of the loser-aware pass term; zero, the default, disables it
    ///
    /// Nonzero, a side-suit card that is not a sure duck is scored by how
    /// exposed its suit is — honours with few small cards under them go,
    /// honours over ducks stay — high hearts over four or more are kept as
    /// guards, and a club void counts for less than a diamond one.
    pub loser_weight: u8,
}

impl HeuristicConfig {
    /// The shipped tuning, in a `const` context
    ///
    /// [`Default`] delegates here, as [`hearts::Rules`] does: a `const fn`
    /// constructor — [`MonteCarloBot::new`](crate::MonteCarloBot::new) —
    /// cannot call a trait method.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            moon_defense: 8,
            void_weight: 1,
            heart_weight: 0,
            spade_guards: 4,
            two_of_clubs_bonus: 6,
            queen_insurance: 4,
            loser_weight: 0,
        }
    }
}

impl Default for HeuristicConfig {
    fn default() -> Self {
        Self::new()
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
}

/// The opponent who has swept every point so far, once the sweep weighs at
/// least `threshold` — enough to smell like a moon attempt
fn moon_threat(view: &View<'_>, threshold: u8) -> Option<Seat> {
    let mut sweeper = None;
    for seat in Seat::ALL {
        if view.points_taken(seat) > 0 {
            if sweeper.is_some() {
                return None;
            }
            sweeper = Some(seat);
        }
    }
    sweeper.filter(|&seat| seat != view.seat() && view.points_taken(seat) >= threshold)
}

/// How a seat defends against a suspected moon: when it starts, and whether
/// it keeps stoppers
#[derive(Debug, Clone, Copy)]
pub(crate) struct Defense {
    /// Sole-sweeper points at which the defense starts.
    pub trigger: u8,
    /// Whether to play [`stopper_defense`] rather than only take tricks
    /// from the sweeper.
    pub stoppers: bool,
}

/// [`suspect`] sign: a high heart led that no outstanding heart beats.
const SIGN_BOSS_HEART: u8 = 1;
/// [`suspect`] sign: the A♠ or K♠ led while the Q♠ is at large, by a seat
/// not known to hold her.
const SIGN_SPADE_HONOUR: u8 = 2;
/// [`suspect`] sign: a suit led in which every other seat is known void.
const SIGN_SOLO_SUIT: u8 = 4;
/// [`suspect`] sign: the Q♠ taken on the first spade round by a card played
/// after her.
const SIGN_QUEEN_TAKEN: u8 = 8;

/// The opponent whose play so far looks like a moon attempt, judged by the
/// enabled `signs` (a mask of the `SIGN_*` bits) before any points settle it
///
/// The latest sign wins, and a suspect stands only while no other seat has
/// taken a point: suspicion ends the moment the points are split, or land
/// on somebody else.  Every test is as the table stood when the card was
/// led — outstanding cards and shown voids are rebuilt trick by trick.
fn suspect(view: &View<'_>, signs: u8) -> Option<Seat> {
    if signs == 0 {
        return None;
    }
    let me = view.seat();
    // Never outstanding to this seat: its hand now and everything it played.
    let mut mine = view.hand();
    for trick in view.tricks().iter().copied().chain(view.current_trick()) {
        mine |= trick.card_from(me).map_or(Hand::EMPTY, Hand::from);
    }
    // The only way to know where the queen is: having passed her there.
    let queen_at = view
        .passed()
        .filter(|passed| passed.contains(Card::QUEEN_OF_SPADES))
        .map(|_| view.direction().receiver(me));

    let mut gone = Hand::EMPTY;
    let mut voids = [0u8; 4];
    let mut spades_led = false;
    let mut suspect = None;
    for trick in view.tricks().iter().copied().chain(view.current_trick()) {
        let Some(led) = trick.suit() else { break };
        let leader = trick.leader();
        let lead = trick.card_from(leader).expect("a led trick has a lead");
        let out = Holding::ALL - mine[led] - gone[led] - Holding::from_rank(lead.rank);
        let boss_heart = led == Suit::Hearts
            // "High" is read as an honour; lower boss hearts are endgames.
            && lead.rank >= Rank::J
            && (out - below(lead.rank)).is_empty();
        let spade_honour = led == Suit::Spades
            && lead.rank > Rank::Q
            // "Early" is read as while the queen is still at large.
            && !gone.contains(Card::QUEEN_OF_SPADES)
            && queen_at != Some(leader);
        let solo_suit = Seat::ALL
            .into_iter()
            .all(|seat| seat == leader || voids[seat as usize] & 1 << led as u8 != 0);
        if signs & SIGN_BOSS_HEART != 0 && boss_heart
            || signs & SIGN_SPADE_HONOUR != 0 && spade_honour
            || signs & SIGN_SOLO_SUIT != 0 && solo_suit
        {
            suspect = Some(leader);
        }
        // Deliberately: the winning card came down after the queen did.
        if signs & SIGN_QUEEN_TAKEN != 0
            && led == Suit::Spades
            && !spades_led
            && trick.is_complete()
            && let Some(winner) = trick.winner()
            && let Some(queen) = trick
                .plays()
                .position(|(_, card)| card == Card::QUEEN_OF_SPADES)
            && trick.plays().position(|(seat, _)| seat == winner) > Some(queen)
        {
            suspect = Some(winner);
        }
        spades_led |= led == Suit::Spades;
        for (seat, card) in trick.plays() {
            if card.suit != led {
                voids[seat as usize] |= 1 << led as u8;
            }
            gone.insert(card);
        }
    }
    suspect.filter(|&suspect| {
        suspect != me
            && Seat::ALL
                .into_iter()
                .all(|seat| seat == suspect || view.points_taken(seat) == 0)
    })
}

/// The knowledge-based moon-defense play, shared by [`HeuristicBot`] and the
/// [`MonteCarloBot`](crate::MonteCarloBot)'s live decision.
///
/// The threat is a single opponent who has swept at least
/// `defense.trigger` points, or failing that the [`suspect`] the enabled
/// `signs` point at.  Against it, stop ducking: beat them as cheaply as
/// possible when they are winning the trick (but never with our own Q♠ if
/// avoidable), and when we cannot beat them, at least gift them no penalty
/// card toward the sweep; with `defense.stoppers`, play
/// [`stopper_defense`] first.  `None` when no defense applies, so the
/// caller falls back to its normal policy.  The Monte Carlo
/// rollouts model opponents as greedy duckers who never shoot the moon, so
/// the search is blind to a moon in progress; this reactive overlay covers
/// the gap.
pub(crate) fn moon_defense(view: &View<'_>, defense: Defense, signs: u8) -> Option<Card> {
    let threat = moon_threat(view, defense.trigger).or_else(|| suspect(view, signs))?;
    let trick = view.current_trick()?;
    defend(
        view.hand(),
        view.legal_plays(),
        trick,
        view.played(),
        view.tricks(),
        threat,
        defense.stoppers,
    )
}

/// One defender's play against `shooter`, from its own cards and the public
/// record: [`stopper_defense`] when `stoppers` is set, else — or when that
/// has nothing to say — [`beat_threat`] on a trick the shooter is winning
fn defend(
    hand: Hand,
    legal: Hand,
    trick: Trick,
    played: Hand,
    tricks: &[Trick],
    shooter: Seat,
    stoppers: bool,
) -> Option<Card> {
    if stoppers && let Some(card) = stopper_defense(hand, legal, trick, played, tricks, shooter) {
        return Some(card);
    }
    if trick.winner() == Some(shooter) {
        beat_threat(legal, trick)
    } else {
        None
    }
}

/// The stopper-keeping defense against a live shot by `shooter`
///
/// A defender's cards have a second value while somebody may be shooting:
/// whether they still stop the shot.  In order:
///
/// 1. Take a point trick when the card that takes it is safe to play — a
///    heart no outstanding heart beats when leading, the cheapest winner
///    once the shooter has played, a card nothing outstanding beats
///    before — unless that means taking the Q♠.
/// 2. Holding the heart no outstanding heart beats, one stopper is as
///    certain as it gets: keep that card and play everything else by the
///    ordinary policy, which unloads the Q♠ and high hearts on the
///    shooter.
/// 3. On lead, a suit the shooter has shown out of, hearts first, lowest
///    card: the shooter cannot win the trick.
/// 4. Following with nothing that wins, in a stopper suit with a card to
///    spare, the second-highest goes and the top card survives.
/// 5. Discarding on a trick the shooter has already lost, a point card, to
///    split the points; otherwise a pointless card that breaks no stopper —
///    never past a stopper's spare length, and a heart only when forced,
///    the top one last.
///
/// `None` when none applies.  `hand` is the defender's whole hand (stopper
/// length counts cards it may not legally play now), `played` includes the
/// trick in progress, and `tricks` are the completed tricks.
fn stopper_defense(
    hand: Hand,
    legal: Hand,
    trick: Trick,
    played: Hand,
    tricks: &[Trick],
    shooter: Seat,
) -> Option<Card> {
    let out = |suit: Suit| Holding::ALL - hand[suit] - played[suit];
    let boss_hearts = top_run(hand[Suit::Hearts], out(Suit::Hearts));
    // The certain stopper, and the ordinary policy over everything else.
    let free = || {
        let rank = boss_hearts.iter().next_back()?;
        let rest = legal
            - Hand::from(Card {
                suit: Suit::Hearts,
                rank,
            });
        (!rest.is_empty()).then(|| greedy_play(rest, trick, played))
    };

    let Some(led) = trick.suit() else {
        // My own heart is a point: a trick it must win ends the shot.
        if let Some(rank) = (legal[Suit::Hearts] & boss_hearts).iter().next() {
            return Some(Card {
                suit: Suit::Hearts,
                rank,
            });
        }
        // The spec's other lead, a low card some other opponent is forced
        // to win, is the same lead: from legal knowledge another seat is
        // *forced* to win only when the shooter cannot follow.
        let shown_out = |suit: Suit| {
            tricks.iter().any(|past| {
                past.suit() == Some(suit)
                    && past
                        .card_from(shooter)
                        .is_some_and(|card| card.suit != suit)
            })
        };
        return [Suit::Hearts, Suit::Clubs, Suit::Diamonds, Suit::Spades]
            .into_iter()
            .filter(|&suit| shown_out(suit))
            .find_map(|suit| {
                (legal - Hand::QUEEN_OF_SPADES)[suit]
                    .iter()
                    .next()
                    .map(|rank| Card { suit, rank })
            })
            .or_else(free);
    };

    let shooter_played = trick.card_from(shooter).is_some();
    let follow = legal[led];
    if follow.is_empty() {
        if shooter_played && trick.winner() != Some(shooter) {
            // Another opponent takes this trick whatever follows.
            let points = legal & Hand::PENALTIES;
            return (!points.is_empty()).then(|| best_dump(points, played));
        }
        if let Some(card) = free() {
            return Some(card);
        }
        let harmless = legal - Hand::PENALTIES;
        let slack = |card: &Card| spare(hand[card.suit], out(card.suit));
        let loose = harmless
            .into_iter()
            .filter(|card| slack(card).is_none())
            .max_by_key(|card| card.rank);
        let spare_low = || {
            harmless
                .into_iter()
                .filter(|card| slack(card).is_some_and(|spare| spare > 0))
                .min_by_key(|card| card.rank)
        };
        // Past here something has to give: a stopper's length, then a low
        // heart, then whatever is left.
        let top_heart = legal[Suit::Hearts].iter().next_back();
        let low_heart = || {
            legal[Suit::Hearts]
                .iter()
                .find(|&rank| Some(rank) != top_heart)
                .map(|rank| Card {
                    suit: Suit::Hearts,
                    rank,
                })
        };
        return loose
            .or_else(spare_low)
            .or_else(|| harmless.into_iter().min_by_key(|card| card.rank))
            .or_else(low_heart)
            .or_else(|| (legal - Hand::HEARTS).into_iter().next())
            .or_else(|| legal.into_iter().next());
    }

    let winner = winning_card(trick).expect("a led trick has a winning card");
    let queen = Card::QUEEN_OF_SPADES;
    let mut beat = follow - below(winner.rank) - Holding::from_rank(winner.rank);
    if led == Suit::Spades {
        beat -= Holding::from_rank(Rank::Q);
    }
    let pointed = led == Suit::Hearts || trick.points() > 0;
    // She is in the trick already, or may still land on a spade trick.
    let queen_risk = trick.cards().contains(queen)
        || led == Suit::Spades
            && trick.len() < 3
            && !played.contains(queen)
            && !hand.contains(queen);
    if pointed && !queen_risk {
        let take = if shooter_played {
            beat
        } else {
            top_run(beat, out(led))
        };
        if let Some(rank) = take.iter().next() {
            return Some(Card { suit: led, rank });
        }
    }
    if let Some(card) = free() {
        return Some(card);
    }
    // Nothing here wins, so every card is a duck: keep the top one.
    let mut tops = follow.iter().rev();
    match (tops.next(), tops.next()) {
        (Some(top), Some(second))
            if top < winner.rank && spare(hand[led], out(led)).is_some_and(|spare| spare > 0) =>
        {
            Some(Card {
                suit: led,
                rank: second,
            })
        }
        _ => None,
    }
}

/// Take this trick away from the seat currently winning it
///
/// The knowledge-free core of [`moon_defense`], shared with the rollout
/// policy: beat the winner as cheaply as possible but never with our own
/// Q♠, and when we cannot beat them, gift them no penalty card toward the
/// sweep.  `None` when the trick has no led suit or every legal play would
/// feed the sweep, so the caller falls back to its normal policy.
fn beat_threat(legal: Hand, trick: Trick) -> Option<Card> {
    let led = trick.suit()?;
    let winner = winning_card(trick).expect("a led trick has a winning card");
    let follow = legal[led];
    if follow.is_empty() {
        // Void: dump the most dangerous card that carries no points into the
        // sweeper's trick.
        let harmless = legal - Hand::PENALTIES;
        return harmless.into_iter().max_by_key(|card| card.rank);
    }
    // Beat them as cheaply as possible — taking a trick with our own Q♠
    // costs more than the sweep it denies.
    let mut beat = follow - below(winner.rank) - Holding::from_rank(winner.rank);
    if led == Suit::Spades {
        beat -= Holding::from_rank(Rank::Q);
    }
    if let Some(rank) = beat.iter().next() {
        return Some(Card { suit: led, rank });
    }
    // Can't beat the sweep: duck plainly — and never ride the dead queen
    // into the sweeper's trick.
    let mut duck = follow & below(winner.rank);
    if led == Suit::Spades {
        let harmless = duck - Holding::from_rank(Rank::Q);
        if !harmless.is_empty() {
            duck = harmless;
        }
    }
    duck.iter().next_back().map(|rank| Card { suit: led, rank })
}

impl Strategy for HeuristicBot {
    fn pass_cards(&mut self, view: &View<'_>) -> [Card; 3] {
        greedy_pass(view.hand(), self.config)
    }

    fn play_card(&mut self, view: &View<'_>) -> Card {
        let defense = Defense {
            trigger: self.config.moon_defense,
            stoppers: false,
        };
        if let Some(card) = moon_defense(view, defense, 0) {
            return card;
        }
        let legal = view.legal_plays();
        let trick = view.current_trick().expect("a play decision has a trick");
        greedy_play(legal, trick, view.played())
    }

    fn name(&self) -> &str {
        "greedy"
    }
}

/// The knowledge-free moon-seeking policy, the mirror of [`greedy_play`]
///
/// A shot has to win every trick left, so: cash our highest card when
/// leading, top the led suit when that beats the current winner, and shed
/// our cheapest junk when void — the hearts and the queen are cards we mean
/// to *win*, not discard.  On a trick we can no longer take, the shot needs
/// nothing from us and [`greedy_play`] keeps the stopper it would have
/// wasted.
#[cfg(any(feature = "rand", test))]
pub(crate) fn shoot_play(legal: Hand, trick: Trick, played: Hand) -> Card {
    let highest = |cards: Hand| {
        cards
            .into_iter()
            .max_by_key(|card| card.rank)
            .expect("the set was checked non-empty")
    };

    let Some(led) = trick.suit() else {
        return highest(legal);
    };

    let follow = legal[led];
    if follow.is_empty() {
        // Void, so this trick is lost: shed the cheapest card that is not a
        // penalty we still want to take ourselves.
        let junk = legal - Hand::PENALTIES;
        let shed = if junk.is_empty() { legal } else { junk };
        return shed
            .into_iter()
            .min_by_key(|card| card.rank)
            .expect("legal plays are never empty on turn");
    }

    let winner = winning_card(trick).expect("a led trick has a winning card");
    match (follow - below(winner.rank) - Holding::from_rank(winner.rank))
        .iter()
        .next_back()
    {
        Some(rank) => Card { suit: led, rank },
        None => greedy_play(legal, trick, played),
    }
}

/// The rollout flavor of the policy, for omniscient Monte Carlo worlds
///
/// `shooter`, when set, is the seat whose moon the world is testing: it
/// plays [`shoot_play`] for as long as the shot is alive, and the other
/// three stop ducking once the shooter holds `defense.trigger` points (the
/// live table's eight by default) and is winning the trick — the same
/// trigger [`HeuristicBot`]'s moon-defense overlay fires on — or, with
/// `defense.stoppers`, play [`stopper_defense`] from that point on.
/// Defending from trick 1 prices every
/// shot against a table no real opponent fields, while greedy duckers
/// with no trigger at all would make every moon look like a laydown.
///
/// `played` must equal `round.played()`; the caller threads it so a
/// rollout does not refold the whole trick history at every play.
#[cfg(feature = "rand")]
pub(crate) fn rollout_play(
    round: &hearts::Round,
    seat: Seat,
    shooter: Option<Seat>,
    played: Hand,
    defense: Defense,
) -> Card {
    let legal = round.legal_plays(seat);
    let trick = round.current_trick().expect("a playing round has a trick");

    if let Some(shooter) = shooter {
        // A shot dies the moment anyone else scores; from there the shooter
        // bails out to the greedy policy, as a real one would.
        let alive = Seat::ALL
            .into_iter()
            .all(|other| other == shooter || round.points_taken(other) == 0);
        if !alive {
            return greedy_play(legal, trick, played);
        }
        if seat == shooter {
            return shoot_play(legal, trick, played);
        }
        // The cheap test first: most plays are not on the shooter's trick.
        if (defense.stoppers || trick.winner() == Some(shooter))
            && round.points_taken(shooter) >= defense.trigger
            && let Some(card) = defend(
                round.hand(seat),
                legal,
                trick,
                played,
                round.tricks(),
                shooter,
                defense.stoppers,
            )
        {
            return card;
        }
    }

    greedy_play(legal, trick, played)
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

    /// The policy this module replaced: one flat sort of the dealt thirteen,
    /// ties falling to whichever suit `Hand`'s iterator reached first.
    fn flat_pass(dealt: Hand, config: HeuristicConfig) -> [Card; 3] {
        let mut cards: Vec<Card> = dealt.into_iter().collect();
        cards.sort_by_key(|&card| -pass_score(dealt, card, config));
        [cards[0], cards[1], cards[2]]
    }

    #[test]
    fn passing_dumps_the_unprotected_queen() {
        // Q♠ with one guard, the A♥, and filler.
        let dealt = hand("234567.89.A.2Q");
        let picks = greedy_pass(dealt, HeuristicConfig::default());
        assert!(picks.contains(&card("Q♠")));
        assert!(picks.contains(&card("A♥")));

        // Well guarded, the queen stays; the hearts and short suits go.
        let guarded = hand("234.8.9A.234567Q");
        let picks = greedy_pass(guarded, HeuristicConfig::default());
        assert!(!picks.contains(&card("Q♠")));
    }

    #[test]
    fn passing_completes_a_void_instead_of_scattering() {
        // ♠J86 ♥A6 ♦K2 ♣976432.  Scored flat against the dealt thirteen the
        // order is A♥ 18, K♦ 17, J♠ 11, 6♥ 10, so one sort passes A♥ K♦ J♠
        // and leaves both red doubletons alive — three void bonuses
        // collected, no void created.  Sequentially the A♥ leaves the 6♥ a
        // singleton worth 6 + 6 = 12, which beats the J♠: the pass opens a
        // real heart void and keeps the 2♦ as a clean duck.
        // Queen insurance would demote the J♠ and blur the contrast.
        let dealt = hand("234679.2K.6A.68J");
        let config = HeuristicConfig {
            queen_insurance: 0,
            ..HeuristicConfig::default()
        };
        assert_eq!(
            greedy_pass(dealt, config),
            [card("A♥"), card("K♦"), card("6♥")],
        );

        assert_eq!(
            flat_pass(dealt, config),
            [card("A♥"), card("K♦"), card("J♠")],
            "the flat form scatters, which is what this replaced",
        );
    }

    #[test]
    fn queen_insurance_keeps_a_low_spade_without_her() {
        let dealt = hand("2345.2345.234.29");
        let uninsured = HeuristicConfig {
            queen_insurance: 0,
            ..HeuristicConfig::default()
        };
        assert!(greedy_pass(dealt, uninsured).contains(&card("9♠")));
        assert!(!greedy_pass(dealt, HeuristicConfig::default()).contains(&card("9♠")));

        // Holding her, a low spade scores bare rank.
        let config = HeuristicConfig::default();
        assert_eq!(pass_score(hand("2345.2345.23.29Q"), card("9♠"), config), 9);
        assert_eq!(pass_score(dealt, card("9♠"), config), 5);
    }

    #[test]
    fn the_pass_tie_break_is_designed_not_inherited() {
        // The ladder: the queen and the two cards that catch her first, then
        // hearts, diamonds, clubs, and a spade below the queen dead last
        // because it is a guard worth keeping.
        assert!(danger(card("Q♠")) > danger(card("A♥")));
        assert!(danger(card("A♥")) > danger(card("A♦")));
        assert!(danger(card("A♦")) > danger(card("A♣")));
        assert!(danger(card("A♣")) > danger(card("J♠")));

        // And the ladder reaching the policy.  Four nines score exactly 9
        // apiece — a four-card suit earns no void bonus and `heart_weight`
        // is zero — so nothing but the tie-break decides.  Stable-sort order
        // used to send the clubs first; the 9♠ is a guard and never goes.
        let nines = hand("2349.2349.2349.9");
        assert_eq!(
            greedy_pass(nines, HeuristicConfig::default()),
            [card("9♥"), card("9♦"), card("9♣")],
        );
        assert_eq!(
            flat_pass(nines, HeuristicConfig::default()),
            [card("9♣"), card("9♦"), card("9♥")],
            "sort stability used to reach for the clubs first",
        );

        // With five guards the Q♠/A♠/K♠ tier is disarmed, so a bare K♠ and a
        // K♣ both score 13 and the danger class is the whole difference: the
        // spade honor goes, the club stays.
        assert_eq!(
            greedy_pass(hand("234K..23K.23456K"), HeuristicConfig::default()),
            [card("K♥"), card("K♠"), card("K♣")],
        );
    }

    #[test]
    fn the_forced_lead_is_worth_passing() {
        // Four-card clubs, so the 2♣ takes no void bonus and the knob is the
        // whole difference; the red three-card fragments are the rivals.
        let dealt = hand("2345.234.234.234");
        assert_eq!(
            greedy_pass(dealt, HeuristicConfig::default()),
            [card("2♣"), card("5♣"), card("4♣")],
            "the shipped bonus sends the forced lead, then works the suit",
        );

        // Priced at bare rank 2 it is the last card in the deck to go, and
        // the pass falls back to emptying hearts.
        let unpriced = HeuristicConfig {
            two_of_clubs_bonus: 0,
            ..HeuristicConfig::default()
        };
        assert_eq!(
            greedy_pass(dealt, unpriced),
            [card("4♥"), card("3♥"), card("2♥")],
        );
    }

    #[test]
    fn shooting_takes_every_trick_it_can() {
        // Leading: cash the highest card in hand.
        let lead = Trick::new(Seat::North);
        assert_eq!(shoot_play(hand("2.3.A.4"), lead, Hand::EMPTY), card("A♥"));

        // Following a trick we can win: top the led suit, not duck under it.
        let mut trick = Trick::new(Seat::North);
        trick.play(card("9♦")).unwrap();
        let held = hand(".2378Q..");
        assert_eq!(shoot_play(held, trick, Hand::EMPTY), card("Q♦"));
        assert_eq!(
            greedy_play(held, trick, Hand::EMPTY),
            card("8♦"),
            "the greedy policy ducks the same holding"
        );
    }

    #[test]
    fn a_dead_trick_costs_the_shooter_nothing() {
        // The nine is out of reach in diamonds, so the shot needs nothing
        // here: fall back to the greedy policy rather than burn a stopper.
        let mut trick = Trick::new(Seat::North);
        trick.play(card("9♦")).unwrap();
        let held = hand(".2378..A");
        assert_eq!(
            shoot_play(held, trick, Hand::EMPTY),
            greedy_play(held, trick, Hand::EMPTY)
        );

        // Void: shed the cheapest junk and keep the penalties to win later.
        assert_eq!(shoot_play(hand("29..3A.Q"), trick, Hand::EMPTY), card("2♣"));
        // Nothing but penalties: the lowest of them goes.
        assert_eq!(shoot_play(hand("..3A.Q"), trick, Hand::EMPTY), card("3♥"));
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
    fn last_hand_takes_only_a_point_free_trick() {
        let mut clean = Trick::new(Seat::North);
        for text in ["5♦", "2♦", "3♦"] {
            clean.play(card(text)).unwrap();
        }
        assert_eq!(
            greedy_play(hand(".47.."), clean, Hand::EMPTY),
            card("7♦"),
            "take control when nobody can add points"
        );

        let mut costly = Trick::new(Seat::North);
        for text in ["5♣", "2♥", "3♣"] {
            costly.play(card(text)).unwrap();
        }
        assert_eq!(
            greedy_play(hand("47..."), costly, Hand::EMPTY),
            card("4♣"),
            "duck when the trick already carries a point"
        );
    }

    #[test]
    fn leading_works_the_shortest_suit() {
        let trick = Trick::new(Seat::North);
        let played = Hand::QUEEN_OF_SPADES;
        assert_eq!(greedy_play(hand("234.5.67.89"), trick, played), card("5♦"));
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
    fn the_loser_aware_pass_reads_the_bottom_of_the_suit() {
        let aware = HeuristicConfig {
            loser_weight: 2,
            ..HeuristicConfig::default()
        };
        let key = |dealt: &str, text: &str| pass_key(hand(dealt), card(text), aware);

        // The spec's example: bare club honours go before diamond honours
        // that sit over two ducks.
        let dealt = "JKA.23KA.2345.23";
        assert!(key(dealt, "J♣") > key(dealt, "A♦"));
        assert_eq!(
            greedy_pass(hand(dealt), aware),
            [card("A♣"), card("K♣"), card("J♣")]
        );

        // An ace over three small cards is nearly safe: kept, where the
        // shipped pass sends it first.
        let covered = hand("6789.234A.2345.T");
        assert_eq!(
            greedy_pass(covered, HeuristicConfig::default())[0],
            card("A♦")
        );
        assert!(!greedy_pass(covered, aware).contains(&card("A♦")));

        // Long hearts keep their guards, and give the second-highest
        // before the top one; short hearts are exposed like any suit.
        let long = "2345.2345.2QKA.7";
        assert!(key(long, "K♥") > key(long, "A♥"));
        let short = "23456.2345.QKA.7";
        assert!(key(short, "A♥") > key(short, "K♥"));
        assert!(key(short, "K♥").0 > 13 && 13 > key(long, "K♥").0);
        assert!(key(short, "A♥") > key(short, "6♣"));

        // A club void is worth less than a diamond void of the same
        // length: the shipped pass empties the clubs, this one the diamonds.
        let twins = hand("9T.89.23456.2345");
        assert_eq!(
            greedy_pass(twins, HeuristicConfig::default())[0],
            card("T♣")
        );
        assert_eq!(greedy_pass(twins, aware)[0], card("9♦"));
    }

    /// A trick led by `leader` with `cards` played in turn.
    fn trick(leader: Seat, cards: &[&str]) -> Trick {
        let mut trick = Trick::new(leader);
        for text in cards {
            trick.play(card(text)).unwrap();
        }
        trick
    }

    #[test]
    fn a_defender_keeps_what_stops_the_shot() {
        // North is shooting throughout; `played` is the trick in progress.
        let defend = |held: &str, trick: Trick, tricks: &[Trick]| {
            let held = hand(held);
            stopper_defense(held, held, trick, trick.cards(), tricks, Seat::North)
        };

        // Following under the shooter's ace: the second-highest goes and
        // the king survives.  The ordinary policy ducks with the king.
        let ace = trick(Seat::North, &["A♦"]);
        assert_eq!(defend(".27K..", ace, &[]), Some(card("7♦")));
        assert_eq!(
            greedy_play(hand(".27K.."), ace, ace.cards()),
            card("K♦"),
            "the ordinary policy ducks the stopper away"
        );

        // Discarding on the shooter's trick: not the heart, and not the
        // K♦ 7♦ stopper, which has no card to spare under the ace.
        let club = trick(Seat::North, &["A♣"]);
        assert_eq!(defend(".7K.4.59", club, &[]), Some(card("9♠")));
        // With the 2♦ as well there is one to spare, and it is the low one.
        assert_eq!(defend(".27K.4.", club, &[]), Some(card("2♦")));
        // Nothing but hearts: a low one, never the last or the top.
        assert_eq!(defend("..48.", club, &[]), Some(card("4♥")));

        // The shooter has lost this trick: split the points.
        let lost = trick(Seat::North, &["5♣", "9♣"]);
        assert_eq!(defend(".7K.4.59", lost, &[]), Some(card("4♥")));

        // A point trick the shooter has yet to play to: take it only with
        // a card nothing outstanding beats.
        let heart = trick(Seat::East, &["5♥"]);
        assert_eq!(defend("..3A.", heart, &[]), Some(card("A♥")));
        assert_eq!(defend("..3K.", heart, &[]), None);
        // ...and never when it means taking the Q♠.
        let queen = trick(Seat::East, &["5♥", "Q♠"]);
        assert_ne!(defend("..3A.", queen, &[]), Some(card("A♥")));

        // One certain stopper frees the rest: the queen goes on the
        // shooter, the boss heart stays.
        assert_eq!(defend("..2A.Q", club, &[]), Some(card("Q♠")));

        // On lead: the boss heart if hearts may be led, else a suit the
        // shooter has shown out of.
        let lead = Trick::new(Seat::South);
        assert_eq!(defend("9.38.2A.", lead, &[]), Some(card("A♥")));
        let shown = [trick(Seat::East, &["5♦", "6♦", "7♦", "2♣"])];
        assert_eq!(defend("9.38..4", lead, &shown), Some(card("3♦")));
        assert_eq!(defend("9.38..4", lead, &[]), None);
    }

    #[cfg(feature = "rand")]
    #[test]
    fn rollout_defenders_keep_stoppers_only_when_told() {
        use hearts::{PassDirection, Round, Rules};

        // North, the shooter, leads the 2♣ and East tops it.  South's king
        // cannot win: the stopper-keeper follows with the seven.
        let hands = [
            hand("2.23456789TJQK.."),
            hand("A...23456789TJQK"),
            hand("37K..23456789TJ."),
            hand("45689TJQ.A.QKA.A"),
        ];
        let mut round = Round::from_deal(Rules::new(), PassDirection::Hold, hands).unwrap();
        round.play(Seat::North, card("2♣")).unwrap();
        round.play(Seat::East, card("A♣")).unwrap();
        let play = |stoppers| {
            let defense = Defense {
                trigger: 0,
                stoppers,
            };
            rollout_play(
                &round,
                Seat::South,
                Some(Seat::North),
                round.played(),
                defense,
            )
        };
        assert_eq!(play(true), card("7♣"));
        assert_eq!(play(false), card("K♣"));
    }

    #[test]
    fn a_spade_honour_led_early_draws_suspicion() {
        use crate::Table;
        use hearts::{PassDirection, Round, Rules};

        let hands = [
            hand("2345678.234567.."),
            hand("A...23456789TJKA"),
            hand("9TJQK.89TJQKA..Q"),
            hand("..23456789TJQKA."),
        ];
        let mut round = Round::from_deal(Rules::new(), PassDirection::Hold, hands).unwrap();
        for (seat, text) in [
            (Seat::North, "2♣"),
            (Seat::East, "A♣"),
            (Seat::South, "9♣"),
            (Seat::West, "2♥"),
        ] {
            round.play(seat, card(text)).unwrap();
        }
        let before = Table::new(round.clone());
        assert_eq!(suspect(&before.view(Seat::South), u8::MAX), None);

        // East cashes the A♠ with the queen still out.
        round.play(Seat::East, card("A♠")).unwrap();
        let table = Table::new(round.clone());
        let view = table.view(Seat::South);
        assert_eq!(suspect(&view, SIGN_SPADE_HONOUR), Some(Seat::East));
        assert_eq!(suspect(&view, !SIGN_SPADE_HONOUR), None);
        assert_eq!(suspect(&table.view(Seat::East), u8::MAX), None);

        // It is enough to start the defense eight points early: South
        // would rather not feed the queen to the trick East is winning...
        let defense = Defense {
            trigger: 8,
            stoppers: false,
        };
        assert_eq!(moon_defense(&view, defense, 0), None);
        // ...though here she is South's only spade, so the forced card
        // comes back either way.
        assert_eq!(
            moon_defense(&view, defense, SIGN_SPADE_HONOUR),
            Some(card("Q♠"))
        );
    }

    /// What the two pass-side knobs do to a random hand: how many A/K/Q the
    /// pass carries at each loser weight, and how often the shape gate
    /// opens at each allowance.  The numbers behind the A/B values.
    ///
    /// ```console
    /// cargo test --release --lib pass_census -- --ignored --nocapture
    /// ```
    #[cfg(feature = "rand")]
    #[test]
    #[ignore = "pass census: 200,000 hands, run on demand"]
    fn pass_census() {
        use crate::shape::{shooter_pass, shot_losers};
        use hearts::{PassDirection, Round, Rules};
        use rand::SeedableRng as _;

        const HANDS: u32 = 200_000;
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x261);
        let weights = [0u8, 1, 2, 3, 4, 6];
        let mut honours = [0u32; 6];
        let mut opens = [0u32; 6];
        for _ in 0..HANDS / 4 {
            let round = Round::deal(Rules::new(), PassDirection::Left, &mut rng);
            for dealt in Seat::ALL.map(|seat| round.hand(seat)) {
                for (count, loser_weight) in honours.iter_mut().zip(weights) {
                    let config = HeuristicConfig {
                        loser_weight,
                        ..HeuristicConfig::default()
                    };
                    *count += greedy_pass(dealt, config)
                        .into_iter()
                        .filter(|card| card.rank >= Rank::Q)
                        .count() as u32;
                }
                let pass: Hand = shooter_pass(dealt).into_iter().collect();
                if let Some(losers) = shot_losers(dealt - pass) {
                    for open in opens.iter_mut().skip(losers) {
                        *open += 1;
                    }
                }
            }
        }
        let per = |count: u32| f64::from(count) / f64::from(HANDS);
        for (weight, count) in weights.into_iter().zip(honours) {
            println!("loser={weight}: {:.3} A/K/Q a pass", per(count));
        }
        for (allowed, open) in opens.into_iter().enumerate() {
            println!("shape={allowed}: gate opens on {:.4} of hands", per(open));
        }
    }

    #[test]
    fn smoke_out_the_queen() {
        let trick = Trick::new(Seat::North);
        let played = hand("2345...");
        // Leading without the queen: low spades hunt it.
        assert_eq!(greedy_play(hand("9.9.9.29"), trick, played), card("2♠"));
        // Holding the queen: no self-hunt, so work a shortest side suit.
        assert_eq!(greedy_play(hand("9.9..29Q"), trick, played), card("9♣"));
        // Holding the queen and higher clubs: lead the lowest card, no hunt.
        assert_eq!(greedy_play(hand("4.9..9Q"), trick, played), card("4♣"));
        // Holding a high spade: keep its guards, work a shortest side suit.
        assert_eq!(greedy_play(hand("9.9.9.29K"), trick, played), card("9♣"));
        assert_eq!(greedy_play(hand("9.9.9.2A"), trick, played), card("9♣"));
    }
}
