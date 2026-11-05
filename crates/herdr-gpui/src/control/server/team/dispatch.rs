//! Waking agents from durable team mutations: the prompts a stored change
//! produces and where they are delivered.
use crate::control::server::team_worker::Reply;
use gpui::App;
use herdr_orchestrator::team_store::{Author, Task, TaskStatus, Team};

/// One prompt prepared from a durable team mutation. `signals` marks a
/// supervisor result wake: the window bridge dedupes on them so an event and
/// the periodic fallback deliver each result once.
pub(crate) struct TeamDispatch {
    pub(crate) team_id: String,
    pub(crate) pane_id: String,
    pub(crate) prompt: String,
    pub(crate) summary: String,
    pub(crate) signals: Vec<String>,
}

pub(super) fn notify_team_mutation(reply: &Reply, cx: &mut App) {
    let dispatches = prompts_for_scoped_reply(reply);
    for handle in crate::control::main_windows(cx) {
        let _ = handle.update(cx, |view, _, cx| {
            view.team_bridge.mark_dirty();
            if view.live.status.is_connected() {
                for dispatch in &dispatches {
                    // The escape hatch: a member whose session heartbeat is
                    // dead is never dispatched to; the work stays pending on
                    // the board until a live session takes it.
                    if view.team_session_dead(&dispatch.pane_id) {
                        continue;
                    }
                    if !dispatch.signals.is_empty()
                        && dispatch
                            .signals
                            .iter()
                            .all(|signal| view.team_bridge.wake_seen(&dispatch.team_id, signal))
                    {
                        continue;
                    }
                    let outcome = view.enqueue_or_dispatch_team_prompt(
                        Some(dispatch.team_id.as_str()),
                        &dispatch.pane_id,
                        dispatch.prompt.clone(),
                        dispatch.summary.clone(),
                        dispatch.signals.clone(),
                        cx,
                    );
                    // A held or undeliverable wake is not recorded: the queued
                    // prompt or the fallback still carries it.
                    if matches!(outcome, crate::team::bridge::prompts::PromptOutcome::Sent)
                        && !dispatch.signals.is_empty()
                    {
                        view.team_bridge
                            .mark_supervisor_woken(&dispatch.team_id, dispatch.signals.clone());
                    }
                }
            }
            cx.notify();
        });
    }
}

pub(super) fn prompts_for_scoped_reply(reply: &Reply) -> Vec<TeamDispatch> {
    match reply {
        Reply::ScopedPosted { team, post } => {
            let team_id = &team.team_id;
            let (_, author_name) = super::projection::author_parts(team, &post.author);
            std::iter::once(&team.supervisor)
                .chain(team.members.iter())
                .filter(|m| post.mentions.contains(&m.member_id))
                .filter_map(|m| {
                    let pane_id = m.target.pane.clone()?;
                    let cli = crate::team::bridge::prompts::team_cli_prefix(
                        &pane_id,
                        m.target.workspace.as_deref(),
                    );
                    Some(TeamDispatch {
                        team_id: team_id.clone(),
                        pane_id: pane_id.clone(),
                        prompt: format!(
                            "[Herdr Team] @{}: new bulletin from {author_name} in team {team_id} (msg {}): \"{}\". Run `{cli} bulletin ack {team_id} {}` to acknowledge and `{cli} bulletin post {team_id} --reply-to {} \"...\"` to reply.",
                            m.display_name, post.post_id, post.body, post.post_id, post.post_id
                        ),
                        summary: format!("{author_name} mentioned @{} ({pane_id})", m.display_name),
                        signals: Vec::new(),
                    })
                })
                .collect()
        }
        Reply::ScopedTask { team, task, .. } if task.status == TaskStatus::Open => {
            let team_id = &team.team_id;
            let Some(assignee_id) = task.assignee.as_deref() else {
                return Vec::new();
            };
            std::iter::once(&team.supervisor)
                .chain(team.members.iter())
                .find(|m| m.member_id == assignee_id)
                .and_then(|m| {
                    let pane_id = m.target.pane.clone()?;
                    let cli = crate::team::bridge::prompts::team_cli_prefix(
                        &pane_id,
                        m.target.workspace.as_deref(),
                    );
                    Some(TeamDispatch {
                        team_id: team_id.clone(),
                        pane_id: pane_id.clone(),
                        prompt: format!(
                            "[Herdr Team] Task assigned to {} in team {team_id}: {} (\"{}\"). Claim it with `{cli} task claim {team_id} {}` and complete it with `{cli} task complete {team_id} {} --result \"...\"`.",
                            m.display_name, task.task_id, task.title, task.task_id, task.task_id
                        ),
                        summary: format!("Assigned task \"{}\" to {} ({pane_id})", task.title, m.display_name),
                        signals: vec![assignee_signal(task)],
                    })
                })
                .into_iter()
                .collect()
        }
        // A finished or blocked task wakes the supervisor once per stored
        // result, so coordination continues without polling. The supervisor's
        // own writes do not wake it.
        Reply::ScopedTask { team, task, actor }
            if matches!(task.status, TaskStatus::Done | TaskStatus::Blocked)
                && !matches!(actor, Author::Member { member_id }
                    if member_id == &team.supervisor.member_id) =>
        {
            supervisor_wake(team, task)
        }
        _ => Vec::new(),
    }
}

