//! The line for an SSH host; the shared policy is tested in `sheprd-client`.

use super::*;

fn resume(cwd: &str) -> Resume {
    Resume {
        agent: Agent::Claude,
        cwd: cwd.to_owned(),
        flags: Vec::new(),
        session: "0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10".to_owned(),
    }
}

#[test]
fn the_remote_line_finds_the_hosts_program() {
    let line = line(&resume("/my work"), None).unwrap();
    assert!(line.starts_with(" sh -c 'for p in "));
    assert!(line.contains(
        "exec \"$p\" \"--resume\" \"0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10\" \"--cwd\" \"/my work\" \"claude\""
    ));
}

#[test]
fn the_local_line_runs_this_apps_program() {
    let line = line(&resume("/work"), Some(Path::new("/app/sheprd-acp"))).unwrap();
    assert!(line.contains("'/app/sheprd-acp' '--resume'"), "{line}");
}

#[test]
fn a_folder_no_remote_line_can_carry_is_refused() {
    for cwd in ["/$HOME", "/`id`", "/a\"b", "/a\\b", "/it's"] {
        assert!(matches!(
            line(&resume(cwd), None),
            Err(convert::Error::Line)
        ));
    }
}
