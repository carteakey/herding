# Herding performance notes

What the team UI costs at scale, which bounds keep it healthy, and where the
measured hotspots are. Numbers below are from one Linux machine (headless
GPUI for frame times, debug-profile store harness for SQLite), so treat them
as relative sizes and ratios, not absolute promises; the benchmark sources
are the stable reference.

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
   dirty — every team event under multi-agent traffic — on the UI thread.
3. **View.** Renders read the prepared snapshot only; the bulletin clips to
   the newest `RECENT_POSTS_LIMIT` (20) posts unless "Show older messages"
   is toggled, and the board folds done tasks under "Completed (N)".

## Measured costs (release, headless, single machine)

| Path | Cost | Notes |
| --- | --- | --- |
| `Bridge::snapshot`, small board | ~17µs/call | 20 posts, 10 tasks, 5 findings |
| `Bridge::snapshot`, at bounds | ~90µs/call | 200 posts, 120 tasks, 60 findings; deep-clones the whole board per dirty flip |
| Threading overhead in `view_posts` | +40% at 200 posts | before the id-map fix: one O(posts) scan per threaded reply |
| Board draw, 64 open tasks + 64 collapsed findings | ~16ms/frame | the dominant frame cost; over a 60fps budget on its own |
| Board draw with done tasks folded | ~10ms/frame | "Completed (N)" compaction matters |
| Bulletin draw, 20-post clip | ~1.9ms/frame | the steady state; cheap by design |
| Bulletin draw, 200-post history expanded | ~22ms/frame | "Show older messages" renders every card; no virtualization |
| Store snapshot at 200 posts | 408 SQL statements, 6–8ms | one `mentions()` + one `recipient_statuses()` per post (debug profile) |
| Store writes | 67–84µs; ack 28–42µs | healthy |

## Fixes applied (low-hanging fruit)

- `bridge/views.rs` builds one member-id → display-name map per projection
  and resolves reply authors through a post-id → author map built in a
  single pass, replacing a linear roster scan per author/recipient/assignee
  and an O(posts²) reply-threading scan. `members()` also computes
  `pane_of` once per member instead of twice. Behavior is pinned by
  `team::bridge::view_tests` (name fallback for departed members, reply
  resolution, offline targets).

## Bounds that keep this healthy

`POST_LIMIT` 200, scan cap 1,000 posts, `MAX_PAGE_SIZE` per read,
`RECENT_POSTS_LIMIT` 20 rendered bulletin posts (200 when expanded),
`MAX_VISIBLE_TASKS`/`MAX_VISIBLE_FINDINGS` 64 each, pause queue 64 prompts,
wake ledger 512 signals, `REFRESH` 2s, supervisor fallback 600s.

## Known hotspots left open

- **Board render cost scales with visible task rows** and there is no list
  virtualization in GPUI here; 60+ open tasks put a full frame over budget.
  Needs element reuse or a bounded visible window before teams grow much
  past the current caps.
- **The expanded bulletin history renders all 200 posts**; same
  virtualization gap, mitigated by the toggle defaulting to closed.
- **Store snapshot is a double N+1** (per-post `mentions` +
  `recipient_statuses` queries). Batched JOINs measured 3–5x faster on the
  same database; `prepare_cached` on the long-lived worker connection is a
  further free win. At the 2s cadence this is ~1% worker duty even under
  stress, so it is worth doing for headroom, not urgency.
- Post-mutation refreshes are unthrottled: one snapshot per mutation in a
  burst, bounded by mutation count. Revisit only if bursts become common.
