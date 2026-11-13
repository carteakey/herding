# Herding Teams handoff, 2026-11-13

From the supervisor session (`claude`, pane `w22:pJ`) to whoever supervises
next. Team `c48cfdff-0422-4299-b03c-804ac19f94f3`. Updated at the end of the
session. For the why behind every decision here, read
`docs/herding-teams-decision-log.md`.

## Where things stand

- **v1 is released.** Herding v1.0.0 (branch `v1-herding`, tags `v1` and
  `v1.0.0`, main `6264db98e`) checkpoints Teams as it runs in the window,
  including #81. Release notes only, no binaries.
- **The pivot is on hold.** The user will restart it deliberately. Do not
  start spikes or the `sheprd-team` extraction until they say so. zcode3 was
  told to hold.
- **Use v1 to build v2.** The v1 team coordinates the workers who will build
  the pivot, so v1 gets reliability fixes only. Do not extend code the pivot
  deletes (see "What the pivot replaces").
- **A new Herding build with #81 is ready** (`target/release/herding`, 13:37)
  but not running. Restart it, then re-register everyone (next section).
- **The plan** is `docs/herding-teams-on-core.md`. Where this note and the plan
  disagree, the plan wins.

## First: restart and re-register

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
| claude (supervisor) | `w22:pJ` | Claude `27a6c02d` | session ended | re-register, park the work below |
| vice-supervisor | `w22:pQ` | Claude `d8db213b` | idle | m1 live wake test, now unblocked by #81 |
| zcode1 | `w22:pM` | `sess_c6368d3e…` | waiting on review | issue sync: land the store side only |
| zcode2 | `w22:pN` | `sess_00d6a935…` | layers D, A, B | finish D (audit log); park A and B |
| zcode3 | `w22:pP` | `sess_2c5462d2…` | holding | board bookkeeping for c2a506e0 after re-registering |

- **vice-supervisor:** main `d091c73e0` is built and checked on fx505 and m1;
  m2 is down. The m1 wake test registers a member from m1 through
  `scripts/control-tunnel.sh m1`; roll the new main out first.
- **zcode1:** upstream kingsword09/zcode-cli#275 (the herdr-zcode plugin fix)
  is open and green, waiting on the maintainer. Its issue sync branch
  (`team/issue-sync-zcode1`, `5b50a1827`, task 171f4799) touches the team
  store and bridge: review the full diff, land the store side, and leave the
  window-side sync loop for the pivot.
- **zcode2:** layer D (audit log) carries into the pivot's phase 2. Layers A
  (guarded Remove) and B (removal with restore) become the pivot's approval
  requests; park them.

## Open work

| Item | State | Recommendation |
| --- | --- | --- |
| `team/issue-sync-zcode1` (171f4799) | pushed, needs full review | land the store side |
| zcode2's layer D (audit log) | in progress | finish; phase 2 builds on it |
| zcode2's layers A, B | in progress | park as `archive/` tags |
| #82 stall detection (`team/stall-on-wakes-zcode2`) | open | park: the pivot replaces it with `last_seen_at` |
| #57 zero-touch dispatch | open | park: wake-path code the pivot deletes |
| #58 supervisor handoff (`feat/supervisor-handoff-rebased`) | open | park: becomes an approval request in the pivot |
| #18, #19 old playbook PRs | open | user's call |
| kingsword09/zcode-cli#275 | open, green | wait for the maintainer |
| #85 control socket collision | filed | v1 fix if cheap; the pivot removes it |
| Board: a0d0bc0a, 0506677f (#74) | open | unassigned |

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

Deleted without a tag because main already holds their changes:
`fix/team-block-permission` (#63), `team/supervisor-playbook-vice` (#76),
`team/wakes-on-green-zcode2` and `team/wakes-on-main-vice` (#72),
`team/drain-any-endpoint-zcode2`, `team/fork-ci-zcode3` (#65's changes landed
in #71), `fix/stale-wake-targets` and its `preserve/` copy.

Kept: `main`, `v0-herding`, `v1-herding`, `team/host-identity-zcode3` (merged;
zcode3's worktree still uses it), `team/issue-sync-zcode1`,
`team/identity-audit-zcode2`, `team/stall-on-wakes-zcode2`, #57's and #58's
branches, `handoff-rebase-tmp` (vice-supervisor's worktree), and #18's and
#19's branches.

## When the pivot resumes

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

Spikes (research only, scratch `HOME`, private `HC_SOCKET`; give to zcode3):

- **A. Restart:** do terminal ids and labels survive a herdr-core restart?
- **B. Remote spawn:** do `Run.env` and `Run.labels` reach a terminal started
  with `on`, and do events report its status changes?
- **C. Receipts per agent:** for ZCode, Claude Code, and Codex, does
  `send { if: "idle" }` return `started: true`, and is a send into a
  permission dialog refused as blocked?
- **D. Terminal from the core:** confirm herdr-core sets `HC_TERMINAL_ID` in
  every terminal it starts (the `herding` CLI and `sheprd-automate` already
  read it), then whether the CLI can learn its terminal from the core rather
  than its environment.

Sequencing: phase 1 moves `team_store` and `scoped()`, and three in-flight
branches touch those files: zcode1's `team/issue-sync-zcode1`, zcode2's stall
detection, and zcode2's layers D, A, B. Land or park all three before the
extraction starts, never during it.

| Part | Ready | Note |
| --- | --- | --- |
| Spikes A-D | yes, when the pivot resumes | research only |
| Phase 1 (extract) | after the three branches | mechanical, clear gate; a cheaper worker can do it with the full diff reviewed |
| Phase 2 (schema, inbox) | nearly | needs the layer D reconciliation and exact DDL as board tasks |
| Phase 3 onward | no | waits on spike results |

The plan gives gates per phase, not task-sized work. Break phases 1 and 2
into board tasks, each with scope and acceptance checks, before assigning.

Audit schema: zcode2's in-flight layer D (audit log) and the plan's `audit`
table must be one schema. Whoever takes phase 2 starts from zcode2's work and
reconciles the plan's table to it.

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
