//! `MonteCarloBot` behind the Hearts Bot Arena's line protocol
//!
//! The arena (<https://hearts-arena.snowfrog.dev/guide>) speaks to one seat
//! over stdin/stdout and tells it only what that seat sees.  The engine's
//! strategies need a [`Table`], which wraps a complete [`Round`] — so at
//! every prompt this adapter deals a *placeholder* round consistent with
//! everything public (each seat's plays, our pass, every shown void),
//! replays the history into it, and asks the bot through
//! [`Table::view`].  The view hides the other hands, so the placeholder's
//! guesses cannot leak into a decision: they only make the `Round` whole.

use hearts::{Card, Hand, PassDirection, Round, Rules, Seat, Suit};
use hearts_engine::{MonteCarloBot, Strategy, Table};
use rand::rngs::StdRng;
use rand::{Rng, RngExt as _, SeedableRng as _};
use std::io::{self, BufRead as _, Write as _};

/// The ruleset revision this bot plays; any other is refused loudly.
const RULES_ID: &str = "std-1";

/// Our absolute seat.  The protocol's seats count clockwise from us, so
/// any seat works; South matches the engine's examples.
const ME: Seat = Seat::South;

/// Salt that splits the placeholder's stream from the bot's own.
const PLACEHOLDER_SALT: u64 = 0x9E37_79B9_7F4A_7C15;

/// The share of a fuel allowance a decision plans to spend, leaving room
/// for one two-thirds dearer than the worst measured
const FUEL_SHARE: f64 = 0.6;

/// The worst fuel per world over 112,000 decisions in 192 four-bot games:
/// a pass, and a play's fixed part and its part per trick left
const PASS_PER_WORLD: f64 = 5.8e6;
const PLAY_PER_WORLD: f64 = 1.3e6;
const PLAY_PER_WORLD_PER_TRICK: f64 = 1.5e5;

/// The fewest worlds a decision samples, however short of fuel
const MIN_SAMPLES: u32 = 16;

/// The most worlds a decision samples, however much fuel is left
const MAX_SAMPLES: u32 = match option_env!("ARENA_MAX_SAMPLES") {
    Some(_) => const_parse(option_env!("ARENA_MAX_SAMPLES")),
    None => 1024,
};

const fn const_parse(digits: Option<&str>) -> u32 {
    let bytes = match digits {
        Some(digits) => digits.as_bytes(),
        None => b"",
    };
    let mut n = 0;
    let mut i = 0;
    while i < bytes.len() {
        assert!(bytes[i].is_ascii_digit(), "ARENA_MAX_SAMPLES is a count");
        n = n * 10 + (bytes[i] - b'0') as u32;
        i += 1;
    }
    n
}

/// The absolute seat `rel` places clockwise from us
fn seat(rel: usize) -> Seat {
    (0..rel).fold(ME, |seat, _| seat.left())
}

/// The protocol's two-character card, e.g. `TH`
fn code(card: Card) -> String {
    format!("{}{}", card.rank.letter(), card.suit.letter())
}

fn parse_card(word: &str) -> Card {
    word.parse()
        .unwrap_or_else(|_| panic!("unparseable card {word:?}"))
}

fn parse_hand(words: &[&str]) -> Hand {
    words.iter().map(|word| parse_card(word)).collect()
}

/// One hand's public record, as our seat saw it
#[derive(Debug, Clone)]
struct Deal {
    direction: PassDirection,
    /// Our 13 cards before the exchange
    dealt: Hand,
    passed: Option<Hand>,
    received: Option<Hand>,
    /// Every card played this hand, in order, ours included
    plays: Vec<(Seat, Card)>,
}

impl Deal {
    const fn new(direction: PassDirection) -> Self {
        Self {
            direction,
            dealt: Hand::EMPTY,
            passed: None,
            received: None,
            plays: Vec::new(),
        }
    }

