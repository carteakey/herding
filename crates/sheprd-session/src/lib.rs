#![doc = include_str!("../README.md")]

mod error;
pub mod panes;
pub mod saved;
pub mod session;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub use error::{Error, Result};
pub use saved::{
    CoreTab, Labelled, MAX_SAVED_BYTES, MAX_SAVED_TABS, NEW_WORKSPACE_LABEL, SESSION_LABEL,
    SavedTabs, SavedWorkspace, WORKSPACE_LABEL, labelled_workspace, new_workspace, tab_workspace,
};
pub use session::{Agent, CoreWorkspace, Keeper, Session, Tab, Workspace, Worktree};
