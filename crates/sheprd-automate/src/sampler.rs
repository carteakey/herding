//! Measuring the agents automations started, once a minute: tokens from
//! their session files, CPU and memory from their panes' processes. An
//! agent counts as done, and stops counting toward the host's agent cap,
//! when its pane closes, its shell is back at the prompt, Herdr has called
//! it idle or done for ten minutes, or a day has passed. On herdr-core, its
//! terminal gone or finished is its pane closed. A read that fails for any
//! other reason, such as Herdr not answering, proves nothing: the agent
//! keeps counting and is read again next time.

use crate::{
    drive::Drive,
    herdr::{self, Herdr},
    herdr_core::HerdrCore,
    logbook::Logbook,
    paths::Paths,
    process::Meter,
    runner::{now, start_of_day},
    store::{AgentRow, Store},
    usage,
};
use sheprd_automation::report::Terminals;
use sheprd_core_client::wire::{StatusState, Terminal, TerminalState};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

pub const INTERVAL: Duration = Duration::from_secs(60);
const IDLE_DONE: i64 = 600;
const AT_MOST: i64 = 86_400;

/// Samples until `stop` is set; checks it every second.
pub fn run(paths: &Paths, drive: &Drive, log: &Logbook, stop: &Arc<AtomicBool>) {
    let mut sampler = Sampler::default();
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    loop {
        match Store::open(&paths.database()) {
            Ok(store) => {
                if let Err(error) = sampler.sample(&store, drive, &home) {
                    log.warn(format!("sampling agents: {error}"));
                }
            }
            Err(error) => log.warn(format!("sampling agents: {error}")),
        }
        for _ in 0..INTERVAL.as_secs() {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }
}

#[derive(Default)]
struct Sampler {
    meters: HashMap<(i64, u32), Meter>,
    idle_since: HashMap<(i64, u32), i64>,
}

/// What one look at an agent's pane found.
#[derive(Default)]
struct Look {
    /// Its pane closed, or its program ended.
    gone: bool,
    /// Its agent said it is idle or done.
    quiet: bool,
    /// The process its CPU and memory are measured from.
    root: Option<u32>,
    session: Option<herdr::Session>,
}

impl Sampler {
    fn sample(
        &mut self,
        store: &Store,
        drive: &Drive,
        home: &std::path::Path,
    ) -> crate::Result<()> {
        let now = now();
        let day = start_of_day();
        let agents = store.agents(true)?;
        // One list serves every herdr-core agent; failing, it proves nothing.
        let terminals = match drive {
            Drive::HerdrCore(core)
                if agents
                    .iter()
                    .any(|agent| agent.terminals == Terminals::HerdrCore) =>
            {
                listed(core)
            }
            _ => None,
        };
        for mut agent in agents {
            let key = (agent.run, agent.position);
            let look = match (agent.terminals, drive) {
                (Terminals::Herdr, Drive::Herdr(herdr)) => look_herdr(herdr, &agent.pane),
                (Terminals::HerdrCore, _) => terminals
                    .as_deref()
                    .map(|terminals| look_core(terminals, &agent.pane))
                    .unwrap_or_default(),
                // The host moved to another Herdr since the agent started:
                // nothing here can see it, so only the day's limit ends it.
                (Terminals::Herdr, Drive::HerdrCore(_)) => Look::default(),
            };
            if let Some(tokens) = usage::tokens(
                &agent.harness,
                &agent.cwd,
                agent.started_at,
                look.session.as_ref(),
                home,
            ) {
                agent.tokens = Some(tokens);
            }
            if let Some(root) = look.root {
                let meter = self.meters.entry(key).or_insert_with(|| Meter::new(root));
                meter.sample();
                agent.cpu_ms = meter.cpu_ms().or(agent.cpu_ms);
                agent.peak_memory = meter.peak_memory().max(agent.peak_memory);
            }
            if look.quiet {
                self.idle_since.entry(key).or_insert(now);
            } else {
                self.idle_since.remove(&key);
            }
            let idle_long = self
                .idle_since
                .get(&key)
                .is_some_and(|since| now - since >= IDLE_DONE);
            agent.sampled_at = Some(now);
            if look.gone || idle_long || now - agent.started_at >= AT_MOST {
                agent.done_at = Some(now);
                self.forget(key);
            }
            store.sample_agent(&agent, day)?;
        }
        Ok(())
    }

    fn forget(&mut self, key: (i64, u32)) {
        self.meters.remove(&key);
        self.idle_since.remove(&key);
    }
}

/// A Herdr 1 pane: closed, or its shell back at the prompt, is gone.
fn look_herdr(herdr: &Herdr, pane: &str) -> Look {
    let agent = herdr.pane_agent(pane).ok();
    let process = herdr.process_info(pane);
    let gone = match &process {
        Ok(process) => process.at_prompt(),
        Err(herdr::Error::PaneNotFound { .. }) => true,
        Err(_) => false,
    };
    Look {
        gone,
        quiet: agent
            .as_ref()
            .and_then(|agent| agent.agent_status.as_deref())
            .is_some_and(|status| matches!(status, "idle" | "done")),
        root: process.ok().and_then(|process| process.shell_pid),
        session: agent.and_then(|agent| agent.agent_session),
    }
}

/// Every terminal the core keeps, or `None` when it cannot be read.
fn listed(core: &HerdrCore) -> Option<Vec<Terminal>> {
    core.terminals().ok()
}

/// A herdr-core terminal: removed, finished, or lost is gone. An agent's
/// terminal is kept, so a program that dies leaves it live with no process
/// and the reason it was lost. herdr-core reports no agent session, so
/// tokens come from the agent's recent files in its folder.
fn look_core(terminals: &[Terminal], address: &str) -> Look {
    let Some(terminal) = terminals
        .iter()
        .find(|terminal| terminal.address == address)
    else {
        return Look {
            gone: true,
            ..Look::default()
        };
    };
    Look {
        gone: terminal.state == TerminalState::Finished
            || (terminal.pid.is_none() && terminal.reason.is_some()),
        quiet: terminal
            .status
            .as_ref()
            .is_some_and(|status| matches!(status.state, StatusState::Idle | StatusState::Done)),
        root: terminal.pid.and_then(|pid| u32::try_from(pid).ok()),
        session: None,
    }
}

/// An agent's usage as reports describe it.
pub fn usage_of(agent: &AgentRow) -> sheprd_automation::report::Usage {
    sheprd_automation::report::Usage {
        tokens: agent.tokens,
        cpu_ms: agent.cpu_ms,
        peak_memory: agent.peak_memory,
    }
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests;
