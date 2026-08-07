# Opponent-aware passing — pricing what the other three seats pass

**Status: PROPOSAL (2026-08-08).**  Design only — nothing below is
implemented.  Sibling: [passing-shape.md](passing-shape.md) covers the
void/shape axis; its P1 rewrites the shared pass policy and lands
before anything here that touches `pass_score`.

## Goal and non-goals

**Goal.**  Better passes through a better model of the other three
seats' passes: make the model's roles independently configurable,
measure its incoming-card prior, give it variance, and teach the
heuristic policy — which today models nothing — the parts of that
prior that vary per hand.

**Non-goals.**  No `Knowledge` ledger (removed by design; everything a
seat may know is derivable from the public history).  No play-policy
changes beyond settling the observation constant (P4).  No
restructuring of the void terms — that is the sibling doc.
Score-conditioned passing (`game_scores()` is legal and unread at pass
time) is real but out of scope for both docs.

## What the engine already models

The intuition "the bots ignore what others pass" is false, and the
proposals only make sense against the precise baseline.

### The greedy pass models nothing

`HeuristicBot::pass_cards` (`src/heuristic.rs:302`) is `greedy_pass`:
the top three of the per-card `pass_score` (`src/heuristic.rs:25-49`).
It reads nothing else — not `view.direction()`, not
`view.game_scores()`.  The post-pass hand is ten keepers plus three
cards from the giver, and the policy prices the keepers as if those
three arrivals were blank.

### The Monte Carlo search already prices the exchange

Every pass-phase world is a real round (`src/mc.rs:289-304`):

1. `sample_hands` deals all 39 unseen cards into three 13-card
   *pre-pass* hands (`src/mc.rs:214-222`).  At pass time nothing is
   known — no voids, no plays, `known_cards` empty — so the deal is
   uniform, and uniform is the exact posterior (see the inference
   section below).
2. The world is built with the real `view.direction()`
   (`src/mc.rs:294`) and each opponent submits
   `greedy_pass(hand, HeuristicConfig::default())` (`src/mc.rs:297`).
3. Our candidate triple is the fourth pass; the mechanics crate
   performs the exchange atomically and play opens with the
   post-exchange 2♣ holder.  The rollout then charges everything
   downstream: the three cards our modeled giver sends us, what our
   discards do in our receiver's hand, the refill of a suit we tried
   to void, the lot.

Two properties worth naming.  *Common random numbers*: the opponents'
passes are fixed before our candidate applies, so within one world
every candidate faces the identical incoming triple — the paired
comparison differs only by our own three cards.  *Direction realism*:
left/right/across geometry is already exact in every rollout;
direction blindness exists only on the heuristic side.

### The play-time observation model

Once the exchange resolves, `View::received()` turns `Some`, and world
sampling reweights by `pass_observation_likelihood`
(`src/mc.rs:454-460`): re-pass the giver's plausible pre-pass holding
with the same greedy model and pay a factor `0.75` per card that
misses the actually received triple.  Shipped; the pass-only form
measured favorable but unresolved (`win +0.0042 ± 0.0026`,
`rank +0.0046 ± 0.0058` over 2,000 paired deals).  It is inert at pass
time — `received()` is `None` during `Phase::Passing`
(`src/view.rs:152-159`).

### One policy, four call sites, three roles

| Site | Role |
| --- | --- |
| `src/heuristic.rs:302` | our own pass (greedy bot) |
| `src/mc.rs:297` | the opponent population in worlds |
| `src/mc.rs:455` | observation model at play time |
| `src/mc.rs:468` | candidate ranking — and the incumbent |

The fourth is the sneaky one: `pass_candidates` ranks by `pass_score`
under the default config, and its first triple is the incumbent that
challengers must beat by 1.5 paired standard errors (`beats`,
`src/mc.rs:746-764`).  The gate is asymmetric, so a scorer change
moves Monte Carlo behavior even when the candidate *set* does not
change.  Every proposal below states which roles it touches.

All four sites hard-code `HeuristicConfig::default()`.  When the
default retune shipped (`heart_weight` 2→0, `spade_guards` 3→5) the
models inherited it — but a bot *configured* away from the defaults
still ranks, models, and observes with the default policy.  There is
no knob: `MonteCarloBot` is `{rng, samples, gate, shooting}`
(`src/mc.rs:117-126`).  (A fifth hard-coded default — the rollout
`moon_defense` trigger at `src/heuristic.rs:397` — is play-side and
out of scope here.)

