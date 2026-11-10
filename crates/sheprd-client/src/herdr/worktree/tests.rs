use super::*;
use tempfile::tempdir;

fn never() -> bool {
    false
}

fn run_git(repository: &Path, args: &[&str]) {
    let status = new_command("git")
        .current_dir(repository)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn committed_repository(parent: &Path) -> PathBuf {
    let repository = parent.join("project");
    std::fs::create_dir(&repository).unwrap();
    run_git(&repository, &["init", "--quiet"]);
    run_git(
        &repository,
        &["commit", "--quiet", "--allow-empty", "-m", "start"],
    );
    repository
}

#[test]
fn a_new_branch_is_checked_out_where_herdr_would_put_it() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let root = dir.path().join("worktrees");
    let checkout = add(
        &repository,
        "worktree/Quiet_forest-a09d",
        "HEAD",
        &root,
        never,
    )
    .unwrap();
    assert_eq!(
        checkout,
        root.join("project").join("worktree-quiet-forest-a09d")
    );
    let head = git(
        &never,
        &checkout,
        [OsStr::new("branch"), OsStr::new("--show-current")],
    )
    .unwrap();
    assert_eq!(head.trim(), "worktree/Quiet_forest-a09d");
}

#[test]
fn a_worktree_of_a_worktree_files_under_the_same_repository() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let root = dir.path().join("worktrees");
    let first = add(&repository, "one", "HEAD", &root, never).unwrap();
    assert_eq!(
        add(&first, "two", "HEAD", &root, never).unwrap(),
        root.join("project").join("two")
    );
}

#[test]
fn removing_a_new_worktree_frees_its_branch() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let root = dir.path().join("worktrees");
    let checkout = add(&repository, "topic", "HEAD", &root, never).unwrap();
    std::fs::write(checkout.join("from-a-hook"), "").unwrap();
    remove(&repository, &checkout, "topic").unwrap();
    assert!(!checkout.exists());
    add(&repository, "topic", "HEAD", &root, never).unwrap();
}

#[test]
fn a_taken_branch_is_refused_with_what_git_said() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let root = dir.path().join("worktrees");
    add(&repository, "topic", "HEAD", &root, never).unwrap();
    let error = add(
        &repository,
        "topic",
        "HEAD",
        &dir.path().join("other"),
        never,
    )
    .unwrap_err();
    let Error::Refused(complaint) = error else {
        panic!("{error}");
    };
    assert!(complaint.contains("topic"), "{complaint}");
}

#[test]
fn a_folder_outside_any_repository_is_refused() {
    let dir = tempdir().unwrap();
    let error = add(
        dir.path(),
        "topic",
        "HEAD",
        &dir.path().join("worktrees"),
        never,
    )
    .unwrap_err();
    assert!(matches!(error, Error::Refused(_)), "{error}");
}

#[test]
fn a_bare_repository_is_named_without_its_suffix() {
    assert_eq!(
        repository_name(Path::new("/code/project.git")),
        Some(OsStr::new("project"))
    );
    assert_eq!(
        repository_name(Path::new("/code/project/.git")),
        Some(OsStr::new("project"))
    );
}

#[test]
fn a_branch_with_nothing_to_keep_gets_the_fallback_folder() {
    assert_eq!(path_slug("--/--"), FALLBACK_SLUG);
    assert_eq!(path_slug("--Fix_42--"), "fix-42");
}

#[test]
fn a_new_branch_starts_from_the_base_asked_for() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let first = git(
        &never,
        &repository,
        [OsStr::new("rev-parse"), OsStr::new("HEAD")],
    )
    .unwrap();
    run_git(
        &repository,
        &["commit", "--quiet", "--allow-empty", "-m", "later"],
    );
    let checkout = add(
        &repository,
        "topic",
        first.trim(),
        &dir.path().join("worktrees"),
        never,
    )
    .unwrap();
    let head = git(
        &never,
        &checkout,
        [OsStr::new("rev-parse"), OsStr::new("HEAD")],
    )
    .unwrap();
    assert_eq!(head.trim(), first.trim());
}

#[test]
fn a_base_that_looks_like_an_option_is_refused_as_a_ref() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let error = add(
        &repository,
        "topic",
        "--detach",
        &dir.path().join("worktrees"),
        never,
    )
    .unwrap_err();
    assert!(matches!(error, Error::Refused(_)), "{error}");
}

#[test]
fn giving_up_kills_git_and_says_so() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let error = add(
        &repository,
        "topic",
        "HEAD",
        &dir.path().join("worktrees"),
        || true,
    )
    .unwrap_err();
    assert!(matches!(error, Error::GaveUp), "{error}");
}

#[test]
fn output_past_the_bound_is_read_to_its_end_and_dropped() {
    let long = vec![b'x'; usize::try_from(MAX_OUTPUT).unwrap() * 3];
    let mut reader = std::io::Cursor::new(long);
    let kept = bounded(&mut reader);
    assert_eq!(kept.len(), usize::try_from(MAX_OUTPUT).unwrap());
    assert_eq!(reader.position(), MAX_OUTPUT * 3, "read to the end");
}

#[test]
fn a_hook_that_prints_a_lot_still_gets_its_worktree() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let hook = repository.join(".git/hooks/post-checkout");
    std::fs::write(
        &hook,
        "#!/bin/sh
head -c 300000 /dev/zero | tr '\\0' x
head -c 300000 /dev/zero | tr '\\0' y >&2
",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let checkout = add(
        &repository,
        "topic",
        "HEAD",
        &dir.path().join("worktrees"),
        never,
    )
    .unwrap();
    assert!(checkout.join(".git").exists());
}
