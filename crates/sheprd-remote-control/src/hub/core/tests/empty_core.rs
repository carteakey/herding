//! A core with nothing in it, such as one a phone just started.

use super::*;

#[test]
fn an_empty_core_alone_offers_a_herdr_2_workspace_in_the_home_folder() {
    let core = Layout {
        connected: true,
        ..Layout::default()
    };
    let merged = merge(&Layout::default(), &core);
    let Some(home) = sheprd_client::local_path::home_dir() else {
        return;
    };
    assert_eq!(merged.workspaces.len(), 1);
    let workspace = &merged.workspaces[0];
    assert_eq!(workspace.id, UNLISTED);
    assert_eq!(workspace.cwd, home.to_string_lossy());
}

#[test]
fn an_empty_core_beside_herdr_1_or_with_terminals_adds_nothing() {
    let core = Layout {
        connected: true,
        ..Layout::default()
    };
    let daemon = Layout {
        connected: true,
        ..Layout::default()
    };
    assert!(merge(&daemon, &core).workspaces.is_empty());
    assert!(
        merge(&Layout::default(), &Layout::default())
            .workspaces
            .is_empty()
    );
}
