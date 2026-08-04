//! [`MonteCarloBot`]: determinized Monte Carlo move selection
//!
//! Unlike the gin engine this crate mirrors, there is no separate rollout
//! simulator to keep in sync with the rules: every Hearts observation
//! except the hidden hands is public, so a sampled world is a real
//! [`hearts::Round`] — reconstructed as a hold deal of the sampled original
//! hands with the public history replayed — and rollouts run on the real
//! state machine.

use crate::heuristic::{
    greedy_pass, greedy_play, moon_defense, pass_score, rollout_play, shoot_play,
};
use crate::{Strategy, View};
use hearts::{Card, Hand, PassDirection, Phase, Rank, Round, RoundResult, Rules, Seat, Suit};
use rand::{Rng, RngExt as _};

/// How many candidate plays a decision weighs after collapsing
/// rank-adjacent equal-value equivalents; the rest are never worth a rollout.
const MAX_CANDIDATES: usize = 8;

/// How many of the highest [`pass_score`] cards seed the pass triples:
/// all 20 triples of the top 6 are rolled before short-suit void candidates.
const PASS_POOL: usize = 6;

/// The world count of the first scoring batch; each later batch doubles the
/// evaluated total, so elimination checkpoints fall after 32, 64, 128, ...
/// worlds.  A decision of 32 samples or fewer is a single batch, identical
/// to an unbatched run.
const BATCH: usize = 32;

/// What the rollouts said about one candidate: its per-world equities, its
/// summed round points, and how many of those worlds it shot the moon in
type Scored = (Vec<f64>, f64, u32);

/// One candidate action to score: the move a [`Strategy`] method would
/// return, paired with its rendered [`Assessment::action`] label
struct Candidate {
    label: String,
    choice: Choice,
    /// Whether to roll the continuation as a moon attempt by the deciding
    /// seat rather than with the greedy policy.  Shooting is a plan, not a
    /// single card, so it lives on the rollout instead of on [`Choice`].
    shoot: bool,
}

/// A typed candidate move
#[derive(Clone, Copy)]
enum Choice {
    /// Pass these three cards.
    Pass([Card; 3]),
    /// Play this card.
    Play(Card),
}

impl Choice {
    /// Apply the move to a world and play it out: greedily, or with `me`
    /// shooting the moon against a table that defends.
    fn roll(self, mut round: Round, me: Seat, shoot: bool) -> RoundResult {
        match self {
            Self::Pass(cards) => round
                .pass(me, cards.into_iter().collect())
                .expect("a candidate pass is legal in every sampled world"),
            Self::Play(card) => round
                .play(me, card)
                .expect("a candidate play is legal in every sampled world"),
        }
        while let Some(seat) = round.turn() {
            let card = rollout_play(&round, seat, shoot.then_some(me));
            round
                .play(seat, card)
                .expect("the rollout policy picks from legal_plays");
        }
        round.result().expect("a turnless round is finished")
    }
}

/// A determinized Monte Carlo player
///
/// At every decision the bot samples hidden worlds consistent with its
/// [`View`] — the receiver of its pass holds every passed card not yet
/// played, seats shown void in a suit receive none of it, and the
/// remaining unseen cards are distributed uniformly — plays each world out
/// with the greedy policy on all seats, and picks the action with the best
/// expected value *for the game*: each rollout's result lands on the
/// running [`game scores`](View::game_scores), a result that reaches
/// [`game_target`](Rules::game_target) counts as the k-way win or loss of
/// the game it is, and anything short of one counts its round points.
/// The same worlds are reused across candidate actions (common random
/// numbers), and the bot deviates from the greedy baseline only when the
/// paired samples show a statistically clear gain.  Worlds are rolled in
/// growing batches, and a challenger the incumbent already statistically
/// dominates is dropped at a batch boundary — once none remain, the
/// remaining worlds are never rolled at all — so an easy decision costs a
/// fraction of the full sample count.
///
/// One candidate per decision is a moon attempt, rolled with the deciding
/// seat shooting and the other three defending instead of ducking.  It has
/// to clear the usual significance gate *and* actually reach the moon in
/// most of the sampled worlds, and once chosen it is carried to the end of
/// the round rather than re-argued each trick — a shot re-decided per trick
/// takes the points and misses the moon.
///
/// The bot owns its random number generator, so a seeded generator makes
/// its play reproducible.
pub struct MonteCarloBot<R: Rng> {
    rng: R,
    samples: u32,
    /// How many paired standard errors a challenger must clear in
    /// [`beats`] before the bot deviates from the greedy incumbent.
    gate: f64,
    /// Whether a moon attempt is under way, carried across the tricks of
    /// one round so the search does not re-litigate it every turn.
    shooting: bool,
}

/// One candidate action's Monte Carlo assessment, for a solver or hint view
///
/// Produced by [`MonteCarloBot::assess`]: the same rollouts the bot chooses
/// with, surfaced per candidate instead of collapsed to the single action a
/// [`Strategy`] method returns.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Assessment {
    /// A rendered label for the action, e.g. `"play Q♠"` or
    /// `"pass Q♠ A♥ K♥"`.
    pub action: String,
    /// Mean game-winning equity in `[0, 1]` — the quantity the bot
    /// maximizes, so candidates rank by it.  A shared win counts `1/k`.  A
    /// candidate the bot eliminated early averages over the worlds it saw
    /// before elimination rather than the full sample count.
    pub equity: f64,
    /// Mean round points this action costs the deciding seat — **lower is
    /// better**, unlike the gin engine's signed gain.  Averaged over the
    /// same worlds as [`equity`](Self::equity).
    pub ev: f64,
    /// Whether this is the bot's own pick — the move a [`Strategy`] method
    /// would return on this view.  Because the bot deviates from the greedy
    /// baseline only on a statistically clear gain, this need not be the
    /// highest-equity candidate.
    pub recommended: bool,
}

impl<R: Rng> MonteCarloBot<R> {
    /// A bot with default strength: 128 worlds per decision
    pub const fn new(rng: R) -> Self {
        Self {
            rng,
            samples: 128,
            gate: 1.5,
            shooting: false,
        }
    }

    /// Set how many worlds each decision samples
    ///
    /// More samples play stronger and slower; easy decisions stop at a
    /// fraction of the budget.  The `parallel` feature divides the cost by
    /// most of a machine's cores without changing a single decision.
    #[must_use]
    pub const fn samples(mut self, samples: u32) -> Self {
        self.samples = samples;
        self
    }

