use super::{
    PaneCandidate, PromptDispatch, REFRESH, SupervisorWake, TeamAction, WAKE_RETRY, roster,
    status_label,
};
use crate::{SheprdWindow, i18n::fl, navigation::NavigationTarget, team::model::PaneIdentity};
use gpui::Context;
use sheprd_orchestrator::team_store::{Task, Team};
use std::collections::HashSet;
use std::time::Instant;

pub(crate) mod reregister;

#[cfg(test)]
#[path = "host_identity_tests.rs"]
mod host_identity_tests;

impl SheprdWindow {
    /// The connected endpoint whose snapshot reports `host` as its daemon
    /// boot id. `None` when no connected daemon currently runs that boot:
    /// the member's host is unreachable, and there is nothing to fall back
    /// to, because a bare pane id names no machine (#40).
    pub(crate) fn endpoint_for_host(&self, host: &str) -> Option<usize> {
        if host.is_empty() {
            return None;
        }
        // A member recorded since the stable-host change names their
        // endpoint's id, which survives daemon restarts; rows from before
        // it name the boot they registered under, so the boot the endpoint
        // currently reports matches those until the daemon restarts.
        self.endpoints
            .iter()
            .enumerate()
            .find(|(index, endpoint)| {
                let live = if *index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                live.status.is_connected()
                    && (endpoint.id == host
                        || live
                            .snapshot
                            .as_deref()
                            .is_some_and(|snapshot| snapshot.boot_id == host))
            })
            .map(|(index, _)| index)
    }

    /// Whether the agent session in `pane_id`, recorded on `host`, is dead:
    /// its state labels carry the crash vocabulary the heartbeat inspection
    /// reports. A dead session is never dispatched to; the work stays pending
    /// on the board. Scoped to the host's own endpoint: a crashed pane that
    /// happens to share the id on another host says nothing about this one.
    pub(crate) fn team_session_dead(&self, host: &str, pane_id: &str) -> bool {
        let Some(index) = self.endpoint_for_host(host) else {
            return false;
        };
        let live = if index == self.selected_endpoint {
            &self.live
        } else {
            &self.endpoints[index].live
        };
        live.snapshot.as_deref().is_some_and(|snapshot| {
            snapshot.agents.iter().any(|agent| {
                agent.pane_id == pane_id
                    && session_health(&agent.state_labels) == SessionHealth::Crashed
            })
        })
    }

    /// Whether the agent in `pane_id`, recorded on `host`, is mid-turn, so a
    /// wake must wait rather than be pasted into an input the program will
    /// clear when it finishes. Scoped to the host's own endpoint for the
    /// same reason as [`Self::team_session_dead`].
    pub(crate) fn team_pane_busy(&self, host: &str, pane_id: &str) -> bool {
        let Some(index) = self.endpoint_for_host(host) else {
            return false;
        };
        let live = if index == self.selected_endpoint {
            &self.live
        } else {
            &self.endpoints[index].live
        };
        live.snapshot.as_deref().is_some_and(|snapshot| {
            snapshot.agents.iter().any(|agent| {
                agent.pane_id == pane_id
                    && matches!(
                        agent.agent_status,
                        sheprd_client::protocol::AgentStatus::Working
                            | sheprd_client::protocol::AgentStatus::Blocked
                    )
            })
        })
    }

