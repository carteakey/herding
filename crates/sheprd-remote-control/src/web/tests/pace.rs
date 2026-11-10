use super::*;
use std::time::Duration;
use tokio::time::timeout;

/// Shorter than the regular interval, so only an echo read fits in it.
const SOONER: Duration = Duration::from_millis(200);

#[tokio::test]
async fn typing_into_the_pane_reads_its_screen_at_once() {
    let typed = Typed::default();
    let mut pace = typed.pace("p1");
    typed.typed("p1");
    timeout(SOONER, pace.wait())
        .await
        .expect("an echo read is due at once");
}

#[tokio::test]
async fn typing_keeps_reading_for_a_slower_redraw() {
    let typed = Typed::default();
    let mut pace = typed.pace("p1");
    typed.typed("p1");
    pace.wait().await;
    // The second and third echo reads, both before the regular one.
    timeout(SOONER, pace.wait()).await.expect("the 50 ms read");
    timeout(SOONER, pace.wait()).await.expect("the 150 ms read");
}

#[tokio::test]
async fn typing_into_another_pane_leaves_the_timer_alone() {
    let typed = Typed::default();
    let mut pace = typed.pace("p1");
    typed.typed("p2");
    assert!(timeout(SOONER, pace.wait()).await.is_err());
}

#[tokio::test]
async fn a_follower_without_a_signal_still_reads_on_the_timer() {
    let typed = Typed::default();
    let mut pace = typed.pace("p1");
    drop(typed);
    timeout(Duration::from_secs(2), pace.wait())
        .await
        .expect("the regular read");
}

#[tokio::test]
async fn a_follower_hears_only_its_own_device_type() {
    let typed = Typed::default();
    let phone = DeviceId("phone".to_owned());
    let other = DeviceId("other".to_owned());
    let mut mine = typed.pace_for("p1", &phone);
    let mut theirs = typed.pace_for("p1", &other);
    let mut web = typed.pace("p1");
    typed.typed_by("p1", &phone);
    for pace in [&mut mine, &mut theirs, &mut web] {
        timeout(SOONER, pace.wait())
            .await
            .expect("an echo read for every follower");
    }
    assert!(mine.typed_here());
    assert!(!mine.typed_here(), "told once");
    assert!(!theirs.typed_here());
    assert!(!web.typed_here());
    // A read the daemon asks for itself is nobody's typing.
    typed.typed("p1");
    timeout(SOONER, mine.wait()).await.expect("the read");
    assert!(!mine.typed_here());
}
