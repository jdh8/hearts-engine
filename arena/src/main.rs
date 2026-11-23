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

/// The arena seat: protocol state plus the bot
struct ArenaBot {
    bot: MonteCarloBot<StdRng>,
    rng: StdRng,
    /// Game totals, clockwise from us as the protocol sends them
    scores: [u16; 4],
    deal: Deal,
}

impl ArenaBot {
    fn new(seed: u64) -> Self {
        let samples = option_env!("ARENA_SAMPLES")
            .map_or(128, |n| n.parse().expect("ARENA_SAMPLES is a count"));
        Self {
            bot: MonteCarloBot::new(StdRng::seed_from_u64(seed)).samples(samples),
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

    /// Absorb one arena line, returning our reply to a prompt
    fn hear(&mut self, words: &[&str]) -> Option<String> {
        match words {
            ["HAND", _, direction] => {
                self.deal = Deal::new(direction.parse().expect("a pass direction"));
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
            ["PASS_REQUEST", ..] => {
                let table = self.table();
                let cards = self.bot.pass_cards(&table.view(ME));
                self.deal.passed = Some(cards.into_iter().collect());
                let [a, b, c] = cards.map(code);
                return Some(format!("PASS {a} {b} {c}"));
            }
            ["YOUR_TURN", ..] => {
                let table = self.table();
                let card = self.bot.play_card(&table.view(ME));
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
}
