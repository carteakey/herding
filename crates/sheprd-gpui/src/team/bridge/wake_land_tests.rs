//! Team wakes land (#62): the wake queue drains on every tick even before
//! the Team panel is first shown, a wake reaches an agent the way
//! `herdr pane run` delivers - text committed as if typed, then Enter, one
//! ordered batch through the daemon's pane input, never a bracketed Paste
//! that some TUIs truncate to fragments - and every delivery decision logs,
//! because a silent one is indistinguishable from a lost wake. The
//! fixture's daemon input is never connected, so "sent" itself is only
//! reachable live; what the tests pin is which path each wake takes and
//! that the decision is logged.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use crate::SheprdWindow;
use crate::agent_notes::Delivered;
use crate::control::server::team::dispatch::{mention_signal, notify_team_mutation};
use crate::control::server::team_worker::Reply;
use crate::sidebar::layout_tests::fixture_window;
use crate::state::ConnectionStatus;
use sheprd_client::protocol::ClientShellSnapshot;
use sheprd_orchestrator::team_store::{
    Author, Member, Post, SessionTarget, Task, TaskStatus, Team,
};
use std::sync::{Arc, Mutex};
use tracing::{
    Event, Id, Level, Metadata, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Record},
};

/// A log collector usable as a thread-local subscriber, so a test can pin
/// which decisions a wake path logs. Filtering at INFO keeps every wake
/// decision (all logged at info, failures at warn) and drops debug chatter.
#[derive(Clone, Debug, Default)]
struct Capture(Arc<Mutex<Vec<String>>>);

impl Capture {
    fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    fn contains(&self, needle: &str) -> bool {
        self.lines().iter().any(|line| line.contains(needle))
    }
}

#[derive(Default)]
struct Line {
    message: Option<String>,
    fields: Vec<(String, String)>,
}

impl Visit for Line {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        } else {
            self.fields
                .push((field.name().to_owned(), format!("{value:?}")));
        }
    }
}