    /// A complete round agreeing with this record from our seat, or `None`
    /// if the record contradicts the rules
    ///
    /// Our own cards and passes are real.  Each other seat holds the cards
    /// it has played, plus — for the receiver — our pass, plus unplayed
    /// cards placed by exhaustive randomized backtracking under every
    /// constraint its plays imposed.  The other three passes are arbitrary
    /// triples, since no seat sees them.
    fn placeholder(&self, rng: &mut (impl Rng + ?Sized)) -> Option<Round> {
        let rules = Rules::new();
        let (Some(passed), Some(received)) = (self.passed, self.received) else {
            if self.direction != PassDirection::Hold {
                // Still passing: deal the others any 13 cards apiece.
                let mut hands = [Hand::EMPTY; 4];
                hands[ME as usize] = self.dealt;
                let rest = shuffled(Hand::ALL - self.dealt, rng);
                for (seat, chunk) in Seat::ALL
                    .into_iter()
                    .filter(|&s| s != ME)
                    .zip(rest.chunks(13))
                {
                    hands[seat as usize] = chunk.iter().copied().collect();
                }
                return Round::from_deal(rules, self.direction, hands).ok();
            }
            return self.replay(
                Round::from_deal(rules, self.direction, self.post_pass(rng, Hand::EMPTY)?).ok()?,
            );
        };

        let post = self.post_pass(rng, passed)?;
        let d = self.direction;
        let mut out = [Hand::EMPTY; 4];
        out[ME as usize] = passed;
        out[d.giver(ME) as usize] = received;
        for seat in Seat::ALL {
            if out[seat as usize].is_empty() {
                // Any three of the receiver's post-pass cards: each seat
                // receives from exactly one giver, so these never collide.
                out[seat as usize] = post[d.receiver(seat) as usize]
                    .into_iter()
                    .take(3)
                    .collect();
            }
        }
        let pre =
            Seat::ALL.map(|s| (post[s as usize] - out[d.giver(s) as usize]) | out[s as usize]);
        let mut round = Round::from_deal(rules, d, pre).ok()?;
        for seat in Seat::ALL {
            round.pass(seat, out[seat as usize]).ok()?;
        }
        self.replay(round)
    }

    /// Post-exchange hands for all four seats: ours real, the others'
    /// sampled around their pinned cards
    fn post_pass(&self, rng: &mut (impl Rng + ?Sized), passed: Hand) -> Option<[Hand; 4]> {
        let mut hands = [Hand::EMPTY; 4];
        hands[ME as usize] = (self.dealt - passed) | self.received.unwrap_or(Hand::EMPTY);
        hands[self.direction.receiver(ME) as usize] |= passed;
        let mut allowed = [Hand::ALL; 4];
        allowed[ME as usize] = Hand::EMPTY;

        let mut hearts_broken = false;
        for (i, trick) in self.plays.chunks(4).enumerate() {
            let (_, lead) = trick[0];
            for (j, &(seat, card)) in trick.iter().enumerate() {
                hands[seat as usize].insert(card);
                let can = &mut allowed[seat as usize];
                if j == 0 {
                    // A heart led unbroken: nothing but hearts was left.
                    if card.suit == Suit::Hearts && !hearts_broken {
                        *can &= Hand::HEARTS;
                    }
                } else if card.suit != lead.suit {
                    *can -= suit_cards(lead.suit);
                    // Points on trick 1: nothing else was left.
                    if i == 0 && card.points() > 0 {
                        *can &= Hand::PENALTIES;
                    }
                }
            }
            hearts_broken |= trick.iter().any(|&(_, card)| card.suit == Suit::Hearts);
        }

        let pool = Hand::ALL - hands.iter().fold(Hand::EMPTY, |all, &hand| all | hand);
        let mut room = hands.map(|hand| 13 - hand.len());
        room[ME as usize] = 0;
        place(&shuffled(pool, rng), &allowed, &mut hands, &mut room, rng).then_some(hands)
    }

    fn replay(&self, mut round: Round) -> Option<Round> {
        for &(seat, card) in &self.plays {
            round.play(seat, card).ok()?;
        }
        Some(round)
    }
}

fn suit_cards(suit: Suit) -> Hand {
    let mut hand = Hand::EMPTY;
    hand[suit] = hearts::Holding::ALL;
    hand
}

fn shuffled(hand: Hand, rng: &mut (impl Rng + ?Sized)) -> Vec<Card> {
    let mut cards: Vec<Card> = hand.into_iter().collect();
    for i in (1..cards.len()).rev() {
        cards.swap(i, rng.random_range(0..=i));
    }
    cards
}

/// Whether `cards` fit into `room` under `allowed`
///
/// Hall's condition, from the cards' side: every set of seats has room
/// for the cards that may go nowhere else.  With as many cards as room in
/// total, that is exactly when a placement exists.
fn feasible(cards: &[Card], allowed: &[Hand; 4], room: &[usize; 4]) -> bool {
    (1..16u8).all(|seats| {
        let inside = |s: usize| seats & 1 << s != 0;
        let space: usize = (0..4).filter(|&s| inside(s)).map(|s| room[s]).sum();
        let confined = cards
            .iter()
            .filter(|&&card| (0..4).all(|s| inside(s) || !allowed[s].contains(card)))
            .count();
        confined <= space
    })
}

