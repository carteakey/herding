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
| zcode3 | `w22:pP` | `sess_2c5462d2…` | #81 CI | the pivot spikes |

- **vice-supervisor:** main `d091c73e0` is built and checked on fx505 and m1;
  m2 is down. The m1 wake test needs a member registered from m1 through
  `scripts/control-tunnel.sh m1`, so it waits on #81.
- **zcode1:** upstream PR kingsword09/zcode-cli#275 (the herdr-zcode plugin
  fix) is open and green, waiting on the maintainer. Its issue auto-sync
  branch (`team/issue-sync-zcode1`, `5b50a1827`, task 171f4799) touches the
  team store and bridge, so it needs a full diff review before merging.
- **zcode2:** working on identity-protection layers D, A, B (audit log,
  guarded Remove, removal with restore), approved in that order.
- **zcode3:** after #81, record its board updates for c2a506e0, then take the
  spikes below.

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

Do these once #81 is merged. Spikes first; they decide the identity design.

Spikes, research only, in a scratch HOME (give to zcode3):

1. Do herdr-core terminal ids and labels survive a core restart?
2. Do `terminal.run` env and labels reach a terminal started with `on` on m1?
3. Does `events.subscribe` report status changes for m1's terminals?
4. Can a running command ask the core which terminal it is in? Fable cited
   `HC_TERMINAL_ID`; our client code never reads it, so it is unconfirmed.

Phase 1, no behaviour change (give to zcode2 once its stall PR is up): extract
`sheprd-team` with `team_store` and `scoped()`
(`crates/sheprd-gpui/src/control/server/team_worker.rs:679`), one validation
layer, one set of serde types. Dependencies: SQLite, serde,
`sheprd-core-client` only.

Then phases 2 to 5 as the plan lays out: CLI opens the store in-process plus
`team create`; inbox, outbox, and `team spawn`; remote members (gate: an m1
worker is woken, completes a task, and posts with the window closed); delete
the window code and decide the UI.

Decisions already taken:

- Trust model is "same user". Removing members and changing roles needs the
  user's approval through the CLI, using something an agent cannot produce (a
  one-time code or a phone confirmation), and every write is logged.
- Schema changes are additive with a version check while the old window and
  the new CLI share one SQLite file.
- New members are herdr-core terminals. Herdr 1 panes become pull-only with a
  `herdr pane run` doorbell. Moving the current workers is a restart with
  `--resume`.

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
