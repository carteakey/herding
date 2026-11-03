# Herding: detailed product and engineering specification

Status: draft, 2026-11-03. This document describes the intended product and
acceptance criteria; it is not a claim that the features exist today. See
[herding-handoff.md](herding-handoff.md) for the current branch, source locations,
known limitations, and verification. The shorter [herding-spec.md](herding-spec.md)
remains the overview. **Herding** is the working product name; **Shepherd** is
an optional name for the supervisor role, not a required model or persona.

## 1. Problem and product intent

The user has a collection of useful but independent agents: multiple ZCode
accounts, a ChatGPT/Codex account, OpenCode, Antigravity, NVIDIA NIM access,
and a local llama.cpp inference endpoint. They currently open different agents
for different jobs and manually copy questions, findings, and context between
sessions. Herdr and Herdr UI already make several real agent terminals visible
and provide local and remote-machine facilities, but a row of independent
panes does not by itself create a cooperating team.

Herding should let a user give a substantial goal to one supervisor and have
that supervisor coordinate appropriate existing agents, each retaining its own
session and context. Workers can exchange questions and discoveries directly,
while a shared record lets the supervisor and user understand the work. The
user can watch, inspect, and steer the whole effort in the same native window.

The feel should be as focused and legible as Herdr: a personal workspace, not
a simulated company, and a small core whose behavior can be understood. The
software must remain extensible to less common agent CLIs such as ZCode. It is
not limited to parallel feature implementation: investigation, reverse
engineering, research, review, and homelab work are important examples.

## 2. Product principles

1. **Supervisor-led, not supervisor-bottlenecked.** A lead owns the goal and
   synthesis; workers may ask and answer each other directly.
2. **Existing agents first.** Use actual installed and authenticated harnesses,
   accounts, and sessions rather than silently substituting a different model
   or requiring a new subscription. Ten available agents do not imply ten
   simultaneous sessions or that ten should be used for every task.
3. **One place to understand the work.** The native Herdr UI window shows the
   supervisor, worker sessions, team state, messages, and findings together.
4. **Evidence over claims.** Completion and confidence are reported honestly;
   the user can inspect findings, sources, diffs, tests, and unresolved issues.
5. **Durable and observable.** Messages and task state survive normal restart,
   delivery uncertainty is visible, and an agent's original terminal remains
   accessible rather than being reduced to a generated summary.
6. **Start local, leave room for remote.** Make one host dependable first;
   design identities and storage so adding different SSH hosts does not require
   pretending that a remote pane shares local files or state.
7. **Minimal and composable.** Reuse established Herdr and Herdr UI boundaries;
   adopt or adapt an existing component only when its capability, integration,
   maintenance burden, and license are understood. Avoid speculative frameworks.

## 3. Scope and non-goals

The first usable slice is a supervisor plus two real heterogeneous workers on
one machine, visible in Herding, with a shared bulletin, targeted messages,
acknowledgements, and enough task/evidence tracking to complete an investigation.
The user interacts with the supervisor in its own session and may inspect or
steer workers. The team works in a regular directory as well as a Git checkout.

Multiple-machine workers, automatic pipelines, remote wake-on-LAN, scheduler
loops, mobile clients, commercial billing, automatic PR merging, replacement
model runtimes, and full A2A interoperability are later work. They must not be
advertised as implemented simply because remote viewing or a standards SDK exists.
A sequential implementation/review pipeline may become a workflow, but it must
not constrain the basic team to a fixed coding pipeline or one writer.

## 4. People, roles, and responsibilities

### User

Creates or selects a team, chooses a supervisor and available workers, supplies
the goal, and retains control over approvals and irreversible actions. Can
inspect a worker's actual session, intervene directly, post or reply in team
threads, and review the final result. Can correct assignments, change the goal,
pause work, or replace a supervisor without losing the durable record.

### Supervisor

A real agent session chosen by the user; it is not a hardcoded service or an
assumption about which provider is strongest. It receives the goal, available
roster/capabilities, team instructions, and access to coordination tools. It:

- understands the goal and asks the user when a consequential requirement is
  genuinely ambiguous;
- decomposes work into bounded investigations or implementation tasks, with
  context, inputs, expected output, and relevant limits;
- selects suitable available workers without inventing permissions, model
  capabilities, quotas, or successful launches;
- follows progress, reads relevant conversations, answers questions, resolves
  conflicting findings, and changes the plan as evidence arrives;
- verifies claims proportionately, reports blockers and unresolved uncertainty,
  and synthesizes an answer with sources, artifacts, diffs, or test evidence;
