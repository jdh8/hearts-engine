# Hearts engine improvement review

**Status: CLOSED (2026-10-18).** Reviewed version 0.1.1 at `e76c139`.
Two reproducible correctness bugs and one verification gap should be
addressed before another strategy-tuning campaign. Score-aware midgame
play was a separate research opportunity; it closed by rescaling the
mid-game margin instead.

## Reset moon plans before forced plays

**Status: fixed in 0.1.2** — the reset now precedes both early returns,
and `a_laydown_moon_is_rolled_as_one` covers the second round.

In [`MonteCarloBot::play_card`](../src/mc.rs), the single-legal-card
return precedes the code that clears `shooting` on the first trick.
`pass_cards` does not clear it either. After a completed moon, a bot
whose first play in the next round is forced can retain the previous
round's shooting plan. Once the first trick finishes, the first-trick
reset no longer applies; with nobody else holding points, the stale
plan can select `shoot_play` without a new search.

**Reproduction.** Extend the existing
`a_laydown_moon_is_rolled_as_one` unit test after its successful moon.
Reuse that bot in `fixed_table()`, where North must open with 2♣:

```rust
assert!(bot.shooting);
let mut next = fixed_table();
assert_eq!(next.turn(), Some(Seat::North));
assert_eq!(next.view(Seat::North).legal_plays().len(), 1);
next.step(&mut bot).unwrap();
assert!(!bot.shooting);
```

The final assertion failed in an isolated copy of the crate. This
reproduces the leaked state; its aggregate effect on playing strength
has not been measured.

**Fix and acceptance.** Apply the existing plan-reset conditions before
early returns. Keep the completed-moon fixture as a regression test
across two rounds, and preserve the existing test that a valid plan
continues within one round. A forced play should still consume no RNG.

## Make hints honor the live decision policy

