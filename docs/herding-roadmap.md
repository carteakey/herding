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