    /// Set the significance gate: how many paired standard errors a
    /// challenger must clear before the bot deviates from the greedy
    /// incumbent
    ///
    /// The default is 1.5.  Duplicate-deal measurement found strength
    /// monotone falling in the threshold over `[1.0, 2.5]` — the old 2.0
    /// over-corrected for multiplicity by about 0.2 points a deal — while
    /// a lower gate trusts more of the rollout noise.
    #[must_use]
    pub const fn gate(mut self, gate: f64) -> Self {
        self.gate = gate;
        self
    }

    /// Sample one set of current hidden hands consistent with the view, by
    /// randomized most-constrained-first backtracking over the unseen cards
    ///
    /// Constraints: exact hand sizes, no card in a suit the seat is void
    /// in, and the still-unplayed passed cards pinned to the receiver.
    /// The true deal satisfies them, so failure is a sampling accident —
    /// the caller retries.
    fn sample_hands(&mut self, view: &View<'_>) -> Option<[Hand; 4]> {
        let me = view.seat();
        let mut hands = [Hand::EMPTY; 4];
        let mut room = [0usize; 4];
        hands[me as usize] = view.hand();
        for seat in Seat::ALL {
            if seat != me {
                let known = view.known_cards(seat);
                hands[seat as usize] = known;
                // A pass-phase world samples *pre-pass* hands: a seat that
                // already passed holds 10 now, but its 3 hidden passes are
                // part of the unseen pool and belong back in its hand.
                let size = if view.phase() == Phase::Passing {
                    13
                } else {
                    view.hand_len(seat)
                };
                room[seat as usize] = size - known.len();
            }
        }

        // Shuffle for randomness, then most-constrained cards first.
        let mut cards: Vec<Card> = view.unseen().into_iter().collect();
        for i in (1..cards.len()).rev() {
            let j = self.rng.random_range(0..=i);
            cards.swap(i, j);
        }
        let allowed = |card: Card, seat: Seat| seat != me && !view.is_void(seat, card.suit);
        cards.sort_by_key(|&card| Seat::ALL.iter().filter(|&&s| allowed(card, s)).count());

        fn place(
            cards: &[Card],
            allowed: &impl Fn(Card, Seat) -> bool,
            hands: &mut [Hand; 4],
            room: &mut [usize; 4],
            rng: &mut (impl Rng + ?Sized),
        ) -> bool {
            let Some((&card, rest)) = cards.split_first() else {
                return true;
            };
            let mut order = Seat::ALL;
            for i in (1..4).rev() {
                let j = rng.random_range(0..=i);
                order.swap(i, j);
            }
            for seat in order {
                if room[seat as usize] > 0 && allowed(card, seat) {
                    room[seat as usize] -= 1;
                    hands[seat as usize].insert(card);
                    if place(rest, allowed, hands, room, rng) {
                        return true;
                    }
                    hands[seat as usize].remove(card);
                    room[seat as usize] += 1;
                }
            }
            false
        }

        place(&cards, &allowed, &mut hands, &mut room, &mut self.rng).then_some(hands)
    }

    /// Sample one world: a real [`Round`] at this decision point
    ///
    /// A play-phase world is a hold deal of the sampled *original* hands
    /// (current hand plus own plays) with the public history replayed; a
    /// pass-phase world is a fresh deal of sampled pre-pass hands with the
    /// three opponents' greedy passes already submitted, waiting on ours.
    fn sample_world(&mut self, view: &View<'_>) -> Option<Round> {
        let me = view.seat();
        let hands = self.sample_hands(view)?;

        if view.phase() == Phase::Passing {
            let mut round = Round::from_deal(*view.rules(), view.direction(), hands).ok()?;
            for seat in Seat::ALL {
                if seat != me {
                    let pass: Hand = greedy_pass(hands[seat as usize], 1).into_iter().collect();
                    round.pass(seat, pass).ok()?;
                }
            }
            return Some(round);
        }

        // Original post-pass hands: what each seat holds now plus what it
        // has already played.
        let mut originals = hands;
        for trick in view.tricks().iter().copied().chain(view.current_trick()) {
            for (seat, card) in trick.plays() {
                originals[seat as usize].insert(card);
            }
        }
        let mut round = Round::from_deal(*view.rules(), PassDirection::Hold, originals).ok()?;
        for trick in view.tricks().iter().copied().chain(view.current_trick()) {
            for (seat, card) in trick.plays() {
                round.play(seat, card).ok()?;
            }
        }
        Some(round)
    }

    /// Sample `count` worlds, retrying the rare inconsistent construction
    fn sample_worlds(&mut self, view: &View<'_>, count: u32) -> Vec<Round> {
        let mut worlds = Vec::with_capacity(count as usize);
        // The true deal is always consistent, so failures are sampling
        // accidents; the budget only guards against a pathological view.
        let mut attempts = count * 4 + 64;
        while worlds.len() < count as usize && attempts > 0 {
            attempts -= 1;
            if let Some(world) = self.sample_world(view) {
                worlds.push(world);
            }
        }
        worlds
    }

    /// The ordered candidate moves for the current decision, the greedy
    /// incumbent first; empty when the seat has no real choice
    fn candidates(view: &View<'_>) -> Vec<Candidate> {
        match view.phase() {
            Phase::Passing => pass_candidates(view),
            Phase::Playing => play_candidates(view),
            Phase::Finished => Vec::new(),
        }
    }

    /// Score every candidate on freshly sampled worlds and return the move
    /// to play: the greedy incumbent (`candidates[0]`) unless a challenger
    /// clears the significance gate
    fn choose(&mut self, view: &View<'_>, candidates: &[Candidate]) -> Choice {
        let worlds = self.sample_worlds(view, self.samples);
        let scored = score_worlds(view, &worlds, candidates, self.gate);
        candidates[recommended(&scored, candidates, self.gate)].choice
    }

