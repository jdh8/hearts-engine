//! The [`Table`] driver: applies strategy decisions to a round
//!
//! The driver owns the [`Round`] and the common-knowledge void table, asks
//! the acting strategy for one decision at a time, validates it, applies
//! it, and keeps the void inference current — so information hygiene holds
//! by construction for any [`Strategy`].

use crate::{Strategy, View};
use hearts::round::RoundError;
use hearts::{Hand, Phase, Round, RoundResult, Seat};
use thiserror::Error;

/// An error while driving a round
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EngineError {
    /// A strategy chose an action the round rejected
    #[error("{seat} chose an illegal action")]
    IllegalAction {
        /// The seat whose strategy misbehaved
        seat: Seat,
        /// The round's rejection
        #[source]
        source: RoundError,
    },
}

/// A round in progress, with common-knowledge void tracking
///
/// [`Table::round`] intentionally exposes the full position — user
/// interfaces and loggers need it — but strategies only ever receive the
/// [`View`]s handed to them by [`Table::step`].
#[derive(Debug, Clone)]
pub struct Table {
    round: Round,
    /// Bit `suit as u8` of `voids[seat as usize]`: seat shown void in suit.
    voids: [u8; 4],
    scores: [u16; 4],
}

/// Recover the void table from the public trick history.
fn infer_voids(round: &Round) -> [u8; 4] {
    let mut voids = [0u8; 4];
    for trick in round.tricks().iter().copied().chain(round.current_trick()) {
        let Some(led) = trick.suit() else { continue };
        for (seat, card) in trick.plays() {
            if card.suit != led {
                voids[seat as usize] |= 1 << led as u8;
            }
        }
    }
    voids
}

impl Table {
    /// Wrap a round, recovering void inference from its history
    ///
    /// Unlike gin rummy, every observation in Hearts is public, so a table
    /// can pick up a round in any phase.
    ///
    /// The game score defaults to level (all seats at zero), so a
    /// standalone round reports [`game_scores`](View::game_scores) of
    /// `[0; 4]`.  Set it with [`Table::scores`] when driving a round within
    /// a game.
    #[must_use]
    pub fn new(round: Round) -> Self {
        Self {
            voids: infer_voids(&round),
            round,
            scores: [0; 4],
        }
    }

    /// Set the running game totals, indexed by [`Seat`]
    ///
    /// Each seat's [`View`] then reports the seat-relative
    /// [`game_scores`](View::game_scores), letting score-aware strategies
    /// hunt the leader or duck for the endgame.
    #[must_use]
    pub const fn scores(mut self, scores: [u16; 4]) -> Self {
        self.scores = scores;
        self
    }

    /// Shuffle and deal a fresh round onto a new table
    #[cfg(feature = "rand")]
    #[must_use]
    pub fn deal(
        rules: hearts::Rules,
        direction: hearts::PassDirection,
        rng: &mut (impl rand::Rng + ?Sized),
    ) -> Self {
        Self::new(Round::deal(rules, direction, rng))
    }

    /// The underlying round, fully visible
    #[must_use]
    pub const fn round(&self) -> &Round {
        &self.round
    }

    /// The seat the driver acts for next, or `None` when the round is over
    ///
    /// During the passing phase — where seats act concurrently in real play
    /// — the driver serializes deterministically: the first unpassed seat
    /// in [`Seat::ALL`] order.
    #[must_use]
    pub fn turn(&self) -> Option<Seat> {
        match self.round.phase() {
            Phase::Passing => Seat::ALL
                .into_iter()
                .find(|&seat| self.round.passed(seat).is_none()),
            Phase::Playing => self.round.turn(),
            Phase::Finished => None,
        }
    }

