//! Browser front end for hearts-engine.
//!
//! The whole game already lives in the engine — [`Table`] drives it and
//! validates every action — so this crate only replaces the terminal I/O of
//! `examples/play.rs` with a JSON snapshot plus one method per human
//! decision.  The human seat (South) is a one-shot [`Strategy`]
//! ([`Pending`]): the UI sets the action for the current decision point and
//! the driver is stepped once, so a browser can collect each move as a
//! click without blocking.

use hearts::{Card, FinalScore, Game, Hand, PassDirection, Phase, RoundResult, Rules, Seat, Trick};
use hearts_engine::{
    Assessment, HeuristicBot, HeuristicConfig, MonteCarloBot, Strategy, Table, View,
};
use rand::rngs::StdRng;
use rand::{RngExt as _, SeedableRng};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// The human sits South, at the bottom of the screen.
const HUMAN: Seat = Seat::South;

/// Screen order clockwise from the human: South, West, North, East —
/// matching [`View::game_scores`].
const RELATIVE: [Seat; 4] = [Seat::South, Seat::West, Seat::North, Seat::East];

/// The screen index of a seat: 0 = you, 1 = left, 2 = across, 3 = right.
fn relative(seat: Seat) -> usize {
    RELATIVE
        .iter()
        .position(|&s| s == seat)
        .expect("every seat has a screen position")
}

/// The on-screen name of a seat.
fn who(seat: Seat) -> &'static str {
    match seat {
        Seat::South => "You",
        Seat::West => "West",
        Seat::North => "North",
        Seat::East => "East",
    }
}

// ---------------------------------------------------------------------------
// The human seat as a one-shot strategy
// ---------------------------------------------------------------------------

/// One human decision, staged by the UI before the driver is stepped.
enum HumanAction {
    Pass([Card; 3]),
    Play(Card),
}

/// The human seat: returns whatever action the UI staged for the current
/// phase.
#[derive(Default)]
struct Pending {
    action: Option<HumanAction>,
}

impl Strategy for Pending {
    fn pass_cards(&mut self, _view: &View<'_>) -> [Card; 3] {
        match self.action.take() {
            Some(HumanAction::Pass(cards)) => cards,
            _ => unreachable!("pass step without a staged pass"),
        }
    }

    fn play_card(&mut self, _view: &View<'_>) -> Card {
        match self.action.take() {
            Some(HumanAction::Play(card)) => card,
            _ => unreachable!("play step without a staged card"),
        }
    }

    fn name(&self) -> &str {
        "you"
    }
}

// ---------------------------------------------------------------------------
// The game, driven one decision at a time
// ---------------------------------------------------------------------------

/// A whole game in progress: the driver, the three bots, and the human's
/// staged action, plus the running transcript.  Split out from the wasm
/// wrapper so it can be driven directly in a native test.
struct Core {
    game: Game,
    table: Table,
    /// One bot per seat; the human slot is never consulted.
    bots: [Box<dyn Strategy>; 4],
    pending: Pending,
    rng: StdRng,
    /// A seed stream for the Hint button, apart from the deal so asking for
    /// a hint never disturbs the game's own randomness.
    hint_rng: StdRng,
    round_no: u32,
    log: Vec<String>,
    /// Cumulative totals after each recorded round, for the score sheet.
    score_sheet: Vec<[u16; 4]>,
    last_result: Option<RoundResult>,
    last_move: Option<Move>,
    settled: Option<FinalScore>,
    /// A round finished and the game continues: the showdown is on screen
    /// and we wait for the player's Continue click before the next deal.
    awaiting_continue: bool,
}

impl Core {
    fn new(bot_spec: &str, target: &str, seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut rules = Rules::new();
        if let Ok(target) = target.parse::<u16>()
            && target > 0
        {
            rules.game_target = target;
        }
        let bots = [
            make_bot(bot_spec, &mut rng),
            make_bot(bot_spec, &mut rng),
            make_bot("greedy", &mut rng), // the human slot, never consulted
            make_bot(bot_spec, &mut rng),
        ];
        let game = Game::new(rules);
        let table = Table::new(game.deal(&mut rng)).scores(game.scores());
        let direction = table.round().direction();
        Self {
            game,
            table,
            bots,
            pending: Pending::default(),
            rng,
            // A distinct stream: xoring keeps the first hint seed from
            // colliding with the bots' own MC seeds drawn from `rng`.
            hint_rng: StdRng::seed_from_u64(seed ^ 0x4849_4E54),
            round_no: 1,
            log: vec![round_header(1, direction)],
            score_sheet: Vec::new(),
            last_result: None,
            last_move: None,
            settled: None,
            awaiting_continue: false,
        }
    }

