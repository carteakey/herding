//! The repository checkout each Herdr 2 workspace's folder is in. Herdr 1
//! reports a workspace's branch and worktree; herdr-core knows neither, so
//! the worker reads them from Git, as Herdr does: the repository is keyed by
//! its common Git folder, which a repository's linked worktrees share, and
//! named after it. A phone then lists Herdr 2 workspaces under their
//! repository beside Herdr 1's.

use super::{UNLISTED, spaces::repository_name};
use crate::hub::Layout;
use sheprd_client::protocol::ClientShellWorktree;
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

/// How long a folder's checkout is trusted: a branch switched in a terminal
/// shows within this.
const FRESH: Duration = Duration::from_secs(5);
/// Folders remembered at once.
pub(super) const MAX_FOLDERS: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Checkout {
    pub(super) branch: Option<String>,
    pub(super) worktree: ClientShellWorktree,
}

#[derive(Default)]
pub(super) struct Checkouts {
    pub(super) cached: HashMap<String, (Instant, Option<Checkout>)>,
}

impl Checkouts {
    /// Gives each Herdr 2 workspace that has none the branch and checkout
    /// of its folder. The group of terminals no tab holds has no one folder.
    pub(super) fn fill(&mut self, layout: &mut Layout, now: Instant) {
        for workspace in &mut layout.workspaces {
            if workspace.id == UNLISTED || workspace.cwd.is_empty() || workspace.worktree.is_some()
            {
                continue;
            }
            let Some(checkout) = self.checkout(&workspace.cwd, now) else {
                continue;
            };
            workspace.worktree = Some(checkout.worktree);
            if workspace.branch.is_none() {
                workspace.branch = checkout.branch;
            }
        }
    }

    fn checkout(&mut self, folder: &str, now: Instant) -> Option<Checkout> {
        if let Some((at, checkout)) = self.cached.get(folder)
            && now.saturating_duration_since(*at) < FRESH
        {
            return checkout.clone();
        }
        let checkout = read(Path::new(folder));
        if self.cached.len() >= MAX_FOLDERS {
            self.cached
                .retain(|_, (at, _)| now.saturating_duration_since(*at) < FRESH);
        }
        // Past the bound, a folder is read each time rather than pushing
        // out readings still fresh.
        if self.cached.len() < MAX_FOLDERS || self.cached.contains_key(folder) {
            self.cached
                .insert(folder.to_owned(), (now, checkout.clone()));
        }
        checkout
    }
}

/// The checkout `folder` is in, if any.
pub(super) fn read(folder: &Path) -> Option<Checkout> {
    let repo = gix::discover(folder).ok()?;
    // A linked worktree names the shared folder through its own
    // (`.git/worktrees/NAME/../..`): only the resolved path names it.
    let common = std::fs::canonicalize(repo.common_dir()).ok()?;
    let label = repository_name(&common)?;
    // A linked worktree has a Git folder of its own beside the shared one.
    let is_linked_worktree = std::fs::canonicalize(repo.git_dir()).ok()? != common;
    let branch = repo
        .head_name()
        .ok()
        .flatten()
        .map(|name| name.shorten().to_string());
    Some(Checkout {
        branch,
        worktree: ClientShellWorktree {
            key: common.display().to_string(),
            label,
            is_linked_worktree,
        },
    })
}