    /// Whether every penalty card already sits in a completed trick, freezing
    /// the round's point tally
    ///
    /// True only mid-play: once it holds, the remaining tricks are all
    /// scoreless and cannot change any seat's result, so an interactive caller
    /// may offer to end the round and jump straight to the outcome.  Reads the
    /// won tricks, not [`Round::played`], so it stays false while the last
    /// penalty card is still in the live, unresolved trick.
    #[must_use]
    pub fn points_settled(&self) -> bool {
        self.round.phase() == Phase::Playing
            && Seat::ALL
                .into_iter()
                .map(|seat| u16::from(self.round.points_taken(seat)))
                .sum::<u16>()
                == 26
    }

    /// The legally visible information for one seat
    #[must_use]
    pub const fn view(&self, seat: Seat) -> View<'_> {
        let scores = [
            self.scores[seat as usize],
            self.scores[seat.left() as usize],
            self.scores[seat.across() as usize],
            self.scores[seat.right() as usize],
        ];
        View::new(&self.round, seat, &self.voids, scores)
    }

    /// Ask `strategy` — which must belong to the seat to act (see
    /// [`Table::turn`]) — for one decision and apply it
    ///
    /// Returns the result once the round finishes, `None` while it
    /// continues.
    ///
    /// # Errors
    ///
    /// [`EngineError::IllegalAction`] when the round rejects the strategy's
    /// choice — including a pass of non-distinct cards.  The table is left
    /// unchanged, so an interactive caller may retry the same decision.
    pub fn step(
        &mut self,
        strategy: &mut dyn Strategy,
    ) -> Result<Option<RoundResult>, EngineError> {
        let Some(seat) = self.turn() else {
            return Ok(self.round.result());
        };
        let reject = |source| EngineError::IllegalAction { seat, source };

        match self.round.phase() {
            Phase::Passing => {
                // Duplicate cards collapse in the set and fail the
                // three-card check inside the round.
                let cards: Hand = strategy.pass_cards(&self.view(seat)).into_iter().collect();
                self.round.pass(seat, cards).map_err(reject)?;
            }
            Phase::Playing => {
                let card = strategy.play_card(&self.view(seat));
                let led = self.round.current_trick().and_then(|trick| trick.suit());
                self.round.play(seat, card).map_err(reject)?;
                if let Some(led) = led
                    && card.suit != led
                {
                    self.voids[seat as usize] |= 1 << led as u8;
                }
            }
            Phase::Finished => {}
        }
        Ok(self.round.result())
    }

    /// Drive the round to completion, one strategy per seat
    ///
    /// # Errors
    ///
    /// [`EngineError::IllegalAction`] when a strategy's choice is rejected.
    pub fn play(&mut self, strategies: [&mut dyn Strategy; 4]) -> Result<RoundResult, EngineError> {
        loop {
            let Some(seat) = self.turn() else {
                return Ok(self.round.result().expect("a turnless round is finished"));
            };
            if let Some(result) = self.step(&mut *strategies[seat as usize])? {
                return Ok(result);
            }
        }
    }
}

/// Play one round to completion, one strategy per seat, indexed by [`Seat`]
///
/// # Errors
///
/// [`EngineError::IllegalAction`] when a strategy's choice is rejected.
pub fn play_round(
    round: Round,
    strategies: [&mut dyn Strategy; 4],
) -> Result<RoundResult, EngineError> {
    Table::new(round).play(strategies)
}

/// Deal and play rounds until the game is over, returning the settled score
///
/// # Errors
///
/// [`EngineError::IllegalAction`] when a strategy's choice is rejected.
/// The game keeps the rounds recorded so far.
#[cfg(feature = "rand")]
pub fn play_game(
    game: &mut hearts::Game,
    strategies: [&mut dyn Strategy; 4],
    rng: &mut (impl rand::Rng + ?Sized),
) -> Result<hearts::FinalScore, EngineError> {
    let [north, east, south, west] = strategies;
    while !game.is_over() {
        let mut table = Table::new(game.deal(rng)).scores(game.scores());
        let result = table.play([&mut *north, &mut *east, &mut *south, &mut *west])?;
        game.record(result)
            .expect("a result produced by the round it was dealt for records cleanly");
    }
    Ok(game.final_score().expect("a game that is over settles"))
}
