use super::*;
use std::{process::Command, time::Duration};

fn run(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn probe(dir: &Path) -> Option<Identity> {
    let deadline = Instant::now() + Duration::from_secs(20);
    identity(dir.to_str().unwrap(), deadline, &|| false).unwrap()
}

#[test]
fn a_main_checkout_and_its_linked_worktree_share_one_key() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("app");
    std::fs::create_dir(&repo).unwrap();
    run(&repo, &["init", "-q"]);
    // A branch with no commit yet still has a name.
    let unborn = probe(&repo).unwrap();
    assert_eq!(unborn.branch.as_deref(), Some("main"));
    run(&repo, &["commit", "-q", "--allow-empty", "-m", "one"]);
    let linked_dir = root.path().join("app-feature");
    run(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            linked_dir.to_str().unwrap(),
        ],
    );
    let main = probe(&repo).unwrap();
    let linked = probe(&linked_dir).unwrap();
    assert_eq!(main.key, linked.key);
    assert_eq!(main.label, "app");
    assert_eq!((main.linked, linked.linked), (false, true));
    assert_eq!(linked.branch.as_deref(), Some("feature"));
    assert!(Path::new(&main.key).is_absolute());
    run(&linked_dir, &["checkout", "-q", "--detach"]);
    assert_eq!(probe(&linked_dir).unwrap().branch, None, "detached");
}

#[test]
fn a_plain_folder_is_no_checkout() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(probe(root.path()), None);
}

#[test]
fn a_bare_repository_is_named_without_its_suffix() {
    assert_eq!(repository_name(Path::new("/src/app/.git")), "app");
    assert_eq!(repository_name(Path::new("/src/app.git")), "app");
    assert_eq!(repository_name(Path::new("/")), "");
}
