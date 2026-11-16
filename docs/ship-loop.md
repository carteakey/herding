# The Ship Loop

How to run an agent team so work keeps landing instead of stalling at 80%.

This is the operating pattern that produced a consistent, multi-hour shipping
run on a real project: one coordinator, a small pool of workers, and a loop
that never lets a finished task sit unlanded. It is deliberately boring. The
value is in the rules being explicit and enforced every single time.

Use it when you have more than one agent working the same repository, or when a
single agent keeps producing work that never quite merges.

---

## Roles

**Supervisor (one).** Owns the gate, the push, the board, and the queue. Does
not implement product code. The supervisor's job is to keep the loop turning:
review fast, push fast, reassign stalled work fast.

**Workers (one or more).** Implement one scoped task at a time. Never push.
Never assign themselves work. Report the commit hash and how they verified it.

The single most important rule: **workers do not push, and the supervisor does
not implement.** Mixing the two is what creates half-finished branches and
contested merges.

---

## The loop

For every task:

1. **Assign.** Give it a durable task ID, a one-paragraph brief, a scope, and a
   gate definition. Put it on the board with an assignee.
2. **Design note first** if the task touches schema, concurrency, auth, or more
   than one file of behavior. The supervisor reviews the design before code
   exists. Cheap up front, saves a rewrite.
3. **Implement in a dedicated worktree.** One logical change per commit. No
   push. Post the hash plus what was verified.
4. **Gate in isolation.** Create a throwaway worktree at the exact commit hash
   and run the gate there. Review the diff. Reproduce the claimed fix. Do not
   trust "tests are green".
5. **Push, then update the board.** The task is not done until it is on the
   default branch.
6. **Assign the next task immediately.** The worker should never finish into an
   empty queue.
7. **If a worker is silent after one nudge, reassign.** Do not wait for a
   stalled worker to un-stall itself.

The loop is only as good as step 4 and step 6. Those are where teams stall.

---

## The gate

Write it down once and enforce it identically every time. For a Go project:

```sh
# in a fresh worktree at the exact commit being gated
gofmt -l .            # must be empty
go build ./...
go vet ./...
go test ./... -count=1
```

Rules:

- **Gate the commit, not the working tree.** A worker's tree may contain the
  next task's uncommitted changes. Use a separate worktree at the hash.
- **Review the diff, not just the result.** Read the actual change. Look for
  deleted safety checks, silent behavior changes, and tests that assert the
  wrong thing.
- **Reproduce the bug being fixed.** If the task claims "X no longer happens",
  make X happen against the old commit and show it is gone at the new one.
- **Check formatting and generated files** if the project cares.
- **Never weaken an existing gate to make a commit pass.**

A gate that takes more than a few minutes will not be run every time. Keep the
fast gate fast; put the slow checks (race, browser, security scans) in CI.

---

## Task shape

A good task brief has four parts:

```
Title: <imperative, specific>
Scope: <files/packages the worker may touch>
Brief: <what and why, constraints, edge cases>
Gate:  <the exact commands + what "verified" means>
```

Keep tasks small enough to gate in one sitting. If a task cannot be described
in a paragraph, it is probably two tasks.

Examples of well-shaped tasks from a real run:

- "Reuse per-source SQLite handles across polling cycles" — one package,
  explicit eviction/staleness rules, benchmark required.
- "Prevent CSV formula injection in the export endpoints" — one helper, two
  call sites, tests for each dangerous prefix.
- "Collapse Overview's scalar full-table scans into one pass" — one query, a
  plan assertion, and a benchmark.

And a well-shaped *negative* result: "SQL-side daily aggregation measured
slower than the Go fast path (291ms vs 137ms); keep the Go path, land the DST
test and benchmark." Rejected experiments are results. Close them.

---

## Worktrees

One worktree per worker, always. Never let a worker switch the shared
checkout's branch — it strands other workers' uncommitted commits and creates
rework. If a worker needs a branch, it creates a worktree.

The supervisor also uses a throwaway worktree per gate, so gating never
disturbs anyone's working tree.

