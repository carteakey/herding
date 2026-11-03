use super::*;

#[test]
fn failed_submission_does_not_block_future_refreshes() {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        ..Bridge::default()
    };
    bridge.refresh("team");
    assert_eq!(bridge.snapshot_in_flight, None);
}

#[test]
fn stale_snapshot_does_not_clear_or_replace_newer_request() {
    let mut bridge = Bridge {
        selected: Some("team".into()),
        snapshot_in_flight: Some(2),
        ..Bridge::default()
    };
    bridge.pending.insert(
        1,
        Pending::Snapshot {
            team_id: "team".into(),
        },
    );
    bridge.complete(Completion {
        request_id: 1,
        result: Ok(Reply::Snapshot {
            team: None,
            goal: None,
            posts: Vec::new(),
            tasks: Vec::new(),
            findings: Vec::new(),
        }),
    });
    assert_eq!(bridge.snapshot_in_flight, Some(2));
    assert!(!bridge.dirty);
}
