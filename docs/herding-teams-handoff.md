# Herding Teams handoff, 2026-11-14

From the supervisor session (`super`, pane `w22:pJ`) to whoever supervises
next. Team `c48cfdff-0422-4299-b03c-804ac19f94f3`. Checkpointed at the
user's request on 2026-11-14. For the reason behind each decision, read
`docs/herding-teams-decision-log.md`.

## Where things stand

- **v1 is released.** Herding v1.0.0 (branch `v1-herding`, tags `v1` and
  `v1.0.0`, `6264db98e`) checkpoints Teams as it runs in the window.
- **The pivot is running, and phase 1 has started.** Main is `d007abdb1`.
  Wave 1 landed: PR 88 (issue sync, store side), PR 90 (audit log, created
  as the final `audit` table), PR 92 (F2 wake fix), and PR 91 (phase 1 and 2
  tasks in the plan, plus team templates in phase 6). Spikes A to E are
  done.
- **Use v1 to build v2.** The v1 team builds the pivot, so v1 gets
  reliability fixes only.
- **The running app** is the 2026-11-13 13:38 build of `6264db98e`. It has
  neither wake fix: app wakes are unreliable, so the supervisor sends them by
  hand with `herdr pane run`, and in Claude Code panes the user presses Enter.
  PR 92 is on main; PR 93 waits for native QA.

## Next steps, in order

1. **PR 93 native QA (F1, the Enter fix).** The binary is built at the
   reviewed tip `71dadaf44`:
   `/home/kchauhan/repos/forks/herding-wake-enter-zcode1/target/release/herding`.
   The user quits Herding and starts that binary. Pass: a mention to zcode1's
   scratch Claude pane `w22:pZ` lands its text and submits about 200 ms
   later; the same happens in a ZCode pane; the supervisor's pane no longer
   needs Enter pressed. On a pass, merge PR 93: CI and vicesuper's LAND are
   in. Then rebuild main in the shared checkout
   (`CARGO_INCREMENTAL=0 cargo build --release -p sheprd-gpui --locked`) and
   restart from it.
2. **Phase 1.1 (zcode3, d168eb1a).** It is done on
   `team/phase1-scaffold-zcode3` at `01831a644`, one commit on `d007abdb1`, and
   not pushed. Authorize the push and the PR, then get vicesuper's full-diff
   review.
3. **Answer the prep questions** before 1.2 and 1.3 start: zcode1's 1.3
   questions (bulletin seq 440: scope now includes `identity.rs` and
   `prefix.rs`; the `private_file` seam) and zcode2's 1.2 questions (seq 444).
4. **Then 1.2 and 1.3 run in parallel, followed by 1.4 and 1.5** (opencode,
   on one branch with two commits and one PR).

## Decisions waiting on the user

- **Upstream report for herdr-core's 7501 probe.** Spike E found that
  herdr-core answers Claude Code's status probe on the screen instead of the
  program's input, so Claude never reports status. zcode3's draft is at
  `/tmp/zcode3-spikes/upstream-report-draft.md`. It is not filed; only the
  user can decide to send it upstream.
- **Team templates issue.** The draft is at
  `/tmp/claude-1000/-home-kchauhan-repos-forks-herding/27a6c02d-4e46-49a8-9b52-5a9a222827bf/scratchpad/templates-issue.md`.
  Filing it was blocked by the permission check. The idea is in the plan's
  phase 6 either way.
- **`~/.cache/uv` (16G) and `~/.cache/huggingface` (3.4G)**, if disk gets
  tight again.

## Restart and re-register

Terminals and agents survive an app restart, and #81 re-binds members, so a
restart needs no roster work in the normal case.

1. Restart the app binary. `ss -xlp | grep gpui/control.sock` must show
   `herding`, not `herdr-gpui` (#85).
2. `herding team status` from each pane. If a member's binding dropped, send
   it `herding team register c48cfdff-0422-4299-b03c-804ac19f94f3 --name <name>`
   with `herdr pane run`, under its own name. Never register under a new name
   and never remove roster rows.
3. `herding team task list`: reassign any task that reset.

## Members

The user rebuilt the roster on 2026-11-13: the old `claude` supervisor row
and the pre-reboot worker rows were removed in the window, and the members
below were added. All are in workspace `w22`.

| Member | Pane | Agent | Role | Now |
| --- | --- | --- | --- | --- |
| super | `w22:pJ` | Claude Code, Opus | supervisor | coordinates and lands |
| vicesuper | `w22:pY` | Claude Code, Sonnet | full-diff reviewer (4cf01afd) | reviews every phase 1 PR; LAND or FIX verdicts, never lands |
| opencode | `w22:pX` | opencode, stronger than the workers | planner, harder tasks | 1.4 and 1.5, blocked on 1.3 |
| zcode1 | `w22:pT` | ZCode | worker | PR 93 QA, then 1.3; scratch Claude pane `w22:pZ` for QA |
| zcode2 | `w22:pV` | ZCode | worker | 1.2, prep done |
| zcode3 | `w22:pW` | ZCode | worker | 1.1 done, awaiting AUTHORIZED |

How this team works:

- Reports go to `@super` and `@vicesuper`. vicesuper posts the verdict, super
  lands: a fast-forward when main has not moved, a GitHub rebase merge when
  the files don't overlap.
- Until the wake fixes run, send every important bulletin as a manual
  `herdr pane run` wake as well.
- `--reply-to` with a short id prefix is sometimes refused; use the full id.

## Open work

| Item | State | Next |
| --- | --- | --- |
| PR 93 F1 delayed Enter (`team/wake-enter-zcode1`) | LAND from vicesuper, CI green | native QA, then merge |
| 1.1 `team/phase1-scaffold-zcode3` (d168eb1a) | done, local | push, PR, review |
| 1.2 (8dcc7816, zcode2), 1.3 (ea038667, zcode1) | prep and questions posted | start when 1.1 lands |
| 1.4 (72845fce), 1.5 (58c26ef7), opencode | design decided (bulletin seq 434) | after 1.3 |
| Phase 2 (2.1 to 2.6) | written in the plan | board tasks after phase 1 |
| Spike E (40db3d58) | done | ZCode: plugin prototype on `feat/herdr-core-status-report` in the ZCode fork (`815fe4872`, not pushed). Claude: upstream fix or our own wrapper. Codex: one signed-in run to confirm |
| a0d0bc0a roster model and account | parked (`archive/team/roster-profile-opencode`) | the board still shows it in progress; cancel it |
| #74 prune/archive (0506677f) | unassigned | deferred |
| #18, #19 old playbook PRs | open | user's call |
| kingsword09/zcode-cli#275 | open, green | wait for the maintainer |
| #85 control socket collision | filed | the pivot removes it |

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
its worktree is gone), `team/issue-sync-store-zcode1` (PR 88, merged),
`team/issue-sync-zcode1` (the superseded WIP branch), `team/wake-fix-zcode1`
(PR 92, merged), `team/wake-enter-zcode1` (PR 93), `handoff-rebase-tmp`, and
#18's and #19's branches. zcode2 deleted `team/audit-log-zcode2` after PR 90
merged.

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