    pub(super) fn team_candidates(&self) -> Vec<PaneCandidate> {
        let mut candidates = Vec::new();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            let Some(snapshot) = live.snapshot.as_deref() else {
                continue;
            };
            if !live.status.is_connected() || snapshot.boot_id.is_empty() {
                continue;
            }
            let host_prefix = if index != self.selected_endpoint {
                format!("{}: ", endpoint.label)
            } else {
                String::new()
            };
            for pane in &snapshot.panes {
                let Some((session, agent_kind)) =
                    crate::control::server::team::prove_pane_session_at(self, index, &pane.pane_id)
                else {
                    continue;
                };
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
                    format!("{host_prefix}{base_label}")
                } else {
                    format!("{host_prefix}{base_label} ({agent_kind})")
                };
                let status = if let Some(agent) = agent_entry {
                    match session_health(&agent.state_labels) {
                        SessionHealth::Alive => status_label(agent.agent_status),
                        SessionHealth::RateLimited => fl!("team-status-pto").into(),
                        // The same crash vocabulary the heartbeat
                        // inspection reports, localized here.
                        SessionHealth::Crashed => fl!("team-status-crashed").into(),
                    }
                } else {
                    fl!("team-status-live").into()
                };
                candidates.push(PaneCandidate {
                    identity: PaneIdentity {
                        daemon_id: snapshot.boot_id.clone(),
                        session_id: Some(session),
                        workspace_id: pane.workspace_id.clone(),
                        pane_id: pane.pane_id.clone(),
                    },
                    label: label.into(),
                    agent: agent_kind.into(),
                    status,
                });
            }
        }
        candidates
    }

    /// Runs on each window tick. The wake queue drains on every tick even
    /// before the Team panel is first shown (#62): after a GUI restart, held
    /// wakes must retry without a human opening the board first. Everything
    /// else - candidates, actions, the shown team's refresh - waits until the
    /// panel is shown or a prior tick already started the bridge.
    pub(crate) fn poll_team(&mut self, cx: &mut Context<Self>) {
        self.dispatch_drained_team_prompts();
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
        let prompts = bridge.take_released_prompts();
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
                }
            }
        }
        // A held wake is retried every tick, not only when an action happens
        // to run: the pause can clear from another window or when a session
        // returns. The queue keeps it until its pane is unpaused.
        if shown
            && let Some(team_id) = bridge.selected.clone()
            && bridge.last_refresh.is_none_or(|at| at.elapsed() >= REFRESH)
        {
            bridge.refresh(&team_id);
        }
        // The bounded periodic supervisor check: only the shown team, only
        // unseen results, at most one attempt per SUPERVISOR_FALLBACK.
        if shown && let Some(wake) = self.team_bridge.supervisor_fallback_due(Instant::now()) {
            let SupervisorWake {
                team_id,
                host,
                pane_id,
                prompt,
                summary,
                signals,
            } = wake;
            let dispatch = PromptDispatch {
                host,
                pane_id,
                prompt,
                summary,
            };
            let outcome = self.enqueue_or_dispatch_team_prompt(
                Some(team_id.as_str()),
                dispatch,
                signals.clone(),
            );
            if matches!(outcome, PromptOutcome::Sent) {
                self.team_bridge.mark_supervisor_woken(&team_id, signals);
            }
        }
        for agent in spawn_agents {
            self.start_chat_or_flash(agent, cx);
        }
        if split_pane {
            self.split_team_pane(cx);
        }
        let current_team = self.team_bridge.current_team_id().map(ToOwned::to_owned);
        for dispatch in prompts {
            self.enqueue_or_dispatch_team_prompt(current_team.as_deref(), dispatch, Vec::new());
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

    /// Dispatches prompts whose pause cleared since the last tick. A wake is
    /// recorded only when it actually reaches the agent; a prompt held again
    /// stays queued, and one the pane could not take is requeued for a later
    /// tick instead of being swallowed. A wake whose pane is gone from both
    /// the roster and every connected snapshot is marked undeliverable once
    /// its retry waits out, instead of being re-offered forever.
    pub(crate) fn dispatch_drained_team_prompts(&mut self) {
        // A drain with no connected daemon has no snapshot to prove a pane
        // against, so it would fail every delivery. Keep the queue. One host
        // being down must not stall the rest: the drain runs whenever any
        // endpoint is connected, and the per-member host resolution decides
        // what each wake can reach - an unreachable host is held with the
        // regular backoff instead of waiting for the selected endpoint
        // (finding 736a1b22).
        let any_endpoint_provable = self.endpoints.iter().enumerate().any(|(index, endpoint)| {
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            live.status.is_connected() && live.snapshot.is_some()
        });
        if !any_endpoint_provable {
            return;
        }
        // Panes the connected daemons currently show, the same proof the
        // dispatch uses: a queued wake for a pane outside this set and the
        // roster has nowhere to land.
        let live_panes: HashSet<String> = self
            .endpoints
            .iter()
            .enumerate()
            .filter(|(index, endpoint)| {
                let live = if *index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                live.status.is_connected()
            })
            .filter_map(|(_, endpoint)| endpoint.live.snapshot.as_deref())
            .filter(|snapshot| !snapshot.boot_id.is_empty())
            .flat_map(|snapshot| snapshot.panes.iter().map(|pane| pane.pane_id.clone()))
            .collect();
        let expired = self.team_bridge.expire_unreachable_prompts(&live_panes);
        if !expired.is_empty() {
            let dropped: Vec<String> = expired.iter().map(|(_, pane)| pane.clone()).collect();
            tracing::info!(
                panes = %dropped.join(", "),
                outcome = "undeliverable",
                "team wake drain marked wakes undeliverable: no agent in the panes"
            );
            self.team_bridge.last_activity =
                Some(format!("Marked undeliverable: no agent in {}", dropped.join(", ")).into());
            self.team_bridge.dirty = true;
        }
        self.reconcile_needs_reregister(&expired, &live_panes);
        let drained = self.team_bridge.drain_unpaused_prompts();
        let mut unreachable: Vec<(String, String)> = Vec::new();
        for item in drained {
            // A wake pasted while the agent is mid-turn is cleared when the
            // turn ends, so keep it queued until the pane is idle. The drain
            // retries it on a later tick.
            if self.team_pane_busy(&item.host, &item.pane_id) {
                tracing::info!(
                    team = %item.team_id,
                    host = %item.host,
                    pane = %item.pane_id,
                    outcome = "requeued_busy",
                    "team wake drain requeued the wake: the pane is busy"
                );
                self.team_bridge
                    .requeue_prompt(item, Instant::now() + WAKE_RETRY);
                continue;
            }
            let outcome = self.enqueue_or_dispatch_team_prompt(
                None,
                PromptDispatch {
                    host: item.host.clone(),
                    pane_id: item.pane_id.clone(),
                    prompt: item.prompt.clone(),
                    summary: item.summary.clone(),
                },
                item.signals.clone(),
            );
            tracing::info!(
                team = %item.team_id,
                pane = %item.pane_id,
                ?outcome,
                signals = item.signals.len(),
                "team wake drain delivered a held wake"
            );
            match outcome {
                PromptOutcome::Sent => {
                    if !item.signals.is_empty()
                        && let Some(team_id) =
                            self.team_bridge.current_team_id().map(ToOwned::to_owned)
                    {
                        self.team_bridge
                            .mark_supervisor_woken(&team_id, item.signals);
                    }
                }
                // Held prompts re-enqueued themselves inside the dispatch,
                // backed off by WAKE_RETRY. An unreachable one did too, and
                // is classified below: its host may be connected with the
                // pane gone, which is a needs-re-register episode, not a
                // silent retry loop.
                PromptOutcome::Held => {}
                PromptOutcome::Unreachable => {
                    unreachable.push((item.host, item.pane_id));
                }
                PromptOutcome::Unavailable | PromptOutcome::Failed => {
                    self.team_bridge
                        .requeue_prompt(item, Instant::now() + WAKE_RETRY);
                }
            }
        }
        self.reconcile_needs_reregister(&unreachable, &live_panes);
    }

    pub(crate) fn enqueue_or_dispatch_team_prompt(
        &mut self,
        team_id: Option<&str>,
        dispatch: PromptDispatch,
        signals: Vec<String>,
    ) -> PromptOutcome {
        let team_id = team_id
            .map(ToOwned::to_owned)
            .or_else(|| self.team_bridge.current_team_id().map(ToOwned::to_owned))
            .unwrap_or_default();
        let pane_id = dispatch.pane_id.clone();
        // A wake pasted into a busy agent is cleared when its turn ends, so
        // hold it until the pane is idle. The drain retries it on a later
        // tick, which is also how an assignment survives a busy worker.
        if self
            .team_bridge
            .is_pane_paused_for_team(&team_id, &dispatch.pane_id)
            || self.team_pane_busy(&dispatch.host, &dispatch.pane_id)
        {
            tracing::info!(
                team = %team_id,
                host = %dispatch.host,
                pane = %pane_id,
                outcome = "held",
                "team wake held: the pane is paused for the team or the agent is mid-turn"
            );
            self.team_bridge.hold_prompt(
                team_id,
                dispatch.host,
                dispatch.pane_id,
                dispatch.prompt,
                dispatch.summary,
                signals,
            );
            let held = self.team_bridge.queued_count();
            self.team_bridge.last_activity =
                Some(format!("Held prompt for {pane_id} ({held} queued)").into());
            self.team_bridge.dirty = true;
            return PromptOutcome::Held;
        }
        let outcome =
            self.dispatch_team_prompt(&dispatch.host, &dispatch.pane_id, dispatch.prompt.clone());
        // The dispatch can discover a busy pane the pre-check missed, or no
        // reachable agent at all; keep the wake in the team queue either way,
        // backed off by WAKE_RETRY so an unavailable pane cannot hot-loop the
        // drain - never in the notes queue where its hold expires into a
        // lost paste. An unreachable wake waits out its retry once and is
        // then marked undeliverable if the pane is still gone.
        if matches!(outcome, PromptOutcome::Held | PromptOutcome::Unreachable) {
            self.team_bridge.hold_prompt(
                team_id,
                dispatch.host,
                dispatch.pane_id,
                dispatch.prompt,
                dispatch.summary,
                signals,
            );
            let held = self.team_bridge.queued_count();
            self.team_bridge.last_activity =
                Some(format!("Held prompt for {pane_id} ({held} queued)").into());
            self.team_bridge.dirty = true;
            return outcome;
        }
        let summary = dispatch.summary;
        self.team_bridge.last_activity = Some(
            match outcome {
                PromptOutcome::Sent => summary.clone(),
                PromptOutcome::Held | PromptOutcome::Unreachable => summary.clone(),
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

    /// Resolves the endpoint that hosts `pane_id` for a member recorded on
    /// `host`: the connected endpoint whose daemon boot id is the recorded
    /// host and whose snapshot still shows the pane. `None` when the host is
    /// not connected or no longer shows the pane: the wake is unreachable,
    /// and it never falls back to the selected endpoint, where the same pane
    /// id may name an unrelated agent on another machine (#40).
    pub(crate) fn target_endpoint_for_member(&self, host: &str, pane_id: &str) -> Option<usize> {
        let index = self.endpoint_for_host(host)?;
        let live = if index == self.selected_endpoint {
            &self.live
        } else {
            &self.endpoints[index].live
        };
        live.snapshot
            .as_deref()
            .is_some_and(|snapshot| snapshot.panes.iter().any(|pane| pane.pane_id == pane_id))
            .then_some(index)
    }

    fn dispatch_team_prompt(&mut self, host: &str, pane_id: &str, prompt: String) -> PromptOutcome {
        // ACP chats are window-local, so only a member recorded on this
        // window's own daemon may be prompted through one: on another host
        // the same pane id can name a different agent entirely (#40).
        let local_id = self
            .endpoints
            .get(self.selected_endpoint)
            .map(|endpoint| endpoint.id.clone());
        if local_id.as_deref() == Some(host)
            && let Some(chat) = self.chats.pane_mut(pane_id)
            && chat.target.existing.is_none()
            && !chat.session.transcript().read_only
        {
            return chat
                .send(sheprd_chat_protocol::Command::Prompt {
                    text: prompt,
                    attachments: Vec::new(),
                })
                .map_or(PromptOutcome::Failed, |_| PromptOutcome::Sent);
        }
        if self.endpoints.is_empty() {
            return PromptOutcome::Unavailable;
        }
        // Team wakes are typed only into an idle agent; a busy one comes back
        // as Held so the team queue retries it, instead of the generic notes
        // queue holding it for 120s and then typing into a mid-turn program.
        // A pane no snapshot proves comes back as Unreachable so the drain
        // backs off and expires it instead of re-offering it every tick.
        match self.deliver_team_prompt(host, pane_id, prompt) {
            crate::agent_notes::Delivered::Agent => PromptOutcome::Sent,
            crate::agent_notes::Delivered::Kept => PromptOutcome::Held,
            crate::agent_notes::Delivered::Unreachable => PromptOutcome::Unreachable,
            crate::agent_notes::Delivered::Copy(_) => PromptOutcome::Unavailable,
        }
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
                sheprd_client::Method::PaneSplit,
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
        .unwrap_or_else(|| "herding".to_owned());
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
) -> Vec<PromptDispatch> {
    match action {
        TeamAction::CreateTeam { name, supervisor } if !name.trim().is_empty() => {
            let cli = team_cli_prefix(&supervisor.pane_id, Some(&supervisor.workspace_id));
            vec![PromptDispatch {
                host: supervisor.daemon_id.clone(),
                pane_id: supervisor.pane_id.clone(),
                prompt: format!(
                    "[Herdr Team] You are the Supervisor of team \"{}\". Strictly supervise: decompose the goal into tasks, assign tasks to workers, review findings, and coordinate via the bulletin rather than doing implementation work yourself. Load skill \"herdr-team\" for workflow and workspace rules. Run `{cli} status` to get your TEAM_ID, `{cli} roster <TEAM_ID>` to inspect workers, `{cli} task list <TEAM_ID>` to manage tasks, and `{cli} bulletin list <TEAM_ID>` to read messages (run `{cli} bulletin ack <TEAM_ID> <MESSAGE_ID>` to acknowledge).",
                    name.trim()
                ),
                summary: format!("Started team \"{}\" with supervisor {}", name.trim(), supervisor.pane_id),
            }]
        }
        TeamAction::SetGoal { team_id, goal } if !goal.trim().is_empty() => team
            .and_then(supervisor_target)
            .into_iter()
            .map(|(host, pane_id)| {
                let cli = cli_for_member(team, &pane_id);
                PromptDispatch {
                    host,
                    pane_id: pane_id.clone(),
                    prompt: format!(
                        "[Herdr Team] Team goal set for {team_id}: \"{}\". As Supervisor, do not implement everything yourself — break this goal into concrete tasks with `{cli} task create {team_id} --title \"...\" --assign <worker>`, delegate to workers, and coordinate via `{cli} bulletin post {team_id} \"...\"`.",
                        goal.trim()
                    ),
                    summary: format!("Sent updated goal to supervisor ({pane_id})"),
                }
            })
            .collect(),
        TeamAction::AddWorker {
            team_id,
            name,
            pane,
        } if !name.trim().is_empty() => {
            let cli = team_cli_prefix(&pane.pane_id, Some(&pane.workspace_id));
            vec![PromptDispatch {
                host: pane.daemon_id.clone(),
                pane_id: pane.pane_id.clone(),
                prompt: format!(
                    "[Herdr Team] You joined team {team_id} as Worker \"{}\". Load skill \"herdr-team\" for workflow and workspace rules. Run `{cli} status {team_id}` to view the goal, `{cli} task list {team_id}` to claim open tasks (`{cli} task claim {team_id} <TASK_ID>`), and `{cli} bulletin list {team_id}` to read messages (run `{cli} bulletin ack {team_id} <MESSAGE_ID>` to acknowledge).",
                    name.trim()
                ),
                summary: format!("Added worker \"{}\" ({})", name.trim(), pane.pane_id),
            }]
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
                    Some(PromptDispatch {
                        host: m.target.host.clone(),
                        pane_id: pane_id.clone(),
                        prompt: format!(
                            "[Herdr Team] Status check for {} in team {team_id}: please run `{cli} status {team_id}`, `{cli} task list {team_id}`, and `{cli} bulletin list {team_id}` to check active tasks and acknowledge messages with `{cli} bulletin ack {team_id} <MESSAGE_ID>`.",
                            m.display_name
                        ),
                        summary: format!("Nudged {} ({pane_id})", m.display_name),
                    })
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
                    Some(PromptDispatch {
                        host: m.target.host.clone(),
                        pane_id: pane_id.clone(),
                        prompt: format!(
                            "[Herdr Team] You ({}) have been promoted to Supervisor of team \"{}\" ({team_id}). Strictly supervise: delegate tasks to workers, review their findings, and coordinate via the bulletin rather than implementing tasks yourself. Load skill \"herdr-team\" for workflow and workspace rules. Run `{cli} status {team_id}`, `{cli} roster {team_id}`, `{cli} task list {team_id}`, and `{cli} bulletin list {team_id}` (run `{cli} bulletin ack {team_id} <MESSAGE_ID>` to acknowledge).",
                            m.display_name, team.name
                        ),
                        summary: format!("Promoted {} to supervisor ({pane_id})", m.display_name),
                    })
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
            let target = team.and_then(|team| {
                resolve_assignee_target(team, assignee.as_deref())
                    .or_else(|| supervisor_target(team))
            });
            let brief_part = if brief.trim().is_empty() {
                String::new()
            } else {
                format!(" — {}", brief.trim())
            };
            target
                .into_iter()
                .map(|(host, pane_id)| {
                    let cli = cli_for_member(team, &pane_id);
                    PromptDispatch {
                        host,
                        pane_id: pane_id.clone(),
                        prompt: format!(
                            "[Herdr Team] New task in team {team_id}: \"{}\"{}. Run `{cli} task list {team_id}` to inspect it, `{cli} task claim {team_id} <TASK_ID>` to start working on it, and `{cli} task complete {team_id} <TASK_ID> --result \"...\"` when finished.",
                            title.trim(),
                            brief_part
                        ),
                        summary: format!("Dispatched task \"{}\" to {pane_id}", title.trim()),
                    }
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
            let target =
                team.and_then(|team| resolve_assignee_target(team, assignee.as_deref()));
            let brief_part = if brief.trim().is_empty() {
                String::new()
            } else {
                format!(" — {}", brief.trim())
            };
            target
                .into_iter()
                .map(|(host, pane_id)| {
                    let cli = cli_for_member(team, &pane_id);
                    PromptDispatch {
                        host,
                        pane_id: pane_id.clone(),
                        prompt: format!(
                            "[Herdr Team] Task {task_id} assigned to you in team {team_id}: \"{}\"{}. Claim it with `{cli} task claim {team_id} {task_id}` and complete it with `{cli} task complete {team_id} {task_id} --result \"...\"`.",
                            title.trim(),
                            brief_part
                        ),
                        summary: format!("Assigned task \"{}\" to {pane_id}", title.trim()),
                    }
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
            let target = task
                .assignee
                .as_deref()
                .and_then(|id| resolve_assignee_target(team, Some(id)))
                .or_else(|| supervisor_target(team));
            let brief_part = if task.brief.trim().is_empty() {
                String::new()
            } else {
                format!(" — {}", task.brief.trim())
            };
            target
                .into_iter()
                .map(|(host, pane_id)| {
                    let cli = cli_for_member(Some(team), &pane_id);
                    PromptDispatch {
                        host,
                        pane_id: pane_id.clone(),
                        prompt: format!(
                            "[Herdr Team] Please work on task {} (\"{}\"){} in team {team_id}. Claim it with `{cli} task claim {team_id} {}` and complete it with `{cli} task complete {team_id} {} --result \"...\"`.",
                            task.task_id, task.title, brief_part, task.task_id, task.task_id
                        ),
                        summary: format!("Sent task \"{}\" to {pane_id}", task.title),
                    }
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
            let recipients = match sheprd_orchestrator::team_store::resolve_recipients(
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
                    sheprd_orchestrator::team_store::Recipients::All => match author {
                        sheprd_orchestrator::team_store::Author::Member { member_id } => {
                            m.member_id != *member_id
                        }
                        sheprd_orchestrator::team_store::Author::Human { .. } => true,
                    },
                    sheprd_orchestrator::team_store::Recipients::Explicit(ids) => {
                        ids.contains(&m.member_id)
                    }
                })
                .filter_map(|m| {
                    let pane_id = m.target.pane.clone()?;
                    let cli = team_cli_prefix(&pane_id, m.target.workspace.as_deref());
                    Some(PromptDispatch {
                        host: m.target.host.clone(),
                        pane_id: pane_id.clone(),
                        prompt: format!(
                            "[Herdr Team] New bulletin message in team {team_id}: \"{}\". Run `{cli} bulletin list {team_id}` to view, `{cli} bulletin ack {team_id} <MESSAGE_ID>` to acknowledge, and `{cli} bulletin post {team_id} \"...\"` to reply.",
                            body.trim()
                        ),
                        summary: format!("Posted bulletin to team ({pane_id})"),
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// What the agent's own state labels say about its session, classified once
/// so the roster badge and the dispatch dead-session hatch agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionHealth {
    Alive,
    RateLimited,
    Crashed,
}

/// Classifies an agent's state labels: rate-limit vocabulary is the PTO
/// state, process-exit or connection-closed vocabulary is a dead session.
pub(crate) fn session_health(state_labels: &[(String, String)]) -> SessionHealth {
    for (key, value) in state_labels {
        if key != "working" && key != "idle" && key != "blocked" {
            continue;
        }
        let lower = value.to_lowercase();
        if lower.contains("429")
            || lower.contains("rate limit")
            || lower.contains("quota")
            || lower.contains("usage")
        {
            return SessionHealth::RateLimited;
        }
        if lower.contains("process exited with status") || lower.contains("connection closed") {
            return SessionHealth::Crashed;
        }
    }
    SessionHealth::Alive
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
    /// No reachable agent in the pane: no snapshot shows it, or no agent sits
    /// in it. Queued with a WAKE_RETRY backoff, then marked undeliverable if
    /// the pane is still gone when the retry comes due.
    Unreachable,
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

/// The member an assignee reference resolves to, as the `(host, pane)` a
/// wake must be routed by: the host picks the daemon, the pane the agent
/// inside it (#40).
fn resolve_assignee_target(team: &Team, assignee: Option<&str>) -> Option<(String, String)> {
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
        .and_then(|m| {
            let pane = m.target.pane.clone()?;
            Some((m.target.host.clone(), pane))
        })
}

fn supervisor_target(team: &Team) -> Option<(String, String)> {
    let pane = team.supervisor.target.pane.clone()?;
    Some((team.supervisor.target.host.clone(), pane))
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "wake_land_tests.rs"]
mod wake_land_tests;
