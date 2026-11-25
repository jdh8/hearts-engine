#!/usr/bin/env python3
"""arena-export.py — mirror the Hearts Bot Arena's public games, in full.

`GET /api/games/export` serves every stored game with its replay (all four
hands, the passes, every play), paged by cursor.  This keeps a local mirror
and flattens it into one line per hand for `examples/field.rs`, which
replays each hand through the real rules.

    scripts/arena-export.py fetch DIR     # resumable; one request a second
    scripts/arena-export.py hands DIR     # one line per hand, on stdout
    scripts/arena-export.py fuel  DIR     # what each bot burns per hand

DIR holds `pages/FIRSTID.json.gz` (full pages, never refetched), `tail.json.gz`
(the last, partial page, refetched every run) and `cursor`.  A hand's line is

    GAME HAND PASS  BOT:VERSION x4  SCORE-BEFORE x4  DEALT x4  PASSED x4
    PLAYS  POINTS x4  SHOOTER

space-separated fields, seats North East South West, hands as concatenated
card codes, PASSED and SHOOTER `-` when there is none, and PLAYS as 52
`<seat><card>` triples in the order played.  Forfeited games are left out.
"""

import argparse
import glob
import gzip
import json
import os
import sys
import time
import urllib.request

SITE = "https://hearts-arena.snowfrog.dev/api"


def get(path):
    request = urllib.request.Request(
        SITE + path, headers={"User-Agent": "arena-export", "Accept-Encoding": "gzip"}
    )
    for attempt in range(5):
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                body = response.read()
                if response.headers.get("Content-Encoding") == "gzip":
                    body = gzip.decompress(body)
                return json.loads(body)
        except OSError as error:
            print(f"retry {path}: {error}", file=sys.stderr)
            time.sleep(10 * (attempt + 1))
    sys.exit(f"gave up on {path}")


def write(path, value):
    with gzip.open(path + ".part", "wt") as out:
        json.dump(value, out)
    os.replace(path + ".part", path)


def fetch(args):
    os.makedirs(f"{args.dir}/pages", exist_ok=True)
    cursor = f"{args.dir}/cursor"
    after = open(cursor).read().strip() if os.path.exists(cursor) else ""
    while True:
        page = get("/games/export?limit=100" + (f"&after={after}" if after else ""))
        if page["next"] is None:
            # A partial page comes back again next time, so it is kept apart.
            write(f"{args.dir}/tail.json.gz", page["items"])
            break
        # Named by its first game, so a crash before the cursor moves only
        # rewrites the same file.
        write(f"{args.dir}/pages/{page['items'][0]['gameId']:08}.json.gz", page["items"])
        after = page["next"]
        with open(cursor + ".part", "w") as out:
            out.write(after)
        os.replace(cursor + ".part", cursor)
        print(f"through game {page['items'][-1]['gameId']}", file=sys.stderr, flush=True)
        time.sleep(1)
    print(len(os.listdir(f"{args.dir}/pages")), "full pages on disk", file=sys.stderr)


def games(directory):
    """Every mirrored game that was played out, oldest first, once each."""
    skipped = 0
    for path in sorted(glob.glob(f"{directory}/pages/*.json.gz")) + [f"{directory}/tail.json.gz"]:
        if not os.path.exists(path):
            continue
        with gzip.open(path, "rt") as file:
            page = json.load(file)
        for game in page:
            if game["forfeit"] is not None or game["replay"]["forfeit"] is not None:
                skipped += 1
            else:
                yield game
    print(f"{skipped} forfeited games left out", file=sys.stderr)


def hands(args):
    for game in games(args.dir):
        bots = [f"{seat['bot']}:{seat['version']}" for seat in game["seats"]]
        plays, index, scores = [], 0, [0, 0, 0, 0]
        for entry in game["replay"]["log"]["entries"]:
            if entry["ev"] == "play":
                plays.append(f"{entry['seat']}{entry['card']}")
            elif entry["ev"] == "hand_end":
                cards = [seat[index] for seat in game["replay"]["cards"]]
                hand = game["hands"][index]
                shooter = hand["moonShooter"]
                print(
                    game["gameId"],
                    index,
                    cards[0]["pass"] or "hold",
                    *bots,
                    *scores,
                    *("".join(c["dealt"]) for c in cards),
                    *("".join(c["passed"] or []) or "-" for c in cards),
                    "".join(plays),
                    *hand["points"],
                    "-" if shooter is None else shooter,
                )
                scores = [a + b for a, b in zip(scores, hand["points"])]
                plays, index = [], index + 1
        assert index == len(game["hands"]), game["gameId"]
        assert scores == [seat["finalScore"] for seat in game["seats"]], game["gameId"]


def fuel(args):
    """Fuel is metered per seat and public on every play, so the table shows
    who searches: a bare network burns the same small amount every hand."""
    used = {}  # bot -> [hands, total, worst hand, worst decision]
    for game in games(args.dir):
        pool = game["replay"]["log"]["perHandPool"]
        bots = [f"{seat['bot']}:{seat['version']}" for seat in game["seats"]]
        left = [pool] * 4
        for entry in game["replay"]["log"]["entries"]:
            if entry["ev"] == "play":
                row = used.setdefault(bots[entry["seat"]], [0, 0, 0, 0])
                row[3] = max(row[3], left[entry["seat"]] - entry["fuelLeft"])
                left[entry["seat"]] = entry["fuelLeft"]
            elif entry["ev"] == "hand_end":
                for bot, rest in zip(bots, left):
                    row = used[bot]
                    row[0] += 1
                    row[1] += pool - rest
                    row[2] = max(row[2], pool - rest)
                left = [pool] * 4
    print(f"{'bot':<24}{'hands':>8}{'mean/hand':>12}{'worst hand':>12}{'worst play':>12}   (share of the hand pool)")
    for bot, (count, total, hand, play) in sorted(used.items(), key=lambda item: -item[1][1] / item[1][0]):
        print(f"{bot:<24}{count:>8}{total / count / pool:>12.2%}{hand / pool:>12.2%}{play / pool:>12.2%}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("fetch", "hands", "fuel"):
        commands.add_parser(name).add_argument("dir")
    args = parser.parse_args()
    {"fetch": fetch, "hands": hands, "fuel": fuel}[args.command](args)


if __name__ == "__main__":
    main()
