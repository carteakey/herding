//! Git warns about line endings on the stream its listings are read from,
//! as it does by default on Windows. The listing must not take the warning
//! for its first file.
use super::*;
use std::{path::Path, process::Command};

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["-c", "user.name=Test"])
        .args(["-c", "user.email=test@example.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .env_remove("GIT_DIR")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

#[test]
fn a_line_ending_warning_leaves_the_first_files_status_and_counts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().canonicalize().unwrap();
    git(&path, &["init", "-q", "-b", "main"]);
    git(&path, &["config", "core.autocrlf", "true"]);
    std::fs::write(path.join("gone.rs"), "bye\n").unwrap();
    std::fs::write(path.join("lib.rs"), "one\ntwo\n").unwrap();
    git(&path, &["add", "-A"]);
    git(&path, &["commit", "-qm", "base"]);
    std::fs::remove_file(path.join("gone.rs")).unwrap();
    std::fs::write(path.join("lib.rs"), "one\nchanged\n").unwrap();

    let checkout = path.to_str().unwrap().to_owned();
    let input = Input {
        checkout: Some(checkout),
        repo_key: Some(path.join(".git").to_str().unwrap().to_owned()),
        branch: "main".into(),
    };
    let loaded = load(&input, Scope::Uncommitted, None, None, false).unwrap();
    let files = &loaded.diff.files;
    assert_eq!(files[0].path, "gone.rs");
    assert_eq!(files[0].status, Status::Deleted);
    assert_eq!((files[0].added, files[0].removed), (Some(0), Some(1)));
    assert_eq!(files[1].status, Status::Modified);
}
