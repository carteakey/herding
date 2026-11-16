//! Timing evidence for `Bridge::snapshot` projections.
//!
//! These tests are evidence, not gates: wall-clock numbers are
//! machine dependent, so they are only printed. Run with
//! `cargo test --release -p sheprd-gpui perf -- --nocapture` to collect
//! citable numbers; the debug suite only proves the fixtures build and the
//! projections stay correct at scale.

use std::time::{Duration, Instant};

use sheprd_orchestrator::team_store::{
    Author, Confidence, Finding, Member, Post, RecipientStatus, SessionTarget, Task, TaskStatus,
    Team,
};

use super::*;
use crate::i18n::fl;
use crate::team::model::{DeliveryState, TeamPost};

fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(secs, 0).unwrap_or(chrono::DateTime::UNIX_EPOCH)
}

fn member(id: &str) -> Member {
    Member {
        member_id: id.to_owned(),
        display_name: format!("agent-{id}"),
        target: SessionTarget {
            host: "local".into(),
            session: format!("sess-{id}"),
            workspace: Some("ws".into()),
            pane: Some(format!("pane-{id}")),
        },
    }
}

fn team(member_count: usize) -> Team {
    Team {
        team_id: "perf-team".into(),
        name: "perf".into(),
        supervisor: member("sup"),
        members: (0..member_count)
            .map(|i| member(&format!("m{i}")))
            .collect(),
    }
}

fn post(index: usize, reply_to: Option<String>) -> (Post, Vec<(String, RecipientStatus)>) {
    let post = Post {
        post_id: format!("post-{index}"),
        team_id: "perf-team".into(),
        author: Author::Member {
            member_id: format!("m{}", index % 4),
        },
        reply_to,
        body: format!(
            "Message {index} with a realistic amount of body text so clones carry \
             actual bytes, roughly one hundred characters long. {index}"
        ),
        mentions: Vec::new(),
        created_at: at(index as i64),
    };
    let deliveries = (0..3)
        .map(|i| (format!("m{i}"), RecipientStatus::Pending))
        .collect();
    (post, deliveries)
}

fn task(index: usize) -> Task {
    Task {
        task_id: format!("task-{index}"),
        team_id: "perf-team".into(),
        creator: Author::Member {
            member_id: "sup".into(),
        },
        title: format!("Task {index}: investigate something specific"),
        brief: format!("Brief {index}: describe the steps the assigned agent should take."),
        assignee: Some(format!("m{}", index % 4)),
        status: TaskStatus::Done,
        blocker_reason: None,
        result: Some(format!(
            "Result {index}: the outcome, the checks actually run, and the limits."
        )),
        created_at: at(index as i64),
        updated_at: at(index as i64 + 1),
    }
}

fn finding(index: usize) -> Finding {
    Finding {
        finding_id: format!("finding-{index}"),
        team_id: "perf-team".into(),
        author: Author::Member {
            member_id: format!("m{}", index % 4),
        },
        task_id: None,
        description: format!("Finding {index}: what was observed and why it matters."),
        evidence: format!(
            "Evidence {index}: the exact commands run and their output, bounded to a \
             realistic paragraph of text for the benchmark."
        ),
        confidence: Confidence::Medium,
        supersedes: None,
        created_at: at(index as i64),
    }
}

/// A busy board at the store's snapshot bounds: POST_LIMIT (200) posts, a
/// typical task backlog, and a findings history.
fn busy_board(post_count: usize, task_count: usize, finding_count: usize) -> Bridge {
    let mut posts = Vec::new();
    for i in 0..post_count {
        // Every fourth post threads onto its predecessor, like real chatter.
        let reply_to = (i % 4 == 1 && i > 0).then(|| format!("post-{}", i - 1));
        posts.push(post(i, reply_to));
    }
    Bridge {
        started: true,
        team: Some(team(6)),
        posts,
        tasks: (0..task_count).map(task).collect(),
        findings: (0..finding_count).map(finding).collect(),
        ..Bridge::default()
    }
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort_unstable();
    times[times.len() / 2]
}