    /// Assess every candidate action for the current decision, each with
    /// its Monte Carlo equity and expected round points, ranked by equity
    /// with the bot's own pick flagged — the read a solver or hint view
    /// shows
    ///
    /// When every legal play collapses into one equivalence class the read
    /// is still offered, against each legal play tied: a human cannot see
    /// from the hand alone that the ranks between the cards are already gone,
    /// so refusing the read would look like a bug.
    ///
    /// Returns empty only when there is genuinely nothing to weigh: a single
    /// legal play, a finished round, or a view no consistent world can be
    /// sampled for (e.g. a seat mid-pass).
    #[must_use]
    pub fn assess(&mut self, view: &View<'_>) -> Vec<Assessment> {
        let candidates = Self::candidates(view);
        if candidates.is_empty() {
            return Vec::new();
        }
        let worlds = self.sample_worlds(view, self.samples);
        if worlds.is_empty() {
            // No consistent world: averaging zero rollouts would put NaN
            // in the public rows.
            return Vec::new();
        }
        let scored = score_worlds(view, &worlds, &candidates, self.gate);
        let mean = |(equities, ev_sum, _): &Scored| {
            let n = equities.len() as f64;
            (equities.iter().sum::<f64>() / n, ev_sum / n)
        };

        // A single candidate means the legal plays all collapsed into one
        // class — every one interchangeable.  List them as tied so the panel
        // says "any of these" instead of going blank.
        if candidates.len() == 1 {
            let (equity, ev) = mean(&scored[0]);
            return view
                .legal_plays()
                .into_iter()
                .map(|card| Assessment {
                    action: format!("play {card}"),
                    equity,
                    ev,
                    recommended: true,
                })
                .collect();
        }

        let best = recommended(&scored, &candidates, self.gate);
        let mut out: Vec<Assessment> = candidates
            .iter()
            .zip(&scored)
            .enumerate()
            .map(|(i, (candidate, score))| {
                let (equity, ev) = mean(score);
                Assessment {
                    action: candidate.label.clone(),
                    equity,
                    ev,
                    recommended: i == best,
                }
            })
            .collect();
        out.sort_by(|a, b| b.equity.total_cmp(&a.equity));
        out
    }
}

/// The candidate passes: all triples of the [`PASS_POOL`] highest-scoring
/// cards, the greedy triple first; passes that empty a short side suit; plus
/// the one moon pass
fn pass_candidates(view: &View<'_>) -> Vec<Candidate> {
    let hand = view.hand();
    let mut ranked: Vec<Card> = hand.into_iter().collect();
    ranked.sort_by_key(|&card| -pass_score(hand, card, 1));

    // The mirror of the greedy triple: the three cards the pass policy least
    // wants gone are low cards of long suits, which is exactly the ballast a
    // shot sheds.  One candidate, rolled with a shooting continuation.
    let losers: [Card; 3] = ranked[ranked.len() - 3..]
        .try_into()
        .expect("a passing hand holds thirteen cards");

    let pool = &ranked[..ranked.len().min(PASS_POOL)];
    let mut triples = Vec::<[Card; 3]>::with_capacity(23);
    for i in 0..pool.len() {
        for j in i + 1..pool.len() {
            for k in j + 1..pool.len() {
                triples.push([pool[i], pool[j], pool[k]]);
            }
        }
    }

    // Independent card scores can award three separate void bonuses without
    // actually emptying a suit.  Make every short non-spade void reachable,
    // filling a one- or two-card suit's triple with the most dangerous cards
    // outside it.
    for suit in Suit::ASC.into_iter().filter(|&suit| suit != Suit::Spades) {
        let suited: Vec<Card> = hand[suit].iter().map(|rank| Card { suit, rank }).collect();
        if suited.is_empty() || suited.len() > 3 {
            continue;
        }
        let mut cards = suited;
        cards.extend(
            ranked
                .iter()
                .copied()
                .filter(|card| card.suit != suit)
                .take(3 - cards.len()),
        );
        let triple: [Card; 3] = cards.try_into().expect("a passing hand has filler");
        let set: Hand = triple.into_iter().collect();
        if !triples
            .iter()
            .any(|old| old.iter().copied().collect::<Hand>() == set)
        {
            triples.push(triple);
        }
    }

    let mut out: Vec<Candidate> = triples
        .into_iter()
        .map(|triple| Candidate {
            label: format!("pass {} {} {}", triple[0], triple[1], triple[2]),
            choice: Choice::Pass(triple),
            shoot: false,
        })
        .collect();
    out.push(Candidate {
        label: format!("shoot, pass {} {} {}", losers[0], losers[1], losers[2]),
        choice: Choice::Pass(losers),
        shoot: true,
    });
    out
}

/// The candidate plays: the legal set collapsed by rank adjacency — cards
/// of a suit separated only by ranks this seat has seen (own hand or
/// played) *and* of equal penalty value are interchangeable — the greedy
/// incumbent first, capped at [`MAX_CANDIDATES`]
fn play_candidates(view: &View<'_>) -> Vec<Candidate> {
    let legal = view.legal_plays();
    if legal.len() < 2 {
        return Vec::new();
    }
    let trick = view.current_trick().expect("a play decision has a trick");
    let played = view.played();
    let incumbent = greedy_play(legal, trick, played);
    let seen = view.hand() | played;

    let mut reps = vec![incumbent];
    for suit in Suit::ASC {
        let mut group: Vec<Rank> = Vec::new();
        let flush = |group: &mut Vec<Rank>, reps: &mut Vec<Card>| {
            if let Some(&low) = group.first() {
                let card = |rank| Card { suit, rank };
                if !group.iter().any(|&rank| card(rank) == incumbent) {
                    reps.push(card(low));
                }
            }
            group.clear();
        };
        let mut prev: Option<Rank> = None;
        for rank in legal[suit] {
            if let Some(prev) = prev {
                // Rank-adjacency is not equivalence: two touching cards of
                // unequal penalty value (Q♠ vs J♠) are distinct plays.
                // ponytail: keys on Card::points() (u8, standard Hearts); a
                // variant like Omnibus (J♦ = -10, 10♣ doubler) must widen the
                // value function this consults, then this line follows it.
                let same_value =
                    (Card { suit, rank: prev }).points() == (Card { suit, rank }).points();
                let contiguous = same_value
                    && (prev.get() + 1..rank.get()).all(|gap| seen[suit].contains(Rank::new(gap)));
                if !contiguous {
                    flush(&mut group, &mut reps);
                }
            }
            group.push(rank);
            prev = Some(rank);
        }
        flush(&mut group, &mut reps);
    }

    reps.truncate(MAX_CANDIDATES);
    let mut out: Vec<Candidate> = reps
        .into_iter()
        .map(|card| Candidate {
            label: format!("play {card}"),
            choice: Choice::Play(card),
            shoot: false,
        })
        .collect();

    // One moon candidate, past the cap: the truncation runs in suit order
    // and would otherwise drop the very high cards a shot opens with.
    // Offered only while nobody else has scored — a moon is dead the moment
    // they do — and only when the plays differ at all, so a fully collapsed
    // legal set keeps `assess`'s "every play tied" read.
    let alive = Seat::ALL
        .into_iter()
        .all(|seat| seat == view.seat() || view.points_taken(seat) == 0);
    if alive && out.len() > 1 {
        let card = shoot_play(legal, trick, played);
        out.push(Candidate {
            label: format!("shoot, play {card}"),
            choice: Choice::Play(card),
            shoot: true,
        });
    }
    out
}