    // --- human decisions -------------------------------------------------

    /// Stage the human's pass, e.g. from `"QS AH 2C"`.
    fn pass_cards(&mut self, codes: &str) {
        let cards: Vec<Card> = codes.split_whitespace().flat_map(str::parse).collect();
        let Ok(cards) = <[Card; 3]>::try_from(cards) else {
            self.log.push("Pick exactly three cards to pass.".into());
            return;
        };
        self.human(HumanAction::Pass(cards));
    }

    /// Stage the human's play.
    fn play(&mut self, code: &str) {
        if let Ok(card) = code.parse::<Card>() {
            self.human(HumanAction::Play(card));
        }
    }

    /// Stage a human action and step the driver once.  The bots' replies are
    /// *not* run here: the UI ticks [`Core::step_once`] itself, pacing and
    /// animating each following action.
    fn human(&mut self, action: HumanAction) {
        // Guard the phase and the turn before staging: stepping [`Pending`]
        // with a mismatched action would panic and abort the wasm module.
        let phase = self.table.round().phase();
        let staged_fits = matches!(
            (&action, phase),
            (HumanAction::Pass(_), Phase::Passing) | (HumanAction::Play(_), Phase::Playing)
        );
        if !staged_fits || self.table.turn() != Some(HUMAN) {
            self.log.push("Not your move right now.".into());
            return;
        }

        let before = self.table.round().clone();
        self.last_move = None;
        self.pending.action = Some(action);
        if let Err(e) = self.table.step(&mut self.pending) {
            // A well-behaved UI only offers legal moves; surface the rest.
            self.log.push(format!("Illegal: {e}"));
            self.pending.action = None;
            return;
        }
        self.narrate(HUMAN, &before);
        self.last_move = self.move_for(HUMAN, &before);
    }

    /// Advance the game by at most one visible action: one bot move, one of
    /// the human's forced steps, or finishing a round.  Does nothing once
    /// the human has a real decision or the game is settled — the UI paces
    /// calls to this, animating the move each one reports.
    fn step_once(&mut self) {
        let Some(seat) = self.table.turn() else {
            self.last_move = None;
            if self.settled.is_none() && !self.awaiting_continue {
                self.record_round();
            }
            return;
        };

        let before = self.table.round().clone();
        if seat == HUMAN {
            // The only forced human step is a single legal card.
            let legal = self.table.view(HUMAN).legal_plays();
            if self.table.round().phase() == Phase::Playing && legal.len() == 1 {
                let card = legal.into_iter().next().expect("checked non-empty");
                self.pending.action = Some(HumanAction::Play(card));
                let _ = self.table.step(&mut self.pending);
                self.log.push(format!("You play {card} (forced)."));
                // Narrate after the log line so a completed trick's
                // winner-credit still lands.
                self.narrate(HUMAN, &before);
                self.last_move = self.move_for(HUMAN, &before);
            }
            return;
        }

        self.table
            .step(&mut *self.bots[seat as usize])
            .expect("the bots always choose legal actions");
        self.narrate(seat, &before);
        self.last_move = self.move_for(seat, &before);
    }

    /// Whether the human has a genuine choice pending.
    fn awaiting_human_input(&self) -> bool {
        if self.settled.is_some() || self.awaiting_continue || self.table.turn() != Some(HUMAN) {
            return false;
        }
        match self.table.round().phase() {
            Phase::Passing => true,
            Phase::Playing => self.table.view(HUMAN).legal_plays().len() > 1,
            Phase::Finished => false,
        }
    }

