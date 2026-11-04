//! Caller identity for team operations: which session a request is proved
//! against, derived from the pane the caller names. The window may be
//! showing another host, so the snapshot comes from the caller's endpoint,
//! not the selected one.
use crate::control::protocol::{ErrorCode, Response};
use crate::{HerdrWindow, state::LiveState};
use herdr_client::protocol::AgentStatus;
use herdr_control_protocol::socket::PeerCredentials;
use herdr_orchestrator::team_store::SessionTarget;
use std::collections::HashMap;

/// Resolves the proven session identity and agent kind for `pane_id` in the
/// window's selected daemon.
/// Kept for the live remote re-verification after the fleet rollout rebuilds
/// the hosts and their panes re-register; unused until then.
#[allow(dead_code)]
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

/// The pane's agent process a member registered from, keyed by the session
/// target the member is proved by. `None` records a pane whose agent
/// process could not be pinned (no discovered binding, or a platform
/// without a process table walk): its callers pass on the user identity
/// alone. In memory only: a UI restart clears it, and a member registers
/// again.
pub(crate) type PeerBindings = HashMap<SessionTarget, Option<ProcessIdentity>>;

/// A process, named by its pid and the kernel's start time for it, so a
/// recycled pid is never mistaken for the original.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProcessIdentity {
    pub pid: u32,
    pub started_ticks: Option<u64>,
}

/// Whether a proven caller holds the process identity its member registered
/// with, on top of the snapshot proof [`current_identity`] ran. Same-user
/// callers are matched against the binding their registration recorded;
/// root-peer callers arrive through the control tunnel, whose forwarded
/// connection terminates at sshd, so their credentials name the tunnel and
/// the snapshot proofs apply as documented in `docs/control-tunnel.md`;
/// anything else is refused, though the mode-0600 socket makes it
/// unreachable in practice.
pub(crate) fn check_peer(
    peers: &PeerBindings,
    proven: &SessionTarget,
    peer: Option<PeerCredentials>,
    peer_auth: bool,
    local_uid: Option<u32>,
    process_info: &dyn Fn(u32) -> Option<(u32, u64)>,
) -> Result<(), Response> {
    // The Windows pipe cannot prove a process identity; its DACL keeps it
    // to the one user, the same trust the socket model documents.
    if !peer_auth {
        return Ok(());
    }
    let Some(cred) = peer else {
        return Err(Response::error(
            ErrorCode::Unsupported,
            "The connecting process's credentials could not be read; team calls need a Unix socket",
        ));
    };
    if Some(cred.uid) != local_uid {
        if cred.uid == 0 {
            // The tunnel's sshd forwards the request; the remote agent is
            // proved by its pane snapshot, as the tunnel docs record.
            return Ok(());
        }
        return Err(Response::error(
            ErrorCode::Unsupported,
            "Team calls come from your own user's processes",
        ));
    }
    match peers.get(proven) {
        // The pane's agent process could not be pinned at registration, so
        // the user identity is all this member binds to.
        None | Some(None) => Ok(()),
        Some(Some(recorded)) => {
            // Every CLI call is a fresh process, so the caller passes when
            // it is the registered agent process or one of its descendants:
            // a bystander naming the pane is neither. The bound walk stops
            // at init; the start-time comparison defeats a recycled pid.
            let mut current = cred.pid;
            for _ in 0..48 {
                let Some(pid) = current else {
                    return Err(process_refused());
                };
                if pid == recorded.pid {
                    let still_same = recorded.started_ticks.is_none_or(|recorded_ticks| {
                        process_info(recorded.pid)
                            .is_some_and(|(_, started)| started == recorded_ticks)
                    });
                    if !still_same {
                        return Err(process_refused());
                    }
                    return Ok(());
                }
                match process_info(pid) {
                    Some((ppid, _)) if ppid > 1 => current = Some(ppid),
                    _ => return Err(process_refused()),
                }
            }
            Err(process_refused())
        }
    }
}

fn process_refused() -> Response {
    Response::error(
        ErrorCode::Unsupported,
        "This caller does not descend from the pane's registered agent process; run the team register command from the agent's own pane",
    )
}

/// The process table facts of `pid`: its parent and its kernel start time.
/// `None` when the process is gone or not ours to read.
#[cfg(target_os = "linux")]
pub(crate) fn process_info(pid: u32) -> Option<(u32, u64)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The comm field may hold spaces, so parsing starts after its `)`.
    let rest = &stat[stat.rfind(')')? + 2..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After the comm, field 0 is the state and field 1 the ppid; the start
    // time is field 19 (kernel field 22, counting pid and comm).
    let ppid: u32 = fields.get(1)?.parse().ok()?;
    let started: u64 = fields.get(19)?.parse().ok()?;
    Some((ppid, started))
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn process_info(pid: u32) -> Option<(u32, u64)> {
    let _ = pid;
    None
}

/// `/proc/PID/cmdline`, arguments joined into one lowercased line.
#[cfg(target_os = "linux")]
fn proc_cmdline(pid: u32) -> Option<String> {
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let line = String::from_utf8_lossy(&bytes).replace('\0', " ");
    (!line.trim().is_empty()).then(|| line.to_lowercase())
}

/// How far an ancestry walk follows parents before giving up; deep enough
/// for any agent nesting, shallow enough to fail fast.
const LINEAGE_CAP: usize = 16;

/// Anchors the agent process for a registering CLI on Linux: walks the
/// caller's ancestry and returns the first ancestor whose command line
/// carries the agent `kind`. Shells in between carry their own command
/// line, not the agent's name, so the first match is the agent; the start
/// time recorded alongside excludes a reused pid. This is the fallback when
/// the UI's discovered process bindings name no process for the pane.
#[cfg(target_os = "linux")]
pub(crate) fn agent_lineage(pid: u32, kind: &str) -> Option<ProcessIdentity> {
    let kind = kind.to_lowercase();
    let mut current = pid;
    for _ in 0..LINEAGE_CAP {
        if let Some(line) = proc_cmdline(current)
            && line.contains(&kind)
            && let Some((_, started)) = process_info(current)
        {
            return Some(ProcessIdentity {
                pid: current,
                started_ticks: Some(started),
            });
        }
        let (parent, _) = process_info(current)?;
        if parent <= 1 {
            return None;
        }
        current = parent;
    }
    None
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn agent_lineage(pid: u32, kind: &str) -> Option<ProcessIdentity> {
    let _ = (pid, kind);
    None
}
