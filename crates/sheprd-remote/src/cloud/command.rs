//! Prepare the same authenticated command used for a device connection, keeping
//! provider-owned credentials alive while a background operation runs.

use super::CloudProvider;
use sheprd_client::{Bridge, ConnectTarget};
use std::{
    io,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

pub fn with_command<T>(
    provider: CloudProvider,
    account: &str,
    id: &str,
    cancelled: &AtomicBool,
    run: impl FnOnce(Command) -> T,
) -> io::Result<T> {
    if cancelled.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            super::Error::Cancelled,
        ));
    }
    super::providers::run_command(provider, account, id, cancelled, run)
}

/// herdr-core on a saved cloud machine, through the same authenticated
/// command its Herdr 1 connection runs; `hc connect` starts the machine's core
/// when none is running. Runs on a worker: `stop` cancels it.
pub fn connect_core(target: &ConnectTarget, stop: &AtomicBool) -> io::Result<Bridge> {
    let ConnectTarget::Cloud {
        provider,
        account,
        id,
        ..
    } = target
    else {
        return Err(io::Error::other(super::Error::Invalid(
            super::Invalid::Target,
        )));
    };
    with_command(*provider, account, id, stop, |command| {
        sheprd_client::core_bridge::command(command, stop)
    })?
    .map_err(|error| io::Error::new(error.kind(), error))
}

#[cfg(test)]
mod tests;
