//! The app's [`Session`] of each host: Herdr 1's, read from the daemon's
//! snapshot, and herdr-core's, which the window keeps from its tabs and
//! what the core reports. Readers that move onto it work on both backends
//! (`docs/session-model.md`).

use sheprd_client::protocol::{ClientShellSnapshot, ClientShellWorkspace};
use sheprd_session::{Agent, Keeper, Session, Tab, Workspace, Worktree};
use std::{cell::RefCell, sync::Arc};

/// The session a Herdr 1 snapshot describes.
pub(crate) fn of_snapshot(snapshot: &ClientShellSnapshot) -> Session {
    Session {
        agents: snapshot
            .agents
            .iter()
            .map(|agent| Agent {
                focused: agent.focused,
                identity: agent.agent.clone(),
                name: crate::sidebar::agent_name(agent).to_owned(),
                pane: agent.pane_id.clone(),
                status: agent.agent_status,
                tab: agent.tab_id.clone(),
                workspace: agent.workspace_id.clone(),
            })
            .collect(),
        // The daemon marks its focused workspace both ways; either does.
        focused: snapshot.focused_workspace_id.clone().or_else(|| {
            snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.focused)
                .map(|workspace| workspace.workspace_id.clone())
        }),
        workspaces: snapshot
            .workspaces
            .iter()
            .map(|workspace| of_workspace(snapshot, workspace))
            .collect(),
    }
}

fn of_workspace(snapshot: &ClientShellSnapshot, workspace: &ClientShellWorkspace) -> Workspace {
    Workspace {
        ahead_behind: workspace.git_ahead_behind,
        branch: workspace.branch.clone(),
        cwd: workspace.new_workspace_cwd.clone(),
        custom_label: workspace.custom_label,
        id: workspace.workspace_id.clone(),
        keeper: Keeper::Herdr1,
        label: workspace.label.clone(),
        status: workspace.agent_status,
        tabs: snapshot
            .tabs
            .iter()
            .filter(|tab| tab.workspace_id == workspace.workspace_id)
            .map(|tab| Tab {
                focused: tab.focused,
                id: tab.tab_id.clone(),
                label: tab.label.clone(),
                status: tab.agent_status,
            })
            .collect(),
        tokens: workspace.tokens.clone(),
        worktree: workspace.worktree.as_ref().map(|worktree| Worktree {
            key: worktree.key.clone(),
            label: worktree.label.clone(),
            linked: worktree.is_linked_worktree,
        }),
    }
}

/// A snapshot's session, built once for each snapshot that lands rather
/// than on every read. Keyed by the snapshot itself, so a snapshot swapped
/// in by any path, a test's included, is never read through a stale one.
#[derive(Clone, Debug, Default)]
pub(crate) struct SnapshotSession(RefCell<Option<(Arc<ClientShellSnapshot>, Arc<Session>)>>);

impl SnapshotSession {
    pub(crate) fn get(&self, snapshot: &Arc<ClientShellSnapshot>) -> Arc<Session> {
        let mut cached = self.0.borrow_mut();
        if let Some((built, session)) = cached.as_ref()
            && Arc::ptr_eq(built, snapshot)
        {
            return session.clone();
        }
        let session = Arc::new(of_snapshot(snapshot));
        *cached = Some((snapshot.clone(), session.clone()));
        session
    }
}

/// One host's sessions together: Herdr 1's, and on this computer the one
/// over herdr-core. Readers walk both without merging them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Sessions {
    pub(crate) core: Option<Arc<Session>>,
    /// Whether the core behind `core` answers, which a host with no Herdr 1
    /// installed is online by.
    pub(crate) core_answers: bool,
    pub(crate) herdr_1: Option<Arc<Session>>,
}

impl Sessions {
    /// Herdr 1's session, then herdr-core's.
    pub(crate) fn each(&self) -> impl Iterator<Item = &Session> {
        self.herdr_1.iter().chain(&self.core).map(Arc::as_ref)
    }

    /// The workspace of `session`, kept by `keeper`, that shows as focused
    /// while `kept_front` is the kept workspace in front, if any: Herdr 1's
    /// own focus unless a kept workspace stands in front of it.
    pub(crate) fn front_of<'a>(
        keeper: Keeper,
        session: &'a Session,
        kept_front: Option<&'a str>,
    ) -> Option<&'a str> {
        match keeper {
            Keeper::Herdr1 => kept_front
                .is_none()
                .then_some(session.focused.as_deref())
                .flatten(),
            Keeper::App => kept_front,
        }
    }

    /// Each session with who keeps its workspaces.
    pub(crate) fn keyed(&self) -> impl Iterator<Item = (Keeper, &Session)> {
        let herdr_1 = self
            .herdr_1
            .as_deref()
            .map(|session| (Keeper::Herdr1, session));
        let core = self.core.as_deref().map(|session| (Keeper::App, session));
        herdr_1.into_iter().chain(core)
    }

    pub(crate) fn agents(&self) -> impl Iterator<Item = &Agent> {
        self.each().flat_map(|session| &session.agents)
    }

    pub(crate) fn workspaces(&self) -> impl Iterator<Item = &Workspace> {
        self.each().flat_map(|session| &session.workspaces)
    }

    pub(crate) fn workspace(&self, id: &str) -> Option<&Workspace> {
        self.workspaces().find(|workspace| workspace.id == id)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.herdr_1.is_none()
            && self
                .core
                .as_ref()
                .is_none_or(|core| core.workspaces.is_empty())
    }
}

/// The herdr-core session with every workspace its agents work in: a Herdr
/// 2 tab opened in a Herdr 1 workspace has its agent here and its checkout
/// in Herdr 1's session, which a reader of agents' checkouts needs too.
/// The same `Arc` when nothing is missing, so a watcher of it sees no change.
pub(crate) fn core_with_checkouts(sessions: &Sessions) -> Option<Arc<Session>> {
    let core = sessions.core.as_ref()?;
    let missing: Vec<&Workspace> = sessions
        .herdr_1
        .iter()
        .flat_map(|herdr_1| &herdr_1.workspaces)
        .filter(|workspace| {
            core.workspace(&workspace.id).is_none()
                && core
                    .agents
                    .iter()
                    .any(|agent| agent.workspace == workspace.id)
        })
        .collect();
    if missing.is_empty() {
        return Some(core.clone());
    }
    let mut session = Session::clone(core);
    session.workspaces.extend(missing.into_iter().cloned());
    Some(Arc::new(session))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
