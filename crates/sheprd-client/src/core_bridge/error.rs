//! Why a remote herdr-core could not be reached.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    /// The host has no `herdr-core` in the known install locations.
    #[error("herdr-core is not installed on the host")]
    Missing,
    // keep-sorted end
}

impl Error {
    /// The transport retry/cancellation category `crate::Error::kind` reports.
    pub(crate) fn kind(&self) -> io::ErrorKind {
        match self {
            Self::Missing => io::ErrorKind::NotFound,
        }
    }
}
