//! Layout regressions for the team board.

#![allow(clippy::expect_used)]

use super::*;
use crate::config::{FontConfig, Theme};
use crate::team::model::{
    PaneIdentity, TeamFinding, TeamLoadState, TeamMember, TeamSnapshot, TeamTask,
};
use gpui::{FontWeight, px, size};
use herdr_orchestrator::team_store::{Author, Confidence, TaskStatus};

use core::prelude::v1::test;

fn font() -> FontConfig {
    FontConfig {
        family: "Menlo".into(),
        size: 14.,
        fallbacks: None,
        line_height_multiple: None,
        weight: FontWeight::NORMAL,
    }
}

fn finding(description: &str) -> TeamFinding {
    TeamFinding {
        id: "finding-1".into(),
        author: "argon".into(),
        description: description.to_owned().into(),
        evidence: "A bounded probe exercised the CLI against a sandboxed daemon.".into(),
        confidence: Confidence::High,
        supersedes: None,
    }
}

fn task(id: &str, status: TaskStatus, assignee: Option<&str>, blocker: Option<&str>) -> TeamTask {
    TeamTask {
        id: id.into(),
        title: format!("Task {id}").into(),
        brief: "".into(),
        assignee: assignee.map(Into::into),
        status,
        blocker_reason: blocker.map(Into::into),
        result: None,
    }
}

fn member(id: &str, name: &str, supervisor: bool) -> TeamMember {
    TeamMember {
        member_id: id.into(),
        name: name.into(),
        role: if supervisor {
            "supervisor".into()
        } else {
            "worker".into()
        },
        agent: "zcode".into(),
        pane: None,
        target_pane: None,
        status: "idle".into(),
        supervisor,
        paused: false,
    }
}

fn team_view(
    cx: &mut TestAppContext,
    findings: Vec<TeamFinding>,
) -> (Entity<TeamView>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|_, cx| TeamView::new(font(), Theme::default(), cx));
    view.update(cx, |view, cx| {
        view.snapshot = TeamSnapshot {
            state: TeamLoadState::Ready,
            team_id: Some("team-test".into()),
            team_name: Some("herding".into()),
            findings,
            ..TeamSnapshot::default()
        };
        cx.notify();
    });
    (view, cx)
}

