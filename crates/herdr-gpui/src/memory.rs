//! Memory instrumentation for the app process: a periodic RSS/swap sampler,
//! a high-water mark, named gauges subsystems can keep their retained size
//! in, and a soft guard that dumps the holders when the process crosses its
//! limit - so the next oomd kill is preceded by a log naming what grew
//! (issue #12: the app peaked at 7G RSS + 7.1G swap inside a 35-minute
//! dogfood session and systemd-oomd killed it twice on 2026-11-05).
//!
//! The sampler is a plain thread, not a gpui executor task: it must run no
//! matter what the UI is doing, and a `/proc` read costs microseconds. The
//! gauges are lock-free to update: a subsystem registers once, stores the
//! returned handle, and sets its count wherever it already mutates. Nothing
//! here holds UI state, and the sampled path allocates nothing beyond the
//! log line itself.

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

static GAUGES: OnceLock<Mutex<BTreeMap<&'static str, Arc<AtomicU64>>>> = OnceLock::new();

fn registry() -> &'static Mutex<BTreeMap<&'static str, Arc<AtomicU64>>> {
    GAUGES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// A named counter a subsystem keeps its retained size or entity count in.
/// [`gauge`] returns the same counter for the same name, so a caller
/// registers once and stores the handle; the sampler's dump reads all of
/// them, which is how a growing subsystem names itself.
#[derive(Clone, Debug)]
pub struct Gauge(Arc<AtomicU64>);

pub fn gauge(name: &'static str) -> Gauge {
    let mut gauges = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Gauge(gauges.entry(name).or_default().clone())
}

impl Gauge {
    pub fn set(&self, value: u64) {
        self.0.store(value, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Every registered gauge as `name=value` joined by commas; the dumps embed
/// this so growth in a wired subsystem names itself.
fn snapshot_gauges() -> String {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .map(|(name, value)| format!("{name}={}", value.load(Ordering::Relaxed)))
        .collect::<Vec<_>>()
        .join(",")
}

/// The guard's decision for one sample: whether it trips (a dump is due)
/// and what arming state it leaves behind. It trips only while armed, and
/// re-arms below half the limit, so one allocation cycle past the limit
/// logs one dump rather than one per tick.
fn guard_transition(total_kib: u64, limit_kib: Option<u64>, armed: bool) -> (bool, bool) {
    let Some(limit_kib) = limit_kib else {
        return (armed, false);
    };
    if total_kib >= limit_kib {
        (false, armed)
    } else if total_kib * 2 < limit_kib {
        (true, false)
    } else {
        (armed, false)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    #[cfg(target_os = "linux")]
    use super::sampler::{holder_dump, parse_status, self_status};
    use super::*;

    #[test]
    fn a_gauge_is_the_same_counter_for_the_same_name() {
        let first = gauge("test-counter");
        first.set(7);
        let second = gauge("test-counter");
        assert_eq!(second.get(), 7);
        second.set(9);
        assert_eq!(first.get(), 9);
        assert_ne!(gauge("test-other-counter").get(), 9);
    }

    #[test]
    fn the_snapshot_names_every_registered_gauge() {
        gauge("snapshot-a").set(3);
        gauge("snapshot-b").set(5);
        let snapshot = snapshot_gauges();
        assert!(snapshot.contains("snapshot-a=3"), "{snapshot}");
        assert!(snapshot.contains("snapshot-b=5"), "{snapshot}");
    }

    #[test]
    fn the_guard_trips_once_and_re_arms_below_half() {
        let limit = Some(1000);
        // Armed and crossing: trips, disarms.
        assert_eq!(guard_transition(1001, limit, true), (false, true));
        // Still above, but disarmed: no second trip.
        assert_eq!(guard_transition(1100, limit, false), (false, false));
        // Between half and the limit: the state holds.
        assert_eq!(guard_transition(600, limit, false), (false, false));
        assert_eq!(guard_transition(600, limit, true), (true, false));
        // Below half: re-arms.
        assert_eq!(guard_transition(499, limit, false), (true, false));
        // No limit: nothing ever trips.
        assert_eq!(guard_transition(10_000, None, true), (true, false));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_status_parser_reads_rss_and_swap() {
        let text =
            "Name:\therdr-gpui\nVmRSS:\t  123456 kB\nVmHWM:\t  234567 kB\nVmSwap:\t\t  1000 kB\n";
        assert_eq!(parse_status(text), Some((123456, 1000)));
        assert_eq!(parse_status("Name:\tx\nVmRSS:\t  123456 kB\n"), None);
        let (rss, _swap) = self_status().expect("this process has a /proc/self/status");
        assert!(rss > 0, "a live process has resident pages");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_holder_dump_names_this_process() {
        let dump = holder_dump();
        let self_pid = std::process::id();
        assert!(dump.contains(&format!("pid {self_pid}")), "{dump}");
    }
}

#[cfg(target_os = "linux")]
mod sampler {
    use super::{guard_transition, snapshot_gauges};
    use std::{
        collections::HashMap,
        sync::OnceLock,
        time::{Duration, Instant},
    };

    /// The sample interval is `HERDR_MEMORY_SAMPLE_SECS`, at least this.
    const MIN_SAMPLE_SECS: u64 = 5;
    /// Sample interval when the environment names none, in seconds.
    const DEFAULT_SAMPLE_SECS: u64 = 30;
    /// Soft limit on RSS + swap when the environment names none, in MiB.
    /// The machine that died had 16G total and oomd acted at 90% of both
    /// memory and swap; the app's own cgroup is capped at 3G, so trip well
    /// below that cap to leave time to read the dump before the kernel acts.
    const DEFAULT_SOFT_LIMIT_MB: u64 = 2048;
    /// Most processes the holder dump names; the tree is walked from the
    /// app outward, and the interesting growth is the app itself anyway.
    const MAX_HOLDERS: usize = 16;
    /// Most characters of a holder's command line the dump keeps.
    const MAX_HOLDER_CMD: usize = 80;

    static LIMIT_KIB: OnceLock<Option<u64>> = OnceLock::new();

    /// Starts the sampler thread: one bounded log line per interval, a
    /// warning with the holder dump when the soft limit is crossed.
    pub(super) fn install() {
        let sample_secs = std::env::var("HERDR_MEMORY_SAMPLE_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(|secs| secs.max(MIN_SAMPLE_SECS))
            .unwrap_or(DEFAULT_SAMPLE_SECS);
        let limit_kib = *LIMIT_KIB.get_or_init(|| {
            match std::env::var("HERDR_MEMORY_SOFT_LIMIT_MB")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
            {
                Some(0) => None,
                Some(mb) => Some(mb * 1024),
                None => Some(DEFAULT_SOFT_LIMIT_MB * 1024),
            }
        });
        let spawned = std::thread::Builder::new()
            .name("memory-sampler".into())
            .spawn(move || sample_loop(sample_secs, limit_kib));
        match spawned {
            Ok(_) => tracing::info!(
                target: "herdr_gpui::memory",
                sample_secs,
                soft_limit_mb = limit_kib.map(|kib| kib / 1024),
                "memory sampler started"
            ),
            Err(error) => tracing::warn!(
                target: "herdr_gpui::memory",
                %error,
                "memory sampler did not start"
            ),
        }
    }

    fn sample_loop(sample_secs: u64, limit_kib: Option<u64>) {
        let started = Instant::now();
        let mut high_water_kib: u64 = 0;
        let mut armed = true;
        loop {
            std::thread::sleep(Duration::from_secs(sample_secs));
            let Some((rss_kib, swap_kib)) = self_status() else {
                continue;
            };
            let total_kib = rss_kib.saturating_add(swap_kib);
            let new_high = total_kib > high_water_kib;
            high_water_kib = high_water_kib.max(total_kib);
            let (armed_now, tripped) = guard_transition(total_kib, limit_kib, armed);
            armed = armed_now;
            let gauges = if new_high {
                snapshot_gauges()
            } else {
                String::new()
            };
            tracing::info!(
                target: "herdr_gpui::memory",
                rss_mb = rss_kib / 1024,
                swap_mb = swap_kib / 1024,
                high_mb = high_water_kib / 1024,
                up_min = started.elapsed().as_secs() / 60,
                gauges = gauges.as_str(),
                new_high,
                "memory sample"
            );
            if tripped {
                let limit_mb = limit_kib.unwrap_or_default() / 1024;
                let holders = holder_dump();
                tracing::warn!(
                    target: "herdr_gpui::memory",
                    rss_mb = rss_kib / 1024,
                    swap_mb = swap_kib / 1024,
                    limit_mb,
                    high_mb = high_water_kib / 1024,
                    gauges = gauges.as_str(),
                    holders = holders.as_str(),
                    "memory soft limit crossed; the next oomd kill would name these"
                );
            }
        }
    }

    /// The process's own `VmRSS` and `VmSwap`, in KiB.
    pub(super) fn self_status() -> Option<(u64, u64)> {
        let text = std::fs::read_to_string("/proc/self/status").ok()?;
        parse_status(&text)
    }

    /// `VmRSS` and `VmSwap` out of a `/proc/<pid>/status` text, in KiB.
    pub(super) fn parse_status(text: &str) -> Option<(u64, u64)> {
        let mut rss = None;
        let mut swap = None;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                rss = kilobytes(rest);
            } else if let Some(rest) = line.strip_prefix("VmSwap:") {
                swap = kilobytes(rest);
            }
            if rss.is_some() && swap.is_some() {
                break;
            }
        }
        Some((rss?, swap?))
    }

    fn kilobytes(field: &str) -> Option<u64> {
        field.trim().trim_end_matches("kB").trim().parse().ok()
    }

    /// The app's process tree, self first then descendants, as one bounded
    /// line: the pid, its RSS in MiB, and a trimmed command line per holder.
    pub(super) fn holder_dump() -> String {
        let mut parents: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut residents: HashMap<u32, u64> = HashMap::new();
        let mut commands: HashMap<u32, String> = HashMap::new();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return String::from("(proc unreadable)");
        };
        for entry in entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            if let Some((ppid, resident_pages)) = process_facts(pid) {
                parents.entry(ppid).or_default().push(pid);
                // statm counts resident pages; the machine's page size is
                // 4 KiB on every platform this app ships on, and a stray
                // larger page would only inflate a display in MiB.
                residents.insert(pid, resident_pages * 4);
            }
            if let Ok(cmd) = std::fs::read_to_string(format!("/proc/{pid}/cmdline")) {
                let cmd = cmd.replace('\0', " ");
                commands.insert(
                    pid,
                    cmd.trim().chars().take(MAX_HOLDER_CMD).collect::<String>(),
                );
            }
        }
        let mut holders = Vec::new();
        let mut queue = vec![std::process::id()];
        while let Some(pid) = queue.pop() {
            let rss_mb = residents.get(&pid).map(|kib| kib / 1024).unwrap_or(0);
            let cmd = commands.get(&pid).map(String::as_str).unwrap_or("?");
            holders.push(format!("pid {pid} rss={rss_mb}MB {cmd}"));
            if holders.len() >= MAX_HOLDERS {
                break;
            }
            if let Some(children) = parents.get(&pid) {
                queue.extend(children.iter().rev());
            }
        }
        holders.join(" | ")
    }

    /// A pid's parent (out of `/proc/<pid>/stat`) and resident pages (out
    /// of `statm`): `None` when the process is gone or not ours to read.
    fn process_facts(pid: u32) -> Option<(u32, u64)> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The comm field may hold spaces, so parsing starts after its `)`;
        // after it, field 0 is the state and field 1 the ppid.
        let rest = &stat[stat.rfind(')')? + 2..];
        let fields: Vec<&str> = rest.split_whitespace().collect();
        let ppid: u32 = fields.get(1)?.parse().ok()?;
        let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
        let resident: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        Some((ppid, resident))
    }
}

/// Starts the memory sampler. A no-op where there is no `/proc` to read.
pub fn install() {
    #[cfg(target_os = "linux")]
    sampler::install();
}
