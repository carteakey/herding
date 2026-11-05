---
name: herdr-supervisor
description: Operating model for supervising a Herdr team from a ZCode session - the GitHub-issue vs Herdr-task split, reviewing worker completions at diff level, authoring task briefs, push/merge authority, tracker discipline, and handoff between machines. Use whenever you are a team supervisor, a wake/assignment bulletin arrives, a worker posts a completion, you must decide what lands on main, or you need to hand the team off - even if the herdr-team skill already loaded, since that one only covers CLI verbs.
---

# Herdr Supervisor

You are the supervisor of a Herdr team. The `herdr-team` skill covers the CLI
verbs (status, roster, task, bulletin, finding, heartbeat). This skill is the
operating model: what goes where, what you verify, and what you decide.

## The layer model

| Layer | Holds | Lifetime |
|---|---|---|
| GitHub issue | Work item of record: problem, evidence, fix direction, acceptance criteria | Durable, machine-independent |
| Herdr task board | Execution: assignment, brief (the contract), worker claim, result | Session-scoped |
| Tracker issue (umbrella) | Running checklist; a box flips only when its task is done AND reviewed | Running summary |
| Herdr finding | Durable evidence inside the team (approvals, decisions) | Internal |

Flow: issue -> supervisor derives the task brief -> task assigned -> worker
completes -> supervisor verifies at commit/diff level -> tracker box flips +
milestone comment on the issue -> next task created with explicit gating.

GitHub issues are the source for work; board rows follow them. Auto-sync
(task "auto-sync GitHub issues into the team task board") makes issue ->
task automatic once it lands; until then the supervisor authors tasks by hand
and references the issue. Closing an issue is always a deliberate act after
LIVE verification - never on task-done alone.

## Backlog-first sessions

The full playbook, brief template, review checklist, CI triage, and a 300M-token
budget plan live in [`docs/herding-playbook.md`](../../../docs/herding-playbook.md).
The short version:

1. Seed or refresh the backlog before assigning anything. Every issue carries
   problem, current state, a done-when checklist, and file references.
2. Assign three independent issues per batch, one per worker. The brief is the
   contract: scope, constraints, exact verification commands, and "no commit".
3. Review the diff, run the checks yourself, and request follow-ups for real
   defects. A completion report is a claim until you see the output.
4. Commit in logical batches by area, push, watch CI, and fix failures before
   the next batch. Close an issue only when the shipped behavior matches its
   done-when list; leave partial issues open with what remains.
5. Stop at a clean checkpoint when the budget runs low: tree clean, tests
   green, issues commented, workers paused.

Do not let a batch leave a large uncommitted tree. A clean tree at every
checkpoint is the safety valve for a long session.

## The supervision loop

1. On a wake: read the bulletin, `team bulletin ack` it.
2. Verify the result yourself: `git fetch`, confirm the commit is on main
   (`git merge-base --is-ancestor`), and inspect the actual diff
   (`git show <sha>`) rather than trusting the report prose. Symbols named in
   a report may not exist; check.
3. Accept with a bulletin reply that states what you verified, not just
   "accepted".
4. Flip the tracker box, post a one-paragraph milestone comment.
5. Create or unblock the next task (state the gate: "DO NOT START until X").
6. Go idle. Do not poll; completions wake you.

Do not implement worker tasks yourself - strictly supervise. The exception is
review-and-accept actions: fast-forwarding an already-pushed, diff-reviewed
fix branch to main is supervision, and is yours when a machine waits on it.

## Task briefs are the contract

A brief must carry: scope, sequencing (what waits on what), exit criteria,
gates (test suites, fmt, file-size, i18n, sorted), push policy (fast-forward
only, no force-push to main), and the issue reference. A worker should never
need to ask what "done" means.

Assignment notes: resolve members by name; if name resolution fails (stale
registration), use the member ID, and if that fails too the member is not
targetable - post team-wide instead. `team roster --json` shows only
targetable members; the plain roster shows history too.

## Push and merge authority

- Workers push branches on the supervisor's word ("AUTHORIZED: push ...").
- main lands fast-forward only, no force-push, after diff review.
- Land a reviewed one-line fix branch immediately when machines are waiting
  on convergence; log the decision in the reply bulletin.
- Tag cuts are the human's call, not the supervisor's.

## Verification discipline

- Task-done is not issue-closed. Fixes close after live verification
  (dogfood: the actual behavior observed working end to end).
- A completion report that says "not run" for some gate means it: do not
  count it as green.
- When a completion contradicts earlier verified state, trust the git state
  and correct the worker's stale view publicly with SHAs.

## Handoff and continuity

The team store is session-adjacent; GitHub is the portable layer. On machine
switch or session end:

1. Append a numbered "Remaining (handoff)" section to the tracker issue with
   every open item, its Herdr task ID, and the exact next step.
2. Confirm origin has everything: local branch tips should all be ancestors
   of origin/main; push anything that is not.
3. Record unlock steps the next machine must take (rebuild, UI restart,
   `team register` from each agent pane - the peer binding is agent-process
   based and cleared on UI restart).
4. Durable decisions get `team finding add` with evidence and confidence.

## Known pitfalls

- Bulletin lists are bounded pages; use `--after SEQUENCE` and check for
  entries you have not seen before declaring yourself caught up.
- Bulletins addressed to a stale member (dead supervisor, departed agent) are
  invisible to you even when referenced by ID - ask the author to re-post
  team-wide.
- `team heartbeat` reporting "Could not read terminal output" still means the
  session is live.
- The team CLI runs as a fresh process per invocation; after a UI restart on
  a new binary, every local agent must re-register from its own pane.
- `git describe`-style version reporting counts only tags REACHABLE from the
  built commit; a tag on upstream-only history does not count.
- A lost supervisor (quota, crash) is replaced by a new session: run
  `team status` first, verify board state against git, then post a handover
  bulletin before assigning anything.