### What is not modeled

- **Variance.**  Opponents pass one deterministic triple per sampled
  hand.  The observation model explicitly tolerates opponents who
  differ from us (a miss costs 0.75, not 0); the generator contradicts
  it with a point mass.
- **Shooters.**  Rollout opponents never attempt a moon, at pass time
  or any other (measured territory — see P6).
- **Score conditioning.**  Modeled opponents pass the same at 95
  points as at 0.
- **Any heuristic-side prior.**  See above.

## Pass-time inference is impossible — a closed question

The tempting symmetry: play-time sampling weights worlds by the
observed incoming pass, so pass-time sampling should weight worlds
by... what?  There is nothing.  At pass time the `View` holds our
thirteen cards and no other card evidence: no plays,
`received() = None`, `known_cards` empty, the void table all-false.
The driver serializes passers N→E→S→W, and a later passer can see
`hand_len == 10` for an earlier one (`src/view.rs:136-138`) — the fact
*that* a seat passed, never what.  Uniform dealing over the 39 unseen
cards is therefore the exact Bayesian posterior, not an approximation.
The only lever over "what others probably pass" at pass time is the
**prior's quality** — the model itself (P0, P2) and what the policy
assumes arrives (P1, P3).  Do not retry inference here; the
information whitelist forbids the evidence by construction.

## Proposals

| # | Proposal | Roles touched | Class |
| --- | --- | --- | --- |
| P0 | name the model roles | none (plumbing) | enabler |
| P1 | measure the incoming prior | none (offline) | measurement |
| P2 | noisy opponent generator | worlds | cheap experiment |
| P3 | prior-informed `pass_score` | all four | high ceiling |
| P4 | settle the 0.75 constant | observation | measurement |
| P5 | direction-conditioned config | self | speculative |
| P6 | shooter-aware opponents | worlds | parked |

### P0 — name the model roles

**Mechanism.**  Give `MonteCarloBot` a `HeuristicConfig` via a builder
(mirroring `samples`/`gate`), threaded to the three pass-model sites
(`src/mc.rs:297`, `455`, `468`).  Default remains
`HeuristicConfig::default()`.  One field first; split into a self
model and an opponent model only when an experiment needs them to
differ.  Extend the arena's `mc:` bot spec with optional pass knobs
when the first sweep wants to face the harness.

**Why.**  Today the default config is the only expressible model, so
every experiment below is blocked or entangled.  Naming the roles also
buys the vocabulary this doc runs on: `mc.rs:297` models the
*population*, `468` models *us*, `455` observes with tolerance.

**Measurement.**  None — a refactor.  Acceptance: the default-config
build reproduces the 200-block arena CSV byte-identically, the house
precedent for rollout refactors.

**Kill criterion.**  None.  If every downstream sweep nulls, the knob
is reverted with the last null and recorded under `### Internal`.

### P1 — measure the incoming-pass prior (offline)

**Mechanism.**  A scratch analysis tool (an `#[ignore]`d test or an
example; never shipped): deal ~10⁶ random 13-card giver hands, apply
`greedy_pass(default)`, tabulate:

- P(receive Q♠ | we lack her), and the same for A♠ and K♠;
- the received rank histogram (prediction: hard high skew);
- expected received cards per suit, and the **refill curve** —
  P(≥ 1 received card of suit `s` | we hold `k` of `s`), k = 0..3;
- expected received spades below the queen.

