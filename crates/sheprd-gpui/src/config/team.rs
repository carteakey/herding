//! The `[teams]` table: coordination tuning that is not a per-run choice.

use serde::Deserialize;

/// The lower bound of a stall timeout: below this every quiet tick stalls,
/// which only produces noise.
pub(crate) const MIN_STALL_TIMEOUT_SECONDS: u64 = 30;

/// The `[teams]` table.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct TeamConfig {
    /// How long a member may hold assigned work with no team CLI call and no
    /// pane output before their supervisor is woken once about the stall.
    /// Defaults to three minutes; issue #56's stall detection.
    #[serde(default, deserialize_with = "crate::lenient::or_default")]
    pub(crate) stall_timeout_seconds: u64,
}

impl Default for TeamConfig {
    fn default() -> Self {
        Self {
            stall_timeout_seconds: crate::team::bridge::prompts::stall::DEFAULT_STALL_TIMEOUT
                .as_secs(),
        }
    }
}

impl TeamConfig {
    /// The effective timeout, never below the noise floor.
    pub(crate) fn stall_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.stall_timeout_seconds.max(MIN_STALL_TIMEOUT_SECONDS))
    }
}

#[cfg(test)]
mod tests;
