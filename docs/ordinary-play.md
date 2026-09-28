# Ordinary play — search throughput, sample efficiency, and the rollout policy

**Status: OPEN (2026-09-29).**  Nothing here is built or measured.  This
is the campaign [cfr-gap.md](cfr-gap.md) closed into: with every moon
mechanism measured or ruled out, the remaining Deep CFR gap is the
ordinary-play residue (about `+0.2` payoff per seat-deal), and the one
strength lever that is measured and unspent is compute — `mc:256` beats
`mc:128`.  So the proposals come in three tiers: make each rollout cheaper
(T), make each rollout worth more (S), and make the policy inside the
rollouts better (P).  Siblings: [passing-shape.md](passing-shape.md) and
[passing-opponent-model.md](passing-opponent-model.md) own the pass
policy; nothing here touches `pass_score`.

## Goal and non-goals

**Goal.**  Raise `mc:128`'s ordinary card play at *fixed wall-clock*: a
seat that decides as fast as today but with more or better-spent
rollouts, and a rollout policy that prices candidate lines closer to how
a strong player continues them.  The yardsticks are the arena's `rank`
column against the greedy field and, for a claim of closure, the
1,600-deal Deep CFR rerun.

**Non-goals.**  No moon machinery changes — the double bar, the latch,
the eight-point trigger and the shoot line are all priced and closed.
No opponent modeling inside worlds beyond what ships (the priors below
wall it off).  No pass-policy changes.  No `Knowledge` ledger and no
`sim.rs`: worlds stay real `hearts::Round`s.

## Measured priors this doc must respect

| Prior (CHANGELOG / docs) | Result | Lesson |
| --- | --- | --- |
| `mc:256` vs `mc:128` | positive, "an independent compute lever" | samples still pay; throughput is strength |
| point-aware greedy play | `rank +0.036`, moons −0.39 pp | the rollout policy is a live lever, and it moves the moon column |
| soft inference from *plays* (`67b25b8`) | reverted; only pass evidence survives | do not weight worlds by what opponents played |
| opponent-moon rollout model | `rank −0.0256 ± 0.0073` | worlds must not out-inform the field |
| maxⁿ endgame solve (`endgame-solver-null`) | −2.0 SE points | perfect-information *play* inside a world is poison |
| ordinary-world sweeper symmetry | null | `shooter == None` worlds stay four greedy duckers |
| noisy opponent pass generator | null, reverted | variance in the opponent *model* buys nothing |
| pass pool 6→7 | 0.9 SE, +34% latency | latency is a real cost axis; gate it on arena throughput |
| gate 1.5 SE | measured, a knob | do not retune the gate to make a proposal look alive |

Two of these shape everything below.  The maxⁿ null and the opponent-moon
null together say: a world may be *sampled* with perfect information, but
nobody inside it may *play* with it.  Every proposal here keeps the rollout
policy knowledge-free (`legal`, `trick`, `played`, points so far), which is
also what lets the same policy ship as `HeuristicBot`.  And the play-
inference reversion says the sampler's soundness is the void table plus
the pass likelihood, nothing softer.

## Where the cost goes today

Function names, not line numbers.

- **`sample_world`** builds every world by dealing the sampled original
  hands as a `Hold` round and replaying the entire public history through
  `Round::play`.  Early in a round that is nothing; on trick eleven it is
  ten tricks of replay in front of two tricks of rollout.  Nothing is
  cached between decisions: the next trick resamples from zero.
- **`score_worlds`** rolls candidates in growing batches (32, 32, 64, …)
  with challenger elimination, and in the `parallel` build each
  `(candidate, batch)` is its own `par_iter` of 32 small jobs.
- **The budget is in worlds**, `samples` of them, extended up to three
  times when the incumbent/challenger comparison stays inside the gate.  A
  pass decision rolls thirteen tricks per world; a trick-twelve decision
  rolls one.  Late decisions are therefore an order of magnitude cheaper
  than the budget lets them use.
- **The policy** is `greedy_play`: lead low spades to smoke the queen,
  else lead the shortest suit low; duck under the winner; last to a clean
  trick takes it cheaply; void, dump the most dangerous card.  It reads
  no history beyond `played` and never counts a suit.

## Proposals

| # | Proposal | Touches | Class | Status |
| --- | --- | --- | --- | --- |
| T2 | coarser parallel tasks | `score_worlds` | throughput | open |
| S1 | stratify worlds by the Q♠ holder | sampler | sample efficiency | open |
| S3 | budget in rollout tricks, not worlds | `score` | sample efficiency | open |
| P1 | cheap greedy lead rules | `greedy_play` | policy | open |
| P2 | a distilled rollout policy | `greedy_play`, new offline tooling | policy, high ceiling | open, last |

