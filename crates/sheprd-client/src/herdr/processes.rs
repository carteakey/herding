//! Which of a pane's foreground processes run a given agent.

use super::snapshot::{Process, ProcessInfo};
use sheprd_chat_protocol::Agent;

/// The most processes matching an agent that are worth telling apart: past
/// these the pane runs something no single session explains.
const MAX_MATCHING_PROCESSES: usize = 16;

/// The foreground processes running `agent`, unless there are more than
/// [`MAX_MATCHING_PROCESSES`] to verify.
pub fn agent_processes(agent: Agent, info: &ProcessInfo) -> Option<Vec<&Process>> {
    let matching: Vec<_> = info
        .foreground_processes
        .iter()
        .filter(|process| is_agent(agent, &process.argv))
        .take(MAX_MATCHING_PROCESSES + 1)
        .collect();
    (matching.len() <= MAX_MATCHING_PROCESSES).then_some(matching)
}

/// The program an argument vector runs, without its directory.
pub fn program(argv: &[String]) -> Option<&str> {
    argv.first()?.rsplit('/').next()
}

/// Whether `argv` runs `agent` itself, directly or as a Node or Bun script,
/// rather than naming it as an argument to some other program.
pub fn is_agent(agent: Agent, argv: &[String]) -> bool {
    agent_args(agent, argv).is_some()
}

/// The arguments given to `agent` when `argv` runs it, natively or through
/// one of the Node or Bun launchers [`is_agent`] knows.
pub fn agent_args(agent: Agent, argv: &[String]) -> Option<&[String]> {
    let program = program(argv)?;
    if program == agent.name() {
        return Some(&argv[1..]);
    }
    if !matches!(program, "node" | "nodejs" | "bun") {
        return None;
    }
    let script = argv.get(1)?;
    let matches = match agent {
        Agent::Claude => {
            script.ends_with("/@anthropic-ai/claude-code/cli.js") || script.ends_with("/claude")
        }
        Agent::Codex => {
            script.ends_with("/@openai/codex/bin/codex.js") || script.ends_with("/codex")
        }
        _ => false,
    };
    matches.then_some(&argv[2..])
}

#[cfg(test)]
mod tests;
