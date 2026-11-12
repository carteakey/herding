//! The poll's script, its answer parse, and the per-endpoint schedule: one
//! poll in flight per endpoint, retention on the connected set only, and a
//! failure backoff that doubles to a cap and resets on an answer.

use super::*;

#[test]
fn the_script_prefers_named_candidates_then_the_path() {
    let script = pane_list_script(None);
    assert!(script.contains("exec \"$c\" pane list"), "{script}");
    assert!(script.contains("exec herdr pane list"), "{script}");
    assert!(
        script.contains("/home/linuxbrew/.linuxbrew/bin/herdr"),
        "{script}"
    );
    assert!(script.contains("$HOME/.cargo/bin/herdr"), "{script}");
    let session = pane_list_script(Some("agent:zcode:ws:w1"));
    assert!(
        session.contains("exec \"$c\" --session 'agent:zcode:ws:w1' pane list"),
        "a non-default session is named: {session}"
    );
    assert!(!pane_list_script(Some("default")).contains("--session"));
}

#[test]
fn the_answer_parses_panes_and_skips_missing_revisions() {
    let answer: PaneListAnswer = serde_json::from_str(
        r#"{
            "id": "cli:pane:list",
            "result": {
                "panes": [
                    {"pane_id": "w1:p1", "revision": 107108, "cwd": "/repo",
                     "scroll": {"offset_from_bottom": 0}, "terminal_id": "term_1"},
                    {"pane_id": "w1:p2", "cwd": "/repo"},
                    {"pane_id": "w1:p3", "revision": 4}
                ]
            },
            "type": "pane_list"
        }"#,
    )
    .expect("the daemon's answer shape parses");
    let panes: Vec<(String, u64)> = answer
        .result
        .panes
        .into_iter()
        .filter_map(|pane| pane.revision.map(|revision| (pane.pane_id, revision)))
        .collect();
    assert_eq!(
        panes,
        vec![("w1:p1".to_owned(), 107108), ("w1:p3".to_owned(), 4)],
        "a pane without a revision stays unobservable, the rest carry theirs"
    );
}

#[test]
fn a_malformed_answer_is_an_error() {
    let parsed: Result<PaneListAnswer, _> = serde_json::from_str("not json");
    assert!(parsed.is_err(), "garbage is a poll failure");
}

#[test]
fn a_truncated_answer_is_an_error() {
    // An envelope cut off mid-structure, which is what the output limit
    // leaves behind when an answer outgrows it.
    let full = r#"{"id": "c", "result": {"panes": [{"pane_id": "w1:p1", "re"#;
    let parsed: Result<PaneListAnswer, _> = serde_json::from_str(full);
    assert!(parsed.is_err(), "a truncated envelope is a poll failure");
    let oversized = format!(
        r#"{{"id": "c", "result": {{"panes": [{{"pane_id": "w1:p1", "revision": 1, "junk": "{}""#,
        "x".repeat(LIMITS.output as usize + 1),
    );
    let parsed: Result<PaneListAnswer, _> = serde_json::from_str(&oversized);
    assert!(parsed.is_err(), "an oversized envelope cannot parse whole");
}

fn target() -> Option<PaneListTarget> {
    Some(PaneListTarget::Local { session: None })
}

