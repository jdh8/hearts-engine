# Ordinary play — search throughput, sample efficiency, and the rollout policy

**Status: OPEN (2026-09-29, revised against the code the same day).**
P1 shipped and S3 was cut on 2026-09-29 (results in their sections);
S1 and T2 were cut the same day on S3's result without being built.
P2 is what remains; its stage 0 passed, so stage 1 is next.  This is the campaign
[cfr-gap.md](cfr-gap.md) closed into: with every moon mechanism measured
or ruled out, the remaining Deep CFR gap is the
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
policy knowledge-free — `greedy_play` reads exactly `legal`, `trick` and
`played`; only `rollout_play`'s shoot branch also reads points taken —
which is also what lets the same policy ship as `HeuristicBot`.  And the
play-inference reversion says the sampler's soundness is the void table
plus the pass likelihood, nothing softer.  That likelihood is applied by
*rejection* inside `sample_hands` (a draw survives with probability
`pass_observation_likelihood`), not as a weight, and it carries real
evidence about the Q♠: a giver that did not pass us the queen probably
does not hold it.

## Where the cost goes today

Function names, not line numbers.

- **`sample_world`** builds every world by dealing the sampled original
  hands as a `Hold` round and replaying the entire public history through
  `Round::play`.  Early in a round that is nothing; on trick eleven it is
  ten tricks of replay in front of two tricks of rollout.  The replay is
  paid once per world; the rollout once per world *per live candidate*.
  Nothing is cached between decisions: the next trick resamples from zero.
- **`score_worlds`** rolls candidates in growing batches (32, 32, 64, …)
  with challenger elimination, and in the `parallel` build each
  `(candidate, batch)` is its own `par_iter` over the batch.  The arena
  already runs blocks under `into_par_iter`, so there the inner
  parallelism only competes for saturated cores; it pays in a
  *sequential* consumer — `vs_cfr`, `play` — and nowhere in the serial
  wasm build.
- **The budget is in worlds**, `samples` of them, extended up to three
  times when the incumbent/challenger comparison stays inside the gate.  A
  pass decision rolls thirteen tricks per world for some twenty
  candidates; a trick-twelve decision replays eleven tricks and rolls two
  for at most two.  Per world, the late decision costs several times
  less — not thirteen times, because the replay does not shrink.
- **The policy** is `greedy_play`: lead low spades to smoke the queen,
  else lead the shortest suit low; duck under the winner; last to a clean
  trick takes it cheaply; void, dump the most dangerous card.  It reads
  no history beyond `played` and never counts a suit.

## Proposals

| # | Proposal | Touches | Class | Status |
| --- | --- | --- | --- | --- |
| T2 | coarser parallel tasks | `score_worlds` | throughput, sequential consumers only | **cut** — compute buys nothing |
| S1 | stratify worlds by the Q♠ holder | sampler, `beats` | sample efficiency | **cut** — capped below the compute lever |
| S3 | spend the budget where compute pays | `score` | sample efficiency | **cut** — nothing to reallocate |
| P1 | keep a high spade guarded | `greedy_play` | policy | **shipped** |
| P2 | a distilled rollout policy | `greedy_play`, new offline tooling | policy, high ceiling | open, stage 0 passed |

Order now: P2 alone.  S3 priced the whole compute lever at about
`+0.006` rank for twice the worlds, and that single number sank both
S1 and T2 (their sections say how), so the rollout policy is the
lever left.  S3 also says the late tricks carry none of `mc:256`'s
gain, so P2's `mc:256` labels are worth most on the early tricks.

**Cut before building (2026-09-29).**  T0 (profile the sampler) only
gated T1 and T3, and changed no decision on its own.  T1 (carry worlds
across decisions) and T3 (pass-likelihood weights instead of rejection)
both rewire the sampler or every `Scored` consumer for a throughput gain
nobody has shown is there, and T3 pays for it in effective sample size.
S2 (enumerate worlds near the end) targets the late decisions S3 already
floods with cheap worlds.

**Revised against the code (2026-09-29).**  S1's unbiasedness argument
was false (below) and its read would have come back null by
construction; S3's cost model ignored the replay and hid that a
throughput-matched budget must take worlds *from* early decisions; T2
was measured on an instrument that cannot see it.  P1(ii) — prefer
leading a suit that has gone round once — is cut: its premise runs
backwards.  A suit's later rounds are the dangerous ones, because that
is when opponents are void and discard points onto it; the
shortest-suit rule it would override exists to be the one creating the
void.  P1(i) is simplified to the rule its knob collapsed to.

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

