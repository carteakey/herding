//! Headless frame-time evidence for the team bulletin feed and task board.
//!
//! Wall-clock numbers are machine dependent and only printed; run with
//! `cargo test --release -p herdr-gpui perf -- --nocapture`. CI assertions
//! stay limited to the draw succeeding, never to a timing bound.

use std::time::{Duration, Instant};

use gpui::{FontWeight, px, size};

use super::*;
use crate::config::{FontConfig, Theme};
use crate::sidebar::layout_tests::fixture_window;
use crate::team::model::{
    DeliveryState, TeamFinding, TeamLoadState, TeamPost, TeamSnapshot, TeamTask,
};
use herdr_orchestrator::team_store::{Confidence, TaskStatus};

// `super::*` re-exports gpui's `test` macro, which would otherwise swallow
// the builtin `#[test]` the gpui test harness generates.
use core::prelude::v1::test;

use super::bulletin::RECENT_POSTS_LIMIT;

/// Renders the real bulletin sidebar through the window, as the app does.
struct BulletinPane {
    window: Entity<HerdrWindow>,
}

impl Render for BulletinPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window.update(cx, |window, cx| {
            window.render_team_bulletin_sidebar(cx).into_any_element()
        })
    }
}

fn font() -> FontConfig {
    FontConfig {
        family: "Menlo".into(),
        size: 14.,
        fallbacks: None,
        line_height_multiple: None,
        weight: FontWeight::NORMAL,
    }
}

fn post(index: usize, body: &str) -> TeamPost {
    TeamPost {
        id: format!("post-{index}"),
        author: format!("agent-{}", index % 4).into(),
        body: body.into(),
        reply_to_author: (index % 4 == 1 && index > 0)
            .then(|| format!("agent-{}", (index - 1) % 4).into()),
        deliveries: (0..3)
            .map(|i| {
                (
                    format!("m{i}"),
                    format!("agent-{i}").into(),
                    DeliveryState::Pending,
                )
            })
            .collect(),
    }
}

fn task(index: usize, done: bool) -> TeamTask {
    TeamTask {
        id: format!("task-{index}"),
        title: format!("Task {index}: investigate something specific").into(),
        brief: format!("Brief {index}: describe the steps the assigned agent should take.").into(),
        assignee: Some(format!("agent-{}", index % 4).into()),
        status: if done {
            TaskStatus::Done
        } else {
            TaskStatus::InProgress
        },
        blocker_reason: None,
        result: done.then(|| format!("Result {index}: outcome, checks run, and limits.").into()),
    }
}

fn finding(index: usize) -> TeamFinding {
    TeamFinding {
        id: format!("finding-{index}"),
        author: format!("agent-{}", index % 4).into(),
        description: format!("Finding {index}: what was observed and why it matters.").into(),
        evidence: format!(
            "Evidence {index}: the exact commands run and their output, bounded to a \
             realistic paragraph of text for the benchmark."
        )
        .into(),
        confidence: Confidence::Medium,
        supersedes: None,
    }
}

/// Median wall time of a full draw, warmed by two discarded draws first.
fn median_draw_times(cx: &mut VisualTestContext, draws: usize) -> Duration {
    let mut times = Vec::new();
    cx.update(|window, cx| {
        for i in 0..draws + 2 {
            let started = Instant::now();
            window.draw(cx).clear(cx);
            if i >= 2 {
                times.push(started.elapsed());
            }
        }
    });
    times.sort_unstable();
    times[times.len() / 2]
}

fn board_snapshot(done_count: usize) -> TeamSnapshot {
    TeamSnapshot {
        state: TeamLoadState::Ready,
        team_id: Some("team-test".into()),
        team_name: Some("herding".into()),
        tasks: (0..64).map(|i| task(i, i < done_count)).collect(),
        findings: (0..64).map(finding).collect(),
        ..TeamSnapshot::default()
    }
}

#[gpui::test]
fn board_frame_cost_with_a_full_backlog(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| TeamView::new(font(), Theme::default(), cx));
    view.update(cx, |view, cx| {
        view.snapshot = board_snapshot(0);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let open = median_draw_times(cx, 9);

    view.update(cx, |view, cx| {
        view.snapshot = board_snapshot(40);
        cx.notify();
    });
    let done = median_draw_times(cx, 9);

    println!(
        "board draw — 64 open tasks + 64 collapsed findings: {open:?}; \
         same board with 40 done (Completed (40) folded): {done:?}"
    );
}

#[gpui::test]
fn bulletin_frame_cost_steady_state_and_expanded_history(cx: &mut TestAppContext) {
    let body = "A realistic chat message with enough words to exercise text layout and \
         wrapping at the 400px sidebar width, so the measurement means something. ";
    let mut draw_times = Vec::new();

    for (post_count, show_all, label) in [
        (RECENT_POSTS_LIMIT, false, "20-post live-stream clip"),
        (MAX_VISIBLE_POSTS, true, "200-post history expanded"),
    ] {
        let (_pane, cx) = cx.add_window_view(|window, cx| {
            let herdr = cx.new(|cx| {
                let w = fixture_window(window, cx);
                w.team_view.update(cx, |view, cx| {
                    view.snapshot = TeamSnapshot {
                        state: TeamLoadState::Ready,
                        team_id: Some("team-test".into()),
                        team_name: Some("herding".into()),
                        posts: (0..post_count).map(|i| post(i, &body.repeat(3))).collect(),
                        ..TeamSnapshot::default()
                    };
                    view.bulletin_show_all = show_all;
                    cx.notify();
                });
                w
            });
            BulletinPane { window: herdr }
        });
        cx.simulate_resize(size(px(600.), px(900.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        draw_times.clear();
        cx.update(|window, cx| {
            for i in 0..11 {
                let started = Instant::now();
                window.draw(cx).clear(cx);
                if i >= 2 {
                    draw_times.push(started.elapsed());
                }
            }
        });
        draw_times.sort_unstable();
        let median = draw_times[draw_times.len() / 2];
        println!("bulletin draw — {label}: {median:?}");
    }
}

#[gpui::test]
fn board_disclosure_toggles_reach_older_rows(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| TeamView::new(font(), Theme::default(), cx));
    view.update(cx, |view, cx| {
        view.snapshot = board_snapshot(0);
        cx.notify();
    });
    cx.simulate_resize(size(px(600.), px(900.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        cx.debug_bounds("team-task-task-0").is_some(),
        "first task should be visible"
    );
    assert!(
        cx.debug_bounds("team-task-task-15").is_some(),
        "16th task should be visible"
    );
    assert!(
        cx.debug_bounds("team-task-task-16").is_none(),
        "tasks beyond the window should be hidden initially"
    );
    assert!(
        cx.debug_bounds("team-tasks-active-toggle").is_some(),
        "active tasks toggle should be rendered"
    );

    view.update(cx, |view, cx| {
        view.active_tasks_show_all = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        cx.debug_bounds("team-task-task-63").is_some(),
        "oldest task should become visible when expanded"
    );

    assert!(
        cx.debug_bounds("team-finding-finding-0").is_some(),
        "first finding should be visible"
    );
    assert!(
        cx.debug_bounds("team-finding-finding-16").is_none(),
        "findings beyond the window should be hidden initially"
    );
    assert!(
        cx.debug_bounds("team-findings-show-more").is_some(),
        "findings show more toggle should be rendered"
    );

    view.update(cx, |view, cx| {
        view.findings_show_all = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        cx.debug_bounds("team-finding-finding-63").is_some(),
        "oldest finding should become visible when expanded"
    );
}
