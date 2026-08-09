# Closing the Deep CFR gap — the moon campaign

**Status: OPEN (2026-08-10).**  P0's instrumentation has landed while
its live smoke/baseline remains in flight; P1 and P2 were refuted at
confirmation and shipped no change; P3 is next.  Siblings:
[passing-shape.md](passing-shape.md) (owns P1's mechanism) and
[passing-opponent-model.md](passing-opponent-model.md) (whose parked P6
is the nearest relative of this doc's P4).

## Goal and non-goals

**Goal.**  Close the measured gap to the strongest external yardstick we
have, brianberns' Deep CFR player, which the 2026-08-09 rerun puts at
`+0.727 ± 0.202` payoff per seat-deal against `mc:128`.  The
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

Two runs, same methodology (Brian's `Tournament.fs`: duplicate 2v2
deals, both seatings of each deal, zero-sum payoff
`mean(others' points) − own`), same seed, against his live server:

| | 2026-07-18 | 2026-08-09 |
| --- | ---: | ---: |
| deals (pairs), seed | 800 (400), 1 | 800 (400), 1 |
| samples, throttle | 128, 100 ms | 128, 100 ms |
| Deep CFR payoff per seat-deal | `+1.283 ± 0.212` | `+0.727 ± 0.202` |
| points per deal, cfr / mc | 15.95 / 19.80 | 15.32 / 17.50 |
| completed moons, cfr / mc | 142 / 8 | 81 / 24 |
| engine | pre-moon `mc:128` | commit `2405636` |

The first run's engine had no Monte Carlo moon machinery at all — no
shoot candidates, no live defense overlay on the bot.  The second run's
commit had to be recovered from the binary's mtime, which is one of the
gaps P0 closes.  Both runs carry the Q♠-breaks-hearts normalization
caveat (`tournament/README.md`): the shim replays his engine under our
hearts-breaking rule, denying his model one trained-for heart lead.

**The payoff identity.**  For any deal, the CFR side's mean seat payoff
reduces to `(mc points − cfr points) / 3`: a seat's payoff is
`(S − 4·own)/3` for deal total `S`, so the two CFR seats average
`(S − 2·cfr)/3 = (mc − cfr)/3`.  Both headlines reproduce exactly —
`(19.80 − 15.95)/3 = 1.283` and `(17.50 − 15.32)/3 = 0.727` — so the
whole gap is expressible in penalty points per deal: 3.85 then 2.18.

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

**What the interim work bought.**  Between the runs the engine gained
the shoot machinery itself (candidates, the double bar, the latch), the
live `moon_defense` overlay and the eight-point rollout defense
trigger, matchpoint-paying terminal rollouts, point-aware greedy play,
the sequential pass policy with its retuned defaults, and a 1.5-SE
gate.  Completions tripled (8 → 24), moons conceded fell 43%
(142 → 81), and points per deal improved on both sides of the table —
his fell too, because fewer of his deals end at 0.

**The checkpoint trail, and its limits.**  No per-deal data survives
from either run; the only within-run structure is the every-20-pair
cumulative checkpoint (journald, `hearts-cfr-tournament` unit):

| pairs | payoff ± SE | moons mc / cfr |
| ---: | ---: | ---: |
| 20 | `−0.617 ± 1.047` | 1 / 2 |
| 40 | `+0.592 ± 0.669` | 3 / 8 |
| 80 | `+0.529 ± 0.487` | 5 / 16 |
| 200 | `+0.668 ± 0.292` | 11 / 44 |
| 300 | `+0.739 ± 0.230` | 16 / 60 |
| 400 | `+0.727 ± 0.202` | 24 / 81 |

His completions accumulate near-linearly; ours arrive in clusters and
mostly in the back half.  At 20-pair cumulative granularity that is an
observation, not a result — which is precisely the problem.  What the
record cannot answer: how many moons we *attempted* and lost
(completions are the only moon signal the harness keeps), whether the
moons we concede are fed by our own passes, whether our defense engages
late, and which deals carry the ordinary-play residue.  Those four
questions gate P4, P5, and the campaign's next tournament verdict, and
every one of them is a harness deficiency, not an engine one.  Hence
P0.

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

The first four define the design space for anything touching attempt
selection: the double bar is load-bearing, and both flat and
decision-conditioned loosening are now dead.  A future revision needs
new evidence or a new mechanism, not another threshold shape.  The
middle four wall off rollout-world opponent modeling.  The last two say
the moon column moves under changes aimed elsewhere — every arena A/B
in this campaign reads `moons` alongside `rank` whether or not moons are
the target.

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
| P4 | anti-moon passing | pass policy | parked on P0 data |
| P5 | earlier live moon defense | overlay | parked on P0 data |

### P0 — tournament instrumentation

**Status: INSTRUMENTATION LANDED; LIVE SMOKE/BASELINE IN FLIGHT
(2026-08-10).**

**Mechanism.**  Four additions, none touching a decision path:

- Two lifetime-monotone counters on `MonteCarloBot`, read by diffing:
  `moon_attempts` (the `shooting` latch's false→true transitions) and
  `moon_passes` (shoot-flagged pass choices).  The attempt/completion
  ratio the shoot feature was tuned on becomes observable in any
  harness, not only the arena.
- `--csv PATH` on `examples/vs_cfr`, mirroring the arena's: one row per
  deal-seating — pair, seating, direction, `deal_seed`, CFR seats, the
  four raw seat scores, shooter seat and side, the mc bots' per-seating
  attempt deltas, and the seating's mean CFR seat payoff — flushed per
  pair, so a crash keeps its prefix.
- Provenance: best-effort `git rev-parse --short HEAD` printed at
  launch, in the final report, and in the CSV header, alongside seed,
  samples, throttle and shim command.  Never again a commit recovered
  from an mtime.
- Per-pair bot reseeding from `deal_seed` (salted), so any single pair
  replays in isolation; today the bots' RNG state threads through the
  whole run and pair *k* is unreachable without replaying `0..k`.

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
numbers from its own rows.  Strength is untouched by construction.

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

**Status: NEXT (2026-08-10).  Ranked low.**

**Mechanism.**  `shoot_play` opens every shot the same way: cash the
highest card.  Some moon hands want the opposite — a long suit headed
A-K-Q with a stopper out prefers to lead *low*, flushing the stopper
while entries remain.  Add exactly one alternative shoot candidate at
the same insertion point, behind the same `alive` gate; the latch
records which line was chosen and replays it.  No re-deciding and no
un-latching — the three-overrides-in-ten measurement stands, and a
bail-out policy would re-tread it in miniature.

**Why ranked low.**  Second-order: it widens the set of hands whose
best line can clear the existing strict-majority bar at all, after both
the pass-shape and conditional-bar attempts failed to improve strength.
It also doubles the most expensive candidate's rollout cost whenever
both lines survive elimination.

**Measurement.**  Screen 2,000 on `moons` + `rank`, confirm 6,000,
`win` gate, plus a full-round arena throughput gate — the passing-shape
campaign's lesson that an isolated bench delta is not a latency result
applies with force to a candidate the significance gate may extend.

**Kill criterion.**  The second line is never chosen, or chosen without
`moons` moving at screen — dead code, revert.  `rank`/`win` negative at
confirm — revert.

### P4 — anti-moon passing

**Status: PARKED on P0 data.**

The defensive half of the differential: 81 conceded completions per 800
deals.  The hypothesis is that some are fed — aces and high hearts
passed toward the seat our direction serves — and the mechanism, if it
is ever built, is a danger term in the pass scorer, the cheapest
defensive lever left (the live overlay already survived its sweep).
It is deliberately not designed further: today we cannot see which CFR
moons our passes fed, and the sibling's parked P6 is the adjacent idea
on the rollout side.  **Unlock condition:** a post-P0 rerun's CSV shows
at least a third of CFR completions received a dangerous card from us
(a working definition rides in the analysis, not the engine).  If the
decomposition shows conceded moons are not pass-fed, this closes
without a line of code — which is the cheap outcome and a fine one.
If built: arena `rank`/`win` non-regression is the gate, because the
arena's field rarely shoots and cannot show the benefit, only the cost;
the benefit is priced by the rerun after.

### P5 — earlier live moon defense

**Status: PARKED on P0 data, and expected to close unbuilt.**

The 5–12 sweep confirmed the live threshold of 8 against fields that
attempt moons in ~1–3% of rounds; against an opponent attempting ~10%
the optimum could sit elsewhere, and no arena field can measure that.
By house convention a change the arena refutes cannot ship, so this
parks until P0's CSV shows CFR completions regularly passing through
the eight-point window while our seats held a beat — defense engaging
demonstrably late.  Anything less and this proposal is the sweep null
re-proposed with extra steps, and the doc says so now to save the
argument later.

## Next-run protocol

- **Size.**  Paired SE scales as `1/√pairs` from the measured `0.202`
  at 400 pairs: 800 pairs (1,600 deals) → `±0.143`, which resolves a
  projected `+0.3` residual at ~2 SE; a `+0.2` residual at 2 SE wants
  ~1,600 *pairs*, unreasonable against a personal server.  The standard
  rerun is therefore **1,600 deals** (~5¼ h at 100 ms), with the note
  that the payoff variance itself shrinks as the moon differential
  closes — each completed moon moves its pair's entry by ±4.33, the
  dominant tail.
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

P0's instrumentation has landed; its instrumented 1,600-deal baseline
rerun remains outstanding.  P1 and P2 both closed without behavior
changes after confirmation refuted them, so P3 is next, in its own
arena window and never concurrent with the sibling docs' campaigns.
P4/P5 remain conditional on their own rerun evidence.  The
campaign closes when a rerun's headline is inside 2 SE of zero,
confirmed once on a fresh seed — or when the remaining proposals are all
measured nulls, in which case the honest close is "the residue is not
moons, and the next campaign is ordinary card play."

Ledger: the P0 counters and CSV are instrumentation and stay; P2's
clamp constants and validation path were deleted on their kill
criterion; P3's second line remains deletable on its own.  The journald
checkpoint trail stops being load-bearing the day the first CSV run
lands, and this doc's checkpoint table becomes the only place it
survives.

## Interactions with the sibling docs

- passing-shape.md's P3 **is** this campaign's P1; the verdict lands in
  both status ledgers, and its mechanism text stays there.
- passing-opponent-model.md's parked P6 (shooter-aware opponent passes)
  named "the Deep CFR yardstick moons ~18% of rounds" as its revival
  trigger; the rerun moves that to ~10% of deals.  The trigger still
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

# the standard rerun (overnight US/Eastern, checkpoints to journald)
systemd-run --user --collect --unit hearts-cfr-tournament \
    target/release/examples/vs_cfr -- --deals 1600 --seed 1 \
    --samples 128 --throttle-ms 100 --csv run.csv
```

Every reported result names the commit (P0 prints it), the deal count,
the seed, the throttle, and the Q♠ caveat.  Checkpoint stderr survives
in journald; the CSV is the record.
