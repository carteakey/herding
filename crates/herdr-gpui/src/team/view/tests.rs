//! Layout regressions for the team board.

#![allow(clippy::expect_used)]

use super::*;
use crate::config::{FontConfig, Theme};
use crate::team::model::{PaneIdentity, TeamFinding, TeamLoadState, TeamMember, TeamSnapshot};
use gpui::{FontWeight, px, size};
use herdr_orchestrator::team_store::Confidence;

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
