# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The web table shows a static gold arrow during passing that points from
  the player toward West, North, or East, making the current pass direction
  visible on the felt as well as in the instruction text.
- `MonteCarloBot::pass_model` sets the `HeuristicConfig` the search models
  passing with, alongside `samples` and `gate`.  One knob names three roles
  that were each hard-coded to the shipped defaults: the passes the other
  three seats submit in a sampled pass-phase world, the soft likelihood
  that reweights worlds against the pass we actually received, and the
  ranking that orders the candidate triples — including candidate 0, the
  greedy incumbent a challenger must clear the significance gate to
  displace, so the setting moves decisions even where the candidate set
  does not.  A bot configured away from the defaults now ranks, models and
  observes with its own pass policy instead of the shipped one; an
  unconfigured bot decides exactly as before, and the 200-block arena CSV
  is byte-identical across the change.  Play-side knobs are deliberately
  out of reach: `moon_defense` belongs to the live overlay and the rollout
  policy, not to passing.  The arena's `mc` spec carries the same knobs as
  `mc:128,void=2,heart=1,guards=3`, named rather than positional because
  they are a mixed bag and a sweep wants to move one without restating the
  rest; bare `mc:128` keeps its meaning.  Only the leading component may be
  a bare sample count — `mc:128,64` is rejected rather than quietly running
  at 64 worlds, because a spec typo that still parses invalidates whatever
  measurement it was typed for.
- `HeuristicConfig::new`, the shipped defaults in a `const` context, with
  `Default` delegating to it as `hearts::Rules` does.
- `HeuristicConfig` exposes the pass policy's two remaining hand-set
  constants: `heart_weight`, the extra pass-score weight per rank for
  hearts, and `spade_guards`, the low-spade count above which the
  Q♠/A♠/K♠ danger bonuses stop applying.  Together with the existing
  `void_weight` they make every contested `pass_score` term tunable; the
  bonuses' 100/90/80 tier stays fixed because it is quasi-lexicographic —
  its internal order is forced and it only competes with rank-scale terms
  after a ~3× drop.
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
- Shooting the moon now has outcome-specific web feedback: the player gets a
  short firework volley with colourful fireworks and confetti, while an
  opponent gets a softened distant gunshot with a red-and-black impact burst.
  The cue fires once when the showdown first appears, respects the existing
  mute setting, keeps its visual channel while muted, and becomes a stationary
  fade under reduced-motion preferences.
- Offer to end a round early once all 26 points are captured.  The web UI
  shows an "End round" button and the terminal `play` example asks before the
  next human play, both jumping straight to the round result.  The leftover
  scoreless tricks are drained through the real engine, so the outcome is
  identical.  New predicate `Table::points_settled`.
- `MonteCarloBot::gate` sets how many paired standard errors a challenger
  must clear before the bot deviates from the greedy incumbent, next to
  `samples()`.

### Changed

- The pass policy is now **set-aware**: `greedy_pass` picks its three cards
  one at a time, rescoring what is left after each, instead of taking the top
  three of one flat sort of the dealt thirteen.  `pass_score`'s void term
  reads the suit's *current* length, so removing a card escalates the bonus
  along the suit it came from — a doubleton's two cards are worth +4 then +6
  — and a triple that finishes a void now outbids three that merely start
  three.  The flat form scored every card against a hand the pass was about
  to change, and so could collect three void bonuses for emptying nothing:
  on `♠J86 ♥A6 ♦K2 ♣976432` it passed A♥ K♦ J♠ and left both red doubletons
  alive, where the sequential form passes A♥ K♦ 6♥ and opens a real heart
  void.  Against a field of the old policy over 8,000 paired blocks on each
  of three seeds: `rank +0.0100 ± 0.0014` (6.9 SE), `win +0.0042 ± 0.0006`
  (7.3 SE), `points +0.088 ± 0.012`; in games mode, `rank +0.0272 ± 0.0073`.
  The sequential selection carries essentially all of it — measured alone it
  is `rank +0.0112 ± 0.0013` (8.8 SE).  The Monte Carlo leg is a null
  (`rank +0.0022 ± 0.0031` over 6,000 blocks on each of two seeds, no column
  negative): the search already rolls out its candidates, so a better
  incumbent buys it little, and the win is the greedy bot's — which is also
  the rollout policy and the browser's easier tiers.

  Two consequences fell out of the rewrite.  Monte Carlo candidate 0 is now
  *built* rather than inferred: `pass_candidates` calls `greedy_pass` and
  dedups it against the pool, because the sequential policy and the flat
  ranking agree only on the first card, so the "candidate 0 is the greedy
  incumbent" invariant the significance gate defends could no longer be
  inherited from two sorts happening to agree.  And the isolated
  `monte carlo pass, 64 samples` bench rose 44% — a better incumbent leaves
  more pass decisions statistically unresolved, so the adaptive width reaches
  its 3× cap more often — while full-round throughput is unchanged (254 vs
  249 rounds/s across three seeds), because the pass is one decision in
  fourteen.  The `void_weight × spade_guards` re-sweep the unit change owed
  was run and kept both defaults: `void_weight = 1` still beats 0 (−3.05 pp),
  2 (−2.50) and 4 (−5.15), and `spade_guards` 3/4/6 all fall within noise
  of 5.