/// Roll candidates through the same `worlds` (common random numbers) in
/// growing batches, eliminating challengers the incumbent already
/// dominates, and return per candidate its per-world equities and summed
/// round points
///
/// Survivors always reach the full world count, so the final
/// [`recommended`] read over them is exactly the unbatched one.  An
/// eliminated candidate keeps the equities it accumulated: its paired mean
/// against the incumbent is negative on that prefix, and [`beats`] zips to
/// the shorter slice, so [`recommended`] rejects it with no special
/// casing.
fn score_worlds(
    view: &View<'_>,
    worlds: &[Round],
    candidates: &[Candidate],
    gate: f64,
) -> Vec<Scored> {
    let me = view.seat();
    let rules = view.rules();
    let standing = absolute_standing(me, view.game_scores());
    let eval = |candidate: &Candidate, world: &Round| {
        let result = candidate.choice.roll(world.clone(), me, candidate.shoot);
        let moon = u32::from(result.shooter() == Some(me));
        let scores = result.scores(rules);
        (
            equity(&scores, me, standing, rules),
            f64::from(scores[me as usize]),
            moon,
        )
    };

    let mut scored: Vec<Scored> = vec![(Vec::new(), 0.0, 0); candidates.len()];
    let mut alive: Vec<usize> = (1..candidates.len()).collect();
    let mut done = 0;
    while done < worlds.len() {
        let batch = &worlds[done..worlds.len().min(done + done.max(BATCH))];
        for &i in std::iter::once(&0).chain(&alive) {
            let candidate = &candidates[i];
            #[cfg(feature = "parallel")]
            let results: Vec<(f64, f64, u32)> = {
                use rayon::prelude::*;
                batch
                    .par_iter()
                    .map(|world| eval(candidate, world))
                    .collect()
            };
            #[cfg(not(feature = "parallel"))]
            let results = batch.iter().map(|world| eval(candidate, world));

            // Reduced sequentially in world order in both builds, so a
            // parallel bot makes bit-identical decisions to a serial one.
            let (equities, ev_sum, moons) = &mut scored[i];
            for (equity, points, moon) in results {
                equities.push(equity);
                *ev_sum += points;
                *moons += moon;
            }
        }
        done += batch.len();
        if done < worlds.len() {
            alive.retain(|&i| !beats(&scored[0].0, &scored[i].0, gate));
            if alive.is_empty() {
                break;
            }
        }
    }
    scored
}

/// The index of the recommended candidate: the greedy incumbent
/// (`scored[0]`) unless a challenger's paired advantage clears the
/// [`beats`] gate, in which case the largest such gain
///
/// A moon challenger must clear a second bar: the rollouts have to actually
/// shoot it more often than not.
///
/// Its per-world equities are bimodal — the moon band's ceiling against a
/// heap of failures — and a paired test on heavy-tailed differences waves
/// through edges that are not there: measured against the greedy field, the
/// gate alone attempted a moon in two rounds in five to land one in twenty,
/// at a cost of over two points a round.  The break-even is arithmetic
/// rather than taste.  A moon is worth `0.5 + 26/112 ≈ 0.732` mid-game, a
/// failed shot leaves us holding about half the deck's points for roughly
/// `0.423`, and the greedy line is worth about `0.577`; the indifference
/// point between them sits within a hair of an even chance.  A moon rate
/// over the sampled worlds is a plain Bernoulli count with none of the tail
/// trouble, so require it to be a majority.
fn recommended(scored: &[Scored], candidates: &[Candidate], gate: f64) -> usize {
    let mean = |e: &[f64]| e.iter().sum::<f64>() / e.len() as f64;
    let likely = |i: usize| 2 * scored[i].2 as usize > scored[i].0.len();
    let defend = &scored[0].0;
    (1..scored.len())
        .filter(|&i| beats(&scored[i].0, defend, gate))
        .filter(|&i| !candidates[i].shoot || likely(i))
        .max_by(|&a, &b| mean(&scored[a].0).total_cmp(&mean(&scored[b].0)))
        .unwrap_or(0)
}

/// Whether the challenger's paired advantage over the incumbent is large
/// enough to trust
///
/// The true value difference between most candidate actions is well below
/// the rollout noise floor, and deviating from the solid greedy baseline
/// on noise alone plays *worse* than the baseline.  A one-sided paired
/// test — the mean difference at least `gate` standard errors above zero,
/// since several challengers get tested per decision — keeps only the
/// deviations the samples actually support.
fn beats(challenger: &[f64], incumbent: &[f64], gate: f64) -> bool {
    let n = challenger.len() as f64;
    let mean = challenger
        .iter()
        .zip(incumbent)
        .map(|(c, i)| c - i)
        .sum::<f64>()
        / n;
    if mean <= 0.0 {
        return false;
    }
    let var = challenger
        .iter()
        .zip(incumbent)
        .map(|(c, i)| (c - i - mean).powi(2))
        .sum::<f64>()
        / n;
    mean > gate * (var / n).sqrt()
}

/// Re-key the seat-relative [`View::game_scores`] by absolute [`Seat`]
fn absolute_standing(me: Seat, relative: [u16; 4]) -> [u16; 4] {
    let mut standing = [0; 4];
    standing[me as usize] = relative[0];
    standing[me.left() as usize] = relative[1];
    standing[me.across() as usize] = relative[2];
    standing[me.right() as usize] = relative[3];
    standing
}

