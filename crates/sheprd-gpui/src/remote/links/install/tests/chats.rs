//! `sheprd-acp` goes beside the host's daemon, so the phone's chats work
//! there, without replacing one the user installed.

use super::*;

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn the_chat_program_lands_beside_the_daemon_and_links_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let build = dir.path().join("sheprd-acp");
    std::fs::write(&build, b"#!/bin/sh\necho chat\n").unwrap();
    let (_, sha256) = host_build::digest(CHAT_PROGRAM, &build).unwrap();
    let home = dir.path().join("home");
    // The user's own, which must stay.
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    std::fs::write(home.join(".local/bin/sheprd-acp"), b"mine").unwrap();
    let mut host = Host::new(&sheprd_client::ConnectTarget::Local).unwrap();
    host.env = vec![
        ("HOME".into(), home.to_string_lossy().into_owned()),
        ("PATH".into(), "/usr/bin:/bin".into()),
    ];
    let cancelled = AtomicBool::new(false);
    let limits = ScriptLimits {
        output: 1024,
        idle: Duration::from_secs(10),
    };
    assert!(
        host.capture(&chat_probe_script("9.9.9"), &cancelled)
            .unwrap()
            .is_empty()
    );
    host.stream(
        &chat_copy_script("9.9.9", &sha256),
        std::fs::File::open(&build).unwrap(),
        Vec::new(),
        limits,
        &cancelled,
    )
    .unwrap();
    let placed = home.join(".local/share/herdr-remote-control/bin/9.9.9/sheprd-acp");
    assert_eq!(
        std::fs::read(&placed).unwrap(),
        std::fs::read(&build).unwrap()
    );
    let probe = host
        .capture(&chat_probe_script("9.9.9"), &cancelled)
        .unwrap();
    assert_eq!(String::from_utf8(probe).unwrap().trim(), sha256);
    assert_eq!(
        std::fs::read(home.join(".local/bin/sheprd-acp")).unwrap(),
        b"mine"
    );
    // Bytes that do not match the sum are refused and leave nothing.
    let refused = host.stream(
        &chat_copy_script("9.9.8", &"0".repeat(64)),
        std::fs::File::open(&build).unwrap(),
        Vec::new(),
        limits,
        &cancelled,
    );
    assert!(refused.is_err());
    assert!(
        !home
            .join(".local/share/herdr-remote-control/bin/9.9.8/sheprd-acp")
            .exists()
    );
}

#[test]
fn a_source_build_builds_the_chat_program_without_needing_it() {
    let script = build_script("dev-0123456789ab");
    assert!(script.contains(
        "if CARGO_TARGET_DIR=\"$cache/target\" cargo build --release --locked -p sheprd-acp"
    ));
    assert!(script.contains("chat_part=\"$dir/.sheprd-acp.part.$$\""));
    assert!(!script.contains("$HOME/.local/bin/sheprd-acp"));
}

/// The source build's chat step alone, from its comment to its `fi`.
#[cfg(unix)]
fn chat_step(script: &str) -> &str {
    let start = script
        .find("# The phone's chats run")
        .unwrap_or_else(|| panic!("no chat step in {script}"));
    let end = script[start..]
        .find("\nfi\n")
        .map(|end| start + end + 4)
        .unwrap_or_else(|| panic!("unterminated chat step"));
    &script[start..end]
}

#[cfg(unix)]
#[test]
fn a_chat_program_that_fails_to_copy_leaves_pairing_going_and_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    // A cargo that builds nothing: the copy that follows fails.
    std::fs::write(bin.join("cargo"), "#!/bin/sh\nexit 0\n").unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(bin.join("cargo"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let script = format!(
        "set -eu\ncache={cache}\ndir={out}\n{step}echo carried-on\n",
        cache = dir.path().join("cache").display(),
        out = out.display(),
        step = chat_step(&build_script("dev-0123456789ab")),
    );
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&script)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "carried-on");
    assert_eq!(
        std::fs::read_dir(&out).unwrap().count(),
        0,
        "no part file left"
    );
}
