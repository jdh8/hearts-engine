# Behavioural spec: loser-aware passing, a shape gate for shots, stopper-keeping defence

This is the hand-over document for a clean-room implementation.  It was
written by a reader of a GPL-3.0 Hearts program and describes *ideas
only*, restated in this crate's terms.  It contains no code, no
identifiers, no rule ordering and no constants from that program.

## Rules for the implementer

- Work from this document and this repository only.  Do not open, search
  for, or ask about any other Hearts program's source, including
  `brianberns/Hearts` and anything named Killer Hearts.
- Every number here marked *tunable* is yours to choose and then to
  measure.  Where the document gives no number, none is implied; derive
  one from the game (a short simulation is fine) and say how.
- If the document is ambiguous, pick the reading that is simplest in
  this codebase and record the choice.  Do not ask the spec's author to
  clarify from the original; ask the maintainer.
- Design the code yourself.  The section order below is not an
  evaluation order, and the behaviours are independent of each other.

## Why

Measured on the arena site (see [arena-gap.md](arena-gap.md)):

- our pass carries more A/K/Q than any other bot's and its recipient
  shoots more often than the other two opponents do;
- shots we start land about one time in five;
- the rollouts' defenders are much softer than the site's, so the search
  overrates shots.

Each behaviour below targets one of these.

## Vocabulary

All of these are computed from one seat's legal knowledge: its own cards
and the public record.

- **Outstanding** cards of a suit: those neither in my hand nor played.
- **Top run**: my cards of a suit that are each higher than every
  outstanding card of it.  At the pass this is the unbroken run down
  from the ace.
- **Expected rounds**: how many times a suit can be led before the
  opponents have none left, as a function of how many are outstanding.
  It rises slower than one round per three cards because holdings are
  uneven.  Derive it; do not guess a table.
- **Winners** of a suit: the top run, or the whole suit when the top run
  is at least as long as the expected rounds (the run draws every
  outstanding card, so the small cards win too).
- **Ducking cards** of a suit: low cards that will very probably lose
  the trick when the suit is led, judged from the bottom of the suit up
  against the outstanding cards below and above them.
- **Exposed suit**: a suit whose high cards have few or no ducking cards
  beneath them, so they must win tricks sooner or later.
- **Stopper**: a suit where my highest card will eventually win a trick
  against one player leading it from the top, because I hold more cards
  in it than there are outstanding cards above my highest, and the
  opponents still hold some.  Its **spare length** is how many cards I
  can discard from it and keep the stopper.
- **Sole holder**: the one seat that has taken every point played so
  far in the round, if there is one.

## B1. Loser-aware pass

Today's pass scores a card mostly by its own rank and suit.  Add the
suit's bottom end to the judgment.

- A high card is dangerous in proportion to how *exposed* its suit is.
  An ace above three small cards is nearly safe; the same ace above a
  king and a jack is not.
- Prefer passing high cards from exposed suits, most exposed first, and
  shorter suits before longer ones at equal exposure.
- Keep high cards in suits that have ducking cards under them.  The net
  effect wanted is fewer honours passed overall, not a different choice
  of which three honours to pass.
- Hearts: high hearts that sit over a long heart suit are guards, and
  are kept.  When a long heart suit must give something up, give the
  second-highest and keep the top one.  Short heart suits headed by a
  high card are exposed like any other suit.
- When choosing which minor suit to empty, remember that the first trick
  forces a club round: a club void is worth less than a diamond void of
  the same length.
- Spade handling (queen protection, short ace/king) already exists in
  `HeuristicConfig` and is out of scope.

The pass stays sequential (pick, remove, rescore), `pass_key` stays
injective over a hand, and Monte Carlo candidate 0 stays whatever
`greedy_pass` returns.

Example of the intent: from `♦A K 3 2` and `♣A K J`, the club honours go
before the diamond honours.

## B2. Shape gate for a pass-time shot

Decide whether to pass for a shot by looking at the ten cards that would
be *kept*, not the thirteen dealt.

- Build the pass a shooter would make: keep high cards, give the lowest
  cards of the suit that contributes least to a shot.  A heart that is
  not a winner is the first thing to go while hearts are short.
- Count winners in the kept hand.
- Pass for the shot only when the kept hand is winners nearly
  everywhere: at most a *tunable* small number of non-winners, and every
  kept heart a winner.
- Otherwise make the ordinary pass.

This is a property of the hand's shape, independent of rollouts, so it
does not inherit the soft-defender bias.  Expose it as a gate the search
can consult (a shot pass that fails it is not offered, or is offered but
may not replace the incumbent); choose whichever is simpler and state
which.

## B3. Stopper-keeping defence

When some seat may be shooting, a defender's cards have a second value:
whether they still stop the shot.

- Never discard a stopper suit down past its spare length, and never
  discard the last heart, while a shot is live.
- When following suit with length to spare, play the second-highest
  rather than the highest so the top card survives.
- When a point trick can be taken from the suspected shooter, take it,
  unless that means taking the Q♠.
- Once one stopper is certain (the shooter cannot get past it), the
  other dangerous cards are free: unload the Q♠ and high hearts on the
  shooter.
- When a different opponent is going to win the trick, put a heart on it
  to split the points.
- On lead, prefer a suit the shooter is void in, or a low card in a suit
  where some other opponent is forced to win.

This belongs in the knowledge-free core as well as the live overlay,
because the point is to harden the *rollouts'* defenders.  Put it behind
its own switch for the live seat and for shoot-world defenders
separately, alongside the existing trigger knobs
(`MonteCarloBot::moon_defense`, `rollout_defense`).

## B4. Earlier suspicion

Today a defence starts when the sole holder reaches a point count.  Let
it also start from how a seat *leads*, before points fall:

- a high heart led that no outstanding heart beats;
- the A♠ or K♠ led by a seat not known to hold the queen, early;
- a suit led in which every other seat is known void;
- the Q♠ won deliberately on the first spade round.

Suspicion ends the moment points are split between two seats.  Treat
each sign as its own *tunable* switch; they are cheap to add and may
well be noise.

## B5. Giving up a shot (optional, last)

A shot in progress should stop when it can no longer succeed:

- one opponent holds every outstanding card of a suit in which I still
  have a loser;
- an opponent down to hearts only holds a stopper in them;
- the Q♠ is on a trick I cannot win.

Between "shooting" and "not shooting" there is a useful third state:
play cards that keep the shot possible without committing to it, and
commit only late in the round or once most of the points are mine.

## Not in scope

- Any model of what the pass recipient will do with the cards.
- Game-score-dependent play (helping a shooter, targeting the leader).
- Changes to the equity function, the gate, or the sampler.

## How it will be judged

Each behaviour ships behind its own arena spec key, default off, and is
priced separately with `scripts/queue-ab.sh` under the A/B convention in
`CLAUDE.md` (`rank` detects, `points` sizes, `win` gates; arena-only
changes gate on `win`).  B3 is additionally read against the stage 1
targets in [arena-gap.md](arena-gap.md) P5: the share of our shots that
land and the field's moons per seat.  A behaviour that measures null is
deleted and the null recorded there.
