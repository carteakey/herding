//! Every pane's content revision, polled from each daemon's own CLI: the
//! surface frame covers only the panes the daemon renders, while
//! `herdr pane list` reports a `revision` counter for every pane, background
//! tabs included, so a member whose pane the window does not show is still
//! observed (finding 736a1b22's observation half). The counter is a number;
//! no terminal text is ever read (issue #55).
//!
//! One poll per endpoint per the configured cadence, run on a worker thread
//! so a
//! slow SSH round trip never touches the UI tick. A daemon the CLI cannot
//! address - a custom client socket, WSL, a cloud host - is skipped and its
//! panes stay unobservable, exactly as before.

use serde::Deserialize;
use sheprd_client::{ScriptHost, ScriptLimits, SshLane, run_script, shell_quote};
use std::{
    collections::HashMap,
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
    time::Instant,
};

/// One answered poll: the endpoint's panes, or why none were read.
pub(crate) struct PaneListUpdate {
    pub(crate) endpoint: String,
    /// `{pane_id: revision}` per pane the daemon reports. A failure carries
    /// the reason for one log line; the next window retries.
    pub(crate) panes: Result<Vec<(String, u64)>, String>,
}

/// Which daemon a poll addresses, owned so the worker can build its script.
#[derive(Debug)]
pub(crate) enum PaneListTarget {
    /// This machine's daemon session; `None` is the default session.
    Local { session: Option<String> },
    /// A saved SSH host's daemon, asked through its own CLI on that host.
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

/// Blocking: asks one daemon for its pane revisions. Never reads text.
fn pane_list(
    target: &PaneListTarget,
    cancelled: &AtomicBool,
) -> Result<Vec<(String, u64)>, String> {
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
    sent.map_err(|error| error.to_string())?;
    let answer: PaneListAnswer =
        serde_json::from_slice(&output).map_err(|error| error.to_string())?;
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

/// Per-endpoint poll state: when each endpoint is next due, the panes its
/// last answered poll reported, and the worker that runs the CLI. A failed
/// poll drops the endpoint's remembered panes, so its members become
/// unobservable - never judged on stale silence.
pub(crate) struct PaneListPolls {
    dues: HashMap<String, Instant>,
    panes: HashMap<String, Vec<(String, u64)>>,
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
        let (sender, updates) = mpsc::sync_channel(8);
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
            dues: HashMap::new(),
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
    /// Fires every due endpoint's poll and folds answered polls into the
    /// remembered panes. `endpoints` names each connected endpoint and how
    /// its CLI is addressed; `None` skips an endpoint the CLI cannot reach.
    pub(crate) fn poll(
        &mut self,
        endpoints: impl IntoIterator<Item = (String, Option<PaneListTarget>)>,
        now: Instant,
        pane_list_poll: Duration,
    ) -> Vec<PaneListUpdate> {
        for (endpoint, target) in endpoints {
            let Some(target) = target else {
                continue;
            };
            let due = self.dues.entry(endpoint.clone()).or_insert(now);
            if now >= *due {
                if let Some(commands) = &self.commands
                    && commands
                        .try_send(PaneListRequest {
                            endpoint: endpoint.clone(),
                            target,
                        })
                        .is_err()
                {
                    // A full queue means the worker is behind; the next
                    // window tries again.
                    continue;
                }
                *due = now + pane_list_poll;
            }
        }
        let mut updates = Vec::new();
        while let Ok(update) = self.updates.try_recv() {
            match &update.panes {
                Ok(panes) => {
                    self.panes.insert(update.endpoint.clone(), panes.clone());
                }
                Err(_) => {
                    self.panes.remove(&update.endpoint);
                }
            }
            updates.push(update);
        }
        updates
    }

    /// The remembered `{pane_id: revision}` per answered endpoint.
    pub(crate) fn panes(&self) -> impl Iterator<Item = (&str, u64)> {
        self.panes
            .values()
            .flatten()
            .map(|(pane, revision)| (pane.as_str(), *revision))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

#[cfg(test)]
impl PaneListPolls {
    /// A test's stand-in for an answered poll: seeds an endpoint's remembered
    /// panes as if its CLI had just answered, so window tests can drive the
    /// merged observation without a daemon.
    pub(crate) fn observe_pane_list_for_test(&mut self, endpoint: &str, panes: &[(&str, u64)]) {
        self.panes.insert(
            endpoint.to_owned(),
            panes
                .iter()
                .map(|(pane, revision)| (pane.to_string(), *revision))
                .collect(),
        );
    }
}
