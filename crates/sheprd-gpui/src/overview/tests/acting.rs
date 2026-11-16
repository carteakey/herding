//! Selecting rows, folding idle ones, and replying, as a user does them.

use super::page::{bounds, click, draw, fixture, row};
use crate::{
    SheprdWindow,
    overview::{Marks, State},
};
use gpui::{Entity, Modifiers, VisualTestContext};
use sheprd_client::{ConnectTarget, protocol::AgentStatus};
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

/// Clicks with the platform's Cmd or Ctrl held.
fn command_click(cx: &mut VisualTestContext, selector: String) {
    let modifiers = cx.update(|_, cx| Modifiers::secondary_key(cx));
    let point = bounds(cx, selector.clone())
        .unwrap_or_else(|| panic!("{selector}"))
        .center();
    cx.simulate_click(point, modifiers);
    draw(cx);
}

#[gpui::test]
fn a_modified_click_selects_a_row_rather_than_opening_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let blocked = row(&view, cx, State::Blocked);
    assert!(cx.debug_bounds("overview-batch").is_none());
    command_click(cx, format!("overview-row-{blocked}"));
    assert!(cx.debug_bounds("overview-batch").is_some(), "the bar shows");
    assert!(
        cx.debug_bounds("overview").is_some(),
        "the page stayed open"
    );
    command_click(cx, format!("overview-row-{blocked}"));
    assert!(
        cx.debug_bounds("overview-batch").is_none(),
        "the last row out takes the bar with it"
    );
}

#[gpui::test]
fn the_bar_settles_every_selected_row(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let (blocked, done) = (row(&view, cx, State::Blocked), row(&view, cx, State::Done));
    command_click(cx, format!("overview-row-{blocked}"));
    command_click(cx, format!("overview-row-{done}"));
    click(cx, "overview-batch-settle");
    let sections = cx.update(|_, cx| view.read(cx).overview_sections(cx));
    assert_eq!(sections.settled.len(), 2);
    assert!(sections.needs.is_empty() && sections.ready.is_empty());
    assert!(cx.debug_bounds("overview-batch").is_none());
}

#[gpui::test]
fn the_bar_snoozes_every_selected_row_until_the_time_picked(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let (blocked, done) = (row(&view, cx, State::Blocked), row(&view, cx, State::Done));
    command_click(cx, format!("overview-row-{blocked}"));
    command_click(cx, format!("overview-row-{done}"));
    click(cx, "overview-batch-snooze");
    click(cx, "overview-snooze-preset-0");
    let sections = cx.update(|_, cx| view.read(cx).overview_sections(cx));
    assert_eq!(sections.snoozed.len(), 2);
    assert!(cx.update(|_, cx| Marks::of(cx).is_some()));
    assert!(cx.debug_bounds("overview-batch").is_none());
}

#[gpui::test]
fn a_selected_row_that_leaves_the_list_leaves_the_selection(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let blocked = row(&view, cx, State::Blocked);
    command_click(cx, format!("overview-row-{blocked}"));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot
                .agents
                .retain(|agent| agent.agent_status != AgentStatus::Blocked);
            view.live.snapshot = Some(Arc::new(snapshot));
            view.poll_overview(cx);
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("overview-batch").is_none());
}

/// Points the selected host at `target`. Tests stay off [`ConnectTarget::Local`]
/// whenever a reply could go, which would run the real `herdr` on this machine.
pub(super) fn set_target(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    target: ConnectTarget,
) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let selected = view.selected_endpoint;
            view.endpoints[selected].connection.target = target;
        })
    });
}

/// A host whose `herdr` this client cannot run.
pub(super) fn unscriptable() -> ConnectTarget {
    ConnectTarget::Socket("/nonexistent/herdr-client.sock".into())
}

#[gpui::test]
fn a_host_that_cannot_take_replies_opens_no_dialog(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let blocked = row(&view, cx, State::Blocked);
    set_target(&view, cx, unscriptable());
    click(cx, format!("overview-reply-{blocked}"));
    assert!(cx.debug_bounds("overview-reply-dialog").is_none());
}

#[gpui::test]
fn snoozing_one_row_leaves_the_selection_alone(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let (blocked, done) = (row(&view, cx, State::Blocked), row(&view, cx, State::Done));
    command_click(cx, format!("overview-row-{blocked}"));
    click(cx, format!("overview-snooze-{done}"));
    click(cx, "overview-snooze-preset-0");
    assert!(cx.debug_bounds("overview-batch").is_some());
    let sections = cx.update(|_, cx| view.read(cx).overview_sections(cx));
    assert_eq!(sections.snoozed.len(), 1);
}

#[gpui::test]
fn long_idle_agents_fold_into_one_row_that_opens(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let rows = cx.update(|_, cx| view.read(cx).overview_rows(cx));
    let working = rows.iter().find(|row| row.state == State::Working).unwrap();
    let selector = format!("overview-row-{}", working.key.selector());
    let (endpoint, pane) = working.pane().unwrap();
    let (endpoint, pane) = (endpoint.to_owned(), pane.to_owned());
    // An hour ago, as far as the page's clock is concerned.
    let hour_ago = SystemTime::now() - Duration::from_secs(2 * 60 * 60);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            let agent = snapshot.agents.iter_mut().find(|a| a.pane_id == pane);
            let agent = agent.unwrap();
            agent.agent_status = AgentStatus::Idle;
            let agents = snapshot.agents.clone();
            view.overview.since.observe(&endpoint, &agents, hour_ago);
            view.live.snapshot = Some(Arc::new(snapshot));
            view.poll_overview(cx);
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("overview-long-idle").is_some());
    assert!(bounds(cx, selector.clone()).is_none(), "it starts folded");
    click(cx, "overview-long-idle");
    assert!(bounds(cx, selector.clone()).is_some());
    click(cx, "overview-long-idle");
    assert!(bounds(cx, selector).is_none());
}

#[gpui::test]
fn a_blocked_rows_excerpt_shows_under_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let blocked = row(&view, cx, State::Blocked);
    assert!(bounds(cx, format!("overview-excerpt-{blocked}")).is_none());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let rows = view.overview_rows(cx);
            let (endpoint, pane) = rows
                .iter()
                .find(|row| row.state == State::Blocked)
                .and_then(|row| row.pane())
                .unwrap();
            view.overview.excerpts = vec![crate::overview::excerpt::Excerpt {
                endpoint: endpoint.to_owned(),
                pane: pane.to_owned(),
                lines: vec!["Run rm -rf?".into()],
            }];
        })
    });
    draw(cx);
    let row_bounds = bounds(cx, format!("overview-row-{blocked}")).unwrap();
    let excerpt = bounds(cx, format!("overview-excerpt-{blocked}")).unwrap();
    assert!(excerpt.top() >= row_bounds.bottom());
}
