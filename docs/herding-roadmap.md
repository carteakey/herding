# Herding: Architecture Review & Implementation Roadmap

Status: 2026-11-03. This document captures the review of [`herding-spec.md`](herding-spec.md),
[`herding-detailed-spec.md`](herding-detailed-spec.md), and [`herding-handoff.md`](herding-handoff.md)
against the native Rust implementation on `codex/herding-bulletin`, and defines the implementation
plan for completing the full feature.

## Product direction: make the core dependable

Teams, tasks, a bulletin, and supervisor delegation form a coherent starting
point. Before adding more roles or automation, prioritize the reliability and
clarity of those existing building blocks:

- Let agents finish work already in progress when coordination is paused.
  Make the pause state and the behavior of queued work visible and explicit.
- Deliver messages and assignments to the intended team and member, preserving
  pending work and reporting uncertain delivery honestly.
- Let users switch teams while existing sessions continue in the background,
  without releasing another team's held prompts or changing focus unexpectedly.
- Keep supervision useful and bounded: continue clearly authorized work, and
  become idle when nothing actionable remains or a human decision is needed.

These are desired guarantees, not a claim that every path already meets them.
The existing pause and delivery implementation needs further verification,
including team switching, multiple windows, restart, and real-agent behavior.

## 1. Baseline Implementation Summary

- **Durable SQLite Team Store (`crates/sheprd-orchestrator/src/team_store.rs`)**:
  - Stores `teams`, `members`, `posts`, `mentions`, and `acknowledgements` in `<state_dir>/team/team.sqlite3` with `0600` permissions.
  - Enforces immutable member session targets (`MemberTargetChanged`), input bounds, human vs. member attribution, and idempotent per-recipient acknowledgements.
- **Background Worker & Control Socket (`crates/sheprd-gpui/src/control/server/team*.rs`)**:
  - Non-blocking serialized worker (`herdr-team-store`) with bounded queue capacity (`16`).
  - Control protocol (`crates/sheprd-control-protocol/src/protocol/team.rs`) and CLI (`crates/sheprd-gpui/src/cli/control/team.rs`).
- **Native GPUI View (`crates/sheprd-gpui/src/team/`)**:
  - Activity bar entry (`ActivityPanel::Team`), team creation, roster display, worker addition, supervisor promotion, pane focus, and bulletin posts/replies.

## 2. Phase A Gaps & Bugs Addressed

1. **Terminal / Non-ACP Agent Sessions (`team_candidates` & `current_identity`)**:
   - Allow both native `herdr-acp` chat panes and standard Herdr daemon terminal panes (such as ZCode, Codex, OpenCode, Antigravity) to join teams and authenticate over the control socket using verified daemon boot ID, workspace ID, pane ID, and session/process identity.
2. **Mention Validation & Aliases (`@name`, `@supervisor`, `@team`)**:
   - Validate explicit `@mentions` in the composer and CLI so unknown or ambiguous names return an explicit error rather than silently broadcasting to all members.
   - Support `@supervisor` and `@team` aliases and reject duplicate member display names within a team.
3. **Unified Shared Worker & SQLite WAL Mode**:
   - Enable `PRAGMA journal_mode = WAL;` in `TeamStore::open` and share the worker across the control socket and UI `Service` so CLI mutations immediately reflect in the UI without SQLite lock contention.
