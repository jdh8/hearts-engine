# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `MonteCarloBot` can now shoot the moon.  Every decision carries one extra
  candidate rolled with the deciding seat shooting — cash the highest card
  when leading, top the led suit when that wins, shed the cheapest
  non-penalty card when void — and the other three defending instead of
  ducking, plus one moon pass built from the three cards the pass policy
  least wants gone.  It is taken only when the rollouts actually reach the
  moon in a majority of worlds, and then carried to the end of the round.
  Against three `HeuristicBot`s over two paired blocks of 2000 rounds,
  `mc:128` shoots 85 of 4000 deals where it used to shoot 36, and widens its
  margin over the field from 3.18 to 3.62 points a round on the first block
  and from 3.49 to 3.59 on the second, and takes 211 of 300 paired games
  instead of 202.  Every measurement moves the same way and none regresses,
  but each gain on its own is inside the noise; the moon rate is the
  unambiguous change, and the point of it is the seat that is not a
  `HeuristicBot` — the greedy field never shoots and only defends past 8
  points, so it cannot punish a bad attempt the way Deep CFR does.

  Both extra conditions were paid for in measurement.  The significance gate
  alone attempted a moon in two rounds in five to land one in twenty, at a
  cost of over two points a round: a moon candidate's per-world equities are
  bimodal, and a paired test on heavy tails waves through edges that are not
  there.  A moon rate is a plain Bernoulli count, and its break-even is
  arithmetic — a moon is worth `0.5 + 26/112` mid-game against about `0.423`
  for a failed shot and `0.577` for the greedy line, which puts indifference
  within a hair of an even chance.  Re-deciding the shot each trick was
  worse still, overriding it in three committed decisions in ten: the
  rollouts price thirteen tricks of shooting, so a bot that wanders back to
  the greedy line has taken the points and bought nothing.
- The web UI plays a short synthesized sting when hearts break and an
  ominous one when Q♠ hits the table (WebAudio, no sound assets).
- The stings now have a visual counterpart for players who can't hear
  them: a large 💔 pops over the table when hearts break, a ♠ when the
  Q♠ lands.  Under `prefers-reduced-motion` the glyph fades in place
  instead of scaling — it stays visible because it is the accessible
  channel.
- A speaker button in the web UI header mutes the stings (keyboard `m`),
  remembered across sessions in `localStorage`.  The visual counterparts
  keep firing while muted.
- Offer to end a round early once all 26 points are captured.  The web UI
  shows an "End round" button and the terminal `play` example asks before the
  next human play, both jumping straight to the round result.  The leftover
  scoreless tricks are drained through the real engine, so the outcome is
  identical.  New predicate `Table::points_settled`.

### Changed

- The offer to end a settled round now stands until it is taken.  The web
  UI's "End round" button lives outside the per-frame action box and shows
  whenever the points are settled — including while the bots are playing, so
  it can be clicked out of turn — and the terminal `play` example re-asks on
  each human turn instead of once per round.
- `MonteCarloBot` now defends against the moon at its live decision,
  reusing the same reactive overlay as `HeuristicBot` (break a lone
  opponent's sweep once they pass 8 points).  Its rollout policy models
  every opponent as a greedy ducker who never shoots, so the search was
  blind to a moon in progress — in the Deep CFR tournament mc:128 was shot
  the moon in 142 of 800 deals.  The overlay draws no randomness, so
  seeded play stays reproducible, and the strength tripwire confirms it
  costs nothing against greedy opponents.
- Retune the web stings: hearts break now sounds like shattering glass (a
  noise crack plus inharmonic high partials) and the Q♠ dyad switches to
  sawtooth with a boosted peak, compensating for the ear's reduced
  sensitivity near 130 Hz.
- Remap the web difficulty menu to bots that are actually distinct in
  strength: Easy `newbie`, Medium `mc:16`, Hard `mc:64`, Expert `mc:128`
  (matching the hint solver).  The old Medium `greedy` was no stronger
  than Easy, and Hard `mc:128`/Expert `mc:1024` were indistinguishable.

### Internal

- Cross-engine tournament harness against the Deep CFR player of
  [brianberns/Hearts](https://github.com/brianberns/Hearts), the strongest
  open-source Hearts bot we know of: an F# shim (`tournament/CfrShim`)
  rebuilds his `InformationSet` from our public history and queries his
  model over Fable.Remoting, and `examples/vs_cfr.rs` mirrors his 2v2
  duplicate-deal benchmark with paired standard errors, cross-checking both
  engines' legal-action sets on every decision.  Excluded from the
  published crate: his repo is GPL3, so it is referenced in place as a
  sibling clone, never vendored, to keep the copyleft off our tree.
- Harden the tournament run against Brian's live server: the shim retries
  `GetActionIndex` on transient HTTP timeouts, and the harness checkpoints
  running results (payoff, points, moons) to stderr every 20 pairs so a
  dropped connection never loses a multi-hour run.  First result over 800
  throttled deals: his Deep CFR beats `mc:128` by +1.28 ± 0.21 payoff per
  seat-deal, shooting the moon in ~18 % of deals to our ~1 %.
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