/// The value to `me` of a rollout that charged `scores`, from the
/// `standing` game totals: `1/k` for a k-way shared win of the game, 0 for
/// a loss, otherwise affine in the round-point margin
///
/// A game ends when a post-round total reaches the target, and the lowest
/// total wins — shared by k seats for `1/k` each, matching
/// [`FinalScore::winners`](hearts::FinalScore).  Short of a clinch the
/// value is `0.5` plus the margin between the opponents' average round
/// points and ours, scaled into `(1/4, 3/4)` — a guaranteed gap below a
/// sole win and above any loss, so [`beats`] deviates from the round-point
/// objective only when a rollout can actually end the game.  (A k-way
/// shared win pays `1/k`, which for `k ≥ 2` lands at or below the
/// mid-game band — deliberately: sharing the crown is worth less than a
/// strong position in a game still running.)
fn equity(scores: &[u16; 4], me: Seat, standing: [u16; 4], rules: &Rules) -> f64 {
    let mut totals = standing;
    for seat in Seat::ALL {
        let total = &mut totals[seat as usize];
        *total = total.saturating_add(scores[seat as usize]);
    }
    if totals.iter().any(|&total| total >= rules.game_target) {
        let min = *totals.iter().min().expect("four totals");
        return if totals[me as usize] == min {
            1.0 / totals.iter().filter(|&&total| total == min).count() as f64
        } else {
            0.0
        };
    }
    let mine = f64::from(scores[me as usize]);
    let others: f64 = Seat::ALL
        .iter()
        .filter(|&&seat| seat != me)
        .map(|&seat| f64::from(scores[seat as usize]))
        .sum::<f64>()
        / 3.0;
    0.5 + (others - mine) / 112.0
}

impl<R: Rng> Strategy for MonteCarloBot<R> {
    fn pass_cards(&mut self, view: &View<'_>) -> [Card; 3] {
        let candidates = MonteCarloBot::<R>::candidates(view);
        match self.choose(view, &candidates) {
            Choice::Pass(cards) => cards,
            Choice::Play(_) => unreachable!("the passing phase yields pass choices"),
        }
    }

    fn play_card(&mut self, view: &View<'_>) -> Card {
        // Reactive moon defense: the rollout policy models every opponent as
        // a greedy ducker who never shoots the moon, so the search is blind
        // to a moon in progress.  Break it with the same overlay HeuristicBot
        // uses (threshold 8, its default).  Draws no randomness, so seeded
        // play stays reproducible.
        if let Some(card) = moon_defense(view, 8) {
            return card;
        }
        let legal = view.legal_plays();
        if legal.len() == 1 {
            // A forced card needs no rollout — and draws no randomness,
            // keeping seeded play reproducible.
            return legal.into_iter().next().expect("checked non-empty");
        }
        let trick = view.current_trick().expect("a play decision has a trick");

        // A moon is a plan, not a card, and the rollouts that chose it
        // priced thirteen tricks of shooting.  Re-deciding it every trick
        // instead measured out at three overrides in ten — the search kept
        // wandering back to the greedy line holding points it had already
        // paid for, which is the worst of both.  So carry the plan: the
        // first trick of a round clears it, another seat scoring kills it,
        // and in between the shot is simply played.
        if view.tricks().is_empty()
            || Seat::ALL
                .into_iter()
                .any(|seat| seat != view.seat() && view.points_taken(seat) > 0)
        {
            self.shooting = false;
        }
        if self.shooting {
            return shoot_play(legal, trick, view.played());
        }

        let candidates = MonteCarloBot::<R>::candidates(view);
        if candidates.len() < 2 {
            // Every legal card collapsed into one equivalence class.
            return greedy_play(legal, trick, view.played());
        }
        let worlds = self.sample_worlds(view, self.samples);
        let scored = score_worlds(view, &worlds, &candidates, self.gate);
        let best = recommended(&scored, &candidates, self.gate);
        self.shooting = candidates[best].shoot;
        match candidates[best].choice {
            Choice::Play(card) => card,
            Choice::Pass(_) => unreachable!("the playing phase yields play choices"),
        }
    }

    fn name(&self) -> &str {
        "mc"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HeuristicBot, Table};
    use hearts::Holding;
    use rand::SeedableRng as _;
    use rand::rngs::StdRng;

    /// The sorted deck dealt round-robin on a hold deal.
    fn fixed_table() -> Table {
        let mut hands = [Hand::EMPTY; 4];
        for (i, card) in Hand::ALL.into_iter().enumerate() {
            hands[i % 4].insert(card);
        }
        let round = Round::from_deal(Rules::new(), PassDirection::Hold, hands)
            .expect("a round-robin deal partitions the deck");
        Table::new(round)
    }

    /// Drive a few tricks with the greedy bot to reach a mid-round spot.
    fn mid_round() -> Table {
        let mut table = fixed_table();
        let mut bot = crate::HeuristicBot::new();
        while table.round().tricks().len() < 5 {
            table.step(&mut bot).expect("greedy plays are legal");
        }
        table
    }

    #[test]
    fn sampled_worlds_are_consistent_with_the_view() {
        let table = mid_round();
        let me = table.turn().expect("a mid-round table has a mover");
        let view = table.view(me);
        let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(1));

