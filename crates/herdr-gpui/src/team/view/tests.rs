//! Layout regressions for the team board.

#![allow(clippy::expect_used)]

use super::*;
use crate::config::{FontConfig, Theme};
use crate::team::model::{TeamFinding, TeamLoadState, TeamSnapshot};
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