- Ties in the pass order are now **broken on purpose**.  At the shipped
  `heart_weight = 0` the score is exactly symmetric under permuting ♣/♦/♥,
  and every tie used to fall to whichever suit `Hand`'s iterator reached
  first, which is clubs — an artifact of sort stability worth 0.145 cards a
  pass that nobody designed and nobody had priced.  The replacement reads the
  deck's danger: ♠A/♠K/♠Q first, then hearts, diamonds, clubs, and the spades
  below the queen dead last because they are guards worth keeping.  The key
  is injective over a hand, so no comparison anywhere resolves by position
  and the `parallel` feature's bit-identity no longer depends on a stable
  sort.  **The reordering is a measured null** and is kept for being explicit
  rather than for being better: against the old clubs-first order over 8,000
  blocks on each of three seeds, `rank +0.0013 ± 0.0013` (1.0 SE) and
  `win +0.0008 ± 0.0005` (1.5 SE), against `not-last −0.0017 ± 0.0005`
  (−3.7 SE) — it trades middle placements for outright wins.  The rival
  ladder that ranks the minors above hearts (♦ > ♣ > ♥, the literal reading
  of "diamonds are more dangerous than clubs") was **refuted** at
  `rank −0.0020 ± 0.0009` (−2.3 SE) on the same three seeds.
- `HeuristicConfig::two_of_clubs_bonus`, a flat pass-score bonus for the 2♣
  alone, **defaulting to 6**.  The 2♣ is the one card in the deck whose play
  is never a choice — its holder must lead it at the first trick — so it
  costs nothing to give away, yet it scored bare rank 2 and was all but
  unpassable.  Priced at 6 it goes ahead of anything scoring under 8.  The
  value was searched on one seed over {1, 2, 3, 4, 6, 8} and confirmed on
  three fresh ones at 8,000 blocks each: `rank +0.0019 ± 0.0004` (4.5 SE),
  `win +0.0007 ± 0.0002` (4.0 SE), with `not-last` positive on every seed;
  in games mode over 3,000 games, `rank +0.0061 ± 0.0022` and
  `win +0.0030 ± 0.0011`.  The Monte Carlo leg is again a null
  (`rank +0.0015 ± 0.0018`) except for `moons`, up 2.5 and 3.3 SE on the two
  seeds — shedding the forced lead slightly helps a shot.  The effect is
  visible directly in the prior instrument: rank 2 is now 0.63% of everything
  received against rank 3's 0.02%, a bump at the bottom of the histogram that
  is entirely this knob.