**Why.**  Predictions, written down before the run so the run can kill
them.  The giver holds Q♠ with chance 1/3 when we lack her and the
model passes her unless five guards sit in the other twelve cards, so
P(receive Q♠) ≈ 0.3 — a large, currently unpriced arrival.  The
uniform baseline for refilling a 3-card suit is
`1 − C(29,3)/C(39,3) ≈ 0.60`; the greedy model skews it both ways
(short-suit cards carry void bonuses, but our short suit runs long in
the giver's hand), and the curve pins the number the sibling doc's
discount needs.  And expected received low spades ≈ 0 — they score
bare rank and carry no void bonus, so the model essentially never
sends them; if confirmed, the "expected incoming guards" adjustment to
the spade tier is a recorded no-build (see P3iii).

**Touch points.**  None in `src/`.

**Kill criterion.**  It is measurement; the output table lands in this
doc and feeds P2/P3.

### P2 — a noisy opponent generator

**Mechanism.**  In the pass branch of `sample_world`
(`src/mc.rs:296-303`), with probability ε per opponent, submit ranks
{1, 2, 4} of that hand's `pass_score` order instead of {1, 2, 3} —
one extra draw from `self.rng` per opponent; seeded determinism and
the parallel bit-identity invariant untouched (sampling is already
serial).  Softmax or top-k generality only if the crude form moves.

**Why.**  The generator delivers the modeled giver's unguarded Q♠ with
certainty ≈ 1 whenever sampled; a human giver sometimes keeps her.
The observation model already concedes exactly this — 0.75 per miss —
and the generator contradicts it with a point mass.  ε widens the
incoming distribution the candidate gate optimizes against.

**Roles touched.**  Worlds only — ranking, incumbent, and observation
unchanged.  The safest model-touching proposal.

**Measurement.**  Needs P0.  `arena --ab` at ε ∈ {0.1, 0.25, 0.5} vs
ε = 0: a 2,000-block screen, then ≥ 6,000 paired duplicate blocks
across two seeds for a survivor (model effects here have historically
been +0.003..0.012 rank).  `rank` primary, `win` gate, and the `mc:64`
pass line of `benches/decision.rs` within +10% of its 5.87 ms
baseline.

**Kill criterion.**  No arm clears +2 SE `rank` at confirm, or the
latency budget blows — revert, record the null under `### Internal`.

### P3 — prior-informed `pass_score`

The ceiling is the retune precedent (`+0.188 ± 0.006` rank); the floor
is the `void_weight = 2` refutation (`−0.20 ± 0.42` pp at 8,000
games).  The **absorption principle** separates them: `tune` fits the
knobs on full games with real exchanges, so any *hand-independent*
prior is already inside the shipped defaults.  Only two kinds of new
content exist: per-hand-varying conditioning the current terms cannot
express, and granularity the integer grid never tried.

**(i) Fractional void weight.**  Scale every `pass_score` term by 4
(rank ×4, spade tier 400/360/320, heart and void terms ×4) — a
monotone map, so ordering and ties are untouched and acceptance is a
byte-identical arena CSV at the new default `void_weight = 4`.  The
integer knob now steps in quarters of the old unit; sweep {2, 3, 4, 5}
(≡ 0.5, 0.75, 1.0, 1.25).  The refill prior says a voided suit stays
void well under half the time, which the old grid could express only
as 0 or 1; the optimum may sit between.  (`greedy:V,H,G` arena specs
change meaning with the scale — one release-notes sentence.)

**(ii) Queen insurance.**  New knob, default 0 (off): when the hand
lacks Q♠, below-queen spades gain keep value (a `pass_score` malus)
sized against P1's measured P(spade honor arrives) — after the
exchange they guard against the incoming queen and against the
low-spade smoke-out leads the greedy policy itself plays
(`src/heuristic.rs:100-110`).  Honest expectation: small — the policy
already keeps low spades on bare rank except at the margin against
short-suit cards.

**(iii) The predicted no-build.**  Raising effective `spade_guards` by
the expected incoming guard count.  P1 is predicted to measure ≈ 0
incoming below-queen spades; if so, record the no-build and stop.

**Roles touched.**  All four — this is the proposal the coupling table
exists for.  Mitigation: the **pool bridge**.  Before any change to
the shared scorer's defaults, run the modified scorer beside the
vanilla one in `pass_candidates` and append its top triple as one
extra candidate when the two disagree (dedup as usual).  Cost ≈ +1
candidate when it fires — the width-7 null priced +15 always-on
triples at +34% latency, so one conditional triple is noise — and the
1.5-SE gate adjudicates the new scorer's opinion with real rollouts
before it becomes anyone's default.

**Measurement.**  Greedy leg: `tune` two-stage (search one seed,
confirm the single best arm on a fresh seed), then `arena --ab` with
`greedy:V,H,G` specs extended for the new knob, ≥ 4,000 blocks.  MC
leg (the incumbent moves): a separate `arena --ab` at `mc:128`,
≥ 6,000 blocks across two seeds.  The pool bridge is measured first as
a pure pool change.

**Kill criterion.**  Per sub-part: stage-two `tune` fails, or an arena
leg fails `rank` ≥ 2 SE or the `win` gate — revert that sub-part
alone; each null recorded.

### P4 — settle the observation constant

**Mechanism.**  Stage one, no code: rerun shipped-vs-disabled soft
pass inference at 8-10,000 paired blocks across two seeds — the
shipped result is favorable but unresolved at 2,000.  Stage two (needs
P0): sweep the fiat miss factor 0.75 ∈ {0.6, 0.75, 0.9}.

**Why.**  This is the one place "what others pass" meets *evidence*,
and it runs on an untuned constant.

**Kill criterion.**  Still unresolved at 10,000 — keep it shipped (it
is cheap), record the spend, and stop.

### P5 — direction-conditioned passing

**Mechanism.**  A per-direction `HeuristicConfig` override selected in
`HeuristicBot::pass_cards` (and, via P0, as the MC self model);
`greedy_pass(hand, config)` stays signature-pure.  Worlds stay
consistent for free — `sample_world` already builds with the real
direction.

**Why (hypotheses only).**  The geometry is code fact: my left
neighbor acts immediately after me in trick order, my right neighbor
immediately before, and across, my giver is my receiver — danger
concentrates bidirectionally in one opponent.  Table lore disagrees
about what follows; this doc commits to nothing.

**Why last.**  A per-direction grid multiplies `tune` arms three- to
four-fold while each knob sees a third of the passing rounds — an
order of magnitude more games for the same resolution, against a weak
prior.  Run only if P3 ships something, i.e. evidence the scorer has
headroom at all.

**Kill criterion.**  A stage-one sweep with no arm ≥ 2 SE records the
null and closes the direction question for good.

### P6 — parked: shooter-aware opponent passes

Rollout opponents never shoot, so a candidate that ships A♥ K♥ to a
monster hand is priced as if the monster ducks.  The adjacent
machinery was built and reverted: promoting an observed sweeper to a
rollout moon attempt worked mechanically and cost
`−0.0256 ± 0.0073` rank (3.5 SE) against a greedy field.  Parked until
the opponent pool actually shoots — the Deep CFR yardstick moons ~18%
of rounds and is the revival trigger; self-play is not.

## Sequencing

P0 ∥ P1 first (both risk-free; P1's table sets P2's ε range and P3's
term sizes) → P2 ∥ P3(i, ii), in separate arena windows and never
concurrent with the sibling doc's campaigns (the `void_weight = 2`
stage-one float was correlated-deal noise) → the P3 pool bridge before
any `greedy_pass` default change → P4 stage one whenever CPU is idle,
stage two after P0 → P5 only on P3 signal → P6 only against a field
that shoots.