/// Place `cards` into `hands` within `room` and `allowed`, each card at a
/// random seat that keeps the rest [`feasible`] — so it never backtracks,
/// and fails only when no placement exists at all
fn place(
    cards: &[Card],
    allowed: &[Hand; 4],
    hands: &mut [Hand; 4],
    room: &mut [usize; 4],
    rng: &mut (impl Rng + ?Sized),
) -> bool {
    if !feasible(cards, allowed, room) {
        return false;
    }
    for (i, &card) in cards.iter().enumerate() {
        let mut order = Seat::ALL;
        for j in (1..4).rev() {
            order.swap(j, rng.random_range(0..=j));
        }
        let seat = order
            .into_iter()
            .map(|seat| seat as usize)
            .find(|&s| {
                room[s] > 0 && allowed[s].contains(card) && {
                    room[s] -= 1;
                    let ok = feasible(&cards[i + 1..], allowed, room);
                    room[s] += 1;
                    ok
                }
            })
            .expect("a feasible placement has a feasible next step");
        room[seat] -= 1;
        hands[seat].insert(card);
    }
    true
}

/// The two kinds of decision, whose worlds cost differently
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Pass,
    Play,
}

/// The measured worst fuel per world for a `kind` decision with
/// `tricks_left` to roll out
fn per_world(kind: Kind, tricks_left: u32) -> f64 {
    match kind {
        Kind::Pass => PASS_PER_WORLD,
        Kind::Play => PLAY_PER_WORLD + PLAY_PER_WORLD_PER_TRICK * f64::from(tricks_left),
    }
}

/// Plans each decision's world count from the fuel the arena reports
///
/// Fuel counts WebAssembly instructions and the clock is stopped, so the
/// only meter is the pair of figures on each prompt.  The governor prices
/// a world at [`per_world`] times a safety scale per kind, which starts at
/// 1 and only rises: a decision's bill is how far the hand's pool fell by
/// our next prompt, and one dearer than its price raises the scale to
/// match.  The plan stays within [`FUEL_SHARE`] of the decision's ceiling
/// and of the hand's pool, the latter after reserving [`MIN_SAMPLES`]
/// full-hand worlds for every later decision.
#[derive(Debug, Clone)]
struct Governor {
    /// Safety scale over [`per_world`], indexed by `Kind as usize`
    scale: [f64; 2],
    /// The decision awaiting its bill: its kind, its price at scale 1, and
    /// the hand's pool at its prompt
    pending: Option<(Kind, f64, u64)>,
}

impl Governor {
    const fn new() -> Self {
        Self {
            scale: [1.0; 2],
            pending: None,
        }
    }

    /// A new hand refills the pool, so an unbilled decision stays unbilled.
    const fn new_hand(&mut self) {
        self.pending = None;
    }

    /// Bill the previous decision, then plan this one: how many worlds a
    /// `kind` decision with `tricks_left` may sample with `decision` and
    /// `hand` fuel left and `remaining` decisions in the hand, this one
    /// included
    fn plan(
        &mut self,
        kind: Kind,
        tricks_left: u32,
        decision: u64,
        hand: u64,
        remaining: u32,
    ) -> u32 {
        if let Some((previous, price, before)) = self.pending.take() {
            let ratio = before.saturating_sub(hand) as f64 / price;
            eprintln!("bill {previous:?} {ratio:.3}");
            let scale = &mut self.scale[previous as usize];
            *scale = scale.max(ratio);
        }
        let future = f64::from(remaining.saturating_sub(1) * MIN_SAMPLES);
        let reserve = future * per_world(Kind::Play, 13) * self.scale[Kind::Play as usize];
        let fuel = (decision as f64).min(hand as f64 - reserve) * FUEL_SHARE;
        let price = per_world(kind, tricks_left);
        let worlds = (fuel / (price * self.scale[kind as usize]))
            .clamp(f64::from(MIN_SAMPLES), f64::from(MAX_SAMPLES)) as u32;
        self.pending = Some((kind, price * f64::from(worlds), hand));
        worlds
    }
}

