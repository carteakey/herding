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
   mutation, an `Operation::Snapshot` reads the team from SQLite: up to
   1,000 scanned posts (hard cap), the last `POST_LIMIT` (200) returned,
   plus all tasks and findings up to `MAX_PAGE_SIZE`.
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
| Store snapshot at 200 posts | 408 statements, 6–8ms | 9 statements, 1.87ms median | batched recipient queries + indexed mentions |
| Store snapshot stress (1,000 posts) | 1,215 statements, 16–21ms | 16 statements, 6.84ms median | 98.7% statement reduction |
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
- **Bridge snapshot projection caching (commit `1fef43d1`):**
  - Built a single-pass `name_map` and author map during `Bridge::snapshot`.
  - Resolved reply-to authors in O(1) via hash map instead of rescanning
    all posts iteratively.
  - Cached member pane resolution (`pane_of`) per member.

## Bounds that keep this healthy

`POST_LIMIT` 200, scan cap 1,000 posts, `MAX_PAGE_SIZE` per read,
`RECENT_POSTS_LIMIT` 20 rendered bulletin posts (windowed to 30 when expanded),
`MAX_VISIBLE_TASKS` 16 per page, `MAX_VISIBLE_FINDINGS` 16 per page, pause
queue 64 prompts, wake ledger 512 signals, `REFRESH` 2s, supervisor fallback 600s.

## Known hotspots left open

- **Scanning the 1,000-post window on refresh (`e9a9dbab`):** The store worker
  snapshot still scans 10 pages (~5.4ms out of a 6.8ms stress refresh) to
  collect the newest 200 posts before discarding earlier pages. Reading the
  newest limit posts directly will eliminate most of that scan time.
- **Post-mutation refreshes are unthrottled:** One snapshot is triggered per
  mutation in a burst, bounded by mutation count. Revisit only if mutation
  bursts become common.
- **UI prompt dispatch timing:** UI prompt dispatches currently send
  optimistically alongside async save operations rather than waiting for
  durable write confirmation from the store.