/// Dedupe key for an assignee wake: one stored assignment state, so a
/// re-assign wakes once per new state while unchanged state never does,
/// the same semantics the supervisor result wake's signal carries.
pub(crate) fn assignee_signal(task: &Task) -> String {
    format!(
        "assignee|{}|{}|{}",
        task.task_id,
        task.assignee.as_deref().unwrap_or(""),
        task.updated_at.timestamp_millis()
    )
}

/// The supervisor wake prompt for a task the supervisor must react to.
fn supervisor_wake(team: &Team, task: &Task) -> Vec<TeamDispatch> {
    let team_id = &team.team_id;
    let Some(pane_id) = team.supervisor.target.pane.clone() else {
        return Vec::new();
    };
    let cli = crate::team::bridge::prompts::team_cli_prefix(
        &pane_id,
        team.supervisor.target.workspace.as_deref(),
    );
    let who = task
        .assignee
        .as_deref()
        .and_then(|id| {
            std::iter::once(&team.supervisor)
                .chain(team.members.iter())
                .find(|m| m.member_id == id)
        })
        .map(|m| format!(" by {}", m.display_name))
        .unwrap_or_default();
    let (situation, summary) = match task.status {
        TaskStatus::Done => (
            format!(
                "was completed{} with result: \"{}\"",
                who,
                crate::team::bridge::prompts::excerpt(task.result.as_deref().unwrap_or("none"))
            ),
            format!("Woke supervisor: task \"{}\" done", task.title),
        ),
        TaskStatus::Blocked => (
            format!(
                "is blocked{}: \"{}\"",
                who,
                task.blocker_reason
                    .as_deref()
                    .unwrap_or("no reason recorded")
            ),
            format!("Woke supervisor: task \"{}\" blocked", task.title),
        ),
        _ => return Vec::new(),
    };
    vec![TeamDispatch {
        team_id: team_id.clone(),
        pane_id,
        prompt: format!(
            "[Herdr Team] Task {} (\"{}\") {situation} in team {team_id}. Run `{cli} task list {team_id}` to review the board, take the next clear action (assign follow-up work, record findings, or ask the human only if a decision genuinely needs them), then go idle. Do not poll or re-check for updates: further results will wake you, and if the goal is complete or nothing clear remains, stop.",
            task.task_id, task.title
        ),
        summary,
        signals: vec![crate::team::bridge::prompts::wake_signal(task)],
    }]
}
