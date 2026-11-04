use crate::i18n::fl;
use gpui::SharedString;
use herdr_orchestrator::team_store::{Author, Confidence, TaskStatus};

/// Explicit daemon, session, workspace, and pane scope for a worker target.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PaneIdentity {
    pub(crate) daemon_id: String,
    /// `None` when the daemon's current snapshot cannot prove the session.
    pub(crate) session_id: Option<String>,
    pub(crate) workspace_id: String,
    pub(crate) pane_id: String,
}

impl PaneIdentity {
    pub(crate) fn verified(&self) -> bool {
        !self.daemon_id.trim().is_empty()
            && !self.workspace_id.trim().is_empty()
            && !self.pane_id.trim().is_empty()
            && self
                .session_id
                .as_ref()
                .is_some_and(|session| !session.trim().is_empty())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PaneCandidate {
    pub(crate) identity: PaneIdentity,
    pub(crate) label: SharedString,
    pub(crate) agent: SharedString,
    pub(crate) status: SharedString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeliveryState {
    Pending,
    Acknowledged,
}

#[derive(Clone, Debug)]
pub(crate) struct TeamMember {
    pub(crate) member_id: String,
    pub(crate) name: SharedString,
    pub(crate) role: SharedString,
    pub(crate) agent: SharedString,
    pub(crate) pane: Option<PaneIdentity>,
    pub(crate) target_pane: Option<PaneIdentity>,
    pub(crate) status: SharedString,
    pub(crate) supervisor: bool,
    pub(crate) paused: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct TeamPost {
    pub(crate) id: String,
    pub(crate) author: SharedString,
    pub(crate) body: SharedString,
    pub(crate) reply_to_author: Option<SharedString>,
    pub(crate) deliveries: Vec<(String, SharedString, DeliveryState)>,
}

#[derive(Clone, Debug)]
pub(crate) struct TeamTask {
    pub(crate) id: String,
    pub(crate) title: SharedString,
    pub(crate) brief: SharedString,
    pub(crate) assignee: Option<SharedString>,
    pub(crate) status: TaskStatus,
    pub(crate) blocker_reason: Option<SharedString>,
    pub(crate) result: Option<SharedString>,
}

#[derive(Clone, Debug)]
pub(crate) struct TeamFinding {
    pub(crate) id: String,
    pub(crate) author: SharedString,
    pub(crate) description: SharedString,
    pub(crate) evidence: SharedString,
    pub(crate) confidence: Confidence,
    pub(crate) supersedes: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct TeamChoice {
    pub(crate) team_id: String,
    pub(crate) name: SharedString,
}

#[derive(Clone, Debug)]
pub(crate) enum TeamLoadState {
    Loading,
    Ready,
    Failed(SharedString),
}

/// A bounded, already-prepared snapshot supplied by the service owner.
#[derive(Clone, Debug)]
pub(crate) struct TeamSnapshot {
    pub(crate) state: TeamLoadState,
    pub(crate) team_id: Option<String>,
    pub(crate) team_name: Option<SharedString>,
    pub(crate) team_goal: Option<SharedString>,
    /// Bounded choices allow selecting a persisted team after app restart.
    pub(crate) teams: Vec<TeamChoice>,
    pub(crate) teams_after: Option<String>,
    pub(crate) members: Vec<TeamMember>,
    pub(crate) pane_candidates: Vec<PaneCandidate>,
    pub(crate) posts: Vec<TeamPost>,
    pub(crate) tasks: Vec<TeamTask>,
    pub(crate) findings: Vec<TeamFinding>,
    /// Older task and finding pages may exist behind the board's
    /// show-earlier controls; false when the board is fully loaded.
    pub(crate) tasks_older: bool,
    pub(crate) findings_older: bool,
    /// An explicit model identity allowed to author posts. `None` hides the
    /// composer; the view never invents an agent or human author.
    pub(crate) post_author: Option<(Author, SharedString)>,
    pub(crate) bulletin_open: bool,
    pub(crate) paused: bool,
    pub(crate) queued_count: usize,
    pub(crate) last_activity: Option<SharedString>,
}

impl Default for TeamSnapshot {
    fn default() -> Self {
        Self {
            state: TeamLoadState::Failed(fl!("team-error-not-connected").into()),
            team_id: None,
            team_name: None,
            team_goal: None,
            teams: Vec::new(),
            teams_after: None,
            members: Vec::new(),
            pane_candidates: Vec::new(),
            posts: Vec::new(),
            tasks: Vec::new(),
            findings: Vec::new(),
            tasks_older: false,
            findings_older: false,
            post_author: None,
            bulletin_open: true,
            paused: false,
            queued_count: 0,
            last_activity: None,
        }
    }
}

#[cfg(test)]
mod tests;
