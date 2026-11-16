# Herding Team: review and cross-harness direction

Reviewed 2026-11-10 at `3c60ab604`, against upstream merge-base `389a551`.
This review covers the fork-only team layer (about 25k of the 28k lines the
fork adds) and asks one question: is this the right mechanism for agents on
different platforms and different harnesses to work together?

## Verdict

**The records layer is right. The delivery layer is wrong as the primary
path, and it runs in the wrong process.**

Keep the durable store, the `herding team` CLI, and the pull contract.
Demote "type the wake into the terminal" to the fallback of a set of delivery
adapters, and move the store and wake queue out of the GUI into a companion
daemon. Almost every open team bug sits in delivery or follows from the GUI
owning the state.

## What the layer is today

| Part | Where | What it does |
| --- | --- | --- |
| Store | `crates/sheprd-orchestrator/src/team_store*` | SQLite (WAL) of teams, members, posts, recipients, acks, tasks, findings |
| Worker | `crates/sheprd-gpui/src/control/server/team_worker*` | One thread owns the store and serves requests in order |
| Control protocol and CLI | `sheprd-control-protocol/src/protocol/team*`, `cli/control/team*` | `herding team …` over the app's control socket, `--json` everywhere |
| Identity | `control/server/team/identity.rs` | Proves a caller from the pane snapshot, then pins the agent process on Linux |
| Bridge and wakes | `crates/sheprd-gpui/src/team/bridge*`, `agent_notes.rs` | Builds prompts, holds them until the write lands and the agent is idle, pastes them into the pane |
| View | `crates/sheprd-gpui/src/team/view*` | Board, bulletin, roster, findings |
| Skill | `skills/herdr-team/SKILL.md` | Tells every agent to check the records, ack, claim, block, and complete |

The store, worker, bridge, and wake queue all live in the GUI process on the
supervisor's machine. Remote members reach it through the control tunnel
(`docs/control-tunnel.md`).

## What works, primitive by primitive

Solid, and worth building on:

