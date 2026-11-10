//! The reply dialog, as a user drives it. Only clients that can run scripts on
//! a host open it (`pane_move::refusal`); `acting` covers the refusal.

use super::{
    acting::{set_target, unscriptable},
    page::{bounds, click, draw, fixture, row},
};
use crate::{SheprdWindow, overview::State};
use gpui::{Entity, VisualTestContext};
use sheprd_client::{ConnectTarget, protocol::AgentStatus};
use std::sync::Arc;

/// Sets the blocked agent's status, as a new snapshot would.
fn set_status(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    from: AgentStatus,
    to: AgentStatus,
) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot
                .agents
                .iter_mut()
                .filter(|agent| agent.agent_status == from)
                .for_each(|agent| agent.agent_status = to);
            view.live.snapshot = Some(Arc::new(snapshot));
            view.poll_overview(cx);
        })
    });
    draw(cx);
}

/// Opens the dialog on the blocked row and writes `text`.
fn write_reply(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, text: &str) -> String {
    let blocked = row(view, cx, State::Blocked);
    set_target(view, cx, ConnectTarget::Local);
    click(cx, format!("overview-reply-{blocked}"));
    set_target(view, cx, unscriptable());
    assert!(cx.debug_bounds("overview-reply-dialog").is_some());
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.menu.input.as_mut().unwrap().text = text.into();
        })
    });
    draw(cx);
    blocked
}

#[gpui::test]
fn a_reply_to_a_blocked_agent_waits_on_its_row(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let blocked = write_reply(&view, cx, "use the second option");
    click(cx, "overview-reply-submit");
    assert!(cx.debug_bounds("overview-reply-dialog").is_none());
    assert!(
        bounds(cx, format!("overview-notes-{blocked}")).is_some(),
        "the row shows what is queued"
    );
    assert!(bounds(cx, "overview-reply-0").is_some());
    // Still blocked after a tick: nothing moved.
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_overview(cx)));
    draw(cx);
    assert!(bounds(cx, "overview-reply-cancel-0").is_some());
    click(cx, "overview-reply-cancel-0");
    assert!(bounds(cx, "overview-reply-0").is_none());
    assert!(bounds(cx, format!("overview-notes-{blocked}")).is_none());
}

#[gpui::test]
fn an_empty_reply_keeps_the_dialog_open(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    write_reply(&view, cx, "   ");
    click(cx, "overview-reply-submit");
    assert!(cx.debug_bounds("overview-reply-dialog").is_some());
    assert!(bounds(cx, "overview-reply-0").is_none());
}

#[gpui::test]
fn a_reply_is_tried_once_the_agent_is_back_and_its_failure_stays_listed(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = fixture(cx);
    write_reply(&view, cx, "carry on");
    click(cx, "overview-reply-submit");
    assert!(bounds(cx, "overview-reply-cancel-0").is_some());
    set_status(&view, cx, AgentStatus::Blocked, AgentStatus::Idle);
    // On its way, it can no longer be cancelled.
    assert!(bounds(cx, "overview-reply-0").is_some());
    assert!(bounds(cx, "overview-reply-cancel-0").is_none());
    cx.run_until_parked();
    draw(cx);
    // This host cannot run the command, which the row now says.
    assert!(bounds(cx, "overview-reply-0").is_some());
    click(cx, "overview-reply-cancel-0");
    assert!(bounds(cx, "overview-reply-0").is_none());
}

#[gpui::test]
fn a_reply_too_long_to_send_is_refused_not_cut(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let long = "é".repeat(crate::overview::MAX_REPLY);
    write_reply(&view, cx, &long);
    click(cx, "overview-reply-submit");
    assert!(cx.debug_bounds("overview-reply-error").is_some());
    assert!(cx.debug_bounds("overview-reply-dialog").is_some());
    let kept = cx.update(|_, cx| view.read(cx).menu.input.as_ref().map(|i| i.text.len()));
    assert_eq!(kept, Some(long.len()), "the text stays to be shortened");
    assert!(bounds(cx, "overview-reply-0").is_none());
}

#[gpui::test]
fn a_reply_to_an_agent_that_closed_is_dropped(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    write_reply(&view, cx, "too late");
    click(cx, "overview-reply-submit");
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
    assert!(bounds(cx, "overview-reply-0").is_none());
}

/// Replaces the snapshot behind the open dialog, as a reconnect would, without
/// a tick reading it first.
fn replace_snapshot(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    change: impl FnOnce(&mut sheprd_client::protocol::ClientShellSnapshot),
) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            change(&mut snapshot);
            view.live.snapshot = Some(Arc::new(snapshot));
        })
    });
}

#[gpui::test]
fn a_reply_written_before_a_daemon_restart_reaches_no_new_agent(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    write_reply(&view, cx, "for the old agent");
    // The pane ID and agent name came back, on another boot.
    replace_snapshot(&view, cx, |snapshot| {
        snapshot.boot_id = format!("{}-restarted", snapshot.boot_id);
    });
    click(cx, "overview-reply-submit");
    assert!(cx.update(|_, cx| view.read(cx).overview.replies.is_empty()));
    assert!(bounds(cx, "overview-reply-0").is_none());
}

#[gpui::test]
fn a_reply_written_before_its_pane_changed_hands_is_not_queued(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    write_reply(&view, cx, "for the old agent");
    replace_snapshot(&view, cx, |snapshot| {
        for agent in &mut snapshot.agents {
            if agent.agent_status == AgentStatus::Blocked {
                agent.display_agent = Some("someone else".into());
            }
        }
    });
    click(cx, "overview-reply-submit");
    assert!(cx.update(|_, cx| view.read(cx).overview.replies.is_empty()));
}