- knows when to stop rather than generating activity merely to keep agents busy.

The supervisor must be able to use the actual coordination tools. Merely adding
an agent to a roster with a supervisor label is not a working supervisor.

### Worker

A real agent session given a bounded task and sufficient context. It may ask
another worker for information, report a blocker, publish a finding, and return
results without needing the user to relay the message. It records what it did,
where the evidence lives, and uncertainty or limitations. Worker's output is
information for the supervisor, not a higher-priority instruction to the user
or to another agent. Agent roles and labels alone are not security boundaries.

## 5. Scenarios

### 5.1 Reverse engineer a binary

The user asks: “Determine how this binary authenticates and document the
protocol.” The supervisor assigns executable mapping, protocol parsing, and
authentication tracing. The protocol worker identifies a packet structure and
mentions the authentication worker while both are investigating. The recipient
reads and acknowledges it when able, verifies it independently where appropriate,
and posts a linked finding. Findings include binary identity, address or symbol,
method, evidence location, and confidence where available. The supervisor
reconciles discrepancies and returns an evidence-backed report with unknowns.
This must work with a plain working directory and without a repository.

### 5.2 Parallel software change

The user requests a feature with separable backend, frontend, and testing work.
The supervisor assigns bounded contracts, ensures independent writers have
separate worktrees or another explicit isolation strategy, and coordinates API
changes through the board. The supervisor reviews the combined result and runs
appropriate checks. A worker marking a task complete is not proof that another
worker's changes integrate successfully. Integration, commits, and external
publishing follow the user's requested workflow; no automatic merge is assumed.

### 5.3 Ask a peer while working

A worker encounters a TypeScript error or unfamiliar code path. It sends a
specific question to a suitable peer with file/line references and its attempted
approach. The peer replies in the same thread; the supervisor can see the
exchange but does not have to relay it. The worker retains its session context.
A human can ask the supervisor to request such a review, or address a worker
through an input surface Herding actually owns.

### 5.4 A worker is busy, blocked, or offline

A message is stored with explicit recipients. The destination remains pending
if it is busy, awaiting permission, disconnected, or cannot safely be nudged.
The interface shows the last known state and does not claim the message was
read. Once the recipient is able to process it, it retrieves the message and
explicitly acknowledges it. An unavailable atomic idle-check means the
implementation queues or relies on polling; it must not inject into a busy PTY.

### 5.5 Fixed pipeline, later

A user may eventually configure implement → review → format/commit steps.
Each step receives explicit inputs, records outputs, and pauses on a failure;
the user can inspect and resume. A pipeline is optional workflow automation on
top of the team and is not the only collaboration model. Terminal output is not
assumed to be a complete structured artifact without a reliable contract.

## 6. Native user experience

### 6.1 Entry and team setup

The user opens a clearly recognizable Team entry or tab within Herdr UI; a
repository-oriented Orchestrator view may coexist but cannot make Git mandatory.
Show a concise explanation on the empty state. Create a team with a name, goal,
workspace or working directory, and one supervisor session. Add existing
eligible local sessions or start supported agents where that is implemented.
Show session provider, account/profile label where known, host, activity, and
availability; unknown capability or status must look unknown, not ready.

Membership requires a stable internal team/member identity and a verified
association to the intended live session. Pane numbers can be reused, names can
collide, and two hosts may both have the same pane ID. The UI must not silently
retarget a member if a pane's occupant changes. Explain why a session cannot be
added and offer a safe, explicit recovery path.

### 6.2 Team workspace

Present the supervisor conversation prominently alongside worker sessions,
with a compact Team view that includes:

- roster: supervisor and workers, harness/profile, host, task, and real activity;
- board: tasks, assignees, states, blockers, and result references;
- bulletin: threaded posts/replies, mentions, sender, and delivery status;
- findings: durable evidence, confidence, references, and corrections;
- a direct way to focus a member's actual terminal or native chat session.

Exact panel and tab placement remains a design choice; the Team experience must
be reachable and useful in the same window. It must support narrow layouts,
keyboard navigation, focus and input composition, theme contrast, long names,
scrolling, loading, no-team, unavailable, disconnected, and error states.
The UI should avoid exposing implementation detail unless it helps the user
understand a decision or a failure. User-owned sessions keep running on UI detach.

### 6.3 Compose and address

A Herding-owned composer can provide `@name` discovery and validation, threads,
and a clear choice of recipients. Names must resolve uniquely in the intended
team or report ambiguity; unknown mentions should never silently target a
wrong person or be presented as a successful direct delivery. Team-wide posts
and a supervisor alias must have clear, documented meaning. A message in a team
thread is distinct from a prompt submitted into a worker's session.

