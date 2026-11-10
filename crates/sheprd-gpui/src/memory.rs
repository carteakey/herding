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
//!
//! Scope lesson from the 2026-11-06 live box (issue #12 correction): oomd
//! acts on the session scope, not the app - the app process sat at ~156 MB
//! while its scope held 5.2 GB + 277 MB swap, and the top holders (zen
//! browser, opencode, hermes) are SIBLINGS of the app in the cgroup, not
//! descendants. The sampler therefore reads the enclosing cgroup's
//! `memory.current` + `memory.swap.current` and names the top RSS holders
//! across the whole cgroup in every sample. The unit is deliberately NOT
//! capped: the cap made the blast radius worse - when oomd acted on the
//! capped unit it took 437 processes with it, growth and neighbours alike.
//! On cgroup-v1 systems the aggregate read is unavailable and the holder
//! dump falls back to the app's own process tree.

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
    use super::sampler::{
        format_holders, holder_dump, parse_cgroup_path, parse_status, self_cgroup, self_status,
    };
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
    fn the_holder_dump_is_bounded_and_well_formed() {
        let dump = holder_dump();
        // Self is no longer guaranteed a slot: the top holders are the
        // cgroup's, and a bigger sibling can crowd the app out. What must
        // hold is that the line is non-empty and every entry is formed.
        assert!(
            !dump.is_empty(),
            "the cgroup scan names somebody, self included"
        );
        for holder in dump.split(" | ") {
            let rest = holder
                .strip_prefix("pid ")
                .unwrap_or_else(|| panic!("every holder starts with a pid, got {holder:?}"));
            let (pid, rest) = rest
                .split_once(' ')
                .unwrap_or_else(|| panic!("every holder carries an rss field, got {holder:?}"));
            assert!(pid.parse::<u32>().is_ok(), "{holder:?}");
            let rss = rest
                .strip_prefix("rss=")
                .unwrap_or_else(|| panic!("every holder carries rss=MiB, got {holder:?}"));
            let token = rss.split(' ').next().unwrap_or_default();
            let mb = token
                .strip_suffix("MB")
                .unwrap_or_else(|| panic!("rss is in MiB, got {holder:?}"));
            assert!(mb.parse::<u64>().is_ok(), "{holder:?}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_cgroup_parser_reads_the_unified_hierarchy() {
        // cgroup v2: one unified line.
        assert_eq!(
            parse_cgroup_path("0::/user.slice/user-1000.slice/session-26687.scope\n"),
            Some("/user.slice/user-1000.slice/session-26687.scope".into())
        );
        // The root cgroup is a real path, not a missing one.
        assert_eq!(parse_cgroup_path("0::/\n"), Some("/".into()));
        // cgroup v1 hierarchies carry controllers; without a unified line
        // the aggregate read is unavailable and the sampler falls back.
        assert_eq!(
            parse_cgroup_path("12:memory:/system.slice/app\n11:cpu:/system.slice/app\n"),
            None
        );
        assert_eq!(parse_cgroup_path(""), None);
        let cgroup = self_cgroup().expect("this process has a /proc/self/cgroup");
        assert!(cgroup.starts_with('/'), "{cgroup}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_holder_summary_sorts_by_rss_and_stays_bounded() {
        let mb = |mib: u64| mib * 1024;
        let holders = vec![
            (mb(100), 1, String::new()),       // dropped by the cap
            (mb(400), 9, String::from("tie")), // rss ties break by pid, kept
            (mb(400), 2, String::from("tie2")),
            (mb(300), 7, String::from("third with a tail")),
            (mb(600), 4, String::from("first")),
            (mb(200), 5, String::new()), // no cmdline -> ?
            (mb(500), 3, String::new()),
        ];
        let line = format_holders(holders);
        assert_eq!(
            line,
            "pid 4 rss=600MB first | pid 3 rss=500MB ? | pid 2 rss=400MB tie2 | \
             pid 9 rss=400MB tie | pid 7 rss=300MB third with a tail"
        );
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
    /// Most holders the dump names, in every sample and on the trip: the
    /// cgroup's top RSS processes. The aggregate growth lives among them,
    /// and the line stays bounded.
    const TOP_HOLDERS: usize = 5;
    /// Most characters of a holder's command line the dump keeps.
    const MAX_HOLDER_CMD: usize = 80;

    static LIMIT_KIB: OnceLock<Option<u64>> = OnceLock::new();
    static CGROUP_CURRENT: OnceLock<super::Gauge> = OnceLock::new();
    static CGROUP_SWAP: OnceLock<super::Gauge> = OnceLock::new();

    /// The enclosing cgroup's `memory.current` + `memory.swap.current` as
    /// `path (bytes)`, when the unified hierarchy is readable.
    pub(super) fn cgroup_usage() -> Option<(String, u64, u64)> {
        let path = self_cgroup()?;
        let (current, swap) = cgroup_usage_at(&path)?;
        Some((path, current, swap))
    }

    fn cgroup_usage_at(path: &str) -> Option<(u64, u64)> {
        let current = parse_cgroup_file(
            &std::fs::read_to_string(format!("/sys/fs/cgroup{path}/memory.current")).ok()?,
        )?;
        // The swap file is absent where swap is off; that is a zero, not a
        // reason to drop the current read.
        let swap = std::fs::read_to_string(format!("/sys/fs/cgroup{path}/memory.swap.current"))
            .ok()
            .and_then(|text| parse_cgroup_file(&text))
            .unwrap_or(0);
        Some((current, swap))
    }

    /// The unified-hierarchy path out of a `/proc/<pid>/cgroup` text.
    /// cgroup v1 hosts have no unified line, so the aggregate read is
    /// unavailable there and the holder dump falls back to the app's tree.
    pub(super) fn parse_cgroup_path(text: &str) -> Option<String> {
        let line = text.lines().find_map(|line| line.strip_prefix("0::"))?;
        let path = line.trim();
        if path.is_empty() {
            None
        } else {
            Some(path.to_string())
        }
    }

    /// The process's own unified-hierarchy cgroup path.
    pub(super) fn self_cgroup() -> Option<String> {
        parse_cgroup_path(&std::fs::read_to_string("/proc/self/cgroup").ok()?)
    }

    fn cgroup_of(pid: u32) -> Option<String> {
        parse_cgroup_path(&std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?)
    }

    fn parse_cgroup_file(text: &str) -> Option<u64> {
        text.trim().parse().ok()
    }

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
                target: "sheprd_gpui::memory",
                sample_secs,
                soft_limit_mb = limit_kib.map(|kib| kib / 1024),
                "memory sampler started"
            ),
            Err(error) => tracing::warn!(
                target: "sheprd_gpui::memory",
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
            // The aggregate oomd actually acts on: the enclosing cgroup,
            // siblings included. Absent on cgroup v1.
            let cgroup = cgroup_usage();
            if let Some((_, current, swap)) = &cgroup {
                CGROUP_CURRENT
                    .get_or_init(|| super::gauge("cgroup-memory-current"))
                    .set(*current);
                CGROUP_SWAP
                    .get_or_init(|| super::gauge("cgroup-memory-swap"))
                    .set(*swap);
            }
            let holders = holder_dump();
            let (cgroup_mb, cgroup_swap_mb) = match &cgroup {
                Some((_, current, swap)) => (Some(current / 1024 / 1024), Some(swap / 1024 / 1024)),
                None => (None, None),
            };
            tracing::info!(
                target: "sheprd_gpui::memory",
                rss_mb = rss_kib / 1024,
                swap_mb = swap_kib / 1024,
                high_mb = high_water_kib / 1024,
                cgroup_mb,
                cgroup_swap_mb,
                holders = holders.as_str(),
                up_min = started.elapsed().as_secs() / 60,
                gauges = gauges.as_str(),
                new_high,
                "memory sample"
            );
            if tripped {
                let limit_mb = limit_kib.unwrap_or_default() / 1024;
                tracing::warn!(
                    target: "sheprd_gpui::memory",
                    rss_mb = rss_kib / 1024,
                    swap_mb = swap_kib / 1024,
                    limit_mb,
                    high_mb = high_water_kib / 1024,
                    cgroup_mb,
                    cgroup_swap_mb,
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

    /// The top RSS holders of the process's enclosing cgroup, as one
    /// bounded line: pid, RSS in MiB, and a trimmed command line per
    /// holder. The interesting growth is usually the app's SIBLINGS - the
    /// browser, the agents, the builds oomd counts against the same scope -
    /// so the scan matches cgroup membership, not the app's process tree.
    /// When the cgroup is unreadable (cgroup v1, hidepid) it falls back to
    /// the app's own tree.
    pub(super) fn holder_dump() -> String {
        match self_cgroup() {
            Some(path) => {
                let line = cgroup_holder_dump(&path);
                if line.is_empty() {
                    tree_holder_dump()
                } else {
                    line
                }
            }
            None => tree_holder_dump(),
        }
    }

    fn cgroup_holder_dump(self_path: &str) -> String {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return String::from("(proc unreadable)");
        };
        let mut holders: Vec<(u64, u32, String)> = Vec::new();
        for entry in entries.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            // The cgroup check is one read and filters out every unrelated
            // process before the costlier per-pid reads run.
            if cgroup_of(pid).as_deref() != Some(self_path) {
                continue;
            }
            let Some(resident_kib) = resident_kib(pid) else {
                continue;
            };
            let cmd = std::fs::read_to_string(format!("/proc/{pid}/cmdline"))
                .map(|text| text.replace('\0', " "))
                .unwrap_or_default();
            let cmd: String = cmd.trim().chars().take(MAX_HOLDER_CMD).collect();
            holders.push((resident_kib, pid, cmd));
        }
        format_holders(holders)
    }

    /// The app's process tree, self first then descendants, as one bounded
    /// line: the fallback where cgroup membership cannot be read.
    fn tree_holder_dump() -> String {
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
            if let Some((ppid, kib)) = process_facts(pid) {
                parents.entry(ppid).or_default().push(pid);
                residents.insert(pid, kib);
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
            if holders.len() >= TOP_HOLDERS {
                break;
            }
            if let Some(children) = parents.get(&pid) {
                queue.extend(children.iter().rev());
            }
        }
        holders.join(" | ")
    }

    /// A pid's resident memory in KiB, out of `statm`: `None` when the
    /// process is gone or not ours to read.
    fn resident_kib(pid: u32) -> Option<u64> {
        let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
        let resident: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        Some(resident * 4)
    }

    /// The bounded `pid rss=..MB cmd` summary, largest RSS first, at most
    /// [`TOP_HOLDERS`] entries, empty command lines shown as `?`. Pure so
    /// the ordering, the cap, and the trimming are testable without `/proc`.
    pub(super) fn format_holders(holders: Vec<(u64, u32, String)>) -> String {
        let mut holders = holders;
        holders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        holders.truncate(TOP_HOLDERS);
        holders
            .iter()
            .map(|(kib, pid, cmd)| {
                let cmd = if cmd.is_empty() { "?" } else { cmd.as_str() };
                format!("pid {pid} rss={}MB {cmd}", kib / 1024)
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// A pid's parent (out of `/proc/<pid>/stat`) and resident KiB (out of
    /// `statm`): `None` when the process is gone or not ours to read.
    fn process_facts(pid: u32) -> Option<(u32, u64)> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The comm field may hold spaces, so parsing starts after its `)`;
        // after it, field 0 is the state and field 1 the ppid.
        let rest = &stat[stat.rfind(')')? + 2..];
        let fields: Vec<&str> = rest.split_whitespace().collect();
        let ppid: u32 = fields.get(1)?.parse().ok()?;
        let kib = resident_kib(pid)?;
        Some((ppid, kib))
    }
}

/// Starts the memory sampler. A no-op where there is no `/proc` to read.
pub fn install() {
    #[cfg(target_os = "linux")]
    sampler::install();
}
