//! Prompts the bridge holds back: ones waiting for a pane to be unpaused, and
//! ones waiting for the store to confirm the write that produced them. No
//! prompt dispatches in the tick its action was queued, so an agent woken with
//! "claim this task" or "ack this post" finds the row when it runs the command
//! it was told to run.
use super::{Bridge, WAKE_ENTER_STALE_AFTER, WAKE_RETRY, roster};
use std::collections::HashSet;
use std::time::Instant;

/// A prompt to dispatch: the member's recorded host and pane to wake, the
/// prompt text, and the one-line summary the panel shows. The host routes
/// the wake to the member's own daemon: pane ids are per-daemon and short,
/// so two connected hosts can both show the same pane id (#40).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PromptDispatch {
    pub(crate) host: String,
    pub(crate) pane_id: String,
    pub(crate) prompt: String,
    pub(crate) summary: String,
}

/// A held prompt released for delivery, with the wake signals it reports so
/// the window can mark them seen only after the delivery succeeds.
pub(super) struct DrainedPrompt {
    pub(super) team_id: String,
    pub(super) host: String,
    pub(super) pane_id: String,
    pub(super) prompt: String,
    pub(super) summary: String,
    pub(super) signals: Vec<String>,
}

/// Prompts waiting for the reply that confirms the operation that produced
/// them, oldest first so a worker that never answers drops them.
pub(super) type HeldPrompts = std::collections::VecDeque<(u64, Vec<PromptDispatch>)>;

/// An Enter still owed for a wake text already typed into a pane, due after
/// [`super::WAKE_ENTER_DELAY`]. Host and pane route it back to the same
/// endpoint the text went to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WakeEnter {
    pub(crate) host: String,
    pub(crate) pane_id: String,
    due: Instant,
}

pub(crate) const PAUSE_QUEUE_CAPACITY: usize = 64;

/// How many owed Enters may wait at once. One row per delivered wake text,
/// and the drain pops them every tick, so the steady state is empty; the
/// cap keeps a pathological run from growing the queue without bound.
pub(super) const WAKE_ENTER_QUEUE_CAP: usize = 64;

/// How many prompts may wait for their action's write to be confirmed.
/// Oldest-first eviction keeps a worker that never answers from turning the
/// bridge into a prompt backlog.
pub(super) const PROMPT_HOLD_CAPACITY: usize = 64;

/// A pending catch-up dispatch for the selected team's supervisor.
pub(crate) struct SupervisorWake {
    pub(super) team_id: String,
    /// The supervisor's recorded host, so the wake routes to the supervisor's
    /// own daemon instead of any endpoint showing the same pane id (#40).
    pub(super) host: String,
    pub(super) pane_id: String,
    pub(super) prompt: String,
    pub(super) summary: String,
    pub(super) signals: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct QueuedPrompt {
    pub(super) team_id: String,
    /// The member's recorded host, carried so the dispatch resolves the
    /// endpoint by daemon boot id rather than by pane id alone (#40).
    pub(super) host: String,
    pub(super) pane_id: String,
    pub(super) prompt: String,
    pub(super) summary: String,
    /// Wake signals this prompt reports. Empty for prompts that carry no
    /// ledger entry (bulletin mentions, action prompts). A held prompt keeps
    /// them so the drain can mark them seen only once it actually delivers.
    pub(super) signals: Vec<String>,
    /// A failed delivery is retried no earlier than this instant, so a pane
    /// that is briefly unreachable does not spin the drain every tick.
    pub(super) retry_at: Instant,
}

impl Bridge {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn enqueue_prompt(
        &mut self,
        team_id: String,
        host: String,
        pane_id: String,
        prompt: String,
        summary: String,
        signals: Vec<String>,
    ) {
        self.push_prompt(
            team_id,
            PromptDispatch {
                host,
                pane_id,
                prompt,
                summary,
            },
            signals,
            Instant::now(),
        );
    }

    /// Queues a prompt the dispatch could not deliver now - a paused pane, a
    /// busy agent, or no reachable agent at all - for a later tick. Unlike
    /// [`Bridge::enqueue_prompt`], the retry is backed off by [`WAKE_RETRY`]:
    /// a pane that stays unavailable must not be re-offered every tick,
    /// which once hot-looped the drain on stale panes.
    pub(super) fn hold_prompt(
        &mut self,
        team_id: String,
        host: String,
        pane_id: String,
        prompt: String,
        summary: String,
        signals: Vec<String>,
    ) {
        self.push_prompt(
            team_id,
            PromptDispatch {
                host,
                pane_id,
                prompt,
                summary,
            },
            signals,
            Instant::now() + WAKE_RETRY,
        );
    }

    /// Puts a prompt the drain could not deliver back in the queue, to be
    /// retried after `retry_at`. A wake is only worth dropping when it was
    /// actually delivered, so an unreachable pane keeps its prompt instead of
    /// swallowing it.
    pub(super) fn requeue_prompt(&mut self, prompt: DrainedPrompt, retry_at: Instant) {
        self.push_prompt(
            prompt.team_id,
            PromptDispatch {
                host: prompt.host,
                pane_id: prompt.pane_id,
                prompt: prompt.prompt,
                summary: prompt.summary,
            },
            prompt.signals,
            retry_at,
        );
    }