- A Monte Carlo rollout that ends the game now pays normalized `3-2-1-0`
  matchpoints — a sole win 1, then ⅔, ⅓, and 0 for a sole last, ties
  averaged — instead of `1/k` for a k-way shared win and 0 for every
  loss.  The old terminal payoff went flat the moment the bot could no
  longer come first, so late-game decisions between 2nd and last fell
  back on the mid-game margin alone, and it priced a two-way tie for the
  crown (½) below a good running position; the new payoff is exactly the
  arena's `rank` column, so the bot fights for the placement the primary
  detector scores.  Only the terminal branch changes — the mid-game
  margin band is untouched, which keeps this distinct from the reverted
  mid-game matchpoint surrogate — so single-deal play is bit-identical
  (the 200-block arena CSV is byte-identical across the change) and only
  games to the target move.  Over 2,000 paired game blocks (replicated
  across two seeds), `mc:128` in a greedy field gains `+0.110 ± 0.008`
  matchpoint rank (13.9 SE), `+0.043 ± 0.003` not-last (15.9 SE), and
  `+0.323 ± 0.017` points/round (18.5 SE), and pays `−0.009 ± 0.004` win
  equity (−2.2 SE) for it.  The columns disagreeing is the design working:
  a rank optimizer stops taking win-or-last gambles the old objective
  liked, and the `win` gate polices strategy changes *under* an objective
  — this entry changes the objective itself to the rank payoff.
- Solver reads are player-facing in expected finishing place,
  `4 − 3·equity`, in `[1, 4]` with 1 a sole win: the terminal `play`
  example's hints and the web hint panel (whose JSON field renames
  `equity` to `place`) — the average-placement convention of four-player
  tables, and both hint columns now read lower-is-better.
- The default pass policy stops overweighting hearts and passes high
  spades far more freely: `heart_weight` drops from the old hard-coded 2
  to 0 and `spade_guards` rises from 3 to 5.  High hearts are duckable on
  demand — a hand eats hearts only when forced to win a trick — while a
  caught Q♠ is thirteen points at once, so the three pass slots are
  better spent on queen-catchers, short suits, and plain high cards.
  Self-play agrees emphatically and monotonically: on the 27-arm search
  grid every step down in `heart_weight` and up in `spade_guards` helped,
  and the confirmed arm wins `+20.04 ± 0.60` percentage points of paired
  game-win rate against three old defaults on a fresh seed.  In the arena
  A/B over 4,000 paired duplicate blocks the new default gains
  `+0.188 ± 0.006` matchpoint rank (31.5 SE), `+0.058 ± 0.002` win equity
  (23.7 SE), and `+1.521 ± 0.048` points/deal, and over 2,000 paired
  games `+0.190 ± 0.005` game-win equity; the edge is transitive, not a
  counter — an old-default bot seated among three new ones loses by
  20.6 SE of rank.  The Monte Carlo bot inherits the new weights in its
  rollout pass model and pass-candidate ranking, and the mc-vs-greedy
  strength tripwire stays green.
- Shoot-world rollout defenders now hold their fire until the shooter
  shows eight points — the same trigger the live moon-defense overlay
  fires on.  Defending from trick 1 priced every shot against a table no
  real opponent fields, and the pessimism was double: it depressed both
  the shot's equity and the in-world moon count the Bernoulli-majority
  bar reads.  Over 6,000 paired duplicate blocks across two seeds,
  `mc:128` gains `+0.0033 ± 0.0006` win equity (5.9 SE),
  `+0.0032 ± 0.0014` matchpoint rank (2.2 SE), and `+0.075 ± 0.020`
  points/deal (3.7 SE), while completed moons rise from 2.0 % to 2.5 % of
  rounds (8.6 SE).  The lone cost is `not-last` at `−0.0011 ± 0.0005`
  (−2.3 SE): more shots means occasionally finishing last, and with four
  players seeking the win and avoiding the loss are different objectives
  — the win is the shipped rule.
- Rollouts thread the running set of played cards through the policy
  instead of refolding the whole trick history at every play, making
  Monte Carlo decisions about 2.5× faster with bit-identical choices:
  `mc:128` play drops from 10.8 to 4.0 ms and `mc:64` pass from 23.9 to
  9.8 ms on the decision bench, and the 200-block arena reference is
  byte-identical.  Width buys strength (`mc:1024` beats `mc:128` by
  5.7 SE), so the same latency now affords about two and a half times
  the worlds; the web menu and hint now spend part of that gain.