impl Subscriber for Capture {
    fn enabled(&self, meta: &Metadata<'_>) -> bool {
        meta.level() <= &Level::INFO
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut line = Line::default();
        event.record(&mut line);
        let mut text = line.message.unwrap_or_default();
        for (name, value) in line.fields {
            text.push_str(&format!(" {name}={value}"));
        }
        self.0.lock().unwrap().push(text);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

/// Runs `work` with every wake decision logged into the returned capture.
/// The callsite interest cache is rebuilt against this subscriber first:
/// tracing caches per callsite, and a sibling test that ran the same path
/// without a subscriber may have cached those callsites as disabled.
fn run_logged(capture: &Capture, work: impl FnOnce()) {
    tracing::subscriber::with_default(capture.clone(), || {
        tracing_core::callsite::rebuild_interest_cache();
        work();
    });
}

/// A snapshot with an optional agent in the given status, so tests drive the
/// reachable, agentless, and busy sides of the delivery gates.
fn snapshot_with(
    boot_id: &str,
    workspace: &str,
    pane: &str,
    agent: Option<&str>,
    agent_status: &str,
    state_labels: &[(&str, &str)],
) -> ClientShellSnapshot {
    let agents = agent
        .map(|agent| {
            serde_json::json!([{
                "pane_id": pane, "workspace_id": workspace, "tab_id": "t0",
                "display_agent": agent, "agent": agent,
                "agent_status": agent_status, "state_change_seq": 1,
                "state_labels": state_labels
                    .iter()
                    .map(|(key, value)| serde_json::json!([key, value]))
                    .collect::<Vec<_>>(),
                "tokens": [], "focused": true
            }])
        })
        .unwrap_or_else(|| serde_json::json!([]));
    serde_json::from_value(serde_json::json!({
        "boot_id": boot_id,
        "revision": 1,
        "update_install_command": "",
        "latest_release_notes_available": false,
        "integration_updates_available": false,
        "worktree_directory": "",
        "tab_bar_right": [],
        "tab_bar_right_separator": "",
        "agent_order": [],
        "workspaces": [{
            "workspace_id": workspace, "active_tab_id": "t0", "new_workspace_cwd": "",
            "number": 1, "label": workspace, "custom_label": false,
            "tokens": [], "focused": true, "agent_status": agent_status
        }],
        "tabs": [{
            "tab_id": "t0", "workspace_id": workspace, "number": 1, "label": "tab",
            "custom_label": false, "zoomed": false, "focused": true,
            "agent_status": agent_status
        }],
        "panes": [{
            "pane_id": pane, "workspace_id": workspace, "tab_id": "t0",
            "focused": true, "right_click_passthrough": false
        }],
        "agents": agents,
        "commands": []
    }))
    .expect("test snapshot is valid")
}

fn connect_local(view: &mut SheprdWindow, snapshot: ClientShellSnapshot) {
    view.live.status = ConnectionStatus::Connected;
    view.live.snapshot = Some(Arc::new(snapshot));
    view.endpoints[0].live = view.live.clone();
}

fn wake_team() -> Team {
    Team {
        team_id: "t-x".into(),
        name: "wake-test".into(),
        supervisor: Member {
            member_id: "m_sup".into(),
            display_name: "supervisor".into(),
            target: SessionTarget {
                host: "boot-local".into(),
                session: "default".into(),
                workspace: Some("w1X".into()),
                pane: Some("p_sup".into()),
            },
        },
        members: vec![Member {
            member_id: "m_w1".into(),
            display_name: "worker1".into(),
            target: SessionTarget {
                host: "boot-local".into(),
                session: "default".into(),
                workspace: Some("w1X".into()),
                pane: Some("w1X:p9".into()),
            },
        }],
    }
}

fn done_task_reply() -> Reply {
    Reply::ScopedTask {
        team: wake_team(),
        task: Task {
            task_id: "task9".into(),
            team_id: "t-x".into(),
            creator: Author::Human {
                user_id: "human".into(),
            },
            title: "Write report".into(),
            brief: String::new(),
            assignee: Some("m_w1".into()),
            status: TaskStatus::Done,
            blocker_reason: None,
            result: Some("Report drafted".into()),
            created_at: at(0),
            updated_at: at(10),
        },
        actor: Author::Member {
            member_id: "m_w1".into(),
        },
    }
}

fn mention_post_reply() -> Reply {
    Reply::ScopedPosted {
        team: wake_team(),
        post: Post {
            post_id: "post-1".into(),
            team_id: "t-x".into(),
            author: Author::Member {
                member_id: "m_w1".into(),
            },
            reply_to: None,
            body: "wake up".into(),
            mentions: vec!["m_sup".into()],
            created_at: at(1),
        },
    }
}

fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(secs, 0).unwrap_or(chrono::DateTime::UNIX_EPOCH)
}

fn hold_supervisor_wake(view: &mut SheprdWindow, pane: &str) {
    view.team_bridge.hold_prompt(
        "t-x".into(),
        "boot-local".into(),
        pane.into(),
        "wake text".into(),
        "summary".into(),
        vec!["task9|done|10000".into()],
    );
    view.team_bridge.force_retry_due_for_test();
}

#[gpui::test]
fn the_drain_runs_before_the_team_panel_is_ever_shown(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    window
        .update(cx, |view, _, _| {
            // The snapshot shows only the worker pane; the supervisor's pane
            // is gone from it.
            connect_local(
                view,
                snapshot_with("boot-local", "w1X", "w1X:p9", Some("opencode"), "idle", &[]),
            );
            view.team_bridge.set_team_for_test(wake_team());
            hold_supervisor_wake(view, "p_sup");
            // The roster re-binds elsewhere, the way members re-register: the
            // held wake's pane is now in neither the roster nor any snapshot.
            let mut rebound = wake_team();
            rebound.supervisor.target.pane = Some("w1X:p9".into());
            view.team_bridge.set_team_for_test(rebound);
            assert!(
                !view.team_panel_shown(),
                "the fixture never shows the Team panel"
            );
        })
        .expect("window open");
    let capture = Capture::default();
    run_logged(&capture, || {
        window
            .update(cx, |view, _, cx| view.poll_team(cx))
            .expect("window open");
    });
    window
        .update(cx, |view, _, _| {
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                0,
                "a wake whose pane is gone is not retried forever, panel or no panel"
            );
            assert_eq!(
                view.team_bridge.snapshot(&[]).undeliverable_count,
                1,
                "the drain marks the stranded wake undeliverable"
            );
        })
        .expect("window open");
    assert!(
        capture.contains("team wake drain marked wakes undeliverable"),
        "the drain decision logs even though the panel was never shown: {capture:?}",
    );
}