**Measurement.**  Not arena throughput: the arena's block-level
`into_par_iter` already saturates the cores, so a +0% there says
nothing.  The consumer is a sequential run — the tournament harness —
so the instrument is repeated `vs_cfr` wall time against a local shim at
`--throttle-ms 0`, first split into time inside our `play_card` versus
time waiting on the shim.  Correctness: the 200-block seed-7 arena CSV
byte-identical across serial, old-parallel and new-parallel builds.

**Kill criterion.**  Any CSV difference is a bug.  If the shim's share
of wall time dominates, T2 has no consumer and is cut unbuilt; if built,
under +5% on our share means rayon overhead was not the cost and the
change is deleted as noise.

**Result (2026-09-29): cut unbuilt.**  T2 changes no decision; its only
payoffs are wall time spent on more worlds, which S3 priced at about
`+0.006` rank per doubling, and shorter tournament reruns, which is a
convenience.  Revive it only if a rerun is slow and the split shows the
time is ours.

### S1 — stratify worlds by the Q♠ holder

**Hypothesis.**  While the queen is unseen and not ours, the seat holding
it may be the hidden variable that dominates the variance of the *paired*
differences the gate tests.  Common random numbers already cancel
whatever a world does to both candidates alike; stratification helps only
with the part of the difference that swings with the queen's seat.
Whether that part is large is the whole question, and it is cheap to
answer first.

**Probe (build nothing).**  Temporarily log, per contested play decision,
each world's queen holder and each challenger's paired difference
`challenger − incumbent`; offline, split the variance of the differences
into between-holder and within-holder parts.  The possible gain in the
gate's SE is bounded by the between-holder share.  Under 20% on median
decisions: cut S1 unbuilt.

**Mechanism, if the probe clears.**  Three pieces, each forced by
something the first draft got wrong.

- *Weights from the sampler, not from room.*  The first draft set strata
  weights proportional to each seat's room, claiming that is what the
  backtracker samples.  It is not: randomized most-constrained-first
  backtracking is not uniform over consistent deals (the Q♠ is placed
  uniformly among allowed seats wherever it falls in the order, blind to
  room), and the pass rejection then moves the queen's marginal on
  purpose.  Room-weighted quotas would overwrite the one piece of evidence
  the sampler has about the queen.  So: draw a pool of hands from the
  unchanged sampler (rejection included — cheap, no rollouts, no replay
  for rejected draws), take the queen marginal from the pool, and fill
  each batch's per-seat quotas (largest remainder) from that same pool.
  Within a stratum the worlds are exact sampler draws; only the weights
  carry pool noise.
- *A stratified variance in `beats`.*  With proportional allocation the
  plain mean already is the stratified estimator, but the plain sample
  variance still counts the between-strata spread the design removed.
  Left alone, the gate would see an SE that barely moves, and S1 would
  read null by construction.  `beats` must pool the within-stratum
  variances, which means `Scored` carries each world's stratum — the one
  consumer-visible change.
- *Play phase only.*  At pass time the queen is either ours or uniform
  over three seats with no evidence yet; nothing to stratify.

A unit test pins the first piece: on a fixed view with a received pass,
the stratified and the plain sampler's queen marginals agree within Monte
Carlo error.

**Measurement.**  Logged per-decision gate SE, which should now fall by
roughly the probe's between-holder share; then the 2,000-block seed-0
screen and 6,000-block fresh-seed confirmation, `rank` primary.  The
sampler draws more hands per world than today, so throughput is
measured, not assumed.

**Kill criterion.**  Probe share under 20%; built, gate SE not reduced,
or `rank` negative beyond 2 SE.

**Result (2026-09-29): cut unbuilt, the probe not run.**  Stratification
changes no mean; it only shrinks the variance the gate sees.  Removing a
share `f` of that variance is worth exactly `1/(1 − f)` times the worlds,
so S1 is a compute lever, and S3 priced that lever at about `+0.006 ±
0.003` rank for a doubling.  At the probe's own 20% bar S1 is worth 1.25×
the worlds; merely matching `mc:256`'s gain needs `f ≥ 50%`, and only on
the play decisions where the queen is still unseen, before paying for the
extra hand draws.  The ceiling sits below a lever already measured as
weak, so no probe result could make S1 worth building.