Suggested order: S1 and S3 first, both small and policy-neutral; T2
whenever the arena or the tournament harness is the bottleneck; P1 as
cheap arena legs; P2 as its own campaign once S3 has bought it enough
samples to be screened at `mc:256`-equivalent cost.

**Cut before building (2026-09-29).**  T0 (profile the sampler) only
gated T1 and T3, and changed no decision on its own.  T1 (carry worlds
across decisions) and T3 (pass-likelihood weights instead of rejection)
both rewire the sampler or every `Scored` consumer for a throughput gain
nobody has shown is there, and T3 pays for it in effective sample size.
S2 (enumerate worlds near the end) targets the late decisions S3 already
floods with cheap worlds.

### T2 — coarser parallel tasks

**Mechanism.**  In `score_worlds`, flatten one batch's work to a single
`par_iter` over `(candidate, world)` pairs — the incumbent plus every
live challenger against every world in the batch — collect into a
`Vec` in that order, and reduce sequentially per candidate exactly as
today.  Same results in the same order; only the rayon job count changes
from `|candidates|` calls of 32 jobs to one call of `32 × |candidates|`.

**Coupling.**  `parallel` feature only; the serial build and the wasm
front end are untouched.  The bit-identity test
(`seeded_pick_is_identical_across_serial_and_parallel_builds`) is the
guard.

**Measurement.**  Repeated 500-block arena throughput on seeds 0/1/2 with
`--features parallel`, and the 200-block seed-7 CSV byte-identical across
serial, old-parallel and new-parallel builds.

**Kill criterion.**  Any CSV difference is a bug.  Throughput below +5%
means rayon overhead was not the cost and the change is deleted as noise.

### S1 — stratify worlds by the Q♠ holder

**Mechanism.**  While the queen is unseen and not ours, the seat holding
it is the hidden variable that dominates equity variance in nearly every
ordinary decision.  Before the backtracking in `sample_hands`, assign the
Q♠ to a seat chosen round-robin across the batch among the seats allowed
to hold it (non-void in spades, room remaining), with the round-robin
weighted by each seat's room so that the marginal over the batch matches
the uniform posterior; then backtrack the rest as today.  Common random
numbers already pair candidates within a world; stratification removes
the between-world noise that comes from an unlucky batch putting the
queen behind us three times in four.

Stratification is unbiased only if the strata weights are the true
marginals.  Under voids alone the marginal is proportional to room among
allowed seats, which is exactly what the unstratified backtracker
samples; the pass likelihood reweights afterwards in both cases, so the
two samplers target the same distribution.  A unit test should assert the
queen's empirical marginal matches between the stratified and the plain
sampler on a fixed view to within Monte Carlo error.

**Coupling.**  Play-phase sampler only.  At pass time nothing is unseen
but everything, and the queen's holder is uniform over three seats; the
same round-robin applies trivially and may as well be on.

**Measurement.**  The 2,000-block seed-0 screen, `rank` primary; the
direct read is a *variance* measurement — the mean standard error the
gate sees per decision, logged temporarily, should fall.  Fresh-seed
confirmation at 6,000 blocks.  Throughput unchanged by construction; the
CSV changes, since worlds do.

**Kill criterion.**  Per-decision SE not reduced, or `rank` negative
beyond 2 SE.

### S3 — budget in rollout tricks, not worlds

**Mechanism.**  Redefine the sample budget as rollout *work*: a decision
with `t` tricks left to roll gets `samples × 13 / t` worlds (capped, say,
at `8 × samples`), so every decision spends about the same number of
simulated tricks.  A pass decision keeps its `samples` worlds; a trick-
twelve decision gets many more, at the same cost.  The width extension
(`MAX_WIDTH_MULTIPLIER`) scales with it.

Whether this is a strength gain depends on where the information is:
early decisions have more to price but the worlds are noisier; late
decisions are cheaper to price exactly.  The arena decides.  The knob is
`samples` itself with new semantics, so the arena spec `mc:128` changes
meaning; ship it as a separate `work=` knob on `MonteCarloBot` and the
arena spec, default off, and only flip the default on a confirmed result.

**Coupling.**  `score` only.  `assess` inherits it, which is right: the
hint panel late in a round gets sharper for free.

**Measurement.**  Match on *throughput* first — pick the constant so that
repeated 500-block arena time equals the baseline's within 2% — then the
2,000-block seed-0 screen and 6,000-block seed-1 confirmation on `rank`.
Report `moons` too; a late-decision-heavy budget may change how often the
shoot candidate clears the majority bar at trick one.

**Kill criterion.**  `rank` not positive at 2 SE on confirm, or any
`win` regression beyond 2 SE.

### P1 — cheap greedy lead rules

