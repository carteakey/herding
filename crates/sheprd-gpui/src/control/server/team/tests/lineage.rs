/// The lineage contract against the real /proc reader, with live processes:
/// these complement the synthetic-table contract test above by proving the
/// walk against the kernel, including the re-parented-orphan case.
/// The lineage contract against the real /proc reader, with live processes:
/// these complement the synthetic-table contract test above by proving the
/// walk against the kernel, including the re-parented-orphan case.
use super::*;
use std::time::Duration;

fn target_fixture() -> SessionTarget {
    SessionTarget {
        host: "boot".into(),
        session: "agent:codex:w0:w0:p1".into(),
        workspace: Some("w0".into()),
        pane: Some("w0:p1".into()),
    }
}

fn bindings_for(agent: ProcessIdentity) -> PeerBindings {
    [(target_fixture(), Some(agent))]
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>()
}

fn caller(pid: u32) -> Option<PeerCredentials> {
    Some(PeerCredentials {
        uid: nix::unistd::geteuid().as_raw(),
        gid: nix::unistd::getgid().as_raw(),
        pid: Some(pid),
    })
}

fn local_uid() -> Option<u32> {
    Some(nix::unistd::geteuid().as_raw())
}

/// Waits until `pid` no longer descends from this test process: a
/// `sh -c 'sleep 5 &'` orphan is still a child of the short-lived shell
/// for a few ticks, which would make the bystander look like a
/// descendant of the agent. Re-parenting lands on a systemd user
/// subreaper rather than init, so the parent pid alone cannot be checked.
fn wait_detached_from_test(pid: u32) {
    let self_pid = std::process::id();
    for _ in 0..40 {
        let mut current = pid;
        let mut reaches_test = false;
        for _ in 0..16 {
            if current == self_pid {
                reaches_test = true;
                break;
            }
            let Some((parent, _)) = process_info(current) else {
                break;
            };
            if parent <= 1 {
                break;
            }
            current = parent;
        }
        if !reaches_test {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the orphan process still descends from this test process");
}

#[test]
fn the_agent_process_itself_is_accepted() {
    let pid = std::process::id();
    let (_, started) = process_info(pid).expect("this test's own process is readable");
    let bindings = bindings_for(ProcessIdentity {
        pid,
        started_ticks: Some(started),
    });
    assert!(
        check_peer(
            &bindings,
            &target_fixture(),
            caller(pid),
            true,
            local_uid(),
            &process_info
        )
        .is_ok()
    );
}

#[test]
fn a_descendant_of_the_agent_is_accepted() {
    let mut child = std::process::Command::new("sleep")
        .arg("5")
        .spawn()
        .expect("a child process spawns");
    let pid = std::process::id();
    let (_, started) = process_info(pid).expect("this test's own process is readable");
    let bindings = bindings_for(ProcessIdentity {
        pid,
        started_ticks: Some(started),
    });
    assert!(
        check_peer(
            &bindings,
            &target_fixture(),
            caller(child.id()),
            true,
            local_uid(),
            &process_info
        )
        .is_ok()
    );
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn a_bystander_outside_the_lineage_is_refused() {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("sleep 5 & echo $!")
        .output()
        .expect("an orphan process spawns");
    let pid: u32 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("the orphan prints its pid");
    wait_detached_from_test(pid);

    let pid_self = std::process::id();
    let (_, started) = process_info(pid_self).expect("this test's own process is readable");
    let bindings = bindings_for(ProcessIdentity {
        pid: pid_self,
        started_ticks: Some(started),
    });
    let refused = check_peer(
        &bindings,
        &target_fixture(),
        caller(pid),
        true,
        local_uid(),
        &process_info,
    )
    .expect_err("a re-parented bystander is not a descendant of the agent");
    let Response::Error { message, .. } = &refused else {
        panic!("an error response is expected");
    };
    assert!(message.contains("descend"), "{message}");
}

#[test]
fn a_reused_pid_with_a_different_start_time_is_refused() {
    let mut child = std::process::Command::new("sleep")
        .arg("5")
        .spawn()
        .expect("a child process spawns");
    // The child's pid with a start time that belongs to no real process:
    // exactly what a reused pid looks like to the check.
    let bindings = bindings_for(ProcessIdentity {
        pid: child.id(),
        started_ticks: Some(1),
    });
    assert!(
        check_peer(
            &bindings,
            &target_fixture(),
            caller(child.id()),
            true,
            local_uid(),
            &process_info
        )
        .is_err()
    );
    let _ = child.kill();
    let _ = child.wait();
}

/// The wrapper-shell regression for issue #3: a tool shell can carry the
/// agent kind in its own command line (a ZCode snapshot shell runs
/// `zsh -c . '~/.zcode/cli/...'`), so the first match on the walk is the
/// ephemeral wrapper, not the agent - anchoring it pinned a process gone
/// by the next CLI call and locked every later call out. The walk must
/// climb through consecutive matches and anchor the topmost one: the
/// long-lived agent the later calls descend from.
#[test]
fn the_walk_anchors_above_a_kind_carrying_wrapper_shell() {
    use std::io::BufRead as _;

    // The "agent" is a shell whose command line carries the marker (in
    // the script text) and which runs the "wrapper" - another shell
    // carrying the marker as $0, the way a snapshot shell repeats the
    // agent's path - waiting on it so both stay alive for the walk.
    let marker = "zcodesim-agent-kind";
    let mut agent = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("sh -c 'sleep 10' {marker} & echo $!; wait"))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("the simulated agent spawns");
    let mut line = String::new();
    std::io::BufReader::new(agent.stdout.take().expect("the agent's stdout is piped"))
        .read_line(&mut line)
        .expect("the agent prints the wrapper's pid");
    let wrapper_pid: u32 = line.trim().parse().expect("the wrapper's pid is printed");

    let cmdline = |pid: u32| {
        String::from_utf8_lossy(
            &std::fs::read(format!("/proc/{pid}/cmdline"))
                .expect("the process is still alive for its command line to be read"),
        )
        .replace('\0', " ")
    };
    assert!(cmdline(wrapper_pid).contains(marker));
    assert!(cmdline(agent.id()).contains(marker));

    let anchored =
        agent_lineage(wrapper_pid, marker).expect("a kind-carrying ancestor anchors the walk");
    assert_ne!(
        anchored.pid, wrapper_pid,
        "the ephemeral wrapper must not be the anchor (issue #3)"
    );
    assert_eq!(
        anchored.pid,
        agent.id(),
        "the topmost consecutive kind-carrying ancestor is the anchor"
    );
    let (_, agent_started) = process_info(agent.id()).expect("the agent is readable");
    assert_eq!(anchored.started_ticks, Some(agent_started));

    assert!(
        agent_lineage(wrapper_pid, "zcodesim-kind-nobody-carries").is_none(),
        "no ancestor carries this kind, so nothing anchors"
    );

    let _ = agent.kill();
    let _ = agent.wait();
}
