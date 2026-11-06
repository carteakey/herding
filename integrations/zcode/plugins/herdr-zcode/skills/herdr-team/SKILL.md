---
name: herdr-team
description: Coordinate with a Herdr Team from a ZCode CLI session running in a Herdr pane.
---

# Herdr Team

Use the Herdr Team CLI when this ZCode session is running in a Herdr pane. The
Herdr plugin reports this session's state and resume command; the `herdr-gpui`
CLI proves the caller from the pane environment and current daemon snapshot.

Start with `herdr-gpui team status` to find the team, goal, pending mentions,
and assigned tasks. Use `herdr-gpui team task list TEAM_ID` to inspect work and
`herdr-gpui team task claim TEAM_ID TASK_ID` to claim an open task. Complete a
finished task with `herdr-gpui team task complete TEAM_ID TASK_ID --result
"RESULT"`; if blocked, use `team task block` with a concise reason.

Read messages with `herdr-gpui team bulletin list TEAM_ID` (the newest page;
`--from 0` reads history from the beginning). Acknowledge a
message addressed to you with `herdr-gpui team bulletin ack TEAM_ID MESSAGE_ID`.
Post concise progress, evidence, and questions with `team bulletin post`.
Use `herdr-gpui team finding add` when a durable finding needs evidence and a
confidence level.

Workspace Isolation:
- Shared Working Tree (default): Preferred for docs, reviews, disjoint files, and
  small fixes to leverage shared build caches.
- Git Worktrees (opt-in): Use when parallel tasks touch the same files/crates or
  run heavy test suites to avoid cargo build directory lock contention (see
  `docs/herding-worktrees.md`).

Autonomous Goal Follow-Through & Idle Protocol:
- Pursue the Goal: Autonomously drive toward the overarching goal until verified complete.
- Proactive Claiming: After finishing a task or when idle, check `team task list` for open tasks to claim or check the bulletin rather than sitting idle.
- When to Stop: Rest idle only when all tasks are complete and verified by QA, or when blocked waiting for user decisions, approvals, or secrets.

These commands take caller identity from the Herdr pane environment. Do not
invent pane or workspace identifiers or pass flags to impersonate another
session. If status says there is no team or the caller is unsupported, explain
that limitation instead of guessing an identity.
