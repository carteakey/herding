use super::{PaneCandidate, REFRESH, TeamAction, roster, status_label};
use crate::{HerdrWindow, i18n::fl, navigation::NavigationTarget, team::model::PaneIdentity};
use gpui::Context;
use herdr_orchestrator::team_store::{Task, Team};
use std::time::Instant;

impl HerdrWindow {
    /// Sessions this window can prove: native ACP chats, verified CLI bindings,
    /// and active terminal agent sessions in the selected daemon's snapshot.
    pub(super) fn team_candidates(&self) -> Vec<PaneCandidate> {
        let Some(snapshot) = self.live.snapshot.as_deref() else {
            return Vec::new();
        };
        if !self.live.status.is_connected() || snapshot.boot_id.is_empty() {
            return Vec::new();
        }
        snapshot
            .panes
            .iter()
            .filter_map(|pane| {
                let (session, agent_kind) =
                    crate::control::server::team::prove_pane_session(self, &pane.pane_id)?;
                let agent_entry = snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.pane_id == pane.pane_id);
                let base_label = pane
                    .label
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| agent_entry.and_then(|a| a.title.as_deref()))
                    .unwrap_or(pane.pane_id.as_str());
                let label = if agent_kind.is_empty() {
                    base_label.to_owned()
                } else {
                    format!("{base_label} ({agent_kind})")
                };
                let status = agent_entry
                    .map(|a| status_label(a.agent_status))
                    .unwrap_or_else(|| fl!("team-status-live").into());
                Some(PaneCandidate {
                    identity: PaneIdentity {
                        daemon_id: snapshot.boot_id.clone(),
                        session_id: Some(session),
                        workspace_id: pane.workspace_id.clone(),
                        pane_id: pane.pane_id.clone(),
                    },
                    label: label.into(),
                    agent: agent_kind.into(),
                    status,
                })
            })
            .collect()
    }

    /// Runs on each window tick. Idle until the Team panel is first shown.
    pub(crate) fn poll_team(&mut self, cx: &mut Context<Self>) {
        let shown = self.team_panel_shown();
        if !shown && !self.team_bridge.started {
            return;
        }
        let candidates = self.team_candidates();
        let actions = self.take_team_actions(cx);
        let bridge = &mut self.team_bridge;
        bridge.start();
        if let Some(service) = bridge.service.as_ref() {
            for completion in service.poll() {
                bridge.complete(completion);
            }
        }
        let mut focus = None;
        // Prompts whose write the store confirmed since the last tick: an agent
        // is told about a row only once the row exists.
        let mut prompts = bridge.take_released_prompts();
        let mut spawn_agents = Vec::new();
        let mut split_pane = false;
        for action in actions {
            match action {
                // Panes belong to the window, so focusing is not the bridge's.
                TeamAction::FocusWorker { pane } => {
                    bridge.last_activity = Some(format!("Focused pane {}", pane.pane_id).into());
                    bridge.dirty = true;
                    focus = Some(pane);
                }
                TeamAction::SpawnAgentSession { agent } => {
                    bridge.last_activity =
                        Some(format!("Spawning {} session...", agent.display_name()).into());
                    bridge.dirty = true;
                    spawn_agents.push(agent);
                }
                TeamAction::SplitNewPane => {
                    bridge.last_activity = Some("Splitting new terminal pane...".into());
                    bridge.dirty = true;
                    split_pane = true;
                }
                action => {
                    let dispatches =
                        prompts_for_team_action(&action, bridge.team.as_ref(), &bridge.tasks);
                    bridge.act(action, &candidates);
                    if bridge.warning.is_none() {
                        bridge.hold_prompts_for_action(dispatches);
                    }
                    prompts.extend(bridge.drain_unpaused_prompts());
                }
            }
        }
        if shown
            && let Some(team_id) = bridge.selected.clone()
            && bridge.last_refresh.is_none_or(|at| at.elapsed() >= REFRESH)
        {
            bridge.refresh(&team_id);
        }
        // The bounded periodic supervisor check: only the shown team, only
        // unseen results, at most one attempt per SUPERVISOR_FALLBACK.
        if shown && let Some(wake) = self.team_bridge.supervisor_fallback_due(Instant::now()) {
            let team_id = wake.team_id.clone();
            let outcome = self.enqueue_or_dispatch_team_prompt(
                Some(team_id.as_str()),
                &wake.pane_id,
                wake.prompt,
                wake.summary,
                cx,
            );
            if matches!(outcome, PromptOutcome::Sent | PromptOutcome::Held) {
                self.team_bridge
                    .mark_supervisor_woken(&team_id, wake.signals);
            }
        }
        for agent in spawn_agents {
            self.start_chat_or_flash(agent, cx);
        }
        if split_pane {
            self.split_team_pane(cx);
        }
        let current_team = self.team_bridge.current_team_id().map(ToOwned::to_owned);
        for (pane_id, prompt, summary) in prompts {
            self.enqueue_or_dispatch_team_prompt(
                current_team.as_deref(),
                &pane_id,
                prompt,
                summary,
                cx,
            );
        }
        let identities: Vec<_> = candidates.iter().map(|c| c.identity.clone()).collect();
        let focus = focus.filter(|pane| {
            candidates
                .iter()
                .any(|c| c.identity.pane_id == pane.pane_id)
                || self
                    .live
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.panes.iter().any(|p| p.pane_id == pane.pane_id))
        });
        let bridge = &mut self.team_bridge;
        if bridge.dirty || bridge.candidates != identities {
            bridge.dirty = false;
            bridge.candidates = identities;
            let warning = bridge.warning.clone();
            let snapshot = bridge.snapshot(&candidates);
            self.set_team_snapshot(snapshot, cx);
            self.team_view
                .update(cx, |view, cx| view.set_action_warning(warning, cx));
        }
        if let Some(pane) = focus {
            self.navigate(NavigationTarget::Pane(pane.pane_id.as_str()), cx);
            if let Some(tab_id) = self
                .live
                .snapshot
                .as_ref()
                .and_then(|s| s.panes.iter().find(|p| p.pane_id == pane.pane_id))
                .map(|p| p.tab_id.clone())
            {
                self.terminal_focus_moved(&tab_id, false, cx);
            }
        }
    }

    pub(crate) fn enqueue_or_dispatch_team_prompt(
        &mut self,
        team_id: Option<&str>,
        pane_id: &str,
        prompt: String,
        summary: String,
        cx: &mut Context<Self>,
    ) -> PromptOutcome {
        let team_id = team_id
            .or_else(|| self.team_bridge.current_team_id())
            .unwrap_or_default();
        if self.team_bridge.is_pane_paused_for_team(team_id, pane_id) {
            self.team_bridge.enqueue_prompt(
                team_id.to_owned(),
                pane_id.to_owned(),
                prompt,
                summary,
            );
            let held = self.team_bridge.queued_count();
            self.team_bridge.last_activity =
                Some(format!("Held prompt for paused pane {pane_id} ({held} queued)").into());
            self.team_bridge.dirty = true;
            return PromptOutcome::Held;
        }
        let outcome = self.dispatch_team_prompt(pane_id, prompt, cx);
        self.team_bridge.last_activity = Some(
            match outcome {
                PromptOutcome::Sent => summary,
                PromptOutcome::Held => summary,
                PromptOutcome::Unavailable => format!(
                    "{summary}: not delivered, no reachable agent in {pane_id}; work stays pending"
                ),
                PromptOutcome::Failed => format!(
                    "{summary}: not delivered, the agent refused the prompt; work stays pending"
                ),
            }
            .into(),
        );
        self.team_bridge.dirty = true;
        outcome
    }

    fn dispatch_team_prompt(
        &mut self,
        pane_id: &str,
        prompt: String,
        cx: &mut Context<Self>,
    ) -> PromptOutcome {
        if let Some(chat) = self.chats.pane_mut(pane_id)
            && chat.target.existing.is_none()
            && !chat.session.transcript().read_only
        {
            return chat
                .send(herdr_chat_protocol::Command::Prompt {
                    text: prompt,
                    attachments: Vec::new(),
                })
                .map_or(PromptOutcome::Failed, |_| PromptOutcome::Sent);
        }
        if self.endpoints.is_empty() {
            return PromptOutcome::Unavailable;
        }
        let target = crate::browser::FeedbackKey {
            scope: crate::browser::scope(&self.endpoints[self.selected_endpoint]),
            pane_id: pane_id.to_owned(),
        };
        let outcome = match self.deliver_notes(Some(target), prompt, None, None, cx) {
            crate::agent_notes::Delivered::Agent => PromptOutcome::Sent,
            crate::agent_notes::Delivered::Kept | crate::agent_notes::Delivered::Copy(_) => {
                PromptOutcome::Unavailable
            }
        };
        self.poll_deliveries(cx);
        outcome
    }

    fn split_team_pane(&mut self, cx: &mut Context<Self>) {
        if !self.input_ready() || self.endpoints.is_empty() {
            return;
        }
        let Some(snapshot) = self.live.snapshot.as_ref() else {
            return;
        };
        let Some(focused_pane) = snapshot
            .focused_pane_id
            .clone()
            .or_else(|| snapshot.panes.first().map(|p| p.pane_id.clone()))
        else {
            return;
        };
        let boot = snapshot.boot_id.clone();
        if let Some(handle) = self.endpoints[self.selected_endpoint]
            .connection
            .handle
            .as_ref()
        {
            let _ = handle.request(
                &boot,
                herdr_client::Method::PaneSplit,
                serde_json::json!({
                    "target_pane_id": focused_pane,
                    "direction": "right",
                    "focus": true,
                }),
            );
            self.fence_focus_change(None);
            cx.notify();
        }
    }
}

