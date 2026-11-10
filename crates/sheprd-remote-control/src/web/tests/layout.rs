//! A phone's split or zoom of one of Herding's Herdr 2 tabs waits for Herding,
//! which keeps the layout, to make it.

use super::*;
use crate::control::{Request, Response, layout};

/// Stands in for Herding: takes one layout change over the control socket's
/// handler and answers it.
fn desktop(state: &Shared, respond: impl FnOnce(&layout::Ask) -> layout::Answer + Send + 'static) {
    let state = state.clone();
    std::thread::spawn(move || {
        let (stop, _) = tokio::sync::watch::channel(false);
        let Response::ChangeAsks { asks, layout } =
            crate::server::answer_for_tests(&state, &stop, Request::ChangeAsks)
        else {
            panic!("the daemon did not hand over the change");
        };
        assert!(asks.is_empty());
        let ask = layout.first().expect("a change");
        let answer = respond(ask);
        let id = ask.id;
        assert_eq!(
            crate::server::answer_for_tests(&state, &stop, Request::LayoutAnswer { id, answer }),
            Response::Done
        );
    });
}

fn open(state: &Shared) {
    let (stop, _) = tokio::sync::watch::channel(false);
    crate::server::answer_for_tests(state, &stop, Request::Status);
}

#[tokio::test]
async fn sheprd_makes_the_change_and_the_phone_hears_when() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with(dir.path(), Hub::offline(Layout::default()));
    open(&state);
    desktop(&state, |ask| {
        assert_eq!((ask.tab, ask.terminal.as_str()), (4, "t1"));
        assert_eq!(ask.op, layout::Op::SplitDown);
        layout::Answer::Split("t9".into())
    });
    // The phone is told the new shell, to show it next.
    let created =
        crate::native::changes::change_layout(&state, 4, "t1".into(), layout::Op::SplitDown)
            .await
            .unwrap();
    assert_eq!(created.as_deref(), Some("t9"));
}

#[test]
fn a_split_answer_names_its_shell_on_the_wire() {
    let answer = layout::Answer::Split("t9".into());
    let wire = serde_json::to_value(&answer).unwrap();
    assert_eq!(wire, serde_json::json!({"answer": "split", "detail": "t9"}));
    assert_eq!(
        serde_json::from_value::<layout::Answer>(wire).unwrap(),
        answer
    );
}

#[tokio::test]
async fn a_refusal_reaches_the_phone_as_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with(dir.path(), Hub::offline(Layout::default()));
    open(&state);
    desktop(&state, |_| {
        layout::Answer::Failed(layout::Failure::Watching)
    });
    let error =
        crate::native::changes::change_layout(&state, 4, "t1".into(), layout::Op::SplitRight)
            .await
            .unwrap_err();
    assert_eq!(error.into_response().status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn without_sheprd_the_phone_is_told_so() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with(dir.path(), Hub::offline(Layout::default()));
    let error = crate::native::changes::change_layout(&state, 4, "t1".into(), layout::Op::Zoom)
        .await
        .unwrap_err();
    assert_eq!(
        error.into_response().status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[test]
fn an_unknown_failure_from_a_newer_sheprd_reads_as_refused() {
    let answer: layout::Answer =
        serde_json::from_value(serde_json::json!({"answer": "failed", "detail": "something_new"}))
            .unwrap();
    assert_eq!(answer, layout::Answer::Failed(layout::Failure::Refused));
}

#[test]
fn an_older_daemon_sends_no_layout_changes() {
    let response: Response =
        serde_json::from_value(serde_json::json!({"response": "change_asks", "asks": []})).unwrap();
    assert_eq!(
        response,
        Response::ChangeAsks {
            asks: Vec::new(),
            layout: Vec::new()
        }
    );
}

#[tokio::test]
async fn a_busy_sheprd_asks_the_phone_to_try_again() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with(dir.path(), Hub::offline(Layout::default()));
    open(&state);
    desktop(&state, |_| layout::Answer::Failed(layout::Failure::Busy));
    let error =
        crate::native::changes::change_layout(&state, 4, "t1".into(), layout::Op::SplitRight)
            .await
            .unwrap_err();
    assert_eq!(
        error.into_response().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}
