//! What the OS says about the processes behind listening sockets, and
//! signalling exactly one of them.
//!
//! A pid is reused once its process exits, so every signal first looks the
//! pid up again and goes ahead only while it still names the process the
//! user chose: the same start time, owned by this user. The signal goes to
//! that process alone, never to a group or a name, and through [`Pinned`]
//! where the OS can hold it.

use super::{Listener, Outcome, naming, pinned::Pinned, scan::Socket};
use crate::processes::Identity;
use sheprd_control_protocol::protocol::hosts::PortAction;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};
use sysinfo::{
    Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, Signal, System, UpdateKind,
};

/// A process name is display text; longer ones are cut.
const NAME_LIMIT: usize = 64;
/// A command line is display text; longer ones are cut.
const COMMAND_LIMIT: usize = 512;
/// Arguments read to name a process.
const ARGUMENT_LIMIT: usize = 64;
/// How often a stopping process is looked at while it has time to exit.
const POLL: Duration = Duration::from_millis(100);

/// This process's real user, whose processes alone are listed and signalled.
pub(super) fn user() -> u32 {
    rustix::process::getuid().as_raw()
}

/// The process `pid` names now, as far as signalling cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    pub started: u64,
    /// `None` when the OS would not say.
    pub user: Option<u32>,
    /// Exited, and only waiting for its parent to reap it.
    pub zombie: bool,
}

/// Whether `found` is still `victim`, a process of `user` that is running.
pub(crate) fn check(found: Option<Found>, victim: Identity, user: u32) -> Result<(), Outcome> {
    let found = found.ok_or(Outcome::Exited)?;
    if found.zombie || found.started != victim.started {
        return Err(Outcome::Exited);
    }
    if found.user.is_some_and(|owner| owner != user) {
        return Err(Outcome::Foreign);
    }
    Ok(())
}

/// Reads `pids` afresh. A new table every time: macOS `sysinfo` keeps the
/// state it first saw for a process it already knows, so a pause or an exit
/// since would go unseen.
fn refresh(system: &mut System, pids: &[Pid]) {
    *system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(pids),
        true,
        ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::Always)
            .with_user(UpdateKind::OnlyIfNotSet),
    );
}

fn found(system: &System, pid: u32) -> Option<Found> {
    let process = system.process(Pid::from_u32(pid))?;
    Some(Found {
        started: process.start_time(),
        user: process.user_id().map(|uid| **uid),
        zombie: matches!(
            process.status(),
            ProcessStatus::Zombie | ProcessStatus::Dead
        ),
    })
}

/// CPU and memory of the listening processes, from one table kept across
/// scans: a process's CPU is a rate between two of its samples.
#[derive(Default)]
pub(super) struct Sampler {
    system: System,
    /// The processes the previous scan sampled.
    seen: HashSet<Identity>,
}

impl Sampler {
    /// Samples `pids`; for each identity, its CPU once it was sampled
    /// before, and its resident memory.
    fn sample(&mut self, pids: &[Pid]) -> HashMap<Identity, (Option<f32>, u64)> {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(pids),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let previous = std::mem::take(&mut self.seen);
        let mut usage = HashMap::new();
        for pid in pids {
            let Some(process) = self.system.process(*pid) else {
                continue;
            };
            let identity = Identity {
                pid: pid.as_u32(),
                started: process.start_time(),
            };
            let cpu = previous
                .contains(&identity)
                .then(|| process.cpu_usage().max(0.));
            self.seen.insert(identity);
            usage.insert(identity, (cpu, process.memory()));
        }
        usage
    }
}

fn display(text: &str, limit: usize) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(limit)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// The rows `sockets` make: each with its process's identity, names,
/// working directory and project, CPU and memory, and state, leaving out
/// sockets whose process exited since or belongs to another user.
pub(super) fn describe(
    system: &mut System,
    sampler: &mut Sampler,
    sockets: Vec<Socket>,
    user: u32,
) -> Vec<Listener> {
    let mut pids: Vec<Pid> = sockets
        .iter()
        .map(|socket| Pid::from_u32(socket.pid))
        .collect();
    pids.sort_unstable();
    pids.dedup();
    *system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::Always)
            .with_cmd(UpdateKind::Always)
            .with_exe(UpdateKind::Always)
            .with_user(UpdateKind::Always),
    );
    let usage = sampler.sample(&pids);
    // Projects are looked up once per directory, not once per socket.
    let mut projects: HashMap<PathBuf, Option<String>> = HashMap::new();
    sockets
        .into_iter()
        .filter_map(|socket| {
            let process = system.process(Pid::from_u32(socket.pid))?;
            if process.user_id().is_some_and(|uid| **uid != user) {
                return None;
            }
            let identity = Identity {
                pid: socket.pid,
                started: process.start_time(),
            };
            // `lsof` cuts names at nine characters; the OS's own is longer.
            let own = display(&process.name().to_string_lossy(), NAME_LIMIT);
            let process_name = if own.is_empty() { socket.process } else { own };
            let arguments: Vec<String> = process
                .cmd()
                .iter()
                .take(ARGUMENT_LIMIT)
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect();
            let directory = process.cwd().map(Path::to_path_buf);
            let project = directory.as_ref().and_then(|directory| {
                projects
                    .entry(directory.clone())
                    .or_insert_with(|| naming::project(directory))
                    .clone()
            });
            let (cpu, memory) = usage.get(&identity).copied().unwrap_or_default();
            Some(Listener {
                identity,
                number: socket.number,
                bind: socket.bind,
                name: naming::name(&process_name, process.exe(), &arguments),
                command: display(&arguments.join(" "), COMMAND_LIMIT),
                system: naming::system(&process_name),
                process: process_name,
                directory,
                project,
                cpu,
                memory,
                paused: process.status() == ProcessStatus::Stop,
            })
        })
        .collect()
}

