# The Hearts Bot Arena gap — moons against a field that shoots

**Status: OPEN (2026-11-25).**  `rusty-hearts` v2 (commit `309faf6`,
`mc` capped at 1024 worlds, win-only payoff) sits 11th of 18 at 2327,
85–110 points under a pack at 2410–2440 and about 200 under the top two.
The site's own game records say the gap is moons: we complete
a third as many as the field, and the seat we pass to shoots more often
than it does behind anyone else's pass.  Ordinary card play is not the
problem.  P0 (an attempt census) is built and measured, and adds a third
finding larger than either: we start a shot on 5.4% of hands and land
about one in five.  P1 (the anti-feed pass, re-priced against a
field that shoots) is a measured null.  P2's calibration is measured
too: the rollouts' moon share says nothing about a shot's chances here
until it is unanimous — but the unanimous bar lost `win` by 12.9 SE
against the local field, which cannot price it.  That is the second
proposal to die on the instrument rather than on its merits, so the
campaign turns to P5: defenders calibrated to the site's pooled moon
statistics, first as the local field and then in the rollouts.  Siblings:
[cfr-gap.md](cfr-gap.md), whose closed proposals this doc re-reads under
a different gate, and [passing-shape.md](passing-shape.md).

## Goal and non-goals

**Goal.**  Win more games on the site.  Its rating counts wins only —
2nd, 3rd and 4th are one result — so the gate here is the arena example's
`win` column, not `rank`.  That is the one licence this campaign has to
reopen anything [cfr-gap.md](cfr-gap.md) closed: several of its
proposals died on `rank` with `win` flat or ahead.

**Non-goals.**  Nothing here changes the default engine.  `rank` stays
the gate for everything a library user gets; a change that wins on `win`
alone ships behind `MonteCarloBot::win_only` or in `arena/`, the way the
win-only payoff did.  No modeling of other authors' bots beyond what
their public games show.

## The record

All 11,070 games stored between our first (40881) and 51947, pulled
2026-11-24 from the public `GET /api/games/export`; 5,464 are v2.  Tables
with a house bot are left out of every comparison.  Each game carries all
four hands, every pass and every play, so the numbers below are counts,
not estimates.

Moons are the ecology: about 15% of passing hands and 8% of hold hands
end in one.

| bot | win % | pts / plain hand | shoots, % of hands | shot at, % of hands | recipient ÷ other |
| --- | ---: | ---: | ---: | ---: | ---: |
| Ava v3 | 35.9 | 5.11 | 3.69 | 10.28 | 1.12 |
| Wren v5 | 31.7 | 7.11 | 4.71 | 9.17 | 0.82 |
| Nightjar v1 | 28.0 | 7.05 | 3.24 | 8.81 | 0.98 |
| Moonwarden v4 | 27.6 | 8.78 | 7.35 | 8.65 | 0.63 |
| Killer-Hearts v3 | 27.6 | 6.66 | 3.81 | 8.46 | 0.96 |
| Brooke v2 | 25.2 | 5.48 | 2.51 | 11.58 | 1.03 |
| DEEP-CFR-JR v1 | 24.8 | 6.68 | 5.33 | 10.23 | 0.94 |
| Cleo v2 | 23.8 | 5.76 | 2.43 | 9.77 | 0.91 |
| Tafuta v1 | 23.1 | 6.52 | 3.58 | 10.75 | 1.03 |
| **RUSTY-HEARTS v2** | **21.9** | **6.03** | **1.50** | **11.60** | **1.17** |
| Kivuli v3 | 21.8 | 6.47 | 3.50 | 9.36 | 0.95 |
| Chui v1 | 18.6 | 7.44 | 3.44 | 7.81 | 1.03 |

"Plain hand" is one nobody mooned.  "Recipient ÷ other" is how often the
seat that received this bot's pass completed a moon, over the rate of
each of the other two opponents on the same hands.

**Ordinary play is fine.**  Fourth-lowest points on plain hands and the
highest share of zero-point hands (33.5%).

**We do not complete moons.**  1.50% of hands against 2.4–7.4% for every
other rated bot.  The same build completes about 5.6% of rounds against
our local `mc:128` and `greedy` fields, so the local fields flatter the
shot by a factor of nearly four.  P0 says where the difference goes: we
try, and fail.

