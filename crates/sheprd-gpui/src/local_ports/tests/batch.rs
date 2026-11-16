//! Stopping several processes at once, against a scripted OS.

use super::*;
use crate::local_ports::process::{Found, act_with};
use std::cell::RefCell;
use sysinfo::Signal;

const USER: u32 = 501;

fn identity(pid: u32) -> Identity {
    Identity { pid, started: 100 }
}

fn alive(victim: Identity) -> Option<Found> {
    Some(Found {
        started: victim.started,
        user: Some(USER),
        zombie: false,
    })
}

#[derive(Debug, PartialEq)]
enum Event {
    Sent(u32, Signal),
    Waited,
}

/// What a stop of `victims` did, each still running while `running` says.
fn stop(
    victims: &[Identity],
    grace: Duration,
    running: impl Fn(Identity, usize) -> bool,
    refuses: impl Fn(Identity) -> bool,
) -> (Vec<(Identity, Outcome)>, Vec<Event>) {
    let events = RefCell::new(Vec::new());
    let waits = RefCell::new(0);
    let outcomes = act_with(
        PortAction::Stop,
        victims,
        USER,
        grace,
        |victim| {
            running(victim, *waits.borrow())
                .then(|| alive(victim))
                .flatten()
        },
        |victim, signal| {
            events.borrow_mut().push(Event::Sent(victim.pid, signal));
            !refuses(victim)
        },
        |_| {
            *waits.borrow_mut() += 1;
            events.borrow_mut().push(Event::Waited);
        },
    );
    (outcomes, events.into_inner())
}

#[test]
fn every_process_is_asked_to_stop_before_any_waiting() {
    let victims = [identity(1), identity(2), identity(3)];
    let (_, events) = stop(&victims, Duration::from_secs(1), |_, _| true, |_| false);
    let waited = events.iter().position(|event| *event == Event::Waited);
    assert_eq!(
        events[..waited.unwrap()],
        [
            Event::Sent(1, Signal::Term),
            Event::Sent(2, Signal::Term),
            Event::Sent(3, Signal::Term),
            Event::Sent(1, Signal::Continue),
            Event::Sent(2, Signal::Continue),
            Event::Sent(3, Signal::Continue),
        ]
    );
}

#[test]
fn a_batch_shares_one_grace_period() {
    let grace = Duration::from_millis(300);
    let one = stop(&[identity(1)], grace, |_, _| true, |_| false).1;
    let many: Vec<Identity> = (1..=8).map(identity).collect();
    let all = stop(&many, grace, |_, _| true, |_| false).1;
    let waits = |events: &[Event]| events.iter().filter(|e| **e == Event::Waited).count();
    assert_eq!(waits(&all), waits(&one));
}

#[test]
fn only_the_processes_still_running_are_killed() {
    let (outcomes, events) = stop(
        &[identity(1), identity(2)],
        Duration::from_millis(300),
        |victim, waits| victim.pid == 2 || waits == 0,
        |_| false,
    );
    assert_eq!(
        outcomes,
        [
            (identity(1), Outcome::Signalled),
            (identity(2), Outcome::Killed)
        ]
    );
    assert!(!events.contains(&Event::Sent(1, Signal::Kill)));
    assert!(events.contains(&Event::Sent(2, Signal::Kill)));
}

#[test]
fn no_waiting_once_every_process_has_exited() {
    let (_, events) = stop(
        &[identity(1), identity(2)],
        Duration::from_secs(3),
        |_, waits| waits == 0,
        |_| false,
    );
    assert_eq!(events.iter().filter(|e| **e == Event::Waited).count(), 1);
}

#[test]
fn one_refused_signal_leaves_the_others_going() {
    let (outcomes, events) = stop(
        &[identity(1), identity(2)],
        Duration::ZERO,
        |_, _| true,
        |victim| victim.pid == 1,
    );
    assert_eq!(outcomes[0], (identity(1), Outcome::Failed));
    assert_eq!(outcomes[1], (identity(2), Outcome::Killed));
    assert!(!events.contains(&Event::Sent(1, Signal::Continue)));
}

#[test]
fn a_process_gone_before_its_signal_gets_none_and_the_rest_go_ahead() {
    let (outcomes, events) = stop(
        &[identity(1), identity(2)],
        Duration::ZERO,
        |victim, _| victim.pid == 2,
        |_| false,
    );
    assert_eq!(outcomes[0], (identity(1), Outcome::Exited));
    assert_eq!(outcomes[1], (identity(2), Outcome::Killed));
    assert!(!events.iter().any(|e| matches!(e, Event::Sent(1, _))));
}

#[test]
fn pausing_a_batch_never_waits() {
    let victims = [identity(1), identity(2)];
    let sent = RefCell::new(Vec::new());
    let outcomes = act_with(
        PortAction::Pause,
        &victims,
        USER,
        Duration::from_secs(3),
        alive,
        |victim, signal| {
            sent.borrow_mut().push((victim.pid, signal));
            true
        },
        |_| panic!("a pause does not wait"),
    );
    assert!(outcomes.iter().all(|(_, o)| *o == Outcome::Signalled));
    assert_eq!(sent.into_inner(), [(1, Signal::Stop), (2, Signal::Stop)]);
}
