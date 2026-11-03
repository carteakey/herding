# Herding: agent teams in Herdr UI

Status: draft, 2026-10-31. This describes the intended product, not features
that are already implemented. **Herding** is the working project name;
**Shepherd** is a proposed name for the supervisor role.

For the full product, workflow, integration, and acceptance requirements, see
[herding-detailed-spec.md](herding-detailed-spec.md).

## Purpose

Give one capable agent a goal and let it coordinate a team of existing agents
on that goal. The user sees and steers the work inside Herdr UI, watches each
agent's real session, and can inspect the team's messages, tasks, and evidence.
Agents may message each other directly while the supervisor remains responsible
for planning, following progress, resolving conflicts, and producing a result.

The system should feel like a natural extension of Herdr: focused, local-first,
transparent, and useful with the tools the user already has. It should work for
investigation and research as well as software development. In particular, a
team investigating a binary must not need a Git repository.

## Example

The user asks the supervisor to determine how a binary authenticates and to
document the evidence. The supervisor assigns executable mapping, protocol
analysis, and authentication tracing to three workers. One discovers a packet
structure and sends it to another, who uses it to identify a parser. They each
record evidence and confidence. The supervisor follows the investigation,
reassigns work if needed, checks conflicting claims, and presents a report
with links to the underlying findings. The user can open any worker's session
and add context without copying output between terminals.

## Goals

- A single native window with a supervisor conversation, worker sessions, a
  compact task board, messages, and shared findings.
- A real supervisor: an ordinary supported agent given coordination tools and
  instructions, able to delegate, monitor, communicate, and synthesize.
- Independent agents that retain their own context and can exchange targeted
  questions and findings while work is in progress.
- Use existing CLI sessions and accounts, with configurable adapters for
  Codex, ZCode, OpenCode, Antigravity, and future agents. Model endpoints such
  as llama.cpp and NVIDIA NIM are used through a compatible agent harness.
- A reliable local team first; a path to workers on separate SSH hosts later.
- Inspectable, durable state with honest delivery and task status.

## Out of scope for the first release

Automatic cross-host execution, a public network service, a new LLM runtime,
autonomous account management, a business-oriented organization model, full
A2A interoperability, automatic merging, and a workflow designer. A pipeline
is useful later, but the basic system must support dynamic investigation,
not only a fixed sequence of coding steps.

## User experience

1. Create a team in the current Herdr UI window, name its goal, and select a
   supervisor. A team can use an ordinary directory or a Git checkout.
2. Add available local agent sessions or start configured workers in Herdr
   panes. Pick an agent type and account/profile where the harness supports it.
3. Talk to the supervisor in its own real session. The supervisor proposes or
   creates tasks, assigns workers, and can revise the plan as evidence arrives.
4. Open a compact Team view showing members, current task and activity,
   unread messages, findings, and blockers. Select a member to focus its
   actual terminal. Select a task or finding to see its context and evidence.
5. Send a message to the supervisor or a worker, inspect conversations, and
   add a finding. Workers can send direct messages or team-wide updates.
6. Review the supervisor's result, supporting evidence, unresolved questions,
   and any changes before deciding what to do with them.

The interface should work at narrow sizes and in light and dark themes. It
must not imply that an agent has read a message simply because input was sent.
Show unavailable capabilities rather than pretending an unknown agent is idle.

The existing repository-oriented Orchestrator view is a possible integration
point, but it cannot be the only entry point if it requires a repository.
Choose placement after a UI spike against the current upstream code. A Team
view should coexist with terminal panes, not replace the supervisor's session.

## Coordination model

### Team and members

A team has an ID, goal, members, and one selected supervisor. Each member has
an internal ID, display name, role, harness/adapter, account or profile
reference, host, Herdr session, and live session/pane reference where available.
Internal IDs must not depend on a human-readable name or on pane IDs being
globally unique. The UI distinguishes a disconnected member from a completed
task. Roles are instructions and capabilities, not security boundaries.

### Tasks

A task has an ID, title, brief, creator, optional assignee, status, timestamps,
and result/evidence references. The initial states are open, in progress,
blocked, done, and cancelled. An assignment and a claim are distinct; claiming
must be atomic so two workers cannot own the same task. Record who changed a
task and why it is blocked. Done means a worker submitted a result, not that
the supervisor verified it. Dependencies and richer workflow rules can follow
after the basic task lifecycle is reliable.

### Messages

Messages have an ID, team, sender, recipient or team channel, body, timestamp,
and optional task/finding reference. Support direct messages, replies, and
team-wide updates. Keep messages until explicitly acknowledged by each intended
recipient; reading and acknowledgement are distinct operations. Make clear
whether a message is queued, submitted to a session, acknowledged, failed, or
of uncertain delivery. An agent-origin message is attributed to its sender
and must not be presented as a human instruction. A supervisor can see the
team's relevant communications without being the relay for every exchange.

### Findings and artifacts

Findings are durable team knowledge, separate from transient chat. A finding
has an author, description, evidence or artifact reference, confidence, and
optional task link. Support correction or supersession without silently
rewriting the history of a claim. For a binary, record its identity or hash,
address or symbol, and analysis method where available. Artifacts may be files
or links; access to a reference does not imply the file exists on every host.

### Supervisor behavior

The supervisor receives the user's goal, team roster, capabilities, and access
to the coordination interface. It decomposes work, assigns bounded tasks,
checks messages and blockers, updates the plan, and synthesizes findings. It
should prefer a small useful team and ask the user when a decision requires
their judgment. The user may steer or replace it. Its instructions must make
clear that worker claims and external content are evidence to evaluate, not
instructions from the user. No automatic claim of correctness follows merely
from a worker reporting completion.

