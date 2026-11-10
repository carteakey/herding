//! A process held by the OS rather than named by its pid.
//!
//! Linux gives a process a descriptor that stays tied to it, so a signal
//! sent through one can never reach a later process given the same pid: it
//! fails once the process has exited. macOS has no public equivalent, nor
//! has a Linux older than 5.3, so there `open` finds nothing and the caller
//! signals by pid right after checking it, which leaves a window of
//! microseconds in which the process would have to exit and its pid be
//! reused.

use sysinfo::Signal;

#[cfg(target_os = "linux")]
pub(super) struct Pinned(std::os::fd::OwnedFd);

#[cfg(target_os = "linux")]
impl Pinned {
    /// Holds the process `pid` names now; `None` when there is none or the
    /// kernel cannot.
    pub(super) fn open(pid: u32) -> Option<Self> {
        use rustix::process::{Pid, PidfdFlags, pidfd_open};
        let pid = Pid::from_raw(i32::try_from(pid).ok()?)?;
        pidfd_open(pid, PidfdFlags::empty()).ok().map(Self)
    }

    /// Whether `signal` reached this very process.
    pub(super) fn send(&self, signal: Signal) -> bool {
        use rustix::process::{Signal as Sent, pidfd_send_signal};
        let signal = match signal {
            Signal::Continue => Sent::CONT,
            Signal::Kill => Sent::KILL,
            Signal::Stop => Sent::STOP,
            Signal::Term => Sent::TERM,
            _ => return false,
        };
        pidfd_send_signal(&self.0, signal).is_ok()
    }
}

/// Nothing can hold a process here.
#[cfg(not(target_os = "linux"))]
pub(super) enum Pinned {}

#[cfg(not(target_os = "linux"))]
impl Pinned {
    pub(super) fn open(_pid: u32) -> Option<Self> {
        None
    }

    pub(super) fn send(&self, _signal: Signal) -> bool {
        match *self {}
    }
}
