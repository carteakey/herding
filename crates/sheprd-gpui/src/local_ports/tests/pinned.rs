//! A held process, against the real kernel.

use crate::local_ports::pinned::Pinned;
use std::{os::unix::process::ExitStatusExt, process::Command};
use sysinfo::Signal;

fn sleeper() -> std::process::Child {
    Command::new("/bin/sh")
        .args(["-c", "exec sleep 30"])
        .spawn()
        .unwrap()
}

#[test]
fn a_signal_reaches_the_process_held() {
    let mut child = sleeper();
    let Some(held) = Pinned::open(child.id()) else {
        // A kernel older than 5.3, or a sandbox that blocks the call.
        child.kill().unwrap();
        child.wait().unwrap();
        return;
    };
    assert!(held.send(Signal::Term));
    assert_eq!(child.wait().unwrap().signal(), Some(15));
}

#[test]
fn a_held_process_that_exited_takes_no_signal_whatever_has_its_pid() {
    let mut child = sleeper();
    let held = Pinned::open(child.id());
    child.kill().unwrap();
    child.wait().unwrap();
    if let Some(held) = held {
        assert!(!held.send(Signal::Term));
    }
}

#[test]
fn nothing_is_held_for_a_pid_no_process_has() {
    assert!(Pinned::open(0).is_none());
    assert!(Pinned::open(u32::MAX).is_none());
}
