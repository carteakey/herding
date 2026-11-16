//! A session as the app reads it, whichever backend keeps it: workspaces,
//! their tabs, and the agents working in them. Herdr 1 reports all of it;
//! for herdr-core, which keeps terminals only, the app keeps the workspaces
//! and tabs and the core reports each terminal's agent.
//!
//! It is a model, not a mirror of either wire format: it holds only what
//! features read, so a reader moved onto it works on both backends.

mod core;

pub use core::CoreWorkspace;

use sheprd_backend::AgentStatus;

/// Who keeps a workspace, and so who carries out what is asked of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Keeper {
    /// The Herdr 1 daemon, which owns its workspaces, tabs, and panes.
    #[default]
    Herdr1,
    /// The app, for terminals herdr-core runs.
    App,
}

/// One session: what one daemon, or the app over herdr-core, holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    pub agents: Vec<Agent>,
    /// The workspace in front, by [`Workspace::id`].
    pub focused: Option<String>,
    /// In the order the user arranged them.
    pub workspaces: Vec<Workspace>,
}

/// A workspace: a folder and the tabs opened in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    /// Commits the branch is ahead of and behind its upstream.
    pub ahead_behind: Option<(usize, usize)>,
    pub branch: Option<String>,
    /// Where a new terminal in it starts.
    pub cwd: String,
    /// Whether the user named it, rather than its folder or branch.
    pub custom_label: bool,
    /// Unique within its session.
    pub id: String,
    pub keeper: Keeper,
    pub label: String,
    /// The most pressing state of its agents.
    pub status: AgentStatus,
    /// In strip order.
    pub tabs: Vec<Tab>,
    /// Named values the sidebar's row layouts show, such as `$branch`.
    pub tokens: Vec<(String, String)>,
    pub worktree: Option<Worktree>,
}

/// The Git checkout a workspace is in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Worktree {
    /// Shared by every checkout of one repository.
    pub key: String,
    pub label: String,
    /// A linked worktree rather than the repository's main checkout.
    pub linked: bool,
}

/// A tab of a workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub focused: bool,
    pub id: String,
    pub label: String,
    pub status: AgentStatus,
}

/// An agent working in a pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agent {
    pub focused: bool,
    /// Which agent it is, such as `claude`, for its icon.
    pub identity: Option<String>,
    /// What to call it.
    pub name: String,
    /// The pane, or herdr-core terminal, it runs in.
    pub pane: String,
    pub status: AgentStatus,
    pub tab: String,
    pub workspace: String,
}

impl Session {
    pub fn workspace(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    pub fn focused_workspace(&self) -> Option<&Workspace> {
        self.workspace(self.focused.as_deref()?)
    }

    /// The agents working in one workspace.
    pub fn agents_in<'a>(&'a self, workspace: &'a str) -> impl Iterator<Item = &'a Agent> + 'a {
        self.agents
            .iter()
            .filter(move |agent| agent.workspace == workspace)
    }

    /// Adds `other`'s workspaces and agents after this session's own. The
    /// focus stays with this session unless it has none.
    pub fn extend(&mut self, other: Self) {
        self.workspaces.extend(other.workspaces);
        self.agents.extend(other.agents);
        if self.focused.is_none() {
            self.focused = other.focused;
        }
    }
}

/// How pressing a state is, so a workspace or tab shows its most pressing
/// agent: blocked first, then working, done, and idle.
pub fn urgency(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Blocked => 4,
        AgentStatus::Working => 3,
        AgentStatus::Done => 2,
        AgentStatus::Idle => 1,
        AgentStatus::Unknown => 0,
    }
}

/// The most pressing of `statuses`, or `Unknown` with none.
pub fn most_pressing(statuses: impl IntoIterator<Item = AgentStatus>) -> AgentStatus {
    statuses
        .into_iter()
        .max_by_key(|status| urgency(*status))
        .unwrap_or(AgentStatus::Unknown)
}