    /// A solver read on the human's current decision at `samples` worlds.
    /// Empty unless the human has a genuine choice pending.
    fn hint(&mut self, samples: u32) -> Vec<Assessment> {
        if !self.awaiting_human_input() {
            return Vec::new();
        }
        let seed = self.hint_rng.random();
        MonteCarloBot::new(StdRng::seed_from_u64(seed))
            .samples(samples)
            .assess(&self.table.view(HUMAN))
    }

    /// The move just applied, for the UI to animate.
    fn move_for(&self, seat: Seat, before: &hearts::Round) -> Option<Move> {
        let round = self.table.round();
        match before.phase() {
            Phase::Passing => Some(Move {
                actor: relative(seat),
                kind: if round.phase() == Phase::Playing {
                    "exchange"
                } else {
                    "pass"
                },
                card: None,
            }),
            Phase::Playing => Some(Move {
                actor: relative(seat),
                kind: "play",
                card: round
                    .played()
                    .into_iter()
                    .find(|&card| !before.played().contains(card))
                    .map(|card| card.to_string()),
            }),
            Phase::Finished => None,
        }
    }

    /// Narrate a step from public information alone.
    fn narrate(&mut self, seat: Seat, before: &hearts::Round) {
        let round = self.table.round();
        match before.phase() {
            Phase::Passing => {
                if seat != HUMAN {
                    self.log.push(format!("{} passes 3 cards.", who(seat)));
                } else {
                    self.log.push("You pass 3 cards.".into());
                }
                if round.phase() == Phase::Playing {
                    self.log
                        .push(format!("Cards exchanged to the {}.", round.direction()));
                }
            }
            Phase::Playing => {
                let played = round.played() - before.played();
                if let Some(card) = played.into_iter().next()
                    && seat != HUMAN
                {
                    self.log.push(format!("{} plays {card}.", who(seat)));
                }
                // A trick just closed: credit its winner.
                if round.tricks().len() > before.tricks().len()
                    && let Some(trick) = round.tricks().last()
                {
                    let winner = trick.winner().expect("a complete trick has a winner");
                    let points = trick.points();
                    self.log.push(if points > 0 {
                        format!("{} takes the trick (+{points}).", who(winner))
                    } else {
                        format!("{} takes the trick.", who(winner))
                    });
                }
            }
            Phase::Finished => {}
        }
    }

    /// Record the finished round, then either settle the game or hold for
    /// the player's Continue click.
    fn record_round(&mut self) {
        let result = self
            .table
            .round()
            .result()
            .expect("a turnless round finished");
        self.last_result = Some(result);
        self.game
            .record(result)
            .expect("a result from the round it was dealt for records cleanly");
        self.score_sheet.push(self.game.scores());

        let points = result.points();
        self.log.push(format!(
            "Round {}: you {}, West {}, North {}, East {}.",
            self.round_no,
            points[HUMAN as usize],
            points[Seat::West as usize],
            points[Seat::North as usize],
            points[Seat::East as usize],
        ));
        if let Some(shooter) = result.shooter() {
            self.log.push(if shooter == HUMAN {
                "You shot the moon!".into()
            } else {
                format!("{} shot the moon!", who(shooter))
            });
        }

        if self.game.is_over() {
            let settled = self
                .game
                .final_score()
                .expect("a game that is over settles");
            self.log.push(final_line(&settled));
            self.settled = Some(settled);
        } else {
            self.awaiting_continue = true;
        }
    }

    /// End a settled round early: drain its remaining scoreless tricks with a
    /// bot and record the outcome, jumping straight to the showdown.  A no-op
    /// until [`Table::points_settled`] holds, so the frozen score carries
    /// through to the same [`RoundResult`] the round would reach by hand.
    fn finish_round(&mut self) {
        if !self.table.points_settled() {
            return;
        }
        // `step` hands the bot the acting seat's view, so any bot plays legally
        // for every remaining seat — the human's included.
        while self.table.turn().is_some() {
            self.table
                .step(&mut *self.bots[0])
                .expect("the bots always choose legal actions");
        }
        self.last_move = None;
        self.record_round();
    }

    /// Clear the showdown and deal the next round.  A no-op unless a
    /// finished round is waiting on the player's Continue click.
    fn next_deal(&mut self) {
        if !self.awaiting_continue {
            return;
        }
        self.awaiting_continue = false;
        self.last_move = None;
        self.last_result = None;
        self.round_no += 1;
        self.table = Table::new(self.game.deal(&mut self.rng)).scores(self.game.scores());
        self.log
            .push(round_header(self.round_no, self.table.round().direction()));
    }

