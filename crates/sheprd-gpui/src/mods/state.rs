//! The state mods read, built from what the window already holds: each
//! listed host's sessions, Herdr 1's and on this computer Herdr 2's. Usage and the pull request are not offered yet,
//! so those topics read as empty.

use crate::{SheprdWindow, state::ConnectionStatus};
use sheprd_client::protocol::AgentStatus as WireStatus;
use sheprd_mods::{Agent, AgentStatus, Focus, Host, State, Tab, Workspace};

impl SheprdWindow {
    pub(crate) fn mods_state(&self) -> State {
        let mut state = State::default();
        // A kept workspace's tabs are the app's: the one shown is the
        // active group's.
        let kept_tab = self
            .kept_in_front()
            .and_then(|_| self.group_pick(self.active_group()?))
            .and_then(|pick| match pick {
                crate::browser::Pick::Page(tab) => Some(tab.number().to_string()),
                _ => None,
            });
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            if !endpoint.enabled {
                continue;
            }
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            state.hosts.push(Host {
                id: endpoint.id.clone(),
                name: endpoint.label.clone(),
                connected: matches!(live.status, ConnectionStatus::Connected),
            });
            // Herdr 1's workspaces, then those the app keeps for Herdr 2.
            let sessions = self.sessions(index);
            let selected = index == self.selected_endpoint;
            // The selected host's front is the window's; another host's is
            // the workspace its Herdr 1 reports focused.
            let front = if selected {
                self.browser_key().map(|(_, workspace)| workspace)
            } else {
                sessions
                    .herdr_1
                    .as_ref()
                    .and_then(|session| session.focused.clone())
            };
            for workspace in sessions.workspaces() {
                let in_front = front.as_deref() == Some(workspace.id.as_str());
                let shown = in_front.then_some(kept_tab.as_deref()).flatten();
                state.workspaces.push(Workspace {
                    id: workspace.id.clone(),
                    host: endpoint.id.clone(),
                    name: workspace.label.clone(),
                    focused: in_front,
                    tabs: workspace
                        .tabs
                        .iter()
                        .map(|tab| Tab {
                            id: tab.id.clone(),
                            name: tab.label.clone(),
                            focused: tab.focused || shown == Some(tab.id.as_str()),
                        })
                        .collect(),
                });
            }
            state.agents.extend(sessions.agents().map(|agent| Agent {
                id: agent.pane.clone(),
                host: endpoint.id.clone(),
                workspace: agent.workspace.clone(),
                name: agent.name.clone(),
                status: status(agent.status),
            }));
            if selected {
                let workspace = front.as_deref().and_then(|id| sessions.workspace(id));
                // A kept workspace's tabs are the app's; Herdr 1 names its own.
                let tab = match self.kept_in_front() {
                    Some(_) => kept_tab.clone(),
                    None => live
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.focused_tab_id.clone()),
                };
                state.focus = Some(Focus {
                    host: endpoint.id.clone(),
                    workspace: front.clone(),
                    tab,
                    name: workspace.map(|workspace| workspace.label.clone()),
                    path: workspace
                        .map(|workspace| workspace.cwd.clone())
                        .filter(|path| !path.is_empty()),
                });
            }
        }
        state
    }
}

fn status(status: WireStatus) -> AgentStatus {
    match status {
        WireStatus::Working => AgentStatus::Working,
        WireStatus::Blocked => AgentStatus::Blocked,
        WireStatus::Done => AgentStatus::Done,
        WireStatus::Idle | WireStatus::Unknown => AgentStatus::Idle,
    }
}
