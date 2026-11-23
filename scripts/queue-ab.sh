#!/usr/bin/env bash
#
# queue-ab.sh — queue an arena run (usually an A/B) behind the others on this box.
#
# An arena run saturates the machine, so they go one at a time through pueue
# (`cargo install --locked pueue`; `pueued` runs as a systemd user unit; its
# default group is one task wide).  This script freezes the checkout as it is
# right now, uncommitted and untracked files included, into a detached
# worktree, and queues the arena example there.  You keep editing; the queued
# run builds and measures the frozen copy, and its `cargo build` waits its
# turn too, so nothing is rebuilt under a running A/B.
#
# Usage:
#   scripts/queue-ab.sh SLUG ARENA-ARGS...
#
# Example:
#   scripts/queue-ab.sh win-only --games 2000 --seed 21 \
#       --ab mc:1024,win=true mc:1024 mc:128 mc:128 mc:128
#
#   worktree  ${AB_WORKTREES:-~/ab}/<repo>-SLUG   (its own target/)
#   result    ab-results/SLUG.txt in the main checkout (appended), headed by
#             the snapshot commit and the arguments
#   SHA       a clean snapshot commit whose parent is HEAD, not `HEAD-dirty`
#
#   pueue status              the queue (add --json for scripts)
#   pueue kill ID / remove ID stop a running task / drop a queued one
#   git worktree remove --force ~/ab/<repo>-SLUG     once the verdict is read
#
# Adapted from pons' scripts/queue-ab.sh.
#
set -euo pipefail

if [[ $# -lt 2 || $1 == -h || $1 == --help ]]; then
	sed -n '2,/^set -euo/p' "$0" | sed 's/^# \{0,1\}//;/^set -euo/d'
	exit 0
fi
slug=$1
shift

cd "$(git rev-parse --show-toplevel)"
main=$(git worktree list --porcelain | sed -n '1s/^worktree //p')
wt=${AB_WORKTREES:-$HOME/ab}/${main##*/}-$slug

# Snapshot the working tree through a scratch index: the real index, the
# branch and the files are untouched.
rev=$(git rev-parse HEAD)
if [[ -n $(git status --porcelain) ]]; then
	idx=$(mktemp)
	trap 'rm -f "$idx"' EXIT
	cp "$(git rev-parse --git-path index)" "$idx"
	GIT_INDEX_FILE=$idx git add -A
	tree=$(GIT_INDEX_FILE=$idx git write-tree)
	rev=$(git commit-tree --no-gpg-sign "$tree" -p "$rev" -m "queue-ab: $slug")
fi

# Refuses an existing worktree: remove it first.
git worktree add --detach "$wt" "$rev" >&2
mkdir -p "$main/ab-results"

printf -v args '%q ' "$@"
printf -v out '%q' "$main/ab-results/$slug.txt"
printf -v head '%q' "# $rev: arena $*"
pueue add --label "$slug" --working-directory "$wt" -- \
	"{ echo $head; cargo run --release --all-features --example arena -- $args; } >>$out 2>&1"
