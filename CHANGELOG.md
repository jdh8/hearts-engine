# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
