//! Performance harness for the team store (task 92181e9e, zcode-bee2's half:
//! store query latency and serial worker throughput under multi-pane traffic).
//!
//! Deliberately `#[ignore]`d so regular test runs stay deterministic; run it
//! with:
//! ```text
//! cargo test -p herdr-orchestrator --test team_store_perf -- --ignored --nocapture
//! ```
//! The workload is fixed (no randomness), timings are wall-clock, and every
//! number is reproducible on the same machine state. Two replicas mirror
//! `fn snapshot` in `herdr-gpui`'s `team_worker.rs` statement for statement:
//! `snapshot_replica` is the pre-batching shape (pages of MAX_PAGE_SIZE up to
//! MAX_SCANNED_POSTS, one mentions() per post, one recipient_statuses() per
//! returned post), and `snapshot_replica_batched` is the shape the worker uses
//! now (mentions in the page query, one status query per STATUS_BATCH_CHUNK
//! ids). Their difference is what one team panel refresh saved.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use herdr_orchestrator::team_store::{
    Author, MAX_MEMBERS, MAX_PAGE_SIZE, RecipientStatus, Recipients, STATUS_BATCH_CHUNK,
    TaskStatus, Team, TeamStore,
};
use rusqlite::{Connection, OpenFlags};
use std::time::{Duration, Instant};

const SNAPSHOT_LIMIT: usize = 200;
/// The window the pre-fix worker scanned before it dropped all but its page.
const MAX_SCANNED_POSTS: usize = 1000;

struct Scale {
    label: &'static str,
    members: usize,
    posts: usize,
    tasks: usize,
    findings: usize,
}

const REALISTIC: Scale = Scale {
    label: "realistic (9 members, 200 posts, 40 tasks, 20 findings)",
    members: 9,
    posts: 200,
    tasks: 40,
    findings: 20,
};

const STRESS: Scale = Scale {
    label: "stress (64 members, 1000 posts, 100 tasks, 100 findings)",
    members: MAX_MEMBERS,
    posts: MAX_SCANNED_POSTS,
    tasks: 100,
    findings: 100,
};

/// More posts than the old scan window reached, so a refresh that has to walk
/// history pays for all of it and a newest-page read does not.
const DEEP: Scale = Scale {
    label: "deep history (9 members, 4000 posts, 40 tasks, 20 findings)",
    members: 9,
    posts: 4000,
    tasks: 40,
    findings: 20,
};

fn member_id(index: usize) -> String {
    format!("m{index:04}")
}

/// The store's `prepare` requires creating its own 0700 directory, so the
/// database lives in a nested path under the tempdir.
fn db_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("store").join("teams.db")
}

fn store_in(dir: &std::path::Path) -> TeamStore {
    TeamStore::open(&db_path(dir)).expect("store opens")
}