---

## Reviewing: repro first

The highest-leverage thing a supervisor does is not reading code — it is
reproducing the claim.

- If a fix says "the cursor no longer skips the record", write the smallest
  test that makes the old code fail and run it against the new commit.
- If a security fix says "cells are neutralized", export a CSV containing
  `=cmd` and inspect the bytes.
- If a performance fix says "6ms", run the benchmark on a production-shaped
  database, not an empty one.

This is how you catch the bugs that the worker's own tests were written to
miss. It is also how you decide whether a "done" claim is real.

---

## Handling stalls

Stalls are the default failure mode of multi-agent work. Handle them
mechanically:

| Symptom | Action |
| --- | --- |
| Task assigned, not claimed after one nudge | Reassign to an active worker |
| Worker silent on a critical-path task | Nudge once, then reassign |
| Worker's branch is behind main | Ask for a rebase, or cherry-pick the scoped commit yourself |
| Work stacked on a blocked commit | Stop the stack; fix the blocker first |
| Two workers touching the same file | Serialize: one lands, the other rebases |
| Task growing beyond its brief | Split it; land the smallest correct slice |

Do not let a task sit "in progress" indefinitely. The board should always
reflect reality: every open task has an owner who is actually working it.

---

## Backlog grooming

Turn every observation into a durable task, even the small ones:

- review findings (with measured evidence and file:line)
- follow-ups discovered during a gate
- hardening notes from a security pass
- "we tried X and it was slower" (with numbers)

Groom the backlog in batches and keep it unassigned until a worker frees up.
This is what prevents the "worker finished, now what?" pause that kills
momentum.

---

## Release discipline

Before a release, run a readiness review as its own task:

1. Clean clone: build, vet, full suite, race on concurrency-touched packages.
2. Upgrade smoke on a production-shaped database copy (migrations, health,
   dashboard, new indexes).
3. Docs match reality: changelog and spec claim only what shipped.
4. No WIP or rejected experiments on the default branch.
5. A security/privacy pass with an explicit "blocks the tag or not" verdict.
6. Then tag; let CI publish.

The release commit should be gated like any other commit. Tagging is the
supervisor's action, not a worker's.

---

## Why teams stall (and the fix)

| Failure mode | Fix |
| --- | --- |
| Nobody owns landing | One supervisor owns gate + push |
| "Done" is vague | Written gate, enforced every time |
| Tasks are too big | Slice until each fits one gate |
| Worker finishes into an empty queue | Keep 1–2 tasks queued per worker |
| Stalled worker holds critical work | Nudge once, then reassign |
| Shared checkout collisions | One worktree per worker |
| Trusting "tests green" | Reproduce the claim at the gate |
| Negative results feel like failure | Close them with numbers and keep the test |

---

## Copy-paste checklist

For each task:

- [ ] Task ID, brief, scope, gate on the board
- [ ] Design note reviewed (if risky)
- [ ] Implemented in a dedicated worktree, scoped commit, no push
- [ ] Gated in a worktree at the exact hash: fmt, build, vet, test
- [ ] Diff reviewed; claimed fix reproduced
- [ ] Pushed; board updated
- [ ] Next task assigned immediately
- [ ] Stalled work nudged once, then reassigned

---

## A worked example

A real run on a Go project, over one session:

- Workers implemented: incremental log tail-parsing, a session-metadata index
  cache, per-source SQLite handle reuse, covering indexes for the dashboard's
  expensive aggregates, CSV exports, sortable tables, accessibility fixes, auth
  hardening, and a connector-down data-loss fix.
- The supervisor reproduced two bugs the workers' tests missed (a partial-record
  cursor skip and a CSV injection path), rejected one performance experiment
  with benchmarks, and gated/pushed every commit in an isolated worktree.
- Every review finding became a board task. When a worker went idle, its task
  was reassigned within minutes.
- The run ended with a tagged release and a groomed backlog, not a pile of
  half-finished branches.

The pattern is not clever. It is just the loop, run every time, without
skipping the boring parts.
