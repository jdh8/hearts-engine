#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

pub use hearts;

mod driver;
mod heuristic;
#[cfg(feature = "rand")]
mod mc;
mod strategy;
mod view;

#[cfg(feature = "rand")]
pub use driver::play_game;
pub use driver::{EngineError, Table, play_round};
pub use heuristic::{HeuristicBot, HeuristicConfig};
#[cfg(feature = "rand")]
pub use mc::{Assessment, MonteCarloBot};
pub use strategy::Strategy;
pub use view::View;
