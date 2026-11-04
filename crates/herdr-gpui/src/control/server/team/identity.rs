//! Caller identity for team operations: which session a request is proved
//! against, derived from the pane the caller names. The window may be
//! showing another host, so the snapshot comes from the caller's endpoint,
//! not the selected one.
use crate::control::protocol::{ErrorCode, Response};
use crate::{HerdrWindow, state::LiveState};
use herdr_client::protocol::AgentStatus;
use herdr_orchestrator::team_store::SessionTarget;
use std::collections::HashMap;

/// Resolves the proven session identity and agent kind for `pane_id` in the
/// window's selected daemon.
pub(crate) fn prove_pane_session(view: &HerdrWindow, pane_id: &str) -> Option<(String, String)> {
    prove_pane_session_in(view, view.selected_endpoint, &view.live, pane_id)
}

/// [`prove_pane_session`] against the endpoint at `index`. ACP transcripts and
/// CLI bindings are projected from the selected daemon alone, so they only
/// prove a pane on that daemon; every other host is proved by the agent
/// session its own snapshot reports.
pub(crate) fn prove_pane_session_at(
    view: &HerdrWindow,
    index: usize,
    pane_id: &str,
) -> Option<(String, String)> {
    let live = if index == view.selected_endpoint {
        &view.live
    } else {
        &view.endpoints.get(index)?.live
    };
    prove_pane_session_in(view, index, live, pane_id)
}

fn prove_pane_session_in(
    view: &HerdrWindow,
    index: usize,
    live: &LiveState,
    pane_id: &str,
) -> Option<(String, String)> {
    let snapshot = live.snapshot.as_deref()?;
    if !live.status.is_connected() || snapshot.boot_id.is_empty() {
        return None;
    }
    let pane = snapshot.panes.iter().find(|p| p.pane_id == pane_id)?;
    let agent_entry = snapshot.agents.iter().find(|a| a.pane_id == pane_id);
    let agent_label = agent_entry
        .and_then(|a| a.display_agent.as_deref().or(a.agent.as_deref()))
        .unwrap_or_default()
        .trim()
        .to_owned();

    // 1. Native attached ACP chat transcript
    if index == view.selected_endpoint
        && let Some(chat) = view.chats.pane(pane_id)
        && chat.target.existing.is_none()
        && chat.attached()
        && chat.target.pane_id == pane_id
        && let Some(session) = chat
            .session
            .transcript()
            .session
            .as_ref()
            .map(|s| s.id.as_str())
            .filter(|id| !id.is_empty())
    {
        let kind = if agent_label.is_empty() {
            "acp".to_owned()
        } else {
            agent_label
        };
        return Some((session.to_owned(), kind));
    }

    // 2. Verified existing CLI conversation binding (Codex / Claude process + session ID)
    if index == view.selected_endpoint
        && let Some(binding) = view
            .chats
            .existing
            .bindings()
            .iter()
            .find(|b| b.pane_id == pane_id)
    {
        let kind = if agent_label.is_empty() {
            binding.agent.name().to_owned()
        } else {
            agent_label
        };
        return Some((
            format!(
                "cli:{}:{}:{}",
                binding.session_id, binding.pid, binding.started_ticks
            ),
            kind,
        ));
    }

    // 3. Active terminal agent session reported by the daemon snapshot (ZCode, OpenCode, Codex, Claude, etc.)
    if let Some(agent) = agent_entry {
        let kind = agent
            .agent
            .as_deref()
            .or(agent.display_agent.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(kind) = kind
            && agent.agent_status != AgentStatus::Unknown
        {
            let label = if agent_label.is_empty() {
                kind.to_owned()
            } else {
                agent_label
            };
            return Some((
                format!(
                    "agent:{}:{}:{}",
                    kind.to_ascii_lowercase(),
                    pane.workspace_id,
                    pane.pane_id
                ),
                label,
            ));
        }
    }

    None
}

pub(super) fn pane_agents(view: &HerdrWindow, endpoint: usize) -> HashMap<String, String> {
    let Some(snapshot) = live_at(view, endpoint).and_then(|live| live.snapshot.as_deref()) else {
        return HashMap::new();
    };
    snapshot
        .panes
        .iter()
        .filter_map(|pane| {
            let (_, kind) = prove_pane_session_at(view, endpoint, &pane.pane_id)?;
            Some((pane.pane_id.clone(), kind))
        })
        .collect()
}

/// The live projection of the endpoint at `index`, which the selected endpoint
/// keeps as `view.live`.
pub(super) fn live_at(view: &HerdrWindow, index: usize) -> Option<&LiveState> {
    if index == view.selected_endpoint {
        Some(&view.live)
    } else {
        view.endpoints.get(index).map(|endpoint| &endpoint.live)
    }
}

pub(super) fn current_identity(
    view: &HerdrWindow,
    caller: &crate::control::protocol::Caller,
) -> Result<(SessionTarget, usize), Response> {
    let pane = caller
        .pane_id
        .as_deref()
        .ok_or_else(|| super::projection::invalid("Team operations require --pane"))?;
    let found = crate::control::server::work::find(view, caller)?;
    let live = live_at(view, found.index).ok_or_else(|| {
        Response::error(
            ErrorCode::Unsupported,
            "The caller's host is no longer shown by this window",
        )
    })?;
    if !live.status.is_connected() {
        return Err(Response::error(
            ErrorCode::Unsupported,
            "Team calls require the caller's connected daemon",
        ));
    }
    if caller.workspace_id.as_deref() != Some(found.workspace.workspace_id.as_str()) {
        return Err(super::projection::invalid(
            "Team calls require an exact caller workspace",
        ));
    }
    if !found.snapshot.panes.iter().any(|candidate| {
        candidate.pane_id == pane && candidate.workspace_id == found.workspace.workspace_id
    }) || found.snapshot.boot_id.is_empty()
    {
        return Err(Response::error(
            ErrorCode::Unsupported,
            "The caller pane does not match the current pane snapshot",
        ));
    }
    let (session, _) = prove_pane_session_at(view, found.index, pane).ok_or_else(|| {
        Response::error(
            ErrorCode::Unsupported,
            "No active agent session or attached ACP transcript proves this caller's pane",
        )
    })?;
    Ok((
        SessionTarget {
            host: found.snapshot.boot_id.clone(),
            session,
            workspace: Some(found.workspace.workspace_id.clone()),
            pane: Some(pane.to_owned()),
        },
        found.index,
    ))
}
