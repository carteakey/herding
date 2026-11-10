//! Share the desktop's prepared PR cache, never credentials or a second lookup,
//! and keep what answers phones' questions about each workspace beside it.
use super::*;
use crate::{
    SheprdWindow,
    pull_request::Origin,
    remote::changes::{Source, Sources},
};
use gpui::Context;
use sheprd_client::protocol::{ClientShellWorkspace, ClientShellWorktree};
use sheprd_remote_control::control::pull_requests::{
    MAX_PULL_REQUESTS, UnopenedBranch, WorkspacePullRequest,
};

impl SheprdWindow {
    pub(crate) fn publish_remote_pull_requests(&self, cx: &mut Context<Self>) {
        let Some(supervisor) = cx.try_global::<Supervisor>() else {
            return;
        };
        if !supervisor.config.enabled
            || supervisor
                .pull_requests_checked
                .is_some_and(|checked| checked.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        let Some(endpoint) = self.endpoints.first() else {
            return;
        };
        let live = if self.selected_endpoint == 0 {
            &self.live
        } else {
            &endpoint.live
        };
        // Answers belong to the session the daemon shows phones now: Herdr
        // 1's while it answers there, else this computer's herdr-core.
        let Health::Running(status) = &supervisor.health else {
            return;
        };
        let boot = status.boot_id.clone();
        let herdr = live
            .snapshot
            .as_ref()
            .filter(|_| live.local_daemon_peer && live.status.is_connected())
            .filter(|snapshot| snapshot.boot_id == boot);
        // The workspaces the app keeps for Herdr 2 on this computer, which
        // the daemon lists beside Herdr 1's under the same boot.
        let kept = self
            .sessions(0)
            .core
            .map(|core| {
                core.workspaces
                    .iter()
                    .map(shell_workspace)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if herdr.is_none() && kept.is_empty() {
            return;
        }
        let workspaces = herdr
            .into_iter()
            .flat_map(|snapshot| snapshot.workspaces.iter().cloned())
            .chain(kept);
        let view = self
            .endpoint_pr_view(0)
            .filter(|view| view.origin == Origin::Local);
        let tokens = self
            .endpoint_pr_profile(0)
            .map(|profile| self.tokens_over(profile));
        let mut sources = Sources {
            boot_id: boot.clone(),
            ..Sources::default()
        };
        let mut entries = Vec::new();
        let mut unopened = Vec::new();
        for workspace in workspaces {
            let workspace = &workspace;
            let checkout = view
                .as_ref()
                .zip(workspace.worktree.as_ref())
                .zip(workspace.branch.as_ref())
                .map(|((view, tree), branch)| (view, tree, branch));
            let found = checkout
                .and_then(|(view, tree, branch)| self.menu.pr_cache.peek(view, &tree.key, branch));
            // Every workspace gets a source, past the badge limit and without
            // a pull request: its changes are still there to ask about.
            sources.workspaces.insert(
                workspace.workspace_id.clone(),
                Source::new(workspace.clone(), found.as_deref(), tokens.as_ref()),
            );
            let Some((view, tree, branch)) = checkout else {
                continue;
            };
            // The same reading the sidebar's row makes of this checkout.
            if let Some(ahead) = self.git.ahead(&tree.key, branch)
                && self
                    .menu
                    .pr_cache
                    .unopened(view, &tree.key, branch, &ahead.head)
            {
                let badge = UnopenedBranch {
                    workspace_id: workspace.workspace_id.clone(),
                    cwd: workspace.new_workspace_cwd.clone(),
                    branch: branch.clone(),
                    commits: ahead.commits,
                };
                if badge.valid() && unopened.len() < MAX_PULL_REQUESTS {
                    unopened.push(badge);
                }
                continue;
            }
            let Some(pr) = found else {
                continue;
            };
            if entries.len() == MAX_PULL_REQUESTS {
                continue;
            }
            let badge = WorkspacePullRequest {
                workspace_id: workspace.workspace_id.clone(),
                cwd: workspace.new_workspace_cwd.clone(),
                branch: branch.clone(),
                number: pr.number,
                url: pr.url.clone(),
                tone: pr.tone(),
            };
            if badge.valid() {
                entries.push(badge);
            }
        }
        entries.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
        unopened.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
        let update = PullRequests {
            boot_id: boot,
            entries,
            unopened,
        };
        let supervisor = cx.global_mut::<Supervisor>();
        supervisor.set_change_sources(sources);
        if !update.valid() {
            return;
        }
        supervisor.pull_requests_checked = Some(Instant::now());
        if supervisor.pull_requests.as_deref() != Some(&update) {
            supervisor.pull_requests = Some(Arc::new(update));
            supervisor.poke = true;
        }
    }
}

/// A workspace the app keeps for Herdr 2, in the shape Herdr 1 reports
/// its own, which Review and pull request lookups read.
fn shell_workspace(workspace: &sheprd_session::Workspace) -> ClientShellWorkspace {
    ClientShellWorkspace {
        workspace_id: workspace.id.clone(),
        active_tab_id: String::new(),
        new_workspace_cwd: workspace.cwd.clone(),
        number: 0,
        label: workspace.label.clone(),
        custom_label: workspace.custom_label,
        branch: workspace.branch.clone(),
        git_ahead_behind: workspace.ahead_behind,
        tokens: workspace.tokens.clone(),
        worktree: workspace.worktree.as_ref().map(|tree| ClientShellWorktree {
            key: tree.key.clone(),
            label: tree.label.clone(),
            is_linked_worktree: tree.linked,
        }),
        focused: false,
        agent_status: sheprd_client::protocol::AgentStatus::Unknown,
    }
}

#[cfg(test)]
mod tests;
