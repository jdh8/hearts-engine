# hearts-engine

Bots and strategy tooling for Hearts over the sibling
[hearts](https://crates.io/crates/hearts) mechanics crate, mirroring the
architecture of [gin-rummy-engine](https://crates.io/crates/gin-rummy-engine):
the `Strategy` trait + information-hygienic `View` + `Table` driver
triangle, a heuristic bot whose knowledge-free core doubles as the Monte
Carlo rollout policy, and a determinized `MonteCarloBot`.  The `web/`
subdirectory is a non-published wasm front end deployed to GitHub Pages.

Two deliberate simplifications over the gin engine — do not reintroduce
what they removed:

- **No `Knowledge` ledger.**  Everything a Hearts seat may know is
  derivable from the public history the `Round` already retains (passes
  are kept, all plays are public).  The only cached inference is the
  common-knowledge void table on `Table`; `Table::new` recovers it from
  any mid-round `Round` via `infer_voids`.
- **No `sim.rs`.**  Monte Carlo worlds are real `hearts::Round`s: sample
  hidden hands, rebuild the *original* hands (current ∪ own plays), deal
  them as a `PassDirection::Hold` round, and replay the public history.
  The rules can never drift from the rollouts because they are the same
  code.  `tests/`' reconstruction test guards this.

## Map of the crate

- `src/lib.rs` — re-exports; `pub use hearts;` so downstreams need one dep.
- `src/strategy.rs` — `Strategy`: `pass_cards -> [Card; 3]`,
  `play_card -> Card`, object-safe.
- `src/view.rs` — `View`: the legal whitelist (own hand, history, own
  pass/received, `is_void`, `known_cards`, `unseen`, `possible_cards`,
  seat-relative `game_scores` clockwise from the seat).
- `src/driver.rs` — `Table` (round + voids + scores), `step`/`play`,
  `play_round`, rand-gated `play_game`; `EngineError::IllegalAction`
  leaves the table untouched so retries work.  Passing turn order is the
  first unpassed seat in `Seat::ALL`.
- `src/heuristic.rs` — `pass_score`/`greedy_pass`/`greedy_play` (the
  knowledge-free core, `pub(crate)`, shared with rollouts) and
  `HeuristicBot` with the knowledge-based moon-defense overlay (OFF in
  rollouts).
- `src/mc.rs` — `MonteCarloBot`: `sample_hands` (randomized
  most-constrained-first backtracking under voids/known/capacities),
  world reconstruction, candidate generation (plays collapsed by
  rank-adjacency, passes = all 20 triples of the top-6 `pass_score`
  cards), common-random-number batches with 2-SE challenger elimination,
  `assess()`.  `Assessment::ev` is expected round points — LOWER is
  better, unlike gin's signed gain.
- `tests/` — `view` (hygiene, void soundness, unseen identity, score
  rotation), `driver` (cheater rejection + retry, whole games),
  `proptest` (seeded termination + point conservation), `strength`
  (`#[ignore]`d mc-vs-greedy tripwire).
- `examples/` — `play` (terminal, human = South), `arena`, `tune`.
- `web/` — nested crate with its own workspace; see `web/README.md` and
  the wasm traps below.

## Invariants

- Strategies never see a `Round`; only `Table` hands out `View`s.  A new
  `View` accessor must be information a seat may legally see.
- The void table is *sound*, never complete: `is_void` true implies the
  seat is genuinely void.  Update it in exactly one place (`Table::step`)
  and recover it in `infer_voids` the same way.
- Monte Carlo candidate 0 is the greedy incumbent; the bot deviates only
  past the 2-SE paired gate in `beats`.  The `parallel` feature must stay
  bit-identical: batch results are collected in world order and reduced
  sequentially.
- Equity is `1/k` for a k-way game win, 0 for a loss, else
  `0.5 + margin/112` pinned inside (¼, ¾) — a guaranteed gap below any
  clinch.
- `Table::step` must leave the table untouched on `IllegalAction`.

## wasm traps (web/)

- `web/.cargo/config.toml` pins
  `[target.wasm32-unknown-unknown] rustflags = ["-Ctarget-feature=+reference-types"]`
  — a global `-Ctarget-cpu=native` otherwise leaks into the cross-compile
  and breaks wasm-bindgen's externref transform.  Do not delete it.
- `getrandom = { version = "0.4", features = ["wasm_js"] }` as a
  wasm-only dependency.
- The web crate must depend on the SAME `hearts` version as the engine,
  or path/registry duplicates make `Card` types unequal.
- Build with `wasm-pack build --release --target web`; serve `web/` and
  click through a deal before shipping UI changes.

## Verification — mirror CI before declaring work done

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo test --all-features
cargo test                        # default features (serial MC leg)
cargo check --no-default-features # the rand-free surface still compiles
```

When `Cargo.toml` dependencies change, also run the minimal-versions
check: `cargo +nightly update -Z direct-minimal-versions && cargo +nightly
check --all-features --all-targets`, then restore `Cargo.lock` (it is
committed).  The strength tripwire is manual:
`cargo test --release --test strength -- --ignored`.

## After updating the codebase

- Format with `cargo fmt`; run the gate above.
- Update [CHANGELOG.md](CHANGELOG.md) (Keep-a-Changelog; pending section
  `## [X.Y.Z] — Unreleased`, em dash; maintainer-only changes under
  `### Internal`).
- Propose a clear and descriptive commit message.
