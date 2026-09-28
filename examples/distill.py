"""P2 stage 1, the tree-ensemble rung: how much of mc:N's play a
LightGBM ranker recovers where `distill fit`'s linear scorer falls short.

    cargo run --release --example distill -- gen --blocks 10000 > s0.tsv
    python examples/distill.py s0.tsv [TREES LEAVES [TEST.tsv]]

Reads `distill gen`'s TSV.  Without TEST.tsv it holds out one block in
five, as `distill fit` does; with it, it trains on all of the first file
and tests on the second (a fresh seed).  Reports top-1 agreement with the
label's equivalence class against greedy's, the difference carrying a
block-clustered SE.  Offline only; needs numpy, lightgbm, scikit-learn.
Every feature is one a rollout seat may read: own hand, legal set, trick,
`played`, points taken.
"""

import sys
from collections import defaultdict

import lightgbm as lgb
import numpy as np

QS = 3 * 16 + 12
NAMES = (
    "sit tlen suit rank slen sround higher lower tpts wrank wins greedy qout "
    "first mypts maxother scorers cpts nhearts nspades hasK hasA hasQ "
    "lowerlegal higherlegal nlegal nhand solme"
).split()


def cards(bits):
    return [i for i in range(64) if bits >> i & 1]


def suit(bits, s):
    return bits >> (16 * s) & 0xFFFF


def points(c):
    return 13 if c == QS else int(c // 16 == 2)


def load(path, offset=0):
    """Per-card rows, labels, greedy flags, and per-decision groups/blocks."""
    rows, label, greedy, groups, blocks = [], [], [], [], []
    for line in open(path):
        block, hand, legal, played, trick, pts, lab, g, _twin = line.rstrip("\n").split("\t")
        hand, legal, played, lab = (int(x, 16) for x in (hand, legal, played, lab))
        g = int(g)
        t = [int(x) for x in trick.split(",") if x]
        p = [int(x) for x in pts.split(",")]
        led = t[0] // 16 if t else -1
        follow = bool(t) and suit(legal, led) != 0
        winner = max((c for c in t if c // 16 == led), default=0)
        seen = hand | played
        first = played & ~sum(1 << c for c in t) == 0
        scorers = sum(x > 0 for x in p)
        legal_cards = cards(legal)
        for c in legal_cards:
            s, r = divmod(c, 16)
            out = ~suit(seen, s) & 0x7FFC
            rows.append([
                0 if not t else 1 if follow else 2, len(t), s, r,
                suit(hand, s).bit_count(), suit(played, s).bit_count(),
                (out >> (r + 1)).bit_count(), (out & ((1 << r) - 1)).bit_count(),
                sum(map(points, t)), winner % 16, int(follow and r > winner % 16),
                int(c == g), int(not seen >> QS & 1), int(first), p[0], max(p[1:]),
                scorers, points(c), suit(hand, 2).bit_count(), suit(hand, 3).bit_count(),
                hand >> (QS + 1) & 1, hand >> (QS + 2) & 1, hand >> QS & 1,
                (suit(legal, s) & ((1 << r) - 1)).bit_count(),
                (suit(legal, s) >> (r + 1)).bit_count(), len(legal_cards),
                hand.bit_count(), int(scorers == 1 and p[0] > 0),
            ])
            label.append(lab >> c & 1)
            greedy.append(int(c == g))
        groups.append(len(legal_cards))
        blocks.append(int(block) + offset)
    return (np.array(rows, np.float32), np.array(label), np.array(greedy),
            np.array(groups), np.array(blocks))


def subset(data, keep):
    x, y, g, groups, blocks = data
    rows = np.repeat(keep, groups)
    return x[rows], y[rows], g[rows], groups[keep], blocks[keep]


def agreement(model, data):
    x, y, g, groups, blocks = data
    scores = model.predict(x)
    ends = np.cumsum(groups)
    ours = np.array([y[e - n + np.argmax(scores[e - n:e])] for n, e in zip(groups, ends)], float)
    base = np.array([y[e - n + np.argmax(g[e - n:e])] for n, e in zip(groups, ends)], float)
    diff = ours - base
    cluster = defaultdict(float)
    for v, b in zip(diff - diff.mean(), blocks):
        cluster[b] += v
    se = np.sqrt(sum(v * v for v in cluster.values())) / len(diff)
    return ours.mean(), base.mean(), diff.mean(), se


def main():
    trees, leaves = (int(a) for a in sys.argv[2:4]) if len(sys.argv) > 3 else (300, 63)
    data = load(sys.argv[1])
    if len(sys.argv) > 4:
        train, test = data, load(sys.argv[4], offset=1 << 32)
    else:
        held = data[4] % 5 == 0
        train, test = subset(data, ~held), subset(data, held)
    model = lgb.LGBMRanker(n_estimators=trees, learning_rate=0.1, num_leaves=leaves,
                           min_child_samples=50, verbose=-1)
    model.fit(train[0], train[1], group=train[3], feature_name=NAMES,
              categorical_feature=["sit", "suit"])
    for name, part in (("train", train), ("test ", test)):
        print("%s model %.4f greedy %.4f diff %+.4f±%.4f" % (name, *agreement(model, part)))


if __name__ == "__main__":
    main()
