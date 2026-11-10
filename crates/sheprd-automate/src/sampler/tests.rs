use super::*;
use crate::testing::Fixture;

#[test]
fn only_a_closed_pane_ends_an_agent_not_a_failed_read() {
    let fixture = Fixture::new();
    let store = fixture.store();
    for (position, pane) in [(1, "w1p1"), (2, "gone"), (3, "flaky")] {
        store
            .add_agent(&AgentRow {
                run: 1,
                position,
                path: "script".to_owned(),
                harness: "claude".to_owned(),
                pane: pane.to_owned(),
                cwd: "/tmp".to_owned(),
                started_at: now(),
                ..AgentRow::default()
            })
            .unwrap();
    }
    let home = fixture.root();
    Sampler::default()
        .sample(&store, &fixture.drive, &home)
        .unwrap();
    let live = store
        .agents(true)
        .unwrap()
        .into_iter()
        .map(|agent| agent.pane)
        .collect::<Vec<_>>();
    assert!(live.contains(&"w1p1".to_owned()), "{live:?}");
    assert!(
        live.contains(&"flaky".to_owned()),
        "a failed read is not an exit: {live:?}"
    );
    assert!(!live.contains(&"gone".to_owned()), "{live:?}");
}

fn core_terminal(address: &str, state: &str, status: Option<&str>) -> Terminal {
    let status = status.map(|state| serde_json::json!({"app": "claude-code", "state": state}));
    serde_json::from_value(serde_json::json!({
        "address": address, "state": state, "pid": 42, "status": status,
    }))
    .unwrap()
}

#[test]
fn a_core_terminal_gone_or_finished_ends_its_agent() {
    let terminals = [
        core_terminal("t1", "live", Some("working")),
        core_terminal("t2", "finished", None),
        core_terminal("t3", "live", Some("idle")),
    ];
    let working = look_core(&terminals, "t1");
    assert!(!working.gone && !working.quiet);
    assert_eq!(working.root, Some(42));
    assert!(look_core(&terminals, "t2").gone);
    assert!(look_core(&terminals, "t3").quiet);
    assert!(look_core(&terminals, "t9").gone, "removed is gone");
}

#[test]
fn a_kept_core_terminal_whose_agent_was_lost_ends_its_agent() {
    let lost: Terminal = serde_json::from_value(serde_json::json!({
        "address": "t1", "state": "live", "reason": "the program was killed",
    }))
    .unwrap();
    assert!(look_core(&[lost], "t1").gone);
}

#[test]
fn a_herdr_1_agent_outlives_a_switch_to_herdr_core_until_its_day_ends() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let agent = |position, started_at| AgentRow {
        run: 1,
        position,
        path: "script".to_owned(),
        harness: "claude".to_owned(),
        pane: "w1p1".to_owned(),
        cwd: "/tmp".to_owned(),
        started_at,
        ..AgentRow::default()
    };
    store.add_agent(&agent(1, now())).unwrap();
    store.add_agent(&agent(2, now() - AT_MOST)).unwrap();
    // Herdr 1's panes cannot be seen through a core, so nothing ends but
    // the agent past its day.
    let drive = Drive::HerdrCore(HerdrCore::new(None));
    Sampler::default()
        .sample(&store, &drive, &fixture.root())
        .unwrap();
    let live: Vec<u32> = store
        .agents(true)
        .unwrap()
        .into_iter()
        .map(|agent| agent.position)
        .collect();
    assert_eq!(live, [1]);
}
