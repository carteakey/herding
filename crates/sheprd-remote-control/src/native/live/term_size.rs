//! A terminal view's size: while a phone follows a terminal at its own
//! size, the view holds a direct attach that makes Herdr size the pane's
//! terminal to it. Dropping the view drops the attach, and Herdr sizes the
//! terminal back to its layout. A Herdr 2 terminal is held the same way,
//! and its view scrolls through the history the daemon keeps.

use super::super::wire::TermSize;
use crate::{
    hub::{Hub, TermHold},
    web::{ApiError, Shared, read_scrolled},
};
use sheprd_client::{protocol::AttachScrollDirection, terminal_attach};

/// The attach a terminal view holds, and the size it holds it at.
#[derive(Default)]
pub(super) struct Sizing {
    held: Option<Held>,
    /// History lines above the screen at the last read of a Herdr 2 pane.
    history: usize,
    /// Lines the pane had pushed into history at that read, which keeps a
    /// scrolled view on the same lines as output arrives.
    lines: Option<u64>,
    size: Option<TermSize>,
    /// Lines a Herdr 2 view is scrolled up into its history.
    up: usize,
}

impl Sizing {
    /// Holds `pane`'s terminal at `wanted`, or lets it go when `None`.
    /// Returns whether the size changed. A failed attach leaves the
    /// terminal at its layout size; the view goes on regardless.
    pub(super) async fn apply(
        &mut self,
        state: &Shared,
        pane: &str,
        wanted: Option<TermSize>,
    ) -> bool {
        // A hold lost meanwhile, as when a Herdr 2 stream recovers on a new
        // screen, is made again at the same size.
        let lost = self
            .held
            .as_ref()
            .and_then(|held| held.attach.as_ref())
            .is_some_and(|attach| !attach.is_attached());
        if wanted == self.size && !lost {
            return false;
        }
        self.size = wanted;
        let Some(size) = wanted else {
            self.held = None;
            return true;
        };
        if let Some(attach) = self.held.as_ref().and_then(|held| held.attach.as_ref())
            && attach.is_attached()
            && attach.resize(size.cols, size.rows).await
        {
            return true;
        }
        self.held = None;
        match state.hub.attach_terminal(pane, size.cols, size.rows).await {
            Ok(attach) => {
                tracing::debug!(pane, size.cols, size.rows, "terminal sized to the phone");
                self.held = Some(Held {
                    attach: Some(attach),
                });
            }
            Err(error) => {
                tracing::info!(pane, %error, "terminal stays at its layout size");
            }
        }
        true
    }

    /// The screen as this view shows it: a Herdr 2 pane scrolled where the
    /// view scrolled it.
    pub(super) async fn read(&mut self, state: &Shared, pane: &str) -> Result<String, ApiError> {
        let read = read_scrolled(state, pane, self.up, self.lines).await?;
        self.history = read.history;
        self.lines = read.lines;
        self.up = read.up;
        Ok(read.text)
    }

    /// Back to the live screen, as when this phone types.
    pub(super) fn live(&mut self) {
        self.up = 0;
    }

    /// Scrolls the view `lines`, up toward its history when positive.
    /// False when nothing can take the scroll: a Herdr 1 terminal scrolls
    /// only through an attach holding it.
    pub(super) fn scroll(&mut self, pane: &str, lines: i32) -> bool {
        if Hub::is_core_pane(pane) {
            let by = usize::try_from(lines.unsigned_abs()).unwrap_or(usize::MAX);
            self.up = if lines > 0 {
                self.up.saturating_add(by).min(self.history)
            } else {
                self.up.saturating_sub(by)
            };
            return true;
        }
        let Some(attach) = self
            .held
            .as_ref()
            .and_then(|held| held.attach.as_ref())
            .and_then(TermHold::attach)
        else {
            return false;
        };
        let direction = if lines > 0 {
            AttachScrollDirection::Up
        } else {
            AttachScrollDirection::Down
        };
        let lines = u16::try_from(lines.unsigned_abs()).unwrap_or(u16::MAX);
        match attach.scroll(direction, lines) {
            // Herdr is behind on scrolls: this one is dropped, not refused.
            Ok(()) | Err(terminal_attach::Error::Busy) => true,
            Err(error) => {
                tracing::debug!(%error, "terminal scroll not sent");
                false
            }
        }
    }
}

/// An attach dropped off the async threads: dropping one waits briefly
/// for its worker to send `Detach`.
struct Held {
    attach: Option<TermHold>,
}

impl Drop for Held {
    fn drop(&mut self) {
        let Some(attach) = self.attach.take() else {
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn_blocking(move || drop(attach));
            }
            Err(_) => drop(attach),
        }
    }
}

#[cfg(test)]
mod tests;