Two independent arena legs, each a few lines in `greedy_play`, each
knowledge-free so it flows into rollouts and `HeuristicBot` alike.

**P1(i) — stop smoking the queen behind an unguarded A♠/K♠.**  Today a
seat that holds neither the queen nor has seen it leads low spades to
smoke it out.  When the seat also holds A♠ or K♠ with fewer low spades
than `spade_guards` (the same knob the pass policy uses), each low spade
led is a guard spent, and the queen may well be behind us waiting for
exactly that.  Rule: skip the smoke-out while an unguarded A♠/K♠ is in
hand; fall through to the shortest-suit lead.

**P1(ii) — prefer leading a suit that has gone round once.**  The
shortest-suit lead is about creating a void.  A cheaper, safer lead is
often the suit with the fewest live high cards behind us — a suit already
played once has at most two rounds of danger left, and `played` tells us
which.  Rule: among suits where our lowest card is below the highest
unplayed card (we will not win), prefer the suit with the most cards
already played; ties fall to the existing shortest-suit rule.  This reads
only `played`, which the policy already takes.

**Coupling.**  Both change `HeuristicBot`, the web tiers, and every
rollout.  The greedy self-check (`greedy greedy greedy greedy` prints
`0.000±0.000`) still holds, since the change is deterministic.

**Measurement.**  Each leg separately against the shipped policy over
2,000 seed-0 blocks, then joint if both are non-negative; confirm on
6,000 seed-1 blocks.  `rank` primary, `moons` reported: point-aware
greedy play moved the moon column by −0.39 pp, and a lead rule that
keeps control cards may move it the other way.

**Kill criterion.**  The house one per leg.  A leg that helps `greedy`
but hurts `mc:128` is the interesting failure — it would mean the rollout
policy and the live policy want to part company, which is a design
change, not a tweak.

### P2 — a distilled rollout policy

**Why it is last.**  Every prior in the table is a hand-written rule;
this is the one mechanism class the campaign has never tried.  In
determinized Monte Carlo the rollout policy's quality is the classic
largest lever, and the point-aware greedy row is this engine's own
evidence that the lever is live.  It is last because it is the most
work, because it needs the throughput tier to make its screening
affordable, and because a fitted policy can lose in ways a rule cannot —
the moon column above all.

**Mechanism.**  Offline: log `mc:256` play decisions from the arena —
features that are legal for a rollout policy to read (the legal set, the
trick so far, `played`, points taken so far, own hand shape) and the card
chosen.  Fit a small model that ranks legal cards: a linear scorer over
a few dozen hand-crafted card features first, a tiny tree ensemble only
if the linear one is clearly short.  Online: the fitted scorer replaces
`greedy_play` as the rollout policy *and* as `HeuristicBot`'s play,
which keeps the shipped invariant that the rollout policy is the
knowledge-free core of the live heuristic.  Weights ship as constants;
no runtime dependency, nothing in the wasm build changes shape.

Two disciplines from the priors.  The scorer must be knowledge-free by
construction — its features come from the same three arguments
`greedy_play` takes, so it *cannot* out-inform the field.  And it must be
screened as a rollout policy, not as a player: the question is whether
`mc:128` with the new rollouts beats `mc:128` with greedy rollouts, and
only secondarily whether the new `HeuristicBot` beats the old.

**Measurement.**  Three stages.  (1) Offline: held-out agreement with
`mc:256`'s choice, against greedy's agreement as the floor.  (2) Arena:
`mc:128`-new vs `mc:128`-old over 2,000 seed-0 blocks, `rank` primary,
`moons` watched, throughput within 5% (the scorer runs on every card of
every rollout trick — it must be cheap).  (3) Confirm on 6,000 seed-1
blocks; then the 1,600-deal Deep CFR rerun on seed 1, since the
ordinary-play residue against *him* is the number this campaign is
named for.

**Kill criterion.**  Stage 1 not clearly above greedy's agreement, or
stage 2 `rank` not positive at 2 SE, or throughput loss beyond 5%.  A
positive `rank` with a falling moon column is a *trade*, to be priced on
the tournament rerun, not a ship.

## Interactions

- P1 and P2 are mutually exclusive in the long run: a fitted policy
  subsumes the lead rules, and P1's arena legs are cheap evidence about
  which features P2 should carry.

## Appendix — measurement boilerplate

House discipline, unchanged: search on one seed, confirm on a fresh one;
`rank` primary, `points` magnitude, `win` the ship gate; a homogeneous
greedy field must print `0.000±0.000` exactly; latency is gated on
repeated full-round arena throughput, never an isolated bench; the
200-block seed-7 CSV byte-identical across serial and `parallel` builds
for any change that claims not to move decisions.  The strength tripwire
stays manual: `cargo test --release --test strength -- --ignored`.
