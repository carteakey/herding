//! Which alerts the history keeps, and what quiet hours hold back.

use super::*;

/// [`tick`] with the quiet hours in force or not.
fn tick_quiet(endpoints: &mut [Endpoint], config: NotificationConfig, quiet: bool, now: Instant) {
    super::super::tick(endpoints, 1, config, None, false, true, quiet, None, now);
}

fn kept(endpoints: &[Endpoint]) -> usize {
    endpoints
        .iter()
        .map(|endpoint| endpoint.toasts.history.alerts().count())
        .sum()
}

#[test]
fn an_alert_shown_joins_the_history_once() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    for offset in 0..3 {
        tick_quiet(
            &mut endpoints,
            config(0),
            false,
            now + Duration::from_millis(offset * 100),
        );
    }
    assert_eq!(visible(&endpoints), ["event"]);
    assert_eq!(kept(&endpoints), 1);
}

#[test]
fn quiet_hours_hold_an_alert_back_but_keep_it_for_later() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    tick_quiet(&mut endpoints, config(0), true, now);
    assert!(visible(&endpoints).is_empty());
    assert!(endpoints[0].toasts.entries.is_empty());
    assert_eq!(kept(&endpoints), 1);
    // It does not arrive when the quiet hours end.
    tick_quiet(
        &mut endpoints,
        config(0),
        false,
        now + Duration::from_secs(1),
    );
    assert!(visible(&endpoints).is_empty());
}

#[test]
fn an_alert_whose_delay_ends_after_quiet_hours_stays_in_the_history() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    Arc::make_mut(endpoints[0].live.snapshot.as_mut().unwrap()).agents[0].agent_status =
        AgentStatus::Blocked;
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    // Received a moment before the quiet hours end, with a five-second delay.
    tick_quiet(&mut endpoints, config(5), true, now);
    assert_eq!(endpoints[0].toasts.entries.len(), 1, "still waiting");
    tick_quiet(
        &mut endpoints,
        config(5),
        false,
        now + Duration::from_secs(5),
    );
    assert!(visible(&endpoints).is_empty());
    assert!(endpoints[0].toasts.entries.is_empty());
    assert_eq!(kept(&endpoints), 1);
}

#[test]
fn quiet_hours_hold_back_system_notifications_too() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    let config = NotificationConfig {
        system: true,
        delay_seconds: 0,
        ..Default::default()
    };
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    tick_quiet(&mut endpoints, config, true, now);
    assert!(take_system(&mut endpoints, config).is_empty());
    assert_eq!(kept(&endpoints), 1);
}

#[test]
fn feedback_for_what_the_user_did_ignores_quiet_hours() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    let notice = Notice::local_feedback(tests::notification("saved"), now);
    endpoints[0].toasts.receive([notice]);
    tick_quiet(&mut endpoints, config(0), true, now);
    assert_eq!(visible(&endpoints), ["saved"]);
    assert_eq!(kept(&endpoints), 0, "only agents' alerts are kept");
}

#[test]
fn a_toast_already_showing_stays_when_quiet_hours_begin() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    tick_quiet(&mut endpoints, config(0), false, now);
    tick_quiet(&mut endpoints, config(0), true, now);
    assert_eq!(visible(&endpoints), ["event"]);
}

#[test]
fn a_toast_waiting_its_turn_does_not_appear_during_quiet_hours() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    let mut second = wire(Kind::NeedsAttention);
    second.pane_id = Some("w1:p2".into());
    receive(&mut endpoints[0], second, now);
    tick_quiet(&mut endpoints, config(0), false, now);
    assert_eq!(endpoints[0].toasts.entries.len(), 2, "one shows, one waits");
    assert_eq!(visible(&endpoints).len(), 1);
    tick_quiet(&mut endpoints, config(0), true, now);
    assert_eq!(endpoints[0].toasts.entries.len(), 1, "the waiting one left");
    assert_eq!(visible(&endpoints).len(), 1, "the one showing stayed");
    assert_eq!(kept(&endpoints), 2, "both stay in the history");
}

#[test]
fn feedback_waiting_its_turn_survives_quiet_hours() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    tick_quiet(&mut endpoints, config(0), false, now);
    let notice = Notice::local_feedback(tests::notification("saved"), now);
    endpoints[0].toasts.receive([notice]);
    tick_quiet(&mut endpoints, config(0), true, now);
    assert_eq!(endpoints[0].toasts.entries.len(), 2);
}

#[test]
fn delivery_off_shows_nothing_but_keeps_the_alert() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    let off = NotificationConfig {
        enabled: false,
        delay_seconds: 0,
        ..Default::default()
    };
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    tick_quiet(&mut endpoints, off, false, now);
    assert!(endpoints[0].toasts.entries.is_empty());
    assert_eq!(kept(&endpoints), 1);
}

#[test]
fn turning_delivery_off_takes_a_shown_toast_away() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    tick_quiet(&mut endpoints, config(0), false, now);
    assert_eq!(visible(&endpoints), ["event"]);
    let off = NotificationConfig {
        enabled: false,
        delay_seconds: 0,
        ..Default::default()
    };
    tick_quiet(&mut endpoints, off, false, now);
    assert!(endpoints[0].toasts.entries.is_empty());
    assert_eq!(kept(&endpoints), 1, "it is not counted twice");
}

#[test]
fn an_alert_the_user_turned_off_is_not_kept() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    let config = NotificationConfig {
        finished: false,
        ..config(0)
    };
    receive(&mut endpoints[0], wire(Kind::Finished), now);
    tick_quiet(&mut endpoints, config, false, now);
    assert_eq!(kept(&endpoints), 0);
}

#[test]
fn a_custom_notice_is_shown_but_not_kept() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[0], wire(Kind::Custom), now);
    tick_quiet(&mut endpoints, config(0), false, now);
    assert_eq!(visible(&endpoints), ["event"]);
    assert_eq!(kept(&endpoints), 0);
}

#[test]
fn the_history_keeps_only_the_newest_alerts() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    for round in 0..HISTORY_LIMIT + 3 {
        let mut wire = wire(Kind::NeedsAttention);
        wire.title = format!("alert {round}");
        receive(&mut endpoints[0], wire, now);
        tick_quiet(&mut endpoints, config(0), true, now);
    }
    let history = &endpoints[0].toasts.history;
    assert_eq!(history.alerts().count(), HISTORY_LIMIT);
    assert_eq!(history.alerts().next().unwrap().notice.title, "alert 3");
    assert!(history.get(0).is_none(), "the oldest is gone");
    assert!(history.get(3).is_some());
}

#[test]
fn a_kept_alert_finds_its_pane_only_on_its_own_boot() {
    let now = Instant::now();
    let mut endpoints = endpoints();
    receive(&mut endpoints[0], wire(Kind::NeedsAttention), now);
    tick_quiet(&mut endpoints, config(0), true, now);
    let snapshot = endpoints[0].live.snapshot.clone().unwrap();
    let notice = &endpoints[0].toasts.history.get(0).unwrap().notice;
    assert_eq!(
        notice.target(&snapshot),
        Some(NavigationTarget::Pane("w1:p1"))
    );
    let mut restarted = (*snapshot).clone();
    restarted.boot_id = "another boot".into();
    assert_eq!(notice.target(&restarted), None);
}
