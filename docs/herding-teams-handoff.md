# Herding Teams handoff, 2026-11-13

From the supervisor session (`claude`, pane `w22:pJ`) to whoever supervises
next. Team `c48cfdff-0422-4299-b03c-804ac19f94f3`.

## Where things stand

The team pivots to the Teams-on-core redesign once #81 merges and everyone
re-registers. Until then, today's window-based Teams gets fixes that keep the
team running, and no new features.

- **The pivot:** Teams becomes a library crate (`sheprd-team`) that the
  `herding team` CLI and the Teams window both open directly, plus a small
  delivery loop on herdr-core. Identity is given at spawn through terminal
  labels; wakes become an inbox with a short doorbell. The plan is
  `docs/herding-teams-on-core.md`. It is up to date but **not committed**
  (uncommitted change in the main checkout; commit it as `docs(team): ...`).
- **Why:** this morning's reboot broke the team. The old Herdr UI app
  (`/usr/bin/herdr-gpui`) took Herding's control socket (#85), and every
  roster row stayed bound to a pre-reboot pane, so no one could re-register.
- **Repo:** a crate in this repo for now, kept to SQLite, serde, and
  `sheprd-core-client` as dependencies. Split it into its own repo
  (`herding-teams`) once the types settle, around the mod spike.
- **Team CLI is blocked** for every member until #81 lands. Coordinate with
  `herdr pane run <pane> "..."` until then.

## Merge #81 and re-register everyone

#81 (`team/host-identity-zcode3`) is reviewed and approved at tip `2191e2903`.
Merge it when its Linux CI job passes.

1. `gh pr checks 81 -R carteakey/herding`: "Format, lint, and test" must say
   pass. `skipping` is not a pass.
2. Merge #81 to main.
3. Rebuild and restart: `CARGO_INCREMENTAL=0 cargo build --release -p
   sheprd-gpui --locked`, then restart `target/release/herding`.
4. `ss -xlp | grep gpui/control.sock` must show `herding`, not `herdr-gpui`.
5. Supervisor first, from its own pane:
   `herding team register c48cfdff-0422-4299-b03c-804ac19f94f3 --name claude`.
6. Send each member its register command with `herdr pane run`, under its own
   name (table below). Never register under a new name; never remove roster
   rows.
7. `herding team task list`: reassign any task that reset.
8. Delete the "Known gap" paragraph from the reattach section of
   `skills/herdr-team/SKILL.md`; #81 closes it.

What #81 does: after a reboot, a member takes back its row from a new pane
when the row's daemon is dead. On a live daemon it may only do so once the old
pane has no running agent, so no one can take over a live member's row,
including the supervisor's.

## Members

All in workspace `w22`, resumed after the reboot.

| Member | Pane | Session | Doing now | Next |
| --- | --- | --- | --- | --- |
| claude (supervisor) | `w22:pJ` | Claude `27a6c02d` | handoff | merge #81, start the pivot |
| vice-supervisor | `w22:pQ` | Claude `d8db213b` | idle | m1 live wake test after #81 |
| zcode1 | `w22:pM` | `sess_c6368d3e…` | waiting on review | live check of issue auto-sync after merge |
| zcode2 | `w22:pN` | `sess_00d6a935…` | roster removal and restore, layer B | stall-detection PR replacing #68 |
| zcode3 | `w22:pP` | `sess_2c5462d2…` | #81 CI | spikes A-D, now |

- **vice-supervisor:** main `d091c73e0` is built and checked on fx505 and m1;
  m2 is down. The m1 wake test needs a member registered from m1 through
  `scripts/control-tunnel.sh m1`, so it waits on #81.
- **zcode1:** upstream PR kingsword09/zcode-cli#275 (the herdr-zcode plugin
  fix) is open and green, waiting on the maintainer. Its issue auto-sync
  branch (`team/issue-sync-zcode1`, `5b50a1827`, task 171f4799) touches the
  team store and bridge, so it needs a full diff review before merging.
- **zcode2:** working on identity-protection layers D, A, B (audit log,
  guarded Remove, removal with restore), approved in that order.
- **zcode3:** can start spikes A-D now (they do not need #81); after #81,
  record its board updates for c2a506e0.

## Open work to land or close

| Item | State | Owner |
| --- | --- | --- |
| #81 host identity and rebind | approved, CI pending | supervisor merges |
| `team/issue-sync-zcode1` (171f4799) | pushed, needs full review | supervisor |
| Stall detection (`team/stall-on-wakes-zcode2`) | accepted, needs rebase and a PR replacing #68 | zcode2 |
| #57, #58 rebases (8d73788c, 02d0e932) | #58 rebased at `605791a62`, CI pending | vice-supervisor |
| kingsword09/zcode-cli#275 | open, green | upstream maintainer |
| #85 control socket collision | filed | pivot removes it for teams |
| Board: a0d0bc0a, 0506677f (#74) | open | unassigned |

## Starting the pivot

`docs/herding-teams-on-core.md` is the authority. Where this note and the plan
disagree, the plan wins. This is only the order of work.

Phases (each leaves a working system; every team is marked `legacy` or
`daemon` in `teams.delivery`, so nothing switches over on one day):

0. #81 merges. Today's Teams then gets bug fixes only.
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
| Spikes A-D | yes, now | research only, independent of #81 |
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
