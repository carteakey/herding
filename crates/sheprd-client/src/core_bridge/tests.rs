use super::*;
use std::{io::Read, path::PathBuf};

/// A private folder standing in for the host's home, its name quoted so the
/// script must keep paths in variables.
fn home(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "sheprd-core-bridge-{}-{name} ' x",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".local/bin")).unwrap();
    root
}

/// The bridge command run by `/bin/sh` here rather than by `ssh` there.
fn local(home: &std::path::Path) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .env_clear()
        .env("HOME", home)
        .env("PATH", home.join("empty"));
    command
}

#[test]
fn the_bridge_runs_the_installed_core_as_connect_after_the_banner() {
    let root = home("connect");
    crate::test_executable::write(
        root.join(".local/bin/herdr-core"),
        "#!/bin/sh\n[ \"$1\" = connect ] && [ $# -eq 1 ] || exit 3\nexec /bin/cat\n",
        0o700,
    )
    .unwrap();
    let mut bridge = command(local(&root), &AtomicBool::new(false)).unwrap();
    bridge.write_all(b"hello\n").unwrap();
    bridge.flush().unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    let mut echoed = Vec::new();
    while !echoed.ends_with(b"\n") && Instant::now() < deadline {
        let mut byte = [0];
        match bridge.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => echoed.push(byte[0]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {}
            Err(error) => panic!("{error}"),
        }
    }
    assert_eq!(echoed, b"hello\n");
    drop(bridge);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_host_without_a_core_is_missing_rather_than_unreachable() {
    let root = home("missing");
    let Err(error) = command(local(&root), &AtomicBool::new(false)) else {
        panic!("a host without herdr-core bridged");
    };
    assert!(
        matches!(error, crate::Error::CoreBridge(Error::Missing)),
        "{error:?}"
    );
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_core_that_cannot_run_is_skipped_for_the_next_location() {
    let root = home("skip");
    // Not executable, so the search moves on and finds none.
    std::fs::write(root.join(".local/bin/herdr-core"), "#!/bin/sh\n").unwrap();
    assert!(matches!(
        command(local(&root), &AtomicBool::new(false)),
        Err(crate::Error::CoreBridge(Error::Missing))
    ));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_windows_bridge_starts_the_core_as_connect_after_the_banner() {
    let encoded = powershell_command();
    let base64 = encoded.rsplit(' ').next().unwrap();
    let utf16 = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64).unwrap();
    let units: Vec<u16> = utf16
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    let script = String::from_utf16(&units).unwrap();
    assert!(script.contains("herdr-core.exe"));
    assert!(script.contains("-ArgumentList connect"));
    let ready = script.find("herdr-remote-output-ready:1").unwrap();
    assert!(ready < script.find("Start-Process").unwrap());
    assert!(script.contains("exit 127"));
}

#[test]
fn scripts_find_the_core_the_bridge_runs_even_with_unset_variables() {
    let root = home("locate");
    let core = root.join(".local/bin/herdr-core");
    crate::test_executable::write(&core, "#!/bin/sh\n", 0o700).unwrap();
    let script = format!("set +u\n{}\nprintf '%s' \"$herdr_core\"", locate_script());
    let found = local(&root).arg(&script).output().unwrap();
    assert!(found.status.success(), "{found:?}");
    assert_eq!(found.stdout, core.to_string_lossy().as_bytes());
    std::fs::remove_file(&core).unwrap();
    let missing = local(&root).arg(&script).output().unwrap();
    assert!(missing.stdout.is_empty(), "{missing:?}");
    std::fs::remove_dir_all(root).unwrap();
}