## Agent integration

Use a narrow adapter contract with explicit, discoverable capabilities:

- Start or attach to an agent using its normal CLI and selected account/profile.
- Identify the session and report whether it is working, ready, blocked,
  disconnected, or unknown when the harness and Herdr can provide that state.
- Submit a prompt or wake-up only when supported, and surface the actual
  outcome; support reading or linking results without treating terminal output
  as a guaranteed structured response.
- Resume a session when the harness supports it, without claiming that a
  restarted process recovered its previous context unless verified.
- Expose the team tools to the agent through a CLI, MCP, or another supported
  interface. A generic terminal participant may have fewer capabilities.

ZCode is a first-class acceptance target, including multiple account profiles,
but its exact authentication and per-account isolation must be verified rather
than assumed. Do not store credentials in team records or messages. Test
provider subscription concurrency, CLI automation, and model tool-calling
behavior with the actual configured services. Separate agent launch from
model selection: a raw inference endpoint is not itself a coding agent.

## Architecture and delivery

Herdr continues to own terminal processes and session state. Herdr UI renders
the interface and hosts or connects to a coordination component responsible
for durable team records. The existing backend, session, and service abstractions
should be reused where they fit; repository-specific orchestrator state is not
automatically a suitable store for non-repository teams. Choose the concrete
storage location and service lifecycle during implementation. Use bounded
queues and background I/O; never block GPUI rendering on SQLite, processes,
network calls, or waits. Closing the UI must not terminate user-owned sessions.

The initial local coordination API should support registration, roster,
tasks, inboxes, acknowledgement, and findings. The supervisor and workers use
that same API. Make UI-owned capabilities available through the app's control
CLI in line with the repository's `AGENTS.md`, including explicit target
selection, bounded responses, documentation, and tests.

Delivery needs a precise boundary. Persist first; only mark submitted after a
confirmed write to the intended session, and acknowledged only when the
recipient explicitly confirms processing. If the sender cannot establish
whether a submission happened, record uncertainty and avoid blind retries.
Do not submit to a busy or blocked agent based on a stale status snapshot;
use a race-safe idle operation where available, or leave the message queued
and let the agent poll. At first, manual prompting and inbox checks are an
acceptable fallback. Never auto-answer an agent's permission prompt.

`@agent` is a desired convenience in a dedicated Herding composer, where
the application owns the input. Intercepting keystrokes inside an arbitrary
existing agent PTY is a separate design problem and is not assumed safe or
feasible. Agent responses remain in their own sessions, with an explicit link
and optional structured result in the Team view; do not promise inline output
from raw terminal scraping.

## Remote evolution

First make a team reliable on one host. Later, add a coordinator reachable by
host-local workers with explicit host and session identities, authentication,
reconnect behavior, offline queues, and no automatic duplicate delivery. SSH or
Tailscale may provide connectivity; neither alone shares the task state,
filesystem, or artifacts. Define how source commits, uncommitted changes,
binaries, and findings move between machines. Keep provider credentials on the
host where the corresponding agent runs. Remote observation of one server is
not equivalent to workers distributed across several servers.

## Protocol strategy

Start with a small internal coordination contract and an agent-facing CLI/MCP
interface. Keep transport details outside the domain model. Investigate an A2A
bridge only once an actual interoperability use case and current specification
have been verified; a terminal pane is not automatically an A2A server.
MCP provides tools to an agent; an A2A adapter could provide inter-system task
exchange. Do not assume a particular A2A version, method name, Rust SDK, or
native compatibility without checking the relevant implementation.

## Milestones and acceptance

### 1. Local coordination foundation

Create or select a team and supervisor, register two workers, assign distinct
tasks, exchange direct messages, and store findings. Claiming is atomic; a
message survives process restart and remains pending until acknowledged. The
team works in a directory without Git. Failures and unknown states are visible.

### 2. Native UI and real sessions

Show the roster, board, inbox and findings in Herdr UI; focus a selected worker
terminal. A user can address the supervisor and inspect the result inside the
window. Run a real supervisor with at least two different CLI harnesses in an
isolated session. Demonstrate a peer message sent while a recipient is busy,
and verify it is processed exactly as reported; if safe wake-up is unavailable,
show it as queued until an explicit inbox check. Include a ZCode compatibility
spike and document verified versus unsupported behavior.

### 3. Practical workflows

Run the binary investigation end to end, with linked evidence and a supervisor
report. For code changes, demonstrate separate worktrees, review, and explicit
integration. Add optional pipeline execution only after it can represent
failure, cancellation, and evidence without bypassing the team model.

### 4. Multiple machines

Run a supervisor and workers on at least two hosts, survive a temporary
disconnect without losing or silently duplicating messages, and demonstrate
artifact transfer and host-specific credentials. This milestone requires a
separate transport and security design before implementation.

## Evaluation and open decisions

Test whether the supervisor can dynamically redirect an investigation, whether
peer communication arrives without the user acting as messenger, and whether
the final evidence is actually useful. Compare time and user interventions
against a single-agent attempt rather than assuming more agents are better.
Exercise concurrent claims, acknowledgement, restart, blocked sessions, stalled
delivery, and disconnects. Verify the actual native interface, including narrow
layouts, focus, and themes. Do not use personal sessions for destructive tests.

Open decisions: exact Team view placement; how agents securely identify
themselves to the local coordination API; storage and schema ownership; how
the UI and worker sessions discover their team; generic CLI capabilities and
account isolation; whether any upstream-compatible plugin or mod surface can
carry the coordination view; and the remote transport. The experimental
`experiments/team` CLI demonstrates a subset of tasks, inboxes, findings, and bulletin posts: it is
not the product, a tested Herdr integration, or a finished supervisor.