#[gpui::test]
fn a_wake_reaches_an_idle_agent_as_typed_text_then_enter(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    window
        .update(cx, |view, _, _| {
            connect_local(
                view,
                snapshot_with("boot-local", "w1X", "p_sup", Some("opencode"), "idle", &[]),
            );
            view.team_bridge.set_team_for_test(wake_team());
            hold_supervisor_wake(view, "p_sup");
            assert!(
                !view.team_panel_shown(),
                "the wake retries without anyone opening the board"
            );
        })
        .expect("window open");
    let capture = Capture::default();
    run_logged(&capture, || {
        window
            .update(cx, |view, _, cx| view.poll_team(cx))
            .expect("window open");
    });
    assert!(
        capture.contains("could not be typed into the pane"),
        "the wake went through the daemon's pane input as a typed batch: {capture:?}",
    );
    assert!(
        capture.contains("outcome=\"kept\""),
        "the failed typed batch names its outcome: {capture:?}",
    );
    assert!(
        !capture
            .lines()
            .iter()
            .any(|line| line.contains("Could not paste notes")),
        "wakes never ride the paste path some TUIs truncate: {capture:?}",
    );
    assert!(
        capture.contains("team wake drain delivered a held wake"),
        "the drain logged the delivery attempt: {capture:?}",
    );
    window
        .update(cx, |view, _, _| {
            // The fixture's daemon input is not connected, so the batch could
            // not be queued: the wake stays held for a retry, and nothing is
            // recorded as delivered. The old paste path claimed this very
            // delivery instead - the silent loss #62 is about.
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                1,
                "the wake waits for a connected daemon input"
            );
            assert!(
                !view.team_bridge.wake_seen("t-x", "task9|done|10000"),
                "fail closed: an undelivered wake is never recorded"
            );
        })
        .expect("window open");
}

#[gpui::test]
fn a_seen_mention_wake_dedupes_and_logs(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    window
        .update(cx, |view, _, _| {
            connect_local(
                view,
                snapshot_with("boot-local", "w1X", "p_sup", Some("opencode"), "idle", &[]),
            );
            view.team_bridge.set_team_for_test(wake_team());
            // The mention's signal is exactly the one the dispatch carries:
            // one post per recipient, so an event and the periodic fallback
            // wake the mentioned member once.
            view.team_bridge
                .mark_supervisor_woken("t-x", [mention_signal("post-1", "m_sup")]);
        })
        .expect("window open");
    let capture = Capture::default();
    run_logged(&capture, || {
        cx.update(|app| notify_team_mutation(&mention_post_reply(), app));
    });
    assert!(
        capture.contains("team wake not dispatched: every wake signal was already seen"),
        "a seen mention wake is deduped: {capture:?}",
    );
    for field in [
        "team=t-x",
        "member=supervisor",
        "pane=p_sup",
        "outcome=\"deduped\"",
    ] {
        assert!(
            capture.contains(field),
            "the dedupe log names the wake it skipped ({field}): {capture:?}",
        );
    }
    window
        .update(cx, |view, _, _| {
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                0,
                "a deduped wake queues nothing"
            );
        })
        .expect("window open");
}

#[gpui::test]
fn a_dead_session_skips_the_wake_and_logs(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    window
        .update(cx, |view, _, _| {
            connect_local(
                view,
                snapshot_with(
                    "boot-local",
                    "w1X",
                    "p_sup",
                    Some("opencode"),
                    "idle",
                    &[("idle", "process exited with status 1")],
                ),
            );
            view.team_bridge.set_team_for_test(wake_team());
        })
        .expect("window open");
    let capture = Capture::default();
    run_logged(&capture, || {
        cx.update(|app| notify_team_mutation(&done_task_reply(), app));
    });
    assert!(
        capture.contains("team wake not dispatched: the pane's session reports crashed"),
        "a dead session is skipped, not dispatched to: {capture:?}",
    );
    assert!(
        capture.contains("outcome=\"skipped_dead_session\""),
        "the skip names its outcome: {capture:?}",
    );
    window
        .update(cx, |view, _, _| {
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                0,
                "a skipped wake queues nothing"
            );
        })
        .expect("window open");
}