/// The arena seat: protocol state plus the bot
struct ArenaBot {
    /// `None` only while a decision has it out to set its world count
    bot: Option<MonteCarloBot<StdRng>>,
    governor: Governor,
    rng: StdRng,
    /// Game totals, clockwise from us as the protocol sends them
    scores: [u16; 4],
    deal: Deal,
}

impl ArenaBot {
    fn new(seed: u64) -> Self {
        Self {
            // The arena rates wins and nothing else, so second is worth
            // what last is.  Its field defends a moon far better than
            // the rollouts' plain defenders do (a fifth of our shots land
            // there), so the shoot worlds' defenders keep their stoppers.
            bot: Some(
                MonteCarloBot::new(StdRng::seed_from_u64(seed))
                    .win_only(true)
                    .rollout_stoppers(true),
            ),
            governor: Governor::new(),
            rng: StdRng::seed_from_u64(seed ^ PLACEHOLDER_SALT),
            scores: [0; 4],
            deal: Deal::new(PassDirection::Hold),
        }
    }

    fn table(&mut self) -> Table {
        let round = self
            .deal
            .placeholder(&mut self.rng)
            .unwrap_or_else(|| panic!("no round fits the record {:?}", self.deal));
        let mut scores = [0; 4];
        for (rel, &score) in self.scores.iter().enumerate() {
            scores[seat(rel) as usize] = score;
        }
        Table::new(round).scores(scores)
    }

    /// Size the bot for this prompt's fuel, then let `decide` consult it
    fn decide<T>(
        &mut self,
        kind: Kind,
        fuel: &[&str],
        decide: impl FnOnce(&mut MonteCarloBot<StdRng>, &Table) -> T,
    ) -> T {
        let [decision, hand] = [0, 1].map(|i| fuel[i].parse().expect("a 64-bit fuel figure"));
        let table = self.table();
        let tricks_left = 13 - self.deal.plays.len() as u32 / 4;
        let ours = self
            .deal
            .plays
            .iter()
            .filter(|&&(seat, _)| seat == ME)
            .count() as u32;
        let remaining = 13 - ours + u32::from(kind == Kind::Pass);
        let worlds = self
            .governor
            .plan(kind, tricks_left, decision, hand, remaining);
        let scale = self.governor.scale[kind as usize];
        eprintln!("{kind:?}: {worlds} worlds, {tricks_left} tricks left, scale {scale:.2}");
        let mut bot = self.bot.take().expect("the bot is home").samples(worlds);
        let moons = (bot.moon_attempts(), bot.moon_passes());
        let choice = decide(&mut bot, &table);
        if (bot.moon_attempts(), bot.moon_passes()) != moons {
            // Read back by scripts/arena-census.py from a replayed transcript.
            let (moons, worlds) = bot.moon_share();
            eprintln!("shoot {moons}/{worlds}");
        }
        self.bot = Some(bot);
        choice
    }

    /// Absorb one arena line, returning our reply to a prompt
    fn hear(&mut self, words: &[&str]) -> Option<String> {
        match words {
            ["HAND", _, direction] => {
                self.deal = Deal::new(direction.parse().expect("a pass direction"));
                self.governor.new_hand();
            }
            ["CARDS", cards @ ..] => self.deal.dealt = parse_hand(cards),
            ["SCORES", scores @ ..] => {
                for (slot, score) in self.scores.iter_mut().zip(scores) {
                    *slot = score.parse().expect("a score");
                }
            }
            ["RECEIVED", cards @ ..] => self.deal.received = Some(parse_hand(cards)),
            ["PLAYED", rel, card] => {
                let rel = rel.parse().expect("a seat");
                self.deal.plays.push((seat(rel), parse_card(card)));
            }
            ["PASS_REQUEST", fuel @ ..] => {
                let cards = self.decide(Kind::Pass, fuel, |bot, table| {
                    bot.pass_cards(&table.view(ME))
                });
                self.deal.passed = Some(cards.into_iter().collect());
                let [a, b, c] = cards.map(code);
                return Some(format!("PASS {a} {b} {c}"));
            }
            ["YOUR_TURN", fuel @ ..] => {
                let card = self.decide(Kind::Play, fuel, |bot, table| {
                    bot.play_card(&table.view(ME))
                });
                self.deal.plays.push((ME, card));
                return Some(format!("PLAY {}", code(card)));
            }
            _ => {}
        }
        None
    }
}

