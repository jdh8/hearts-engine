//! The [`Strategy`] trait: a decision procedure for one seat

use crate::View;
use hearts::Card;

/// A decision procedure for one seat of Hearts
///
/// Every method receives a [`View`] restricted to the information the seat
/// may legally see.  Methods take `&mut self` so strategies can keep state —
/// an internal random number generator, an opponent model — and the trait is
/// object-safe, so the driver works with `&mut dyn Strategy`.
///
/// A strategy never applies its decisions itself; the [`Table`] driver
/// validates and applies them, rejecting illegal choices as
/// [`EngineError::IllegalAction`].
///
/// [`Table`]: crate::Table
/// [`EngineError::IllegalAction`]: crate::EngineError::IllegalAction
pub trait Strategy {
    /// Choose three distinct cards from [`View::hand`] to pass
    ///
    /// Consulted once per seat on a passing deal, before any card is
    /// played; [`View::received`] is still `None` at that point.
    fn pass_cards(&mut self, view: &View<'_>) -> [Card; 3];

    /// Choose a card from [`View::legal_plays`] to play
    fn play_card(&mut self, view: &View<'_>) -> Card;

    /// A display name for tournament output
    fn name(&self) -> &str {
        "unnamed"
    }
}
