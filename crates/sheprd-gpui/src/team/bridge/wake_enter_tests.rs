//! The Enter owed for a typed wake text: nothing is due before its delay
//! has run out, each entry pops once oldest first, an entry stale beyond
//! [`WAKE_ENTER_STALE_AFTER`] is returned for the caller to drop rather
//! than send, and the queue is capped. The queue drains independently of
//! the prompt queue's pause state - the text is already in the pane's
//! input, so withholding the Enter would strand it.

use super::*;

#[test]
fn an_enter_is_not_due_before_its_delay_has_run_out() {
    let mut bridge = Bridge::default();
    let now = Instant::now();
    bridge.schedule_wake_enter("boot-local", "w1:p1", now + WAKE_ENTER_DELAY);
    assert_eq!(
        bridge.drain_due_enters(now),
        (Vec::new(), Vec::new()),
        "the Enter waits out its delay"
    );

    // One due now and one due after the delay: at the later instant both
    // are due, and they leave oldest first, each exactly once.
    bridge.schedule_wake_enter("boot-local", "w1:p2", now);
    let (send, stale) = bridge.drain_due_enters(now + WAKE_ENTER_DELAY);
    assert!(stale.is_empty());
    assert_eq!(
        send.iter()
            .map(|enter| (enter.host.as_str(), enter.pane_id.as_str()))
            .collect::<Vec<_>>(),
        vec![("boot-local", "w1:p1"), ("boot-local", "w1:p2")],
        "owed Enters leave oldest first"
    );
    assert_eq!(
        bridge.drain_due_enters(now + WAKE_ENTER_DELAY),
        (Vec::new(), Vec::new()),
        "a popped Enter is not offered twice"
    );
    assert_eq!(bridge.pending_enter_count_for_test(), 0);
}

#[test]
fn an_enter_stale_beyond_the_bound_is_returned_for_the_caller_to_drop() {
    let mut bridge = Bridge::default();
    let now = Instant::now();
    // Due before `now - WAKE_ENTER_STALE_AFTER`: the pane's input may hold
    // the member's own typing by now, so this one must never be sent.
    bridge.schedule_wake_enter(
        "boot-local",
        "w1:p9",
        now - WAKE_ENTER_STALE_AFTER - Duration::from_secs(1),
    );
    let (send, stale) = bridge.drain_due_enters(now);
    assert!(
        send.is_empty(),
        "a stale Enter is never handed to the sender"
    );
    assert_eq!(
        stale
            .iter()
            .map(|enter| enter.pane_id.as_str())
            .collect::<Vec<_>>(),
        vec!["w1:p9"]
    );
}

#[test]
fn the_owed_enter_queue_is_capped_and_drops_the_oldest() {
    let mut bridge = Bridge::default();
    let now = Instant::now();
    for number in 0..WAKE_ENTER_QUEUE_CAP {
        bridge.schedule_wake_enter("boot-local", &format!("w1:p{number}"), now);
    }
    assert_eq!(bridge.pending_enter_count_for_test(), WAKE_ENTER_QUEUE_CAP);
    // One more pushes the oldest out; the queue never exceeds the cap.
    bridge.schedule_wake_enter("boot-local", "w1:new", now);
    assert_eq!(bridge.pending_enter_count_for_test(), WAKE_ENTER_QUEUE_CAP);
    let (send, stale) = bridge.drain_due_enters(now);
    assert!(stale.is_empty());
    assert_eq!(
        send.first().map(|enter| enter.pane_id.as_str()),
        Some("w1:p1"),
        "the oldest entry was dropped at the cap"
    );
    assert_eq!(
        send.last().map(|enter| enter.pane_id.as_str()),
        Some("w1:new")
    );
}
