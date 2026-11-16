# Herding playbook: backlog-first team sessions

A practical playbook for spending a large token budget productively with a
Herdr team. It is the operating model the `herdr-supervisor` skill points at:
one supervisor, three workers, and a durable backlog.

Validated on 2026-11-05 in the Lytte repository: 28 issues seeded, 13 shipped,
11 logical commits, CI green, and a clean working tree at handoff.

## The one rule: the backlog is the source of truth

Chat is not a plan. Before assigning any work, turn the goal into issues.
Every issue is the durable record of why the work exists and what "done"
means; a Herdr task is only the session-scoped execution row for it.

| Layer | Holds | Lifetime |
|---|---|---|
| GitHub issue | Problem, current state, done-when checklist, references | Durable |
| Herdr task | Assignment, brief, claim, result | Session |
| Bulletin | Coordination, review notes, pause/resume | Session |
| Finding | Durable evidence and decisions | Team |

If the human says "make progress", the first move is always: inspect the
repository, seed or refresh the backlog, then assign the highest-value
independent issues. Do not start with code.

## Issue shape

Every issue should be answerable without a meeting:

```markdown
## Problem
What is wrong or missing, and who feels it.

## Current state
Where the code/docs stand today, with file paths.

## Done when
- [ ] Observable outcome 1
- [ ] Observable outcome 2
- [ ] Tests cover the failure and the fix
- [ ] Docs/status updated

## References
- `path/to/file.ts`
- `docs/RELEVANT.md`
```

Labels: `area: server`, `area: web`, `area: engine`, `area: asr`, `area: tts`,
`area: desktop`, `area: ci`, `area: docs`; `priority: high|medium|low`.

Priority rule of thumb:

- **High**: correctness, security, data loss, broken deploys, misleading docs.
- **Medium**: user-visible workflows, missing core integrations, performance.
- **Low**: optional backends, launch polish, speculative integrations.

Group tiny items into one issue with a checklist; do not create 50 one-line
issues. Aim for 20-30 issues for a multi-hour, three-worker session.

## Session shape

- One supervisor and three workers. The supervisor never implements worker
  tasks; it reviews, commits, pushes, and closes.
- Shared working tree by default for docs, small disjoint edits, and reviews.
  Use worktrees when tasks overlap files or need heavy compiles
  (`docs/herding-worktrees.md`).
- One issue per worker per batch. Three issues in flight is the sweet spot:
  enough parallelism to hide review time, few enough to keep diffs reviewable.
- Keep a 10-15% token reserve for CI fixes and review follow-ups.

## Phase 0: seed the backlog

1. Read the repository's own backlog sources: `TODO.md`, review docs,
   architecture "feature backlog" sections, open issues, and code comments.
2. Write issues with the shape above. Verify every claim against the code;
   do not copy a stale doc into an issue.
3. Label and prioritize. Mark partial features explicitly ("first slice").
4. Post a short bulletin: the goal, the issue range, and the rules.

## Phase 1: assign a batch

Use `team task create` with a self-contained brief. The brief is the contract;
a worker should never need to ask what "done" means.

### Brief template

```text
Repo: /absolute/path (shared tree; preserve all uncommitted work;
do not commit or push - I will review and commit).
GitHub issue #N.

Implement:
1. <smallest correct change>
2. <edge cases and bounds>
3. <tests: failure, fix, and regression>
4. <docs/status update>

Verify: <exact commands>. Report exact results and any limitation.
Code/docs only; no commit.
```

Rules that prevent most failures:

- Name the files or modules to touch, and the ones to leave alone.
- State the constraints: no new dependency, no schema change, no UI rewrite.
- Require evidence: `file:line`, exact test commands, actual output.
- Tell the worker to stop and report if the brief is ambiguous or the change
  needs a product decision.
- For docs-only work, say "docs-only" and require a link check.
- For code work, say "run the focused tests and type-check" and require the
  commands and counts.

### Waking workers

The `herdr-gpui` on `PATH` may be older than the running app. Give workers the
exact binary path and the pane environment:

```bash
HERDR_PANE_ID=w1Y:p2 HERDR_WORKSPACE_ID=w1Y \
  /path/to/herdr-gpui team task claim <TEAM_ID> <TASK_ID>
```

If a worker sits idle after a task is created, prompt the pane with the task
ID and the exact claim command. Do not assume task creation wakes it.

## Phase 2: supervise