#[gpui::test]
fn the_dispatch_outcome_logs_team_member_pane_and_outcome(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    window
        .update(cx, |view, _, _| {
            connect_local(
                view,
                snapshot_with("boot-local", "w1X", "p_sup", Some("opencode"), "idle", &[]),
            );
            view.team_bridge.set_team_for_test(wake_team());
        })
        .expect("window open");
    let capture = Capture::default();
    run_logged(&capture, || {
        cx.update(|app| notify_team_mutation(&done_task_reply(), app));
    });
    assert!(
        capture.contains("team wake dispatch outcome"),
        "every dispatch logs its outcome: {capture:?}",
    );
    for field in [
        "team=t-x",
        "member=supervisor",
        "pane=p_sup",
        "outcome=Held",
    ] {
        assert!(
            capture.contains(field),
            "the outcome log names the wake ({field}): {capture:?}",
        );
    }
}

#[gpui::test]
fn the_direct_delivery_decisions_each_log(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    let capture = Capture::default();
    run_logged(&capture, || {
        window
            .update(cx, |view, _, _| {
                // No endpoints connected at all: nothing can prove a pane.
                let endpoints = std::mem::take(&mut view.endpoints);
                assert_eq!(
                    view.deliver_team_prompt("boot-local", "p_sup", "wake".into()),
                    Delivered::Unreachable
                );
                assert!(
                    capture.contains("no endpoints connected"),
                    "an endpoint-less delivery logs: {capture:?}"
                );
                view.endpoints = endpoints;

                // Endpoints exist, but none runs the member's recorded host:
                // the wake is unreachable, never a fallback onto another
                // machine that happens to show the same pane id.
                assert_eq!(
                    view.deliver_team_prompt("boot-gone", "p_sup", "wake".into()),
                    Delivered::Unreachable
                );
                assert!(
                    capture.contains("no connected endpoint runs the member's host"),
                    "an unknown host logs: {capture:?}"
                );

                // The host is reachable and shows the pane, but no agent
                // session lives in it.
                connect_local(
                    view,
                    snapshot_with("boot-local", "w1X", "p_sup", None, "idle", &[]),
                );
                assert_eq!(
                    view.deliver_team_prompt("boot-local", "p_sup", "wake".into()),
                    Delivered::Unreachable
                );
                assert!(
                    capture.contains("no agent session in the pane"),
                    "an agentless pane logs: {capture:?}"
                );

                // The agent is mid-turn: the wake must wait for its turn to
                // end, or the input would be cleared underneath it.
                connect_local(
                    view,
                    snapshot_with(
                        "boot-local",
                        "w1X",
                        "p_sup",
                        Some("opencode"),
                        "working",
                        &[],
                    ),
                );
                assert_eq!(
                    view.deliver_team_prompt("boot-local", "p_sup", "wake".into()),
                    Delivered::Kept
                );
                assert!(
                    capture.contains("team wake held: the agent is mid-turn"),
                    "a busy agent logs: {capture:?}"
                );
                assert!(
                    capture.contains("outcome=\"held_busy\""),
                    "the hold names its outcome: {capture:?}"
                );

                // The agent is idle and the host is proven: the wake is
                // committed as typed text plus Enter through the daemon's
                // pane input. The fixture's daemon input is not connected, so
                // the batch is kept for a retry and the outcome logs.
                connect_local(
                    view,
                    snapshot_with("boot-local", "w1X", "p_sup", Some("opencode"), "idle", &[]),
                );
                assert_eq!(
                    view.deliver_team_prompt("boot-local", "p_sup", "wake".into()),
                    Delivered::Kept
                );
                assert!(
                    capture.contains("could not be typed into the pane"),
                    "a batch the daemon could not take logs: {capture:?}"
                );
            })
            .expect("window open");
    });
    assert!(capture.contains("outcome=\"unreachable\""));
}