### S3 — spend the budget where compute pays

**The trade, stated honestly.**  The first draft gave late decisions
`samples × 13 / t` worlds while "a pass decision keeps its `samples`" —
and then matched throughput, which cannot both hold.  At fixed wall-clock
any worlds added late are taken from early decisions.  So the question is
not "are late decisions under-sampled" but "which decisions does the
`mc:256`-over-`mc:128` gain live in" — and that is answerable with one
knob before any redistribution is designed.

**Probe.**  A trick-dependent sample count on `MonteCarloBot`, e.g.
`samples_from(trick, n)`: `n` worlds from trick `trick` on, `samples`
before.  Two arena legs against plain `mc:128` over 2,000 seed-0
blocks — `128 → 256 from trick 7` and `256 → 128 from trick 7` (the
pass decision stays at 128 in both; its budget belongs to the passing
siblings).  Each leg's `rank` against the full `mc:256`-vs-`mc:128` gain
says where the compute pays.

**Mechanism, if one half carries the gain.**  Move worlds from the other
half at matched throughput.  Price a world honestly: replay `13 − t`
tricks once, plus `t` tricks per live candidate, so the late discount is
several-fold, not thirteen-fold; the exchange rate is set empirically by
the throughput match, not by the formula.  The width extension
(`MAX_WIDTH_MULTIPLIER`) scales with the per-decision count.  If the
probe says the gain is spread evenly, S3 is cut: there is nothing to
reallocate.

**Coupling.**  `score` only; the knob is separate from `samples`, so
`mc:128` keeps its meaning, default off, flipped only on a confirmed
result.  `assess` inherits it.

**Measurement.**  Throughput first — the constant chosen so repeated
500-block arena time equals the baseline's within 2% — then the 2,000-
block seed-0 screen and 6,000-block seed-1 confirmation on `rank`.
Report `moons` too: taking worlds from trick one changes how often the
shoot candidate clears the majority bar.

**Kill criterion.**  Probe gain evenly split; built, `rank` not positive
at 2 SE on confirm, or any `win` regression beyond 2 SE.

**Result (2026-09-29): cut.**  The probe used a temporary
`samples_from(trick, n)` knob, applied to play decisions only, and each
leg was paired against plain `mc:128` over 6,000 blocks.  The knob is
deleted.

| Leg (pass / tricks 1–6 / tricks 7–13) | seed | `rank` | `win` |
| --- | --- | --- | --- |
| 256 / 256 / 256 (`mc:256`) | 0 (2,000) | +0.0027 ± 0.0073 | +0.0046 ± 0.0033 |
| 256 / 256 / 256 | 1 | +0.0090 ± 0.0042 | +0.0049 ± 0.0019 |
| 256 / 256 / 256 | 2 | +0.0044 ± 0.0043 | +0.0030 ± 0.0019 |
| 256 / 256 / 128 | 1 | +0.0073 ± 0.0042 | +0.0044 ± 0.0019 |
| 128 / 256 / 256 | 0 (2,000) | +0.0047 ± 0.0065 | +0.0047 ± 0.0029 |
| 128 / 256 / 256 | 1 | +0.0010 ± 0.0036 | +0.0019 ± 0.0016 |
| 256 / 128 / 128 | 1 | +0.0071 ± 0.0037 | +0.0033 ± 0.0016 |
| 256 / 128 / 128 | 2 | −0.0011 ± 0.0037 | −0.0002 ± 0.0016 |
| 128 / 128 / 256 | 0 (2,000) | −0.0004 ± 0.0016 | +0.0001 ± 0.0009 |
| 128 / 128 / 256 | 1 | −0.0004 ± 0.0011 | +0.0004 ± 0.0005 |
| 128 / 128 / 32 | 1 | −0.0029 ± 0.0015 | −0.0004 ± 0.0007 |
| 128 / 128 / 32 | 2 | −0.0023 ± 0.0014 | −0.0005 ± 0.0007 |

