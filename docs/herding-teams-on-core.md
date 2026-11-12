# Teams on herdr-core: direction and spikes

Status: proposal, 2026-11-12. Nothing here is decided until the spikes below
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

## Not yet verified

- Only herdr.dev's pages were read, on 2026-11-12, not the
  `herdrdev/herdr-core` or `herdrdev/herdr-app` source.
- `hc send`'s delivery semantics, idle-agent submission, and cross-machine
  behavior are unknown until spike 1.
- The official app's UI stack is unknown. Its mods are JavaScript, while Teams
  is Rust (`sheprd-orchestrator`).