    /// The full legally-visible position, as the UI renders it.
    fn snapshot(&self) -> Snapshot {
        let view = self.table.view(HUMAN);
        let round = self.table.round();
        let legal = view.legal_plays();
        let interactive = self.awaiting_human_input();
        let received = view.received().unwrap_or(Hand::EMPTY);

        let relative_points = |absolute: [u8; 4]| RELATIVE.map(|seat| absolute[seat as usize]);

        Snapshot {
            round_no: self.round_no,
            phase: phase_name(round.phase()),
            pass_direction: round.direction().to_string(),
            your_turn: interactive,
            points_settled: self.table.points_settled(),
            hand: view
                .hand()
                .into_iter()
                .map(|card| CardJson {
                    code: card.to_string(),
                    rank: card.rank.get(),
                    suit: card.suit.letter(),
                    legal: interactive && round.phase() == Phase::Playing && legal.contains(card),
                    received: received.contains(card),
                })
                .collect(),
            trick: trick_slots(round.current_trick()),
            trick_winner: round.current_trick().and_then(Trick::winner).map(relative),
            last_trick: trick_slots(round.tricks().last().copied()),
            last_trick_winner: round
                .tricks()
                .last()
                .and_then(|trick| trick.winner())
                .map(relative),
            hearts_broken: round.hearts_broken(),
            opponents: [Seat::West, Seat::North, Seat::East].map(|seat| Opponent {
                name: who(seat),
                hand_len: view.hand_len(seat),
                tricks: round
                    .tricks()
                    .iter()
                    .filter(|t| t.winner() == Some(seat))
                    .count(),
                points: round.points_taken(seat),
            }),
            round_points: RELATIVE.map(|seat| round.points_taken(seat)),
            // From the game, not the table: the table's copy predates the
            // round whose showdown is on screen.
            scores: RELATIVE.map(|seat| self.game.scores()[seat as usize]),
            score_sheet: self
                .score_sheet
                .iter()
                .map(|totals| RELATIVE.map(|seat| totals[seat as usize]))
                .collect(),
            moon: self
                .last_result
                .and_then(RoundResult::shooter)
                .map(relative),
            round_over: self.awaiting_continue,
            result: self.last_result.map(|r| relative_points(r.points())),
            game_over: self.settled.is_some(),
            winners: self
                .settled
                .as_ref()
                .map(|s| s.winners().map(who).collect())
                .unwrap_or_default(),
            last_move: self.last_move.clone(),
            log: self.log.clone(),
        }
    }
}

/// The current trick as four optional card codes indexed by screen seat.
fn trick_slots(trick: Option<Trick>) -> [Option<String>; 4] {
    let mut slots = [const { None }; 4];
    if let Some(trick) = trick {
        for (seat, card) in trick.plays() {
            slots[relative(seat)] = Some(card.to_string());
        }
    }
    slots
}

// ---------------------------------------------------------------------------
// Snapshot: the JSON the browser renders
// ---------------------------------------------------------------------------

/// One card of the human's hand, ready to render and echo back as `code`.
#[derive(Serialize)]
struct CardJson {
    /// The card's canonical name, e.g. `"T♠"`, parseable back into a `Card`.
    code: String,
    /// Rank as 2–14 (ace high), for sorting and a friendly label.
    rank: u8,
    /// Suit letter `C`/`D`/`H`/`S`, for colour and glyph.
    suit: char,
    /// Whether the card may be played right now (false off turn).
    legal: bool,
    /// Whether the card arrived in this round's exchange.
    received: bool,
}

/// The action just applied, for the UI to animate.
#[derive(Serialize, Clone)]
struct Move {
    /// Screen seat of the actor: 0 = you, 1 = left, 2 = across, 3 = right.
    actor: usize,
    /// `pass` | `exchange` | `play`.
    kind: &'static str,
    /// The played card's `code`; `None` for the face-down pass.
    card: Option<String>,
}

/// One face-down opponent, clockwise from the human's left.
#[derive(Serialize)]
struct Opponent {
    name: &'static str,
    hand_len: usize,
    tricks: usize,
    points: u8,
}

