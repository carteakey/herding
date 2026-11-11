# Herding — consolidated state, 2026-11-09

Reconciled from `herding-handoff-2026-11-07.md`, the live repo/worktrees, the
team store, and upstream.

**Main:** `151295daf` (pushed to `origin/main`).
**Last release:** `v20261107.1` on `8f647973`; fleet fx505, m1, m2, yeti-cachy
all still on it. `main` now has one unreleased commit.
**Team:** `6aa93cf4` ("herding"), supervisor `ds412` — but every member is
pinned to workspace `wA`, and the running app now has only `wB`. The team CLI
refuses callers until the team is re-registered (issue #35).

## Consolidated this session

- **Landed #12 aggregate memory** as `151295daf`
  (`feat(app): log the cgroup aggregate's top holders`).
  Rebased onto `04b46f3a`, fmt clean, `cargo test -p herdr-gpui memory`
  30 passed / 0 failed, clippy exit 0 (only pre-existing
  `cloned_ref_to_slice_refs` warnings in `prefix_id_tests.rs`).
- Removed the merged worktree `herding-w2-cgroup`; deleted local and remote
  `feat/memory-cgroup-holders`.
- Dropped the `boot-drift wip` stash — it was an earlier copy of the change
  already landed as `94fd2d80`.
- Kept `herding-wb-brand` / `feat/v2-brand`; it is blocked on the user's
  pixel-art mascot and its gates have not run.

## Repos / worktrees

| Path | Branch | State |
| --- | --- | --- |
| `repos/forks/herdr-gpui` | `main` @ `151295daf` | clean, synced with origin |
| `repos/forks/herding-wb-brand` | `feat/v2-brand` @ `a8f2a9d0` | clean, 12 behind main, blocked |
| `repos/forks/zcode-cli` | `main` | clean |
| `repos/{agent-skills,agentdex,cartebase,dotfiles,garmin-*,homelabbr,skillrex}` | — | unrelated; `dotfiles` and `homelabbr` have local edits, left alone |

Only one herding worktree remains. `v0-herding` (`86a7e0c4`) is the kept
checkpoint.

## Open herding branches

| Branch | Tip | Note |
| --- | --- | --- |
| `feat/v2-brand` | `a8f2a9d0` | WIP, blocked on mascot; rebase before resuming |
| `origin/fix/team-caller-lineage` | `f932245a` (Nov 4) | unmerged; decide land or drop |
| `origin/team/endpoint-workspace-zcode-bee1` | `1e0460d3` (Nov 4) | unmerged; decide |
| `origin/team/trust-doc-zcode-bee2` | `8e86f26a` (Nov 4) | unmerged; decide |
| `origin/docs/ship-loop` | `42895a3c` | PR #19 open |
| `origin/playbook-backlog` | `88033b8b` | PR #18 open |
| `origin/v0-herding` | `86a7e0c4` | keep |

## Team board — needs reconciliation

| Task | Status | Reality |
| --- | --- | --- |
| `97edaf07` #12 aggregate memory | in_progress | **landed** as `151295daf`; mark done |
| `5b7751f6` #34 batch 1 | in_progress | batch 1 landed as `8f647973`; close it |
| `67b5887c` #6 completion wake | in_progress | path verified complete on main; needs dogfood closeout |
| `616e7ab4` #32 V2 slice 1 | open | not started |
| `ab146a5c` #33 branding | blocked | waiting on pixel-art asset |
| `11947e30` issue auto-sync | open | unassigned |
| `93470ea1` #8 multi-machine POC | open | unassigned |

## Upstream

- Merge base `b93958f5` (2026-10-31) → `upstream/main` `98916c39` (2026-11-09):
  **1170 commits** (411 fix/perf/security, 428 iOS/remote-control).
- That is **+740 commits since the Nov 7 handoff** (`6a8d044c` → `98916c39`).
- Batch 1 took 6 picks (`8f647973`). The 3 chat picks (`a747543e`,
  `4cdb2101`, `1336e7e4`) were deliberately deferred: they fix upstream's
  patch parser / preview budget, which is absent here; they belong to a later
  module-port batch.

## What's next (ordered)

1. **Re-establish the team.** Re-register the supervisor/workers in `wB` (or
   create a fresh team) and reconcile the board. This is the blocker for
   everything team-driven; it is the live case behind #35.
2. **Reconcile the board:** close #12 and #34 batch 1, re-scope #6.
3. **Upstream batch 2.** Re-triage against the new 1170 (the old 430-commit
   triage is stale): remote-control/remote, sidebar/keymap/automations/vcs/
   orchestrator; Windows parity optional; chat picks when the module is ported.
4. **V2 slice 1** (`616e7ab4`, #32): Team home as the default body route.
5. **#6 completion wake** (`67b5887c`): dogfood and close.
6. **Branding** (`ab146a5c`, #33) when the pixel-art asset lands: rebase
   `feat/v2-brand`, run gates, finish.
7. Then #16, #15, #5, #4, #7, #8.
8. Decide the old unmerged branches above and the two docs PRs (#18/#19).
9. Next ship: `main` has `151295daf` unreleased; tag/deploy when the batch is
   ready.

## Risks

- Disk 80% used (23G free; `target/` is 25G). Clean the build cache per
  `docs/build-cache.md` if builds fail on space.
- Memory 15G total, swap 23G (6.5G used). Serialize builds; never cap the
  app's systemd unit.
- Upstream moves ~370 commits/day; batch triage goes stale fast.
- macOS has no memory sampler yet (follow-up on #12).
