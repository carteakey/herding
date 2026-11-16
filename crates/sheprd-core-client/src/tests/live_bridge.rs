//! A real herdr-core reached through the bridge another machine's core is
//! reached through, run by `/bin/sh` here in place of `ssh`. Opt in as
//! `live` does.

use super::live::Sandbox;
use crate::{Core, wire::Run};
use std::{process::Command, sync::atomic::AtomicBool};

#[test]
#[ignore = "needs HERDR_CORE_TEST_BINARY"]
fn a_bridged_core_runs_and_lists_terminals() {
    let binary = std::env::var_os("HERDR_CORE_TEST_BINARY").expect("HERDR_CORE_TEST_BINARY");
    let sandbox = Sandbox::start(binary.clone());
    // The bridge finds the core where an install puts it on the host.
    let home = sandbox.dir().join("home");
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    std::os::unix::fs::symlink(&binary, home.join(".local/bin/herdr-core")).unwrap();
    let mut shell = Command::new("/bin/sh");
    shell
        .arg("-c")
        .env_clear()
        .envs(sandbox.vars.iter().map(|(name, value)| (name, value)))
        // Only the linked core, never one installed on this machine.
        .env("PATH", "/usr/bin:/bin");
    let bridge = sheprd_client::core_bridge::command(shell, &AtomicBool::new(false)).unwrap();
    let core = Core::bridged(bridge).unwrap();
    assert_eq!(core.hello().generation, 1);

    let argv = ["sh".to_owned(), "-c".to_owned(), "sleep 30".to_owned()];
    let started = core
        .run(&Run {
            argv: &argv,
            name: Some("bridged"),
            ..Run::default()
        })
        .unwrap();
    let listed = core.list(false).unwrap();
    assert!(
        listed
            .terminals
            .iter()
            .any(|terminal| terminal.address == started.address)
    );
    // A label set, then removed with an empty value.
    let set = core
        .label(&started.address, &[("herdr-gpui-1", "open")].into())
        .unwrap();
    assert_eq!(
        set.labels.get("herdr-gpui-1").map(String::as_str),
        Some("open")
    );
    let cleared = core
        .label(&started.address, &[("herdr-gpui-1", "")].into())
        .unwrap();
    assert!(!cleared.labels.contains_key("herdr-gpui-1"));
    core.kill(&started.address).unwrap();
}