Pooled, the whole compute lever is about `+0.006 ± 0.003` rank and
`+0.004 ± 0.001` win, for 65% more wall time (1,000 blocks: 9.2 s →
15.2 s).  Tricks 7–13 are saturated at 128 worlds: widening them is a
tight zero, but they are not oversupplied either, since cutting them to
32 saves 13% of wall time and costs rank at about 2.6 SE pooled.  The
remaining gain is split between the pass and tricks 1–6.  The pass-only
leg's +1.9 SE on seed 1 did not replicate on seed 2, and that seed's
full `mc:256` gain is itself only 1.0 SE.  No half carries the gain, so
nothing can be reallocated.  The real lesson is the pooled size: at
fixed wall-clock, compute is a weak lever for this campaign, and the
rollout policy is the stronger one.

### P1 — keep a high spade guarded

One arena leg, a one-condition change in `greedy_play`, knowledge-free so
it flows into rollouts and `HeuristicBot` alike.

**Rule.**  Today a seat that holds neither the queen nor has seen it
leads low spades to smoke it out, even while holding A♠ or K♠.  Each low
spade led is a guard spent, and the queen may be sitting behind us for
exactly the moment our high spade is bare.  So smoke only when all our
spades are below the queen; otherwise fall through to the shortest-suit
lead.  The first draft gated this on `spade_guards`, but `greedy_play`
takes no `HeuristicConfig`, and at the shipped 5 the gate holds in
nearly every hand anyway — it collapses to "holding A♠ or K♠".  A spade
count threshold is a second leg only if this one is positive.

**Coupling.**  Changes `HeuristicBot`, the web tiers, and every rollout.
The greedy self-check (`greedy greedy greedy greedy` prints
`0.000±0.000`) still holds, since the change is deterministic.

**Measurement.**  Against the shipped policy over 2,000 seed-0 blocks in
two lineups — `greedy` (the live heuristic) and `mc:128` (the rollout
policy) — then confirm on 6,000 seed-1 blocks.  `rank` primary, `moons`
reported: point-aware greedy play moved the moon column by −0.39 pp, and
a lead rule that keeps control cards may move it the other way.

**Kill criterion.**  The house one.  A rule that helps `greedy` but hurts
`mc:128` is the interesting failure — it would mean the rollout policy
and the live policy want to part company, which is a design change, not
a tweak.

**Result (2026-09-29): shipped.**  The A/B used a temporary thread-local
toggle, wrapped around each decision of the measured seat and sound in the
serial MC build, where rollouts run on the deciding thread.  It is
deleted.  Each row is paired against the same bot without the rule, in a
greedy field.

| Bot | seed 0 (2,000) `rank` | seed 1 (6,000) `rank` | seed 1 `win` | seed 1 `moons` |
| --- | --- | --- | --- | --- |
| `greedy` | +0.0143 ± 0.0043 | +0.0110 ± 0.0024 | +0.0027 ± 0.0008 | +0.0000 ± 0.0001 |
| `newbie` (web Easy) | — | +0.0154 ± 0.0025 | +0.0043 ± 0.0009 | −0.0000 ± 0.0001 |
| `mc:32` | — | −0.0003 ± 0.0049 | +0.0013 ± 0.0021 | +0.0020 ± 0.0010 |
| `mc:128` | +0.0092 ± 0.0078 | −0.0013 ± 0.0046 | +0.0010 ± 0.0020 | +0.0014 ± 0.0009 |
| `mc:256` | — | −0.0016 ± 0.0044 | −0.0007 ± 0.0020 | +0.0025 ± 0.0009 |

The rule is a clear gain for the live heuristic and neutral for the
search at every width, so it ships in both.  It lifts the MC bot's own
completed moons by about 0.2 pp.  The interesting failure did not happen:
the rollout policy and the live policy still want the same lead.  The
search does not feel the change because rollouts only price the
candidates, and the incumbent the gate protects moves with the policy.
A spade-count threshold (the second leg) is not worth building for a
neutral MC result.

### P2 — a distilled rollout policy

**Why it is last.**  Every prior in the table is a hand-written rule;
this is the one mechanism class the campaign has never tried.  In
determinized Monte Carlo the rollout policy's quality is the classic
largest lever, and the point-aware greedy row is this engine's own
evidence that the lever is live.  It is last because it is the most
work, because its labels cost `mc:256` decisions, and because a fitted
policy can lose in ways a rule cannot — the moon column above all.

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
construction.  `greedy_play`'s three arguments do not carry the feature
list above — `legal` is the hand only on a lead, and points taken are
not an argument at all — so the signature widens to the seat's own hand
and each seat's points taken, both of which every seat at the live table
sees.  Nothing about hidden hands or the void table: the scorer *cannot*
out-inform the field.  And it must be
screened as a rollout policy, not as a player: the question is whether
`mc:128` with the new rollouts beats `mc:128` with greedy rollouts, and
only secondarily whether the new `HeuristicBot` beats the old.

