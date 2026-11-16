//! A Herdr 2 workspace's branch and checkout, read from Git as Herdr does.

use super::*;
use crate::hub::Workspace;
use crate::hub::core::checkouts::{Checkouts, read};
use std::path::{Path, PathBuf};

/// A repository `app` on `main`, and a linked worktree of it on `topic`,
/// laid out as `git worktree add` leaves them: no `git` runs, so tests in
/// parallel never wait on each other's.
fn repository() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let main = root.join("app");
    gix::init(&main).unwrap();
    std::fs::write(main.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    let linked = root.join("app-topic");
    let admin = main.join(".git/worktrees/app-topic");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::create_dir(&linked).unwrap();
    std::fs::write(admin.join("HEAD"), "ref: refs/heads/topic\n").unwrap();
    std::fs::write(admin.join("commondir"), "../..\n").unwrap();
    std::fs::write(
        admin.join("gitdir"),
        format!("{}\n", linked.join(".git").display()),
    )
    .unwrap();
    std::fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .unwrap();
    (dir, main, linked)
}

#[test]
fn a_checkout_and_its_linked_worktree_share_their_repository() {
    let (_dir, main, linked) = repository();
    let primary = read(&main).unwrap();
    assert_eq!(primary.branch.as_deref(), Some("main"));
    assert_eq!(primary.worktree.label, "app");
    assert!(!primary.worktree.is_linked_worktree);
    let common = std::fs::canonicalize(main.join(".git")).unwrap();
    assert_eq!(primary.worktree.key, common.display().to_string());
    let worktree = read(&linked.join(".")).unwrap();
    assert_eq!(worktree.branch.as_deref(), Some("topic"));
    assert_eq!(worktree.worktree.key, primary.worktree.key);
    assert_eq!(worktree.worktree.label, "app");
    assert!(worktree.worktree.is_linked_worktree);
}

#[test]
fn a_folder_outside_any_repository_has_no_checkout() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read(dir.path()).is_none());
}

fn workspace(id: &str, cwd: &Path) -> Workspace {
    Workspace {
        branch: None,
        chats: Vec::new(),
        cwd: cwd.to_string_lossy().into_owned(),
        id: id.to_owned(),
        label: "app".to_owned(),
        session: None,
        terminals: Vec::new(),
        worktree: None,
    }
}

#[test]
fn herdr_2_workspaces_get_their_checkout_but_the_unlisted_group_does_not() {
    let (_dir, main, linked) = repository();
    let mut layout = Layout {
        workspaces: vec![
            workspace("core:space:aa", &linked),
            workspace(UNLISTED, &main),
        ],
        ..Layout::default()
    };
    let now = Instant::now();
    let mut checkouts = Checkouts::default();
    checkouts.fill(&mut layout, now);
    let space = &layout.workspaces[0];
    assert_eq!(space.branch.as_deref(), Some("topic"));
    assert!(space.worktree.as_ref().unwrap().is_linked_worktree);
    assert!(layout.workspaces[1].worktree.is_none());
    // A branch switched shows once the cached reading is stale.
    let admin = main.join(".git/worktrees/app-topic/HEAD");
    std::fs::write(admin, "ref: refs/heads/next\n").unwrap();
    let mut again = Layout {
        workspaces: vec![workspace("core:space:aa", &linked)],
        ..Layout::default()
    };
    checkouts.fill(&mut again, now);
    assert_eq!(again.workspaces[0].branch.as_deref(), Some("topic"));
    let mut later = Layout {
        workspaces: vec![workspace("core:space:aa", &linked)],
        ..Layout::default()
    };
    checkouts.fill(&mut later, now + Duration::from_secs(10));
    assert_eq!(later.workspaces[0].branch.as_deref(), Some("next"));
}

#[test]
fn a_full_cache_keeps_its_fresh_readings() {
    let dir = tempfile::tempdir().unwrap();
    let now = Instant::now();
    let mut checkouts = Checkouts::default();
    let folder = |n: usize| dir.path().join(n.to_string());
    let workspaces = (0..=crate::hub::core::checkouts::MAX_FOLDERS)
        .map(|n| workspace(&format!("core:space:{n}"), &folder(n)))
        .collect();
    let mut layout = Layout {
        workspaces,
        ..Layout::default()
    };
    checkouts.fill(&mut layout, now);
    let cached = &checkouts.cached;
    assert_eq!(cached.len(), crate::hub::core::checkouts::MAX_FOLDERS);
    assert!(cached.contains_key(&folder(0).to_string_lossy().into_owned()));
    // Once stale, room is made again.
    let later = now + Duration::from_secs(10);
    let mut one = Layout {
        workspaces: vec![workspace("core:space:x", &dir.path().join("x"))],
        ..Layout::default()
    };
    checkouts.fill(&mut one, later);
    assert_eq!(checkouts.cached.len(), 1);
}