    /// Owes an Enter for a wake text just typed into `pane_id`. Scheduled
    /// only after the text batch succeeded: a failed text has nothing to
    /// submit. At the capacity cap the oldest entry is dropped with a log -
    /// it is the one most likely stale, and a pathological run of sent
    /// texts must not grow the queue without bound.
    pub(super) fn schedule_wake_enter(&mut self, host: &str, pane_id: &str, due: Instant) {
        while self.wake_enters.len() >= WAKE_ENTER_QUEUE_CAP {
            match self.wake_enters.pop_front() {
                Some(dropped) => tracing::warn!(
                    host = %dropped.host,
                    pane = %dropped.pane_id,
                    "team wake enter dropped: the owed-enter queue is at capacity"
                ),
                None => break,
            }
        }
        self.wake_enters.push_back(WakeEnter {
            host: host.to_owned(),
            pane_id: pane_id.to_owned(),
            due,
        });
    }

    /// Pops every owed Enter whose delay has run out, oldest first, split
    /// into sendable ones and stale ones. A stale entry - due longer ago
    /// than [`super::WAKE_ENTER_STALE_AFTER`] - is returned for the caller
    /// to drop with a log, never sent: by then the pane's input may hold
    /// the member's own typing instead of the wake text. The drain runs
    /// every tick, so in steady state both lists are empty.
    pub(super) fn drain_due_enters(&mut self, now: Instant) -> (Vec<WakeEnter>, Vec<WakeEnter>) {
        let mut send = Vec::new();
        let mut stale = Vec::new();
        while self
            .wake_enters
            .front()
            .is_some_and(|enter| enter.due <= now)
        {
            match self.wake_enters.pop_front() {
                Some(enter) if now > enter.due + WAKE_ENTER_STALE_AFTER => stale.push(enter),
                Some(enter) => send.push(enter),
                None => break,
            }
        }
        (send, stale)
    }

    fn push_prompt(
        &mut self,
        team_id: String,
        dispatch: PromptDispatch,
        signals: Vec<String>,
        retry_at: Instant,
    ) {
        // A wake that is still held must not be queued twice: the event path
        // and the periodic fallback can both offer the same unseen result
        // while the pane stays paused. Prompts without a signal (mentions,
        // action prompts) are not deduped because their text may legitimately
        // repeat.
        if !signals.is_empty()
            && self.queued_prompts.iter().any(|queued| {
                queued.team_id == team_id
                    && queued.pane_id == dispatch.pane_id
                    && queued.signals == signals
            })
        {
            return;
        }
        if self.queued_prompts.len() >= PAUSE_QUEUE_CAPACITY {
            self.queued_prompts.remove(0);
        }
        self.queued_prompts.push(QueuedPrompt {
            team_id,
            host: dispatch.host,
            pane_id: dispatch.pane_id,
            prompt: dispatch.prompt,
            summary: dispatch.summary,
            signals,
            retry_at,
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
    pub(super) fn drain_unpaused_prompts(&mut self) -> Vec<DrainedPrompt> {
        let Some(team) = self.team.as_ref() else {
            return Vec::new();
        };
        if self.is_team_paused() {
            return Vec::new();
        }
        let now = Instant::now();
        let team_id = team.team_id.as_str();
        let paused_panes: HashSet<String> = roster(team)
            .filter(|m| self.paused_members.contains(&m.member_id))
            .filter_map(|m| m.target.pane.clone())
            .collect();
        let mut ready = Vec::new();
        let mut held = Vec::new();
        for item in self.queued_prompts.drain(..) {
            if item.team_id == team_id {
                if paused_panes.contains(&item.pane_id) || item.retry_at > now {
                    held.push(item);
                } else {
                    ready.push(DrainedPrompt {
                        team_id: item.team_id,
                        host: item.host,
                        pane_id: item.pane_id,
                        prompt: item.prompt,
                        summary: item.summary,
                        signals: item.signals,
                    });
                }
            } else {
                held.push(item);
            }
        }
        self.queued_prompts = held;
        ready
    }

    /// Drops the selected team's queued prompts whose retry is due and whose
    /// pane can no longer take a wake: it is neither in the team roster nor
    /// shown by any connected snapshot. A pane that vanished mid-hold waits
    /// out [`WAKE_RETRY`] once - one that returns in time still gets its
    /// wake - and one that is still gone is marked undeliverable instead of
    /// being re-offered forever. Prompts whose pane is merely reconnecting
    /// (still in the roster) and other teams' prompts are kept. Each dropped
    /// prompt counts toward the status surface; the unique panes are
    /// returned for the activity line.
    pub(super) fn expire_unreachable_prompts(
        &mut self,
        live_panes: &HashSet<String>,
    ) -> Vec<(String, String)> {
        let Some(team) = self.team.as_ref() else {
            return Vec::new();
        };
        let team_id = team.team_id.as_str();
        let roster_panes: HashSet<String> =
            roster(team).filter_map(|m| m.target.pane.clone()).collect();
        let now = Instant::now();
        let mut dropped: Vec<(String, String)> = Vec::new();
        let mut dropped_count = 0usize;
        let mut kept = Vec::new();
        for item in self.queued_prompts.drain(..) {
            let unreachable = item.team_id == team_id
                && item.retry_at <= now
                && !roster_panes.contains(&item.pane_id)
                && !live_panes.contains(&item.pane_id);
            if unreachable {
                self.undeliverable += 1;
                dropped_count += 1;
                if !dropped.iter().any(|(_, pane)| pane == &item.pane_id) {
                    dropped.push((item.host.clone(), item.pane_id.clone()));
                }
            } else {
                kept.push(item);
            }
        }
        self.queued_prompts = kept;
        if dropped_count > 0 {
            tracing::warn!(
                count = dropped_count,
                panes = %dropped.iter().map(|(_, pane)| pane).cloned().collect::<Vec<_>>().join(", "),
                "Queued team wakes marked undeliverable: their panes are in neither the roster nor any connected snapshot"
            );
        }
        dropped
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
        while held + prompts.len() > PROMPT_HOLD_CAPACITY {
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