/// Everything the human may legally see, plus the running transcript.
///
/// Seat-indexed arrays are in screen order: 0 = you (South), 1 = left
/// (West), 2 = across (North), 3 = right (East).
#[derive(Serialize)]
struct Snapshot {
    round_no: u32,
    phase: &'static str,
    pass_direction: String,
    your_turn: bool,
    /// Every penalty card is in a completed trick: the round's points are
    /// frozen and the UI may offer to end it early.
    points_settled: bool,
    hand: Vec<CardJson>,
    /// The trick in progress, one optional card code per screen seat.
    trick: [Option<String>; 4],
    /// Who currently wins the trick in progress.
    trick_winner: Option<usize>,
    /// The last completed trick, for the sweep animation.
    last_trick: [Option<String>; 4],
    last_trick_winner: Option<usize>,
    hearts_broken: bool,
    opponents: [Opponent; 3],
    /// Penalty points taken this round, by screen seat.
    round_points: [u8; 4],
    /// Game totals, by screen seat.
    scores: [u16; 4],
    /// Cumulative totals after each finished round, by screen seat.
    score_sheet: Vec<[u16; 4]>,
    /// Screen seat of the moon shooter of the finished round, if any.
    moon: Option<usize>,
    /// A round just finished and the game continues; the UI shows Continue.
    round_over: bool,
    /// The finished round's raw points by screen seat.
    result: Option<[u8; 4]>,
    game_over: bool,
    /// The names of the game's winners (ties share), empty until game over.
    winners: Vec<&'static str>,
    /// The move that produced this snapshot, if any, for animation.
    last_move: Option<Move>,
    log: Vec<String>,
}

/// One candidate move's solver assessment, for the Hint panel.
#[derive(Serialize)]
struct HintJson {
    /// The move label, e.g. `"play Q♠"` or `"pass Q♠ A♥ K♥"`.
    action: String,
    /// Chance to win the game if this move is played, in `[0, 1]`.
    equity: f64,
    /// Expected round points the move costs you — lower is better.
    ev: f64,
    /// Whether this is the bot's own pick.
    recommended: bool,
}

// ---------------------------------------------------------------------------
// The wasm-bindgen wrapper
// ---------------------------------------------------------------------------

/// A Hearts game the browser drives one decision at a time.  Each method
/// applies a move and returns the fresh [`Snapshot`] as a JSON string.
#[wasm_bindgen]
pub struct WebGame {
    core: Core,
}

