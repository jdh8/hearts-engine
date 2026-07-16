# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The web UI plays a short synthesized sting when hearts break and an
  ominous one when Q♠ hits the table (WebAudio, no sound assets).
- Offer to end a round early once all 26 points are captured.  The web UI
  shows an "End round" button and the terminal `play` example asks before the
  next human play, both jumping straight to the round result.  The leftover
  scoreless tricks are drained through the real engine, so the outcome is
  identical.  New predicate `Table::points_settled`.

### Changed

- Remap the web difficulty menu to bots that are actually distinct in
  strength: Easy `newbie`, Medium `mc:16`, Hard `mc:64`, Expert `mc:128`
  (matching the hint solver).  The old Medium `greedy` was no stronger
  than Easy, and Hard `mc:128`/Expert `mc:1024` were indistinguishable.

### Internal

- Fix the web hint at 128 sampled worlds instead of adapting it upward to
  2048; the extra worlds cost latency with no measurable change in the
  recommended play.

### Fixed

- The solver/hint no longer refuses a read when every legal play collapses
  into one equivalence class; it lists the interchangeable plays tied, since
  a human cannot tell from the hand alone that the ranks between them are
  already gone.
- Monte Carlo candidate collapse no longer treats rank-adjacent cards of
  unequal penalty value as interchangeable, so Q♠ is weighed as its own play
  and the solver/hint stops refusing a J♠/Q♠ choice.

## [0.1.0] — 2026-07-15

### Changed

- Use the published `hearts` 0.1.0 crate instead of a sibling path dependency.
- Raise the web crate's `wasm-bindgen` lower bound to the version required by
  its `getrandom` backend.
- Simplify the web cards to the flat, four-colour faces and straight, fully
  visible player hand used by the gin-rummy engine.
- Align the numeric columns in the web solver's hint table.

### Added

- The design triangle: the `Strategy` trait (pass three cards, play a
  card), the information-hygienic `View` (own hand, public history, pass
  knowledge, common-knowledge voids), and the `Table` driver that
  validates and applies decisions.
- `HeuristicBot`: a deterministic knowledge-based player with tunable
  `HeuristicConfig { moon_defense, void_weight }`; its knowledge-free core
  doubles as the Monte Carlo rollout policy.
- `MonteCarloBot` (feature `rand`): determinized Monte Carlo over sampled
  worlds reconstructed as real `hearts::Round`s, with common random
  numbers, growing batches with paired-test elimination, and `assess()`
  for solver/hint views.
- Feature `parallel`: rayon rollouts, bit-identical to serial decisions.
- Examples: `play` (terminal game vs bots with hints), `arena`
  (tournaments with Wilson intervals), `tune` (config sweeps).
- `web/`: a wasm front end deployed to
  <https://jdh8.github.io/hearts-engine/>.
