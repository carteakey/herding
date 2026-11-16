//! A herdr-core terminal's processes, named by its core's terminal record.

use super::*;
use sheprd_core_client::wire::Terminal;

fn record(json: serde_json::Value) -> Terminal {
    serde_json::from_value(json).unwrap()
}

#[test]
fn a_core_terminal_names_its_program_and_foreground_job_as_herdr_does() {
    let busy = record(serde_json::json!({
        "address": "t7", "state": "live", "pid": 4100,
        "foreground": {"pid": 4200, "argv": ["cargo", "test"], "cwd": "/src", "name": "cargo"}
    }));
    let info = core_info(&busy);
    assert_eq!(info.shell_pid, Some(4100));
    assert_eq!(info.foreground_pids(), [4200]);
    assert_eq!(info.foreground_processes[0].cwd.as_deref(), Some("/src"));
}

#[test]
fn an_idle_core_terminal_has_no_foreground_job() {
    let idle = record(serde_json::json!({
        "address": "t7", "state": "live", "pid": 4100,
        "foreground": {"pid": 4100, "argv": ["zsh"], "cwd": "", "name": "zsh"}
    }));
    assert!(core_info(&idle).foreground_pids().is_empty());
    // A lost program leaves no process to list under.
    let lost = record(serde_json::json!({"address": "t7", "state": "live", "reason": "exited"}));
    assert_eq!(core_info(&lost).shell_pid, None);
    let bogus = record(serde_json::json!({"address": "t7", "state": "live", "pid": -1}));
    assert_eq!(core_info(&bogus).shell_pid, None);
}

#[test]
fn a_core_source_is_named_by_its_terminal() {
    let source = Source::Core {
        endpoint: sheprd_core_client::Endpoint {
            socket: "/tmp/hc.sock".into(),
            session: "default".into(),
        },
        terminal: "t7".into(),
    };
    assert_eq!(source.name(), "t7");
}

#[test]
fn a_terminal_on_another_machine_is_refused_before_asking_the_core() {
    let endpoint = sheprd_core_client::Endpoint {
        // Nothing answers here: the refusal must come first.
        socket: "/nonexistent/hc.sock".into(),
        session: "default".into(),
    };
    assert!(matches!(
        core_process_info(&endpoint, "box/t7"),
        Err(crate::Error::Processes(Error::RemoteTerminal))
    ));
}