**Measurement.**  Four stages.  (0) Sensitivity, before any logging or
fitting: `mc:128` with a deliberately *worse* rollout policy (uniform
random legal, or the pre-point-aware greedy) against `mc:128` with the
shipped one, 2,000 seed-0 blocks, the knob temporary.  If making the
policy worse barely costs `rank`, making it better will not pay either,
and P2 is cut here.  (1) Offline: held-out agreement with
`mc:256`'s choice, against greedy's agreement as the floor.  (2) Arena:
`mc:128`-new vs `mc:128`-old over 2,000 seed-0 blocks, `rank` primary,
`moons` watched, throughput within 5% (the scorer runs on every card of
every rollout trick — it must be cheap).  (3) Confirm on 6,000 seed-1
blocks; then the 1,600-deal Deep CFR rerun on seed 1, since the
ordinary-play residue against *him* is the number this campaign is
named for.

**Kill criterion.**  Stage 0 costing under 2 SE, stage 1 not clearly
above greedy's agreement, stage 2 `rank` not positive at 2 SE, or
throughput loss beyond 5%.  A positive `rank` with a falling moon
column is a *trade*, to be priced on the tournament rerun, not a ship.

**Stage 0 result (2026-09-29): passed.**  A temporary thread-local mode
swapped the policy inside `rollout_play`'s ordinary branches only.  The
incumbent, the shoot line and the field stayed shipped.  Each row is
`mc:128` with the swapped rollouts minus plain `mc:128`, paired, in a
greedy field.  The knob is deleted.

| Rollout policy | seed | `rank` | `points` | `win` | `moons` |
| --- | --- | --- | --- | --- | --- |
| pre-`d18d8d0` greedy (no P1) | 0 (2,000) | −0.0197 ± 0.0107 | −0.071 ± 0.104 | −0.0012 ± 0.0046 | +0.0044 ± 0.0023 |
| pre-`d18d8d0` greedy (no P1) | 1 (6,000) | −0.0202 ± 0.0062 | −0.157 ± 0.060 | −0.0026 ± 0.0026 | +0.0032 ± 0.0013 |
| uniform random legal | 0 (2,000) | −0.3319 ± 0.0123 | −2.982 ± 0.115 | −0.1028 ± 0.0049 | −0.0328 ± 0.0024 |

One rule-sized step back in rollout quality costs `0.020` rank at 3.3 SE,
about three times what a doubling of worlds buys (S3).  So the search
does feel its rollout policy.  P1's null was about P1: one narrow lead
rule, not rollouts in general.  The old policy's cost comes with the
old profile: more moons, and `win` flat, the same as the point-aware
change's own ledger.  That warns stage 2 to expect a `rank` gain paid
partly in moons.  Random rollouts show the lever is steep at the bottom,
as expected; they say nothing about the top.

## Interactions

- P1 and P2 are mutually exclusive in the long run: a fitted policy
  subsumes the lead rule, and P1's arena leg is cheap evidence about
  which features P2 should carry.
- S1, S3 and T2 all spend or save compute, so S3's pricing of the
  compute lever closed all three.
- P1 moves every rollout, so a P1 ship re-baselines S3.  The S3 probe
  ran on the pre-P1 policy, and P1 is neutral for the search, so the cut
  stands.
- P1 was neutral for the search even though it helps the live heuristic
  by 4.6 SE: rollouts only price candidates, and the gated incumbent
  moves with the policy.  P2's stage 0 then showed the search does feel
  a rule-sized rollout change (−3.3 SE), so P1's null was specific to P1.

## Appendix — measurement boilerplate

House discipline, unchanged: search on one seed, confirm on a fresh one;
`rank` primary, `points` magnitude, `win` the ship gate; a homogeneous
greedy field must print `0.000±0.000` exactly; latency is gated on
repeated full-round arena throughput, never an isolated bench; the
200-block seed-7 CSV byte-identical across serial and `parallel` builds
for any change that claims not to move decisions.  The strength tripwire
stays manual: `cargo test --release --test strength -- --ignored`.