4. **Team Discovery (`team status` / `team register`) & Goal Metadata**:
   - Support `herding team status` (discovering the caller pane's active team, member identity, goal, pending mentions, and assigned tasks without requiring a known `TEAM_ID`) and `herding team register`.
   - Persist team `goal` and optional `working_dir` (supporting plain directories without requiring Git), plus member `agent` harness kind and `role`.
5. **UI Team Switching & Navigation**:
   - Allow switching or creating teams even when a team is already selected, and allow setting the team goal and supervisor display name.

## 3. Full Feature Implementation Plan (Phases B & C)

### 3.1 Task Board & Atomic Task Lifecycle (`TeamStore` + Protocol + CLI + UI)
- **Store**: `tasks` table (`task_id`, `team_id`, `creator_kind`, `creator_id`, `title`, `brief`, `assignee_id`, `status` [`open`, `in_progress`, `blocked`, `done`, `cancelled`], `blocker_reason`, `result`, `created_at`, `updated_at`).
- **Invariants**:
  - Distinct assignment (`assign_task`) vs. atomic worker claim (`claim_task` where `status = 'open'` and assignee is unset or matches caller).
  - Record blocker reasons and completion results/evidence.
- **CLI & Protocol**:
  - `herding team task list TEAM_ID`
  - `herding team task create TEAM_ID --title TITLE [--brief BRIEF] [--assign MEMBER_ID]`
  - `herding team task assign TEAM_ID TASK_ID --to MEMBER_ID`
  - `herding team task claim TEAM_ID TASK_ID`
  - `herding team task block TEAM_ID TASK_ID --reason REASON`
  - `herding team task complete TEAM_ID TASK_ID --result RESULT`
  - `herding team task cancel TEAM_ID TASK_ID`
- **UI**:
  - Render the compact Task Board inside `TeamView` with task creation, status badges, assignee labels, blocker details, and completion results.

### 3.2 Findings & Evidence (`TeamStore` + Protocol + CLI + UI)
- **Store**: `findings` table (`finding_id`, `team_id`, `author_kind`, `author_id`, `task_id`, `description`, `evidence`, `confidence` [`low`, `medium`, `high`], `superseded_by`, `created_at`).
- **Invariants**:
  - Findings are append-only durable knowledge; corrections link via `superseded_by` without erasing prior claims.
- **CLI & Protocol**:
  - `herding team finding list TEAM_ID`
  - `herding team finding add TEAM_ID --confidence low|medium|high --evidence EVIDENCE [--task TASK_ID] [--supersedes FINDING_ID] DESCRIPTION...`
- **UI**:
  - Render Findings & Evidence in `TeamView` with confidence badges, evidence references, task links, and supersession indicators, plus a UI form to record findings.

### 3.3 Delivery States & Safe Session Notification
- Track recipient delivery states (`Pending`, `Submitted`, `Acknowledged`, `Failed`, `Uncertain`).
- Surface whether a member session is live, busy, or unavailable from the daemon snapshot without injecting raw keystrokes into busy or permission-blocked PTYs.

### 3.4 Agent Skill & Supervisor Bootstrap
- Update the managed skill documentation (`docs/cli.md` and `skills/herdr-gpui-browser/SKILL.md`) so supervisor and worker agents know how to discover their team (`herding team status`), manage tasks (`team task ...`), exchange bulletin updates (`team bulletin ...`), and record evidence (`team finding ...`).

---

## 4. Phase D: Dual-Sidebar Workspace, Live Control Loop & QoL Upgrades (Completed)

1. **Dual-Sidebar Layout (Left Team Board + Center Terminals + Right Bulletin Sidebar)**:
   - **Left Sidebar (`ActivityPanel::Team`, ~380px)**: Renders Team Header, Live Status Banner (`X working · Y idle · Z active tasks · W pending` + `Last action:` ticker), Goal card, Agents roster, Task Board, and Findings beside live terminal/chat panes.
   - **Right Sidebar (`HerdrWindow::with_team_bulletin_panel`, ~340px)**: Dedicated `team-bulletin-sidebar` with post/pending badges, per-recipient delivery status pills (`pending` / `acknowledged`), `@team` / `@supervisor` / `@member` quick-insert mention chips, reply banner, and sticky bottom composer.
2. **Full Inline Rename, Edit, and Delete**:
   - Rename or delete teams (`TeamStore::delete_team` cascading cleanup).
   - Rename workers/supervisors inline (enforcing unique display names per team) and remove workers (`TeamStore::remove_member` unassigning open tasks).
   - Edit Team Goal and Task title/brief/assignee (`TeamStore::update_task`) inline.
3. **1-Click Session & Pane Spawning**:
   - Quick-spawn bar (`+ Claude`, `+ Codex`, `+ Gemini`, `+ OpenCode`, `+ Split pane`) inside **Start a team** and **+ Add worker** so users can launch new ACP agent tabs or split terminal panes directly from the Team sidebar.
4. **Pause / Resume with Queued Prompt Holding & Flushing**:
   - Team-level (`TogglePauseTeam`) and per-agent (`TogglePauseMember`) pause controls.
   - The UI bridge holds prompts it recognizes as targeting paused panes in `Bridge::queued_prompts` and drains eligible prompts on **Resume**. This is an in-memory, window-local mechanism; it is not yet a verified guarantee across all delivery paths, windows, or restarts.
5. **Local SQLite Task Engine + GitHub Issue (`#123`) Import**:
   - `Operation::ImportGithubIssue` fetches issue metadata via `gh issue view` off the UI thread and creates a structured task or sets the Team Goal, keeping sub-millisecond atomic task claims local in SQLite.
6. **Autonomous CLI-to-Pane Wakeups & Exact Binary/Pane Routing**:
   - Every injected prompt includes `<current_exe> --pane <pane_id> --workspace <workspace_id> team ...` (`team_cli_prefix`) so both ACP chat sessions (`herdr-acp`) and terminal CLI sessions always invoke the active binary with verified pane/workspace identity.
   - When a Supervisor or Worker creates/assigns a task or posts a `@mention` via CLI (`notify_team_mutation`), Herdr automatically wakes the assigned or mentioned agent's pane.

---

## 5. Phase E: Predefined Agent Roles & Role-Driven Herding (Next Evolution)

Moving beyond a generic `Supervisor` + `Worker` task board, Herding should treat **Roles as first-class coordination contracts**:

### 5.1 Strict Supervisor Discipline ("Supervisor Strictly Supervises")
- **Rule**: The **Supervisor** must *never* become a bottleneck by writing implementation code or getting lost in deep file edits itself.
- **Responsibilities**:
  1. Decompose the Team Goal (or imported GitHub Issue) into scoped, verifiable tasks.
  2. Match and assign tasks to specialized teammates (`--assign <member>`).
  3. Monitor the Bulletin and Findings feed, unblock stuck workers, and request verification/reviews.
  4. Synthesize findings and declare the goal complete only after Reviewer/QA verification passes.

### 5.2 Predefined Specialized Agent Roles
Instead of generic "Worker" slots, adding or spawning a teammate should let the user (or Supervisor) assign a **Predefined Role Preset** with tailored kickoff instructions, bulletin expectations, and workflow handoffs:

| Role | Primary Responsibility | Output Artifact |
| :--- | :--- | :--- |
| **Supervisor** | Decomposes goals, assigns tasks, unblocks teammates, synthesizes results; does **not** write implementation code directly | Tasks, Assignments, Goal Synthesis |
| **Architect / Planner** | Explores codebase architecture, identifies edge cases, proposes interfaces, and records design findings | High-confidence Architecture Findings & Sub-tasks |
| **Implementer** | Claims assigned implementation tasks, writes code + unit tests in its pane/worktree, and reports completion results | Code changes + Task completion result |
| **Reviewer / QA** | Reviews diffs, runs test suites/linters, verifies claims against evidence, and either approves or files blocker findings | Verification Findings (`high` confidence) or Blocker reports |
| **Researcher / Explorer** | Investigates external docs, logs, or reproduction steps without modifying source files | Evidence-backed Findings (`--evidence`) |
| **Debugger / Triager** | Reproduces bugs, isolates root causes, and posts minimal reproduction + fix proposals | Root-cause Finding + Fix Task |

### 5.3 Role-Driven Workflow Pipelines
- **Automatic Role Handoffs**:
  - When an **Implementer** completes a task (`team task complete`), Herdr can automatically notify the **Reviewer / QA** role (or **Supervisor**) to verify the result before closing out the parent goal.
  - When a **Researcher** or **Debugger** records a high-confidence finding (`team finding add`), the **Supervisor** is notified to turn that finding into an actionable implementation task.
- **Schema & UI Integration**:
  - Store `role_preset` (`supervisor`, `architect`, `implementer`, `reviewer`, `researcher`, `debugger`) on `team_members`.
  - Display the role badge on each Agent Card and allow changing a member's role preset inline.
  - Inject role-specific system instructions whenever an agent is spawned, added, or promoted.

### 5.4 Flexible Workspace Isolation: Shared Working Tree vs. Opt-In Git Worktrees
Workers may or may not want to use git worktrees depending on the task and role:
- **Shared Working Tree (Fast Iteration & Shared Build Cache)**:
  - Zero setup overhead and shared Cargo `target/` cache.
  - Ideal for `Supervisor`, `Researcher`, `Reviewer`, or `Implementer` agents working on disjoint files/docs (e.g., `README.md` vs. `team_store`), or small sequential fixes where branch merging would add unnecessary friction.
- **Opt-In Git Worktrees (Isolated Compilation & Parallel Branches)**:
  - Prevents mid-edit compile errors in one worker's crate (e.g. an in-progress UI edit in `sheprd-gpui`) from blocking another worker's `cargo test` run in the same repository.
  - Ideal for larger parallel feature work (like `worker bee`'s `zcode` CLI integration or multi-file refactors).
- **Herdr Support**:
  - Keep worktrees strictly **optional** (per-worker or per-task opt-in via UI spawn toggle or agent choice) rather than forced.
  - Because `TeamStore` lives in the app's central SQLite database and routes via `HERDR_PANE_ID` / `HERDR_WORKSPACE_ID`, agents in both the main working tree and any secondary `git worktree` coordinate seamlessly on the same Bulletin, Task Board, and Findings feed.

### 5.5 Agent "PTO": Quota Exhaustion, Rate Limits & Supervisor Succession
In real-world multi-agent runs, any agent — **including the Supervisor** — can suddenly "take PTO" (hit provider usage caps, 429 rate limits, context exhaustion, or session crashes):

1. **Detection (Quota / Rate-Limit / Lease Expiry)**:
   - **ACP & Terminal Signals**: Detect provider quota/rate-limit errors from ACP streams or terminal output (e.g. Claude/Codex/Gemini usage limit banners) and transition the member's state to **`OutOfOffice` (`QuotaExhausted`)** with an optional reset timestamp.
   - **Task Lease & Heartbeat**: Track `claimed_at` / `last_activity_at` on `[in_progress]` tasks; if a pane exits or errors out while holding an active task, flag the task as stalled.
2. **Worker PTO Failover (Cross-Provider Handoff)**:
   - Because all coordination state (task brief, linked findings, bulletin thread, and git working tree/worktree) lives in **SQLite (`TeamStore`)** rather than inside one agent's memory:
     - Releasing or reassigning a PTO worker's `[in_progress]` task resets its status to `[open]` (already implemented in `TeamStore::update_task` / `remove_member`) while preserving every recorded finding and bulletin note.
     - The Supervisor (or Herdr's auto-failover policy) can immediately spawn or assign a standby worker on a **different provider** (e.g. failing over from a quota-exhausted Claude worker to Codex, Gemini, OpenCode, or `zcode`).
3. **Supervisor PTO & Automatic Succession ("Acting Supervisor")**:
   - If the **Supervisor** hits a usage limit, workers completing tasks would otherwise stall waiting for `@supervisor`.
   - **Dynamic `@supervisor` Role Resolution**: Because `TeamStore::resolve_recipients` resolves `@supervisor` dynamically to whichever member currently holds `TeamRole::Supervisor` (and `TeamAction::SelectSupervisor` can promote any member on the fly), Herdr can designate a **Deputy / Fallback Supervisor** (or auto-promote a healthy `Reviewer` / `Architect` on a different provider when the Supervisor goes `OutOfOffice`).
   - Upon promotion, the new Acting Supervisor receives the full SQLite state snapshot (Goal, Open/In-Progress Tasks, Findings, and unacknowledged `@supervisor` mentions) and resumes orchestration without losing a single beat.

---

### 5.6 Supervisor follow-through without endless polling

While a team has useful work in progress, let its supervisor react to task
results, blockers, and messages without requiring the user to repeatedly wake
it. Prefer event-driven check-ins and use a bounded, low-frequency fallback
only where needed. Avoid busy loops, continuous model calls, and unbounded
unattended execution. Show the user whether coordination is active, idle, or
waiting for a decision, and provide a way to pause or stop it.

Continue with clear work within the user's existing authorization. Stop or
become idle when the goal is complete or there is no clear useful action;
ask the user when a decision, approval, or missing information actually needs
them. Never infer that a quiet worker has completed its task. Respect member
identity, durable message state, and busy or permission-blocked sessions;
where a safe wake-up is unavailable, leave work pending and say so. Stopping
coordination or closing the UI must not terminate user-owned sessions.

**Acceptance:** With deterministic events and a controllable clock, verify
that new work wakes coordination without a spin loop, periodic checks are
bounded, interruption and pause work, completed and blocked goals stop
unattended activity, and an unavailable recipient is not falsely reported
as notified. Exercise the flow with real agents separately.

## 6. Remote agent teams (planned)

Build on a reliable local team first. Herding can already show panes from an
SSH host, but showing a pane is not the same as giving its agent access to the
team's board. The current SSH-side `herding` shim supports browser commands;
it does not provide the Team CLI. Do not describe remote teams as working until
their coordination path is implemented and tested.

### 6.1 First milestone: one team on one remote machine

Let the user select an existing SSH host in Herding and create a team whose
supervisor and workers run on that host. Keep one authoritative team record in
the coordinating UI, and let the remote agents use the same task, bulletin,
acknowledgement, and finding operations as local agents through a deliberately
scoped, authenticated connection. Choose and document the actual transport and
its lifecycle after checking the existing SSH and control-socket capabilities;
viewing a remote daemon or forwarding an unprotected socket is not sufficient.

- Bind every member to a verified host, daemon, and agent session. Revalidate
  identity after reconnects and pane reuse, and show unavailable or unsupported
  capabilities rather than silently targeting another session.
- Preserve the existing approval and session controls. Store a message before
  attempting notification; do not inject into a busy or blocked agent, and
  surface failed or uncertain delivery without blindly replaying a submission.
- Keep queues and responses bounded. Define how remote agents discover the
  coordinator, what remains available when the UI or connection is down, and
  how access is removed when a member leaves.
- Keep credentials on the machine where each agent runs. Treat files, working
  directories, and artifacts as belonging to that remote host; record enough
  context to make evidence references useful without implying they are local.
- Show the selected host and the actual state of each member in the Team view.
  Reuse existing launch and attach paths where they can establish the needed
  identity and capabilities; leave user-owned sessions running when UI detaches.

**Acceptance:** In isolated test environments, create a team on one SSH host
with a real supervisor and two real workers. Have them claim distinct tasks,
exchange a targeted message while a recipient is busy, acknowledge it later,
and produce a result with a remote evidence reference. Verify scoped access,
session replacement, disconnect/reconnect, and accurate pending, failure, and
uncertain states. Also validate the actual native UI; simulated peers alone do
not establish real agent compatibility.

### 6.2 Later milestone: one team across multiple hosts

Allow a single team to include members from the local machine and multiple SSH
hosts. Retain one authority for team state, and route each operation to the
member's verified host and session rather than the currently selected host or a
pane ID alone. Specify offline handoff, ordering, duplicate and uncertain
delivery, reconnect, and coordinator availability before claiming support.

Filesystems and Git worktrees remain host-local unless deliberately shared.
Define an explicit, user-visible handoff for source revisions, uncommitted
changes, binaries, and artifacts, including availability and integrity. Keep
provider credentials on their own hosts and do not require Git for a team.

**Acceptance:** Run a supervisor and workers across at least two hosts, take a
host offline during work, and reconnect without silent loss or duplicate
delivery. Verify the resulting task state, evidence, and any transferred
artifact on the intended host. See the separate multi-machine requirements in
the [detailed spec](herding-detailed-spec.md#10-multi-machine-evolution).

For related projects and questions to investigate, see
[Herding: projects to study](herding-references.md).

## 7. UI responsiveness (planned)

Investigate the reported slow UI with a representative team and a release
build. Measure the interaction that feels slow, identify whether state updates,
layout, rendering, or background work causes it, and fix the measured cause.
Keep UI-thread work bounded and compare before and after; headless tests alone
do not establish native interactive performance.
