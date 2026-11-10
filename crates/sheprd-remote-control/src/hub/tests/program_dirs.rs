//! Where the daemon looks for `sheprd-acp`: beside itself first, then where
//! Herding looks on an SSH host, then `PATH`.

use std::path::Path;

#[test]
fn a_chat_program_is_found_beside_the_daemon_then_where_sheprd_looks() {
    // An absolute root on every platform: `/home` has no drive on Windows.
    let root = std::env::temp_dir();
    let home = root.join("home/me");
    let bin = root.join("usr/bin");
    let path = std::env::join_paths([bin.as_path(), Path::new("relative/bin")]).unwrap();
    let beside = home.join(".local/share/herdr-remote-control/bin/1.0");
    let dirs = crate::hub::program_dirs(
        Some(&beside.join("herdr-remote-control")),
        Some(&home),
        Some(&path),
    );
    assert_eq!(
        dirs,
        [
            beside,
            home.join(".herdr/bin"),
            home.join(".cargo/bin"),
            home.join(".local/bin"),
            bin,
        ]
    );
    // A relative home names no folder.
    let dirs = crate::hub::program_dirs(None, Some(Path::new("me")), None);
    assert!(dirs.is_empty());
}

#[test]
#[cfg(unix)]
fn a_file_without_the_execute_bit_is_passed_over() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("sheprd-acp");
    std::fs::write(&file, b"#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!crate::hub::runnable(&file));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(crate::hub::runnable(&file));
    assert!(
        !crate::hub::runnable(dir.path()),
        "a folder is not a program"
    );
}