- Upgrade to `hearts` 0.1.1 and reuse a scratch `Round` for rollout
  continuations, retaining its completed-trick allocation instead of cloning
  a fresh vector for every candidate/world pair.  Together with the mechanics
  crate's faster heart-break check, this makes another byte-identical cut:
  `mc:128` play drops from 3.97 to 2.18 ms (45 %), `mc:64` pass from 9.64 to
  5.87 ms (39 %), and `mc:1024` play from 21.39 to 11.49 ms (46 %).  The
  200-block arena CSV remains exactly unchanged.
- Extend only contested Monte Carlo decisions: when the mean-best surviving
  challenger is inside the significance gate in both directions, score
  another base-width batch for the incumbent and survivors, capped at three
  times the configured width.  Across 4,000 paired duplicate blocks on two
  seeds, `mc:128` gains `+0.0118 ± 0.0049` matchpoint rank (2.4 SE),
  `+0.0055 ± 0.0022` win equity (2.5 SE), and `+0.084 ± 0.042` points/deal
  (2.0 SE), with `not-last` unchanged at `+0.0003 ± 0.0016`.  CPU work rises
  47 %; a four-times cap was stronger on the first seed but cost 63 % more
  wall time, so the three-times cap keeps the adaptive search near its
  intended 1.5× budget.
- Restrict soft card-location inference to the known incoming pass.  Public
  safe ducks no longer reweight worlds according to the exact greedy rollout
  policy.  The pass-only form remains favorable but unresolved over 2,000
  paired deals (`win +0.0042 ± 0.0026`, `rank +0.0046 ± 0.0058`).
- Add soft card-location inference to Monte Carlo worlds.  A last-hand duck
  on a clean trick now counts against a hidden safe winner, and the known
  giver's pass softly favors hands where those cards rank as dangerous; both
  remain likelihoods rather than hard constraints because opponents need not
  share the rollout policy.  Across 10,195 paired duplicate deals over two
  seeds, `mc:128` improves by `+0.0059 ± 0.0027` matchpoint rank (2.2 SE)
  and `+0.0031 ± 0.0012` win equity (2.5 SE); the point-margin estimate is
  positive but unresolved at `+0.027 ± 0.023` points/deal.
- Let the Monte Carlo pass search consider passes that actually empty every
  1–3 card non-spade suit, supplementing the 20 triples drawn from its six
  highest independently scored cards.  Those scores could award three void
  bonuses to three different suits while making no void reachable.  Over
  6,195 paired duplicate deals, the new candidates improve `mc:128` by
  `+0.0817 ± 0.0164` points/deal, `+0.0083 ± 0.0018` matchpoint rank, and
  `+0.0038 ± 0.0008` win equity; completed moons rise by `0.19 ± 0.04`
  percentage points.
- Make the shared greedy play/rollout policy point-aware.  Last to a clean
  trick it now takes control with the cheapest winner, while it still ducks
  a trick carrying penalties; on lead it works its shortest suit instead of
  letting global card order favor clubs.  For `mc:128` this improves the
  paired matchpoint rank payoff by `+0.0359 ± 0.0114` (3.2 SE) and the point
  margin by `+0.205 ± 0.095` per deal, with win equity unchanged
  (`+0.0020 ± 0.0050`).  Completed moons fall by `0.39 ± 0.17` percentage
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
- Spend the rollout speedup on the web difficulty menu: Easy stays `newbie`,
  while Medium rises to `mc:32`, Hard to `mc:128`, and Expert to `mc:256`,
  matching the hint solver.  The four tiers win 3.4 / 25.1 / 32.5 / 39.1 %
  of 300 duplicate game blocks in a shared lineup.  In the more sensitive
  deal A/B, `mc:256` beats `mc:128` by `+0.0212 ± 0.0077` matchpoint rank
  (2.8 SE) and `+0.149 ± 0.067` points/deal, with win equity positive but
  unresolved at `+0.0043 ± 0.0035`.

### Fixed

- `MonteCarloBot`'s rank-adjacency collapse treated a card sitting in the
  trick in progress as a gone rank, so it bridged sequences across it: behind
  a led J♦, a holding of 7♦ T♦ Q♦ K♦ A♦ folded into a single class, and the
  bot ducked with the lowest card without ever rolling out the three plays
  that take the trick.  The hint panel, which lists an all-equivalent class
  as tied, showed all five followers at the same equity.  Sequences now
  collapse only across ranks gone in *completed* tricks.

