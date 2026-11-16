//! Which Git checkout a folder is, as Herdr 1 reports it for each of its
//! workspaces: the repository every checkout of it shares, whether this one
//! is a linked worktree, and its branch. herdr-core reports none of this, so
//! the app asks Git for the workspaces it keeps for Herdr 2.

use super::git;
use std::{path::Path, time::Instant};

/// A folder's checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// The checked-out branch; `None` when `HEAD` is detached.
    pub branch: Option<String>,
    /// The repository's common Git directory, absolute: every checkout of
    /// one repository shares it, as Herdr 1's worktree key.
    pub key: String,
    /// The repository's name, from that directory.
    pub label: String,
    /// A linked worktree rather than the repository's main checkout.
    pub linked: bool,
}

/// `dir`'s checkout, or `None` when it is no Git checkout. Read-only: two
/// `git` children that never touch the index or a remote.
pub fn identity(
    dir: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Option<Identity>> {
    let dirs = match git(
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
            "--git-dir",
        ],
        "rev-parse",
        deadline,
        cancelled,
    ) {
        Ok(dirs) => dirs,
        // Not a checkout, or one Git cannot read: no identity to show.
        Err(crate::Error::Git(super::Error::Failed { .. })) => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut lines = dirs.lines();
    let (Some(common), Some(own)) = (lines.next(), lines.next()) else {
        return Ok(None);
    };
    // Unlike `rev-parse --abbrev-ref HEAD`, this answers on a branch with
    // no commit yet; it fails, quietly, only when HEAD is detached.
    let branch = git(
        dir,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        "symbolic-ref",
        deadline,
        cancelled,
    )
    .ok()
    .filter(|branch| !branch.is_empty());
    Ok(Some(Identity {
        branch,
        key: common.to_owned(),
        label: repository_name(Path::new(common)),
        linked: Path::new(own) != Path::new(common),
    }))
}

/// The repository's folder name from its common Git directory: the folder
/// holding `.git`, or a bare repository's name without `.git`.
fn repository_name(common: &Path) -> String {
    let name = match common.file_name() {
        Some(name) if name == ".git" => common.parent().and_then(Path::file_name),
        Some(_) => common.file_stem(),
        None => None,
    };
    name.map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
