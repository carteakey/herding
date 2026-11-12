//! Every pane's content revision, polled from each daemon's own CLI: the
//! surface frame covers only the panes the daemon renders, while
//! `herdr pane list` reports a `revision` counter for every pane, background
//! tabs included, so a member whose pane the window does not show is still
//! observed (finding 736a1b22's observation half). The counter is a number;
//! no terminal text is ever read (issue #55).
//!
//! One poll per endpoint per cadence, at most one in flight per endpoint,
//! run on a worker thread so a slow SSH round trip never touches the UI
//! tick and one slow host cannot starve the others. A failed poll backs off
//! by doubling the wait up to a minute, logs once per failure episode, and
//! drops the endpoint's remembered panes: its members become unobservable -
//! never judged on stale silence - and the next window retries. A daemon
//! the CLI cannot address - a custom client socket, WSL, a cloud host - is
//! skipped, and an endpoint that disconnects loses its remembered panes with
//! it: a member on a host that is merely down is not a stall.

use serde::Deserialize;
use sheprd_client::{ScriptHost, ScriptLimits, SshLane, run_script, shell_quote};
use std::{
    collections::{HashMap, HashSet},
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

/// One answered poll: the endpoint's panes, or why none were read.
pub(crate) struct PaneListUpdate {
    pub(crate) endpoint: String,
    pub(crate) panes: Result<Vec<(String, u64)>, Error>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// The CLI could not be run, or its host refused the script.
    #[error("The daemon's pane list did not answer: {0}")]
    Script(#[from] sheprd_client::Error),
    /// The answer was not the JSON the daemon sends: malformed, or cut off
    /// by the output limit.
    #[error("The daemon's pane list was not readable: {0}")]
    Answer(#[from] serde_json::Error),
}

/// Which daemon a poll addresses, owned so the worker can build its script.
#[derive(Debug)]
pub(crate) enum PaneListTarget {
    /// This machine's daemon session; `None` is the default session.
    Local { session: Option<String> },
    /// A saved SSH host's daemon, asked through its own CLI on that host.
    /// The SSH command carries the client policy itself: `BatchMode=yes`,
    /// `ConnectTimeout=10`, no password prompts.
    Ssh {
        target: String,
        session: Option<String>,
    },
}

/// The poll runs under the host's `/bin/sh`, which a non-interactive SSH
/// session often starts without the daemon's bin directory on `PATH`
/// (Homebrew, Cargo), so the candidates [`crate::daemon::executable`]
/// scans locally are tried by name before falling back to `PATH`.
const CANDIDATES: &str = "\"$HOME/.local/bin/herdr\" \"$HOME/.cargo/bin/herdr\" \
         /home/linuxbrew/.linuxbrew/bin/herdr /opt/homebrew/bin/herdr \
         /usr/local/bin/herdr /usr/bin/herdr";

fn pane_list_script(session: Option<&str>) -> String {
    let session = match session {
        Some(session) if session != "default" => {
            format!(" --session {}", shell_quote(session))
        }
        _ => String::new(),
    };
    format!(
        "for c in {CANDIDATES}; do [ -x \"$c\" ] && exec \"$c\"{session} pane list; done\n\
         exec herdr{session} pane list\n"
    )
}

/// The `herdr pane list` answer: one counter per pane. Unknown fields are
/// ignored so newer daemons stay readable, and a pane without a revision is
/// skipped rather than failing the whole answer.
#[derive(Deserialize)]
struct PaneListAnswer {
    #[serde(default)]
    result: PaneListResult,
}

#[derive(Default, Deserialize)]
struct PaneListResult {
    #[serde(default)]
    panes: Vec<PaneListPane>,
}

#[derive(Deserialize)]
struct PaneListPane {
    pane_id: String,
    #[serde(default)]
    revision: Option<u64>,
}

const LIMITS: ScriptLimits = ScriptLimits {
    output: 256 * 1024,
    idle: Duration::from_secs(10),
};

/// A failed poll waits this long before the first retry, then doubles up to
/// [`MAX_FAILURE_BACKOFF`], so an unreachable host is probed rarely while
/// the rest of the fleet keeps its normal cadence.
const FIRST_FAILURE_BACKOFF: Duration = Duration::from_secs(20);
const MAX_FAILURE_BACKOFF: Duration = Duration::from_secs(60);

/// Blocking: asks one daemon for its pane revisions. Never reads text.
fn pane_list(target: &PaneListTarget, cancelled: &AtomicBool) -> Result<Vec<(String, u64)>, Error> {
    let script = pane_list_script(target_session(target));
    let mut output = Vec::new();
    let sent = match target {
        PaneListTarget::Local { .. } => run_script(
            ScriptHost::Local,
            &script,
            io::empty(),
            &mut output,
            LIMITS,
            cancelled,
        ),
        PaneListTarget::Ssh { target, .. } => run_script(
            ScriptHost::Ssh(target, SshLane::Quick),
            &script,
            io::empty(),
            &mut output,
            LIMITS,
            cancelled,
        ),
    };
    sent?;
    let answer: PaneListAnswer = serde_json::from_slice(&output)?;
    Ok(answer
        .result
        .panes
        .into_iter()
        .filter_map(|pane| pane.revision.map(|revision| (pane.pane_id, revision)))
        .collect())
}

fn target_session(target: &PaneListTarget) -> Option<&str> {
    match target {
        PaneListTarget::Local { session } | PaneListTarget::Ssh { session, .. } => {
            session.as_deref()
        }
    }
}

/// One pane-list request in flight.
pub(crate) struct PaneListRequest {
    pub(crate) endpoint: String,
    pub(crate) target: PaneListTarget,
}

/// One endpoint's poll schedule and failure episode.
struct EndpointPolls {
    /// When the endpoint's next poll may fire.
    due: Instant,
    /// The backoff the next failed poll waits out: doubled per consecutive
    /// failure up to [`MAX_FAILURE_BACKOFF`], reset by an answer.
    failure_backoff: Option<Duration>,
}

/// Per-endpoint poll state: each connected endpoint's schedule, at most one
/// poll in flight per endpoint, and the panes its last answered poll
/// reported, keyed by `(endpoint, pane_id)` so one daemon's counters can
/// never be read for another's. A failed poll drops the endpoint's
/// remembered panes; so does a disconnect, because `poll` retains only the
/// connected set.
pub(crate) struct PaneListPolls {
    endpoints: HashMap<String, EndpointPolls>,
    in_flight: HashSet<String>,
    panes: HashMap<(String, String), u64>,
    commands: Option<mpsc::SyncSender<PaneListRequest>>,
    updates: mpsc::Receiver<PaneListUpdate>,
    cancelled: Arc<AtomicBool>,
}

impl Default for PaneListPolls {
    fn default() -> Self {
        let (commands, received): (
            mpsc::SyncSender<PaneListRequest>,
            mpsc::Receiver<PaneListRequest>,
        ) = mpsc::sync_channel(4);
        let (sender, updates): (
            mpsc::SyncSender<PaneListUpdate>,
            mpsc::Receiver<PaneListUpdate>,
        ) = mpsc::sync_channel(8);
        let cancelled = Arc::new(AtomicBool::new(false));
        let shared = cancelled.clone();
        let _ = thread::Builder::new()
            .name("herdr-team-pane-list".into())
            .spawn(move || {
                while let Ok(request) = received.recv() {
                    let panes = pane_list(&request.target, &shared);
                    if sender
                        .send(PaneListUpdate {
                            endpoint: request.endpoint,
                            panes,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            });
        Self {
            endpoints: HashMap::new(),
            in_flight: HashSet::new(),
            panes: HashMap::new(),
            commands: Some(commands),
            updates,
            cancelled,
        }
    }
}

impl Drop for PaneListPolls {
    /// Abandons a CLI call in flight; the closed channel ends the worker.
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl PaneListPolls {
    /// Fires every due endpoint's at-most-one-in-flight poll and folds
    /// answered polls into the remembered panes. `endpoints` names each
    /// CONNECTED endpoint and how its CLI is addressed; `None` skips an
    /// endpoint the CLI cannot reach, and an endpoint absent from the set
    /// loses its schedule and remembered panes, so a host that is merely
    /// down cannot read as a stall. A result that lands after its endpoint
    /// disconnected is dropped rather than remembered.
    pub(crate) fn poll(
        &mut self,
        endpoints: impl IntoIterator<Item = (String, Option<PaneListTarget>)>,
        now: Instant,
        pane_list_poll: Duration,
    ) -> Vec<PaneListUpdate> {
        let connected: HashMap<String, PaneListTarget> = endpoints
            .into_iter()
            .filter_map(|(endpoint, target)| target.map(|target| (endpoint, target)))
            .collect();
        self.endpoints
            .retain(|endpoint, _| connected.contains_key(endpoint));
        self.panes
            .retain(|(endpoint, _), _| connected.contains_key(endpoint));

        for (endpoint, target) in &connected {
            if self.in_flight.contains(endpoint) {
                continue;
            }
            let state = self
                .endpoints
                .entry(endpoint.clone())
                .or_insert(EndpointPolls {
                    due: now,
                    failure_backoff: None,
                });
            if now < state.due {
                continue;
            }
            if let Some(commands) = &self.commands
                && commands
                    .try_send(PaneListRequest {
                        endpoint: endpoint.clone(),
                        target: clone_target(target),
                    })
                    .is_ok()
            {
                self.in_flight.insert(endpoint.clone());
                state.due = now + pane_list_poll;
            }
        }

        let mut updates = Vec::new();
        while let Ok(update) = self.updates.try_recv() {
            self.in_flight.remove(&update.endpoint);
            if !connected.contains_key(&update.endpoint) {
                // The endpoint disconnected while its poll was in flight:
                // the answer is stale, so it is dropped, not remembered.
                continue;
            }
            let state = self
                .endpoints
                .entry(update.endpoint.clone())
                .or_insert(EndpointPolls {
                    due: now,
                    failure_backoff: None,
                });
            match &update.panes {
                Ok(panes) => {
                    if state.failure_backoff.take().is_some() {
                        tracing::info!(
                            endpoint = %update.endpoint,
                            outcome = "recovered",
                            "team stall pane-list poll answers again after failures"
                        );
                    }
                    state.due = now + pane_list_poll;
                    for (pane, revision) in panes {
                        self.panes
                            .insert((update.endpoint.clone(), pane.clone()), *revision);
                    }
                }
                Err(error) => {
                    let backoff = state
                        .failure_backoff
                        .unwrap_or(FIRST_FAILURE_BACKOFF)
                        .min(MAX_FAILURE_BACKOFF);
                    let first_of_episode = state.failure_backoff.is_none();
                    state.failure_backoff = Some((backoff * 2).min(MAX_FAILURE_BACKOFF));
                    state.due = now + backoff;
                    if first_of_episode {
                        tracing::warn!(
                            endpoint = %update.endpoint,
                            outcome = "unreachable",
                            %error,
                            "team stall pane-list poll failed; the endpoint's panes are unobservable until it answers, retried with backoff"
                        );
                    }
                }
            }
            updates.push(update);
        }
        updates
    }

    /// The remembered revisions, one pane at a time, for the merged
    /// observation.
    pub(crate) fn panes(&self) -> impl Iterator<Item = (&str, u64)> {
        self.panes
            .iter()
            .map(|((_, pane), revision)| (pane.as_str(), *revision))
    }
}

/// The request carries an owned copy of the target, so the worker never
/// touches window state.
fn clone_target(target: &PaneListTarget) -> PaneListTarget {
    match target {
        PaneListTarget::Local { session } => PaneListTarget::Local {
            session: session.clone(),
        },
        PaneListTarget::Ssh { target, session } => PaneListTarget::Ssh {
            target: target.clone(),
            session: session.clone(),
        },
    }
}

#[cfg(test)]
impl PaneListPolls {
    /// A test's stand-in for an answered poll: seeds an endpoint's
    /// remembered panes as if its CLI had just answered, so window tests can
    /// drive the merged observation without a daemon.
    pub(crate) fn observe_pane_list_for_test(&mut self, endpoint: &str, panes: &[(&str, u64)]) {
        for (pane, revision) in panes {
            self.panes
                .insert((endpoint.to_owned(), pane.to_string()), *revision);
        }
    }

    /// A poller with no worker: the test plays the worker's part through the
    /// returned update sender, and reads the requests it was sent.
    pub(crate) fn fake() -> (
        Self,
        mpsc::SyncSender<PaneListUpdate>,
        mpsc::Receiver<PaneListRequest>,
    ) {
        let (commands, received): (
            mpsc::SyncSender<PaneListRequest>,
            mpsc::Receiver<PaneListRequest>,
        ) = mpsc::sync_channel(4);
        let (sender, updates): (
            mpsc::SyncSender<PaneListUpdate>,
            mpsc::Receiver<PaneListUpdate>,
        ) = mpsc::sync_channel(8);
        (
            Self {
                endpoints: HashMap::new(),
                in_flight: HashSet::new(),
                panes: HashMap::new(),
                commands: Some(commands),
                updates,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
            sender,
            received,
        )
    }

    /// Whether `endpoint` has a poll in flight.
    pub(crate) fn in_flight_for_test(&self, endpoint: &str) -> bool {
        self.in_flight.contains(endpoint)
    }

    /// Whether `endpoint`'s remembered panes are present.
    pub(crate) fn remembers_for_test(&self, endpoint: &str) -> bool {
        self.panes
            .keys()
            .any(|(remembered, _)| remembered == endpoint)
    }

    /// Whether `endpoint` is mid failure episode: its last poll failed and
    /// no answer has arrived since.
    pub(crate) fn failing_for_test(&self, endpoint: &str) -> bool {
        self.endpoints
            .get(endpoint)
            .is_some_and(|state| state.failure_backoff.is_some())
    }

    /// When `endpoint`'s next poll may fire.
    pub(crate) fn due_for_test(&self, endpoint: &str) -> Option<Instant> {
        self.endpoints.get(endpoint).map(|state| state.due)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