## Interactions with the shape doc

- The sibling's P1 (sequential greedy) rewrites the shared policy the
  models here embed.  It lands first; the observation model's miss
  distribution and every generator above re-baseline on top of it.
- The refill discount composes *multiplicatively* with whatever void
  term the sibling ships, attaching to the marginal completion steps,
  never the first-card bonus.  Its customers are policy and candidate
  priorities only — the rollouts already price refill exactly.
- P1's refill-curve table publishes here; the sibling consumes it.

## Subtleties register

1. Hold rounds never enter `Phase::Passing` — `Round::from_deal`
   starts them in `Playing`, so `pass_cards` never runs there.  No
   pass logic can have a Hold bug; Hold merely dilutes pass knobs by
   about a quarter in game-mode tuning.
2. Across, giver = receiver, and no direction lets our passed cards
   bounce back — every pass is chosen from pre-pass hands.
3. The received-card skew under the greedy model: spade honors
   (P ≈ 0.3 for the queen), side-suit aces and kings, short-suit
   dumps; essentially never low spades.  P1 pins all of these.
4. `hand_len == 10` during passing reveals that a seat passed and
   nothing else; `sample_hands` already re-inflates to 13.
5. The absorption principle: hand-independent priors are already
   inside the tuned defaults (P3).
6. The incumbent coupling: scorer changes move Monte Carlo through the
   asymmetric gate even with an unchanged candidate set.