fn main() {
    let mut bot = None;
    for line in io::stdin().lock().lines() {
        let line = line.expect("stdin is readable");
        let words: Vec<&str> = line.split_whitespace().collect();
        let reply = match words.as_slice() {
            ["INIT", _, seed, rules] => {
                assert_eq!(*rules, RULES_ID, "this bot plays {RULES_ID} only");
                bot = Some(ArenaBot::new(seed.parse().expect("a 64-bit seed")));
                None
            }
            ["GAME_END", ..] => break,
            _ => bot.as_mut().expect("INIT comes first").hear(&words),
        };
        if let Some(reply) = reply {
            let mut out = io::stdout().lock();
            writeln!(out, "{reply}").expect("stdout is writable");
            out.flush().expect("stdout is writable");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearts::Phase;
    use hearts_engine::{HeuristicBot, View};

    /// Everything `View` exposes, from both rounds, must agree.
    fn assert_same_view(real: &View<'_>, fake: &View<'_>) {
        assert_eq!(real.phase(), fake.phase());
        assert_eq!(real.hand(), fake.hand());
        assert_eq!(real.legal_plays(), fake.legal_plays());
        assert_eq!(real.turn(), fake.turn());
        assert_eq!(real.current_trick(), fake.current_trick());
        assert_eq!(real.tricks(), fake.tricks());
        assert_eq!(real.hearts_broken(), fake.hearts_broken());
        assert_eq!(real.passed(), fake.passed());
        assert_eq!(real.received(), fake.received());
        assert_eq!(real.unseen(), fake.unseen());
        for seat in Seat::ALL {
            assert_eq!(real.points_taken(seat), fake.points_taken(seat));
            // `Table` serializes passes, so seats before ours already
            // hold 10 in the real round: a driver artifact, not knowledge.
            if real.phase() != Phase::Passing {
                assert_eq!(real.hand_len(seat), fake.hand_len(seat));
            }
            assert_eq!(real.known_cards(seat), fake.known_cards(seat));
            assert_eq!(real.possible_cards(seat), fake.possible_cards(seat));
            for suit in Suit::ASC {
                assert_eq!(real.is_void(seat, suit), fake.is_void(seat, suit));
            }
        }
    }

    /// The record our seat would have built from the arena's lines
    fn record(round: &Round, dealt: Hand) -> Deal {
        let mut deal = Deal::new(round.direction());
        deal.dealt = dealt;
        if round.phase() != Phase::Passing {
            deal.passed = round.passed(ME);
            deal.received = round.passed(round.direction().giver(ME));
        }
        let tricks = round.tricks().iter().copied().chain(round.current_trick());
        deal.plays = tricks.flat_map(|trick| trick.plays()).collect();
        deal
    }

    #[test]
    fn seats_run_clockwise_from_us() {
        assert_eq!(seat(0), ME);
        assert_eq!(seat(1), ME.left());
        assert_eq!(seat(2), ME.across());
        assert_eq!(seat(3), ME.right());
    }

    #[test]
    fn cards_round_trip_through_the_protocol() {
        for card in Hand::ALL {
            assert_eq!(parse_card(&code(card)), card);
        }
        assert_eq!(code(parse_card("TH")), "TH");
    }

    /// At every one of our decisions across many greedy deals, the
    /// placeholder round shows our seat exactly what the real one does.
    #[test]
    fn placeholder_views_match_the_real_round() {
        let mut rng = StdRng::seed_from_u64(7);
        for deal_no in 0..400 {
            let direction = PassDirection::from_deal_index(deal_no);
            let mut table = Table::deal(Rules::new(), direction, &mut rng);
            let dealt = table.round().hand(ME);
            let mut bots = [(); 4].map(|()| HeuristicBot::new());
            while let Some(turn) = table.turn() {
                if turn == ME {
                    let fake = record(table.round(), dealt)
                        .placeholder(&mut rng)
                        .expect("the real deal fits its own record");
                    assert_same_view(&table.view(ME), &Table::new(fake).view(ME));
                }
                table
                    .step(&mut bots[turn as usize])
                    .expect("greedy plays legally");
            }
        }
    }

    fn cards(codes: &str) -> Hand {
        parse_hand(&codes.split_whitespace().collect::<Vec<_>>())
    }

    /// Play `plays` on a hold deal of `hands` (North, East, South, West),
    /// then check many placeholders against the real view at our turn.
    fn check_corner(hands: [&str; 4], plays: &[(Seat, &str)]) {
        let hands = hands.map(cards);
        let mut round = Round::from_deal(Rules::new(), PassDirection::Hold, hands).unwrap();
        for &(seat, card) in plays {
            round.play(seat, parse_card(card)).unwrap();
        }
        assert_eq!(round.turn(), Some(ME));
        let real = Table::new(round.clone());
        let mut rng = StdRng::seed_from_u64(1);
        for _ in 0..200 {
            let fake = record(&round, hands[ME as usize])
                .placeholder(&mut rng)
                .expect("the real deal fits its own record");
            assert_same_view(&real.view(ME), &Table::new(fake).view(ME));
        }
    }

    /// The void table cannot say "nothing but penalty cards": East sheds
    /// a heart on trick 1, which only an all-penalty hand may.
    #[test]
    fn points_on_trick_one_pin_an_all_penalty_hand() {
        check_corner(
            [
                "2C 3C 4C 5C 2H 2D 3D 4D 5D 6D 7D 8D 9D",
                "QS 3H 4H 5H 6H 7H 8H 9H TH JH QH KH AH",
                "6C 7C 8C 9C TC TD JD QD KD AD 2S 3S 4S",
                "JC QC KC AC 5S 6S 7S 8S 9S TS JS KS AS",
            ],
            &[(Seat::North, "2C"), (Seat::East, "3H")],
        );
    }

    /// Nor "nothing but hearts": West leads a heart before hearts break.
    #[test]
    fn an_unbroken_heart_lead_pins_an_all_heart_hand() {
        check_corner(
            [
                "2C 3C 4C 2D 3D 4D 5D 6D 7D 8D 9D TD JD",
                "5C 6C 7C 2H QD KD AD 2S 3S 4S 5S 6S 7S",
                "8C 9C TC JC QC KC 8S 9S TS JS QS KS AS",
                "AC 3H 4H 5H 6H 7H 8H 9H TH JH QH KH AH",
            ],
            &[
                (Seat::North, "2C"),
                (Seat::East, "5C"),
                (Seat::South, "8C"),
                (Seat::West, "AC"),
                (Seat::West, "3H"),
                (Seat::North, "2D"),
                (Seat::East, "2H"),
            ],
        );
    }

    #[test]
    fn a_full_pool_buys_the_ceiling_share() {
        let mut governor = Governor::new();
        // 0.6 × 2e9 over 5.8e6 for a pass, over 1.3e6 + 13 × 1.5e5 for a play
        assert_eq!(
            governor.plan(Kind::Pass, 13, 2_000_000_000, 10_000_000_000, 14),
            206
        );
        governor.new_hand();
        assert_eq!(
            governor.plan(Kind::Play, 13, 2_000_000_000, 10_000_000_000, 13),
            369
        );
        governor.new_hand();
        assert_eq!(
            governor.plan(Kind::Play, 3, 2_000_000_000, 10_000_000_000, 3),
            685
        );
    }

    #[test]
    fn a_dear_bill_raises_the_scale() {
        let mut governor = Governor::new();
        assert_eq!(
            governor.plan(Kind::Play, 13, 2_000_000_000, 10_000_000_000, 13),
            369
        );
        // Priced at 369 × 3.25e6 = 1.19925e9; billed twice that.
        let next = governor.plan(Kind::Play, 13, 2_000_000_000, 7_601_500_000, 12);
        assert_eq!(next, 184);
        // A cheap bill never lowers it.
        assert_eq!(
            governor.plan(Kind::Play, 13, 2_000_000_000, 7_600_000_000, 11),
            184
        );
    }

    #[test]
    fn a_new_hand_leaves_the_last_play_unbilled() {
        let mut governor = Governor::new();
        governor.plan(Kind::Play, 1, 2_000_000_000, 1_000_000_000, 1);
        governor.new_hand();
        assert_eq!(
            governor.plan(Kind::Pass, 13, 2_000_000_000, 9_000_000_000, 14),
            206
        );
        assert_eq!(governor.scale, [1.0; 2]);
    }

    #[test]
    fn a_drained_pool_keeps_the_floor() {
        let mut governor = Governor::new();
        // 16 full-hand worlds for each of 9 later plays reserve 468e6.
        assert_eq!(
            governor.plan(Kind::Play, 4, 2_000_000_000, 250_000_000, 10),
            16
        );
        // Priced at 16 × 1.9e6 = 30.4e6, billed 100e6: scale 3.29, so the
        // last play buys 0.6 × 150e6 / (1.45e6 × 3.29) worlds.
        assert_eq!(
            governor.plan(Kind::Play, 1, 2_000_000_000, 150_000_000, 1),
            18
        );
    }
}