fn median_snapshot_time(bridge: &Bridge, runs: usize) -> Duration {
    let mut times = Vec::new();
    for _ in 0..runs {
        let started = Instant::now();
        let snapshot = bridge.snapshot(&[]);
        times.push(started.elapsed());
        // Keep the projection honest: the fixture must actually project.
        assert!(!snapshot.posts.is_empty());
    }
    median(times)
}

#[test]
fn snapshot_time_scales_with_the_board() {
    // Warm fluent lookups and any lazy global state before measuring.
    let warmup = busy_board(4, 2, 2);
    let _ = median_snapshot_time(&warmup, 1);

    let small = busy_board(20, 10, 5);
    let large = busy_board(200, 120, 60);

    let small_median = median_snapshot_time(&small, 7);
    let large_median = median_snapshot_time(&large, 7);
    println!(
        "snapshot per call — 20 posts/10 tasks/5 findings: {small_median:?}; \
         200 posts/120 tasks/60 findings: {large_median:?} \
         (each call deep-clones the whole board onto the UI thread)"
    );
}

/// The reply-resolution shape `view_posts` used before the projection fix:
/// one linear scan of every post per reply, plus one roster scan per author
/// and per delivery. Kept as a baseline replica so the measured payoff stays
/// reproducible instead of being a claim in a commit message.
fn legacy_view_posts(
    team: &Team,
    posts: &[(Post, Vec<(String, RecipientStatus)>)],
) -> Vec<TeamPost> {
    let name = |id: &str| -> SharedString {
        roster(team)
            .find(|member| member.member_id == id)
            .map_or_else(|| id.to_owned(), |member| member.display_name.clone())
            .into()
    };
    let author = |author: &Author| match author {
        Author::Human { .. } => SharedString::from(fl!("team-author-human")),
        Author::Member { member_id } => name(member_id),
    };
    let roster_ids: HashSet<&str> = roster(team)
        .map(|member| member.member_id.as_str())
        .collect();
    posts
        .iter()
        .map(|(post, statuses)| TeamPost {
            id: post.post_id.clone(),
            author: author(&post.author),
            body: post.body.clone().into(),
            reply_to_author: post.reply_to.as_ref().and_then(|id| {
                posts
                    .iter()
                    .find(|(other, _)| other.post_id == *id)
                    .map(|(other, _)| author(&other.author))
            }),
            deliveries: statuses
                .iter()
                .filter(|(recipient, _)| roster_ids.contains(recipient.as_str()))
                .map(|(recipient, status)| {
                    let state = match status {
                        RecipientStatus::Pending => DeliveryState::Pending,
                        RecipientStatus::Acknowledged(_) => DeliveryState::Acknowledged,
                    };
                    (recipient.clone(), name(recipient), state)
                })
                .collect(),
        })
        .collect()
}

fn median_projection(bridge: &Bridge, legacy: bool) -> Duration {
    let Some(team) = bridge.team.as_ref() else {
        panic!("the fixture carries a team");
    };
    let mut times = Vec::new();
    for _ in 0..7 {
        let started = Instant::now();
        let projected = if legacy {
            legacy_view_posts(team, &bridge.posts)
        } else {
            view_posts(team, &bridge.posts)
        };
        times.push(started.elapsed());
        assert!(!projected.is_empty());
    }
    median(times)
}

#[test]
fn reply_threading_stays_flat_after_the_projection_fix() {
    // Warm up outside measurement.
    let warmup = busy_board(4, 0, 0);
    let _ = median_snapshot_time(&warmup, 1);

    for count in [100, 200] {
        let flat = busy_board(count, 0, 0);
        let mut threaded = busy_board(count, 0, 0);
        for (i, (post, _)) in threaded.posts.iter_mut().enumerate() {
            post.reply_to = (i > 0).then(|| format!("post-{}", i - 1));
        }

        let flat_median = median_snapshot_time(&flat, 7);
        let threaded_median = median_snapshot_time(&threaded, 7);
        let legacy_median = median_projection(&threaded, true);
        let current_median = median_projection(&threaded, false);
        let speedup =
            legacy_median.as_secs_f64() / current_median.as_secs_f64().max(f64::MIN_POSITIVE);
        println!(
            "{count} posts: flat {flat_median:?}, every post threaded {threaded_median:?} \
             (one-pass author map); legacy rescan {legacy_median:?}, current \
             projection {current_median:?} ({speedup:.1}x)"
        );
    }
}
