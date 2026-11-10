//! A [`Session`] over herdr-core: the app keeps the workspaces and the tabs
//! in them, and the core reports each terminal's agent.

use super::{Agent, Keeper, Session, Tab, Workspace, Worktree, most_pressing};
use crate::{CoreTab, SavedWorkspace};
use sheprd_backend::{AgentStatus, TerminalId, TerminalInfo};

/// A workspace the app keeps for herdr-core terminals.
#[derive(Clone, Debug)]
pub struct CoreWorkspace<'a> {
    /// The checked-out branch, as Git reports it for `cwd`.
    pub branch: Option<&'a str>,
    pub cwd: &'a str,
    /// Whether the user named it, rather than the app after its folder.
    pub custom_label: bool,
    pub id: &'a str,
    pub label: &'a str,
    /// The checkout `cwd` is, as Git reports it.
    pub worktree: Option<Worktree>,
}

impl<'a> From<&'a SavedWorkspace> for CoreWorkspace<'a> {
    fn from(saved: &'a SavedWorkspace) -> Self {
        Self {
            branch: None,
            cwd: &saved.cwd,
            custom_label: saved.custom,
            id: &saved.id,
            label: &saved.label,
            worktree: None,
        }
    }
}

impl Session {
    /// The session of herdr-core terminals: `workspaces` in order, each with
    /// its tabs among `tabs`, and an agent for every terminal `info` reports
    /// one in. A tab in a workspace not listed, such as a Herdr 1 workspace
    /// a core tab opened in, adds its agents but no workspace.
    pub fn core<'w, 't, 'i>(
        workspaces: impl IntoIterator<Item = CoreWorkspace<'w>>,
        tabs: impl IntoIterator<Item = CoreTab<'t>>,
        focused: Option<&str>,
        info: impl Fn(&TerminalId) -> Option<&'i TerminalInfo>,
    ) -> Self {
        let mut session = Self {
            agents: Vec::new(),
            focused: None,
            workspaces: workspaces
                .into_iter()
                .map(|workspace| Workspace {
                    ahead_behind: None,
                    branch: workspace.branch.map(str::to_owned),
                    cwd: workspace.cwd.to_owned(),
                    custom_label: workspace.custom_label,
                    id: workspace.id.to_owned(),
                    keeper: Keeper::App,
                    label: workspace.label.to_owned(),
                    status: AgentStatus::Unknown,
                    tabs: Vec::new(),
                    tokens: Vec::new(),
                    worktree: workspace.worktree,
                })
                .collect(),
        };
        for tab in tabs {
            let id = tab.id.to_string();
            let mut statuses = Vec::new();
            for terminal in tab.panes.terminals() {
                let Some(info) = info(terminal).filter(|info| info.agent != AgentStatus::Unknown)
                else {
                    continue;
                };
                statuses.push(info.agent);
                session.agents.push(Agent {
                    focused: false,
                    identity: None,
                    name: info
                        .name
                        .clone()
                        .or_else(|| info.title.clone())
                        .unwrap_or_else(|| terminal.to_string()),
                    pane: terminal.to_string(),
                    status: info.agent,
                    tab: id.clone(),
                    workspace: tab.workspace_id.to_owned(),
                });
            }
            if let Some(workspace) = session
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == tab.workspace_id)
            {
                workspace.tabs.push(Tab {
                    focused: false,
                    id,
                    label: tab.title.to_owned(),
                    status: most_pressing(statuses),
                });
            }
        }
        for workspace in &mut session.workspaces {
            workspace.status = most_pressing(workspace.tabs.iter().map(|tab| tab.status));
        }
        session.focused = focused
            .filter(|focused| session.workspace(focused).is_some())
            .or_else(|| {
                session
                    .workspaces
                    .first()
                    .map(|workspace| workspace.id.as_str())
            })
            .map(str::to_owned);
        session
    }
}