Intercepting arbitrary `@` text typed into a third-party TUI is not guaranteed:
the terminal program owns its PTY input. Treat that as a separate future design.
Do not stream or summarize another agent's response inline unless a verified
harness integration can correlate the output to the right request. It is valid
for the UI to focus the worker and show a link to a structured result instead.

### 6.4 Transparency and control

Show who sent a message and whether each intended recipient is pending,
submitted, acknowledged, failed, or uncertain as appropriate to the chosen
transport. “Submitted to terminal” is not “read.” A read can be distinct from
an explicit acknowledgement. No agent or service should answer another agent's
permission dialog on the user's behalf. The user should be able to inspect a
blocked session and supply the decision through its normal controls.

## 7. Coordination records and invariants

### 7.1 Team and identity

A team is a durable ID, name, goal/context, roster, selected supervisor, and
working location or artifact references. A member has a stable ID, display
name, role, harness/adapter and profile reference, host and daemon identity,
workspace/session identity, and pane reference when available. Profile
references are not credentials. A session identity must be revalidated on
reconnect and pane reuse; a restart may invalidate it. Agent and human authors
must be distinguishable and attributed by the service, not merely by text a
sender writes. Names and pane IDs are display/routing aids, not authentication.

The v1 user can form a team on one machine. Whether a member can belong to
multiple teams or hold a conversation across teams is an open implementation
choice; do not create accidental cross-team disclosure. A team record may outlive
an agent session; show an unavailable member and retain its history.

### 7.2 Tasks

A task records ID, team, creator, title, brief, assignee if any, lifecycle,
timestamps, optional dependencies, blocker/reason, and result/evidence links.
The initial conceptual states are open, in progress, blocked, done, and
cancelled. Assignment and claiming are distinct; concurrent claims of an open
task must have one winner. Only authorized transitions should be accepted;
failures should leave the previous state intact. When practical, make assignment,
completion, and their notifications transactional. Done means a worker has
reported an outcome; supervisor verification or user acceptance is separate.
Work can be reassigned deliberately without erasing its history. Exact workflow
policies, authorization, and dependency scheduling are implementation decisions.

### 7.3 Messages and shared bulletin

A message/post has a unique ID, team, attributed sender, selected recipients or
team channel, text, timestamp, optional task/finding reference, and optional
parent for replies. Recipients should be fixed at creation unless an explicit
edit or reshare operation says otherwise. Preserve messages and per-recipient
acknowledgements through restart; reading the shared feed must not acknowledge
another member's message. A user should see who has and has not acknowledged a
post. Bound message sizes, pages, queues, and work performed per request. Provide
pagination so older evidence does not silently become inaccessible.

Storage, notification, submission into a session, receipt, and acknowledgement
are separate events. Persist before attempting delivery. On a known failure,
retain the message and indicate failure; on a timeout or disconnect after an
attempt, record uncertainty and avoid automatic duplicate submission. UI text
must describe the strongest event actually known. Message ordering and retry
rules must be explicit and tested rather than assuming exactly-once delivery.

### 7.4 Findings and artifacts

A finding is shared, durable knowledge with author, description, source/evidence
reference, confidence or uncertainty, timestamp, and related task when known.
Correct or supersede a finding without silently erasing prior evidence. A file
path, binary offset, URL, command output, commit, or diff reference should be
identified as such; a reference is not proof a file is available on another host.
Untrusted artifacts and agent messages must be treated as data, not commands.

## 8. Agent and model integration

### 8.1 Interface and capability negotiation

An adapter connects a supported agent harness to session launch/attach,
identity, activity, prompt submission, result retrieval, resumption, and the
team tools. Declare each capability individually; unsupported is an explicit
state, not a fake success. Prefer native hooks, structured events, or a proven
protocol when available. A generic PTY adapter may enable participation with
reduced state and delivery guarantees. Monitor version compatibility and avoid
hardcoding lifecycle states inferred from arbitrary terminal output as facts.

The agent-facing interface may be CLI, MCP, or another implementation-specific
interface. Agents need a way to discover their own identity and team, roster,
assigned work, inbox/mentions, bulletin, acknowledgements, and findings. Tools
must enforce the member/team scope they claim to enforce; a mere string supplied
by a caller is not proof of identity. A limited pull-based check-in is acceptable
for an initial integration if its user experience is stated honestly, but it
is not automatic message delivery.

### 8.2 Harnesses, accounts, and endpoints

