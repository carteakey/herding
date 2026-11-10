//! Sizing a terminal to a phone: a direct attach to the pane's terminal,
//! which Herdr resizes to the attach's size for as long as it is held. A
//! Herdr 2 terminal is held through its shared screen instead.

use super::{ApiMethod, Error, Hub, Result};
use serde_json::json;
use sheprd_client::{TerminalAttach, terminal_attach};
use std::{sync::Arc, time::Duration};

/// How long a refused attach waits before its one retry: the attach a
/// view just dropped may still be detaching.
const RETRY: Duration = Duration::from_millis(250);

/// A terminal held at a phone's size until dropped. Dropping one waits on
/// the daemon, so drop it off the async threads.
pub(crate) enum TermHold {
    Core(Arc<super::core::Hold>),
    Herdr(TerminalAttach),
}

impl TermHold {
    /// Whether it still holds the terminal, so a resize can reuse it.
    pub(crate) fn is_attached(&self) -> bool {
        match self {
            // A recovered stream's new screen does not hold it yet.
            Self::Core(hold) => hold.is_held(),
            Self::Herdr(attach) => attach.is_attached(),
        }
    }

    /// Holds the terminal at a new size; false when it no longer can.
    pub(crate) async fn resize(&self, cols: u16, rows: u16) -> bool {
        match self {
            Self::Core(hold) => {
                let hold = hold.clone();
                tokio::task::spawn_blocking(move || hold.resize(cols, rows).is_ok())
                    .await
                    .unwrap_or(false)
            }
            Self::Herdr(attach) => attach.resize(cols, rows).is_ok(),
        }
    }

    /// Herdr 1's attach, which scrolls the terminal; a Herdr 2 view
    /// scrolls its own reads.
    pub(crate) fn attach(&self) -> Option<&TerminalAttach> {
        match self {
            Self::Core(_) => None,
            Self::Herdr(attach) => Some(attach),
        }
    }
}

impl Hub {
    /// Whether `pane` is a Herdr 2 terminal's.
    pub(crate) fn is_core_pane(pane: &str) -> bool {
        super::core::owns(&json!({"pane_id": pane}))
    }

    /// Holds `pane`'s terminal at `cols` by `rows` until the hold is
    /// dropped. Never takes a Herdr 1 terminal from another direct client;
    /// a Herdr 2 terminal goes back to its previous driver's size.
    pub(crate) async fn attach_terminal(
        &self,
        pane: &str,
        cols: u16,
        rows: u16,
    ) -> Result<TermHold> {
        if Self::is_core_pane(pane) {
            let core = self.core.clone().ok_or(super::core::Error::Disconnected)?;
            let pane = pane.to_owned();
            let hold = tokio::task::spawn_blocking(move || core.hold(&pane, cols, rows))
                .await
                .map_err(|_| super::core::Error::Disconnected)??;
            return Ok(TermHold::Core(Arc::new(hold)));
        }
        // A named session's pane is asked of, and attached on, its socket.
        let (api, pane) = match self.session_for(ApiMethod::PaneGet, &json!({ "pane_id": pane }))? {
            Some((session, params)) => (
                session.api.clone(),
                params["pane_id"].as_str().unwrap_or_default().to_owned(),
            ),
            None => (self.api.clone(), pane.to_owned()),
        };
        let answer = api
            .call(ApiMethod::PaneGet, json!({ "pane_id": pane }))
            .await?;
        let terminal = answer["pane"]["terminal_id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or(Error::Response)?
            .to_owned();
        let target = api.target();
        let open = move || TerminalAttach::open(&target, &terminal, cols, rows);
        let mut attempts = 0;
        loop {
            attempts += 1;
            let attempt = open.clone();
            let result = tokio::task::spawn_blocking(attempt)
                .await
                .map_err(|_| Error::NotConnected)?;
            match result {
                Err(sheprd_client::Error::TerminalAttach(terminal_attach::Error::Refused(_)))
                    if attempts == 1 =>
                {
                    tokio::time::sleep(RETRY).await;
                }
                result => return Ok(TermHold::Herdr(result?)),
            }
        }
    }
}
