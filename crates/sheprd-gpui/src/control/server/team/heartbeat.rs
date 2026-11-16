//! Heartbeat inspection logic for agent pane terminals and wire serialization.
use super::*;
use crate::control::protocol::team::Reply as WireReply;
use sheprd_orchestrator::team_store::{Member, Team};
use std::collections::HashMap;

pub(super) fn heartbeat_response(
    team: &Team,
    target_member_id: Option<&str>,
    pane_agents: &HashMap<String, String>,
) -> Response {
    let team_id = team.team_id.clone();
    let all_members: Vec<&Member> = std::iter::once(&team.supervisor)
        .chain(team.members.iter())
        .filter(|m| target_member_id.is_none_or(|target| m.member_id == target))
        .collect();

    let heartbeat_members: Vec<team::HeartbeatInfo> = all_members
        .into_iter()
        .map(|m| {
            let agent_entry = m
                .target
                .pane
                .as_ref()
                .and_then(|pane_id| pane_agents.get(pane_id));
            let (status, role, agent_kind) = match agent_entry {
                Some(a) => ("live".to_string(), "worker", a.as_str()),
                None => (
                    "unknown".to_string(),
                    if m.member_id == team.supervisor.member_id {
                        "supervisor"
                    } else {
                        "worker"
                    },
                    "cli",
                ),
            };

            // Inspect terminal scrollback or recent status if pane is reachable
            let (health, details) = match &m.target.pane {
                Some(pane_id) => inspect_pane_health(pane_id),
                None => (
                    "DISCONNECTED".to_string(),
                    "No attached terminal pane".to_string(),
                ),
            };

            team::HeartbeatInfo {
                member_id: m.member_id.clone(),
                name: m.display_name.clone(),
                role: role.to_string(),
                agent: agent_kind.to_string(),
                pane_id: m.target.pane.clone(),
                status,
                health,
                details,
                rtt_ms: None,
            }
        })
        .collect();

    Response::Team {
        reply: WireReply::Heartbeat {
            team_id,
            members: heartbeat_members,
        },
    }
}

/// Scan recent terminal scrollback of a pane using herdr CLI for rate limits or crashes.
pub(crate) fn inspect_pane_health(pane_id: &str) -> (String, String) {
    let output = sheprd_client::new_command("herdr")
        .args(["pane", "read", pane_id, "--lines", "30"])
        .output();

    let text = match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => {
            return (
                "UNKNOWN".to_string(),
                "Could not read terminal output".to_string(),
            );
        }
    };

    let lower = text.to_lowercase();
    if lower.contains("429 too many requests") {
        ("PTO".to_string(), "HTTP 429 Too Many Requests".to_string())
    } else if lower.contains("resource_exhausted") {
        (
            "PTO".to_string(),
            "API Resource Exhausted (429)".to_string(),
        )
    } else if lower.contains("usage limit reached") || lower.contains("reached your usage limit") {
        (
            "PTO".to_string(),
            "Provider usage limit reached".to_string(),
        )
    } else if lower.contains("rate limit") {
        ("PTO".to_string(), "Rate limit exceeded".to_string())
    } else if lower.contains("insufficient_quota") || lower.contains("quota exceeded") {
        ("PTO".to_string(), "API Quota Exceeded".to_string())
    } else if lower.contains("out of usage") {
        ("PTO".to_string(), "Worker out of usage".to_string())
    } else if lower.contains("context window exhausted") {
        ("PTO".to_string(), "Context window exhausted".to_string())
    } else if lower.contains("process exited with status") || lower.contains("connection closed") {
        (
            "CRASHED".to_string(),
            "Process or session terminated".to_string(),
        )
    } else {
        ("HEALTHY".to_string(), "Active and responsive".to_string())
    }
}