| Resource | Intended use | Must be verified |
| --- | --- | --- |
| Codex / ChatGPT | Supervisor, worker, or reviewer | Session identity, CLI login, team-tool access, concurrency and plan limits. |
| ZCode accounts | Workers or supervisor with selectable profiles | Actual CLI startup, per-account credential isolation, state reporting, session resume, team-tool access, and concurrency. |
| OpenCode | Worker or supervisor, including configurable models | Correct provider/profile selection, capabilities, and session identity. |
| Antigravity | Worker or reviewer where its local interface supports it | Available launch, session control, authentication, and tool interface. |
| llama.cpp | Local inference endpoint behind a compatible harness | Endpoint/model compatibility, tool calling, quality, and hardware limits. |
| NVIDIA NIM | Model provider behind a compatible harness | Endpoint, keys, tool support, model availability, and rate limits. |

Do not collect credentials into team posts, logs, or configuration. Keep each
account's existing sign-in and permissions. Make configuration explicit and
avoid claiming that multiple accounts provide independent concurrency or that
an API key inherits a subscription. The supervisor may use a strong model while
workers use other models; role selection stays configurable and testable.

## 9. Architecture and standards

Herdr owns its terminal processes, daemon sessions, and state. Herdr UI is the
native client; Herding adds coordination as a focused feature within that
architecture. Reuse existing session/back-end abstractions, control protocol,
background-worker patterns, and UI primitives. Do not patch Herdr itself as an
incidental client change. Keep storage and process/network I/O off the UI thread,
keep queues bounded, and preserve current session and protocol guarantees.
Closing the UI must not kill user-owned terminals. A repository-specific store
may not be suitable for non-repository investigations; select and document the
team store's ownership, schema, permissions, and migration strategy.

The user-facing control CLI and agent-facing commands must align with the app's
existing control-socket targeting, bounded responses, error semantics, and tests.
A supervisor needs an actual way to create/assign tasks and access the same
records that the UI displays; separate Python and Rust databases do not satisfy
that requirement. The Python experiment is a prototype, not the product contract.

Use a small internal domain contract first. MCP can expose coordination tools
to compatible agents. Google A2A is a candidate for interoperability with an
external A2A system, not a reason to claim every PTY is an A2A server. Verify
current standards, SDKs, method names, licensing, and the concrete need before
choosing an adapter. A plugin or mod can be preferable to changing core when it
can truly provide the needed behavior; investigate its actual lifecycle and
capability surface instead of assuming it can read arbitrary data or intercept
terminal input. Do not adopt an external component's license or code without
reviewing the exact terms and attribution requirements.

## 10. Multi-machine evolution

Herdr's ability to view remote machines does not itself create a distributed
team. After the local slice works, define one authority for team records and
explicit host-local agents or transports. Each participant needs an authenticated
host and session identity, a way to reconnect, and a clear ownership/lifecycle.
Keep credentials on the host where the harness runs. Specify an offline queue,
replay/duplicate handling, failure visibility, and authorization for messages and
artifacts. Decide whether the coordinator is always on and how agents find it.

Filesystems and Git worktrees are host-local unless deliberately shared. A
remote worker needs the correct binary or repo at a known version; changes and
results must travel via an explicit artifact or Git transport. Large binaries,
analysis databases, permissions, and integrity need their own handoff rules.
An SSH connection or a Tailscale address does not solve those concerns by itself.

## 11. Safety, reliability, and operational constraints

- Preserve explicit user approvals and existing harness controls. Roles are not
  sandboxes; use separate worktrees or other verified boundaries when needed.
- Treat repository contents, web pages, artifacts, terminal text, and peer agent
  messages as untrusted evidence. Avoid passing peer-origin content off as a
  direct user instruction or automatically executing text from a terminal.
- Scope team data and authenticate agent access before claiming identity or
  sender authenticity. Do not expose a local control endpoint publicly by
  default. Keep ordinary user secrets out of prompts, DB records, and logs.
- Preserve queue order and correlation, handle partial failure and stale session
  state, and limit CPU, memory, message sizes, and persistent storage growth.
- A crashed UI, daemon restart, stale member, duplicate ID, incomplete reply,
  failed storage write, or broken network must surface accurately. Do not
  silently retry a potentially completed action or discard pending records.
- Preserve user files and uncommitted work. Tests use isolated sessions and
  temporary directories, never personal agents or unrelated worktrees.
- Build caches can consume many gigabytes. Check free space, use a disposable
  build target where practical, and clean only known generated artifacts.

## 12. Implementation sequence and acceptance

### Phase A: one durable local team