#[wasm_bindgen]
impl WebGame {
    /// Start a game.  `bot` is `newbie`, `greedy`, or `mc[:samples]`;
    /// `target` is the losing score as a decimal string (empty for the
    /// standard 100); `seed` is a decimal string.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(bot: &str, target: &str, seed: &str) -> Self {
        Self {
            core: Core::new(bot, target, seed.parse().unwrap_or(0)),
        }
    }

    /// The current position as JSON.
    #[must_use]
    pub fn snapshot(&self) -> String {
        json(&self.core.snapshot())
    }

    /// Advance one bot move or forced step and return the fresh snapshot.
    /// The page calls this on a timer while `your_turn` is false, animating
    /// each reported `last_move` between the calls.
    pub fn tick(&mut self) -> String {
        self.core.step_once();
        self.snapshot()
    }

    /// Pass three cards, named by space-separated codes, e.g. `"QS AH 2C"`.
    pub fn pass_cards(&mut self, codes: &str) -> String {
        self.core.pass_cards(codes);
        self.snapshot()
    }

    /// Play the card named by `code` (a value from a `CardJson.code`).
    pub fn play(&mut self, code: &str) -> String {
        self.core.play(code);
        self.snapshot()
    }

    /// Deal the next round after the between-rounds pause.
    pub fn next_deal(&mut self) -> String {
        self.core.next_deal();
        self.snapshot()
    }

    /// End a settled round early: drain its scoreless tricks and jump to the
    /// showdown.  A no-op until the round's points are settled
    /// (`points_settled` in the snapshot).
    pub fn finish_round(&mut self) -> String {
        self.core.finish_round();
        self.snapshot()
    }

    /// A solver read on the current decision as JSON: candidate moves with
    /// equity (chance to win the game) and expected round points (lower is
    /// better), the bot's own pick flagged; empty when the move is forced.
    ///
    /// `samples` is the Monte Carlo world count; the JS side fixes it at
    /// 128, matching the Expert bot (more worlds show no measurable gain).
    #[must_use]
    pub fn hint(&mut self, samples: u32) -> String {
        let rows: Vec<HintJson> = self
            .core
            .hint(samples)
            .into_iter()
            .map(|a| HintJson {
                action: a.action,
                equity: a.equity,
                ev: a.ev,
                recommended: a.recommended,
            })
            .collect();
        json(&rows)
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// Build the bot named by `spec` — the same names `examples/play.rs`
/// accepts, though the two front ends draw their seeds differently.
fn make_bot(spec: &str, rng: &mut StdRng) -> Box<dyn Strategy> {
    let (kind, samples) = match spec.split_once(':') {
        Some((kind, n)) => (kind, n.parse::<u32>().ok()),
        None => (spec, None),
    };
    match kind {
        // A newcomer: no moon awareness, no positional passing.
        "newbie" => {
            let mut config = HeuristicConfig::default();
            config.moon_defense = u8::MAX;
            config.void_weight = 0;
            Box::new(HeuristicBot::with_config(config))
        }
        "greedy" => Box::new(HeuristicBot::new()),
        // Default to the Monte Carlo bot; it needs its own seeded generator.
        _ => Box::new(
            MonteCarloBot::new(StdRng::seed_from_u64(rng.random())).samples(samples.unwrap_or(64)),
        ),
    }
}

fn round_header(number: u32, direction: PassDirection) -> String {
    format!("=== Round {number} (pass {direction}) ===")
}

fn final_line(settled: &FinalScore) -> String {
    let winners: Vec<&str> = settled.winners().map(who).collect();
    format!(
        "Game over — {} win{}.",
        winners.join(" and "),
        if winners.len() == 1 && winners[0] != "You" {
            "s"
        } else {
            ""
        },
    )
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Passing => "passing",
        Phase::Playing => "playing",
        Phase::Finished => "finished",
    }
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("a snapshot serializes")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Tick through bot moves and forced steps to the next human decision,
    /// showdown, or the end of the game.
    fn settle(core: &mut Core) {
        let mut guard = 0;
        while !core.awaiting_human_input() && core.settled.is_none() && !core.awaiting_continue {
            guard += 1;
            assert!(guard < 100_000, "the game must terminate");
            core.step_once();
        }
    }

    /// The human plays every decision greedily.
    fn act_greedily(core: &mut Core) {
        let view = core.table.view(HUMAN);
        match core.table.round().phase() {
            Phase::Passing => {
                let mut greedy = HeuristicBot::new();
                let cards = greedy.pass_cards(&view);
                let codes = cards.map(|c| c.to_string()).join(" ");
                core.pass_cards(&codes);
            }
            Phase::Playing => {
                let mut greedy = HeuristicBot::new();
                let card = greedy.play_card(&view);
                core.play(&card.to_string());
            }
            Phase::Finished => unreachable!("a decision implies a live round"),
        }
    }

    /// Drive one whole game through the public decision methods.
    fn drive_game(seed: u64) -> Core {
        let mut core = Core::new("greedy", "", seed);
        loop {
            settle(&mut core);
            if core.settled.is_some() {
                return core;
            }
            if core.awaiting_continue {
                let showdown = core.snapshot();
                assert!(showdown.round_over && !showdown.game_over && !showdown.your_turn);
                assert!(showdown.result.is_some(), "a finished round has points");
                core.next_deal();
                let dealt = core.snapshot();
                assert!(!dealt.round_over && dealt.result.is_none());
                continue;
            }
            let snap = core.snapshot();
            assert!(snap.your_turn, "awaiting_human_input implies your_turn");
            act_greedily(&mut core);
        }
    }

    #[test]
    fn a_scripted_game_reaches_a_settled_score() {
        let core = drive_game(42);
        let settled = core.settled.expect("drive_game only returns settled");
        assert!(settled.winners().count() >= 1);
        assert!(core.snapshot().game_over);
        assert!(!core.snapshot().winners.is_empty());
        // The score sheet grew one row per recorded round.
        assert_eq!(core.score_sheet.len() as u32, core.game.deals());
    }

    #[test]
    fn snapshots_carry_the_passing_flow() {
        let mut core = Core::new("greedy", "", 1);
        settle(&mut core);
        let snap = core.snapshot();
        assert_eq!(snap.phase, "passing");
        assert!(snap.your_turn);
        assert_eq!(snap.hand.len(), 13);

        // A bad pass is surfaced, not applied.
        core.pass_cards("2C");
        assert!(core.snapshot().log.last().unwrap().contains("three cards"));

        // A genuine pass goes through; received cards are flagged after
        // the exchange.
        act_greedily(&mut core);
        settle(&mut core);
        let snap = core.snapshot();
        assert_eq!(snap.phase, "playing");
        assert_eq!(snap.hand.len(), 13);
        assert_eq!(snap.hand.iter().filter(|c| c.received).count(), 3);
    }

    #[test]
    fn snapshot_arrays_are_in_screen_order_and_fresh() {
        let mut core = Core::new("greedy", "", 42);
        settle(&mut core);
        act_greedily(&mut core);
        loop {
            settle(&mut core);
            if core.awaiting_continue || core.settled.is_some() {
                break;
            }
            act_greedily(&mut core);
        }

        // At the showdown: round_points/result are by SCREEN seat (0 =
        // South), and scores already include the round on display.
        let snap = core.snapshot();
        let round = core.table.round();
        let expected = RELATIVE.map(|seat| round.points_taken(seat));
        assert_eq!(snap.round_points, expected);
        assert_eq!(
            snap.result.unwrap(),
            RELATIVE.map(|seat| core.last_result.unwrap().points()[seat as usize]),
        );
        assert_eq!(
            snap.scores,
            RELATIVE.map(|s| core.game.scores()[s as usize])
        );
        assert_eq!(snap.score_sheet.last().copied(), Some(snap.scores));
    }

    #[test]
    fn cross_phase_actions_are_rejected_not_fatal() {
        let mut core = Core::new("greedy", "", 7);
        settle(&mut core);
        assert_eq!(core.table.round().phase(), Phase::Passing);

        // A play staged during passing must not abort the module.
        let code = core.snapshot().hand[0].code.clone();
        core.play(&code);
        assert!(
            core.snapshot()
                .log
                .last()
                .unwrap()
                .contains("Not your move")
        );

        // Passing still works afterwards.
        act_greedily(&mut core);
        settle(&mut core);
        assert_eq!(core.table.round().phase(), Phase::Playing);

        // And a pass staged during play is equally harmless.
        core.pass_cards("2C 3C 4C");
        assert!(
            core.snapshot()
                .log
                .last()
                .unwrap()
                .contains("Not your move")
        );
    }

    #[test]
    fn hints_rate_a_real_decision() {
        let mut core = Core::new("greedy", "", 42);
        settle(&mut core);
        let hints = core.hint(32);
        assert!(!hints.is_empty(), "the pass is a real choice");
        assert!(hints.iter().all(|h| (0.0..=1.0).contains(&h.equity)));
        assert_eq!(hints.iter().filter(|h| h.recommended).count(), 1);
    }

    #[test]
    fn a_moon_travels_the_whole_web_path() {
        // Greedy self-play moons every dozen games or so; find a seed
        // deterministically rather than pinning one that code drift breaks.
        for seed in 0..500 {
            let core = drive_game(seed);
            if core.log.iter().any(|line| line.contains("shot the moon")) {
                let sheet = &core.score_sheet;
                // Some row charged 26 to exactly three seats.
                let mooned = sheet.iter().enumerate().any(|(i, row)| {
                    let prev = if i == 0 { [0; 4] } else { sheet[i - 1] };
                    let deltas: Vec<u16> = row.iter().zip(prev).map(|(&a, b)| a - b).collect();
                    deltas.iter().filter(|&&d| d == 26).count() == 3 && deltas.contains(&0)
                });
                assert!(mooned, "the moon shows up on the score sheet");
                return;
            }
        }
        panic!("no moon in 500 seeded games");
    }
}
