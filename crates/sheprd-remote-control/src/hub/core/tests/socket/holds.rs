//! A phone holds a Herdr 2 terminal at its size through the pane's shared
//! screen, and letting go hands the size back to the driver before.

use super::*;

fn next(fake: &Fake) -> Value {
    fake.attaches.recv_timeout(Duration::from_secs(2)).unwrap()
}

#[test]
fn a_hold_drives_at_the_phone_size_and_letting_go_observes_again() {
    let (source, fake) = fake();
    source.poll().unwrap();
    let hold = source.hold(&id("t1"), 46, 30).unwrap();
    let watching = next(&fake);
    assert_eq!(watching["observe"], true);
    assert_eq!(watching["history"], screen::HISTORY);
    let driving = next(&fake);
    assert!(driving.get("observe").is_none());
    assert!(driving.get("history").is_none(), "history is painted once");
    assert_eq!(
        (&driving["cols"], &driving["rows"]),
        (&json!(46), &json!(30))
    );

    hold.resize(50, 32).unwrap();
    let resized = fake.resizes.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(resized, (50, 32));
    assert!(
        fake.attaches.try_recv().is_err(),
        "a resize keeps the attach"
    );

    drop(hold);
    let observing = next(&fake);
    assert_eq!(observing["observe"], true);
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn the_latest_hold_wins_until_it_lets_go() {
    let (source, fake) = fake();
    source.poll().unwrap();
    let first = source.hold(&id("t1"), 46, 30).unwrap();
    next(&fake);
    next(&fake);
    let second = source.hold(&id("t1"), 60, 40).unwrap();
    assert_eq!(
        fake.resizes.recv_timeout(Duration::from_secs(2)).unwrap(),
        (60, 40)
    );
    drop(second);
    assert_eq!(
        fake.resizes.recv_timeout(Duration::from_secs(2)).unwrap(),
        (46, 30)
    );
    assert!(fake.attaches.try_recv().is_err(), "one hold still drives");
    drop(first);
    assert_eq!(next(&fake)["observe"], true);
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_held_screen_neither_expires_nor_makes_way() {
    let (source, fake) = fake();
    source.poll().unwrap();
    let hold = source.hold(&id("t1"), 46, 30).unwrap();
    {
        let state = lock(&source.state);
        let mut screen = lock(&state.screens["t1"]);
        screen.age(Duration::from_secs(600));
        assert!(!screen.expired());
        assert_eq!(screen.idle(), Duration::ZERO);
    }
    source.poll().unwrap();
    assert!(lock(&source.state).screens.contains_key("t1"));
    drop(hold);
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn an_unknown_pane_or_size_holds_nothing() {
    let (source, fake) = fake();
    source.poll().unwrap();
    assert!(source.hold(&id("t404"), 46, 30).is_err());
    assert!(source.hold(&id("t1"), 0, 30).is_err());
    assert!(fake.attaches.try_recv().is_err());
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_refused_hold_leaves_nothing_to_release() {
    let (source, fake) = fake();
    source.poll().unwrap();
    // The fake refuses an attach 13 columns wide.
    assert!(source.hold(&id("t1"), 13, 30).is_err());
    let watching = next(&fake);
    assert_eq!(watching["observe"], true);
    {
        let state = lock(&source.state);
        let mut screen = lock(&state.screens["t1"]);
        screen.age(Duration::from_secs(600));
        assert!(screen.expired(), "no hold keeps it");
    }
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_driver_that_takes_the_size_back_is_followed() {
    let (source, fake) = fake();
    source.poll().unwrap();
    let hold = source.hold(&id("t1"), 46, 30).unwrap();
    let screen = lock(&source.state).screens["t1"].clone();
    let listed = |cols: u16| {
        let mut record: Terminal = serde_json::from_value(record("t1")).unwrap();
        record.cols = cols;
        record.rows = 30;
        record
    };
    let mut screen = lock(&screen);
    // A listing from before the hold took is older than the hold.
    screen.follow_size(&listed(80)).unwrap();
    assert_eq!(screen.size().cols(), 46);
    screen.follow_size(&listed(46)).unwrap();
    assert_eq!(screen.size().cols(), 46);
    // Listed at the hold, then at another size: the desktop resized it.
    screen.follow_size(&listed(120)).unwrap();
    assert_eq!(screen.size().cols(), 120);
    drop(screen);
    drop(hold);
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn closing_a_view_leaves_a_size_the_desktop_took_back() {
    let (source, fake) = fake();
    source.poll().unwrap();
    let older = source.hold(&id("t1"), 46, 30).unwrap();
    next(&fake);
    next(&fake);
    let newer = source.hold(&id("t1"), 60, 40).unwrap();
    assert_eq!(
        fake.resizes.recv_timeout(Duration::from_secs(2)).unwrap(),
        (60, 40)
    );
    let screen = lock(&source.state).screens["t1"].clone();
    let listed = |cols: u16, rows: u16| {
        let mut record: Terminal = serde_json::from_value(record("t1")).unwrap();
        record.cols = cols;
        record.rows = rows;
        record
    };
    {
        let mut screen = lock(&screen);
        screen.follow_size(&listed(60, 40)).unwrap();
        // The desktop resized its pane after the phones' holds.
        screen.follow_size(&listed(120, 40)).unwrap();
    }
    drop(older);
    assert!(
        fake.resizes
            .recv_timeout(Duration::from_millis(200))
            .is_err(),
        "the desktop keeps its size"
    );
    assert_eq!(lock(&screen).size().cols(), 120);
    // A phone asking for its size again takes it back.
    let again = source.hold(&id("t1"), 60, 40).unwrap();
    assert_eq!(
        fake.resizes.recv_timeout(Duration::from_secs(2)).unwrap(),
        (60, 40)
    );
    drop(again);
    drop(newer);
    drop(screen);
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_hold_on_a_screen_that_was_replaced_counts_as_lost_and_lets_the_new_one_be() {
    let (source, fake) = fake();
    source.poll().unwrap();
    let hold = source.hold(&id("t1"), 46, 30).unwrap();
    next(&fake);
    next(&fake);
    assert!(hold.is_held());
    // A closed stream gives the pane a new screen, which holds nothing: the
    // phone's view holds it again (`Sizing::apply`).
    let old = lock(&source.state).screens.remove("t1").unwrap();
    let (core, terminal) = source.terminal(&id("t1")).unwrap();
    let replacement = source.screen(&core, &terminal).unwrap();
    assert_eq!(next(&fake)["observe"], true, "the new screen only watches");
    assert!(!hold.is_held());
    // Letting the old hold go must not release the new screen.
    drop(hold);
    assert!(fake.attaches.try_recv().is_err(), "nothing re-attached");
    drop((old, replacement, core));
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_last_hold_that_cannot_go_back_to_watching_takes_its_screen_along() {
    let (source, fake) = fake();
    source.poll().unwrap();
    let hold = source.hold(&id("t1"), 46, 30).unwrap();
    next(&fake);
    next(&fake);
    // The fake core refuses any attach 13 columns wide, so watching again
    // at the phone's last size fails.
    hold.resize(13, 30).unwrap();
    fake.resizes.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(hold);
    assert!(
        !lock(&source.state).screens.contains_key("t1"),
        "the driving attach went with its screen"
    );
    drop(source);
    fake.server.join().unwrap();
}