/// Looks `pid` up afresh.
fn look(system: &mut System, pid: u32) -> Option<Found> {
    refresh(system, &[Pid::from_u32(pid)]);
    found(system, pid)
}

fn send(system: &System, pid: u32, signal: Signal) -> bool {
    system
        .process(Pid::from_u32(pid))
        .and_then(|process| process.kill_with(signal))
        .unwrap_or(false)
}

/// Does `action` to each of `victims` that is still that process. Blocking:
/// a worker's call.
pub(super) fn act(
    system: &mut System,
    action: PortAction,
    victims: &[Identity],
    user: u32,
    grace: Duration,
) -> Vec<(Identity, Outcome)> {
    // Held before the first look, so what the look proves of a pid is true
    // of the process every later signal goes to.
    let pinned: HashMap<Identity, Pinned> = victims
        .iter()
        .filter_map(|victim| Some((*victim, Pinned::open(victim.pid)?)))
        .collect();
    let system = RefCell::new(system);
    act_with(
        action,
        victims,
        user,
        grace,
        |victim| look(&mut system.borrow_mut(), victim.pid),
        |victim, signal| match pinned.get(&victim) {
            Some(process) => process.send(signal),
            None => send(&system.borrow(), victim.pid, signal),
        },
        thread::sleep,
    )
}

/// [`act`]'s steps, with the OS behind `look` (the pid's process now),
/// `send`, and `wait`. Every signal goes out only once a fresh look shows
/// the pid still names its victim. Stopping sends SIGTERM to them all, then
/// SIGCONT so a paused one can act on it, and SIGKILL to those still running
/// after one shared `grace`, so a batch takes no longer than one process. A
/// process that exits and whose pid is reused in between is never resumed
/// or killed.
pub(crate) fn act_with(
    action: PortAction,
    victims: &[Identity],
    user: u32,
    grace: Duration,
    mut look: impl FnMut(Identity) -> Option<Found>,
    mut send: impl FnMut(Identity, Signal) -> bool,
    mut wait: impl FnMut(Duration),
) -> Vec<(Identity, Outcome)> {
    let signal = match action {
        PortAction::Stop => Signal::Term,
        PortAction::Pause => Signal::Stop,
        PortAction::Resume => Signal::Continue,
    };
    let mut outcomes: Vec<(Identity, Outcome)> = victims
        .iter()
        .map(|&victim| {
            let outcome = match check(look(victim), victim, user) {
                Err(outcome) => outcome,
                Ok(()) if send(victim, signal) => Outcome::Signalled,
                Ok(()) => Outcome::Failed,
            };
            (victim, outcome)
        })
        .collect();
    if action != PortAction::Stop {
        return outcomes;
    }
    let mut running: Vec<usize> = (0..outcomes.len())
        .filter(|&index| outcomes[index].1 == Outcome::Signalled)
        .collect();
    running.retain(|&index| {
        let victim = outcomes[index].0;
        let alive = check(look(victim), victim, user).is_ok();
        if alive {
            send(victim, Signal::Continue);
        }
        alive
    });
    let mut waited = Duration::ZERO;
    while !running.is_empty() && waited < grace {
        wait(POLL);
        waited += POLL;
        running.retain(|&index| {
            let victim = outcomes[index].0;
            check(look(victim), victim, user).is_ok()
        });
    }
    for index in running {
        let victim = outcomes[index].0;
        if check(look(victim), victim, user).is_ok() {
            outcomes[index].1 = if send(victim, Signal::Kill) {
                Outcome::Killed
            } else {
                Outcome::Failed
            };
        }
    }
    outcomes
}
