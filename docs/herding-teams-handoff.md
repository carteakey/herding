# Herding Teams handoff, 2026-11-13

From the supervisor session (`claude`, pane `w22:pJ`) to whoever supervises
next. Team `c48cfdff-0422-4299-b03c-804ac19f94f3`. Updated at the end of the
session. For the why behind every decision here, read
`docs/herding-teams-decision-log.md`.

## Where things stand

- **v1 is released.** Herding v1.0.0 (branch `v1-herding`, tags `v1` and
  `v1.0.0`, main `6264db98e`) checkpoints Teams as it runs in the window,
  including #81. Release notes only, no binaries.
- **The pivot restarted on 2026-11-13.** Wave 1 is issue sync (PR 88) and
  layer D (PR 90); phases 1 and 2 are broken into tasks in the plan's
  "Phase 1 and 2 tasks" section. Spikes A-D are done (task `8d2c281f`).
- **Use v1 to build v2.** The v1 team coordinates the workers who will build
  the pivot, so v1 gets reliability fixes only. Do not extend code the pivot
  deletes (see "What the pivot replaces").
- **The running app** is on main (`2e808a02d` after PR 88's review fixes) and
  members are registered. The plan is `docs/herding-teams-on-core.md`. Where
  this note and the plan disagree, the plan wins.

## Restart and re-register

The app is running and every member re-registered under its own name. Keep
the procedure for the next restart:

1. Restart `target/release/herding`. Terminals and agents survive it.
2. `ss -xlp | grep gpui/control.sock` must show `herding`, not `herdr-gpui`.
   If it shows `herdr-gpui`, ask the user to quit that app (#85).
3. Supervisor first, from its own pane:
   `herding team register c48cfdff-0422-4299-b03c-804ac19f94f3 --name claude`.
4. Send each member its register command with `herdr pane run`, under its own
   name (table below). #81 re-binds each existing row, keeping roles and
   assignments. Never register under a new name; never remove roster rows.
5. `herding team task list`: reassign any task that reset.

## Members

All in workspace `w22`, resumed after the morning reboot.

| Member | Pane | Session | State | Next |
| --- | --- | --- | --- | --- |
| claude (supervisor) | `w22:pJ` | Claude `27a6c02d` | running wave 1 | phase 1-2 board tasks from the plan |
| vice-supervisor | `w22:pQ` | Claude `d8db213b` | reviewing PRs 88 and 90 | LAND or FIX on each |
| zcode1 | `w22:pM` | `sess_c6368d3e…` | PR 88 green, awaiting verdict | then the #35 wake diagnosis (F1/F2) |
| zcode2 | `w22:pN` | `sess_00d6a935…` | PR 90 open, fixes applied | rebase on PR 88, then phase 2.4 |
| zcode3 | `w22:pP` | `sess_2c5462d2…` | spikes A-D done | spike E (agent status into herdr-core) |
| opencode | `w22:pX` | this session | phase 1-2 tasks written | phase 1.1 after the wave lands |

- **vice-supervisor:** PR 88 (issue sync store) and PR 90 (layer D audit) are
  the wave-1 gate. Main is `2e808a02d` after PR 88's review fixes.
- **zcode1:** the issue sync store side landed as
  `team/issue-sync-store-zcode1` (PR 88); the original
  `team/issue-sync-zcode1` WIP branch is reviewed and superseded. Upstream
  kingsword09/zcode-cli#275 (the herdr-zcode plugin fix) is green, waiting on
  the maintainer.
- **zcode2:** layer D is `team/audit-log-zcode2` (PR 90), a separate branch
  from the parked layers A and B. Phase 2.4 starts from it.
- **zcode3:** spikes A-D are recorded as findings `2eb2b7f6` (A), `43ba946e`
  (B), `5c78160f` (C), `0c1aebe3` (D). C's result (no agent reports status
  to herdr-core) is phase 3's blocker.

## Open work

| Item | State | Recommendation |
| --- | --- | --- |
| PR 88 `team/issue-sync-store-zcode1` (171f4799) | green, awaiting vicesuper's verdict | LAND after the verdict; the four review fixes are in |
| PR 90 `team/audit-log-zcode2` (f4a5c9ee) | open, fixes applied | rebase on PR 88, then LAND; phase 2.4 starts from it |
| Phase 1-2 tasks | written in the plan | the supervisor creates the board tasks |
| #35 wake diagnosis (9863cb5d) | F2 confirmed, F1 plausible | F2 lands alone; F1 needs the pacing probe first |
| Spike E (agent status into herdr-core) | new, zcode3 | research and design only; C blocks phase 3 |
| #18, #19 old playbook PRs | open | user's call |
| kingsword09/zcode-cli#275 | open, green | wait for the maintainer |
| #85 control socket collision | filed | v1 fix if cheap; the pivot removes it |
| #74 prune/archive (0506677f) | open, unassigned | deferred: the plan schedules pruning after phase 6 |
| a0d0bc0a roster model/account | parked | folded into phase 2.2; branch kept as `archive/team/roster-profile-opencode` |

To park a branch: `git tag archive/<branch> origin/<branch>`, push the tag,
then close its PR with a comment naming the tag.

## What the pivot replaces

About 9,000 of today's 18,600 Teams lines (with tests) are deleted at phase 5.
That code ran the team and taught us what v2 needs; it just does not carry
over.

| Part | Lines | After the pivot |
| --- | --- | --- |
| Store, board, bulletin, roles (`sheprd-orchestrator/src/team_store*`) | ~4,900 | kept, moved into `sheprd-team` |
| Teams window (`sheprd-gpui/src/team/view*`) | ~3,900 | kept, reading the store directly |
| Bridge: wakes, prompts, queues, stall (`sheprd-gpui/src/team/bridge*`) | ~6,000 | mostly deleted |
| Window-side handlers and identity (`sheprd-gpui/src/control/server/team*`) | ~3,800 | mostly deleted |

- **Replaced:** the wake delivery fixes (#62, #40, drain on any endpoint,
  #35) by the doorbell over herdr-core's idle-gated send; identity proof (#81,
  lineage, peer bindings) by identity given at spawn; stall detection (#82) by
  `last_seen_at`; the skill's workaround, multi-machine, and reattach sections.
- **Carried over in part:** zcode2's audit log (phase 2's `audit` table); the
  idea of #58's handoff and layers A and B (approval requests); the store side
  of zcode1's issue sync.
- **Kept whole:** the board, bulletin, roles, CLI semantics, Teams window,
  supervisor playbook, fleet tooling, the upstream plugin fix, fork CI, and
  v1 itself until phase 5's full-day gate passes.

## Archived branches

The end-of-session cleanup deleted 15 local and 17 remote branches. Each was
merged into main, superseded, or abandoned. Every commit not in main is kept
as a tag on origin. Restore one with `git branch <name> archive/<name>`.

| Tag | Holds | Why deleted |
| --- | --- | --- |
| `archive/team/stall-zcode3` | first stall detection (#68) | superseded by #82; #68 closed |
| `archive/team/wake-host-zcode1` | wake routing by host (#66) | superseded by #72 |
| `archive/team/wake-land-zcode2` | wake landing, earlier cut | landed through #72 |
| `archive/team/green-main-zcode1` | remote tip of #71's branch | #71 merged; one CI-only commit left |
| `archive/tmp/ci-kick-71` | CI rerun branch for #71 | CI kick only |
| `archive/fix/team-caller-lineage` | caller process lineage check | replaced by #81 and, later, spawn identity |
| `archive/team/trust-doc-zcode-bee2` | socket trust model doc and test | stale since 2026-11-04 |
| `archive/team/endpoint-workspace-zcode-bee1` | remote workspace by session | stale since 2026-11-04 |
| `archive/team/pane-revision-doc-zcode2` | upstream proposal draft (ClientShellPane revision) | draft, never filed |
| `archive/docs/herding-playbook` | backlog-first playbook | duplicate of #18 |
| `archive/feat/v2-brand` | V2 brand brief and mark (#33) | work in progress, stale |
| `archive/preserve/supervisor-handoff-20261113` | #58 before its rebase | #58 is rebased |
| `archive/preserve/zero-touch-dispatch-20261113` | #57 before its rebase | #57 is rebased |
| `archive/team/stall-on-wakes-zcode2` | #82 stall detection | parked: the pivot replaces it with `last_seen_at` |
| `archive/feat/zero-touch-dispatch-rebased` | #57 zero-touch dispatch | parked: wake-path code the pivot deletes |
| `archive/feat/supervisor-handoff-rebased` | #58 supervisor handoff | parked: becomes an approval request in the pivot |
| `archive/team/identity-audit-zcode2` | layers A (guarded Remove) and B (restore) | parked: become phase-2 approval requests |
| `archive/team/roster-profile-opencode` | roster model and account (a0d0bc0a) | parked: folds into phase 2.2's member columns |

Deleted without a tag because main already holds their changes:
`fix/team-block-permission` (#63), `team/supervisor-playbook-vice` (#76),
`team/wakes-on-green-zcode2` and `team/wakes-on-main-vice` (#72),
`team/drain-any-endpoint-zcode2`, `team/fork-ci-zcode3` (#65's changes landed
in #71), `fix/stale-wake-targets` and its `preserve/` copy.

Kept: `main`, `v0-herding`, `v1-herding`, `team/host-identity-zcode3` (merged;
zcode3's worktree still uses it), `team/issue-sync-store-zcode1` (PR 88),
`team/audit-log-zcode2` (PR 90), `team/issue-sync-zcode1` (the superseded WIP
branch), `handoff-rebase-tmp` (vice-supervisor's worktree), and #18's and
#19's branches.

## Pivot order of work

`docs/herding-teams-on-core.md` is the authority. Where this note and the plan
disagree, the plan wins. This is only the order of work.

Phases (each leaves a working system; every team is marked `legacy` or
`daemon` in `teams.delivery`, so nothing switches over on one day):

0. Done: #81 merged (`6264db98e`). Today's Teams gets bug fixes only.
1. **Extract** `sheprd-team` from `team_store*` and `scoped()`
   (`crates/sheprd-gpui/src/control/server/team_worker.rs:679`): one set of
   types, one validation layer, wire byte-identical. Gate:
   `cargo tree -p sheprd-team` shows no GPUI or `gix`.
2. **Schema and inbox:** `user_version`, the new member columns, and the
   `inbox`, `doorbell`, `audit`, `requests` tables. Legacy teams behave as
   before, including with today's binary writing to the migrated file.
3. **The new path:** the `sheprd-team` binary on herdr-core terminals
   (`create`, `spawn`, `inbox`, approvals, the daemon). Daemon teams only.
   Needs spikes A, C, D.
4. **Remote:** `spawn --machine` and the tunnel. Needs spike B. Gate: an m1
   worker gets a doorbell, completes a task, and posts with the window closed.
5. **Cut over and delete:** `adopt` moves a team to `daemon`; each current
   member is started once on herdr-core with `spawn --resume`. A member left on
   a Herdr 1 pane only pulls its inbox; it gets **no doorbell**.
6. **Headless and UI:** `watch`, spawn limits, cost budget (#73), then the mod
   spike and the repository decision.

Spikes A-D are done (task `8d2c281f`); spike E is the phase-3 blocker:

- **A. Restart:** done (finding `2eb2b7f6`) - terminal ids, names, and labels
  survive a graceful stop and a crash restart.
- **B. Remote spawn:** done (finding `43ba946e`) - `Run.env`, labels, status
  events, gated-send receipts, and dialog refusal work on m1; `HC_TERMINAL_ID`
  is bare, so spawn carries the machine name in the environment.
- **C. Receipts per agent:** done (finding `5c78160f`) - the core's semantics
  are correct, but no agent reports status to it today; phase 3 needs spike E.
- **D. Terminal from the core:** done (finding `0c1aebe3`) - the environment
  is set in every local terminal; the core does not resolve callers from peer
  credentials, so identity stays environment-carried and the audit records
  the claimed terminal.

Sequencing: phase 1 moves `team_store` and `scoped()`, and the wave-1
branches touch those files: PR 88 (`team/issue-sync-store-zcode1`) and PR 90
(`team/audit-log-zcode2`) land first. #82 stall detection, #57 zero-touch
dispatch, #58 supervisor handoff, layers A and B, and the roster profile are
already parked as `archive/` tags. Land or park every branch before the
extraction starts, never during it.

| Part | Ready | Note |
| --- | --- | --- |
| Spikes A-D | done | findings `2eb2b7f6`, `43ba946e`, `5c78160f`, `0c1aebe3`; C blocks phase 3 |
| Phase 1 (extract) | after PR 88 and PR 90 | mechanical, clear gate; a cheaper worker can do it with the full diff reviewed |
| Phase 2 (schema, inbox) | after phase 1 | 2.4 needs PR 90's audit table; the exact DDL is in the plan's task list |
| Phase 3 onward | no | needs spike E (agent status into herdr-core) |

The plan now carries one task per phase-1 and phase-2 step, with scope,
acceptance checks, dependencies, and review tier. The supervisor creates the
board rows from it before assigning.

Audit schema: PR 90 creates the unified `audit` table (the plan's shape plus
the identity snapshot columns, `actor` defaulting to `unknown`, `ref_id =
member_id` for identity ops). Phase 2.4 fills `actor` and `terminal` from
`apply`, logs `delete_team`, and edits the plan's Audit section to match.

**Approvals (decided 2026-11-13):** `approve` refusing a caller that resolves
to a member, plus the audit log, is enough. The risk is mistakes, not malice,
so there is no phone or window confirmation. See the plan's Approvals section.

## Rules and gotchas

- Never modify Herdr. Issues and PRs only in `carteakey/herding`, never
  `penso/herdr-gpui`. `gh pr list` without `-R carteakey/herding` shows
  upstream.
- No AI attribution in commits or PRs. No force-push to main. The shared
  checkout stays on main; workers use worktrees.
- Keep 15G of disk free: `CARGO_INCREMENTAL=0`, `docs/build-cache.md`.
- Fork CI is commits plus the Linux checks job only. Never enable
  `close-low-confidence-prs.yml`.
- Tiered review: read the full diff for the team store, wake delivery,
  protocol, and security; for the rest, green CI plus the worker's report.
- If team commands fail with "unknown variant `team`", the old
  `/usr/bin/herdr-gpui` owns the socket: ask the user to quit it.
- Use LAN IPs for the fleet; m1's Tailscale name did not answer. Fleet steps:
  `~/.agents/skills/fleet-rollout/SKILL.md`.
- Reattach procedure: the "Reattach after a reboot or restart" section of
  `skills/herdr-team/SKILL.md`.