#[test]
fn one_poll_in_flight_per_endpoint_and_a_stale_result_is_dropped() {
    let (mut polls, updates, requests) = PaneListPolls::fake();
    let now = Instant::now();
    polls.poll([("m1".to_owned(), target())], now, Duration::from_secs(10));
    assert_eq!(requests.try_recv().unwrap().endpoint, "m1");
    assert!(polls.in_flight_for_test("m1"));

    // Due again later, but the previous poll has not answered: no second
    // request, so a slow SSH host cannot fill the queue and starve the rest.
    polls.poll(
        [("m1".to_owned(), target()), ("local".to_owned(), target())],
        now + Duration::from_secs(10),
        Duration::from_secs(10),
    );
    let sent: Vec<String> = std::iter::from_fn(|| requests.try_recv().ok())
        .map(|request| request.endpoint)
        .collect();
    assert_eq!(
        sent,
        vec!["local".to_owned()],
        "m1 is still in flight; only the other endpoint fires"
    );

    // The answer lands after the endpoint disconnected: dropped, not
    // remembered, and the in-flight flag clears so a reconnect polls again.
    updates
        .send(PaneListUpdate {
            endpoint: "m1".into(),
            panes: Ok(vec![("m1:p2".into(), 5)]),
        })
        .unwrap();
    polls.poll(
        [("local".to_owned(), target())],
        now + Duration::from_secs(11),
        Duration::from_secs(10),
    );
    assert!(!polls.in_flight_for_test("m1"));
    assert!(
        !polls.remembers_for_test("m1"),
        "a stale result is not kept"
    );
}

#[test]
fn a_disconnected_endpoint_loses_its_remembered_panes_and_schedule() {
    let (mut polls, updates, requests) = PaneListPolls::fake();
    let now = Instant::now();
    polls.poll([("m1".to_owned(), target())], now, Duration::from_secs(10));
    assert!(requests.try_recv().is_ok());
    updates
        .send(PaneListUpdate {
            endpoint: "m1".into(),
            panes: Ok(vec![("m1:p2".into(), 5)]),
        })
        .unwrap();
    polls.poll([("m1".to_owned(), target())], now, Duration::from_secs(10));
    assert!(polls.remembers_for_test("m1"));

    // The endpoint disconnects: retention drops its remembered panes with
    // its schedule, so a member on a host that is merely down is never
    // judged on frozen counters.
    polls.poll(
        [("local".to_owned(), target())],
        now + Duration::from_secs(1),
        Duration::from_secs(10),
    );
    assert!(!polls.remembers_for_test("m1"));
    assert!(polls.due_for_test("m1").is_none());
}

#[test]
fn a_failure_backs_off_doubling_to_the_cap_and_resets_on_an_answer() {
    let (mut polls, updates, requests) = PaneListPolls::fake();
    let cadence = Duration::from_secs(10);
    let mut now = Instant::now();
    let send = |updates: &mpsc::SyncSender<PaneListUpdate>, ok: bool| {
        updates
            .send(PaneListUpdate {
                endpoint: "m1".into(),
                panes: if ok {
                    Ok(vec![("m1:p2".into(), 1)])
                } else {
                    Err(Error::Answer(
                        serde_json::from_str::<serde_json::Value>("broken").unwrap_err(),
                    ))
                },
            })
            .unwrap();
    };

    polls.poll([("m1".to_owned(), target())], now, cadence);
    assert!(requests.try_recv().is_ok());
    send(&updates, false);
    polls.poll([("m1".to_owned(), target())], now, cadence);
    assert!(polls.failing_for_test("m1"));
    let first_retry = polls.due_for_test("m1").unwrap();
    assert_eq!(
        first_retry - now,
        Duration::from_secs(20),
        "the first failure doubles the cadence"
    );

    // Second failure: 40s. Third: capped at 60s.
    for expected in [Duration::from_secs(40), Duration::from_secs(60)] {
        now = polls.due_for_test("m1").unwrap();
        polls.poll([("m1".to_owned(), target())], now, cadence);
        assert!(
            requests.try_recv().is_ok(),
            "the backoff elapsed, so the poll fires"
        );
        send(&updates, false);
        polls.poll([("m1".to_owned(), target())], now, cadence);
        assert_eq!(polls.due_for_test("m1").unwrap() - now, expected);
    }

    // An answer resets the episode: the next failure starts at the first
    // backoff again, not at the cap.
    now = polls.due_for_test("m1").unwrap();
    polls.poll([("m1".to_owned(), target())], now, cadence);
    assert!(requests.try_recv().is_ok());
    send(&updates, true);
    polls.poll([("m1".to_owned(), target())], now, cadence);
    assert!(!polls.failing_for_test("m1"));
    assert_eq!(polls.due_for_test("m1").unwrap() - now, cadence);
}