/// Seeds the exact workload for `scale` and returns the team id.
fn seed(store: &mut TeamStore, scale: &Scale) -> String {
    let team_id = "team-perf";
    let mut team = Team {
        team_id: team_id.into(),
        name: "perf".into(),
        supervisor: member(0),
        members: Vec::new(),
    };
    for index in 1..scale.members {
        team.members.push(member(index));
    }
    store.save_team(&team).expect("team saved");
    store
        .set_team_goal(team_id, "benchmark goal")
        .expect("goal");

    for task in 0..scale.tasks {
        let status = match task % 5 {
            0 => TaskStatus::Open,
            1 => TaskStatus::InProgress,
            2 => TaskStatus::Blocked,
            3 => TaskStatus::Done,
            _ => TaskStatus::Cancelled,
        };
        store
            .create_task(
                &format!("task{task:04}"),
                team_id,
                &Author::Human {
                    user_id: "human".into(),
                },
                &format!("Task {task}"),
                "Do the thing with a reasonably long brief so rows carry realistic weight.",
                Some(&member_id(task % scale.members)),
            )
            .expect("task created");
        if matches!(status, TaskStatus::Done) {
            let assignee = member_id(task % scale.members);
            store
                .claim_task(team_id, &format!("task{task:04}"), &assignee)
                .expect("task claimed");
            store
                .complete_task(
                    team_id,
                    &format!("task{task:04}"),
                    &Author::Member {
                        member_id: member_id(task % scale.members),
                    },
                    "Finished; results recorded for the supervisor to review.",
                )
                .expect("task completed");
        }
    }

    let body = "Status update with a realistic amount of text so post rows are not \
                artificially tiny: progress is steady, next step is the review pass.";
    for post in 0..scale.posts {
        // Every 5th post mentions one or two members; the rest are broadcast
        // authored by members, which the store persists as explicit rows.
        let recipients = match post % 5 {
            0 => Recipients::Explicit(vec![member_id(1)]),
            1 => Recipients::Explicit(vec![member_id(1), member_id(2)]),
            _ => Recipients::Explicit(vec![member_id(post % scale.members)]),
        };
        let author = if post % 10 == 9 {
            Author::Human {
                user_id: "human".into(),
            }
        } else {
            Author::Member {
                member_id: member_id(post % scale.members),
            }
        };
        store
            .add_post_with_recipients(
                &format!("post{post:05}"),
                team_id,
                &author,
                None,
                body,
                &recipients,
            )
            .expect("post added");
    }

    for finding in 0..scale.findings {
        store
            .add_finding(
                &format!("find{finding:04}"),
                team_id,
                &Author::Member {
                    member_id: member_id(finding % scale.members),
                },
                None,
                "Finding: the measured path regressed",
                "Evidence: reproducible timing rows attached to this report",
                herdr_orchestrator::team_store::Confidence::Medium,
                None,
            )
            .expect("finding added");
    }

    team_id.to_owned()
}

fn member(index: usize) -> herdr_orchestrator::team_store::Member {
    herdr_orchestrator::team_store::Member {
        member_id: member_id(index),
        display_name: format!("agent{index}"),
        target: herdr_orchestrator::team_store::SessionTarget {
            host: "local".into(),
            session: format!("sess-{index}"),
            workspace: Some("ws".into()),
            pane: Some(format!("pane-{index}")),
        },
    }
}

fn time_once<T>(run: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let value = run();
    (value, start.elapsed())
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples
        .get(samples.len() / 2)
        .copied()
        .unwrap_or(Duration::ZERO)
}

fn report(label: &str, samples: &[Duration], extra: &str) {
    let best = samples.iter().min().copied().unwrap_or(Duration::ZERO);
    let mid = median(samples.to_vec());
    println!("{label:<58} best {best:>10.3?}  median {mid:>10.3?}  {extra}");
}

/// The worker's snapshot sequence from `team_worker.rs`, statement for
/// statement, so the numbers are the per-refresh cost of one team panel.
fn snapshot_replica(store: &mut TeamStore, team_id: &str) -> (Duration, usize) {
    let mut statements = 0usize;
    let start = Instant::now();
    let team = store.team(team_id).expect("team");
    statements += 2; // team row + members
    let _goal = store.team_goal(team_id);
    statements += 1;
    let mut scanned = Vec::new();
    let mut cursor = 0i64;
    while scanned.len() < MAX_SCANNED_POSTS {
        let page = store.posts(team_id, cursor, MAX_PAGE_SIZE).expect("page");
        statements += 1;
        statements += page.len(); // one mentions() query per post row
        let Some((last, _)) = page.last() else { break };
        cursor = *last;
        let full = page.len() == MAX_PAGE_SIZE;
        scanned.extend(page);
        if !full {
            break;
        }
    }
    let skip = scanned.len().saturating_sub(SNAPSHOT_LIMIT);
    let mut returned = 0usize;
    for (_, post) in scanned.into_iter().skip(skip) {
        let _statuses = store.recipient_statuses(&post.post_id).expect("statuses");
        statements += 1;
        returned += 1;
    }
    let _tasks = store.tasks(team_id, MAX_PAGE_SIZE).expect("tasks");
    let _findings = store.findings(team_id, MAX_PAGE_SIZE).expect("findings");
    statements += 2;
    let _ = team;
    let _ = returned;
    (start.elapsed(), statements)
}

