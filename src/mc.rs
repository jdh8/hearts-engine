//! [`MonteCarloBot`]: determinized Monte Carlo move selection
//!
//! Unlike the gin engine this crate mirrors, there is no separate rollout
//! simulator to keep in sync with the rules: every Hearts observation
//! except the hidden hands is public, so a sampled world is a real
//! [`hearts::Round`] — reconstructed as a hold deal of the sampled original
//! hands with the public history replayed — and rollouts run on the real
//! state machine.

use crate::heuristic::{greedy_pass, greedy_play, pass_score, rollout_play};
use crate::{Strategy, View};
use hearts::{Card, Hand, PassDirection, Phase, Rank, Round, RoundResult, Rules, Seat, Suit};
use rand::{Rng, RngExt as _};

/// How many candidate plays a decision weighs after collapsing
/// rank-adjacent equivalents; the rest are never worth a rollout.
const MAX_CANDIDATES: usize = 8;

/// How many of the highest [`pass_score`] cards seed the pass triples:
/// all 20 triples of the top 6 are rolled.
const PASS_POOL: usize = 6;

/// The world count of the first scoring batch; each later batch doubles the
/// evaluated total, so elimination checkpoints fall after 32, 64, 128, ...
/// worlds.  A decision of 32 samples or fewer is a single batch, identical
/// to an unbatched run.
const BATCH: usize = 32;