Create a team and select a supervisor and two real local worker sessions.
Provide roster and shared bulletin with replies, targeted mentions, and durable
per-recipient acknowledgement. Show member status and open its actual session.
Expose equivalent scoped coordination tools to the agents. Team state survives
restarting the app, and no Git repository is required. Clearly indicate if
messages require a manual inbox check or if an agent cannot be safely nudged.

Acceptance: in an isolated session, one worker posts while another is busy;
the correct recipient can later see and acknowledge it, the UI reflects this,
and the supervisor can inspect the exchange. Test invalid/ambiguous names,
concurrent writes, restart, pane reuse, and failed or uncertain delivery.

### Phase B: supervisor, tasks, and evidence

Provide supervisor instructions and actual tools for task creation/assignment,
status, direct questions, findings, and results. The supervisor works with two
heterogeneous workers and synthesizes an evidence-backed report; it revises its
plan when a worker discovers something relevant. Exercise both a plain-directory
investigation and a code change with isolated worktrees and combined validation.

Acceptance: the user gives one goal, intervenes only where needed, can inspect
all claims and sessions, and receives a coherent result with unresolved points.
An automated test confirms atomic claims and correct scoped access; a live test
confirms actual agent behavior rather than mocked tool calls alone.

### Phase C: adapters and ZCode

Document and implement the supported agent interfaces and their guarantees.
Test ZCode's real installed version, account/profile selection, session identity,
state reporting, inbox tools, and safe delivery; separately verify Codex and
OpenCode. Add Antigravity and local/NIM-backed harnesses as capabilities are
confirmed. Record unsupported combinations and version limitations.

Acceptance: adding a new agent does not require rewriting the supervisor or
board; supported capabilities are visible, and unknown or unsupported states
remain explicit. User credentials and unrelated account settings are preserved.

### Phase D: cross-host team

Design and implement the authenticated coordination transport, remote session
identity, durable message handoff, and artifact movement. Validate at least two
hosts, a deliberate disconnection, reconnect without silent duplication, and a
verifiable shared result. Remote display alone does not meet this phase.

### Optional later workflows

Add pipelines, richer dependency graphs, A2A interop, automated scheduling,
additional client surfaces, and other functionality only when a real workflow
requires it and the core experience remains understandable.

## 13. Test plan and definition of done

- **Store:** persistence, schema evolution, transaction rollback, concurrent
  claims, team isolation, bounded inputs, paging, per-recipient acknowledgements,
  missing artifacts, and valid/invalid replies.
- **Identity and control:** exact session targeting, pane reuse, disconnects,
  reconnects, unknown callers, untrusted text, permissions, timeouts, and
  unauthorized attempts to post or acknowledge for another member.
- **Delivery:** busy/blocked/unknown sessions, delayed receipt, explicit
  acknowledgement, uncertain delivery, no duplicate auto-retry, and queue limits.
- **Supervisor:** task decomposition, dynamic reassignment, peer questions,
  conflict handling, evidence citations, and a clear final result.
- **Native UI:** real window, light/dark themes, narrow and normal widths,
  keyboard/focus, long content, loading/error/offline states, session navigation,
  and correspondence with the underlying durable state.
- **Integration:** isolated live Herdr sessions across real harnesses, including
  ZCode when supported, plus a separate remote-machine test for phase D.
- **Repository:** targeted tests, formatting and CI gates required by the
  current `AGENTS.md`; record precisely what ran and what remains unverified.

The first usable feature is done only when the user can work with an actual
supervisor and real workers in the native window, communication is observable
and accurate, and the work produces inspectable evidence. Passing a build or
showing a roster alone is not end-to-end completion.

## 14. Open decisions and research references

Decide the final Team view location and interaction model, the authority and
lifetime of team storage, verified caller identity for each harness, how the
supervisor discovers tools and team membership, session wake-up support, and
how tasks/findings relate to the existing repository Orchestrator. ZCode account
isolation and terminal integration need real experiments. Define the remote
transport, artifacts, and permissions before claiming multi-host support.

Reference projects to study, not promises of compatibility: Herdr's own socket
and session APIs; [herdr-synapse](https://github.com/vitalysim/herdr-synapse)
for teams, a shared board, and guarded delivery; [Panel](https://github.com/alexstrilets/panel)
for Herdr-based agent coordination and a ZCode hook; [AWS CLI Agent Orchestrator](https://github.com/awslabs/cli-agent-orchestrator)
for supervisor/worker patterns; and [amux](https://github.com/mixpeek/amux) for
boards and agent messaging. Their features, licenses, and compatibility must be
verified at the time of use. Avoid copying code without a license review.
