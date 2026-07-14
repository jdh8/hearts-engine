//! The [`View`]: what one seat may legally see of a round
//!
//! [`Round`](hearts::Round) exposes the whole position — all four hands and
//! every pass — and leaves information hygiene to its consumers.  This
//! module is that hygiene: a [`View`] borrows the round privately and
//! re-exposes only the whitelist of legally visible information.
//!
//! Unlike gin rummy, everything a Hearts seat may know is derivable from
//! the public history the round already retains, so there is no hidden
//! per-seat ledger: the only cached inference is the common-knowledge void
//! table that the [`Table`](crate::Table) maintains (a seat that does not
//! follow suit is void in the led suit, in front of everyone).

use hearts::{Hand, PassDirection, Phase, Round, Rules, Seat, Suit, Trick};

/// What one seat may legally see of a round
///
/// The wrapped [`Round`] is private, and these accessors are the whitelist:
/// the other hands and the pre-exchange passes are structurally
/// unreachable.
pub struct View<'a> {
    round: &'a Round,
    seat: Seat,
    /// Common-knowledge voids: bit `suit as u8` of `voids[seat as usize]`.
    voids: &'a [u8; 4],
    /// Running game totals, clockwise from this seat.
    scores: [u16; 4],
}

impl<'a> View<'a> {
    pub(crate) const fn new(
        round: &'a Round,
        seat: Seat,
        voids: &'a [u8; 4],
        scores: [u16; 4],
    ) -> Self {
        Self {
            round,
            seat,
            voids,
            scores,
        }
    }

    /// The seat this view belongs to
    #[must_use]
    pub const fn seat(&self) -> Seat {
        self.seat
    }

    /// The running game totals, clockwise from this seat:
    /// `[mine, my left, across, my right]`
    ///
    /// Note the order: seat-relative, unlike [`Table::scores`], which is
    /// indexed by [`Seat`].  All totals sit on the scoreboard in plain
    /// sight, so they are part of the legal whitelist.  `[0; 4]` for a
    /// standalone round played outside a [`Game`](hearts::Game); the losing
    /// threshold is [`rules().game_target`](hearts::Rules::game_target).
    ///
    /// [`Table::scores`]: crate::Table::scores
    #[must_use]
    pub const fn game_scores(&self) -> [u16; 4] {
        self.scores
    }

    /// The rules of the round
    #[must_use]
    pub const fn rules(&self) -> &'a Rules {
        self.round.rules()
    }

    /// The pass direction of the round
    #[must_use]
    pub const fn direction(&self) -> PassDirection {
        self.round.direction()
    }

    /// The current phase of the round
    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.round.phase()
    }

    /// This seat's hand
    #[must_use]
    pub const fn hand(&self) -> Hand {
        self.round.hand(self.seat)
    }

    /// The seat to play, or `None` outside the playing phase
    #[must_use]
    pub fn turn(&self) -> Option<Seat> {
        self.round.turn()
    }

    /// The trick in progress; `Some` exactly during [`Phase::Playing`]
    #[must_use]
    pub const fn current_trick(&self) -> Option<Trick> {
        self.round.current_trick()
    }

    /// The completed tricks in play order
    #[must_use]
    pub fn tricks(&self) -> &'a [Trick] {
        self.round.tricks()
    }

    /// Every card played so far, including the trick in progress
    #[must_use]
    pub fn played(&self) -> Hand {
        self.round.played()
    }

    /// Whether a heart has been played, allowing hearts to be led
    #[must_use]
    pub fn hearts_broken(&self) -> bool {
        self.round.hearts_broken()
    }

    /// The cards this seat may legally play right now
    ///
    /// Empty when it is not this seat's turn.
    #[must_use]
    pub fn legal_plays(&self) -> Hand {
        self.round.legal_plays(self.seat)
    }

    /// The penalty points `seat` has taken so far — public bookkeeping
    #[must_use]
    pub fn points_taken(&self, seat: Seat) -> u8 {
        self.round.points_taken(seat)
    }

    /// How many cards `seat` holds — public arithmetic
    #[must_use]
    pub const fn hand_len(&self, seat: Seat) -> usize {
        self.round.hand(seat).len()
    }

    /// The three cards this seat chose to pass, or `None` if it has not
    /// passed (always `None` on a hold deal)
    #[must_use]
    pub const fn passed(&self) -> Option<Hand> {
        self.round.passed(self.seat)
    }

    /// The three cards this seat received in the exchange
    ///
    /// `None` until the exchange happens and on hold deals — a seat never
    /// sees incoming cards while passing is still open.
    #[must_use]
    pub fn received(&self) -> Option<Hand> {
        match self.phase() {
            Phase::Passing => None,
            Phase::Playing | Phase::Finished => {
                self.round.passed(self.direction().giver(self.seat))
            }
        }
    }

    /// Whether `seat` is known to be void in `suit`
    ///
    /// Void inference is common knowledge: everyone sees a seat fail to
    /// follow.  The table is *sound, not complete* — it records only voids
    /// shown in play, so it lags a seat's real hand (including this seat's
    /// own; check `hand()[suit].is_empty()` for that).
    #[must_use]
    pub const fn is_void(&self, seat: Seat, suit: Suit) -> bool {
        self.voids[seat as usize] & 1 << suit as u8 != 0
    }

    /// The unplayed cards this seat *knows* `seat` holds: the passed cards
    /// still in the receiver's hand
    ///
    /// Empty for every seat other than this seat's
    /// [receiver](PassDirection::receiver), and before the exchange.
    #[must_use]
    pub fn known_cards(&self, seat: Seat) -> Hand {
        let receiver = self.direction().receiver(self.seat);
        if seat != self.seat && seat == receiver && self.phase() != Phase::Passing {
            self.passed()
                .map_or(Hand::EMPTY, |passed| passed - self.played())
        } else {
            Hand::EMPTY
        }
    }

    /// The cards this seat cannot locate: the whole deck minus its own
    /// hand, everything played, and its own passed cards
    ///
    /// Exactly the cards a determinizing bot must distribute among the
    /// other hands.  Once play begins, `unseen().len()` equals the other
    /// hands' total minus the cards [`known_cards`](Self::known_cards)
    /// already places.  During the passing phase the identity is offset by
    /// passes in flight — a seat that has passed holds 10 cards while its
    /// 3 hidden passes stay unseen — so a pass-time determinizer should
    /// sample 13-card *pre-pass* hands instead.
    #[must_use]
    pub fn unseen(&self) -> Hand {
        Hand::ALL - self.hand() - self.played() - self.passed().unwrap_or(Hand::EMPTY)
    }

    /// The cards `seat` may hold, as far as this seat can tell
    ///
    /// This seat's own hand for itself; otherwise the unseen cards outside
    /// `seat`'s known voids, plus the [`known_cards`](Self::known_cards).
    #[must_use]
    pub fn possible_cards(&self, seat: Seat) -> Hand {
        if seat == self.seat {
            return self.hand();
        }
        let mut cards = self.unseen();
        for suit in Suit::ASC {
            if self.is_void(seat, suit) {
                cards[suit] = hearts::Holding::EMPTY;
            }
        }
        cards | self.known_cards(seat)
    }
}
