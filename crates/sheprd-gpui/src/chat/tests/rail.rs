//! The rail lists what runs beside the conversation in its own sections,
//! stands in for the working row's table, and stops what can be stopped.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{Modifiers, TestAppContext};
use sheprd_chat_protocol::{Activity, ActivityKind, ActivityState, Command};
use sheprd_client::ConnectTarget;

fn activity(call: &str, kind: ActivityKind, stoppable: bool) -> Activity {
    Activity {
        call_id: call.into(),
        detail: Some("Compiling".into()),
        finished_ms: None,
        kind,
        label: "npm run dev".into(),
        started_ms: now_ms().saturating_sub(65_000),
        state: ActivityState::Running,
        stoppable,
    }
}

fn next_command(stream: &mut Peer) -> Command {
    match read(stream) {
        Some(ClientMessage::Command { command, .. }) => command,
        other => panic!("a command, got {other:?}"),
    }
}

fn snapshot(activities: Vec<Activity>, read_only: bool) -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![agent(1, "It runs in the background.")];
    transcript.activities = activities;
    transcript.read_only = read_only;
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

#[gpui::test]
fn each_kind_of_work_gets_its_section_and_the_row_opens_no_table(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let work = vec![
        activity("agent", ActivityKind::Subagent, false),
        activity("shell", ActivityKind::Shell, false),
        activity("watch", ActivityKind::Monitor, false),
    ];
    let _stream = attach_with(&view, &program, snapshot(work, false), cx);
    assert!(cx.debug_bounds("chat-rail").is_some());
    for section in [
        "chat-rail-section-0",
        "chat-rail-section-1",
        "chat-rail-section-2",
    ] {
        assert!(cx.debug_bounds(section).is_some(), "{section}");
    }
    let row = cx.debug_bounds("chat-running").unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-running-table").is_none());
}

#[gpui::test]
fn stop_asks_at_once_for_the_work_it_is_on(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let work = vec![activity("shell", ActivityKind::Shell, true)];
    let mut stream = attach_with(&view, &program, snapshot(work, false), cx);
    let stop = cx.debug_bounds("chat-rail-stop-shell").unwrap();
    cx.simulate_click(stop.center(), Modifiers::default());
    assert_eq!(
        next_command(&mut stream),
        Command::StopActivity {
            call_id: "shell".into()
        }
    );
}

#[gpui::test]
fn work_the_agent_cannot_stop_offers_no_stop(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let work = vec![activity("shell", ActivityKind::Shell, false)];
    let _stream = attach_with(&view, &program, snapshot(work, false), cx);
    assert!(cx.debug_bounds("chat-rail-section-1").is_some());
    assert!(cx.debug_bounds("chat-rail-stop-shell").is_none());
}

#[gpui::test]
fn a_read_only_chat_offers_no_stop(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let work = vec![activity("shell", ActivityKind::Shell, true)];
    let _stream = attach_with(&view, &program, snapshot(work, true), cx);
    assert!(cx.debug_bounds("chat-rail-section-1").is_some());
    assert!(cx.debug_bounds("chat-rail-stop-shell").is_none());
}
