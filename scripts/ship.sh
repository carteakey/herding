#!/usr/bin/env bash
# Ship the current main: gate it, tag a calendar release, build the binary,
# swap the local UI, and clean up merged branches and worktrees.
#
# This is a HELPER, not the workflow. The authoritative process is the
# `herding-ship` skill (.agents/skills/herding-ship/SKILL.md): it covers the
# judgment calls this script cannot make (a red gate, a rebase conflict, an
# unreachable host, a dirty tree, unmerged WIP). Run the skill's steps and use
# this script only for the mechanical parts, verifying each step's output.
#
# Usage: scripts/ship.sh [--version VERSION] [--no-deploy] [--fleet]
#
# The fleet step is opt-in and expects the host list in HERDR_FLEET_HOSTS
# (space-separated ssh targets). Each host is expected to have the repo at
# HERDR_FLEET_REPO (default: ~/repos/forks/herdr-gpui) and a working cargo.
set -euo pipefail

cd "$(dirname "$0")/.."

version=""
deploy=1
fleet=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) version="$2"; shift 2 ;;
        --no-deploy) deploy=0; shift ;;
        --fleet) fleet=1; shift ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

say() { printf '\n== %s\n' "$*"; }

say "preflight"
[[ "$(git rev-parse --abbrev-ref HEAD)" == "main" ]] || {
    echo "ship must run on main" >&2
    exit 1
}
[[ -z "$(git status --porcelain)" ]] || {
    echo "working tree is not clean; commit or stash first" >&2
    exit 1
}
git fetch origin --prune

say "gates"
cargo fmt --check
cargo clippy -p herdr-gpui -p herdr-orchestrator --all-targets
cargo test -p herdr-orchestrator
cargo test -p herdr-gpui team

say "version"
if [[ -z "$version" ]]; then
    version="$(git tag -l | python3 scripts/release/next-version.py "$(date -u +%Y%m%d)")"
fi
tag="v$version"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    echo "tag $tag already exists" >&2
    exit 1
fi
echo "releasing $tag on $(git rev-parse --short HEAD)"

say "build"
cargo build --release --bin herdr-gpui
./target/release/herdr-gpui --version

say "tag and push"
git tag -a "$tag" -m "$tag"
git push origin main
git push origin "$tag"

if [[ "$deploy" == 1 ]]; then
    say "deploy local"
    ./target/release/herdr-gpui app quit || true
    sleep 2
    # Reuse the running session's display environment when present.
    env_args=(XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}")
    [[ -n "${DISPLAY:-}" ]] && env_args+=(DISPLAY="$DISPLAY")
    [[ -n "${WAYLAND_DISPLAY:-}" ]] && env_args+=(WAYLAND_DISPLAY="$WAYLAND_DISPLAY")
    [[ -n "${XAUTHORITY:-}" ]] && env_args+=(XAUTHORITY="$XAUTHORITY")
    [[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]] && env_args+=(DBUS_SESSION_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS")
    setsid env "${env_args[@]}" nohup ./target/release/herdr-gpui >>/tmp/herdr-gpui-ship.log 2>&1 </dev/null &
    sleep 6
    ./target/release/herdr-gpui status --json >/dev/null
fi

if [[ "$fleet" == 1 ]]; then
    say "deploy fleet"
    : "${HERDR_FLEET_HOSTS:?set HERDR_FLEET_HOSTS to the ssh targets}"
    repo="${HERDR_FLEET_REPO:-~/repos/forks/herdr-gpui}"
    for host in $HERDR_FLEET_HOSTS; do
        echo "-- $host"
        ssh "$host" "cd $repo && git fetch origin --tags && git checkout $tag && \
            export PATH=\$HOME/.cargo/bin:\$PATH && \
            cargo build --release --bin herdr-gpui && \
            ./target/release/herdr-gpui --version"
    done
fi

say "cleanup merged branches"
current="$(git rev-parse --abbrev-ref HEAD)"
git for-each-ref --format='%(refname:short)' refs/heads \
    | while read -r branch; do
        [[ "$branch" == "$current" ]] && continue
        if git merge-base --is-ancestor "$branch" main; then
            git branch -d "$branch"
        fi
    done

say "cleanup merged remote branches"
git for-each-ref --format='%(refname:short)' refs/remotes/origin \
    | sed 's#^origin/##' \
    | while read -r branch; do
        [[ "$branch" == "HEAD" || "$branch" == "main" ]] && continue
        if git merge-base --is-ancestor "origin/$branch" main 2>/dev/null; then
            git push origin --delete "$branch" || true
        fi
    done

say "cleanup merged worktrees"
git worktree list --porcelain \
    | awk '/^worktree /{print $2}' \
    | while read -r path; do
        [[ "$path" == "$PWD" ]] && continue
        if git -C "$path" merge-base --is-ancestor HEAD main 2>/dev/null; then
            git worktree remove "$path" || true
        fi
    done

say "done: $tag"