### Internal

- **Spade-void and double-void pass candidates are a screen null.**  Three
  disjoint `mc:128` pool arms faced the shipped pool over the same 2,000
  seed-0 paired blocks: spade-emptying candidates moved `rank −0.0003 ±
  0.0010`, side-suit double voids produced exactly `+0.0000 ± 0.0000`, and
  their joint arm again moved `−0.0003 ± 0.0010`; no `win` estimate was
  worse than 2 SE.  The predeclared rule required a positive rank estimate
  to nominate one 6,195-block confirmation arm, so none advanced and no
  full-round latency gate was run.  A 4,096-hand diagnostic shows the
  search was selective rather than blind: only 42 newly offered
  spade-emptying passes cleared its 1.5-SE decision gate, led by Q-bearing
  doubletons at 13 of 223 hands.  That local rollout confidence did not
  become aggregate strength.  The fixed-hand `mc:64` pass benchmark moved
  from 9.764 to 9.685 ms (−0.8%, within Criterion's noise threshold).  The
  generator changes, temporary arena selector and probe were removed; no
  public API or configuration remains.

- **A per-diamond pass bonus is refuted.**  The first trick is led in clubs
  and cannot score, so one round of clubs is free and a club ought to be the
  safest side-suit card to keep, making diamonds the dearer minor.  Built as
  a flat `diamond_bonus`, swept, and measured wrong: `tune` put 1 at
  `+0.20 ± 0.63` pp and 2 and 3 clearly under water (−0.70, −2.45), and the
  arena refuted even the best arm at `rank −0.0075 ± 0.0013` (−5.9 SE) over
  8,000 blocks on each of two seeds, `win` −3.9 and −3.8 SE.  The same
  ordering was independently refuted as a tie-break (see the ♦ > ♣ > ♥ ladder
  above), so two mechanisms of very different size agree.  The knob is
  reverted in full — the argument survives, the number does not.  Anyone
  reviving it should note the finest step available on this grid is a whole
  rank, roughly an order of magnitude more than the tie-break artifact it
  competes with, which is the case `passing-opponent-model.md`'s P3(i)
  rescale exists to serve.
- **The forced-void pass generator earns its keep under the new policy.**
  The doc's sequencing owed a deletion A/B: sequential selection escalates
  the void bonus along a completing suit, so it might subsume the
  candidate-side patch that makes every short non-spade void reachable.  It
  does not.  Removing the generator cost `rank −0.0063 ± 0.0017` (−3.7 SE),
  `win −0.0032 ± 0.0007` (−4.8 SE) and `moons −0.0030 ± 0.0004` (−7.6 SE)
  over 6,000 paired blocks, and bought no throughput back (252 vs 261
  rounds/s).  The doc's worked example A — a hand where both the flat *and*
  the sequential policy scatter, so only a forced triple reaches the void —
  is now measured rather than argued.  Kept.
- Measure the incoming-pass prior — what the shipped greedy pass actually
  sends the seat downstream of it — with a new `#[ignore]`d instrument,
  `tests/pass_prior.rs`, over 10⁶ four-hand deals
  (`cargo test --release --test pass_prior -- --ignored --nocapture`; the
  `--nocapture` is load-bearing, since libtest swallows a passing test's
  stdout and the table is the result).  It deals all four hands rather
  than a lone giver hand, because our cards and the giver's are dependent
  in a way that moves the giver's *shape*, not just where the queen is;
  `P(giver holds the honor)` reads 0.3333 as its own self-check.  Three
  of the four recorded predictions confirmed: we receive Q♠ on 0.3235 of
  the hands where we lack her (the policy ships her on 0.9707 of the
  hands where it holds her, so the arrival is essentially the 1/3 prior
  undiminished, and A♠ and K♠ are higher still); A, K and Q are 77 % of
  everything received; and below-queen spades arrive 0.0270 times per
  pass, which retires the "expected incoming guards" adjustment to the
  spade tier as a no-build.  The fourth resolved against the guess: the
  refill curve is *flat* in how many cards of the suit we hold, where the
  uniform null falls 0.115 across zero to three, so voiding a side suit
  buys much less protection than the null suggests — the giver cannot see
  our hand, so its policy cannot react to our shape.  Unpredicted, and
  the largest single finding: at the shipped `heart_weight = 0`,
  `pass_score` is exactly symmetric across ♣/♦/♥, yet the measured
  per-suit means are ♣ 0.8188, ♦ 0.7447, ♥ 0.6741.  That spread can only
  be the stable sort resolving ties in `Suit::ASC` order, so the pass
  policy carries a 0.145-cards-per-pass club bias that nobody chose.
- Keep the Monte Carlo world generator's opponents deterministic after
  testing a noisy one.  Modeled opponents pass their greedy triple with
  certainty, which hands over an unguarded Q♠ every time a world is
  sampled, while the play-time observation model concedes the opposite at
  a quarter of the weight per miss; the experiment gave each modeled
  opponent probability ε of swapping the third-ranked card of its pass
  for the fourth.  The knob was live and bought nothing: over 2,000
  paired duplicate blocks, matchpoint rank moves `−0.0037 ± 0.0079`,
  `−0.0046 ± 0.0080` and `+0.0004 ± 0.0077` at ε of 0.1, 0.25 and 0.5,
  and a 6,000-block confirm on a second seed leaves every column flat
  (`rank −0.0003 ± 0.0046` and `+0.0016 ± 0.0045`).  The first seed's
  uniformly negative points margin did not reproduce on the second — the
  correlated-deal noise this project has been caught by before, and the
  reason the second seed ran before the revert rather than after.  Ranks
  three and four of the pass score are usually near-neighbours, so the
  swap perturbs a sampled world barely at all while adding exactly the
  variance the significance gate exists to absorb.  Reverted; nothing
  ships.
- Close the soft pass-inference question.  The reweight shipped on a
  favourable but unresolved 2,000-deal result, so it was rerun over
  16,000 paired duplicate blocks at `mc:128`, 8,000 on each of two seeds,
  as a cross-build pairing in which the disabled arm still draws its
  uniform and only ever accepts — so the two builds consume the identical
  random stream and differ solely in whether a world is rejected.  It is
  still unresolved: pooled `win +0.0011 ± 0.0009`, `rank +0.0009 ±
  0.0021`, `points +0.0019 ± 0.0193`.  The model is nearly free and stays
  shipped, but the original estimate was optimistic by about four times
  (`win +0.0042`, `rank +0.0046`), so it is the figures here that should
  size any future pass-inference work.  The planned sweep of the fiat
  0.75 miss factor is dropped rather than deferred: tuning a constant
  inside an effect that will not resolve at 16,000 blocks needs an order
  of magnitude more games than the effect is worth.
- Open `docs/` with two passing design proposals:
  `docs/passing-opponent-model.md` (configurable pass models, an offline
  incoming-card prior, world-generator noise) and `docs/passing-shape.md`
  (a sequential set-aware `greedy_pass`, spade and double void
  candidates, a shape-aware moon pass).  Ranked proposals with
  measurement plans and kill criteria; design only, nothing ships yet.
- Keep ordinary Monte Carlo rollouts greedy after testing promotion of a sole
  opponent sweeper to a moon attempt at the live eight-point defense trigger.
  The model has its intended mechanical effect: against three legacy
  `mc0:32` opponents over 2,000 paired game blocks, opponent moons fall
  `−0.0063 ± 0.0015` per round (4.2 SE).  It does not improve strength there:
  matchpoint rank moves `−0.0106 ± 0.0154`, win equity
  `+0.0019 ± 0.0062`, points/deal `−0.027 ± 0.047`, and own moons
  `−0.0012 ± 0.0009`.  Worse, against three greedy bots over 2,000 paired
  deal blocks it costs `−0.0256 ± 0.0073` rank (3.5 SE),
  `−0.0144 ± 0.0034` win equity (4.2 SE), and `−0.148 ± 0.063`
  points/deal.  Criterion puts the extra policy at about 14 % on `mc:128`
  play and 10 % on `mc:64` pass; the paired arena ran 37.1 deal blocks/s and
  1.56 game blocks/s.  The model and its measurement-only `mc0` control were
  removed; the live deterministic moon-defense overlay remains unchanged.
- Keep the Monte Carlo pass pool at six cards after testing seven.  The
  wider pool adds every triple containing the seventh-ranked pass card — 35
  score-pool candidates instead of 20 — but over 6,000 paired duplicate
  blocks on two seeds it moves the primary matchpoint-rank detector only
  `+0.0010 ± 0.0012` (0.9 SE).  Points/deal move `+0.0066 ± 0.0123` and win
  equity `+0.0010 ± 0.0005`, while not-last moves `−0.0008 ± 0.0004`;
  `mc:64` pass latency rises from 6.50 to 8.71 ms (34 %).  The primary
  detector does not clear the ship gate, so the wider pool was reverted and
  nothing ships.
- Confirm `moon_defense = 8` and `void_weight = 1` at the retuned pass
  defaults.  The shipped trigger was tuned against the old pass policy and
  only on a coarse grid, so a fine sweep (moon 5–12 crossed with void
  0/1/2/4) reran it: over 1,000 paired games every moon arm at the shipped
  `void_weight` sits within one SE of the default, while `void_weight = 2`
  floats `+2.0 ± 1.3` pp across all moon arms.  8,000 games at a fresh
  seed refute both — `void_weight = 2` lands at `−0.20 ± 0.42` pp (the
  stage-one signal was correlated-deal noise) and `moon_defense = 6` is
  resolvably worse at `−0.12 ± 0.07` pp.  The defaults survive the new
  pass policy unchanged; nothing shipped.
- The `tune` example sweeps the Cartesian product of all four
  `HeuristicConfig` knobs (`--heart-weight` and `--spade-guards` join the
  existing flags), reporting one flat row per arm — the paired
  game-win-rate delta ± its paired standard error — plus a paste-able
  `best:` line, replacing the fixed two-knob matrix.  An arm equal to the
  default plays literally the same games and must print `+0.00 ± 0.00`
  exactly, the tune-side analog of the arena's homogeneous-field
  self-check.  The `arena` example accepts `greedy:V,H,G` specs setting
  the three pass knobs, so tuned candidates can face the ship gate
  directly.
- Keep the strict-majority bar for moon candidates after testing a 45 %
  threshold.  Over 2,000 paired duplicate blocks, the looser bar leaves
  matchpoint rank flat (`−0.0001 ± 0.0014`) and its win-equity gain is
  unresolved (`+0.0009 ± 0.0006`, 1.6 SE), despite the intended rise in
  completed moons from 2.41 % to 2.65 %.  Points move `+0.028 ± 0.020` per
  deal and `not-last` moves `−0.0008 ± 0.0005`; because the primary rank
  detector did not improve, the 45 % arm was reverted and the conditional
  40 % arm was skipped.
- Close two strength-queue experiments after paired measurement.  Carrying
  compatible determinized particles between decisions and replenishing the
  rejected worlds is strength-neutral over 2,000 blocks (`rank +0.0016 ±
  0.0054`, `win +0.0001 ± 0.0025`) and trims only 1.7 % from a matched
  arena workload, so the cache complexity was reverted.  Replacing mid-game
  point margin with projected soft matchpoints is worse over 600 game blocks:
  the pure surrogate moves `win −0.0086 ± 0.0096`, `rank −0.0254 ±
  0.0187`, and points/deal `−0.376 ± 0.056`; blending toward it with game
  progress still moves `win −0.0069 ± 0.0087`, `rank −0.0169 ± 0.0163`,
  and points/deal `−0.212 ± 0.050`.  Both equity shapes were reverted.
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
- Raise the fixed web hint from 128 to 256 sampled worlds.  With contested
  decisions now extending adaptively, the two widths recommend different
  moves on 61 of 1,103 real human decisions (5.5 %), while their native mean
  latency is 2.61 vs 4.37 ms on those positions; 1024 worlds changes another
  6.3 % but costs 14.0 ms natively, leaving a conservative margin for slower
  WebAssembly devices.

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