**Moons against us decide our games.**  v2 wins 38.4% of games with no
moon against it, 21.7% with one, 9.8% with two, 1.4% with three or more;
73.6% of its games have at least one.

**The leak is the pass.**  The recipient of our pass shoots 1.17 times as
often as the other two opponents — the worst ratio on the board, in all
three directions (left 5.07 vs 4.15/4.12 per 100 hands, right 4.62 vs
4.28/4.04, across 4.67 vs 4.17/3.82).  What we pass explains it:

| passer | aces | A/K/Q | low ♥ (2–9) | 2–6 any suit |
| --- | ---: | ---: | ---: | ---: |
| **RUSTY-HEARTS v2** | **0.71** | **1.92** | **0.14** | **0.22** |
| Nightjar v1 | 0.60 | 1.70 | 0.10 | 0.32 |
| Tafuta v1 | 0.47 | 1.43 | 0.33 | 0.42 |
| Killer-Hearts v3 | 0.43 | 1.31 | 0.39 | 0.55 |
| Ava v3 | 0.37 | 1.22 | 0.31 | 0.43 |
| Wren v5 | 0.40 | 1.11 | 0.30 | 0.62 |
| Moonwarden v4 | 0.22 | 0.78 | 0.56 | 1.00 |

Cards per three-card pass.  We hand over more top cards than anyone and
fewer low ones, which is `pass_score` doing what it says: base rank.  The
recipient's moon rate climbs with the aces and kings in a pass (ours:
3.4%, 4.9%, 5.7%, 6.4% for zero to three; everyone else's: 2.4%, 3.6%,
4.5%, 5.1%) and halves when the pass holds a low heart (ours 5.4% → 2.6%;
others' 3.9% → 1.8%).

**It is not late defence.**  Counting moons in which a seat ducked a
trick it could have won after the shooter already held 13 or more points
and every point taken: we did so in 0.6% of the moons against us, the
best on the board (others 4.5–11.3%).

**We close worse.**  Entering the final hand with the lowest score we win
77.7% (Ava 83.2, Nightjar 86.1, Moonwarden 88.8); from second, 9.0%
(Killer-Hearts 13.4, Wren 19.8, Moonwarden 22.5).

## Priors this doc re-reads

Every row of [cfr-gap.md](cfr-gap.md)'s priors table stands as measured.
What changes is the question asked of four of them.

| Prior | Its verdict | Why it is open here |
| --- | --- | --- |
| anti-moon pass penalty (P4) | `rank −0.0165 ± 0.0053`, `win −0.0036 ± 0.0023` at confirm | priced against three `greedy`, which rarely shoot — its own text says the stage "could price only the ordinary-play cost".  The benefit side is now measured on the site. |
| decision-conditional moon bar (P2) | `rank +0.0000 ± 0.0026`, `win +0.0056 ± 0.0010` | failed `rank` only; `win` was +5.6 SE.  But see P0 before lowering any bar. |
| flat 45% majority bar | `win` +1.6 SE, `rank` flat | same |
| opponent-moon rollout model | `rank −0.0256 ± 0.0073` vs greedy | "worlds must not out-inform the field" — true of a field that does not shoot.  This one does. |

The win-only payoff itself is the precedent: `win +3.6/+5.3 SE` bought
with `rank −8.1/−2.7 SE`, shipped for this site alone, worth about +7
rating (2320 → 2327).

## Instruments

- **Site games** — the export above.  Free, exact, and the only
  measurement of the real field; it sees outcomes, not our intentions.
- **`scripts/arena-export.py`** — mirrors that export (`fetch`,
  resumable, a request a second), flattens it to one line per hand
  (`hands`) for `examples/field.rs` to replay through the real rules, and
  tabulates what each bot burns (`fuel`): fuel is metered per seat and
  public on every play, so it shows who searches.
- **`scripts/arena-census.py`** — replays our seat's public transcript
  through the arena wasm and reads the `shoot M/N` line `arena/` now
  prints on stderr (the shot completed in M of N sampled worlds).  It
  also counts each shot's *losers* at its first play: cards held that are
  not sure winners against the cards still out.  The wasm is deterministic in the `INIT` seed and the fuel
  figures on each prompt, and a replay whose replies differ from the
  transcript is dropped.  About 1.5 CPU-seconds a game.
- **The `mc:128` field** — unlike `greedy`, it shoots (about 5.5% of
  rounds a seat), so it can price a pass that feeds a shooter.  It is
  still a far softer defence than the site's.
- **Practice games** (`POST /api/practice`, needs the API key) — seat a
  draft against other authors' live bots at a chosen seed.  The only
  paired A/B against the real field, and it runs on someone else's server:
  ask the site's owner before scripting more than a handful, and use it
  for moon counts (hundreds of games), not `win` (thousands).

## Proposals

| # | Proposal | Touches | State |
| --- | --- | --- | --- |
| P0 | attempt census on site games | `arena/` stderr + a script | measured on 1,500 games |
| P1 | anti-feed pass, priced against shooters | pass policy | null (2026-11-25); knob deleted |
| P2 | the moon bar against real defenders | selection rule | calibrated; unanimous bar fails the local gate (`win` −12.9 SE), unpriced on the site |
| P3 | closing the last hand | win-only payoff | unbuilt |
| P4 | paired practice games | instrument | needs the owner's leave |
| P5 | calibrated defenders | instrument, then rollout defenders | stages 0–1 done (2026-11-26); stage 2 sweep queued |

### P0 — attempt census

**Status: MEASURED (2026-11-25), on 1,500 games.**

The 1,500 most recent v2 games, 14,642 hands, replayed with no divergence:

| | count | rate |
| --- | ---: | ---: |
| hands with a shot started in play | 796 | 5.4% of hands |
| … completed | 167 | 21.0% of shots |
| … failed | 629 | mean 19.0 points taken |
| shoot passes chosen | 399 | 2.7% of hands |
| … ending in our moon | 72 | 18.0% of shoot passes |
| moons completed with no shot logged | 38 | |

We do shoot — as often as the board's keenest shooters complete — and
one shot in five lands.  The strict-majority bar admits a shot only when
it completes in more than half of the sampled worlds, and the same build
completes about 70% of its attempts against the local fields, so the
rollouts' defenders are far softer than the site's.  Shots started on
the first three tricks are the worst: 86 of 471 (18%).

The cost is large.  A completed moon is a 26-point swing on each
opponent; a failed shot leaves us about 19 points against 6 on an
ordinary hand.  Over the 796 shots the realized margin
(`mean(others) − own`) is −7.3 points a shot, on 5.4% of hands — about
0.4 points a hand, four points a game.  Break-even in points is near 40%
completion.  This, not timidity, is why our moon count is the lowest on
the board, and it is the largest single leak this doc has found.

```console
python3 scripts/arena-census.py fetch  ab-results/arena-census --limit 1500
python3 scripts/arena-census.py replay ab-results/arena-census ab-results/arena-census/share.wasm
```

### P1 — anti-feed pass, priced against shooters

**Status: NULL (2026-11-25).  No `win` signal at either weight, and the
heavier one costs placement; the knob is deleted.**

The mechanism was cfr-gap P4's, rebuilt as an experimental knob:
`HeuristicConfig::feed_penalty` (doc-hidden, default 0; arena spec
`feed=N`) subtracted from `pass_score` for any queen, king or ace and for
T♥/J♥.  The spade tiers (+100/+90/+80) still moved an unguarded Q♠ and
her catchers at any weight tried, and the shoot-pass ballast was ranked
with the penalty off so a shooter kept its controls.  Through
`pass_model` it also reached the rollout opponents' passes and the
incoming-pass likelihood, as P4's did.

What was new was the field and the gate.  2,000 games, seed 21, paired,
against three `mc:128`:

```console
scripts/queue-ab.sh feed7-mcfield --games 2000 --seed 21 \
    --ab mc:1024,win=true,feed=7 mc:1024,win=true mc:128 mc:128 mc:128
scripts/queue-ab.sh feed4-mcfield --games 2000 --seed 21 \
    --ab mc:1024,win=true,feed=4 mc:1024,win=true mc:128 mc:128 mc:128
```

Weight 7 is P4's retention knee (it kept 54% of the cards CFR was fed);
4 was a milder arm in case 7 paid too much ordinary play.

**Gate.**  `win ≥ +2 SE` at either weight would have advanced the better
one to a fresh-seed confirmation of 6,000 games, with a greedy-field leg
holding `win ≥ −2 SE`.  Anything less: delete the knob, the ballast split
and the arena key, and record the null here.

**Result.**  Challenger minus baseline, paired over the same blocks:

| | `feed=7` | `feed=4` |
| --- | --- | --- |
| `win` | −0.0023 ± 0.0062 (−0.4 SE) | −0.0003 ± 0.0061 (−0.0 SE) |
| `rank` | −0.0389 ± 0.0159 (−2.4 SE) | −0.0158 ± 0.0156 (−1.0 SE) |
| `not-last` | −0.0211 ± 0.0062 (−3.4 SE) | −0.0094 ± 0.0063 (−1.5 SE) |
| `points` | −0.1201 ± 0.0566 (−2.1 SE) | −0.0325 ± 0.0541 (−0.6 SE) |
| `moons` (own) | +0.0040 ± 0.0011 (+3.6 SE) | +0.0001 ± 0.0010 (+0.1 SE) |

Neither arm clears the gate.  The dose-response runs the wrong way on
everything but the seat's own moons: weight 7 is worse than weight 4 on
`rank`, `not-last` and `points`, and its extra moons (the kept top cards
are a shooter's own controls) do not reach `win`.

**What the null does not say.**  The arena prints the challenger's own
`moons`, not the moons completed against it, so this cannot tell a pass
that failed to starve the `mc:128` shooters from one that starved them
at too high a price.  The plan was to add that column if the screen
landed within ±1 SE on `win`, which it did; the column was skipped
because the answer could not change the decision — a penalty that
already costs −2.4 SE of `rank` at the weight that retains the cards has
no arm left to ship.  The column is still owed if feeding is priced
again by another mechanism.

**Not tried yet.**  The site data points at a second, cheaper rule — put
one low heart in the pass — which halves the recipient's moon rate for
every bot on the board.  It is a different mechanism from a penalty on
top cards and was to get its own arm only if P1's knob showed the field
can price feeding at all.  The knob did not show that, so the arm stays
unqueued; it would need the moons-against column first.

### P2 — the moon bar against real defenders

P0 turns the inherited question around.  cfr-gap's P2 and the 45% bar
asked whether to shoot *more*; on this site the shipped bar already
shoots four times too often for what lands.  The local arena cannot
price a fix, because its fields let the same shots through, so the steps
are ordered by what each can be measured with:

1. **Calibrate** — done, below.
2. **Decide the arena build's rule** from that curve — done: the
   unanimous bar.  Arena-only, like `win_only`.
3. **Check it where it can be checked**: locally it must not lose `win`
   by more than 2 SE against the `mc:128` field (it will give up cheap
   moons there; the point is to bound the price), and on the site by
   P4's practice games or the next version's census.  Local check
   failed (below); the site check waits on P4.

#### Calibration (2026-11-25)

The census wasm rebuilt to print the share (`shoot M/N`), same 1,500
games, still no divergence.  Realized completion against the share of
worlds the shot completed in:

| in-world share | shots | landed | rate | mean points |
| --- | ---: | ---: | ---: | ---: |
| 50–60% | 237 | 28 | 11.8% | 15.7 |
| 60–70% | 166 | 27 | 16.3% | 15.4 |
| 70–80% | 120 | 26 | 21.7% | 15.1 |
| 80–90% | 112 | 23 | 20.5% | 16.0 |
| 90–99.9% | 83 | 18 | 21.7% | 16.7 |
| 100% | 78 | 45 | 57.7% | 8.5 |

The curve is flat where the bar lives.  From 70% to just under 100% the
rollouts' confidence buys nothing, and a shot they call nine in ten
lands one time in five.  Only unanimity carries information: one world
in a thousand with a defender who stops the shot is the difference
between 22% and 58%.  The 718 shots short of unanimous landed 17.0%.

Shoot *passes* never get there.  Their share tops out below 100%, and no
band lands above 29% (50–60%: 32 of 213; 60–70%: 22 of 114; 70–80%: 10
of 44; 80–90%: 7 of 24; above: 1 of 4).

#### The boss-card rule, priced on the same shots

Moonwarden, a bot of our own design, shoots most on the board (7.4% of
hands) by a rule its author describes as running a moon "only from
strong hands, and only … while its cards are still boss".  A proxy for
it, read off the public record at each shot's first play — *losers*,
the cards held that are neither above every outstanding card of their
suit nor long enough to outlast them:

| losers | shots | landed | rate | mean points |
| --- | ---: | ---: | ---: | ---: |
| 0 | 8 | 8 | 100.0% | 0.0 |
| 1 | 37 | 18 | 48.6% | 10.6 |
| 2 | 61 | 21 | 34.4% | 13.2 |
| 3 | 78 | 20 | 25.6% | 13.5 |
| 4–5 | 205 | 44 | 21.5% | 14.9 |
| 6+ | 407 | 56 | 13.8% | 16.3 |

Monotone, and "at most one loser" clears break-even on both halves of
the games (8 of 15 on even game ids, 18 of 30 on odd).  But it is the
weaker of the two filters, and most of what it knows the unanimous
rollouts know too:

| | shots | landed | rate |
| --- | ---: | ---: | ---: |
| unanimous, at most 2 losers | 45 | 34 | 75.6% |
| unanimous, 3 or more | 33 | 11 | 33.3% |
| short of unanimous, at most 2 losers | 61 | 13 | 21.3% |
| short of unanimous, 3 or more | 657 | 109 | 16.6% |

A boss hand the rollouts are not unanimous about is no better than any
other shot.  So the rule is one knob, not two: `MonteCarloBot::moon_bar`
(arena spec `bar=X`; default 0.5, the strict majority), set to 1.0.  The
losers term would lift 58% to 76% by refusing 33 more shots in 1,500
games that land a third of the time — near break-even, not worth a
second mechanism until the bar is on the site.

**What the bar gives up.**  On these games it keeps 45 of the 167 logged
moons (and the 38 that arrive unplanned), and refuses 718 shots that
took 15.7 points each with their 122 moons counted as zero.  It also
ends shoot passes, none of which reach 100%.  What those hands score
played straight is the one number the census cannot give.

**Queued (2026-11-25).**  2,000 games, seed 21, paired:

```console
scripts/queue-ab.sh bar1-mcfield --games 2000 --seed 21 \
    --ab mc:1024,win=true,bar=1 mc:1024,win=true mc:128 mc:128 mc:128
scripts/queue-ab.sh bar1-greedyfield --games 2000 --seed 21 \
    --ab mc:1024,win=true,bar=1 mc:1024,win=true greedy greedy greedy
```

**Gate.**  `win ≥ −2 SE` against three `mc:128` sets `moon_bar(1.0)` in
`arena/`.  The local fields let seven shots in ten through, so the bar
is expected to lose moons and points there; the gate bounds what the
site's gain has to cover, and the greedy leg is the softest field and
so the worst case for that price.  Worse than −2 SE: weigh the measured
local loss against the census's four points a game before deciding.

**Result (2026-11-25): fails, by far.**  `bar=1 − baseline`, paired:

| | against three `mc:128` | against three `greedy` |
| --- | --- | --- |
| `win` | −0.0574 ± 0.0044 (−12.9 SE) | −0.0336 ± 0.0040 (−8.4 SE) |
| `rank` | −0.1323 ± 0.0109 (−12.2 SE) | −0.0390 ± 0.0070 (−5.6 SE) |
| `not-last` | −0.0262 ± 0.0041 (−6.4 SE) | +0.0005 ± 0.0019 (+0.3 SE) |
| `points` | −0.540 ± 0.039 (−13.7 SE) | −0.527 ± 0.028 (−18.9 SE) |
| `moons` | −0.0276 ± 0.0009 | −0.0297 ± 0.0008 |

Half the moons go, and with them about 4.7 points a game against the
`mc:128` field (8.7 rounds a game).  The weighing does not clear it.
The census's four points count each refused shot against nothing; those
hands played straight are high-card hands and score something, which
leaves perhaps 2.5–3.5 points a game, the same size as a local price
that is itself inflated by soft defenders.  Neither instrument can
settle a change this large, and a new version resets the rating, so
`arena/` does not set the bar and the site check moves to P4.

The deeper fix is rollout defenders that play like this field, which is
the walled-off opponent-modeling prior.  It stays walled off until step 1
says the bar alone cannot do the job.

### P3 — closing the last hand

Leading into the final hand we convert 77.7% where the top bots convert
83–89%, and from second we win half as often as the bots that shoot.
Win-only already pays the terminal correctly, so the candidates are the
things it cannot see: rollout opponents that never shoot when a moon is
their only way to win, and a leader who does not defend against exactly
that.  Unbuilt, and behind P1 and P2 because both move the same hands.

### P4 — paired practice games

A draft and v2 at the same seed against the same three live bots is the
measurement every proposal above lacks.  Before building it: the owner's
leave, a throttle, and a look at how long a queued practice game takes.
The first use would be P1's recipient ratio and P0's completion rate on
a few hundred seeds, not a win rate.

### P5 — calibrate the field, don't clone it

**Why.**  P1 and P2 were both priced against `mc:128`, a field that lets
seven shots in ten through where the site lets two.  A third moon rule
would meet the same instrument, and the rollouts carry the same bias:
their defenders are our defenders.  The authors' own descriptions
(`GET /api/bots/<name>`) say where the field's strength comes from:

| bot | rating (2026-11-25) | what its author says |
| --- | ---: | --- |
| Ava v3 | 2562 | no method given |
| Brooke v2 | 2422 | 415k-parameter residual MLP distilled from Ava |
| Moonwarden v4 | 2410 | determinized search like ours, a learned pass, a rule-based shot |
| Nightjar v1 | 2385 | determinized search, fast heuristic rollouts, match-win payoff — this design |
| Cleo v2 | 2379 | 90k-parameter residual MLP |
| `rusty-hearts` v2 | 2360 | 1024 worlds over the greedy rollout |

The bot built exactly like ours sits 25 points up, and more worlds are
measured flat past 512.  What is left is who the rollouts think they are
playing against, which is also what the census found: defenders too
soft by a factor of three.

**Why not a clone.**  The first draft fitted a small network to the
strong bots' recorded decisions, as the local field and then as the
rollouts' opponents.  The export shows every hand face up, but the site
offers no way to download another bot's module, and that reads as a
choice: the bots are their authors'.  A network trained on Cleo's plays
recovers what that choice withholds, so the owner's answer is expected
to be no, and the draft is dropped rather than asked about.

**What instead.**  Tune our own defenders until the local field
reproduces the site's *pooled* outcomes — how often shots start and
land, and how often we win — and fit no bot's decisions.  That is the
census's use of the export, which needs no leave.  Our defence has one
knob today, the trigger: once a single opponent has swept that many
points and is winning the trick, stop ducking and take it.  It is 8,
hard-coded in three places (`HeuristicConfig::moon_defense`,
`MonteCarloBot`'s live preempt, and the shoot worlds in `rollout_play`).
The trigger goes first; a second knob (keeping a stopper rather than
ducking it away early) is built only if the trigger cannot reach the
target.

| stage | step | gate |
| --- | --- | --- |
| 0 | mirror the export; replay every hand through the real rules as each seat's `View` | no hand diverges — **passed** |
| 1 | the targets from the mirror: our shots started and landed (the census), our win rate, the field's moons per hand | written down with their SEs |
| 2 | the trigger as a `MonteCarloBot` knob, live and in the rollouts separately, today's value by default; sweep the field's | `rusty-hearts` v2 against three calibrated `mc:128` lands about 21% of its shots (not 70%) and wins about 22% |
| 3 | re-price P1 and P2's bar against the calibrated field | the usual `win` gate |
| 4 | the rollouts' trigger, against the calibrated field | `win` |
| 5 | confirm on the site (P4, or the new version's census), then upload v3 | site win rate |

**Costs and risks.**

- *The knob may be too small.*  A reactive trigger breaks a shot in
  progress; real defenders also keep stoppers from the pass on.  If no
  trigger value lands 21% of shots, stage 2 fails and says so, and the
  stopper knob is next.
- *Circularity.*  With one defence model in the field and the rollouts,
  the rollouts know the field exactly and the local A/B flatters them.
  The field is honest only on the statistics it was fitted to, and the
  site has the last word (stage 5).
- *Fuel.*  None to speak of: a trigger costs a comparison.  We spend a
  quarter of the pool on the mean hand (below).

**Expectation, not measurement.**  Cruder than a clone.  It repairs the
instrument that killed P1 and P2's bar, and the rollouts' largest known
bias, which could bring the entry to the Nightjar–Moonwarden level.  It
does not reach past 2450; this doc has no route there now.

**Stage 0: passed (2026-11-25).**

```console
scripts/arena-export.py fetch ab-results/arena-export
scripts/arena-export.py hands ab-results/arena-export \
    | cargo run --release --example field
scripts/arena-export.py fuel ab-results/arena-export
```

All 56,300 games through game 56,301 (525,603 hands, 1.66 million
passes and 27.3 million plays) replay through the real rules with no
divergence; no forfeited game was left out.

The fuel table, as a share of the per-hand pool (mean over hands, worst
hand):

| bot | mean | worst |
| --- | ---: | ---: |
| Wren v3–v5 | 92–96% | 97–99% |
| Moonbreaker, Moonwarden v2–v4, Nightjar | 60–69% | 84–92% |
| Chui | 33% | 54% |
| `rusty-hearts` v1, v2 | 23% | 76%, 88% |
| Tafuta | 8% | 59% |
| Kivuli, Deep-CFR, Killer-Hearts, Ava, Brooke, Cleo | ≤ 3% | ≤ 3.3% |

The field splits as the authors say: the searchers spend most of the
pool, the learned policies (Ava, Brooke, Cleo, the CFR bots) almost
none.  We spend under a quarter on the mean hand (88% on the worst,
which the Governor caps), so the budget is not what holds the entry
back.

**Stage 1: the targets (2026-11-26).**

```console
scripts/arena-export.py hands ab-results/arena-export > hands.txt
scripts/arena-export.py targets RUSTY-HEARTS:2 < hands.txt
```

All 6,457 v2 games in the mirror at tables with no house bot (61,443
hands); rates are ratios of per-game sums, SE clustered on games.  The
two shot rows are the census's 1,500 games with binomial SEs.  The local
column is the baseline row of P2's `bar1-mcfield` run: the same build
against three `mc:128`, 2,000 game blocks.

| | site | local, three `mc:128` |
| --- | ---: | ---: |
| `win` (1/k on a k-way tie) | 0.213 ± 0.005 | 0.280 ± 0.005 |
| own moons per hand | 0.0147 ± 0.0005 | 0.056 ± 0.001 |
| moons per opposing seat per hand | 0.0401 ± 0.0004 | 0.055 ± 0.001 |
| … passing hands | 0.0448 ± 0.0005 | |
| … hold hands | 0.0221 ± 0.0007 | |
| shots started, per hand | 0.054 ± 0.002 | unmeasured |
| shots landed, per shot | 0.210 ± 0.014 | about 0.7 |
| points per plain hand | 6.02 ± 0.03 | |
| hands per game | 9.52 ± 0.03 | 8.74 |

Two things the table says before any sweep.  The local field is wrong in
both directions the trigger moves: it lets our shots through (own moons
3.8 times the site's) *and* shoots more itself than the site's field does
against us (0.055 a seat against 0.040), so a harder defence in the field
alone would fix the first and leave the second, which is our own live
defence at work and already the board's best.  And the win target is
0.213, not 0.25: the calibrated field has to be stronger than `mc:128`
at ordinary play too, which no moon knob supplies — stage 2 matches the
moon rows and reports whatever `win` comes out.

**Stage 2: the trigger sweep (queued 2026-11-26).**  The trigger is now
two `MonteCarloBot` knobs, both 8 by default: `moon_defense` (the live
play; arena spec `def=N`) and `rollout_defense` (the defenders of a shoot
world; `rdef=N`).  The sweep moves the field's live trigger only and
needs no new column: with three identical opponents the `moons` rows are
our moons and the moons against us, a seat.

```console
for d in 1 2 4 6; do
    scripts/queue-ab.sh field-def$d --games 500 --seed 21 \
        mc:1024,win=true mc:128,def=$d mc:128,def=$d mc:128,def=$d
done
```

500 game blocks resolve `moons` to about ±0.002, against a gap of 0.041.
`def=8` is the local column above (same seed, 2,000 blocks).

**Gate.**  Some trigger puts our `moons` within 2 SE of 0.0147.  Then
that field is the instrument for stage 3, with its `win` and the field's
own moon rate (target 0.040 a seat) reported beside it.  If `def=1`
still leaves us above about 0.03, the trigger is too small a knob: build
the stopper knob, as planned.  Shots started and landed stay uncounted
locally unless the moon rows match and the split is needed.

## Sequencing

P0 → P2's calibration step (cheap, and the bigger leak) beside P1's two
screens → confirm or delete P1 → P2's rule (failed locally, 2026-11-25)
→ P5 stages 0–1 (done) → P5 stages 2–4 (stage 2 here now) → P4, with
the owner's leave, as the confirm → P3.  A new version resets the site
rating and takes about a thousand games to settle, so changes are batched:
nothing is re-uploaded before P5 stage 5.
