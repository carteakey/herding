use super::{MockCore, connected, terminal};
use crate::{
    Error,
    agent::{self, Until},
    wire::Run,
};
use serde_json::{Value, json};
use std::{
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};

fn waited(ended: bool, reason: &str, turn: u64) -> Value {
    json!({"terminal": "t1", "ended": ended, "reason": reason, "turn": turn,
        "children": [], "status": if ended { "idle" } else { "working" }})
}

fn until(stop: &AtomicBool) -> Until<'_> {
    Until {
        deadline: Instant::now() + Duration::from_secs(30),
        stop,
    }
}

/// Answers the next call, checking it is `method`, and hands it back.
fn expect(core: &mut MockCore, method: &str, result: Value) -> Value {
    let request = core.request();
    assert_eq!(request["method"], method, "{request}");
    core.answer(&request, result);
    request
}

#[test]
fn the_prompt_waits_for_the_agent_to_be_ready() {
    let (client, mut core) = connected();
    let launched = thread::spawn(move || {
        let stop = AtomicBool::new(false);
        let argv = ["claude".to_owned()];
        let run = Run {
            argv: &argv,
            cwd: Some("/work"),
            ..Run::default()
        };
        agent::launch(&client, &run, "fix it", until(&stop))
    });
    let run = expect(&mut core, "terminal.run", terminal("t1"));
    assert_eq!(run["params"]["argv"], json!(["claude"]));
    // Still booting: the wait checks in, and is asked again.
    let wait = expect(&mut core, "terminal.wait", waited(false, "timeout", 0));
    assert_eq!(wait["params"]["terminal"], "t1");
    assert!(wait["params"]["timeout"].as_f64().unwrap() <= 5.);
    expect(&mut core, "terminal.wait", waited(true, "status", 0));
    let send = expect(
        &mut core,
        "terminal.send",
        json!({"started": true, "status": "idle", "turn": 0}),
    );
    assert_eq!(send["params"], json!({"terminal": "t1", "text": "fix it"}));
    let started = launched.join().unwrap().unwrap();
    assert_eq!(started.terminal.address, "t1");
    assert_eq!(started.sent.turn, 0);
}

#[test]
fn an_agent_that_exits_before_it_is_ready_gets_no_prompt() {
    let (client, mut core) = connected();
    let launched = thread::spawn(move || {
        let stop = AtomicBool::new(false);
        agent::launch(&client, &Run::default(), "fix it", until(&stop))
    });
    expect(&mut core, "terminal.run", terminal("t1"));
    let mut exit = waited(true, "exit", 0);
    exit["exit_code"] = json!(127);
    expect(&mut core, "terminal.wait", exit);
    let error = launched.join().unwrap().unwrap_err();
    assert!(
        matches!(error, Error::AgentExited { code: Some(127) }),
        "{error}"
    );
}

#[test]
fn finishing_waits_for_the_turn_the_prompt_started() {
    let (client, mut core) = connected();
    let finished = thread::spawn(move || {
        let stop = AtomicBool::new(false);
        agent::finish(&client, "t1", 0, until(&stop))
    });
    // Idle on the prompt's own turn: it has not started on it yet.
    expect(&mut core, "terminal.wait", waited(true, "status", 0));
    expect(&mut core, "terminal.wait", waited(false, "timeout", 1));
    expect(&mut core, "terminal.wait", waited(true, "status", 1));
    assert_eq!(finished.join().unwrap().unwrap().turn, 1);
}

#[test]
fn a_stop_request_ends_the_wait_without_asking_the_core() {
    let (client, _core) = connected();
    let stop = AtomicBool::new(true);
    let error = agent::finish(&client, "t1", 0, until(&stop)).unwrap_err();
    assert!(matches!(error, Error::Stopped), "{error}");
}

#[test]
fn a_passed_deadline_while_working_is_busy() {
    let (client, _core) = connected();
    let stop = AtomicBool::new(false);
    let past = Until {
        deadline: Instant::now(),
        stop: &stop,
    };
    let error = agent::finish(&client, "t1", 0, past).unwrap_err();
    assert!(matches!(error, Error::AgentBusy), "{error}");
    // Silence is not readiness: a dialog may be waiting for an answer.
    let error = agent::ready(&client, "t1", past).unwrap_err();
    assert!(
        matches!(error, Error::AgentNotReady { status: None }),
        "{error}"
    );
}

#[test]
fn an_agent_on_a_dialog_gets_no_prompt() {
    let (client, mut core) = connected();
    let prompted = thread::spawn(move || {
        let stop = AtomicBool::new(false);
        agent::prompt(&client, "t1", "fix it", until(&stop))
    });
    let mut blocked = waited(true, "status", 0);
    blocked["status"] = json!("blocked");
    expect(&mut core, "terminal.wait", blocked);
    let error = prompted.join().unwrap().unwrap_err();
    assert!(
        matches!(error, Error::AgentNotReady { status: Some(ref status) } if status == "blocked"),
        "{error}"
    );
}

#[test]
fn a_question_mid_turn_keeps_the_wait_going_and_an_error_fails_it() {
    let (client, mut core) = connected();
    let finished = thread::spawn(move || {
        let stop = AtomicBool::new(false);
        agent::finish(&client, "t1", 0, until(&stop))
    });
    let mut blocked = waited(true, "status", 1);
    blocked["status"] = json!("blocked");
    expect(&mut core, "terminal.wait", blocked);
    let mut failed = waited(true, "status", 1);
    failed["status"] = json!("error");
    expect(&mut core, "terminal.wait", failed);
    let error = finished.join().unwrap().unwrap_err();
    assert!(matches!(error, Error::AgentFailed { .. }), "{error}");
}

#[test]
fn a_stopped_or_late_step_starts_nothing() {
    let stop = AtomicBool::new(false);
    let late = Until {
        deadline: Instant::now(),
        stop: &stop,
    };
    assert!(matches!(late.check(), Err(Error::Expired)));
    let stopped = AtomicBool::new(true);
    assert!(matches!(until(&stopped).check(), Err(Error::Stopped)));
    until(&stop).check().unwrap();
}
