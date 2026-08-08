# Shape-aware passing — betting on voids and shapely hands

**Status: IN PROGRESS (2026-08-08).**  P1 has **shipped**, with its
mandatory re-sweep and its deletion A/B both run; the tie-break question
it left open is settled and a new P6 opened and closed alongside it.  P2
and P3 are unstarted and now unblocked.  Sibling:
[passing-opponent-model.md](passing-opponent-model.md), whose P1
measures the refill risk this doc's void bets are discounted by; this
doc's P1 lands before any of the sibling's scorer changes.

## Goal and non-goals

**Goal.**  Make the pass policy bet on shape: actually complete voids
instead of spraying void bonuses, reach the void triples the candidate
pool cannot express today (spade voids, double voids), and shed the
right ballast when the candidate is a moon attempt.  The thesis is
that the same bet pays on both sides of the game: a void or a shapely
hand is strong whether we defend (slough points into others' tricks)
or shoot (shed unwinnable cards, keep control) — and the existing
per-candidate machinery prices both sides without new statistics.

**Non-goals.**  No new equity shapes, no new gates, no hand-level keep
scorer (argued down as P4).  Refill modeling belongs to the sibling.

## Measured priors this doc must respect

| Prior (CHANGELOG) | Result | Lesson |
| --- | --- | --- |
| pass-default retune | `+0.188 ± 0.006` rank | biggest lever there is |
| forced void triples | `+0.0083 ± 0.0018` rank | set-level shape pays |
| `void_weight = 2` | `−0.20 ± 0.42` pp | scalar raises are dead |
| pass pool 6→7 | 0.9 SE, +34% latency | blanket widening is dead |
| 45% moon bar | reverted | shoot rides the majority bar |
| `mc:64` pass 5.87 ms | — | latency is a real cost axis |
| P1 sequential pass | `+0.0100 ± 0.0014` rank | structure beats scalars, again |
| P1's tie-break | `+0.0013 ± 0.0013` rank | ties are worth less than they look |
| `diamond_bonus = 1` | `−0.0075 ± 0.0013` rank | the minor-suit story is wrong |
| `two_of_clubs_bonus = 6` | `+0.0019 ± 0.0004` rank | a *card* can be new content |

The first two say passing and shape pay; the middle two say how *not*
to buy them — structure instead of scalars, targeted candidates
instead of wider pools.

## Where shape lives today

### One weak per-card term

`pass_score` (`src/heuristic.rs:25-49`) values shape in one clause:

```rust
// A short side suit is a void in the making; spades keep their guards.
let len = hand[card.suit].len() as i32;
if card.suit != Suit::Spades && len <= 3 {
    score += i32::from(config.void_weight) * (8 - 2 * len);
}
```

Singleton +6, doubleton +4, three-card suit +2 at the default
`void_weight = 1` — against a 2..14 rank scale and an 80..100 spade
tier.  Every card is scored against the *un-mutated* thirteen-card
hand, and `greedy_pass` (`src/heuristic.rs:52-56`) is one flat sort,
so three cards from three different 3-card suits collect +2 each while
emptying nothing.  The policy cannot see whether its triple creates a
void.

### Four consumers

The policy serves four call sites at once — own pass
(`src/heuristic.rs:302`), the rollout opponents (`src/mc.rs:297`), the
play-time observation model (`src/mc.rs:455`), and candidate ranking
(`src/mc.rs:468`), whose first triple is the Monte Carlo incumbent.
The incumbent identity is an *accident*: `pass_candidates` never calls
`greedy_pass`, it re-sorts by the same score (`src/mc.rs:477-485`).
Any change to the policy moves all four; the sibling doc's coupling
table is the reference.

### The Monte Carlo set-level patch

`pass_candidates` (`src/mc.rs:465-528`): the 20 triples over the top-6
pool, then — precisely because independent scores cannot complete a
void — a forced triple per **non-spade** suit of length 1-3, the
suit's cards padded with the highest-scored outsiders, deduped; plus
one shoot candidate, the bottom three scores.  The forced triples are
the only set-level shape reasoning in the crate and they shipped
`+0.0817 ± 0.0164` points/deal.  Gaps: spades are excluded
(`src/mc.rs:491`), each suit gets exactly one filler choice, and
double voids are generically unreachable — a singleton's fill is the
top-scored outsiders, which are rarely the doubleton's cards.

### The rollouts already price shape

A pass-phase world performs the real exchange and plays the round out,
so sloughing power, control, moon support, and refill are all inside
the equity already.  The MC-side question is only pool coverage and
incumbent honesty; the policy-side question is whether `greedy_pass`
itself can chase shape.  Worked example A shows both failing at once.

### Worked example A — everyone scatters

`♠8642 ♥J94 ♦Q83 ♣K72`, defaults.  Flat scores: K♣ 15, Q♦ 14, J♥ 13,
9♥ 11, 8♦ 10, 7♣ 9 — each short-suit card wearing its +2.  The greedy
pass K♣ Q♦ J♥ empties nothing: three two-card fragments.  No triple of
the top-6 pool voids any suit (3♦, 2♣, 4♥ all sit below the pool), so
the forced triples Q♦ 8♦ 3♦, K♣ 7♣ 2♣, J♥ 9♥ 4♥ are the only real
voids on offer.  Sequential selection (P1) *also* scatters here —
after K♣ leaves, the recomputed 7♣ (11) still loses to Q♦ (14) and
J♥ (13).  Set-awareness does not subsume the forced patch; they are
complements.

## Proposals

| # | Proposal | Touches | Class |
| --- | --- | --- | --- |
| P1 | sequential `greedy_pass` | policy (all four) | structural fix |
| P2 | spade voids, double voids | pool only | shipped-pattern clone |
| P3 | shape-aware shoot pass | pool only | dual-use bet |
| P4 | hand-level keep score | policy | argued YAGNI |
| P5 | 4-card-suit shortening | — | note and skip |
| P6 | per-suit and per-card hazard offsets | policy (all four) | half shipped |

### P1 — sequential set-aware `greedy_pass`

**Status: SHIPPED (2026-08-08).**  Built as designed; the numbers, the
settled tie-break and the two obligations it carried are in the closing
subsection below.

**Mechanism.**  Three rounds of: argmax `pass_score` over the
*current* hand, remove the pick, rescore.

```rust
pub(crate) fn greedy_pass(mut hand: Hand, config: HeuristicConfig)
    -> [Card; 3] {
    [(); 3].map(|()| {
        let pick = argmax_by_score_then_card_order(hand, config);
        hand.remove(pick);
        pick
    })
}
```

The tie-break must be explicit and pinned by a test: today's stable
sort resolves equal scores club-first via hand iteration order, and
the parallel feature's bit-identity rides on whatever rule replaces
it.  The void term now escalates along a completing suit — a
doubleton's cards collect +4 then +6, a three-card suit +2, +4, +6 —
so completion outscores scatter wherever ranks tie, at zero new knobs.

**Worked example B — the flip.**  `♠J86 ♥A6 ♦K2 ♣976432`, defaults.
Flat: A♥ 18, K♦ 17, J♠ 11, 6♥ 10 — the pass A♥ K♦ J♠ leaves both red
fragments alive.  Sequential: A♥ (18) leaves 6♥ a singleton worth
6 + 6 = 12; K♦ (17); then 6♥ (12) beats J♠ (11) — the pass A♥ K♦ 6♥
opens a real heart void and keeps 2♦ as a clean duck.  For the greedy
bot this is a policy change outright.  For Monte Carlo the same set
already sits in the pool (as a top-6 triple *and* as the forced hearts
triple), so the MC-side effect is **incumbent honesty**: the void
triple stops paying the 1.5-SE toll against a scatter incumbent.

**Why it respects the `void_weight = 2` refutation.**  Flat ×2 doubles
scatter and completion alike — measured dead.  Sequential leaves
first-card and singleton bonuses exactly where the tuned defaults put
them and raises only the completion path.  Structure, not a scalar.

**The spade-tier interaction.**  `guards` is recomputed from the
current hand on every call (`src/heuristic.rs:28`), so passing a
below-queen spade can flip the tier gate mid-selection and arm
+100/90/80 on a newly bare Q♠/A♠/K♠.  That is domain-correct — the
kept hand really will hold fewer guards, and today's flat policy can
pass two guards while still rating the queen protected — with one
myopia wart: a degenerate hand can spend a pick on J♠ (bare rank) and
only then discover the queen it exposed, burning two slots.  Shelf
variant **P1b** freezes `guards` at the thirteen-card hand (one line);
A/B it only if rollout transcripts actually show the loop.

**Coupling.**  All four consumer sites move: the greedy bot (the tune
leg measures it directly), the rollout opponents and observation model
(realism shifts), and the incumbent — `pass_candidates` must start
*calling* `greedy_pass` for candidate 0 and dedup it against the pool
triples, because the accidental sort-agreement breaks the moment the
policy is sequential (`src/mc.rs:477-485`).  CLAUDE.md's "candidate 0
is the greedy incumbent" invariant becomes enforced rather than
inherited.  A unit test pinning example B's flip guards the rewrite;
the existing pass tests produce identical triples under both forms
(checked by hand) and would not catch a regression.

**Latency.**  Replaces one Vec allocation and a full sort with three
alloc-free argmax scans (~39 `pass_score` calls); the policy runs
three times per sampled world plus once per observation reweight, so
expect neutral-to-faster.  Verify the `mc:64` pass line of
`benches/decision.rs` against its 5.87 ms baseline.

**Measurement.**  Greedy leg first — self-play is where a pure policy
change shows largest: cross-build paired game blocks (deal seeds are a
pure function of `(--seed, block)`, so CSVs pair across builds), a
2,000-block screen, then 8,000 on a fresh seed.  MC leg: `mc:128`
against three greedy, 2,000-block screen, then 6,000 across two seeds
— sized for effects on the forced-patch scale (+0.008 rank).  `rank`
primary, `win` gate.  Then the **mandatory re-sweep**:
`void_weight × spade_guards` via `tune` two-stage, because the knob's
unit changed from flat multiplier to marginal-schedule multiplier —
the resweep-after-retune precedent applies.  Last, the **deletion
A/B**: drop the forced-void generator, keep the deletion only if
`rank` stays within noise *and* latency improves.  Expected verdict:
KEEP, per example A.

**Kill criterion.**  At confirm: `rank` under 2 SE, or `win` negative
by more than 2 SE, or the greedy self-play leg negative by more than
2 SE — revert the one-function diff and record the null.

#### What P1 settled

Every arm below is one new-policy seat against three seats of the old
one, paired on identical deals inside a single build — a knobless policy
change moves the arm *and* the field it is scored against, so the
"cross-build paired game blocks" this section originally called for
cannot work: a homogeneous greedy field prints `0.000±0.000` in both
builds by construction.  A temporary `legacy_pass` config value carried
the old policy alongside the new one and was deleted on the last commit;
it reproduced the pre-change engine's arena CSV byte for byte, including
an `mc:128` seat, which is what makes the pairing trustworthy.

| arm, vs the old policy | pooled `rank`, 3 × 8,000 blocks |
| --- | --- |
| P1 + the new tie-break (shipped) | `+0.0100 ± 0.0014` (6.9 SE) |
| P1 alone, old tie-break | `+0.0112 ± 0.0013` (8.8 SE) |
| the tie-break alone | `+0.0013 ± 0.0013` (1.0 SE) |
| the ♦ > ♣ > ♥ rival ladder | `−0.0020 ± 0.0009` (−2.3 SE) |

**The sequential selection is the whole effect.**  `win` moves
`+0.0042 ± 0.0006` and games mode reads `rank +0.0272 ± 0.0073`.  The
Monte Carlo leg is a null — `rank +0.0022 ± 0.0031` over 6,000 blocks on
each of two seeds, no column negative — which is the expected shape: the
search rolls out its candidates anyway, so a better incumbent buys it
little, and the win belongs to the greedy bot, the rollout policy and
the browser's easier tiers.

**The tie-break is a measured null, and is kept for being explicit.**
This doc predicted the replacement rule "should be chosen on purpose";
it now is (♠A/♠K/♠Q, then ♥, ♦, ♣, then the queen's guards), and it is
worth about nothing — `+1.0 SE` on `rank`, `+1.5 SE` on `win`, against
`−3.7 SE` on `not-last`, i.e. it trades middle placements for outright
wins.  What the measurement *did* settle is the direction: the rival
ladder that ranks the minors above hearts is refuted at `−2.3 SE`, so
whatever else is true, hearts belong at the top of the order.  The
0.145-card club bias is gone; the instrument now reads ♥ 0.8391,
♠ 0.7488, ♦ 0.7385, ♣ 0.6735, a spread of the same size pointing the
other way and on purpose this time.

**Both obligations discharged.**  The mandatory `void_weight ×
spade_guards` re-sweep — owed because the knob's unit changed from a
flat multiplier to a marginal schedule — kept both defaults:
`void_weight = 1` still beats 0 (−3.05 pp), 2 (−2.50) and 4 (−5.15), and
`spade_guards` 3, 4 and 6 all sit inside noise of 5.  The **deletion
A/B** came back the way this doc predicted, and harder: dropping the
forced-void generator costs `rank −0.0063 ± 0.0017` (−3.7 SE),
`win −4.8 SE`, `moons −7.6 SE`, and returns no throughput.  Worked
example A is now measured rather than argued — the two mechanisms are
complements.  **KEEP.**

**Latency, with a wrinkle worth keeping.**  The isolated
`monte carlo pass, 64 samples` bench rose **44%**, not the predicted
neutral-to-faster.  The policy itself did get cheaper (three alloc-free
argmax scans replace a `Vec` and a full sort); what grew is the *search*
around it, because a better incumbent leaves more pass decisions
statistically unresolved and the adaptive width reaches its 3× cap more
often.  Full-round throughput is unchanged (254 vs 249 rounds/s across
three seeds) — the pass is one decision in fourteen.  The lesson for the
next pass proposal: a pass-bench delta is not a latency result, because
the bench measures the one decision whose cost the significance gate is
allowed to triple.

### P6 — per-suit and per-card hazard offsets

**Status: HALF SHIPPED (2026-08-08).**  Opened after P1, from the
observation that the rules break the ♣/♦/♥ symmetry the scorer treats as
exact.  Two independent claims, measured separately; they split.

**Refuted — diamonds are not the dearer minor.**  The first trick is led
in clubs and cannot score, so one round of clubs is free and a club
should be the safest side-suit card to keep.  Built as a flat
`diamond_bonus` (flat, not per rank like `heart_weight`: the safe round
belongs to the suit, not to the card in it), swept, and wrong.  `tune`
put 1 at `+0.20 ± 0.63` pp with 2 and 3 under water; the arena refuted
even that best arm at `rank −0.0075 ± 0.0013` (−5.9 SE) over two seeds.
The tie-break ladder above refutes the same ordering by a mechanism an
order of magnitude smaller, so the two agree.  The knob is reverted in
full.  The honest reading is that the free club round is real but is
already spent by everyone at once, and what survives trick 1 is a suit
four cards shorter that others void out of sooner — which is a reason to
*shed* clubs, not keep them, and is what the old accidental order was
doing.

**Shipped — the 2♣ is worth its forced lead.**  It is the one card whose
play is never a choice, so it costs nothing to give away, and it scored
bare rank 2.  `two_of_clubs_bonus` defaults to **6**: value searched on
one seed over {1, 2, 3, 4, 6, 8}, confirmed on three fresh ones at
`rank +0.0019 ± 0.0004` (4.5 SE), `win +0.0007 ± 0.0002` (4.0 SE), with
`not-last` positive on every seed and games mode at
`rank +0.0061 ± 0.0022`.  The Monte Carlo leg is a null except `moons`,
up 2.5 and 3.3 SE — shedding the forced lead slightly helps a shot.

**Why this was not already absorbed.**  The absorption principle says
`tune` has already priced any hand-independent prior into the shipped
defaults.  It only holds where some existing term *can express* the
prior: at `heart_weight = 0` nothing distinguished ♣ from ♦ from ♥, and
no term had ever distinguished a single card.  Both were new content in
P3's sense; one of them was also wrong.

**Shelf variant, unbuilt.**  A singleton 2♣ collects the void bonus and
this one for the same card leaving.  The overlap is partial — passing it
makes us club-void *at* trick 1, which buys a free slough the void term
does not price — so no gate was built.  Revisit only if a joint arm ever
underperforms the parts.

### P2 — complete the generator: spade voids and double voids

**Mechanism.**  Two targeted additions to `pass_candidates`:

- *(a) Spade voids.*  Delete the `suit != Suit::Spades` filter
  (`src/mc.rs:491`) so a one-to-three-card spade holding also proposes
  its emptying triple, filled as usual.  No honor gate: candidates are
  hypotheses and the rollouts judge them — the shipped patch's own
  design rule.
- *(b) Double voids.*  For every pair of non-empty suits with
  `len(a) + len(b) ≤ 3`, the union padded with top outsiders, deduped.

**Why.**  A spade void immunizes the hand against the queen hunt — the
greedy policy itself smokes her out with low-spade leads
(`src/heuristic.rs:100-110`), and a seat that cannot follow spades
cannot be caught; every later spade lead becomes a free slough.  The
jewel is Q♠xx: pass all three and the queen leaves *and* the suit
closes — a triple the tiered pool almost never contains because her
low escorts score bare rank.  Double voids are the strongest defensive
shape (two slough channels) and the classic moon-support shape at once
— the dual-use thesis in a single candidate.

**Coupling.**  None — pool only.

**Latency.**  Worst case +3 candidates against the width-7 precedent
of +15 always-on triples for +34%, and elimination drops hopeless
triples after the first 32-world batch.  Budget +10% on the pass
bench.

**Measurement.**  One joint A/B (both arms clone the shipped
mechanism): 2,000-block screen, then a 6,195-block confirm at `mc:128`
— the forced patch's own scale.  `rank` primary, `win` gate, `moons`
watched: both additions should nudge attempts up.

**Kill criterion.**  Joint `rank` under 2 SE at confirm reverts both;
if it ships but latency exceeds +10%, ablate the pairs arm to
attribute.

### P3 — shape-aware shoot passes

**The open question in the current rule.**  The moon candidate passes
the bottom three of `pass_score`; the comment calls them "low cards of
long suits, which is exactly the ballast a shot sheds"
(`src/mc.rs:470-475`).  Because the void term pushes short-suit low
cards *up* the score, they escape the bottom — the rule systematically
keeps short-suit junk in hand and sheds long-suit low cards.  For a
long strong suit that reads backwards: after three rounds of a
six-card suit headed A K Q, the fives and twos *are* masters — cards a
shooter wants — while a kept ♦74 doubleton is two tricks the shot must
survive without being able to win them.  Both readings are hypotheses;
the machinery below prices them instead of arguing.

**Mechanism.**

- *(d1)* Build the moon pass as the three lowest-rank cards *outside
  the longest suit*, shortest suits first: shed the unwinnable, build
  voids for the shot's sloughs, never break the running suit.  Prefer
  replacement — candidate count unchanged — and fall back to addition
  if replacement regresses.
- *(d2)* Re-offer the shortest-suit forced-void triple with
  `shoot: true`.  The dual-use bet then runs both ways on the same
  set: as `shoot: false` it clears `beats` alone; as `shoot: true` it
  must also reach the moon in a strict majority of worlds
  (`recommended`, `src/mc.rs:726-735`).  The bimodal-tails lesson
  stands — no shoot candidate ever rides the paired gate alone — and
  no new statistics are needed.

**Coupling.**  Pool only.

**Measurement.**  `moons` is the sensitive detector — the eight-point
trigger resolved a half-point-of-percentage attempt shift at 8.6 SE
over 6,000 blocks.  Screen 2,000 blocks on `moons` and `rank`, confirm
at 6,000.  A small `not-last` cost is acceptable while `win` holds,
the trade already shipped with the trigger.

**Kill criterion.**  The new candidates are never chosen (dead code),
or `rank`/`win` negative at confirm — revert.

### P4 — hand-level keep score: argued YAGNI

The expressive alternative: score the kept ten cards — voids opened,
stoppers held, longest-suit strength — over all C(13,3) = 286 triples.
Three reasons not to build it:

1. P1 *is* the same set objective optimized coordinate-wise, at zero
   new knobs; build the free version first.
2. At the Monte Carlo level the rollouts already price keep-shape
   exactly; the only unique customer is the standalone greedy bot, and
   three-plus new weights break `tune`'s Cartesian discipline.
3. The policy runs inside world sampling — three passes per world,
   every world, on the engine's most expensive decision — and a
   286-triple scorer multiplies that inner loop by two orders of
   magnitude.  Model consistency forbids making it heuristic-only:
   the rollout opponents must pass the way we pass, or the worlds and
   the observation model drift from the policy they claim to model.

Revive only on P1's grave, as a full replacement, its weights facing
`tune` two-stage like everything else.

### P5 — four-card suits: note and skip

You pass three cards; a four-card suit cannot become a void.  The
generator's `len > 3` skip is correct, and under P1 the 4→3→2
shortening prices itself at the margin — each removal raises the
survivors' term.  A dedicated shortening bonus would reward passing
the low three while keeping the suit's *high* card as the residue, the
worst possible duck.  Revisit only if post-P1 transcripts show
systematic four-suit misses.

## Sequencing and the deletion ledger

P1 → P1's re-sweep and the forced-generator deletion A/B → P2 → P3 →
P4 only on P1's grave; P5 never.  P1 goes first because it re-baselines
the incumbent every later A/B must beat — measuring P2 or P3 against
the scatter incumbent double-measures them.  **All of the first arrow
is done**: P1 shipped, the re-sweep kept both defaults, the deletion A/B
said KEEP, and P6 ran in its own window afterwards.  P2 is next and is
now measured against an honest incumbent.  Never share an arena
window with the sibling doc's campaigns; correlated-deal noise already
made one `void_weight = 2` stage-one float look real.

P1 landed, and the ledger settled: the `Vec`-and-sort in `greedy_pass`
is gone by construction; the forced-void generator **stays** (the A/B
refuted its deletion at −3.7 SE); the CLAUDE.md map line and the
candidate-0 invariant wording were both rewritten, the latter because
the invariant is now enforced rather than inherited; and `void_weight`
kept its value through the re-sweep the unit change owed.  Two things
the ledger did not anticipate were also deleted: the temporary
`legacy_pass` measurement scaffold, and `diamond_bonus`, which P6 built
and refuted.

## Interactions with the opponent-model doc

- Its P1 publishes the refill curve; the discount composes
  multiplicatively with this doc's void term, on the marginal steps
  (+4, +6) and never the first-card bonus — a void bet is worth less
  when the exchange refills the suit, and only policy and candidate
  priorities consume the discount (rollouts already price refill).
- Its P0 config plumbing is what lets this doc's re-sweep face the
  arena as `mc:` specs rather than only `greedy:` specs.
- This doc owns the deterministic tie-break rule (P1); the sibling's
  observation model inherits whatever is pinned here.

### Measured, 2026-08-08 — two results this doc must absorb

The sibling's P0 shipped (`MonteCarloBot::pass_model`, so `mc:` specs now
carry pass knobs) and its P1 ran.  Two of its numbers land directly on
proposals here.

**The refill discount is a constant, not a curve.**  Measured
`P(≥ 1 received card of suit s | we hold k of s)` over 10⁶ deals is
*flat* in `k` for the side suits — clubs 0.6948 → 0.6831 across
k = 0..3, diamonds 0.640 → 0.637, hearts 0.588 → 0.587 — where the
uniform null falls 0.7155 → 0.6002.  The reason is structural and worth
stating plainly: **the giver cannot see our hand**, so no policy it runs
can react to our shape, and the only `k`-dependence available is card
counting.  So a void bet is discounted by a roughly constant factor
(~0.59 in hearts, ~0.68 in clubs), and P1's escalating +4/+6 completion
schedule keeps its *shape* — the discount rescales it rather than
bending it.  Spades are the exception and do track the null (0.6984 →
0.6129), because the honor tier makes the giver's spade pass depend on
its own spade length; P2's spade-void candidates should expect a
genuinely lower refill risk as they empty the suit.

**The tie-break is a live term, not a formality — and it is currently a
bug-shaped accident.**  P1 above already requires the tie-break rule to
be "explicit and pinned by a test".  The measurement upgrades that from
hygiene to a defect worth fixing: at the shipped `heart_weight = 0`,
`pass_score` is *exactly* symmetric under permuting ♣/♦/♥ — nothing in
the base rank, the void bonus or the spade tier distinguishes side suits
— yet the measured mean cards passed per suit are ♣ 0.8188, ♦ 0.7447,
♥ 0.6741.  That entire 0.145-card spread is `sort_by_key`'s stability
resolving ties in `Suit::ASC` order, clubs first.  The policy has a club
bias nobody designed and nobody has measured the cost of.

That makes P1's rewrite the natural place to fix it, and gives the
rewrite a second, independent reason to exist beyond set-awareness.  The
sequential form re-sorts three times, so it will *redistribute* this bias
rather than remove it; the replacement rule should therefore be chosen on
purpose.  The obvious candidates — break ties toward the shorter suit
(which serves the void thesis directly) or toward the suit with fewer
kept honors — are cheap, and either is defensible where "clubs first" is
not.  A/B them against the current order as part of P1's own arena leg,
not as a follow-up.

**Amended after P1 shipped.**  Two clauses above are now false and are
kept only because the reasoning that follows from them is not.  The
symmetry claim is void: `two_of_clubs_bonus = 6` distinguishes one club
from every other card, so `pass_score` is no longer symmetric under
permuting ♣/♦/♥, and the per-suit table those numbers came from has been
re-measured (♥ 0.8391, ♠ 0.7488, ♦ 0.7385, ♣ 0.6735).  And "a defect
worth fixing" oversold it: the tie-break was A/B'd on P1's own arena leg
exactly as this paragraph asked, and replacing the club bias with a
designed order is worth `+1.0 SE` of `rank`.  The bias was real, it was
undesigned, and it was **nearly free** — a useful calibration for the
next time a measurement turns up a large-looking artifact in a term that
only fires on exact ties.  Neither of the two candidates this paragraph
proposed was the rule chosen; the danger ladder came from the rules
instead, and the one rival that was measured (♦ > ♣ > ♥) lost.

## Appendix — measurement boilerplate

The house discipline, from the README and CHANGELOG practice:

- Search on one seed, confirm the single best arm on a fresh seed,
  then face the arena:

```console
cargo run --release --example tune -- --games 2000 --seed 1 \
  --void-weight 0,1,2,4 --spade-guards 3,4,5,6
cargo run --release --example arena -- --games 2000 --ab \
  greedy:2,0,5 greedy greedy greedy greedy
cargo run --release --example arena -- --blocks 500 --csv \
  mc:128 greedy greedy greedy > candidate.csv
```

- `rank` is the primary detector, `points` the interpretable
  magnitude, `win` the ship gate; a homogeneous greedy field must
  print `0.000±0.000` exactly, and `tune`'s default arm must print
  `+0.00 ± 0.00` exactly.
- Latency on the Criterion bench (`benches/decision.rs`), watching the
  `mc:64` pass entry, baseline 5.87 ms.
- The strength tripwire stays manual:
  `cargo test --release --test strength -- --ignored`.