/// The worker's current snapshot: it reads its page from the newest end
/// (`latest_posts`, page-sized slices), each page carries its mentions in the
/// same query, and the returned page's statuses come back one query per id
/// chunk. The cost is the page count, not the team's history.
fn snapshot_replica_batched(store: &mut TeamStore, team_id: &str) -> (Duration, usize) {
    let mut statements = 0usize;
    let start = Instant::now();
    let team = store.team(team_id).expect("team");
    statements += 2; // team row + members
    let _goal = store.team_goal(team_id);
    statements += 1;
    let mut pages: Vec<Vec<(i64, herdr_orchestrator::team_store::Post)>> = Vec::new();
    let mut remaining = SNAPSHOT_LIMIT;
    while remaining > 0 {
        let want = remaining.min(MAX_PAGE_SIZE);
        let before = pages
            .last()
            .and_then(|page| page.first())
            .map(|(sequence, _)| *sequence);
        let page = store
            .latest_posts(team_id, before, want)
            .expect("newest page");
        let got = page.len();
        remaining = remaining.saturating_sub(got);
        statements += 1;
        pages.push(page);
        if got < want {
            break;
        }
    }
    let returned: Vec<(i64, herdr_orchestrator::team_store::Post)> =
        pages.into_iter().rev().flatten().collect();
    let ids: Vec<String> = returned
        .iter()
        .map(|(_, post)| post.post_id.clone())
        .collect();
    let statuses = store.recipient_statuses_for_posts(&ids).expect("statuses");
    statements += ids.len().div_ceil(STATUS_BATCH_CHUNK).max(1);
    let _tasks = store.tasks(team_id, MAX_PAGE_SIZE).expect("tasks");
    let _findings = store.findings(team_id, MAX_PAGE_SIZE).expect("findings");
    statements += 2;
    assert_eq!(
        statuses.len(),
        returned.len(),
        "one entry per returned post"
    );
    // Cost is only half the point: the page has to be the team's latest
    // conversation. This reads the newest post separately and is not counted in
    // the statement total.
    let newest = store
        .latest_posts(team_id, None, 1)
        .expect("newest post")
        .last()
        .map(|(_, post)| post.post_id.clone());
    assert_eq!(
        returned.last().map(|(_, post)| post.post_id.clone()),
        newest,
        "the shipped page reaches the team's newest post"
    );
    let _ = team;
    (start.elapsed(), statements)
}