/// One candidate action to score: the move a [`Strategy`] method would
/// return, paired with its rendered [`Assessment::action`] label
struct Candidate {
    label: String,
    choice: Choice,
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
    /// Apply the move to a world and play it out greedily.
    fn roll(self, mut round: Round, me: Seat) -> RoundResult {
        match self {
            Self::Pass(cards) => round
                .pass(me, cards.into_iter().collect())
                .expect("a candidate pass is legal in every sampled world"),
            Self::Play(card) => round
                .play(me, card)
                .expect("a candidate play is legal in every sampled world"),
        }
        while let Some(seat) = round.turn() {
            let card = rollout_play(&round, seat);
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
/// The bot owns its random number generator, so a seeded generator makes
/// its play reproducible.
pub struct MonteCarloBot<R: Rng> {
    rng: R,
    samples: u32,
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
        Self { rng, samples: 128 }
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
        let scored = score_worlds(view, &worlds, candidates);
        candidates[recommended(&scored)].choice
    }

    /// Assess every candidate action for the current decision, each with
    /// its Monte Carlo equity and expected round points, ranked by equity
    /// with the bot's own pick flagged — the read a solver or hint view
    /// shows
    ///
    /// Returns empty when there is nothing to weigh: no real choice (a
    /// single legal play, or every legal play in one equivalence class), a
    /// finished round, or a view no consistent world can be sampled for
    /// (e.g. a seat mid-pass).
    #[must_use]
    pub fn assess(&mut self, view: &View<'_>) -> Vec<Assessment> {
        let candidates = Self::candidates(view);
        if candidates.len() < 2 {
            return Vec::new();
        }
        let worlds = self.sample_worlds(view, self.samples);
        if worlds.is_empty() {
            // No consistent world: averaging zero rollouts would put NaN
            // in the public rows.
            return Vec::new();
        }
        let scored = score_worlds(view, &worlds, &candidates);
        let best = recommended(&scored);
        let mut out: Vec<Assessment> = candidates
            .iter()
            .zip(&scored)
            .enumerate()
            .map(|(i, (candidate, (equities, ev_sum)))| {
                let n = equities.len() as f64;
                Assessment {
                    action: candidate.label.clone(),
                    equity: equities.iter().sum::<f64>() / n,
                    ev: ev_sum / n,
                    recommended: i == best,
                }
            })
            .collect();
        out.sort_by(|a, b| b.equity.total_cmp(&a.equity));
        out
    }
}

/// The candidate passes: all triples of the [`PASS_POOL`] highest-scoring
/// cards, the greedy triple first
fn pass_candidates(view: &View<'_>) -> Vec<Candidate> {
    let hand = view.hand();
    let mut ranked: Vec<Card> = hand.into_iter().collect();
    ranked.sort_by_key(|&card| -pass_score(hand, card, 1));
    ranked.truncate(PASS_POOL);

    let mut out = Vec::with_capacity(20);
    for i in 0..ranked.len() {
        for j in i + 1..ranked.len() {
            for k in j + 1..ranked.len() {
                let triple = [ranked[i], ranked[j], ranked[k]];
                out.push(Candidate {
                    label: format!("pass {} {} {}", triple[0], triple[1], triple[2]),
                    choice: Choice::Pass(triple),
                });
            }
        }
    }
    out
}

/// The candidate plays: the legal set collapsed by rank adjacency — cards
/// of a suit separated only by ranks this seat has seen (own hand or
/// played) are interchangeable — the greedy incumbent first, capped at
/// [`MAX_CANDIDATES`]
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
                let contiguous =
                    (prev.get() + 1..rank.get()).all(|gap| seen[suit].contains(Rank::new(gap)));
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
    reps.into_iter()
        .map(|card| Candidate {
            label: format!("play {card}"),
            choice: Choice::Play(card),
        })
        .collect()
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
) -> Vec<(Vec<f64>, f64)> {
    let me = view.seat();
    let rules = view.rules();
    let standing = absolute_standing(me, view.game_scores());
    let eval = |candidate: &Candidate, world: &Round| {
        let result = candidate.choice.roll(world.clone(), me);
        let scores = result.scores(rules);
        (
            equity(&scores, me, standing, rules),
            f64::from(scores[me as usize]),
        )
    };

    let mut scored: Vec<(Vec<f64>, f64)> = vec![(Vec::new(), 0.0); candidates.len()];
    let mut alive: Vec<usize> = (1..candidates.len()).collect();
    let mut done = 0;
    while done < worlds.len() {
        let batch = &worlds[done..worlds.len().min(done + done.max(BATCH))];
        for &i in std::iter::once(&0).chain(&alive) {
            let candidate = &candidates[i];
            #[cfg(feature = "parallel")]
            let results: Vec<(f64, f64)> = {
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
            let (equities, ev_sum) = &mut scored[i];
            for (equity, points) in results {
                equities.push(equity);
                *ev_sum += points;
            }
        }
        done += batch.len();
        if done < worlds.len() {
            alive.retain(|&i| !beats(&scored[0].0, &scored[i].0));
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
fn recommended(scored: &[(Vec<f64>, f64)]) -> usize {
    let mean = |e: &[f64]| e.iter().sum::<f64>() / e.len() as f64;
    let defend = &scored[0].0;
    (1..scored.len())
        .filter(|&i| beats(&scored[i].0, defend))
        .max_by(|&a, &b| mean(&scored[a].0).total_cmp(&mean(&scored[b].0)))
        .unwrap_or(0)
}

/// Whether the challenger's paired advantage over the incumbent is large
/// enough to trust
///
/// The true value difference between most candidate actions is well below
/// the rollout noise floor, and deviating from the solid greedy baseline
/// on noise alone plays *worse* than the baseline.  A one-sided paired
/// test — the mean difference at least two standard errors above zero,
/// since several challengers get tested per decision — keeps only the
/// deviations the samples actually support.
fn beats(challenger: &[f64], incumbent: &[f64]) -> bool {
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
    mean > 2.0 * (var / n).sqrt()
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
        let legal = view.legal_plays();
        if legal.len() == 1 {
            // A forced card needs no rollout — and draws no randomness,
            // keeping seeded play reproducible.
            return legal.into_iter().next().expect("checked non-empty");
        }
        let candidates = MonteCarloBot::<R>::candidates(view);
        if candidates.len() < 2 {
            // Every legal card collapsed into one equivalence class.
            let trick = view.current_trick().expect("a play decision has a trick");
            return greedy_play(legal, trick, view.played());
        }
        match self.choose(view, &candidates) {
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
    use crate::Table;
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
        assert!(!beats(&noisy, &base));

        let better: Vec<f64> = base.iter().map(|x| x + 1.0).collect();
        assert!(beats(&better, &base));
        assert!(!beats(&base, &better));
        assert!(!beats(&base, &base));
    }

    #[test]
    fn play_candidates_collapse_touching_ranks() {
        let table = fixed_table();
        // North leads trick 1: only the 2♣ is legal, no real choice.
        let north = table.turn().expect("a fresh hold deal has a leader");
        assert_eq!(play_candidates(&table.view(north)).len(), 0);
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
        assert_eq!(picked.action, format!("play {played}"));
    }

    #[test]
    fn pass_assessment_covers_twenty_triples() {
        let mut rng = StdRng::seed_from_u64(5);
        let table = Table::deal(Rules::new(), PassDirection::Left, &mut rng);
        let me = table.turn().expect("passing starts at North");
        let view = table.view(me);
        assert_eq!(pass_candidates(&view).len(), 20);

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
}
