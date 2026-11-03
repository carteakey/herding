# Herding: Architecture Review & Implementation Roadmap

Status: 2026-11-03. This document captures the review of [`herding-spec.md`](herding-spec.md),
[`herding-detailed-spec.md`](herding-detailed-spec.md), and [`herding-handoff.md`](herding-handoff.md)
against the native Rust implementation on `codex/herding-bulletin`, and defines the implementation
plan for completing the full feature.

## 1. Baseline Implementation Summary

- **Durable SQLite Team Store (`crates/herdr-orchestrator/src/team_store.rs`)**:
  - Stores `teams`, `members`, `posts`, `mentions`, and `acknowledgements` in `<state_dir>/team/team.sqlite3` with `0600` permissions.
  - Enforces immutable member session targets (`MemberTargetChanged`), input bounds, human vs. member attribution, and idempotent per-recipient acknowledgements.
- **Background Worker & Control Socket (`crates/herdr-gpui/src/control/server/team*.rs`)**:
  - Non-blocking serialized worker (`herdr-team-store`) with bounded queue capacity (`16`).
  - Control protocol (`crates/herdr-control-protocol/src/protocol/team.rs`) and CLI (`crates/herdr-gpui/src/cli/control/team.rs`).
- **Native GPUI View (`crates/herdr-gpui/src/team/`)**:
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
   - Support `herdr-gpui team status` (discovering the caller pane's active team, member identity, goal, pending mentions, and assigned tasks without requiring a known `TEAM_ID`) and `herdr-gpui team register`.
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
  - `herdr-gpui team task list TEAM_ID`
  - `herdr-gpui team task create TEAM_ID --title TITLE [--brief BRIEF] [--assign MEMBER_ID]`
  - `herdr-gpui team task assign TEAM_ID TASK_ID --to MEMBER_ID`
  - `herdr-gpui team task claim TEAM_ID TASK_ID`
  - `herdr-gpui team task block TEAM_ID TASK_ID --reason REASON`
  - `herdr-gpui team task complete TEAM_ID TASK_ID --result RESULT`
  - `herdr-gpui team task cancel TEAM_ID TASK_ID`
- **UI**:
  - Render the compact Task Board inside `TeamView` with task creation, status badges, assignee labels, blocker details, and completion results.

### 3.2 Findings & Evidence (`TeamStore` + Protocol + CLI + UI)
- **Store**: `findings` table (`finding_id`, `team_id`, `author_kind`, `author_id`, `task_id`, `description`, `evidence`, `confidence` [`low`, `medium`, `high`], `superseded_by`, `created_at`).
- **Invariants**:
  - Findings are append-only durable knowledge; corrections link via `superseded_by` without erasing prior claims.
- **CLI & Protocol**:
  - `herdr-gpui team finding list TEAM_ID`
  - `herdr-gpui team finding add TEAM_ID --confidence low|medium|high --evidence EVIDENCE [--task TASK_ID] [--supersedes FINDING_ID] DESCRIPTION...`
- **UI**:
  - Render Findings & Evidence in `TeamView` with confidence badges, evidence references, task links, and supersession indicators, plus a UI form to record findings.

### 3.3 Delivery States & Safe Session Notification
- Track recipient delivery states (`Pending`, `Submitted`, `Acknowledged`, `Failed`, `Uncertain`).
- Surface whether a member session is live, busy, or unavailable from the daemon snapshot without injecting raw keystrokes into busy or permission-blocked PTYs.

### 3.4 Agent Skill & Supervisor Bootstrap
- Update the managed skill documentation (`docs/cli.md` and `skills/herdr-gpui-browser/SKILL.md`) so supervisor and worker agents know how to discover their team (`herdr-gpui team status`), manage tasks (`team task ...`), exchange bulletin updates (`team bulletin ...`), and record evidence (`team finding ...`).

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
   - Outgoing prompts targeting paused panes are held in `Bridge::queued_prompts` (`Paused (N queued)`) and flushed automatically on **Resume**.
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