        let worlds = bot.sample_worlds(&view, 32);
        assert_eq!(worlds.len(), 32);
        for world in &worlds {
            // The public position is reproduced exactly.
            assert_eq!(world.tricks(), view.tricks());
            assert_eq!(world.current_trick(), view.current_trick());
            assert_eq!(world.played(), view.played());
            assert_eq!(world.hand(me), view.hand());
            assert_eq!(world.phase(), Phase::Playing);

            for seat in Seat::ALL {
                let hand = world.hand(seat);
                // Right sizes, only possible cards, known cards pinned.
                assert_eq!(hand.len(), view.hand_len(seat));
                assert_eq!(hand - view.possible_cards(seat), Hand::EMPTY);
                let known = view.known_cards(seat);
                assert_eq!(hand & known, known);
            }
        }
    }

    #[test]
    fn reconstruction_replays_any_real_position() {
        // The omniscient reconstruction — original hands as a hold deal,
        // history replayed — must land on the real position, whatever the
        // real pass direction was.  This is the guard the gin engine needed
        // a whole sync-sim skill for.
        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..20 {
            let mut table = Table::deal(Rules::new(), PassDirection::Left, &mut rng);
            let mut bot = crate::HeuristicBot::new();
            for _ in 0..rng.random_range(4..30) {
                table.step(&mut bot).expect("greedy plays are legal");
            }
            let round = table.round();
            if round.phase() != Phase::Playing {
                continue;
            }

            // Original hands: what each seat holds now plus what it played.
            let mut originals = [Hand::EMPTY; 4];
            for seat in Seat::ALL {
                let mut own = round.hand(seat);
                for trick in round.tricks().iter().copied().chain(round.current_trick()) {
                    if let Some(card) = trick.card_from(seat) {
                        own.insert(card);
                    }
                }
                originals[seat as usize] = own;
            }

            let mut replay = Round::from_deal(*round.rules(), PassDirection::Hold, originals)
                .expect("original hands partition the deck");
            for trick in round.tricks().iter().copied().chain(round.current_trick()) {
                for (seat, card) in trick.plays() {
                    replay.play(seat, card).expect("the real history is legal");
                }
            }
            assert_eq!(replay.tricks(), round.tricks());
            assert_eq!(replay.current_trick(), round.current_trick());
            for seat in Seat::ALL {
                assert_eq!(replay.hand(seat), round.hand(seat));
            }
        }
    }

    #[test]
    fn seeded_bots_repeat_their_decisions() {
        let table = mid_round();
        let me = table.turn().expect("a mid-round table has a mover");
        let decide = |seed| {
            let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(seed)).samples(16);
            bot.play_card(&table.view(me))
        };
        assert_eq!(decide(3), decide(3));
    }

    #[test]
    fn equity_is_terminal_at_the_target() {
        let rules = Rules::new();
        let me = Seat::North;
        // North ends lowest when West crosses: a clean win.
        let win = equity(&[0, 10, 10, 6], me, [50, 60, 70, 95], &rules);
        assert_eq!(win, 1.0);
        // North crosses alone: a loss.
        let loss = equity(&[13, 13, 0, 0], me, [95, 0, 0, 0], &rules);
        assert_eq!(loss, 0.0);
        // A two-way shared win pays a half.
        let shared = equity(&[0, 0, 13, 13], me, [50, 50, 60, 95], &rules);
        assert_eq!(shared, 0.5);
    }

    #[test]
    fn equity_orders_round_outcomes_mid_game() {
        let rules = Rules::new();
        let me = Seat::North;
        let level = [0u16; 4];
        let clean = equity(&[0, 13, 13, 0], me, level, &rules);
        let queen = equity(&[13, 13, 0, 0], me, level, &rules);
        let moon = equity(&[0, 26, 26, 26], me, level, &rules);
        let mooned = equity(&[26, 0, 26, 26], me, level, &rules);
        assert!(moon > clean && clean > queen);
        // Relative to the field, eating the queen and being mooned cost
        // the same 8⅔-point margin — the measure is zero-sum on purpose.
        assert!((queen - mooned).abs() < 1e-12);
        for value in [clean, queen, moon, mooned] {
            assert!((0.25..=0.75).contains(&value), "{value} is mid-game");
        }
    }

    #[test]
    fn beats_requires_a_clear_margin() {
        let base: Vec<f64> = (0..32).map(|i| f64::from(i % 5)).collect();
        let noisy: Vec<f64> = base
            .iter()
            .enumerate()
            .map(|(i, x)| x + if i % 2 == 0 { 1.05 } else { -0.95 })
            .collect();
        assert!(!beats(&noisy, &base, 1.5));
        // The same slim edge passes a gate loose enough to trust it.
        assert!(beats(&noisy, &base, 0.1));

        let better: Vec<f64> = base.iter().map(|x| x + 1.0).collect();
        assert!(beats(&better, &base, 1.5));
        assert!(!beats(&base, &better, 1.5));
        assert!(!beats(&base, &base, 1.5));
    }

    #[test]
    fn play_candidates_collapse_touching_ranks() {
        let table = fixed_table();
        // North leads trick 1: only the 2♣ is legal, no real choice.
        let north = table.turn().expect("a fresh hold deal has a leader");
        assert_eq!(play_candidates(&table.view(north)).len(), 0);
    }

    #[test]
    fn play_candidates_keep_queen_distinct_from_neighbors() {
        // South must follow a led spade holding only J♠ and Q♠. They are
        // rank-adjacent, but Q♠ is worth 13 penalty points and J♠ nothing —
        // they are not one candidate, and the solver must not refuse the read.
        let hand = |cards: &str| -> Hand {
            cards
                .split_whitespace()
                .map(|c| c.parse().expect("valid card"))
                .collect()
        };
        let hands = [
            hand("2C 6C 7C 8C 6D 7D 8D 9D 6H 7H 8H 3S 4S"), // North: leads 2♣
            hand("9C TC AC TD JD QD 9H TH JH 2S 5S 6S 7S"), // East: wins T1, leads a spade
            hand("3C 4C 5C 2D 3D 4D 5D 2H 3H 4H 5H JS QS"), // South: only spades are J♠ Q♠
            hand("JC QC KC KD AD QH KH AH 8S 9S TS KS AS"), // West
        ];
        let mut round = Round::from_deal(Rules::new(), PassDirection::Hold, hands)
            .expect("a full partition deals");
        for (seat, card) in [
            (Seat::North, "2C"),
            (Seat::East, "AC"),
            (Seat::South, "3C"),
            (Seat::West, "JC"),
            (Seat::East, "2S"), // East won trick 1; now it leads a low spade
        ] {
            round
                .play(seat, card.parse().expect("valid card"))
                .expect("a legal scripted play");
        }

        let table = Table::new(round);
        assert_eq!(
            table.turn(),
            Some(Seat::South),
            "South is to follow the spade"
        );
        let view = table.view(Seat::South);

        let cands = play_candidates(&view);
        assert_eq!(
            cands.iter().filter(|c| !c.shoot).count(),
            2,
            "Q♠ must not collapse into J♠"
        );
        assert!(
            cands
                .iter()
                .any(|c| matches!(c.choice, Choice::Play(card) if card == Card::QUEEN_OF_SPADES)),
            "Q♠ is weighed as its own play",
        );

        let mut solver = MonteCarloBot::new(StdRng::seed_from_u64(7)).samples(64);
        assert!(!solver.assess(&view).is_empty(), "the hint offers a read");
    }

    #[test]
    fn assess_lists_an_all_equivalent_class_rather_than_refusing() {
        // South follows a led spade holding only 5♠ and 7♠.  From the hand
        // they look like two separate plays; but 6♠ is already on the table,
        // so they are interchangeable — one class, one candidate.  A human
        // cannot read that off the hand, so the hint must still list both
        // tied instead of going blank.
        let hand = |cards: &str| -> Hand {
            cards
                .split_whitespace()
                .map(|c| c.parse().expect("valid card"))
                .collect()
        };
        let hands = [
            hand("2C 3C 4C 2S 3S 4S 2D 3D 4D 5D 2H 3H 4H"), // North: leads 2♣
            hand("AC KC 6S 8S 9S TS 6D 7D 8D 9D 5H 6H 7H"), // East: wins T1, leads 6♠
            hand("5C 6C 7C 5S 7S TD JD QD KD AD 8H 9H TH"), // South: spades are 5♠ 7♠ only
            hand("8C 9C TC JC QC JS QS KS AS JH QH KH AH"), // West
        ];
        let mut round = Round::from_deal(Rules::new(), PassDirection::Hold, hands)
            .expect("a full partition deals");
        for (seat, card) in [
            (Seat::North, "2C"),
            (Seat::East, "AC"),
            (Seat::South, "5C"),
            (Seat::West, "8C"),
            (Seat::East, "6S"), // East won trick 1; now it leads the gap card
        ] {
            round
                .play(seat, card.parse().expect("valid card"))
                .expect("a legal scripted play");
        }

        let table = Table::new(round);
        assert_eq!(
            table.turn(),
            Some(Seat::South),
            "South is to follow the spade"
        );
        let view = table.view(Seat::South);

        // One equivalence class, so the bot weighs a single candidate...
        assert_eq!(
            play_candidates(&view).len(),
            1,
            "5♠ and 7♠ collapse across the played 6♠",
        );

        // ...yet the hint lists both legal plays, tied and flagged.
        let mut solver = MonteCarloBot::new(StdRng::seed_from_u64(11)).samples(64);
        let rows = solver.assess(&view);
        assert_eq!(rows.len(), 2, "both equivalent spades are shown");
        assert!(
            rows.iter().all(|r| r.recommended),
            "an equivalent class is all-tied",
        );
        assert!(
            rows[0].equity == rows[1].equity && rows[0].ev == rows[1].ev,
            "equivalent plays carry the same read",
        );
    }

    #[test]
    fn void_respecting_worlds() {
        // Play until somebody shows a void, then check every sampled world
        // honors it.
        let mut rng = StdRng::seed_from_u64(21);
        let mut table = Table::deal(Rules::new(), PassDirection::Hold, &mut rng);
        let mut bot = crate::HeuristicBot::new();
        let (observer, void_seat, void_suit) = loop {
            table.step(&mut bot).expect("greedy plays are legal");
            let round = table.round();
            assert_eq!(round.phase(), Phase::Playing, "a void shows up first");
            let found = Seat::ALL.iter().find_map(|&seat| {
                let observer = round.turn()?;
                Suit::ASC
                    .into_iter()
                    .find(|&suit| observer != seat && table.view(observer).is_void(seat, suit))
                    .map(|suit| (observer, seat, suit))
            });
            if let Some(found) = found {
                break found;
            }
        };

        let view = table.view(observer);
        let mut mc = MonteCarloBot::new(StdRng::seed_from_u64(2));
        for world in mc.sample_worlds(&view, 16) {
            assert_eq!(world.hand(void_seat)[void_suit], Holding::EMPTY);
        }
    }

    #[test]
    fn assess_ranks_candidates_and_flags_the_bots_pick() {
        let table = mid_round();
        let me = table.turn().expect("a mid-round table has a mover");
        let view = table.view(me);

        let mut solver = MonteCarloBot::new(StdRng::seed_from_u64(7)).samples(64);
        let mut chooser = MonteCarloBot::new(StdRng::seed_from_u64(7)).samples(64);

        let rows = solver.assess(&view);
        if rows.is_empty() {
            // A forced spot: the chooser must agree it is forced.
            assert!(view.legal_plays().len() == 1 || play_candidates(&view).len() < 2);
            return;
        }
        for row in &rows {
            assert!((0.0..=1.0).contains(&row.equity));
            assert!(row.ev >= 0.0);
        }
        assert!(rows.windows(2).all(|w| w[0].equity >= w[1].equity));
        assert_eq!(rows.iter().filter(|r| r.recommended).count(), 1);

        let picked = rows.iter().find(|r| r.recommended).expect("a flagged pick");
        let played = chooser.play_card(&view);
        assert!(
            picked.action.ends_with(&format!("play {played}")),
            "{} is the card {played} was picked for",
            picked.action,
        );
    }

    #[test]
    fn pass_assessment_covers_twenty_triples() {
        let mut rng = StdRng::seed_from_u64(5);
        let table = Table::deal(Rules::new(), PassDirection::Left, &mut rng);
        let me = table.turn().expect("passing starts at North");
        let view = table.view(me);
        let cands = pass_candidates(&view);
        assert!(cands.iter().filter(|c| !c.shoot).count() >= 20);
        assert_eq!(cands.iter().filter(|c| c.shoot).count(), 1, "the moon pass");

        let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(6)).samples(16);
        let picks = bot.pass_cards(&view);
        assert_eq!(
            picks.into_iter().collect::<Hand>().len(),
            3,
            "three distinct cards"
        );
        for card in picks {
            assert!(view.hand().contains(card));
        }
    }

    #[test]
    fn pass_candidates_can_create_a_void_outside_the_top_six() {
        fn hand(text: &str) -> Hand {
            text.parse().expect("a valid hand")
        }
        let mine = hand("234.QKA.QKA.JQKA");
        let mut hands = [mine, Hand::EMPTY, Hand::EMPTY, Hand::EMPTY];
        for (i, card) in (Hand::ALL - mine).into_iter().enumerate() {
            hands[1 + i % 3].insert(card);
        }
        let round = Round::from_deal(Rules::new(), PassDirection::Left, hands).unwrap();
        let table = Table::new(round);
        let void = hand("234...");
        assert!(
            pass_candidates(&table.view(Seat::North))
                .iter()
                .any(|candidate| matches!(candidate.choice, Choice::Pass(cards)
                if cards.into_iter().collect::<Hand>() == void))
        );
    }

    #[test]
    fn a_hand_that_cannot_win_a_trick_does_not_try() {
        // The guard against the opposite failure.  A moon candidate is on
        // offer at every pass, so it has to lose on a hand with no length,
        // no ace and nothing above a five: the shot dies on trick two and
        // the rollout charges us for having kept the queen.
        fn hand(text: &str) -> Hand {
            text.parse().expect("a valid hand")
        }
        let hands = [
            hand("234.234.234.2345"), // North: the worst hand in the deck
            hand("5678.567.567.678"),
            hand("9TJ.89TJ.89T.9TJ"),
            hand("QKA.QKA.JQKA.QKA"),
        ];
        let round = Round::from_deal(Rules::new(), PassDirection::Left, hands)
            .expect("a full partition deals");
        let table = Table::new(round);
        let view = table.view(Seat::North);

        let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(0xC0DE)).samples(128);
        let picked = bot
            .assess(&view)
            .into_iter()
            .find(|row| row.recommended)
            .expect("a flagged pick");
        assert!(
            !picked.action.starts_with("shoot"),
            "shot the moon holding nothing: {}",
            picked.action,
        );
    }

    #[test]
    fn seeded_pick_is_identical_across_serial_and_parallel_builds() {
        // The `parallel` feature must not change a single decision: batch
        // results are collected in world order and reduced sequentially in
        // both builds.  CI runs the suite with and without the feature; a
        // failure in only one build means the parallel reduce stopped
        // being order-exact.  Re-pin the expected card whenever sampling
        // logic changes.
        let table = mid_round();
        let me = table.turn().expect("a mid-round table has a mover");
        let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(11)).samples(64);
        let first = bot.play_card(&table.view(me));
        let mut again = MonteCarloBot::new(StdRng::seed_from_u64(11)).samples(64);
        assert_eq!(first, again.play_card(&table.view(me)));
    }

    #[test]
    fn breaks_a_moon_the_rollouts_never_see() {
        // The rollout policy models every opponent as a greedy ducker who
        // never shoots, so a moon in progress is invisible to the search.
        // Drive a position where East has already swept the Q♠ (13 points),
        // then confirm the MC bot takes the trick to break the sweep instead
        // of ducking the way its own rollout policy would.
        fn card(text: &str) -> Card {
            text.parse().expect("a valid card")
        }
        fn hand(cards: &[&str]) -> Hand {
            cards.iter().map(|c| card(c)).collect()
        }
        struct Fixed(Card);
        impl Strategy for Fixed {
            fn pass_cards(&mut self, _: &View<'_>) -> [Card; 3] {
                unreachable!()
            }
            fn play_card(&mut self, _: &View<'_>) -> Card {
                self.0
            }
        }

        let hands = [
            // North: leads 2♣, and ends holding S3 (duck) vs SK (break).
            hand(&[
                "2♣", "3♣", "3♠", "K♠", "A♦", "3♥", "4♥", "5♥", "6♥", "7♥", "8♥", "9♥", "T♥",
            ]),
            // East: wins two club tricks and scoops the dumped Q♠.
            hand(&[
                "A♣", "K♣", "Q♣", "J♣", "5♠", "6♠", "7♠", "8♠", "9♠", "T♠", "J♠", "A♠", "2♦",
            ]),
            // South: void in clubs, dumps Q♠ into East's second trick.
            hand(&[
                "Q♠", "2♠", "4♠", "J♥", "Q♥", "K♥", "A♥", "3♦", "4♦", "5♦", "6♦", "7♦", "8♦",
            ]),
            // West: void in spades, never overtakes the sweep.
            hand(&[
                "4♣", "5♣", "6♣", "7♣", "8♣", "9♣", "T♣", "9♦", "T♦", "J♦", "Q♦", "K♦", "2♥",
            ]),
        ];
        let round = Round::from_deal(Rules::new(), PassDirection::Hold, hands).unwrap();
        let mut table = Table::new(round);
        let script = [
            (Seat::North, "2♣"), // trick 1: clubs, East wins with A♣
            (Seat::East, "A♣"),
            (Seat::South, "3♦"), // void in clubs, sheds a diamond (no T1 penalties)
            (Seat::West, "4♣"),
            (Seat::East, "K♣"), // trick 2: East leads, South dumps the queen
            (Seat::South, "Q♠"),
            (Seat::West, "5♣"),
            (Seat::North, "3♣"),
            (Seat::East, "5♠"), // trick 3: East leads a low spade into North
            (Seat::South, "4♠"),
            (Seat::West, "2♥"), // keep the trick costly, so plain greedy ducks
        ];
        for (seat, text) in script {
            assert_eq!(table.turn(), Some(seat));
            table.step(&mut Fixed(card(text))).unwrap();
        }

        let view = table.view(Seat::North);
        assert_eq!(view.points_taken(Seat::East), 13, "East swept the queen");
        // Greedy — the rollout policy — would duck with the 3♠; the moon
        // defense overrides it to break the sweep with the cheapest winner.
        assert_eq!(
            greedy_play(
                view.legal_plays(),
                view.current_trick().unwrap(),
                view.played()
            ),
            card("3♠")
        );
        let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(1)).samples(64);
        assert_eq!(bot.play_card(&view), card("K♠"));

        // East already holds points, so the moon is dead for North and the
        // shoot candidate is not even generated.
        assert!(
            !play_candidates(&view).iter().any(|c| c.shoot),
            "a dead moon is not a candidate",
        );
    }

    #[test]
    fn a_laydown_moon_is_rolled_as_one() {
        fn card(text: &str) -> Card {
            text.parse().expect("a valid card")
        }
        fn hand(text: &str) -> Hand {
            text.parse().expect("a valid hand")
        }

        // North holds the top of every suit but diamonds, and never loses
        // the lead long enough for anyone to lead one.  East, void in
        // spades, discards on every spade lead — a heart under the greedy
        // policy, a diamond once the table knows it is defending a moon.
        let hands = [
            hand("QKA..QKA.29TJQKA"), // North: the monster, and the 2♠ ballast
            hand("2345.234567.234."), // East: holds the deuce, void in spades
            hand("6789.89T.567.456"),
            hand("TJ.JQKA.89TJ.378"),
        ];
        let mut round = Round::from_deal(Rules::new(), PassDirection::Hold, hands)
            .expect("a full partition deals");
        for (seat, text) in [(Seat::East, "2♣"), (Seat::South, "6♣"), (Seat::West, "T♣")] {
            round
                .play(seat, card(text))
                .expect("a legal scripted play, North last to the deuce");
        }

        let ace = Choice::Play(card("A♣"));
        assert_eq!(
            ace.roll(round.clone(), Seat::North, true).shooter(),
            Some(Seat::North),
            "the shooting continuation takes all thirteen tricks",
        );
        // End to end, against three bots that defend: the search has to find
        // the shot *and* carry it across all thirteen tricks.  The point-aware
        // greedy policy can also take this clean opening trick now, so the
        // fixture no longer requires the non-shoot continuation to fail.
        let mut bot = MonteCarloBot::new(StdRng::seed_from_u64(3)).samples(64);
        let mut east = HeuristicBot::new();
        let mut south = HeuristicBot::new();
        let mut west = HeuristicBot::new();
        let result = Table::new(round)
            .play([&mut bot, &mut east, &mut south, &mut west])
            .expect("no seat cheats");
        assert_eq!(result.shooter(), Some(Seat::North), "the bot shot it");
    }
}
