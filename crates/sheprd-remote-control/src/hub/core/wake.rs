//! Starting this machine's core for a phone's new work, when none answers.

use super::{Error, Result, Source, lock};
use crate::core_target::CoreTarget;
use sheprd_core_client::{Core, Endpoint};
use std::{
    path::Path,
    process::Command,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

/// How long `herdr-core server start` may take.
const START: Duration = Duration::from_secs(10);
/// How long a started core may take to answer its first dial.
const DIAL: Duration = Duration::from_secs(5);

impl Source {
    /// Starts the core at `binary` in the session this source joins, and
    /// joins it. Blocking. A core that already answers is joined instead:
    /// `server start` never starts a second one.
    pub(in crate::hub) fn wake(&self, binary: &Path) -> Result<()> {
        let endpoint = self.endpoint.endpoint()?;
        sheprd_core_client::start(binary, |command| configure(command, &self.endpoint), START)?;
        let core = Arc::new(dial(&endpoint)?);
        lock(&self.state).core = Some(core);
        // New work goes here at once; the worker's next poll lists it.
        self.connected.store(true, Ordering::Release);
        Ok(())
    }
}

/// The login shell's environment, and the session the desktop named, so
/// the core starts where this source looks for it.
fn configure(command: &mut Command, target: &CoreTarget) {
    sheprd_client::login_env::apply(command);
    if let CoreTarget::Endpoint(endpoint) = target {
        command
            .env("HC_SOCKET", &endpoint.socket)
            .env("HC_SESSION", &endpoint.session);
    }
}

/// `server start` returns once the core listens; a slow file system can
/// still refuse the first dial.
fn dial(endpoint: &Endpoint) -> Result<Core> {
    let deadline = Instant::now() + DIAL;
    loop {
        match Core::connect(endpoint) {
            Ok(core) => return Ok(core),
            Err(sheprd_core_client::Error::Connect { .. }) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(Error::Client(error).into()),
        }
    }
}
