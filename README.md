# Hearts Engine

[![Crates.io](https://img.shields.io/crates/v/hearts-engine)](https://crates.io/crates/hearts-engine)
[![Docs.rs](https://docs.rs/hearts-engine/badge.svg)](https://docs.rs/hearts-engine)
[![Build Status](https://github.com/jdh8/hearts-engine/actions/workflows/rust.yml/badge.svg)](https://github.com/jdh8/hearts-engine)

Bots and strategy tooling for
[Hearts](https://www.pagat.com/reverse/hearts.html), built on the [hearts]
mechanics crate.  Where hearts answers *"what plays are legal?"*, this crate answers
*"which card should I play?"* — plus a wasm front end you can play in the
browser at <https://jdh8.github.io/hearts-engine/>.

The design triangle:

- [`Strategy`]: a decision procedure for one seat — which three cards to
  pass, which card to play.
- [`View`]: the information a seat may legally see.  The underlying
  [`Round`] exposes all four hands and every pass; strategies never touch
  it.  A `View` shows only the seat's own hand, the public trick history,
  its own pass and (after the exchange) what it received, plus the
  common-knowledge inferences: who is void in what, and where the passed
  cards went.
- [`Table`]: the driver.  It owns the `Round`, tracks the void table, asks
  strategies for decisions, and applies them — so information hygiene holds
  by construction.

## Bots

- [`HeuristicBot`]: deterministic and fast.  Passes its most dangerous
  cards (an unguarded Q♠ first), ducks under the trick winner, dumps the
  queen and high hearts when void, and stops ducking to kill a suspected
  moon.
- [`MonteCarloBot`] (feature `rand`): determinized Monte Carlo.  At each
  decision it samples hidden hands consistent with the `View` — voids
  respected, its own passed cards pinned to the receiver — replays the
  public history into a real [`Round`] per world, rolls each out with the
  greedy policy, and picks the action with the best expected game equity.

## Quick start

A bot-vs-bot round needs no features:

```rust
use hearts::{Hand, PassDirection, Round, Rules};
use hearts_engine::{HeuristicBot, play_round};

// The sorted deck dealt round-robin, one card per seat.
let mut hands = [Hand::EMPTY; 4];
for (i, card) in Hand::ALL.into_iter().enumerate() {
    hands[i % 4].insert(card);
}
let round = Round::from_deal(Rules::new(), PassDirection::Hold, hands)?;

let [mut n, mut e, mut s, mut w] = [HeuristicBot::new(); 4];
let result = play_round(round, [&mut n, &mut e, &mut s, &mut w])?;
println!("{:?}", result.points());
# Ok::<(), Box<dyn std::error::Error>>(())
```

With the (default) `rand` feature, deal and settle whole games:

```rust
# #[cfg(feature = "rand")]
# fn main() -> Result<(), hearts_engine::EngineError> {
use hearts::{Game, Rules};
use hearts_engine::{HeuristicBot, MonteCarloBot, play_game};

let mut game = Game::new(Rules::default());
let [mut n, mut e, mut s] = [HeuristicBot::new(); 3];
let mut mc = MonteCarloBot::new(rand::rng()).samples(16);
let score = play_game(&mut game, [&mut n, &mut e, &mut s, &mut mc], &mut rand::rng())?;
println!("{:?}", score.totals);
# Ok(())
# }
# #[cfg(not(feature = "rand"))]
# fn main() {}
```

Writing your own bot is implementing [`Strategy`]'s two decisions against a
[`View`]; the driver handles all bookkeeping.

## Feature flags

- `rand` (default): the Monte Carlo bot, `Table::deal`, `play_game`, and
  the examples.  Disable it for a dependency-free heuristic-only build.
- `parallel`: Monte Carlo rollouts across the CPU cores via rayon.
  Decisions are bit-identical to the serial build, each just arrives
  faster; worthwhile at high sample counts.  Off by default.

## Examples

- `play`: play against three bots in the terminal —
  `cargo run --example play` (`--bot mc:128`, `--seed 7`, `hint` at the
  prompt, …)
- `arena`: bot-vs-bot tournaments with win-rate statistics —
  `cargo run --release --example arena -- --games 200 greedy greedy greedy mc:64`

[hearts]: https://crates.io/crates/hearts
[`Strategy`]: https://docs.rs/hearts-engine/latest/hearts_engine/trait.Strategy.html
[`View`]: https://docs.rs/hearts-engine/latest/hearts_engine/struct.View.html
[`Table`]: https://docs.rs/hearts-engine/latest/hearts_engine/struct.Table.html
[`HeuristicBot`]: https://docs.rs/hearts-engine/latest/hearts_engine/struct.HeuristicBot.html
[`MonteCarloBot`]: https://docs.rs/hearts-engine/latest/hearts_engine/struct.MonteCarloBot.html
[`Round`]: https://docs.rs/hearts/latest/hearts/round/struct.Round.html
