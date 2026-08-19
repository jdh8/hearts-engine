# Closing the Deep CFR gap — the moon campaign

**Status: OPEN (2026-08-19).**  P0 is complete: Deep CFR remains ahead
`+0.938 ± 0.151` over 1,600 deals, 6.2 SE from zero, and the net moon
differential still explains up to 77% of the gap.  P1–P3 were refuted,
P4 is unlocked by the pass traces, and P5 closes unbuilt.  Siblings:
[passing-shape.md](passing-shape.md) (owns P1's mechanism) and
[passing-opponent-model.md](passing-opponent-model.md) (whose parked P6
is the nearest relative of this doc's P4).

## Goal and non-goals

**Goal.**  Close the measured gap to the strongest external yardstick we
have, brianberns' Deep CFR player, which the 2026-08-19 P0 rerun puts at
`+0.938 ± 0.151` payoff per seat-deal against `mc:128`.  The
decomposition below says the remaining gap is still mostly the
completed-moon differential, so this is a moon campaign: raise the rate
and quality of our attempts, starve his, and make the tournament itself
measurable enough that the next rerun says *which* of those happened.

**Non-goals.**  No re-treading of the priors table's five nulls — in
particular, no opponent-moon modeling inside rollout worlds and no
exact endgame solving; both were built, measured, and reverted.  No
modeling of CFR internals: his attempts, his information sets and his
training variant stay opaque behind the shim, and every proposal here
must be measurable with our own instruments.

## The tournament record and the gap decomposition

Three runs, same methodology (Brian's `Tournament.fs`: duplicate 2v2
deals, both seatings of each deal, zero-sum payoff
`mean(others' points) − own`), same seed, against his live server:

| | 2026-07-18 | 2026-08-09 | 2026-08-19 (P0) |
| --- | ---: | ---: | ---: |
| deals (pairs), seed | 800 (400), 1 | 800 (400), 1 | 1,600 (800), 1 |
| samples, throttle | 128, 100 ms | 128, 100 ms | 128, 500 ms |
| Deep CFR payoff per seat-deal | `+1.283 ± 0.212` | `+0.727 ± 0.202` | `+0.938 ± 0.151` |
| points per deal, cfr / mc | 15.95 / 19.80 | 15.32 / 17.50 | 15.69 / 18.50 |
| completed moons, cfr / mc | 142 / 8 | 81 / 24 | 193 / 59 |
| engine | pre-moon `mc:128` | commit `2405636` | `2960003-dirty` (trace-only harness diff) |

The first run's engine had no Monte Carlo moon machinery at all — no
shoot candidates, no live defense overlay on the bot.  The second run's
commit had to be recovered from the binary's mtime, which is one of the
gaps P0 closes.  The P0 suffix records the uncommitted CSV trace and
provenance changes described below; the engine decision source is commit
`2960003`.  All three runs carry the Q♠-breaks-hearts normalization
caveat (`tournament/README.md`): the shim replays his engine under our
hearts-breaking rule, denying his model one trained-for heart lead.

**The payoff identity.**  For any deal, the CFR side's mean seat payoff
reduces to `(mc points − cfr points) / 3`: a seat's payoff is
`(S − 4·own)/3` for deal total `S`, so the two CFR seats average
`(S − 2·cfr)/3 = (mc − cfr)/3`.  All three headlines reproduce
from the points columns; P0 gives `(18.50 − 15.69)/3 = 0.937`, with the
one-thousandth difference from `0.938` due only to displayed rounding.

**Moon attribution.**  A completed CFR moon pays its shooter
`mean(26, 26, 26) − 0 = +26` and costs the CFR partner
`(78 − 4·26)/3 = −8.67`, so the pair banks `+17.33` on that deal —
`17.33/4 = 4.33` on the pair's payoff entry, `0.0108` per seat-deal
over 400 pairs.  The net completed-moon differential of `81 − 24 = 57`
therefore accounts for `57 × 0.0108 ≈ 0.62` of the `+0.727` headline
(~85%), or in points, `57 × 26/800 = 1.85` of the 2.18 per deal.  That
is an upper bound, not a measurement: the counterfactual to a completed
moon is not a zero-payoff deal, and failed attempts cost the shooter in
the ordinary-play residue instead.  But the direction survives any
reasonable discount, and the interim trend agrees — the payoff fell 43%
while the differential fell from 134 to 57.  The residual that is
*not* moons is at most a few tenths of a point of payoff: our ordinary
card play is close to his.

P0 confirms the same shape at twice the sample: its net moon differential
is `193 − 59 = 134`, worth at most
`134 × 26 / 1600 / 3 = 0.726` of the `+0.938` headline (77%).  The
remaining ordinary-play residue is about `+0.212`.  This is not closure:
the total gap is 6.2 SE from zero, and the P0 point estimate is compatible
with the earlier `+0.727 ± 0.202` rather than evidence of a shift.

**What the interim work bought.**  Between the runs the engine gained
the shoot machinery itself (candidates, the double bar, the latch), the
live `moon_defense` overlay and the eight-point rollout defense
trigger, matchpoint-paying terminal rollouts, point-aware greedy play,
the sequential pass policy with its retuned defaults, and a 1.5-SE
gate.  Completions tripled (8 → 24), moons conceded fell 43%
(142 → 81), and points per deal improved on both sides of the table —
his fell too, because fewer of his deals end at 0.

**The checkpoint trail, and its limits.**  No per-deal data survives
from the first two runs; the only within-run structure from 2026-08-09 is
the every-20-pair cumulative checkpoint (journald,
`hearts-cfr-tournament` unit):

| pairs | payoff ± SE | moons mc / cfr |
| ---: | ---: | ---: |
| 20 | `−0.617 ± 1.047` | 1 / 2 |
| 40 | `+0.592 ± 0.669` | 3 / 8 |
| 80 | `+0.529 ± 0.487` | 5 / 16 |
| 200 | `+0.668 ± 0.292` | 11 / 44 |
| 300 | `+0.739 ± 0.230` | 16 / 60 |
| 400 | `+0.727 ± 0.202` | 24 / 81 |

P0 keeps every deal in `run.csv`; selected cumulative checkpoints preserve
the comparable trail while adding attempts / shoot passes:

| pairs | payoff ± SE | moons mc / cfr | attempts / shoot passes |
| ---: | ---: | ---: | ---: |
| 20 | `+1.867 ± 1.075` | 1 / 6 | 5 / 0 |
| 40 | `+2.383 ± 0.792` | 3 / 15 | 11 / 3 |
| 80 | `+0.642 ± 0.544` | 6 / 21 | 17 / 8 |
| 200 | `+0.817 ± 0.329` | 12 / 49 | 44 / 20 |
| 300 | `+0.977 ± 0.253` | 20 / 73 | 73 / 31 |
| 400 | `+1.064 ± 0.220` | 28 / 100 | 94 / 46 |
| 600 | `+0.979 ± 0.181` | 43 / 154 | 133 / 62 |
| 800 | `+0.938 ± 0.151` | 59 / 193 | 172 / 78 |

His completions accumulate near-linearly; ours arrive in clusters and
mostly in the back half.  At 20-pair cumulative granularity that is an
observation, not a result — which is precisely the problem.  What the
record cannot answer: how many moons we *attempted* and lost
(completions are the only moon signal the harness keeps), whether the
moons we concede are fed by our own passes, whether our defense engages
late, and which deals carry the ordinary-play residue.  Those four
questions gate P4, P5, and the campaign's next tournament verdict, and
every one of them was a harness deficiency, not an engine one.  P0 closes
that deficiency; its proposal verdicts are below.

## Measured priors this doc must respect

| Prior (CHANGELOG / branch) | Result | Lesson |
| --- | --- | --- |
| shoot machinery, both bars | arena moons 36 → 85 per 4000 | attempts pay only behind both bars |
| gate alone, no majority bar | 2-in-5 attempts, 1-in-20 landed | the paired gate cannot price bimodal tails |
| flat 45% majority bar | `win` +1.6 SE only, `rank` flat | uniform loosening is dead |
| independent conditional moon bar | deal `rank +0.0000 ± 0.0026`; game `−0.0033 ± 0.0087` | more moons and wins traded away aggregate placement |
| eight-point rollout defense | `win +0.0033 ± 0.0006` (5.9 SE) | defender realism in shoot worlds pays |
| ordinary-world sweeper symmetry | null, moons −1.6 SE | `shooter == None` worlds stay greedy |
| opponent-moon rollout model | `rank −0.0256 ± 0.0073` vs greedy | worlds must not out-inform the field |
| maxⁿ endgame solve (`endgame-solver-null`) | −2.0 SE points, moons led | perfect-info defense kills real moons |
| `moon_defense` sweep 5–12 | 8 survives | the live trigger is priced — vs fields that rarely shoot |
| point-aware greedy play | `rank +0.036`, moons −0.39 pp | strength can trade against moon rate |
| `two_of_clubs_bonus` MC leg | null except moons +2.5/3.3 SE | pass shape reaches the moon column |
| shape-aware shoot passes | `rank −0.0054 ± 0.0024` at confirm | local shape did not improve attempt quality |
| flush-before-cash shoot line | `rank −0.0058 ± 0.0022` at screen | a live alternative line reduced both strength and moons |

The first four define the design space for anything touching attempt
selection: the double bar is load-bearing, and both flat and
decision-conditioned loosening are now dead.  A future revision needs
new evidence or a new mechanism, not another threshold shape.  The next
four wall off rollout-world opponent modeling.  The pass and ordinary-play
rows show why every arena A/B in this campaign reads `moons` alongside
`rank` even when moons are not the target; the final row closes the simple
second-line bet itself.

## Where the moon machinery lives today

All in `src/mc.rs` and `src/heuristic.rs`; function names, not line
numbers, because this doc will outlive the lines.

- **The shoot pass** — `pass_candidates`: exactly one candidate, the
  bottom three of the `pass_key` ranking ("the ballast a shot sheds"),
  rolled with a shooting continuation.
- **The shoot play** — `play_candidates`: exactly one candidate, the
  `shoot_play` card, appended past the `MAX_CANDIDATES` cap, generated
  only while no other seat has taken a point.
- **The double bar** — `recommended`: a shoot candidate must clear the
  paired `beats` gate (1.5 SE) *and* reach the moon in a strict
  majority of its sampled worlds.  The majority constant comes from
  mid-game arithmetic: moon ≈ 0.732, failed shot ≈ 0.423, greedy
  ≈ 0.577, indifference within a hair of ½.
- **The latch** — `play_card`: a chosen shot is carried, tricks played
  by `shoot_play` with no rollouts, until round end or another seat
  scoring.  Re-deciding measured out at three overrides in ten.  A
  chosen shoot *pass* deliberately does not latch: the pass-time
  majority was measured before the three incoming cards existed, so
  the shot must re-win the first play decision on the full hand.
- **The worlds** — `rollout_play`: in `shooter == Some` worlds the
  three defenders `beat_threat` only once the shooter shows eight
  points and is winning the trick; `shooter == None` worlds are four
  greedy duckers, deliberately blind (see the priors).
- **The live overlay** — `moon_defense(view, 8)` in
  `MonteCarloBot::play_card`, reactive, threshold hardcoded to the
  swept-and-confirmed 8.
- **Equity** — a game-ending rollout pays normalized `3-2-1-0`
  matchpoints; otherwise `0.5 + margin/112` pinned inside (¼, ¾).
  Moons flow through the scores alone (own moon ≈ 0.732, being mooned
  ≈ 0.268); there is no explicit moon term anywhere in the equity.

## Proposals

| # | Proposal | Touches | Class |
| --- | --- | --- | --- |
| P0 | tournament instrumentation | harness + two counters | enabler |
| P1 | shape-aware shoot passes | pool only | consumed from passing-shape P3 |
| P2 | decision-conditional moon bar | selection rule | structural fix |
| P3 | a second shoot line | pool only | shipped-pattern clone |
| P4 | anti-moon passing | pass policy | unlocked by P0 |
| P5 | earlier live moon defense | overlay | closed unbuilt |

### P0 — tournament instrumentation

**Status: COMPLETE (2026-08-19).**

**Mechanism.**  Four additions, none touching a decision path:

- Two lifetime-monotone counters on `MonteCarloBot`, read by diffing:
  `moon_attempts` (the `shooting` latch's false→true transitions) and
  `moon_passes` (shoot-flagged pass choices).  The attempt/completion
  ratio the shoot feature was tuned on becomes observable in any
  harness, not only the arena.
- `--csv PATH` on `examples/vs_cfr`, mirroring the arena's: one row per
  deal-seating — pair, seating, direction, `deal_seed`, CFR seats, the
  four rule-scored seat totals, shooter seat and side, the mc bots'
  per-seating attempt deltas, compact passes in NESW order, public plays
  grouped by trick, and the seating's mean CFR seat payoff — flushed per
  pair, so a crash keeps its prefix.  The traces are the minimum record
  needed to evaluate P4/P5 without replaying the live server.
- Provenance: best-effort `git rev-parse --short HEAD` printed at
  launch, in the final report, and in the CSV header, alongside seed,
  samples, throttle and shim command; `-dirty` marks an uncommitted
  source tree.  Never again a commit recovered from an mtime.
- Per-pair bot reseeding from `deal_seed` (salted), so any single pair
  replays in isolation; previously the bots' RNG state threaded through
  the whole run and pair *k* was unreachable without replaying `0..k`.

CFR-side attempts stay unobservable — the shim sees his card choices,
not his intent — and no proxy is invented; his completions remain his
only moon column.

**Coupling.**  None.  The counters write only where `shooting` is
already assigned; the CSV path is inert unless the flag is passed; the
reseeding changes this harness's decisions relative to the old binary,
which is fine because the next run is a fresh instrumented baseline —
cross-run pairing is by deal, and deals are unchanged.

**Measurement.**  Self-checks, not an A/B: the counter asserts ride the
existing laydown-moon and moon-defense unit tests, and the first
tournament smoke (4 deals, `--csv`) must reproduce the printed headline
numbers from its own rows.  The 2026-08-18 smoke at `2960003-dirty`, seed
1, `mc:128`, 500 ms did: both surfaces gave CFR
`−0.333 ± 0.333`, points 20.00 / 19.00, moons 0 / 1, one MC attempt and
one shoot pass; every pass/play trace was complete.  Strength is untouched
by construction.  `ruby tournament/analyze_csv.rb run.csv` performs the
same check on any pair-flushed prefix and the final file.

The full run at `2960003-dirty`, seed 1, `mc:128`, 500 ms completed
1,600 deals in 12 h 16 min despite recovered HTTP retries.  Stdout and the
CSV independently give CFR `+0.938 ± 0.151`, points 15.69 / 18.50, moons
193 / 59, 172 MC attempts and 78 shoot passes.  The MC completion rate was
59/172 = 34.3%.  The Q♠ normalization caveat applies.

**Kill criterion.**  None — if any output of the uninstrumented paths
changes at the same seed, that is a bug to fix, not a result.

### P1 — shape-aware shoot passes

**Status: REFUTED AT CONFIRM (2026-08-10).**  This is
passing-shape.md's P3, absorbed here as the campaign's first strength
proposal; its mechanism and full measurement table stay in the sibling
and are not duplicated.

**Why it led.**  The differential is the gap, and the shoot pass is
the earliest lever on attempt quality.  The current rule — the bottom
three of `pass_key` — systematically keeps short-suit junk (the void
term pushes short-suit low cards *up* the score, out of the bottom) and
sheds long-suit low cards that are masters-in-waiting behind a running
A-K-Q.  The hypothesis was that better-shaped shoot hands would raise
the completion rate that the majority bar then verifies over real
rollouts; at `0.011` payoff per net moon, every point of completion
percentage is worth about `0.09` payoff against CFR if his side holds
still.

**Measurement and verdict.**  The candidates were live: a 4,096-hand
probe selected d1 133 times and d2 24 times beside legacy.  Over the
2,000-block seed-0 screen, d1 and joint had small positive `rank`
estimates; joint led by `+0.0014 ± 0.0042`, passed the latency gate at
−3.0%, and became the sole confirmation arm.  On 6,000 fresh seed-1
blocks it lost `rank −0.0054 ± 0.0024`, `win −0.0034 ± 0.0010`,
`points −0.0843 ± 0.0335`, and completed `moons −0.0042 ± 0.0009`.
Both ship gates failed, so the legacy bottom-three shoot pass is
restored and every measurement scaffold is gone.  No tournament
transfer is claimed; the next P0 CSV rerun remains a baseline for the
surviving campaign, not a price for P1.

### P2 — decision-conditional moon bar

**Status: REFUTED AT CONFIRM (2026-08-10); strict majority restored.**

**Design correction.**  The proposed same-sample formula was
tautological.  For observed moon rate `p`, the shoot mean is
`p·E_moon + (1−p)·E_fail`, so its proposed inequality is exactly
`E_shoot > E_greedy` whenever `E_moon > E_fail` — already guaranteed by
the stronger `beats` gate.  After clamping, only the 0.35 floor could
filter anything; the 0.65 ceiling could not.  The experiment therefore
made the second bar independent: primary rollouts estimated the
threshold, then any shoot candidate that cleared `beats` received one
fresh base-width batch whose plain Bernoulli completion rate had to
clear it.  The predeclared 8/8 subset guard, `[0.35, 0.65]` clamp and
strict comparison stayed fixed.

**The arm was live and cheap.**  In the 2,000-block seed-0 screen it
calibrated below ½ on 0.1824 decisions/round and above ½ on 0.0105,
falling back on 0.1233 and validating 0.3161.  It advanced with
`rank +0.0008 ± 0.0044`, `win +0.0057 ± 0.0017`,
`not-last −0.0052 ± 0.0015`, `points +0.1095 ± 0.0587`, and
`moons +0.0106 ± 0.0015`.  Attempts rose by 0.0234/round but completed
at 67.7%, nowhere near the gate-alone pathology.  Repeated 500-block
throughput on seeds 0/1/2 was 283/280, 240/271 and 266/271 rounds/s for
majority/conditional; aggregate time favored the candidate by 4.5%.
A 200-block seed-7 CSV was byte-identical between serial and parallel
builds, and the unchanged majority arm reproduced its pre-change
200-block CSV byte for byte.

**Confirmation and verdict.**  On 6,000 fresh seed-1 deal blocks the
candidate kept the win and moon signal — `win +0.0056 ± 0.0010`,
`moons +0.0105 ± 0.0009`, `points +0.0591 ± 0.0346` — but produced
`rank +0.0000 ± 0.0026` and `not-last −0.0060 ± 0.0009`.  The required
+2-SE rank gate failed; attempts rose by 0.0248/round and completed at
68.2%.  The predeclared 2,000-game leg made the trade clearer:
`points +0.1059 ± 0.0302`, `win +0.0061 ± 0.0052`, and
`moons +0.0150 ± 0.0008`, against `rank −0.0033 ± 0.0087` and
`not-last −0.0062 ± 0.0025`.  Game attempts rose by 0.0286/round and
completed at 74.4%; this was not reckless shooting, just more wins and
moons bought with worse aggregate placement.  Both rank gates failed,
so the conditional bar, its fresh worlds, counters, arena knob and all
configuration were deleted.  The measured strict majority remains the
sole rule.

### P3 — a second shoot line: flush before cashing

**Status: REFUTED AT SCREEN (2026-08-10); cash-only restored.**

**Mechanism tested.**  A private `Cash`/`Flush` line replaced the
shooting boolean through candidates, rollouts and the live latch.  Cash
was the shipped policy.  Flush led the lowest legal rank, ties in
`Suit::ASC`, and otherwise delegated exactly to cash; both lines stayed
distinct even when their immediate play matched because their latched
continuations differed.  They sat together past `MAX_CANDIDATES`,
behind the same `alive` gate, 1.5-SE paired gate and strict-majority
completion bar.  Shoot passes remained cash-only.

**Liveness and determinism.**  A deterministic 4,096-deal probe rotated
the measured `mc:128` seat and selected flush 12 times among 264 total
attempts, completing 10 of those flush shots.  The line was rare but
live.  Serial and `parallel` candidate builds produced byte-identical
200-block seed-7 CSVs (SHA-256 prefix `134cd6d05cfe`).

**Screen and verdict.**  Against the saved cash-only executable over
2,000 paired seed-0 blocks, flush moved `points −0.0308 ± 0.0222`,
`win −0.0016 ± 0.0009`, `not-last −0.0015 ± 0.0008`,
`rank −0.0058 ± 0.0022`, and completed `moons −0.0001 ± 0.0005`.
Both predeclared screen signs — positive `rank` and positive `moons` —
failed, with rank negative by 2.7 SE.  The arm therefore did not reach
the throughput or fresh-seed confirmation gates.  The line enum,
candidate, latch changes, tests and probe were deleted; the engine is
byte-for-byte back to the cash-only source.

### P4 — anti-moon passing

**Status: UNLOCKED by P0 (2026-08-19).**

The defensive half of the differential: 81 conceded completions per 800
deals.  The hypothesis is that some are fed — aces and high hearts
passed toward the seat our direction serves — and the mechanism, if it
is ever built, is a danger term in the pass scorer, the cheapest
defensive lever left (the live overlay already survived its sweep).
Before P0 it was deliberately not designed further because the harness
could not show which CFR moons our passes fed; the sibling's parked P6 is
the adjacent idea on the rollout side.  **Unlock condition:** a post-P0
rerun's CSV shows at least a third of CFR completions received a dangerous
card from us.

The predeclared analysis definition is Q/K/A of any suit or T/J hearts:
high control, plus the high hearts immediately needed for the sweep.

P0 clears the gate decisively: 125/193 CFR completions (64.8%) received
at least one such card from an MC seat; 125/129 of the completions eligible
to receive from us on left/right deals did.  The conclusion is robust to
narrow definitions chosen after the predeclared test: ace-only is 78/193
(40.4%) and T-or-higher-heart-only 69/193 (35.8%), each still above one
third.  This is an experiment trigger, not causality — it says the proposed
lever reaches enough losses to test, not that a different pass would have
stopped them.  Arena `rank`/`win` non-regression is the gate, because the
arena's field rarely shoots and cannot show the benefit, only the cost;
the benefit is priced by the rerun after.

### P5 — earlier live moon defense

**Status: CLOSED UNBUILT (2026-08-19).**

The 5–12 sweep confirmed the live threshold of 8 against fields that
attempt moons in ~1–3% of rounds; against an opponent attempting ~10%
the optimum could sit elsewhere, and no arena field can measure that.
By house convention a change the arena refutes cannot ship, so this
parks until P0's CSV shows CFR completions regularly passing through
the eight-point window while our seats held a beat — defense engaging
demonstrably late.  Operationally, the analyzer counts a missed beat when
the shooter has 1–7 points, is winning the live trick, an MC seat holds a
legal higher card in the led suit other than Q♠, and its actual play does
not overtake; it separately reports rows in the swept 5–7 window and rows
where that trick carries the shooter across eight points.

P0 finds no regular late-defense seam.  Although 48/193 CFR completions
had some missed beat at 1–7 points, only 8/193 (4.1%) had one in the
actually swept 5–7 window, and only 7/193 (3.6%) had one on the trick that
crossed eight.  Those are opportunity upper bounds, not moons an earlier
trigger would necessarily prevent.  The prior 5–12 arena sweep therefore
stands; changing 8 would re-propose its null for a tiny reachable subset.

## Next-run protocol

- **Size.**  Paired SE scales as `1/√pairs` from the measured `0.202`
  at 400 pairs: 800 pairs (1,600 deals) → `±0.143`, which resolves a
  projected `+0.3` residual at ~2 SE; a `+0.2` residual at 2 SE wants
  ~1,600 *pairs*, unreasonable against a personal server.  The standard
  rerun is therefore **1,600 deals** (~5¼ h at 100 ms), with the note
  that the payoff variance itself shrinks as the moon differential
  closes — each completed moon moves its pair's entry by ±4.33, the
  dominant tail.
  P0 realized `±0.151`, close to the projection.
- **Seed.**  Development reruns stay on seed 1: `deal_seed` is a pure
  function of `(seed, pair)`, so every rerun is paired card-for-card
  with both recorded runs, and with P0's CSV the pairing finally
  resolves per deal instead of per 20-pair checkpoint.  When the
  campaign claims closure, confirm once on a fresh seed — the standard
  overfitting guard, owed doubly to a fixed 400-deal card set.
- **Etiquette.**  The runs hit Brian's live personal server: 100 ms
  throttle overnight US/Eastern (≈ Taipei midday), 500 ms–1 s during
  his day, checkpoints on, and results posted to the issue thread as
  before.  The Q♠ normalization caveat rides on every reported number.

## Sequencing and the deletion ledger

P0 and its instrumented 1,600-deal baseline are complete.  P1 and P2
closed without behavior changes at confirmation and P3 at screen.  P0
rules out campaign closure, unlocks P4 as the next engine experiment, and
closes P5 unbuilt.  The measured `mc:256` sample-count gain remains the
independent compute lever if a second yardstick leg is wanted; it does not
replace P4's mechanism test.  The
campaign closes when a rerun's headline is inside 2 SE of zero,
confirmed once on a fresh seed — or when the remaining proposals are all
measured nulls, in which case the honest close is "the residue is not
moons, and the next campaign is ordinary card play."

Ledger: the P0 counters and CSV are instrumentation and stay; P2's
clamp constants and validation path and P3's second line were deleted
on their kill criteria.  The journald checkpoint trail stops being
load-bearing the day the first CSV run lands, and this doc's checkpoint
table becomes the only place it survives.

## Interactions with the sibling docs

- passing-shape.md's P3 **is** this campaign's P1; the verdict lands in
  both status ledgers, and its mechanism text stays there.
- passing-opponent-model.md's parked P6 (shooter-aware opponent passes)
  named "the Deep CFR yardstick moons ~18% of rounds" as its revival
  trigger; P0 measures it at 12.1% of deals.  The trigger still
  reads as met in spirit — the yardstick field does shoot — but P6
  remains parked behind this doc's P4, which asks the pass-side
  question with data instead of worlds.
- The sibling's P0 (`pass_model` plumbing) is what lets P1's arms face
  the arena as `mc:` specs; nothing here re-plumbs.

## Appendix — measurement boilerplate

House discipline, unchanged: search on one seed, confirm on a fresh
one; `rank` primary, `points` magnitude, `win` the ship gate; a
homogeneous greedy field must print `0.000±0.000` exactly; latency is
gated on repeated full-round arena throughput, never an isolated bench.
The strength tripwire stays manual:
`cargo test --release --test strength -- --ignored`.

Tournament additions:

```console
# smoke: 4 deals, throttled, CSV must reproduce the printed headline
cargo run --release --example vs_cfr -- --deals 4 --throttle-ms 500 \
    --csv smoke.csv

# reproduce the headline and evaluate P4/P5 on a prefix or final run
ruby tournament/analyze_csv.rb run.csv

# the standard rerun (overnight US/Eastern, checkpoints to journald)
systemd-run --user --collect --unit hearts-cfr-tournament \
    target/release/examples/vs_cfr -- --deals 1600 --seed 1 \
    --samples 128 --throttle-ms 100 --csv run.csv
```

Every reported result names the commit (P0 prints it), the deal count,
the seed, the throttle, and the Q♠ caveat.  Checkpoint stderr survives
in journald; the CSV is the record.