pub(crate) fn team_cli_prefix(pane_id: &str, workspace_id: Option<&str>) -> String {
    let bin = std::env::current_exe()
        .ok()
        .map(|p| {
            let s = p.display().to_string();
            s.strip_suffix(" (deleted)").unwrap_or(&s).to_owned()
        })
        .unwrap_or_else(|| "herdr-gpui".to_owned());
    match workspace_id.filter(|w| !w.trim().is_empty()) {
        Some(ws) => format!("HERDR_PANE_ID={pane_id} HERDR_WORKSPACE_ID={ws} {bin} team"),
        None => format!("HERDR_PANE_ID={pane_id} {bin} team"),
    }
}

fn cli_for_member(team: Option<&Team>, pane_id: &str) -> String {
    let ws = team.and_then(|t| {
        roster(t)
            .find(|m| m.target.pane.as_deref() == Some(pane_id))
            .and_then(|m| m.target.workspace.as_deref())
    });
    team_cli_prefix(pane_id, ws)
}

fn prompts_for_team_action(
    action: &TeamAction,
    team: Option<&Team>,
    tasks: &[Task],
) -> Vec<(String, String, String)> {
    match action {
        TeamAction::CreateTeam { name, supervisor } if !name.trim().is_empty() => {
            let cli = team_cli_prefix(&supervisor.pane_id, Some(&supervisor.workspace_id));
            vec![(
                supervisor.pane_id.clone(),
                format!(
                    "[Herdr Team] You are the Supervisor of team \"{}\". Strictly supervise: decompose the goal into tasks, assign tasks to workers, review findings, and coordinate via the bulletin rather than doing implementation work yourself. Load skill \"herdr-team\" for workflow and workspace rules. Run `{cli} status` to get your TEAM_ID, `{cli} roster <TEAM_ID>` to inspect workers, `{cli} task list <TEAM_ID>` to manage tasks, and `{cli} bulletin list <TEAM_ID>` to read messages (run `{cli} bulletin ack <TEAM_ID> <MESSAGE_ID>` to acknowledge).",
                    name.trim()
                ),
                format!("Started team \"{}\" with supervisor {}", name.trim(), supervisor.pane_id),
            )]
        }
        TeamAction::SetGoal { team_id, goal } if !goal.trim().is_empty() => team
            .and_then(supervisor_pane_id)
            .into_iter()
            .map(|pane_id| {
                let cli = cli_for_member(team, &pane_id);
                (
                    pane_id.clone(),
                    format!(
                        "[Herdr Team] Team goal set for {team_id}: \"{}\". As Supervisor, do not implement everything yourself — break this goal into concrete tasks with `{cli} task create {team_id} --title \"...\" --assign <worker>`, delegate to workers, and coordinate via `{cli} bulletin post {team_id} \"...\"`.",
                        goal.trim()
                    ),
                    format!("Sent updated goal to supervisor ({pane_id})"),
                )
            })
            .collect(),
        TeamAction::AddWorker {
            team_id,
            name,
            pane,
        } if !name.trim().is_empty() => {
            let cli = team_cli_prefix(&pane.pane_id, Some(&pane.workspace_id));
            vec![(
                pane.pane_id.clone(),
                format!(
                    "[Herdr Team] You joined team {team_id} as Worker \"{}\". Load skill \"herdr-team\" for workflow and workspace rules. Run `{cli} status {team_id}` to view the goal, `{cli} task list {team_id}` to claim open tasks (`{cli} task claim {team_id} <TASK_ID>`), and `{cli} bulletin list {team_id}` to read messages (run `{cli} bulletin ack {team_id} <MESSAGE_ID>` to acknowledge).",
                    name.trim()
                ),
                format!("Added worker \"{}\" ({})", name.trim(), pane.pane_id),
            )]
        }
        TeamAction::NudgeMember { member_id } => {
            let Some(team) = team else {
                return Vec::new();
            };
            let team_id = &team.team_id;
            roster(team)
                .find(|m| m.member_id == *member_id)
                .and_then(|m| {
                    let pane_id = m.target.pane.clone()?;
                    let cli = team_cli_prefix(&pane_id, m.target.workspace.as_deref());
                    Some((
                        pane_id.clone(),
                        format!(
                            "[Herdr Team] Status check for {} in team {team_id}: please run `{cli} status {team_id}`, `{cli} task list {team_id}`, and `{cli} bulletin list {team_id}` to check active tasks and acknowledge messages with `{cli} bulletin ack {team_id} <MESSAGE_ID>`.",
                            m.display_name
                        ),
                        format!("Nudged {} ({pane_id})", m.display_name),
                    ))
                })
                .into_iter()
                .collect()
        }
        TeamAction::SelectSupervisor { team_id, member_id } => {
            let Some(team) = team else {
                return Vec::new();
            };
            roster(team)
                .find(|m| m.member_id == *member_id)
                .and_then(|m| {
                    let pane_id = m.target.pane.clone()?;
                    let cli = team_cli_prefix(&pane_id, m.target.workspace.as_deref());
                    Some((
                        pane_id.clone(),
                        format!(
                            "[Herdr Team] You ({}) have been promoted to Supervisor of team \"{}\" ({team_id}). Strictly supervise: delegate tasks to workers, review their findings, and coordinate via the bulletin rather than implementing tasks yourself. Load skill \"herdr-team\" for workflow and workspace rules. Run `{cli} status {team_id}`, `{cli} roster {team_id}`, `{cli} task list {team_id}`, and `{cli} bulletin list {team_id}` (run `{cli} bulletin ack {team_id} <MESSAGE_ID>` to acknowledge).",
                            m.display_name, team.name
                        ),
                        format!("Promoted {} to supervisor ({pane_id})", m.display_name),
                    ))
                })
                .into_iter()
                .collect()
        }
        TeamAction::CreateTask {
            team_id,
            title,
            brief,
            assignee,
        } if !title.trim().is_empty() => {
            let target_pane = team.and_then(|team| {
                resolve_assignee_pane(team, assignee.as_deref())
                    .or_else(|| supervisor_pane_id(team))
            });
            let brief_part = if brief.trim().is_empty() {
                String::new()
            } else {
                format!(" — {}", brief.trim())
            };
            target_pane
                .into_iter()
                .map(|pane_id| {
                    let cli = cli_for_member(team, &pane_id);
                    (
                        pane_id.clone(),
                        format!(
                            "[Herdr Team] New task in team {team_id}: \"{}\"{}. Run `{cli} task list {team_id}` to inspect it, `{cli} task claim {team_id} <TASK_ID>` to start working on it, and `{cli} task complete {team_id} <TASK_ID> --result \"...\"` when finished.",
                            title.trim(),
                            brief_part
                        ),
                        format!("Dispatched task \"{}\" to {pane_id}", title.trim()),
                    )
                })
                .collect()
        }
        TeamAction::UpdateTask {
            team_id,
            task_id,
            title,
            brief,
            assignee,
        } if !title.trim().is_empty() => {
            let target_pane =
                team.and_then(|team| resolve_assignee_pane(team, assignee.as_deref()));
            let brief_part = if brief.trim().is_empty() {
                String::new()
            } else {
                format!(" — {}", brief.trim())
            };
            target_pane
                .into_iter()
                .map(|pane_id| {
                    let cli = cli_for_member(team, &pane_id);
                    (
                        pane_id.clone(),
                        format!(
                            "[Herdr Team] Task {task_id} assigned to you in team {team_id}: \"{}\"{}. Claim it with `{cli} task claim {team_id} {task_id}` and complete it with `{cli} task complete {team_id} {task_id} --result \"...\"`.",
                            title.trim(),
                            brief_part
                        ),
                        format!("Assigned task \"{}\" to {pane_id}", title.trim()),
                    )
                })
                .collect()
        }
        TeamAction::DispatchTask { task_id } => {
            let Some(team) = team else {
                return Vec::new();
            };
            let team_id = &team.team_id;
            let Some(task) = tasks.iter().find(|t| t.task_id == *task_id) else {
                return Vec::new();
            };
            let target_pane = task
                .assignee
                .as_deref()
                .and_then(|id| resolve_assignee_pane(team, Some(id)))
                .or_else(|| supervisor_pane_id(team));
            let brief_part = if task.brief.trim().is_empty() {
                String::new()
            } else {
                format!(" — {}", task.brief.trim())
            };
            target_pane
                .into_iter()
                .map(|pane_id| {
                    let cli = cli_for_member(Some(team), &pane_id);
                    (
                        pane_id.clone(),
                        format!(
                            "[Herdr Team] Please work on task {} (\"{}\"){} in team {team_id}. Claim it with `{cli} task claim {team_id} {}` and complete it with `{cli} task complete {team_id} {} --result \"...\"`.",
                            task.task_id, task.title, brief_part, task.task_id, task.task_id
                        ),
                        format!("Sent task \"{}\" to {pane_id}", task.title),
                    )
                })
                .collect()
        }
        TeamAction::Post {
            team_id,
            author,
            body,
            ..
        }
        | TeamAction::Reply {
            team_id,
            author,
            body,
            ..
        } if !body.trim().is_empty() => {
            let Some(team) = team else {
                return Vec::new();
            };
            let recipients = match herdr_orchestrator::team_store::resolve_recipients(
                team,
                author,
                &[],
                body,
            ) {
                Ok(recipients) => recipients,
                Err(_) => return Vec::new(),
            };
            roster(team)
                .filter(|m| match &recipients {
                    herdr_orchestrator::team_store::Recipients::All => match author {
                        herdr_orchestrator::team_store::Author::Member { member_id } => {
                            m.member_id != *member_id
                        }
                        herdr_orchestrator::team_store::Author::Human { .. } => true,
                    },
                    herdr_orchestrator::team_store::Recipients::Explicit(ids) => {
                        ids.contains(&m.member_id)
                    }
                })
                .filter_map(|m| {
                    let pane_id = m.target.pane.clone()?;
                    let cli = team_cli_prefix(&pane_id, m.target.workspace.as_deref());
                    Some((
                        pane_id.clone(),
                        format!(
                            "[Herdr Team] New bulletin message in team {team_id}: \"{}\". Run `{cli} bulletin list {team_id}` to view, `{cli} bulletin ack {team_id} <MESSAGE_ID>` to acknowledge, and `{cli} bulletin post {team_id} \"...\"` to reply.",
                            body.trim()
                        ),
                        format!("Posted bulletin to team ({pane_id})"),
                    ))
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// What actually happened to a dispatched team prompt, so activity lines
/// never claim a delivery that did not reach an agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PromptOutcome {
    /// Sent to the agent now, or queued by the delivery machinery for when it
    /// is idle.
    Sent,
    /// Held in the bridge queue for a paused team or member.
    Held,
    /// No reachable agent in the pane; the text was kept, not injected.
    Unavailable,
    /// The agent or chat refused the prompt.
    Failed,
}

/// Caps a stored result to what fits in a wake prompt.
pub(crate) fn excerpt(text: &str) -> String {
    let trimmed = text.trim();
    const LIMIT: usize = 240;
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_owned();
    }
    let cut: String = trimmed.chars().take(LIMIT).collect();
    format!("{cut}...")
}

/// Dedupe key for a supervisor result wake: one stored task state, so a
/// re-opened task wakes again while unchanged state never does.
pub(crate) fn wake_signal(task: &Task) -> String {
    format!(
        "{}|{}|{}",
        task.task_id,
        task.status.as_str(),
        task.updated_at.timestamp_millis()
    )
}

fn resolve_assignee_pane(team: &Team, assignee: Option<&str>) -> Option<String> {
    let target = assignee?.trim();
    if target.is_empty() {
        return None;
    }
    roster(team)
        .find(|m| {
            m.member_id == target
                || m.target.pane.as_deref() == Some(target)
                || m.display_name.eq_ignore_ascii_case(target)
        })
        .and_then(|m| m.target.pane.clone())
}

fn supervisor_pane_id(team: &Team) -> Option<String> {
    team.supervisor.target.pane.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdr_orchestrator::team_store::{Author, Member, SessionTarget, Task, TaskStatus};

    fn test_team() -> Team {
        Team {
            team_id: "t1".into(),
            name: "team1".into(),
            supervisor: Member {
                member_id: "m_sup".into(),
                display_name: "supervisor".into(),
                target: SessionTarget {
                    host: "local".into(),
                    session: "default".into(),
                    workspace: Some("ws".into()),
                    pane: Some("p_sup".into()),
                },
            },
            members: vec![
                Member {
                    member_id: "m_w1".into(),
                    display_name: "worker1".into(),
                    target: SessionTarget {
                        host: "local".into(),
                        session: "default".into(),
                        workspace: Some("ws".into()),
                        pane: Some("p_w1".into()),
                    },
                },
                Member {
                    member_id: "m_w2".into(),
                    display_name: "worker2".into(),
                    target: SessionTarget {
                        host: "local".into(),
                        session: "default".into(),
                        workspace: Some("ws".into()),
                        pane: Some("p_w2".into()),
                    },
                },
            ],
        }
    }

    #[test]
    fn bulletin_post_targets_only_mentioned_recipient() {
        let team = test_team();
        let action = TeamAction::Post {
            team_id: "t1".into(),
            author: Author::Human {
                user_id: "human".into(),
            },
            body: "@worker1 please check this".into(),
        };
        let prompts = prompts_for_team_action(&action, Some(&team), &[]);
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].0, "p_w1");
        assert!(prompts[0].1.contains("@worker1 please check this"));
    }

    #[test]
    fn bulletin_prompts_instruct_explicit_ack() {
        let team = test_team();
        let post_action = TeamAction::Post {
            team_id: "t1".into(),
            author: Author::Human {
                user_id: "human".into(),
            },
            body: "@worker1 please check this".into(),
        };
        let prompts = prompts_for_team_action(&post_action, Some(&team), &[]);
        assert!(prompts[0].1.contains("bulletin ack t1"));

        let worker_action = TeamAction::AddWorker {
            team_id: "t1".into(),
            name: "worker3".into(),
            pane: PaneIdentity {
                daemon_id: "d1".into(),
                session_id: Some("s1".into()),
                workspace_id: "ws".into(),
                pane_id: "p_w3".into(),
            },
        };
        let worker_prompts = prompts_for_team_action(&worker_action, Some(&team), &[]);
        assert!(worker_prompts[0].1.contains("bulletin ack t1"));
    }

    #[test]
    fn bulletin_post_broadcasts_to_all_when_no_mention() {
        let team = test_team();
        let action = TeamAction::Post {
            team_id: "t1".into(),
            author: Author::Human {
                user_id: "human".into(),
            },
            body: "Team sync in 5m".into(),
        };
        let prompts = prompts_for_team_action(&action, Some(&team), &[]);
        assert_eq!(prompts.len(), 3);
        let panes: Vec<_> = prompts.iter().map(|(p, _, _)| p.as_str()).collect();
        assert!(panes.contains(&"p_sup"));
        assert!(panes.contains(&"p_w1"));
        assert!(panes.contains(&"p_w2"));
    }

    #[test]
    fn bulletin_post_excludes_member_author_from_all_broadcast() {
        let team = test_team();
        let action = TeamAction::Post {
            team_id: "t1".into(),
            author: Author::Member {
                member_id: "m_w1".into(),
            },
            body: "General update from w1".into(),
        };
        let prompts = prompts_for_team_action(&action, Some(&team), &[]);
        assert_eq!(prompts.len(), 2);
        let panes: Vec<_> = prompts.iter().map(|(p, _, _)| p.as_str()).collect();
        assert!(panes.contains(&"p_sup"));
        assert!(panes.contains(&"p_w2"));
        assert!(!panes.contains(&"p_w1"));
    }

    #[test]
    fn bulletin_post_with_invalid_mention_dispatches_to_none() {
        let team = test_team();
        let action = TeamAction::Post {
            team_id: "t1".into(),
            author: Author::Human {
                user_id: "human".into(),
            },
            body: "@nonexistent_person check this".into(),
        };
        let prompts = prompts_for_team_action(&action, Some(&team), &[]);
        assert_eq!(prompts.len(), 0);
    }

    fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(secs, 0).unwrap_or(chrono::DateTime::UNIX_EPOCH)
    }

    fn signal_task(status: TaskStatus) -> Task {
        Task {
            task_id: "task1".into(),
            team_id: "t1".into(),
            creator: Author::Human {
                user_id: "human".into(),
            },
            title: "Write report".into(),
            brief: String::new(),
            assignee: None,
            status,
            blocker_reason: None,
            result: None,
            created_at: at(0),
            updated_at: at(10),
        }
    }

    #[test]
    fn excerpt_caps_long_results() {
        let long = "x".repeat(500);
        assert_eq!(excerpt(&long).chars().count(), 243);
        assert!(excerpt(&long).ends_with("..."));
        assert_eq!(excerpt("  short  "), "short");
    }

    #[test]
    fn wake_signal_tracks_status_and_revision() {
        let mut task = signal_task(TaskStatus::Done);
        let first = wake_signal(&task);
        task.updated_at = at(11);
        assert_ne!(wake_signal(&task), first, "a re-done task wakes again");
        task.status = TaskStatus::Open;
        assert_ne!(wake_signal(&task), first, "a reopened task wakes again");
    }
}
