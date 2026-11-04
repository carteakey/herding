//! Prompts the bridge holds back: ones waiting for a pane to be unpaused, and
//! ones waiting for the store to confirm the write that produced them. No
//! prompt dispatches in the tick its action was queued, so an agent woken with
//! "claim this task" or "ack this post" finds the row when it runs the command
//! it was told to run.
use super::{Bridge, QueuedPrompt, roster};
use std::collections::HashSet;

/// A prompt to dispatch: the pane to wake, the prompt text, and the one-line
/// summary the panel shows.
pub(super) type PromptDispatch = (String, String, String);

/// Prompts waiting for the reply that confirms the operation that produced
/// them, oldest first so a worker that never answers drops them.
pub(super) type HeldPrompts = std::collections::VecDeque<(u64, Vec<PromptDispatch>)>;

impl Bridge {
    pub(super) fn enqueue_prompt(
        &mut self,
        team_id: String,
        pane_id: String,
        prompt: String,
        summary: String,
    ) {
        if self.queued_prompts.len() >= super::PAUSE_QUEUE_CAPACITY {
            self.queued_prompts.remove(0);
        }
        self.queued_prompts.push(QueuedPrompt {
            team_id,
            pane_id,
            prompt,
            summary,
        });
    }

    pub(super) fn queued_count(&self) -> usize {
        match self.current_team_id() {
            Some(team_id) => self
                .queued_prompts
                .iter()
                .filter(|p| p.team_id == team_id)
                .count(),
            None => self.queued_prompts.len(),
        }
    }

    /// Prompts queued for this team whose pane is no longer paused. A paused
    /// pane keeps its prompts; another team's keeps its own.
    pub(super) fn drain_unpaused_prompts(&mut self) -> Vec<PromptDispatch> {
        let Some(team) = self.team.as_ref() else {
            return Vec::new();
        };
        if self.is_team_paused() {
            return Vec::new();
        }
        let team_id = team.team_id.as_str();
        let paused_panes: HashSet<String> = roster(team)
            .filter(|m| self.paused_members.contains(&m.member_id))
            .filter_map(|m| m.target.pane.clone())
            .collect();
        let mut ready = Vec::new();
        let mut held = Vec::new();
        for item in self.queued_prompts.drain(..) {
            if item.team_id == team_id {
                if paused_panes.contains(&item.pane_id) {
                    held.push(item);
                } else {
                    ready.push((item.pane_id, item.prompt, item.summary));
                }
            } else {
                held.push(item);
            }
        }
        self.queued_prompts = held;
        ready
    }

    /// Split an action's prompts by whether the store has to confirm them. The
    /// prompts whose action queued an operation wait for that reply, so an agent
    /// is never told to claim or ack a row the store has yet to receive - its
    /// first CLI call would fail. Prompts for actions that queued nothing (a
    /// nudge is window-local) are returned for this tick: there is no write to
    /// wait behind. An action refused by its own validation, or turned away by a
    /// full worker queue, queued nothing and announces nothing.
    pub(super) fn hold_prompts_for_action(
        &mut self,
        prompts: Vec<PromptDispatch>,
    ) -> Vec<PromptDispatch> {
        let request = self.action_request.take();
        let refused = std::mem::take(&mut self.action_refused);
        if prompts.is_empty() {
            return Vec::new();
        }
        let request = match (request, refused) {
            (Some(request), false) => request,
            (Some(_), true) => {
                tracing::warn!("team action was refused after queueing; its prompts are withdrawn");
                return Vec::new();
            }
            (None, true) => {
                tracing::warn!("team action queued no operation; its prompts are withdrawn");
                return Vec::new();
            }
            (None, false) => return prompts,
        };
        let mut held: usize = self.awaiting_write.iter().map(|(_, held)| held.len()).sum();
        while held + prompts.len() > super::PROMPT_HOLD_CAPACITY {
            let Some((_, oldest)) = self.awaiting_write.pop_front() else {
                break;
            };
            held -= oldest.len();
            tracing::warn!(
                count = oldest.len(),
                "Held team prompts exceeded their bound; the oldest were dropped"
            );
        }
        self.awaiting_write.push_back((request, prompts));
        Vec::new()
    }

    /// Prompts whose operation the worker confirmed, ready to dispatch. Taken
    /// once, so a confirmed prompt cannot be delivered twice.
    pub(super) fn take_released_prompts(&mut self) -> Vec<PromptDispatch> {
        std::mem::take(&mut self.released_prompts)
    }

    #[cfg(test)]
    pub(super) fn held_prompt_count(&self) -> usize {
        self.awaiting_write.iter().map(|(_, held)| held.len()).sum()
    }

    pub(super) fn release_held_prompts(&mut self, request_id: u64) {
        let waiting = std::mem::take(&mut self.awaiting_write);
        for (request, prompts) in waiting {
            if request == request_id {
                self.released_prompts.extend(prompts);
            } else {
                self.awaiting_write.push_back((request, prompts));
            }
        }
    }

    /// A reply the bridge could not honour withdraws what waited on it: no
    /// later reply will ever confirm that request.
    pub(super) fn drop_held_prompts(&mut self, request_id: u64) {
        let waiting = std::mem::take(&mut self.awaiting_write);
        for (request, prompts) in waiting {
            if request == request_id {
                tracing::warn!(
                    count = prompts.len(),
                    "Team operation failed; the prompts waiting on it were withdrawn"
                );
            } else {
                self.awaiting_write.push_back((request, prompts));
            }
        }
    }
}