#[gpui::test]
fn task_rows_show_self_describing_icons_for_status_role_and_gate(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| TeamView::new(font(), Theme::default(), cx));
    view.update(cx, |view, cx| {
        view.snapshot = TeamSnapshot {
            state: TeamLoadState::Ready,
            team_id: Some("team-test".into()),
            team_name: Some("herding".into()),
            members: vec![member("m-sup", "argon", true), member("m-w", "kilo", false)],
            tasks: vec![
                task("t-open", TaskStatus::Open, Some("argon"), None),
                task("t-claimed", TaskStatus::InProgress, Some("kilo"), None),
                task(
                    "t-gated",
                    TaskStatus::Blocked,
                    Some("kilo"),
                    Some("waiting on main"),
                ),
                task("t-done", TaskStatus::Done, None, None),
            ],
            ..TeamSnapshot::default()
        };
        view.completed_tasks_open = true;
        cx.notify();
    });
    cx.simulate_resize(size(px(600.), px(900.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    // Every task row lays out its status mark, whatever the state.
    // debug_bounds takes &'static str, so the selectors stay literals.
    assert!(
        cx.debug_bounds("team-task-status-icon-t-open").is_some(),
        "task t-open should lay out its status icon"
    );
    assert!(
        cx.debug_bounds("team-task-status-icon-t-claimed").is_some(),
        "task t-claimed should lay out its status icon"
    );
    assert!(
        cx.debug_bounds("team-task-status-icon-t-gated").is_some(),
        "task t-gated should lay out its status icon"
    );
    assert!(
        cx.debug_bounds("team-task-status-icon-t-done").is_some(),
        "task t-done should lay out its status icon"
    );
    // The role mark follows the assignee's roster role, assigned tasks only.
    assert!(
        cx.debug_bounds("team-task-role-icon-t-open").is_some(),
        "a task with a roster assignee should lay out its role icon"
    );
    assert!(cx.debug_bounds("team-task-role-icon-t-claimed").is_some());
    assert!(
        cx.debug_bounds("team-task-role-icon-t-done").is_none(),
        "an unassigned task should not lay out a role icon"
    );
    // The gate mark appears only where a blocker reason is recorded.
    assert!(
        cx.debug_bounds("team-task-gate-icon-t-gated").is_some(),
        "a task with a blocker reason should lay out its gate icon"
    );
    assert!(
        cx.debug_bounds("team-task-gate-icon-t-open").is_none(),
        "a task without a blocker reason should not lay out a gate icon"
    );
}

#[gpui::test]
fn a_long_finding_ellipsizes_inside_the_sidebar(cx: &mut TestAppContext) {
    let (_view, cx) = team_view(
        cx,
        vec![finding(
            "The finding description is intentionally long enough to wrap inside the narrow \
             team sidebar, which is roughly four hundred logical pixels wide.",
        )],
    );
    cx.simulate_resize(size(px(400.), px(900.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let row = cx
        .debug_bounds("team-finding-row")
        .expect("the finding row should be laid out");
    let description = cx
        .debug_bounds("team-finding-description")
        .expect("the finding summary should be laid out");
    assert!(
        row.size.height <= px(24.),
        "a collapsed finding should stay one line, but its row is {:?} tall",
        row.size.height
    );
    assert!(
        description.right() <= row.right(),
        "the summary overflows the row: {description:?} ends past {row:?}"
    );
    assert!(
        description.size.width < px(320.),
        "the summary should be shortened to the sidebar, not laid out at its full width: \
         {description:?}"
    );
}

#[gpui::test]
fn expanding_a_finding_shows_the_full_description_below_the_row(cx: &mut TestAppContext) {
    let description = "The finding description is intentionally long enough to wrap inside the \
                       narrow team sidebar, which is roughly four hundred logical pixels wide.";
    let (view, cx) = team_view(cx, vec![finding(description)]);
    cx.simulate_resize(size(px(400.), px(900.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let collapsed = cx
        .debug_bounds("team-finding-card")
        .expect("the finding card should be laid out");
    view.update(cx, |view, cx| {
        view.expanded_finding_ids.insert("finding-1".into());
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let expanded = cx
        .debug_bounds("team-finding-card")
        .expect("the finding card should be laid out");
    let row = cx
        .debug_bounds("team-finding-row")
        .expect("the finding row should be laid out");
    let full = cx
        .debug_bounds("team-finding-description")
        .expect("the full description should be laid out");
    assert!(
        expanded.size.height > collapsed.size.height + px(20.),
        "expanding a finding should reveal its description below the row"
    );
    assert!(
        row.size.height <= px(24.),
        "the summary row should stay one line when expanded, but it is {:?} tall",
        row.size.height
    );
    assert!(
        full.size.width > px(300.),
        "the full description should use the card width, not a narrow column: {full:?}"
    );
    assert!(
        full.size.height > px(24.),
        "the full description should wrap onto its own lines: {full:?}"
    );
}

use crate::team::model::TeamPost;

fn post(id: &str, author: &str, body: &str) -> TeamPost {
    TeamPost {
        id: id.into(),
        author: author.into(),
        body: body.into(),
        reply_to_author: None,
        deliveries: Vec::new(),
    }
}

struct BulletinView {
    window: Entity<HerdrWindow>,
}

impl BulletinView {
    fn new(window: Entity<HerdrWindow>, cx: &mut Context<Self>) -> Self {
        let team_view = window.read(cx).team_view.clone();
        cx.observe(&team_view, |_, _, cx| cx.notify()).detach();
        Self { window }
    }
}

impl Render for BulletinView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window.update(cx, |window, cx| {
            window.render_team_bulletin_sidebar(cx).into_any_element()
        })
    }
}

fn bulletin_fixture(
    cx: &mut TestAppContext,
    posts: Vec<TeamPost>,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let mut herdr_window_entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let herdr = cx.new(|cx| {
            let w = crate::sidebar::layout_tests::fixture_window(window, cx);
            w.team_view.update(cx, |view, cx| {
                view.snapshot = TeamSnapshot {
                    state: TeamLoadState::Ready,
                    team_id: Some("team-test".into()),
                    team_name: Some("herding".into()),
                    posts,
                    ..TeamSnapshot::default()
                };
                cx.notify();
            });
            w
        });
        herdr_window_entity = Some(herdr.clone());
        BulletinView::new(herdr, cx)
    });
    let window = herdr_window_entity.expect("window entity initialized");
    (window, cx)
}

#[gpui::test]
fn bulletin_clips_older_messages_and_toggles_expansion(cx: &mut TestAppContext) {
    let posts: Vec<_> = (0..25)
        .map(|i| post(&format!("p{i}"), "agent", &format!("Message number {i}")))
        .collect();
    let (window, cx) = bulletin_fixture(cx, posts);
    cx.simulate_resize(size(px(600.), px(900.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        cx.debug_bounds("bulletin-show-earlier").is_some(),
        "the 'Show older messages' button should be visible when posts exceed RECENT_POSTS_LIMIT"
    );
    assert!(
        cx.debug_bounds("bulletin-post-0").is_none(),
        "oldest post should be hidden by default"
    );
    assert!(
        cx.debug_bounds("bulletin-post-24").is_some(),
        "latest post should be visible"
    );

    window.update(cx, |this, cx| {
        this.team_view.update(cx, |view, cx| {
            view.bulletin_show_all = true;
            cx.notify();
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        cx.debug_bounds("bulletin-collapse-earlier").is_some(),
        "the 'Show recent messages only' button should appear when expanded"
    );
    assert!(
        cx.debug_bounds("bulletin-post-0").is_some(),
        "oldest post should now be visible"
    );

    window.update(cx, |this, cx| {
        this.team_view.update(cx, |view, cx| {
            view.bulletin_show_all = false;
            cx.notify();
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        cx.debug_bounds("bulletin-show-earlier").is_some(),
        "the 'Show older messages' button should be restored"
    );
    assert!(
        cx.debug_bounds("bulletin-post-0").is_none(),
        "oldest post should be hidden again"
    );
}

#[gpui::test]
fn bulletin_auto_scroll_following_and_jump_to_latest(cx: &mut TestAppContext) {
    let posts: Vec<_> = (0..5)
        .map(|i| post(&format!("p{i}"), "agent", &format!("Message number {i}")))
        .collect();
    let (window, cx) = bulletin_fixture(cx, posts);
    cx.simulate_resize(size(px(600.), px(900.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    window.update(cx, |this, cx| {
        let view = this.team_view.read(cx);
        assert!(view.bulletin_following);
    });
    assert!(cx.debug_bounds("bulletin-jump-to-latest").is_none());

    window.update(cx, |this, cx| {
        this.team_view.update(cx, |view, cx| {
            view.bulletin_following = false;
            cx.notify();
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        cx.debug_bounds("bulletin-jump-to-latest").is_some(),
        "'Jump to latest' button should appear when following is paused"
    );

    window.update(cx, |this, cx| {
        this.team_view.update(cx, |view, cx| {
            view.bulletin_following = true;
            cx.notify();
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds("bulletin-jump-to-latest").is_none());
}

#[gpui::test]
fn dense_ui_icon_toolbars_layout_properly(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, cx| TeamView::new(font(), Theme::default(), cx));
    view.update(cx, |view, cx| {
        view.snapshot = TeamSnapshot {
            state: TeamLoadState::Ready,
            team_id: Some("team-test".into()),
            team_name: Some("herding".into()),
            team_goal: Some("Build dense UI with compact icon toolbars".into()),
            members: vec![TeamMember {
                member_id: "m1".into(),
                name: "agyolo2".into(),
                role: "worker".into(),
                agent: "antigravity".into(),
                pane: Some(PaneIdentity {
                    daemon_id: "d1".into(),
                    session_id: Some("s1".into()),
                    workspace_id: "w1".into(),
                    pane_id: "p1".into(),
                }),
                target_pane: None,
                status: "idle".into(),
                supervisor: false,
                paused: false,
            }],
            ..TeamSnapshot::default()
        };
        cx.notify();
    });
    cx.simulate_resize(size(px(600.), px(900.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    // Header toolbar icon buttons
    for id in [
        "team-rename-toggle",
        "team-toggle-pause",
        "team-toggle-bulletin",
        "team-switch",
        "team-delete",
    ] {
        let b = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("header button {id} should be laid out"));
        assert!(
            b.size.width <= px(24.) && b.size.height <= px(24.),
            "header button {id} should be a compact icon button: {b:?}"
        );
    }

    // Goal edit icon button
    let goal_edit = cx
        .debug_bounds("team-goal-edit")
        .expect("goal edit button should be laid out");
    assert!(
        goal_edit.size.width <= px(24.) && goal_edit.size.height <= px(24.),
        "goal edit button should be a compact icon button: {goal_edit:?}"
    );

    // Roster toggle form icon button
    let roster_toggle = cx
        .debug_bounds("team-worker-toggle-form")
        .expect("roster worker toggle button should be laid out");
    assert!(
        roster_toggle.size.width <= px(24.) && roster_toggle.size.height <= px(24.),
        "roster worker toggle button should be a compact icon button: {roster_toggle:?}"
    );

    // Member card toolbar icon buttons
    for id in [
        "team-focus-0",
        "team-nudge-0",
        "team-pause-member-0",
        "team-rename-member-0",
        "team-supervisor-0",
        "team-remove-0",
    ] {
        let b = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("member action button {id} should be laid out"));
        assert!(
            b.size.width <= px(24.) && b.size.height <= px(24.),
            "member action button {id} should be a compact icon button: {b:?}"
        );
    }
}

#[gpui::test]
fn bulletin_composer_renders_input_wrapper_and_submit(cx: &mut TestAppContext) {
    let (window, cx) = bulletin_fixture(cx, vec![post("p1", "agent", "Hello")]);
    window.update(cx, |this, cx| {
        this.team_view.update(cx, |view, cx| {
            view.snapshot.post_author = Some((
                Author::Human {
                    user_id: "human".into(),
                },
                "human".into(),
            ));
            cx.notify();
        });
    });
    cx.simulate_resize(size(px(600.), px(900.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        cx.debug_bounds("bulletin-composer-input").is_some(),
        "the bulletin composer input container with key handling should be laid out"
    );
    assert!(
        cx.debug_bounds("team-post-submit").is_some(),
        "the bulletin post submit button should be laid out"
    );
}
