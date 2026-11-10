//! Why the pinned herdr-core could not be installed on a machine.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    /// The download did not match the pinned checksum; nothing was installed.
    #[error("the herdr-core download does not match its pinned checksum")]
    Checksum,
    /// An install folder that is not absolute or holds control characters.
    #[error("herdr-core's install folder must be an absolute path")]
    Dir,
    /// herdr-core publishes no build for the machine's system.
    #[error("herdr-core has no build for this machine")]
    NoBuild,
    /// The machine has neither `sha256sum` nor `shasum` to check the download.
    #[error("the machine has no sha256sum or shasum to check herdr-core")]
    NoChecksumTool,
    /// The machine has no `curl` to download the core with.
    #[error("the machine has no curl to download herdr-core")]
    NoCurl,
    /// The pinned release names no build, or a malformed checksum, for a
    /// system the script installs on.
    #[error("the pinned herdr-core has no usable {0} build")]
    NoPinnedBuild(&'static str),
    /// The pinned release could not be read.
    #[error("the pinned herdr-core release is unreadable")]
    Pinned(#[source] serde_json::Error),
    /// The script could not run, or failed in a way it does not name.
    #[error(transparent)]
    Run(sheprd_client::Error),
    /// The script ended without saying it finished.
    #[error("the herdr-core installer did not finish")]
    Unfinished,
    // keep-sorted end
}
