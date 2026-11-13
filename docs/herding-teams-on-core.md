# Teams on herdr-core: direction and spikes

Status: proposal, 2026-11-12; spike 1 passed and a porting plan added 2026-11-13. Nothing here is decided until the spikes below
report. It extends the companion-daemon plan in `docs/herding-team-review.md`.

## Target shape

```
 UI layer (pick one or more)       Herding window · Herdr app mod · herding team CLI
            │
 Teams service (ours)               store, board, bulletin, wake queue, roles
            │  hc send / hc machine
 herdr-core (Herdr's, unmodified)   terminals, agents, machines, status
```

The Teams service runs beside herdr-core as its client, never inside it. The UI
layer is a choice: Herding's Teams window, a mod in Herdr's official app, or
both. Agents reach Teams through the `herding team` CLI whichever app is in
front. Open: one service on the supervisor's host as the single authority, or
one per host that syncs.

## What is ours

Three things sit in this stack, and only one is this fork's own work:

| Piece | Owner | What it brings |
| --- | --- | --- |
| Sheprd (`penso/herdr-gpui`) | upstream community app this fork tracks | the GPUI window, browser, git and PR views, cloud machines, automations, phone remote control |
| **Teams** | **this fork** | the Teams window, task board, bulletin, roles, findings, wakes, the `herding team` CLI, and the supervisor/worker workflow |
| Herdr's official app and herdr-core (`hc`) | Herdr ([herdr.dev](https://herdr.dev/), Apache-2.0) | the terminal runtime, `hc machine add` over SSH or Tailscale, agent status over OSC 7501, and agent-to-agent `hc run --bg`, `hc send`, `hc wait`, `hc read` |

Neither Sheprd nor the official app has a team layer. Teams is the
differentiator, so the question is not which app to use. It is which base gives
Teams the most reliable delivery and multi-machine reach.

## Where Teams depends on its base

Teams touches the base in two places only:

1. **Delivery.** A wake is pasted into the member's pane as bracketed text plus
   a delayed Enter. That path truncated ZCode wakes to "f" and "in" and
   reported success on transport alone (#62). Status is read from pane output,
   which misreports ZCode (#56, #61).
2. **Reach.** A remote member is reached through the GUI's connection to that
   host's daemon, so the store and wake queue live in the supervisor's window
   (#8, #40, #41, #42).

The board, bulletin, roles, store, and CLI do not depend on the base.

## Proposed direction

Run Teams as a standalone service on herdr-core, the companion daemon from
`docs/herding-team-review.md`, using `hc` as a client:

- The service owns the SQLite store, the wake queue, and bindings. No app
  window decides what drains or holds the authority.
- Wakes go out through `hc send` instead of a synthesized paste. Remote members
  are reached through `hc machine`.
- Any app in front is a view:
  - Herding keeps its Teams window.
  - Herdr's official app gets a Teams mod (it has a mods system,
    `~/.config/herdr-app/mods`).
  - Agents keep using the `herding team` CLI.

This uses herdr-core and `hc` as published; it does not patch them
(`AGENTS.md`: never modify Herdr). One Teams codebase then serves Herding, the
official app, and the TUI, and a sister fork of the official app is needed
only if a mod cannot show the Teams window.

### Known constraint

Herdr 1 (TUI daemon) and herdr-core sessions do not share agents. Today's team
panes run on Herdr 1, so delivering through `hc send` means moving the team's
panes to herdr-core. Herding already speaks both
(`crates/sheprd-core-client`), so this is a pane migration, not an app switch.

## Spikes, in order

1. **`hc send` as the wake path** (board task `bfe3cff1`). Research only, in a
   scratch HOME. Answer:
   - Does `hc send` reach an idle ZCode or Claude agent and submit it, with no
     truncation?
   - Does it reach a terminal on another machine added with `hc machine add`
     (m1)?
   - What would Herding need to route team wakes through it?
2. **A Teams mod in the official app.** Queued only if spike 1 passes. Can a
   mod show a board and bulletin and run `herding team` commands?
3. **Decision:**
   - If mods suffice: build the service plus a mod, with no fork.
   - If mods cannot: start a thin sister fork, thin because Teams lives in the
     service.

Until then, #71 and #72 still land: they make today's Herdr 1 path work while
the spikes answer what comes next.

## Spike 1 outcome

`hc send` passed (zcode3, board task `bfe3cff1`, 2026-11-13): prompts over
2,700 bytes arrived intact in idle ZCode and Claude agents, locally and on m1
through `hc machine add`. Two catches: a send into an open dialog types into
the dialog, so delivery must check agent status first (ZCode reports it only
with the plugin's environment set), and machines must be added by LAN name.

## Porting plan: Teams as a service, UI as a client

The Teams code today, in non-test lines:

| Layer | Lines | In the service |
| --- | --- | --- |
| Store, board, bulletin, roles (`sheprd-orchestrator/src/team_store*`) | ~2,200 | moves as is; no GPUI |
| CLI handlers (`sheprd-gpui/src/control/server/team*`) | ~2,600 | moves to the service's socket, mostly mechanical |
| Bridge: wakes, identity proof, stall (`sheprd-gpui/src/team/bridge*`) | ~2,600 | rewritten against Herdr's API and `hc send` |
| Teams window (`sheprd-gpui/src/team/view*`) | ~3,200 | stays, reading from the service |

Why: most Teams failures so far come from Teams living in the window. Another
app took the control socket after a reboot (#85). Wakes went through a
synthesized paste (#62). Identity is proved from the window's daemon
connection, so restarts and pane moves lose it (#42, #43). The store and wake
queue die with the supervisor's window, and remote members work only while
the window is connected to their host (#8, #40, #41). A service owns all of
that whether or not an app is open.

Precedent: `sheprd-remote-control` is already a daemon with no GPUI that
drives Herdr panes through Herdr's API socket. The Teams service follows it.
Herdr is not modified: the service is a client of herdr-core and Herdr 1, not
an in-process plugin.

### Phases

1. **Service and socket.** A companion binary owns the team store and serves
   the `herding team` requests on its own socket. The Teams window and the CLI
   become its clients. Identity proof and wakes still go through the app for
   now, behind a narrow interface, so this phase changes ownership only.
2. **Identity without the window.** Spike first: is Herdr's `terminal_id`
   (`term_…`, in `herdr pane list`) stable for a session across a GUI restart,
   a pane move, and a daemon restart, and can the caller prove it? Herdr's JSON
   API reports no boot id, which today's proof and #81 rely on. Then move the
   proof into the service.
3. **Delivery.** The wake queue drains in the service: `hc send` for
   herdr-core panes, `herdr pane run` for Herdr 1 panes, both gated on agent
   status so a wake never types into a dialog. Remote members through
   `hc machine` or `herdr --machine`.
4. **UI layer.** The Teams window reads only from the service. Spike 2 (a
   Teams mod in the official app) becomes another client of the same socket.

### Multi-machine

The service removes the window from the path: the store and wake queue
outlive it, and outbound wakes reach remote panes through `hc machine` or
`herdr --machine` whether or not the window is connected to that host. That
covers wakes going out. It does not cover a remote member calling in.

**Inbound calls (with phase 2).** A worker on m1 running `herding team task
complete` must reach the service on the supervisor's host and prove which
member it is. Today that goes through `scripts/control-tunnel.sh`
(`docs/control-tunnel.md`). Options to settle in the phase 2 spike, since
they share the identity proof:

- an SSH tunnel to the service socket, as today, kept up by the service;
- the request carried over `hc` to the supervisor's host;
- a small authenticated network listener on the service.

Also needed:

- Hosts the service can reach. Add machines by LAN name: m1's Tailscale name
  did not answer during spike 1, and m2 is unreachable.
- Herdr 1 panes on remote hosts go through `herdr --machine`, a different
  path from `hc`, untested from a service.
- With one authority, remote workers cannot write to the board while the
  supervisor's host is down. Per-host services that sync would remove that,
  later.

**Gate.** Multi-machine is done when a worker on m1 receives a wake,
completes a task, and posts to the bulletin, all through the service, with
the supervisor's window closed.

### Decisions

- One service on the supervisor's host is the single authority to start
  with; per-host services that sync can follow if latency or partitions need
  them.
- No new fixes to wake delivery inside the window after #81. Bugs there are
  filed against phase 3 instead.
- Herdr 1 and herdr-core both stay supported. Moving the team's panes to
  herdr-core is the user's choice, never automatic.

### Risks

- No stable caller identity in Herdr's API (phase 2 spike decides).
- `hc send` into a modal (status gate required).
- Two daemons, two delivery paths, until panes move to herdr-core.
- Remote members cannot call in without a tunnel or listener (Multi-machine).

## Not yet verified

- Only herdr.dev's pages were read, on 2026-11-12, not the
  `herdrdev/herdr-core` or `herdrdev/herdr-app` source.
- `hc send` into an open dialog or a busy agent, and its behavior beyond
  the local host and m1.
- The official app's UI stack is unknown. Its mods are JavaScript, while Teams
  is Rust (`sheprd-orchestrator`).
