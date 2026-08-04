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
- `MonteCarloBot::gate` sets how many paired standard errors a challenger
  must clear before the bot deviates from the greedy incumbent, next to
  `samples()`.

### Changed

- Let the Monte Carlo pass search consider passes that actually empty every
  1–3 card non-spade suit, supplementing the 20 triples drawn from its six
  highest independently scored cards.  Those scores could award three void
  bonuses to three different suits while making no void reachable.  Over
  6,195 paired duplicate deals, the new candidates improve `mc:128` by
  `+0.0817 ± 0.0164` points/deal, `+0.0083 ± 0.0018` matchpoint rank, and
  `+0.0038 ± 0.0008` win equity; moon attempts rise by `0.19 ± 0.04`
  percentage points.
- Make the shared greedy play/rollout policy point-aware.  Last to a clean
  trick it now takes control with the cheapest winner, while it still ducks
  a trick carrying penalties; on lead it works its shortest suit instead of
  letting global card order favor clubs.  For `mc:128` this improves the
  paired matchpoint rank payoff by `+0.0359 ± 0.0114` (3.2 SE) and the point
  margin by `+0.205 ± 0.095` per deal, with win equity unchanged
  (`+0.0020 ± 0.0050`).  Moon attempts fall by `0.39 ± 0.17` percentage
  points.
- The default significance gate drops from 2.0 to 1.5 standard errors,
  worth `+0.182 ± 0.046` points a deal for `mc:128` on paired duplicate
  deals.  The 2.0 was a multiplicity correction — several challengers get
  tested per decision — and the harness showed it over-corrects: strength
  is monotone falling in the threshold across `[1.0, 2.5]`, and 1.5
  recovers about half of what eight times the rollouts buy while keeping
  some of the correction (1.0 vs 1.5 is unresolved at 2000 blocks).  The
  moon-attempt rate is unaffected (`+0.3 SE`), because shoot candidates
  clear a separate Bernoulli-majority bar.
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
  than Easy.  The four tiers now measure at 5.2 / 19.8 / 34.0 / 41.0 %
  of games won in a shared lineup, so the ladder is real; Expert stops at
  `mc:128` for latency, not for strength, since `mc:1024` does play
  measurably better (see the diagnostics below).

### Internal

- Rebuild `examples/arena` as a duplicate-deal harness.  The unit of
  measurement is now a *block*: one deal — or one game seed — played four
  times with the lineup rotated, so every bot plays all four hands of it.
  That is duplicate bridge in the only form a four-player free-for-all
  admits, there being no sides to swap.  Deal seeds are a pure function of
  `(--seed, block)` and every bot is built inside its trial, seeded from
  the deal rather than from the deal stream, so two runs at one seed are
  paired card for card — even across rebuilds that differ by a private
  `const` — and blocks parallelize with no effect on the output.

  The old harness could not pair even in principle: it drew `mc` seeds
  from the same RNG that then dealt the cards, and only for `mc` bots, so
  two lineups at one `--seed` played different deals.  Its
  `rotation = index % 4` shared a period *and* a phase with
  `PassDirection::from_deal_index`, locking each bot to one pass direction
  per seat.  And it credited every tied winner, so its four win rates
  could sum past 100 % and its Wilson interval was mis-specified.  New
  flags `--ab SPEC` (rerun the same blocks with SPEC in slot 1, report the
  paired delta) and `--csv` (one row per block per bot, for pairing two
  builds); `--rounds N` becomes `--blocks N`; `newbie` — the web UI's
  shipped Easy tier — is playable at last.

  It reports five columns rather than one.  `points` is the zero-sum
  `mean(others) − own` that the Deep CFR harness already used, and is the
  search signal.  The rest are the payoffs a four-player game leaves open
  where a two-player one does not: with two players "seek the win" and
  "avoid the loss" are the same sentence, with four they are different
  objectives with different optimal play, and the rules pin down only one
  of them.  So `win` (`1-0-0-0`, the shipped rule, `1/k` on a shared win
  as `FinalScore::winners` has it), `not-last` (`1-1-1-0`) and `rank`
  (`3-2-1-0` matchpoints, tied ranks averaged) are all reported side by
  side.  They cost nothing once the cards are played, and their
  disagreement is the point — a change that lifts `points` while dropping
  `win` is reducing variance in a way the game does not reward.

  Two invariants keep it honest.  Every column but `moons` is
  constant-sum, asserted per block, so a wrong tie convention or a
  rotation that lost a bot fails immediately instead of printing a
  plausible number three hours later.  And a homogeneous *deterministic*
  field is exactly degenerate: four identical bots play the same round in
  all four rotations, so `arena greedy greedy greedy greedy` must print
  `0.000±0.000` with no residual variance at all.  It does, which is the
  end-to-end proof that the four rotations really are the same deal and
  that the bot-to-seat attribution is right.
