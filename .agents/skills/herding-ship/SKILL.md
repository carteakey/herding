---
name: herding-ship
description: Use when shipping Herding: merging ready branches to main, gating at the exact commit, tagging a calendar release, building the release binary, deploying the local and fleet hosts, and cleaning up branches and worktrees. Also use when a release, deploy, merge, or cleanup is requested, or when a gate fails mid-ship. The skill is authoritative; scripts/ship.sh is only a helper and every step it runs must be verified.
---

# Herding Ship

Shipping is a judgment loop, not a script. `scripts/ship.sh` (and `just ship`)
can run the mechanical steps, but **verify each step's output**; the script
does not decide whether a red gate is a real failure, whether a conflict was
resolved correctly, or whether a host actually came up.

## 1. Preflight (never skip)

- This checkout is `~/repos/forks/herding`; the GitHub repo is
  `carteakey/herding`. `origin` is the fork, `upstream` is
  `penso/herdr-gpui` and is read-only. File issues and PRs in the fork only.
- `git status` must be clean and the branch must be `main`. A dirty tree is a
  stop: commit or stash it, never force.
- `git fetch origin --prune`.
- Inventory: `git branch -vv`, `git branch --merged main`,
  `git branch --no-merged main`, `git worktree list`, `git stash list`.
- Identify the **v0 checkpoint** (`v0-herding`) and any unmerged WIP. Those are
  never deleted.
- Check the box: `free -h`, `swapon --show`. If swap is nearly full, serialize
  builds (`CARGO_BUILD_JOBS=1`) and pause idle workers before starting.
- Kill local inference before gating: `pgrep -af 'strata|llama-swap'`. Those
  servers hold 40+ GB RSS and restart on demand; ask before killing, then
  build. `/tmp` here is a 31 GB tmpfs, so a `target/` under it eats RAM.
- The fork's CI is inert: upstream's workflows gate every code job on
  `penso/herdr-gpui` / `sheprd-labs/sheprd`, so nothing runs here. The local
  gates below are the only evidence a change has; record their counts.

## 2. Gate each ready branch at the exact commit

For every branch to land:

- Rebase it onto current `origin/main` (or fast-forward if it already is).
- Read the diff (`git diff origin/main...<branch>`), not the report. Check the
  mechanism, the tests, and the docs.
- Run the gates **on the rebased tree**:
  - `cargo fmt --check`
  - `cargo clippy -p sheprd-gpui -p sheprd-orchestrator --all-targets`
  - `cargo test -p sheprd-orchestrator`
  - `cargo test -p sheprd-gpui team` (and the affected module for a scoped fix)
  - `python3 scripts/check-i18n.py`, `python3 scripts/check-sorted.py`, and
    `bash scripts/check-file-size.sh` when the change touches user strings,
    sorted lists, or file sizes. `check-file-size` currently fails on three
    Team files that predate the limit; do not let it hide a new offender.
- A red gate is a stop. Fix forward with evidence, or send it back. Never merge
  on "all green" without counts, and never skip a gate because the box is slow.
- A rebase can expose a real bug (a fixture that no longer matches, a call site
  with a new argument). Fix the bug, add the evidence to the commit, and re-run.

## 3. Merge and tag

- Fast-forward only: `git merge --ff-only <branch>`; never force-push `main`.
- Push `main`, then derive the version:
  `git tag -l | python3 scripts/release/next-version.py $(date -u +%Y%m%d)`.
- `git tag -a v<version> -m v<version>` and `git push origin v<version>`.
- The tag is a **source** release: packaging still builds and expects
  `sheprd` (see the follow-up issue), so no installer or updater artifact
  comes out of it yet. Say so in the release notes rather than implying one.
- Update the linked issues with the SHA and gate counts; close only shipped
  issues, leave partials open with what remains.

## 4. Build and verify the binary

- `cargo build --release --bin herding`.
- `./target/release/herding --version` must print the new tag and short SHA.
- `--build-info` must not be a worktree/PR build for a release.

## 5. Deploy

**Local:** `herding app quit`, then relaunch with the session's display
environment (`DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY`, `XDG_RUNTIME_DIR`,
`DBUS_SESSION_BUS_ADDRESS`). Verify with `herding status --json`.

**Fleet:** per host, fetch the tag, check it out, build with
`PATH="$HOME/.cargo/bin:$PATH"`, verify `--version`, then restart the UI. Hosts
differ in repo path and auth; use the key or the password the user gives in the
session, and never write the password to a file or commit it. A host that does
not resolve or refuses the key is a **report**, not a reason to improvise
credentials. Prefer the LAN address when the tailnet port is filtered.

Do not cap the app's systemd **unit** to guard memory: the unit aggregates the
daemon, terminals, and agents, so a cap kills the fleet. Cap the app process
alone, or rely on the sampler's soft guard.

## 6. Clean up

- Delete **merged** local branches (`git branch -d`), merged remote branches
  (`git push origin --delete`), and worktrees whose HEAD is an ancestor of
  `main` (`git worktree remove`).
- **Keep** `main`, the `v0-herding` checkpoint, unmerged WIP branches, and open
  PR branches.
- Before removing a worktree with uncommitted work, commit it to its branch
  (a `wip(...)` commit is fine) so nothing is lost.
- Do not delete a branch you have not proven is merged.

## 7. Verify and report

- `herding team status` and `team task list` show the expected board.
- `scripts/fleet-doctor.sh` (the `herding-fleet` skill) shows every host on the
  fork build with the plugin installed and the team skill present. Quote its
  table; a host that is `unreachable` or on `upstream` is not shipped.
- The release tag, every host's `--version`, and the gate counts are in the
  report. State what was not run.
- Watch memory for a few minutes after the deploy; if the session is under
  pressure, say so and name the holder.

## Judgment calls the script cannot make

| Situation | Do |
| --- | --- |
| Dirty tree | Stop; commit or stash. Never force. |
| Red gate | Fix forward with evidence, or send it back. Never merge. |
| Rebase conflict | Adapt to the current tree, run the affected tests, record the SHA. |
| Fixture/test bug exposed | Fix it; the test was wrong, not the product. |
| Host unreachable | Report the exact error; do not improvise access. |
| Swap nearly full | Serialize builds, pause idle workers, then ship. |
| Unmerged WIP | Keep the branch; commit the worktree before removing it. |
| "All green" without counts | Treat as unverified. |

## Anti-patterns

- Force-pushing `main`, or merging a WIP branch.
- Deleting a branch/worktree without proving it is merged.
- Trusting a completion report over the diff.
- Capping the app's unit (kills the fleet) or capping the session scope without
  understanding the aggregate.
- Deploying without checking `--version` on the host.
- Reporting a gate as green when it was skipped or OOM-killed.
