//! A phone reaching a machine whose Herdr 2 core holds nothing yet, as
//! one started after a reboot: the core offers its home folder.

use super::*;
use crate::hub::Workspace;

/// What the daemon lists for an empty core (`hub::core::merge`).
fn with_empty_core(dir: &std::path::Path, home: &str) -> Shared {
    let layout = Layout {
        connected: true,
        boot: "core:b".into(),
        working: 0,
        workspaces: vec![Workspace {
            branch: None,
            chats: Vec::new(),
            cwd: home.to_owned(),
            id: "core:terminals".into(),
            label: "Herdr 2".into(),
            session: None,
            terminals: Vec::new(),
            worktree: None,
        }],
    };
    let state = Arc::try_unwrap(state(dir)).ok().unwrap();
    Arc::new(State {
        hub: Hub::offline(layout),
        ..state
    })
}

#[tokio::test]
async fn a_new_workspace_may_start_in_the_home_folder_of_an_empty_core() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_string_lossy().into_owned();
    let state = with_empty_core(dir.path(), &home);
    let checked = check_new_workspace(&state, &home, "", "claude").await;
    assert!(checked.is_ok(), "{:?}", checked.err());
    assert!(matches!(
        check_new_workspace(&state, "/elsewhere", "", "claude").await,
        Err(ApiError::Folder)
    ));
}