**Status: fixed in 0.1.2** — `play_card` and `assess` share one
pre-search override (moon defense, then the carried plan), still scoring
every candidate so the flag and the top equity can differ.
`breaks_a_moon_the_rollouts_never_see` and
`a_carried_moon_plan_pins_the_hint` check the agreement.  The terminal
and web hint bots carry a plan through `set_moon_plan` once the human
plays the flagged shot.  A completion review found two gaps, both fixed
in 0.1.2: the frontends kept a plan across a new deal when the human's
opening play was forced (the engine's first-trick reset never ran), and
a fully collapsed hint tied every legal play with `shoot: false`, so
following the carried shot dropped the plan.  Both frontends now reset
hint state per deal (`a_new_deal_drops_the_hint_plan` covers the web),
and tied rows keep the shot's flag (`a_collapsed_shot_keeps_its_flag`).

[`MonteCarloBot::assess`](../src/mc.rs) scores candidates and calls
`recommended` directly. It bypasses both the live `moon_defense`
override and the carried shooting plan used by `play_card`, despite
`Assessment::recommended` promising the bot's own pick.

**Reproduction.** Reuse the view from
`breaks_a_moon_the_rollouts_never_see`: East has taken Q♠, and North can
duck with 3♠ or take the current point-bearing trick with K♠. Create
two fresh bots, each with `StdRng::seed_from_u64(1)` and `samples(64)`.
Call `play_card` on one and `assess` on the other at that same view.
The reproduced result is:

```text
live choice: K♠
recommended hint: play 3♠
```

The hint ducks the trick that the bot takes to stop the opponent's
moon. This affects consumers of `assess`, including the terminal and
web hints. The ordinary-position agreement test does not cover this
override.

**Fix and acceptance.** Share the live recommendation rules with
`assess`, including moon defense and an active shooting plan. Preserve
the distinction between the recommended move and the highest raw
rollout equity. Add agreement checks for these states using equivalent
bot state and RNG seeds; sequential calls on one bot would consume
different random samples.

## Enforce serial and parallel equivalence

**Status: fixed in 0.1.2** — CI's `parallel-equivalence` job plays
`--blocks 200 --seed 7 --csv mc:64 greedy greedy greedy` in both builds
and `cmp`s the CSVs.  The unit test is renamed
`seeded_pick_is_repeatable` and claims only within-build repeatability.

The test `seeded_pick_is_identical_across_serial_and_parallel_builds`
in [`src/mc.rs`](../src/mc.rs) runs two identically seeded bots inside
one build and compares their cards. It proves repeatability within
that build. CI runs it with both feature configurations, but both can
pass while choosing different cards. The comment refers to a pinned
expected card that the assertion no longer contains.

**Fix and acceptance.** Automate the existing 200-block seed-7 arena
CSV comparison between default and `parallel` builds, using identical
arguments. The outputs must be byte-identical. A common pinned expected
result in both builds would be a smaller but narrower guard. Keep
within-build repeatability as a separate claim.

## Investigate score-aware midgame play

**Status: done in 0.1.2** — score-aware equity tied a plain steeper
margin; the margin's scale was the real lever, and `/112` became `/24`.

All runs below are `--games` A/Bs of `mc:128` in slot 1 against three
`greedy` seats, 4,000 blocks unless noted, deltas as `rank` / `win` /
`points`.

**Standings model.**  Each opponent beaten with logistic probability in
the post-round total gap at `spread · √rounds_left` points (rounds left
from the leader's distance to the target over 8), expected matchpoints
squeezed into the (¼, ¾) band.  Seed 1 peaked at spread 3.5–7
(`rank +0.033` and `+0.024`, 5.2 and 4.0 SE) and fell off both ways
(0.875: −4.7 SE; 28: −6.6 SE).  Seed 2 confirmed spread 7:
`rank +0.033 ± 0.006`, `win +0.023 ± 0.004`, `points −0.09 ± 0.02`.

**The control.**  `spread` also sets how steep the mid-game values are
against the fixed terminal matchpoints, so the plain margin was rerun at
other scales.  It improved monotonically from `/112` down to a plateau:
`/80` `rank +0.011`, `/40` `+0.015`, `/28` `+0.020`, `/20` `+0.020`,
then `/14` `+0.014`, all on seed 1.  Head-to-head on seed 4 over 8,000
blocks, spread 7 minus `/24`: `rank +0.001 ± 0.004` (+0.3 SE),
`win +0.005 ± 0.003` (+1.9 SE), `points −0.133 ± 0.015`,
`not-last −2.6 SE`.  The standings model buys nothing measurable over
the rescaled margin.

**Shipped.**  `/24` against `/112` on fresh seed 5, 8,000 blocks:
`rank +0.019 ± 0.003` (+6.3 SE), `win +0.013 ± 0.002` (+6.9 SE),
`not-last +0.004 ± 0.001`, `points +0.024 ± 0.009`, moons per round
`−0.005` (−22.7 SE).  The equity is affine in the margin, so the
moon-majority break-even is unchanged.  The cost is the old band:
mid-game values now leave `[0, 1]`, and the hints clamp expected place
to `[1, 4]`.  Equity costs nothing next to the rollouts, so wall-clock is
unchanged.

**Not ruled out.**  This tests one family of score-aware equities.
Score-conditioned passing, moon decisions read off the standings, and a
standings model fitted to arena data rather than hand-built stay open.

The current `equity` function uses running standings to detect and rank
a game-ending rollout. If the game continues, it uses only the round's
point margin, so the same round outcome receives the same value whether
the deciding seat is leading or trailing, provided neither position
ends the game.

A score-aware midgame policy could change priorities before the final
round. This is a hypothesis, not a demonstrated strength improvement.
Any experiment should preserve the terminal matchpoint semantics and
documented midgame equity band, and use paired full games: screen on
one seed, confirm on a fresh seed, read `rank` as the primary detector,
`points` as magnitude, and `win` as the ship gate. Report moons and
wall-clock cost too.

The closed [ordinary-play campaign](ordinary-play.md) records weak
returns from more samples and a failed distilled rollout policy. The
[opponent-model campaign](passing-opponent-model.md) explicitly leaves
score-conditioned passing outside its scope. These results motivate a
distinct experiment, not reopening the rejected mechanisms unchanged.

## Verification at review

The existing verification gate passed: formatting, Clippy with all
targets and features, rustdoc with warnings denied, default and
all-feature tests, and the no-default-features check. The ignored
strength and pass-prior measurements were not run.

Both bug reproductions failed as expected in an isolated copy. Their
fixture names and reproduction steps above preserve the evidence;
the repository's runtime code and test suite were unchanged by the
review. All of them have since landed in 0.1.2; each section's status
names its regression tests.
