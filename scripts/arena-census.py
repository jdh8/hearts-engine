#!/usr/bin/env python3
"""arena-census.py — count the moon shots our bot starts in its Hearts Bot Arena games.

The site records completed moons only.  Our own attempts are invisible there,
so this replays each game's public transcript through the arena wasm (which is
deterministic in its `INIT` seed and the fuel figures on each prompt) and reads
the `shoot` lines it prints on stderr.  A replay whose replies differ from the
transcript is counted as diverged and left out, so the census cannot quietly
describe a different bot from the one that played.

    scripts/arena-census.py fetch  DIR [--bot NAME] [--version N] [--limit N]
    scripts/arena-census.py replay DIR WASM [--jobs N]
    scripts/arena-census.py report DIR

`fetch` is light (one request per game, throttled); `replay` saturates the box
and belongs in pueue; `report` prints the table.  DIR holds everything and is
safe to delete: `games/ID.json` (transcript + result) and `census.ndjson`.
"""

import argparse
import concurrent.futures
import json
import os
import subprocess
import sys
import time
import urllib.request

SITE = "https://hearts-arena.snowfrog.dev/api"


def get(path):
    request = urllib.request.Request(SITE + path, headers={"User-Agent": "arena-census"})
    for attempt in range(5):
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                return json.load(response)
        except OSError as error:
            print(f"retry {path}: {error}", file=sys.stderr)
            time.sleep(10 * (attempt + 1))
    sys.exit(f"gave up on {path}")


def fetch(args):
    os.makedirs(f"{args.dir}/games", exist_ok=True)
    wanted, page = [], 1
    while len(wanted) < args.limit:
        games = get(f"/bots/{args.bot}?page={page}")["games"]
        wanted += [g for g in games["items"] if g["version"] == args.version]
        if page >= games["pages"]:
            break
        page += 1
        time.sleep(0.3)
    for row in wanted[: args.limit]:
        path = f"{args.dir}/games/{row['gameId']}.json"
        if os.path.exists(path):
            continue
        game = get(f"/games/{row['gameId']}")
        transcript = get(f"/games/{row['gameId']}/seats/{row['seat']}/transcript")
        with open(path, "w") as out:
            json.dump({"seat": row["seat"], "game": game, "transcript": transcript}, out)
        time.sleep(0.3)
    print(len(os.listdir(f"{args.dir}/games")), "games on disk")


def replay_one(job):
    path, wasm = job
    with open(path) as file:
        record = json.load(file)
    seat, hands = record["seat"], record["game"]["hands"]
    lines = [entry for hand in record["transcript"]["hands"] for entry in hand]
    stdin = "".join(e["line"] + "\n" for e in lines if not e["reply"])
    run = subprocess.run(["wasmtime", "run", wasm], input=stdin, capture_output=True, text=True)
    replies = [e["line"] for e in lines if e["reply"]]
    base = {"gameId": record["game"]["gameId"], "won": record["game"]["winner"] == seat}
    if run.stdout.split("\n")[:-1] != replies:
        return {**base, "diverged": True}

    # One stderr line per prompt ("Pass: ..." / "Play: ..."), then "shoot" if
    # that decision began a shot; prompts and replies are in the same order.
    shot = []
    for line in run.stderr.splitlines():
        if line.startswith(("Pass:", "Play:")):
            shot.append(False)
        elif line == "shoot":
            shot[-1] = True
    assert len(shot) == len(replies), (path, len(shot), len(replies))

    rows, decision = [], 0
    for index, hand in enumerate(record["transcript"]["hands"]):
        row = {"pass": None, "shoot_pass": False, "shoot_trick": None}
        trick = 0
        for entry in hand:
            words = entry["line"].split()
            if words[0] == "HAND":
                row["pass"] = words[2]
            elif words[0] == "TRICK":
                trick = int(words[1])
            elif entry["reply"]:
                if shot[decision]:
                    if words[0] == "PASS":
                        row["shoot_pass"] = True
                    elif row["shoot_trick"] is None:
                        row["shoot_trick"] = trick
                decision += 1
        if index < len(hands):
            row["points"] = hands[index]["points"][seat]
            shooter = hands[index]["moonShooter"]
            row["moon"] = "ours" if shooter == seat else "theirs" if shooter is not None else None
            rows.append(row)
    return {**base, "diverged": False, "hands": rows}


def replay(args):
    jobs = sorted((f"{args.dir}/games/{name}", args.wasm) for name in os.listdir(f"{args.dir}/games"))
    with open(f"{args.dir}/census.ndjson", "w") as out:
        with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
            for done, result in enumerate(pool.map(replay_one, jobs), 1):
                out.write(json.dumps(result) + "\n")
                out.flush()
                if done % 50 == 0:
                    print(done, "of", len(jobs), flush=True)
    report(args)


def report(args):
    with open(f"{args.dir}/census.ndjson") as file:
        games = [json.loads(line) for line in file]
    kept = [g for g in games if not g["diverged"]]
    hands = [h for g in kept for h in g["hands"]]
    tried = [h for h in hands if h["shoot_trick"] is not None]
    landed = [h for h in tried if h["moon"] == "ours"]
    failed = [h for h in tried if h["moon"] != "ours"]
    passes = [h for h in hands if h["shoot_pass"]]
    unplanned = [h for h in hands if h["moon"] == "ours" and h["shoot_trick"] is None]

    def pct(part, whole):
        return f"{len(part)} of {len(whole)} ({100 * len(part) / max(1, len(whole)):.1f}%)"

    print(f"games replayed {len(games)}, diverged {len(games) - len(kept)}; hands {len(hands)}")
    print("hands with a shot started in play:", pct(tried, hands))
    print("  completed:", pct(landed, tried))
    print("  failed, mean points taken:", f"{sum(h['points'] for h in failed) / max(1, len(failed)):.1f}")
    print("  failed and an opponent shot instead:", pct([h for h in failed if h["moon"] == "theirs"], failed))
    print("moons completed with no shot logged:", len(unplanned))
    print("shoot passes chosen:", pct(passes, hands))
    print("  followed by a shot in play:", pct([h for h in passes if h["shoot_trick"] is not None], passes))
    print("  ended in our moon:", pct([h for h in passes if h["moon"] == "ours"], passes))
    print("first shoot trick -> started / completed")
    for trick in range(1, 14):
        at = [h for h in tried if h["shoot_trick"] == trick]
        if at:
            print(f"  {trick:2}: {len(at):4} / {sum(h['moon'] == 'ours' for h in at):4}")
    for won in (True, False):
        side = [h for g in kept if g["won"] == won for h in g["hands"]]
        print(f"games {'won' if won else 'lost'}: shots per 100 hands",
              f"{100 * sum(h['shoot_trick'] is not None for h in side) / max(1, len(side)):.2f}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    command = commands.add_parser("fetch")
    command.add_argument("dir")
    command.add_argument("--bot", default="rusty-hearts")
    command.add_argument("--version", type=int, default=2)
    command.add_argument("--limit", type=int, default=1000)
    command.set_defaults(run=fetch)
    command = commands.add_parser("replay")
    command.add_argument("dir")
    command.add_argument("wasm")
    command.add_argument("--jobs", type=int, default=os.cpu_count())
    command.set_defaults(run=replay)
    command = commands.add_parser("report")
    command.add_argument("dir")
    command.set_defaults(run=report)
    args = parser.parse_args()
    args.run(args)


if __name__ == "__main__":
    main()
