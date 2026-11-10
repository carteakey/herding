use super::super::context::{Herdr, api_socket, apply};
use sheprd_backend::Spawn;
use std::path::{Path, PathBuf};

fn env<'a>(spawn: &'a Spawn, name: &str) -> Option<&'a str> {
    spawn
        .env
        .iter()
        .find(|(known, _)| known == name)
        .map(|(_, value)| value.as_str())
}

#[test]
fn a_terminal_in_a_herdr_workspace_names_that_workspace_and_no_pane() {
    let mut spawn = Spawn::default();
    let herdr = Herdr {
        api_socket: PathBuf::from("/run/herdr/herdr.sock"),
        workspace: "wFH".into(),
    };
    apply(&mut spawn, Some(PathBuf::from("/src/app")), Some(herdr));
    assert_eq!(spawn.cwd.as_deref(), Some(Path::new("/src/app")));
    assert_eq!(env(&spawn, "HERDR_WORKSPACE_ID"), Some("wFH"));
    assert_eq!(
        env(&spawn, "HERDR_SOCKET_PATH"),
        Some("/run/herdr/herdr.sock")
    );
    // A core started from a Herdr pane passes that pane's ids on; they are
    // cleared so nothing reports to a pane the terminal is not in.
    assert_eq!(env(&spawn, "HERDR_PANE_ID"), Some(""));
    assert_eq!(env(&spawn, "HERDR_TAB_ID"), Some(""));
}

#[test]
fn a_terminal_outside_herdr_clears_its_workspace_and_keeps_the_socket() {
    let mut spawn = Spawn::default();
    apply(&mut spawn, None, None);
    assert_eq!(spawn.cwd, None);
    assert_eq!(env(&spawn, "HERDR_WORKSPACE_ID"), Some(""));
    assert_eq!(env(&spawn, "HERDR_PANE_ID"), Some(""));
    assert_eq!(env(&spawn, "HERDR_SOCKET_PATH"), None);
}

#[test]
fn what_the_spawn_names_itself_wins() {
    let mut spawn = Spawn {
        cwd: Some(PathBuf::from("/chosen")),
        env: vec![("HERDR_PANE_ID".into(), "kept".into())],
        ..Spawn::default()
    };
    apply(&mut spawn, Some(PathBuf::from("/workspace")), None);
    assert_eq!(spawn.cwd.as_deref(), Some(Path::new("/chosen")));
    let panes = spawn
        .env
        .iter()
        .filter(|(name, _)| name == "HERDR_PANE_ID")
        .count();
    assert_eq!(panes, 1);
    assert_eq!(env(&spawn, "HERDR_PANE_ID"), Some("kept"));
}

#[test]
fn the_api_socket_is_the_client_socket_without_its_suffix() {
    assert_eq!(
        api_socket(Path::new("/home/u/.config/herdr/herdr-client.sock")),
        Some(PathBuf::from("/home/u/.config/herdr/herdr.sock"))
    );
    assert_eq!(
        api_socket(Path::new("/tmp/s/sessions/work/a-client.sock")),
        Some(PathBuf::from("/tmp/s/sessions/work/a.sock"))
    );
    assert_eq!(api_socket(Path::new("/tmp/custom.sock")), None);
    assert_eq!(api_socket(Path::new("/tmp/-client.sock")), None);
}

mod window;