- First measurements off that harness.  They are the point of the rebuild,
  so they are recorded rather than summarised.  All are paired over
  identical deals at 2000 blocks (8000 rounds) against three `HeuristicBot`s
  unless noted.

  *Search width is not saturated.*  `mc:1024` beats `mc:128` by
  `+0.394 ± 0.069` points a deal — 5.7 SE, with `win`, `not-last` and
  `rank` moving with it at 5.4, 6.3 and 7.4 SE.  The two were previously
  called indistinguishable; that was a harness that could not resolve
  them, not a fact about the bots.  Moons are the exception, moving
  `−1.5 SE`, so the gap to Deep CFR's ~18 % is not a sample-count problem.

  *The significance gate is set too high.*  Sweeping the `2.0` in `beats`,
  strength is monotone in the threshold: `1.0` is worth `+0.216 ± 0.061`
  points a deal and `1.5` `+0.182 ± 0.046`, while `2.5` costs
  `−0.272 ± 0.047`.  The `2.0` is a multiplicity correction and it
  over-corrects, by about half of what eight times the rollouts buy.
  Lowering it does not reopen the moon-attempt problem, because the
  Bernoulli-majority bar on shoot candidates is a separate filter: at a
  gate of `1.0` the moon rate moves `+0.3 SE`.

  *The candidate cap is a non-event.*  Raising `MAX_CANDIDATES` from 8 to
  13 is worth `−0.003 ± 0.003` points a deal, with a paired difference SD
  of 0.13 against the 3.09 of the `mc:1024` comparison — the two builds
  agree on very nearly every decision.  Rank-adjacency collapse leaves
  more than eight classes so rarely that the truncation almost never
  fires, systematic though its suit order is.

  *The noise floor.*  A homogeneous `mc:128` field lands within 1.3 SE of
  level on every column, and the block-level payoff SD is 3.94 points a
  deal: resolving 0.2 takes 1549 blocks, 0.1 takes 6195.  Seeding the bots
  from the deal instead of the deal stream cuts the paired difference SD
  from the 5.57 two independent runs would carry to 3.09 — 3.2× fewer
  blocks for the same resolving power.

  *Points buy wins at about 0.15 each*, measured at the `mc:64`→`mc:128`
  rung, so a percentage point of games won costs 0.067 points a deal.  A
  game-level confirmation therefore costs about 1.5× the rounds of the
  points signal for an equivalent effect rather than the 4-5× a
  per-observation count suggests, because a per-round edge compounds over
  the ~10.6 rounds of a game.  Against three greedy bots `mc:128` wins
  67.3 % of games.

  *The rank payoffs never disagree about which bot is stronger*, at any
  rung of the ladder — but they do not saturate together.  `mc:128` has
  taken 76 % of its headroom above a fair share on `not-last` against
  56 % on `win`, and the marginal conversion rotates as strength rises:
  low on the ladder a point of improvement buys mostly *not coming last*,
  high on it mostly *coming first*.  The bot is further along the "don't
  lose" axis than the "win" axis, which is what a points-linear objective
  predicts of it, and what headroom remains is on the `win` axis.
- `rank` is now the primary A/B detector.  In every comparison above that
  carried real signal it was the most sensitive column — 7.4 vs 5.7 SE on
  the width test, 4.6-6.0 vs 3.6-5.8 across the gate sweep — the ordinary
  Wilcoxon-vs-*t* efficiency gain of a bounded rank statistic over a
  heavy-tailed margin, and a moon is a ±78-point tail.  `points` stays the
  interpretable magnitude, `win` the ship gate.
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
  recommended play.  The strength diagnostics below now put that last
  clause in doubt — `mc:1024` outplays `mc:128` by a wide margin — so the
  hint's sample count is due a proper re-measurement; the latency argument
  for capping it stands either way.

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
