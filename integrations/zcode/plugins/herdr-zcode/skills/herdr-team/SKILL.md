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

## If a wake does not arrive (temporary workaround, issue #35)

Team wakes can be silently dropped by the app's paste path after an app/daemon
restart or a workspace/pane change. The member looks registered and
`team heartbeat` says live/HEALTHY, but no message appears in the pane and
`herdr-gpui.jsonl` has no `team wake drain` line for that pane.

Until #35 lands, deliver the prompt manually from the supervisor's pane or any
shell:

    herdr pane run <PANE_ID> "<the full team prompt>"

`herdr pane run` sends the text and Enter in one call. The two-step form is
`herdr pane send-text <PANE_ID> "<prompt>"` then `herdr pane send-keys <PANE_ID>
Enter`.

If the pane is on an SSH host, its pane id belongs to that host's daemon: run
the command on that host, e.g. `herdr --remote <ssh-target> pane run <PANE_ID>
"<prompt>"`, or SSH in and run its local `herdr pane run <PANE_ID> "<prompt>"`.
The local `herdr pane run`/`send-text` only reaches the local daemon; it cannot
address a pane on another machine.

Then verify the member acted (`team task list`, `team bulletin list`) or read
the pane (`herdr pane read <PANE_ID> --lines 40`). This bypasses the app's
paste path entirely. Do not assume a wake was delivered because the post was
stored; check the pane or the ack.

Root cause: `deliver_team_prompt` returns `Kept` for a pane it cannot prove,
and the #17 path maps that to `Held` and re-enqueues with no backoff; the
direct dispatch path also logs nothing on failure. Tracked in issue #35.

## Commit and push

Work on a branch and keep `main` untouched. Do not commit, push, or open a PR
until the supervisor explicitly authorizes it in the bulletin ("AUTHORIZED:
push ..."). Post the branch tip and the gate counts; the supervisor lands it
fast-forward. The repository's "only commit/push when requested" rule applies
to you as a worker; the supervisor's review-and-land authority is separate.

## Multi-machine

The team store and wake queue live in the UI app on the supervisor's machine.
For a remote member, the UI must be connected to that host's endpoint and the
pane must be in the endpoint snapshot; register the member from its own pane
through the control tunnel (`scripts/control-tunnel.sh`), not against the
remote host's own store. The local `herdr pane run` cannot reach a remote pane:
use `herdr --remote <target> pane run` or SSH in and use the host's local
`herdr pane run`. See `docs/control-tunnel.md` and issue #35.

## File issues in this fork, not upstream

Open issues and PRs in `carteakey/herding` (the fork). `penso/herdr-gpui` is
upstream and a read-only reference: do not open issues, pull requests, or
comments there unless the user explicitly asks for an upstream report. Team
findings, fork bugs, and follow-ups belong on the fork's tracker and the team
board. If a worker spots an upstream bug, re-file it in the fork and cite the
upstream commit/issue as evidence only.
