//! A phone's size, held on a Herdr 2 terminal for as long as its view is
//! open. The pane's shared screen takes the driver's role at that size;
//! the core sizes the terminal back to the driver before once the screen
//! lets go, as Herdr 1 does when a direct attach drops.

use super::{Error, Result, Source, lock};
use sheprd_terminal::Size;
use std::sync::{Arc, Weak, atomic::Ordering};

/// Held until dropped. Dropping it talks to the core, so drop it off the
/// async threads.
pub(crate) struct Hold {
    address: String,
    id: u64,
    /// The screen it is held on. A stream that closes gives the pane a new
    /// screen, which holds nothing until the phone holds it again.
    screen: Weak<std::sync::Mutex<super::screen::Screen>>,
    source: Weak<Source>,
}

impl Source {
    /// Holds `pane`'s terminal at `cols` by `rows`.
    pub(in crate::hub) fn hold(self: &Arc<Self>, pane: &str, cols: u16, rows: u16) -> Result<Hold> {
        let size = Size::new(cols, rows).map_err(|_| crate::hub::Error::Response)?;
        let (core, terminal) = self.terminal(pane)?;
        let screen = self.screen(&core, &terminal)?;
        let id = self.holds.fetch_add(1, Ordering::Relaxed);
        lock(&screen).hold(&core, id, size)?;
        Ok(Hold {
            address: terminal.address,
            id,
            screen: Arc::downgrade(&screen),
            source: Arc::downgrade(self),
        })
    }
}

impl Hold {
    pub(in crate::hub) fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let size = Size::new(cols, rows).map_err(|_| crate::hub::Error::Response)?;
        let (core, screen) = self.screen().ok_or(Error::Disconnected)?;
        lock(&screen).hold(&core, self.id, size)
    }

    /// Whether the pane's screen is still the one it was held on. Only the
    /// state lock is taken, never the screen's, which a read may hold.
    pub(in crate::hub) fn is_held(&self) -> bool {
        self.screen().is_some()
    }

    /// The screen and its connection, while the hold still has them. A
    /// screen dropped or replaced meanwhile took its driving attach with it.
    fn screen(&self) -> Option<(Arc<sheprd_core_client::Core>, super::SharedScreen)> {
        let source = self.source.upgrade()?;
        let state = lock(&source.state);
        let screen = state
            .screens
            .get(&self.address)
            .filter(|screen| Weak::ptr_eq(&Arc::downgrade(screen), &self.screen))?;
        Some((state.core.clone()?, screen.clone()))
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        let Some((core, screen)) = self.screen() else {
            return;
        };
        let released = lock(&screen).release(&core, self.id);
        if let Err(error) = released {
            tracing::debug!(%error, "a phone's size was not released; its screen goes");
            // Dropping the screen ends the driving attach, so the core sizes
            // the terminal back to the driver before.
            if let Some(source) = self.source.upgrade() {
                let mut state = lock(&source.state);
                if state
                    .screens
                    .get(&self.address)
                    .is_some_and(|current| Arc::ptr_eq(current, &screen))
                {
                    state.screens.remove(&self.address);
                }
            }
        }
    }
}