| Primitive | Why it holds |
| --- | --- |
| Task claim | Conditional `UPDATE … WHERE status = 'open' AND (assignee IS NULL OR assignee = me)` plus a `changed == 1` check: concurrent claims have one winner |
| Task complete | Assignee (while in progress) or supervisor only, checked in the same transaction |
| Write-then-wake | `hold_prompts_for_action` releases a prompt only after the store confirms its write, so an agent is never told to claim a row that isn't there |
| Store hygiene | WAL, 5s busy timeout, cached statements, bounded fields, every change in a transaction |
| Bulletin paging and acks | Sequence cursors, newest page by default (#14), addressed vs acknowledged tracked separately, no cursor crosses teams |
| Short ids | Unique prefixes for tasks, posts, findings; ambiguous prefixes are refused with candidates (#26) |
| Re-register after resume | A resumed session reclaims its own name (#11) |
| Linux process pinning | Registered agent process or any descendant, start times defeat pid reuse; verified live (#3) |
| Completion wake to supervisor | Marked seen only after real delivery (#6, #13) |

Works, with caveats:

| Primitive | Caveat |
| --- | --- |
| Wake on assign or mention | Breaks when the pane is gone (#35) or two hosts share a pane id (#40) |
| Hold while busy | Right idea; the retry has no backoff (#35) |
| Pause per team or member | Only honoured for the team on screen (#41) |
| Heartbeat | Crashed vs healthy only; no running, waiting, or done (#23); can say healthy while wakes can't land (#35) |
| Caller identity | Strong for ACP chats and CLI bindings; weak for snapshot-proved agents (#43) and after a restart (#42) |
| Remote members | Only while the UI is connected to that host's daemon (#8) |
| Block and reassign | Reassign returns work to the board correctly; block has no permission check (#44) |

Not there yet: several teams at once (#41, #32), liveness and claim TTL (#23),
one-step landing (#22), priority and WIP limits (#27), budgets (#29), and issue
sync beyond a one-shot import (#2).

The pattern: the **board and bulletin are trustworthy**. The weak layer is
getting the right prompt to the right live agent on the right machine.

## Three layers, judged separately

### 1. Records: right, keep it

A shared store that any agent reaches with a shell command is the lowest
common denominator across harnesses. Claude Code, Codex, ZCode, and OpenCode
need no SDK, only the CLI and the skill. The skill already states the correct
contract: a notification "can be delayed, duplicated, or incomplete", so agents
check the records. That contract is what makes cross-harness work possible at
all, and the store implements it carefully.

### 2. Delivery: right as a fallback, wrong as the main path

Pasting a prompt into a terminal depends on things no single component owns:

- **Idle detection per harness.** The daemon's `AgentStatus` comes from
  per-harness heuristics. A wrong `Idle` pastes into a mid-turn agent; a wrong
  `Working` holds forever.
- **TUI state.** A permission dialog, a modal, or a half-typed user prompt
  receives the paste instead of the agent.
- **Pane ids as addresses.** They're per-daemon and reused, so routing and
  identity both drift (#40, #43).
- **No receipt.** A successful paste proves the text was typed, not that the
  agent read it. Only the ack in the store proves that.

Issues #35, #40, #41, and #43 are all delivery problems. More fixes inside the
paste path make it less broken, but its ceiling stays low.

### 3. Where it runs: the GUI is the wrong owner

`AGENTS.md` sets the architecture: the daemons own terminal processes and
session state, and closing or detaching the GUI must leave them running. The
team layer breaks that rule:

- A GUI restart, update, or OOM kill (#12) stops all coordination. Queued
  wakes and process bindings are in memory and are lost.
- Delivery is coupled to the view: only the selected team drains (#41), and
  the drain waits on the *selected* endpoint's connection.
- #42 exists because the bindings live in GUI memory.
- Multi-machine teams need the GUI connected to every member's host (#8).

The repo already has the shape this needs. `sheprd-automate` is a daemon with
SQLite and a CLI, and `sheprd-remote-control` is a daemon that talks to Herdr's
API socket. Herdr itself is off-limits, so the store can't move into
herdr-core; a companion daemon is the remaining option.

## Compared with Claude's own subagents

Claude Code's subagents and agent teams
([docs](https://code.claude.com/docs/en/agent-teams), also listed in
`docs/herding-references.md`) solve a narrower problem: one harness, one host,
a lead that spawns its teammates.

- **Borrow the delivery model.** Messages and results arrive there because the
  harness itself injects them between turns as structured events. Nothing types
  into a terminal and nothing guesses idleness. Cross-harness adapters have to
  recreate that one harness at a time.
- **Don't copy the topology.** Subagents are spawned by their parent, live on
  one host, and don't outlive the session; the docs label agent teams
  experimental. Herding's board is peer-to-peer, durable, and spans harnesses
  and machines. That is the actual product, and it's the harder problem.

In short: Claude's model gets delivery right by owning the harness; Herding
gets state right by owning nothing. The direction below keeps Herding's state
model and approaches Claude's delivery quality through per-harness adapters.

## Field notes: the supervisor pattern that works

An external report (2026-11-06) on what works in practice with a strong model
supervising cheap workers:

- long-running sessions per project;
- the strong model (Opus) does all work through cheaper subagents (DeepSeek
  Flash);
- strict scope when a task is created;
- proof of work instead of a summary;
- a one-step review of the diff to catch work that is "totally off".

That setup runs inside one harness, so delivery comes free. Herding's job is
the same pattern when the workers are long-lived, visible, on other harnesses
or machines, and possibly on other models. How Herding maps today:

| Practice | Herding today | Gap |
| --- | --- | --- |
| Long-running session per project | A durable team per repo, supervisor in its own pane | Survives agent restarts, not GUI restarts (see "Where it runs") |
| Strong supervisor, cheap workers | Any harness or model can be a member, so this works across vendors | Roster doesn't show each member's model or cost; no per-task budget (#29) |
| Strict scope at task creation | `--brief` is free text | No scope fields: files or paths allowed, acceptance checks, out of scope. Worktree per task is #24 |
| Proof of work, not summary | `task complete --result` is free text; findings carry evidence | Results should be structured and checked: branch, commit sha, the exact commands run with exit codes, test counts. The app can verify the sha exists on the branch (#28) |
| One-step diff review | The app has a changes review view; supervisor lands by fast-forward | Not wired to tasks: completing a task should open its diff for the supervisor, and `land` (#22) should require that review |

Two lessons for this plan:

- **Cheap workers raise the value of proof.** A weaker worker writes confident
  summaries of work it didn't do. Structured, verified results (#28) protect
  the supervisor's context and tokens more than anything else on the backlog.
- **Scope and proof are records, not delivery.** They belong in the store and
  the CLI, the layer this review says to keep, so they can ship before the
  delivery work.

## Recommended direction

### Delivery adapters, in tiers

A wake is a **doorbell**: "check team X, you have N items". The content always
lives in the store. Each member records which adapter reaches it, chosen at
registration from what the harness supports:

1. **Native channel.** ACP chats already have it: `dispatch_team_prompt` sends
   `Command::Prompt` to an attached chat. It needs no idle guess and reports
   failure.
2. **Harness integration at turn boundaries.** The harness checks the inbox
   when a turn ends or a prompt is submitted, through its own extension point.
   ZCode is the obvious first target: `carteakey/zcode-cli` already ships
   `integrations/herdr` for state reporting (`docs/herding-zcode.md`). For
   Claude Code and Codex, use their documented hook or notify mechanisms;
   check their current docs before designing against them.
3. **Terminal paste.** Today's path, kept for harnesses with nothing else,
   routed by host (#40), with backoff and an unreachable state (#35).

Inside the app this is one boundary: a delivery enum per member (closed set,
matching the repo's "enums for closed alternatives" rule), not a trait per
harness.

### Store and wake queue in a companion daemon

A `sheprd-team` daemon (name to taste) on the supervisor's host owns the
SQLite store, the wake queue, and process bindings. The GUI and `herding team`
become clients, and remote members connect through the existing tunnel. This:

- removes #41 by construction, because no view state decides what drains;
- makes #42's bindings persistent without a special case;
- survives GUI restarts and #12;
- gives multi-machine teams one authority that isn't a window.

This is the concrete need `AGENTS.md` asks for before adding a crate or
service. It is also the largest change here, so it comes after the delivery
fixes, which pay off immediately either way.

### Identity supplied by the adapter

The pane is a routing hint, not an identity. Where an adapter can supply a real
session id (ACP session, CLI binding, a harness hook's session id), the member
is bound to it. Where it can't, bind to the agent process (`pid` +
`started_ticks`) and treat a change as "departed, re-register" (#43).

## Cross-platform guarantees today

What actually binds a caller to a member:

| Platform | Binding | Same-user process naming a member's pane |
| --- | --- | --- |
| Linux | Pane snapshot, then agent process or descendant, by start time | Refused, except right after a GUI restart (#42) |
| macOS | Pane snapshot, then uid only (`getpeereid`) | Accepted |
| Windows | Pane snapshot, then the pipe's DACL (one user) | Accepted |
| Any, through the control tunnel | Pane snapshot only (uid 0 at sshd) | Any root process on the UI host is trusted |

These are reasonable same-user trust models, but `docs/cli.md` should state
them per platform rather than describing the Linux behaviour.

## Plan

1. **#35 + #40:** delivery that tells busy from gone and routes by host. Same
   code, and the biggest reliability win now. Then remove the workaround
   section from `skills/herdr-team/SKILL.md`.
2. **#44, then structured scope and proof (#28):** consistent task
   permissions, then scope fields on `task create` and verified evidence on
   `task complete`, with completion opening the diff for review. These are
   store and CLI changes, independent of delivery, and they make cheap workers
   safe to use.
3. **Delivery boundary:** the per-member delivery enum, with ACP and paste as
   the first two variants, and a ZCode turn-boundary adapter through
   `zcode-cli`.
4. **Companion daemon:** move the store, queue, and bindings. #41 and #42 are
   resolved by this move; fixing them inside the view first is throwaway work.
   Split `team/bridge.rs` and `team_worker.rs` (#39) as part of it.
5. **#43:** adapter-supplied identity once adapters exist.
6. **Then the coordination backlog (#30):** liveness (#23), `land` (#22),
   priority and WIP limits (#27), on a delivery layer that holds up.

## Open questions

- Should the companion daemon run on every host with members, syncing to one
  authority, or only on the supervisor's host with remote members tunnelling
  in? The second is simpler and matches today's tunnel.
- Should a member be able to belong to several teams?
  `docs/herding-detailed-spec.md` §7.1 leaves it open, and the store allows it.
- Which harnesses expose a turn-boundary hook that can run a command and inject
  its output? This decides how far tier 2 reaches.

## Not verified

- Findings come from reading code. No test suites ran for this review; only
  `scripts/check-file-size.sh` ran (three files over the limit, #39).
- How often pane ids collide across real fleet hosts was not measured; #40's
  example ids come from #35's evidence.
- The comparison with Claude's agent teams rests on its public docs, not on
  testing it alongside Herding.
- Harness hook capabilities for tier 2 were not checked against current
  Claude Code, Codex, or ZCode releases.
