use super::*;
use chrono::Utc;
use sheprd_orchestrator::team_store::{Author, SessionTarget};

fn member(id: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: id.into(),
        target: SessionTarget {
            host: "local".into(),
            session: format!("agent:zcode:ws:{id}"),
            workspace: Some("ws".into()),
            pane: Some(format!("pane-{id}")),
        },
    }
}

fn held_task(id: &str, assignee: &str) -> Task {
    Task {
        task_id: id.into(),
        team_id: "t".into(),
        creator: Author::Member {
            member_id: "sup".into(),
        },
        title: format!("Task {id}"),
        brief: String::new(),
        assignee: Some(assignee.into()),
        status: TaskStatus::InProgress,
        blocker_reason: None,
        result: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

const TIMEOUT: Duration = Duration::from_secs(180);

#[test]
fn a_recent_call_or_fresh_output_holds_the_stall_off() {
    let mut tracker = StallTracker::new();
    let now = Instant::now();
    tracker.observe_panes([("pane-w1", 7u64)], now);
    tracker.note_member_call("agent:zcode:ws:w1", now - Duration::from_secs(30));
    assert!(!tracker.is_stalled(
        "agent:zcode:ws:w1",
        "pane-w1",
        Some(AgentStatus::Idle),
        TIMEOUT,
        now
    ));
    // Fresh output alone also holds it off, even with no call ever seen.
    let mut quiet = StallTracker::new();
    quiet.observe_panes([("pane-w1", 7u64)], now - Duration::from_secs(30));
    assert!(!quiet.is_stalled(
        "agent:zcode:ws:w1",
        "pane-w1",
        Some(AgentStatus::Idle),
        TIMEOUT,
        now
    ));
}

#[test]
fn silence_on_both_signals_for_the_timeout_is_a_stall() {
    let mut tracker = StallTracker::new();
    let start = Instant::now();
    tracker.observe_panes([("pane-w1", 7u64)], start);
    tracker.note_member_call("agent:zcode:ws:w1", start);
    let now = start + TIMEOUT;
    tracker.observe_panes([("pane-w1", 7u64)], now);
    assert!(tracker.is_stalled(
        "agent:zcode:ws:w1",
        "pane-w1",
        Some(AgentStatus::Idle),
        TIMEOUT,
        now
    ));
    let task = held_task("t-1", "w1");
    let w1 = member("w1");
    let members = [(&w1, "pane-w1".to_owned(), Some((AgentStatus::Idle, 7)))];
    let held = [("w1", &task)].into_iter().collect::<HashMap<_, _>>();
    let detected = tracker.detect(&members, &held, TIMEOUT, now);
    assert_eq!(detected.len(), 1);
    assert_eq!(detected[0].task_id, "t-1");
    assert_eq!(detected[0].minutes, 3);
    assert_eq!(detected[0].signal("t"), "stall|t|t-1|w1|1");
    assert!(detected[0].prompt("t", "herding team").contains("t-1"));
}

#[test]
fn a_working_or_unobservable_pane_is_never_stalled() {
    let mut tracker = StallTracker::new();
    let start = Instant::now();
    tracker.observe_panes([("pane-w1", 1u64)], start);
    let now = start + TIMEOUT;
    tracker.observe_panes([("pane-w1", 1u64)], now);
    assert!(!tracker.is_stalled(
        "agent:zcode:ws:w1",
        "pane-w1",
        Some(AgentStatus::Working),
        TIMEOUT,
        now
    ));
    // A pane the snapshots no longer describe is unobservable, so silent.
    assert!(!tracker.is_stalled(
        "agent:zcode:ws:w1",
        "pane-gone",
        Some(AgentStatus::Idle),
        TIMEOUT,
        now
    ));
}

#[test]
fn a_member_without_assigned_work_is_never_reported() {
    let mut tracker = StallTracker::new();
    let start = Instant::now();
    tracker.observe_panes([("pane-w1", 1u64)], start);
    let now = start + TIMEOUT;
    tracker.observe_panes([("pane-w1", 1u64)], now);
    let w1 = member("w1");
    let members = [(&w1, "pane-w1".to_owned(), Some((AgentStatus::Idle, 1)))];
    let held = HashMap::<&str, &Task>::new();
    assert!(tracker.detect(&members, &held, TIMEOUT, now).is_empty());
}

#[test]
fn a_detection_stays_frozen_until_the_member_recovers() {
    let mut tracker = StallTracker::new();
    let start = Instant::now();
    tracker.observe_panes([("pane-w1", 1u64)], start);
    let stalled_at = start + TIMEOUT;
    tracker.observe_panes([("pane-w1", 1u64)], stalled_at);
    let task = held_task("t-1", "w1");
    let w1 = member("w1");
    let members = [(&w1, "pane-w1".to_owned(), Some((AgentStatus::Idle, 1)))];
    let held = [("w1", &task)].into_iter().collect::<HashMap<_, _>>();
    let first = tracker.detect(&members, &held, TIMEOUT, stalled_at);
    let signal = first[0].signal("t");
    // A minute later the stall lasts; the frozen record and its signal do not.
    let later = stalled_at + Duration::from_secs(60);
    tracker.observe_panes([("pane-w1", 1u64)], later);
    let again = tracker.detect(&members, &held, TIMEOUT, later);
    assert_eq!(again[0].signal("t"), signal);
    assert_eq!(again[0].minutes, first[0].minutes);
    // Recovery: a fresh call and fresh output end the stall.
    let recovered_at = later + TIMEOUT;
    tracker.observe_panes([("pane-w1", 2u64)], recovered_at);
    tracker.note_member_call("agent:zcode:ws:w1", recovered_at - Duration::from_secs(10));
    assert!(
        tracker
            .detect(&members, &held, TIMEOUT, recovered_at)
            .is_empty()
    );
}

#[test]
fn a_restall_after_recovery_detects_afresh() {
    let mut tracker = StallTracker::new();
    let start = Instant::now();
    tracker.observe_panes([("pane-w1", 1u64)], start);
    let task = held_task("t-1", "w1");
    let w1 = member("w1");
    let members = [(&w1, "pane-w1".to_owned(), Some((AgentStatus::Idle, 1)))];
    let held = [("w1", &task)].into_iter().collect::<HashMap<_, _>>();
    let stalled_at = start + TIMEOUT;
    tracker.observe_panes([("pane-w1", 1u64)], stalled_at);
    let first = tracker.detect(&members, &held, TIMEOUT, stalled_at);
    // Recover for a shorter stretch than the timeout, then go silent again:
    // the fresh detection carries its own, longer duration.
    let recovered_at = stalled_at + TIMEOUT;
    tracker.observe_panes([("pane-w1", 2u64)], recovered_at);
    tracker.note_member_call("agent:zcode:ws:w1", recovered_at);
    assert!(
        tracker
            .detect(&members, &held, TIMEOUT, recovered_at)
            .is_empty()
    );
    let restalled_at = recovered_at + TIMEOUT;
    tracker.observe_panes([("pane-w1", 2u64)], restalled_at);
    let again = tracker.detect(&members, &held, TIMEOUT, restalled_at);
    assert_eq!(again.len(), 1);
    assert_ne!(again[0].signal("t"), first[0].signal("t"));
}

#[test]
fn panes_that_leave_the_snapshots_stop_being_judged() {
    let mut tracker = StallTracker::new();
    let now = Instant::now();
    tracker.observe_panes([("pane-w1", 1u64), ("pane-w2", 1u64)], now);
    tracker.observe_panes([("pane-w1", 1u64)], now);
    assert!(!tracker.is_stalled(
        "agent:zcode:ws:w2",
        "pane-w2",
        Some(AgentStatus::Idle),
        TIMEOUT,
        now
    ));
}