1. **Review the diff, not the prose.** `git diff -- <paths>` and
   `git show <sha>`. A completion report can be stale or over-claim.
2. **Run the checks yourself.** Type-check, focused tests, compose config,
   link scan, `git diff --check`. A worker's "all green" is a claim until you
   see the output.
3. **Request follow-ups for real defects**, not style preferences. Examples
   from the validated session:
   - a word-level export dropped speaker labels;
   - a hook could overwrite a completed expansion with a stale ref;
   - a docs claim marked an endpoint "done" when it did not cover embeddings;
   - a server warning claimed "other machines can reach it" when the host
     port was loopback-only.
4. **Commit in logical batches.** Group by area or feature, not by worker.
   Conventional subjects (`feat`, `fix`, `docs`, `ci`, `refactor`), a body
   that explains the problem and approach, no AI attribution.
5. **Push, then watch CI.** Fix failures before the next batch. Re-run a
   flaky job rather than calling the run green.
6. **Close the loop.** Comment the commit SHA on each issue; close only when
   the shipped behavior matches the done-when list. Leave partial issues open
   with exactly what remains.
7. **Go idle.** Do not poll; completions wake you.

### Review checklist

- Does the diff implement the issue, not a nearby idea?
- Do the tests fail without the change and pass with it?
- Are bounds, errors, and empty states handled?
- Did the worker touch unrelated files or revert someone else's edits?
- Any secrets, credentials, or machine-local paths?
- Do the docs match the code (endpoints, env names, defaults)?
- Did the worker run the checks it claims, and are the counts plausible?

## Phase 3: close the session

1. Update `CHANGELOG.md` with user-facing entries.
2. Make sure the working tree is clean and `main` is pushed.
3. Post a bulletin summary: shipped issues, commits, checks, open items.
4. Pause workers with an explicit "do not start anything new" message when the
   token budget is low. The supervisor can still finish docs, CI, and commits.
5. On handoff, list every open issue and the exact next step.

## Spending a large budget (the 300M plan)

A three-worker session can burn 300M tokens productively without producing
unreviewable sludge:

- **Seed 25-30 issues** across high/medium/low priority.
- **Run 8-10 batches** of three issues. Each batch is roughly 20-40 minutes of
  worker time and 10-20 minutes of review.
- **Review and commit every batch before starting the next.** Unreviewed work
  compounds; a clean tree at every checkpoint is the safety valve.
- **Keep the supervisor's context for the backlog and diffs**, not for
  implementation details. Workers carry the implementation context.
- **Stop at a clean checkpoint** when the budget runs low: tree clean, tests
  green, issues commented, workers paused.

If a batch produces a large, tangled diff, stop and split the issue. A 100-file
uncommitted tree is a failure mode, not progress.

## CI triage

- **Missing Python test dependency** (for example `numpy`): add it to the
  project's `dev` extra and refresh the lockfile (`uv lock`).
- **Missing system tool** (for example `ffmpeg`): install it in the CI job
  before the test step.
- **Flaky runner queue** (macOS jobs cancelled before any step): re-run the
  failed job once. If it passes, record the flake; do not change code for it.
- **Red on an older commit**: the latest commit's run is what matters, but
  never call a red run green. Re-run or fix, then verify.

## Anti-patterns

- Assigning work before the backlog exists.
- Vague briefs ("improve the docs", "make it faster").
- Two workers editing the same file in one batch.
- Trusting a completion report without reading the diff.
- Committing everything in one blob at the end.
- Closing issues on task-done instead of shipped behavior.
- Leaving a 100+ file working tree uncommitted across batches.
- Polling idle workers instead of using the board and bulletin.
- Spending the last 10% of the budget on a new feature instead of a clean
  checkpoint.

## Worked example: Lytte, 2026-11-05

- 28 GitHub issues seeded from docs, review notes, and architecture backlogs.
- Three workers ran batches of three independent issues on a shared tree.
- 13 issues shipped, 11 logical commits, CI green, working tree clean.
- Review caught four real defects that the completion reports had missed:
  lost speaker labels, a stale-ref race, an over-claimed capability status,
  and a misleading exposure warning.
- CI fixes: `numpy` in the engine test extra, `ffmpeg` in the Python job, and
  a re-run of the macOS runner queue.
- The supervisor paused the workers at a clean checkpoint and handed off a
  prioritized open-issue list.

That is the loop: backlog, batch, review, commit, verify, close, checkpoint.
