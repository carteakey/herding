# Herding performance notes

What the team UI costs at scale, which bounds keep it healthy, and where the
measured hotspots are. Numbers below are from one Linux machine (headless
GPUI for frame times, debug and release profiles for SQLite and bridge tests),
so treat them as relative sizes and ratios, not absolute promises; the benchmark
sources are the stable reference.

Benchmarks in the tree:

```sh
# Projection and frame timings (printed, never asserted in CI)
cargo test --release -p herdr-gpui --bin herdr-gpui perf_tests -- --nocapture
# SQLite store costs (opt-in, ignored in CI)
cargo test -p herdr-orchestrator --test team_store_perf -- --ignored --nocapture
```

## Data flow, and where it can get slow

1. **Store worker (serial).** Every 2s (`REFRESH`) and after each CLI
   mutation, an `Operation::Snapshot` reads the team from SQLite: its newest
   `POST_LIMIT` (200) posts, taken as `MAX_PAGE_SIZE` (100) pages from the
   newest end so the cost does not grow with the team's history, plus all
   tasks and findings up to `MAX_PAGE_SIZE`.
2. **Bridge.** The reply lands in `Bridge`, and `Bridge::snapshot`
   re-projects the whole board into a `TeamSnapshot` whenever the bridge is
   dirty (every team event under multi-agent traffic) on the UI thread.
3. **View.** Renders read the prepared snapshot only. The bulletin clips to
   the newest `RECENT_POSTS_LIMIT` (20) posts in steady state, windowed to
   30 when expanded with paging; the board bounds visible active and completed
   tasks to 16 rows per section with disclosure toggles.

## Measured costs (Linux x86_64, headless)

| Path | Before | After | Notes |
| --- | --- | --- | --- |
| `Bridge::snapshot`, small board | ~17µs/call | ~31µs/call | 20 posts, 10 tasks, 5 findings |
| `Bridge::snapshot`, at bounds | ~90µs/call | ~92–145µs/call | 200 posts, 120 tasks, 60 findings; deep-clones board onto UI thread |
| Threading overhead in `view_posts` (100 posts) | 66.3µs | 47.7µs | 1.4x speedup with one-pass author map |
| Threading overhead in `view_posts` (200 posts) | 144.9µs (+40% overhead) | 92.6µs | 1.6x speedup; eliminates O(posts²) rescan |
| Board draw, 64 open tasks + 64 collapsed findings | 24.03ms (release) | 6.74ms (release) / 4.00ms (debug) | dropped ~72% to 83%; well under 16.6ms 60fps budget |
| Board draw with 40 done tasks folded | 10.00ms | 6.76ms (release) | "Completed (N)" compaction active |
| Bulletin draw, 20-post clip | ~1.9–5.8ms | 2.88ms | steady state; cheap by design |
| Bulletin draw, 200-post history expanded | 33.83ms (release) | 4.43ms (release) / 2.95ms (debug) | dropped 87% to 91% via 30-post window and paging |
| Store snapshot at 200 posts | 408 statements, 6–8ms | 8 statements, 2.02ms median | batched recipient queries, indexed mentions, newest-page read |
| Store snapshot stress (1,000 posts) | 1,215 statements, 16–21ms | 8 statements, 2.39ms median | 99.3% statement reduction |
| Store snapshot with 4,000 posts | 1,215 statements, 7.14ms median, page ended at `post00999` | 8 statements, 1.94ms median, page ends at `post03999` | history no longer costs refresh time or hides the newest posts |
| Store writes | 67–84µs; ack 28–42µs | 67–84µs; ack 28–42µs | healthy |

## Fixes applied

- **UI board and bulletin virtualization (commit `3d4e79d1`):**
  - Active tasks and completed tasks are windowed to 16 rows each with
    disclosure toggles (`#team-tasks-active-toggle`, `#team-tasks-completed-show-more`).
  - Findings section was extracted to `team/view/findings.rs` and windowed to
    16 rows with `#team-findings-show-more`.
  - Expanded bulletin history is bounded to 30 posts with `#bulletin-show-earlier`
    paging, preventing 200 unvirtualized cards from dropping frames.
- **Store batched delivery queries (commit `b0b62058`):**
  - Replaced the double N+1 (`mentions()` + `recipient_statuses()` per post)
    with chunked batch lookups (`TeamStore::recipient_statuses_for_posts`) in
    500-id chunks.
  - Replaced full-history scanning for pending mentions with an aggregate
    `COUNT` query (`TeamStore::pending_mention_count`), backed by a new
    `CREATE INDEX IF NOT EXISTS mentions_recipient_idx`.
  - Added `prepare_cached` for hot reads on the worker connection.
- **Store newest-page snapshot reads (task `e9a9dbab`, finding `6e75fed0`):**
  - `TeamStore::latest_posts(team_id, Some(sequence), limit)` reads a page
    backwards from a sequence, and the worker snapshot now asks for its
    `POST_LIMIT` in `MAX_PAGE_SIZE` pages from the newest end instead of
    scanning a 1,000-post window and dropping all but its last 200.
  - A team older than that window now shows its newest posts: at 4,000 posts
    the old window ended at `post00999` while the newest was `post03999`.
  - `TeamStore::posts` keeps its forward cursor, so `herdr-gpui bulletin list
    --after SEQ` still walks forwards from a post.
- **Board newest-page reads and exact counts (task `7e1f19a7`, finding
  `271faf33`):**
  - `TeamStore::latest_tasks(team_id, Some(sequence), limit)` and
    `TeamStore::latest_findings(...)` read a page backwards from a sequence in
    the same shape as `latest_posts`; `tasks` and `findings` keep their
    forward reads and both directions now share one private page function per
    table, so the `MAX_PAGE_SIZE` bound exists once per table.
  - The worker snapshot and the CLI list replies take the newest page, so a
    team with more tasks or findings than `MAX_PAGE_SIZE` shows its current
    work instead of the oldest 100.
  - `team status` counts tasks with `TeamStore::task_counts` (one aggregate)
    instead of materialising 100 rows and counting them in Rust, which counted
    only the oldest page.
- **Bridge snapshot projection caching (commit `1fef43d1`):**
  - Built a single-pass `name_map` and author map during `Bridge::snapshot`.
  - Resolved reply-to authors in O(1) via hash map instead of rescanning
    all posts iteratively.
  - Cached member pane resolution (`pane_of`) per member.

## Bounds that keep this healthy

`POST_LIMIT` 200 read as `MAX_PAGE_SIZE` (100) pages from the newest end,
`MAX_PAGE_SIZE` per read,
`RECENT_POSTS_LIMIT` 20 rendered bulletin posts (windowed to 30 when expanded),
`MAX_VISIBLE_TASKS` 16 per page, `MAX_VISIBLE_FINDINGS` 16 per page, pause
queue 64 prompts, wake ledger 512 signals, `REFRESH` 2s, supervisor fallback 600s.

## Known hotspots left open

- **The board's older pages are not reachable yet:** the store can page
  tasks and findings backwards, but `herdr-gpui team task list` and
  `team finding list` take no cursor, and the panel has no show-earlier
  control, so a person can see the newest `MAX_PAGE_SIZE` of each and nothing
  older (finding `271faf33`, store and worker half done).
- **Post-mutation refreshes are unthrottled:** One snapshot is triggered per
  mutation in a burst, bounded by mutation count. Revisit only if mutation
  bursts become common.
- **UI prompt dispatch timing:** UI prompt dispatches currently send
  optimistically alongside async save operations rather than waiting for
  durable write confirmation from the store.