/// One batched query standing in for the per-post mentions() N+1, measured on
/// the same database file to size the headroom a JOIN rewrite would buy.
fn batched_mentions_join(path: &std::path::Path, team_id: &str, limit: usize) -> Duration {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("read-only open");
    let start = Instant::now();
    let mut stmt = db
        .prepare(
            "SELECT p.sequence, p.post_id, m.recipient_id FROM posts p
             LEFT JOIN mentions m ON m.post_id = p.post_id
             WHERE p.team_id = ?1 AND p.sequence > 0
             ORDER BY p.sequence LIMIT ?2",
        )
        .expect("prepare join");
    let rows: Vec<(i64, String, Option<String>)> = stmt
        .query_map(rusqlite::params![team_id, limit as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .expect("query join")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert!(!rows.is_empty(), "join returned rows");
    start.elapsed()
}

/// Same shape as the snapshot's per-post recipient_statuses N+1, batched.
fn batched_statuses_join(path: &std::path::Path, team_id: &str) -> Duration {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("read-only open");
    let start = Instant::now();
    let mut stmt = db
        .prepare(
            "SELECT p.post_id, m.recipient_id, a.acknowledged_at FROM posts p
             JOIN mentions m ON m.post_id = p.post_id
             LEFT JOIN acknowledgements a
                    ON a.post_id = m.post_id AND a.recipient_id = m.recipient_id
             WHERE p.team_id = ?1 AND p.sequence > 0
             ORDER BY p.sequence LIMIT 200",
        )
        .expect("prepare join");
    let rows: Vec<(String, String, Option<String>)> = stmt
        .query_map([team_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query join")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert!(!rows.is_empty(), "join returned rows");
    start.elapsed()
}

fn audit_scale(scale: &Scale) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = db_path(dir.path());
    let mut store = store_in(dir.path());
    let team_id = seed(&mut store, scale);
    println!("\n=== {} ===", scale.label);

    // Reads: the snapshot replica dominates every refresh.
    let mut snapshots = Vec::new();
    let mut statements = 0usize;
    for _ in 0..7 {
        let (elapsed, stmts) = snapshot_replica(&mut store, &team_id);
        snapshots.push(elapsed);
        statements = stmts;
    }
    report(
        "worker snapshot replica (pre-batching, modelled)",
        &snapshots,
        "",
    );
    println!(
        "{:>58} {} SQL statements per snapshot (mentions() + recipient_statuses() per post)",
        "", statements
    );
    let team_newest = store
        .latest_posts(&team_id, None, 1)
        .expect("newest post")
        .last()
        .map(|(_, post)| post.post_id.clone());
    let window_newest = store
        .latest_posts(&team_id, Some(MAX_SCANNED_POSTS as i64 + 1), 1)
        .expect("window end")
        .last()
        .map(|(_, post)| post.post_id.clone());
    println!(
        "{:>58} the scanned window ended at {window_newest:?}; the team's newest post is {team_newest:?}",
        ""
    );

    let mut batched = Vec::new();
    let mut batched_statements = 0usize;
    for _ in 0..7 {
        let (elapsed, stmts) = snapshot_replica_batched(&mut store, &team_id);
        batched.push(elapsed);
        batched_statements = stmts;
    }
    report("worker refresh replica (batched, as shipped)", &batched, "");
    println!(
        "{:>58} {} SQL statements per snapshot (was {})",
        "", batched_statements, statements
    );

    let mut pages = Vec::new();
    for _ in 0..7 {
        let (_, elapsed) = time_once(|| store.posts(&team_id, 0, MAX_PAGE_SIZE).expect("page"));
        pages.push(elapsed);
    }
    report("posts page of 100 (mentions in the same query)", &pages, "");

    let join_mentions = batched_mentions_join(&path, &team_id, 1200);
    println!(
        "{:<58} best {:>10.3?}  (one batched JOIN for the same page)",
        "batched mentions JOIN (single query)", join_mentions
    );
    let join_statuses = batched_statuses_join(&path, &team_id);
    println!(
        "{:<58} best {:>10.3?}  (mentions+acks batched for 200 posts)",
        "batched statuses JOIN (single query)", join_statuses
    );

    let mut statuses = Vec::new();
    for _ in 0..7 {
        let (_, elapsed) = time_once(|| store.recipient_statuses("post00000").expect("statuses"));
        statuses.push(elapsed);
    }
    report("recipient_statuses for one post", &statuses, "");

    let mut lists = Vec::new();
    for _ in 0..7 {
        let (_, elapsed) = time_once(|| store.tasks(&team_id, MAX_PAGE_SIZE).expect("tasks"));
        lists.push(elapsed);
    }
    report("tasks page of 100", &lists, "");

    // The same page count from the newest end: same cost, and on a board
    // bigger than a page it names the tasks a team is actually working on.
    let mut newest_tasks = Vec::new();
    let mut newest_start = String::new();
    for _ in 0..7 {
        let (page, elapsed) = time_once(|| {
            store
                .latest_tasks(&team_id, None, MAX_PAGE_SIZE)
                .expect("newest tasks")
        });
        newest_tasks.push(elapsed);
        newest_start = page
            .first()
            .map(|(_, task)| task.title.clone())
            .unwrap_or_default();
    }
    let oldest_end = store
        .tasks(&team_id, MAX_PAGE_SIZE)
        .expect("tasks")
        .last()
        .map(|task| task.title.clone())
        .unwrap_or_default();
    report(
        "newest tasks page of 100",
        &newest_tasks,
        &format!("starts at {newest_start}; the oldest-first page ended at {oldest_end}"),
    );

    let mut counts = Vec::new();
    for _ in 0..7 {
        let (_, elapsed) =
            time_once(|| store.task_counts(&team_id, &member_id(1)).expect("counts"));
        counts.push(elapsed);
    }
    report(
        "task_counts (aggregate, not a page)",
        &counts,
        &format!(
            "{} open of {} tasks",
            store
                .task_counts(&team_id, &member_id(1))
                .expect("counts")
                .open,
            scale.tasks
        ),
    );

    let mut newest_findings = Vec::new();
    for _ in 0..7 {
        let (page, elapsed) = time_once(|| {
            store
                .latest_findings(&team_id, None, MAX_PAGE_SIZE)
                .expect("newest findings")
        });
        newest_findings.push(elapsed);
        let _ = page.last().map(|(_, finding)| finding.description.clone());
    }
    report("newest findings page of 100", &newest_findings, "");

    // Writes: the serial worker's per-op floor.
    let mut adds = Vec::new();
    for index in 0..20 {
        let (_, elapsed) = time_once(|| {
            store
                .add_post_with_recipients(
                    &format!("bench{index:04}"),
                    &team_id,
                    &Author::Member {
                        member_id: member_id(1),
                    },
                    None,
                    "Bench write body long enough to be honest about row size.",
                    &Recipients::Explicit(vec![member_id(2)]),
                )
                .expect("write");
        });
        adds.push(elapsed);
    }
    report("add_post_with_recipients (one member mention)", &adds, "");

    let mut acks = Vec::new();
    for index in 0..20 {
        let (_, elapsed) = time_once(|| {
            let _ = store.acknowledge(&format!("post{index:05}"), &member_id(1));
        });
        acks.push(elapsed);
    }
    report(
        "acknowledge (membership + mention checks + insert)",
        &acks,
        "",
    );

    // Serial mixed load: one write plus one full snapshot per round, the
    // worst realistic refresh collision on the single worker thread.
    let rounds = 5;
    let (rounds_done, elapsed) = time_once(|| {
        for round in 0..rounds {
            store
                .add_post_with_recipients(
                    &format!("mixed{round:04}"),
                    &team_id,
                    &Author::Member {
                        member_id: member_id(3),
                    },
                    None,
                    "Mixed-load write colliding with a refresh.",
                    &Recipients::Explicit(vec![member_id(4)]),
                )
                .expect("write");
            snapshot_replica(&mut store, &team_id);
        }
        rounds
    });
    println!(
        "{:<58} {:.1} ms per write+snapshot round ({} rounds)",
        "serial round: add_post + full snapshot",
        elapsed.as_secs_f64() * 1000.0 / rounds_done as f64,
        rounds_done
    );

    // Unread backlog: how many of the seeded mentions are still pending.
    let pending = store
        .recipient_statuses("post00000")
        .expect("statuses")
        .into_iter()
        .filter(|(_, status)| matches!(status, RecipientStatus::Pending))
        .count();
    println!("{:>58} {} pending on a mentioned post", "", pending);
}

#[test]
#[ignore = "perf harness: cargo test -p herdr-orchestrator --test team_store_perf -- --ignored --nocapture"]
fn team_store_perf_audit() {
    println!("team_store perf audit (timings are wall-clock; run on an idle machine)");
    audit_scale(&REALISTIC);
    audit_scale(&STRESS);
    audit_scale(&DEEP);
}
