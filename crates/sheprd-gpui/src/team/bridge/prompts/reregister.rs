//! The needs-re-register ledger and its supervisor wake: expiry and
//! unreachable dispatches are never silent while the member's host is
//! connected (issue #40's follow-up on stable host identity).
use super::{PromptDispatch, PromptOutcome, team_cli_prefix};
use crate::SheprdWindow;
use crate::team::bridge::views::roster;
use std::collections::HashSet;

impl SheprdWindow {
    /// A wake that cannot land is never silent while the member's host is
    /// still connected: the pane vanished from a connected endpoint - the
    /// daemon restarted or the pane closed - so the supervisor is woken once
    /// per episode (a later restart brings a new boot, and with it a new
    /// wake) and the roster marks the member until their pane is proved
    /// again or they re-register. A host that is simply offline is not
    /// marked: nothing on that machine can answer, and the work stays on
    /// the board. Covers both ways a wake fails to land: expiry (the pane
    /// is gone from the roster too) and the dispatch's own `Unreachable`
    /// (the roster still names the pane, so the wake is retained and
    /// re-offered, which must not loop silently).
    pub(crate) fn reconcile_needs_reregister(
        &mut self,
        expired: &[(String, String)],
        live_panes: &HashSet<String>,
    ) {
        let Some(team) = self.team_bridge.team.clone() else {
            return;
        };
        let recovered: Vec<String> = roster(&team)
            .filter(|member| {
                self.team_bridge
                    .needs_reregister
                    .contains_key(&member.member_id)
            })
            .filter(|member| {
                member
                    .target
                    .pane
                    .as_deref()
                    .is_some_and(|pane| live_panes.contains(pane))
            })
            .map(|member| member.member_id.clone())
            .collect();
        for member_id in recovered {
            self.team_bridge.needs_reregister.remove(&member_id);
            self.team_bridge.dirty = true;
        }
        for (host, pane) in expired {
            let Some(index) = self.endpoint_for_host(host) else {
                continue;
            };
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &self.endpoints[index].live
            };
            let Some(boot) = live
                .snapshot
                .as_ref()
                .filter(|snapshot| !snapshot.boot_id.is_empty())
                .map(|snapshot| snapshot.boot_id.clone())
            else {
                continue;
            };
            let Some(member) = roster(&team).find(|member| {
                member.target.host == *host && member.target.pane.as_deref() == Some(pane)
            }) else {
                continue;
            };
            self.team_bridge
                .needs_reregister
                .insert(member.member_id.clone(), boot.clone());
            self.team_bridge.dirty = true;
            let signal = format!("reregister|{}|{}|{}", team.team_id, member.member_id, boot);
            if self.team_bridge.wake_seen(&team.team_id, &signal) {
                continue;
            }
            let Some(supervisor_pane) = team.supervisor.target.pane.clone() else {
                continue;
            };
            let cli = team_cli_prefix(
                &supervisor_pane,
                team.supervisor.target.workspace.as_deref(),
            );
            let team_id = &team.team_id;
            let prompt = format!(
                "[Herdr Team] Member {} ({pane}) needs re-registering: their recorded host \
                 {host} is connected but that pane is gone (the daemon restarted or the pane \
                 closed). From the member's pane run `{cli} register {team_id} --name {}`; if \
                 the pane id changed too, remove and re-add them. `{cli} roster {team_id}` shows \
                 who is marked.",
                member.display_name, member.display_name,
            );
            let outcome = self.enqueue_or_dispatch_team_prompt(
                None,
                PromptDispatch {
                    host: team.supervisor.target.host.clone(),
                    pane_id: supervisor_pane,
                    prompt,
                    summary: format!(
                        "Woke supervisor: {} needs re-registering",
                        member.display_name
                    ),
                },
                vec![signal.clone()],
            );
            if matches!(outcome, PromptOutcome::Sent) {
                self.team_bridge
                    .mark_supervisor_woken(&team.team_id, vec![signal]);
            }
        }
    }
}
