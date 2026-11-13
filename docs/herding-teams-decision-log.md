# Herding Teams decision log, 2026-11-12 to 2026-11-13

Why Teams is pivoting to herdr-core, what happened on the way, and the reason
behind each decision. The plan itself is `docs/herding-teams-on-core.md`; the
current state and next steps are in `docs/herding-teams-handoff.md`. This file
is the record of how we got there.

## The short version

Teams works, but it lives inside the Herding window, and almost every failure
in two days came from that. The window held the store, the wake queue, the
identity check, and the control socket, so a reboot, a restart, a pane move,
or another app on the socket stopped the whole team. The pivot moves the
records into a library the CLI opens directly, gives each member its identity
when it is spawned, and replaces pasted wakes with an inbox and a short
doorbell sent through herdr-core. v1.0.0 checkpoints the window-based Teams,
and the v1 team will build v2.

## What broke, and what each break taught

| When | What happened | Root cause | What it means for the design |
| --- | --- | --- | --- |
| 2026-11-12 | ZCode panes showed status "unknown" | herdr-zcode 0.1.1 appended `-- zcode --resume <id>` to `herdr pane report-agent`, which rejects `--` (zcode1) | Status comes from what the agent reports. Fixed in the plugin; upstream kingsword09/zcode-cli#275 |
| 2026-11-12 | Roster rows vanished | The user removed members in the Teams window while the plugin bug made live members look dead (zcode2) | Removal needs a guard and an audit trail: layers D (audit), A (guarded Remove), B (restore) |
| 2026-11-11 to 12 | ZCode wakes arrived as "f" and "in" | The bracketed paste path truncated them and reported success on transport alone (#62) | Delivery needs a receipt, not a queue entry |
| 2026-11-11 to 12 | Wakes held forever or went to the wrong host | The window decided what drained and could reach only hosts it was connected to (#35, #40, #41) | The wake queue must not live in a window |
| 2026-11-12 | Disk at 97%, the logger died | Worktree builds and incremental caches (#78) | Keep 15G free; a team service must build without GPUI |
| 2026-11-13 morning | After a reboot every member showed "Session not attached" | Upstream `herdr-gpui` autostarted and took Herding's control socket, so team commands failed with "unknown variant `team`" (#85) | Nothing about a team should depend on which app owns a socket |
| 2026-11-13 morning | Nobody could re-register after the reboot | Rows were bound to pre-reboot pane ids on a dead daemon boot; re-binding required the same pane | Pane ids are not identity. Fixed for v1 in #81; the pivot gives identity at spawn instead |

## The hc spike (spike 1)

zcode3 tested `hc send`, herdr-core's command to type into an agent, in a
scratch HOME. Prompts over 2,700 bytes arrived intact in idle ZCode and Claude
agents, locally and on m1 after `hc machine add`. Two catches: a send into an
open dialog types into the dialog unless status is checked first, and machines
must be added by LAN name. Conclusion: herdr-core can carry team wakes. That
result made the pivot practical.

## How the design changed

**First plan (2026-11-12): a Teams service.** Teams would run as our own
service beside herdr-core, with a choice of UI on top (Herding's window, a mod
in Herdr's official app, or the CLI). We would never modify Herdr; the
service is its client. Herdr's official app and the community app (Sheprd)
both lack a team layer, so Teams is the differentiator either way.

**Second plan (2026-11-13, midday): port the service, maybe a new repo.** The
supervisor proposed a socket service with four phases and a new repository,
arguing the repo split would cut build time.

**Review (Fable, 2026-11-13).** A second model reviewed the code and the plan
and found it ported too much:

- The herdr-core client already in this repo (`crates/sheprd-core-client`)
  has what delivery needs: `terminal.send` with an `if: idle` gate that the
  core checks as it types, a `Sent` receipt saying whether the agent started
  working, `terminal.run` with environment, labels, and a target machine,
  `terminal.wait`, and `events.subscribe`. The supervisor had only read
  Herdr's website.
- The store logic (`scoped()` in `team_worker.rs`) is already free of the
  window. A socket service in front of it is unnecessary locally.
- The build-cost argument was wrong. A crate in this workspace builds only its
  own dependencies: `sheprd-automate`, the same shape as a team service,
  resolves to 121 crates; the app resolves to 914. Verified with `cargo tree`.

**Adopted plan.** Three changes make it smaller rather than a port:

1. **A library, not a service.** `herding team` opens the SQLite store
   directly. Records work with nothing running, which removes the socket
   collision and identity loss on restart.
2. **Identity at birth.** `team spawn` starts the member's terminal with
   labels naming its team and member. No registration step left to break.
3. **An inbox and a doorbell.** Everything for a member is an inbox row,
   written in the same transaction as the record. A small loop rings an idle
   member with one fixed line, "you have N items". A missed doorbell loses
   nothing.

## Decisions and their reasons

| Decision | Reason |
| --- | --- |
| Never modify Herdr; Teams is its client | Herdr belongs to another team; the official app and Sheprd can both sit in front |
| Keep the name Herding; a future repo would be `herding-teams` | Herding is already the product identity; `herdr-` is Herdr's brand |
| A crate in this repo now, a separate repo later | Wire types will change through the inbox, address, and role work; a split now means a tag and a bump in two repos for every change. Split once the types settle, around the mod spike |
| New members are herdr-core terminals; Herdr 1 panes only pull their inbox, no doorbell | Herdr 1 has no labels, idle-gated send, or events. Moving a worker is a restart with `--resume`, which keeps its conversation |
| Trust model is "same user" | Identity from labels and environment can be claimed by any process running as the user. Spike D checks whether the core can confirm the caller's terminal |
| Approvals: `approve` refuses member callers, plus the audit log; no phone or window confirmation | The user's call: the agents are the user's own, so the risk is mistakes, not malice. A code in the database would be readable by any same-user agent anyway. Revisit if teams include agents the user does not run |
| One audit schema, starting from zcode2's layer D | Two designs of the same table would conflict in phase 2 |
| Old window and new CLI share one SQLite file during the move, with additive schema changes and a version check | Each team switches over on its own (`legacy` or `daemon`), so nothing flips on one day |
| Land or park the three branches that touch the store before phase 1 | Phase 1 moves those files; extracting during open work guarantees conflicts |
| #81 merged and included in v1 | Without it a reboot breaks the team, which is the failure v1 should not ship with |
| #81's re-bind rule | Pane ids change on reboot, so a dead daemon's row re-binds from any pane in the same workspace. On a live daemon, only once the old pane has no running agent. Two review rounds closed takeover holes (below) |
| v1.0.0 as a checkpoint like v0 (branch plus tags plus notes-only release) | A known-good state to fall back to while v2 is built |
| Use v1 to build v2; v1 gets reliability fixes only | The v1 team coordinates the work on v2. Work on code the pivot deletes is parked |
| Pivot on hold after v1 | The user wants to restart it deliberately |
| Never remove roster rows to fix identity; never register under a new name | Rows carry the supervisor role, assignments, and mention history; a new row is not the supervisor |

## The #81 review

#81 lets a member take back its row after a reboot. Two review rounds
tightened who may take a row from another pane:

1. The first version let a session on the same machine re-bind across panes
   whenever the row's daemon was its own. Any agent running
   `register --name claude` would have taken the supervisor's row and role
   while the supervisor was alive in another pane. Fix: across panes, only
   when the old pane has no running agent.
2. The second version still let rows recorded under the current boot id (the
   old row format) re-bind pane-free. Fix: the same live-pane condition for
   those rows; only a dead daemon's row re-binds from any pane.

Both fixes came with tests, including the supervisor case.

## What the pivot throws away

About 9,000 of the 18,600 Teams lines, mostly the window-side wake delivery
and identity proof where this week's fixes went. That work was not wasted: it
ran the team, and each failure above is now a requirement or a gate in the
plan. The breakdown and the park list are in the handoff, "What the pivot
replaces".

## How the team was run

- One supervisor on an expensive model, workers on cheaper ones. Routing
  saves tokens only when the supervisor stays thin, so the team skill now
  carries a cost-aware playbook: tiered review (full diffs only for the store,
  delivery, protocol, and security), a structured worker report, delegated
  diagnosis, no pane polling, and CI as the gate (#73).
- When the team CLI was down after the reboot, coordination fell back to
  `herdr pane run`. The reattach procedure is in the team skill.
- Identity code got full-diff review, and that is where both #81 holes were
  found.
- Branch cleanup kept every unmerged commit as an `archive/*` tag before
  deleting anything; the handoff lists them.

## Open questions

- Spikes A to D: do terminal ids and labels survive a core restart; do
  environment and labels reach a terminal started on another machine; does the
  idle-gated send return a receipt for each agent and refuse a dialog; can the
  core tell the CLI which terminal it runs in.
- How remote members call in: an SSH tunnel kept up by the daemon, a request
  carried over `hc`, or a small authenticated listener.
- Whether a mod in Herdr's official app can show the Teams board (spike 2),
  which decides the UI layer and the repo split.
