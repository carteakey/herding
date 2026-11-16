//! The editor a herdr-core pane runs: the program's words, not a typed line.

use super::*;
use std::os::unix::fs::PermissionsExt;

/// Runs `argv` as herdr-core would, with a fake `editor` as `$EDITOR`, and
/// returns the arguments that editor got.
fn ran(argv: &[String]) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("args");
    let program = dir.path().join("vim");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\n",
            out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let status = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .env_remove("VISUAL")
        .env("EDITOR", &program)
        .status()
        .unwrap();
    assert!(status.success(), "{argv:?}");
    std::fs::read_to_string(out)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_core_pane_runs_the_editor_at_the_line_with_no_shell_line_typed() {
    let file = "/w/it is $(here)`x`;y.rs";
    let argv = argv(&target(file, Some(12)), None).unwrap();
    assert_eq!(&argv[..2], ["/bin/sh", "-c"]);
    assert_eq!(&argv[3..], [file, "12"]);
    assert_eq!(ran(&argv), ["+12", file]);
}

#[test]
fn a_core_pane_refuses_what_a_herdr_pane_refuses() {
    assert!(argv(&target("relative.rs", None), None).is_err());
    assert!(argv(&target("/w/it's.rs", None), None).is_err());
}

#[test]
fn the_typed_line_still_quotes_every_word_but_the_line() {
    let socket = Path::new("/tmp/nvim/s.sock");
    let line = command_line(&target("/w/a.rs", Some(3)), None, Some(socket)).unwrap();
    assert!(line.ends_with(" '/w/a.rs' 3 '/tmp/nvim/s.sock'"), "{line}");
    let line = command_line(&target("/w/a.rs", None), None, None).unwrap();
    assert!(line.ends_with("' '/w/a.rs' 1"), "{line}");
}
