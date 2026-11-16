//! Which agent runs in a herdr-core terminal. herdr-core has no notion of an
//! agent: it passes on what the program reports (OSC 7501) and which process
//! is in front, and the policy that names the agent is `sheprd-client`'s.

use sheprd_backend::{AgentStatus, RunState, TerminalInfo};
use sheprd_client::herdr::AgentKind;

/// The agent `info`'s terminal runs, by its report, else its foreground
/// process; none for a shell or any other program, and none once the
/// program is gone: a lost or finished terminal keeps the last report.
pub fn agent_kind(info: &TerminalInfo) -> Option<AgentKind> {
    if info.state != RunState::Running {
        return None;
    }
    let (name, argv) = info.foreground.as_ref().map_or(("", &[][..]), |process| {
        (process.name.as_str(), process.argv.as_slice())
    });
    AgentKind::identify(info.reporter.as_deref(), name, argv)
}

/// The longest agent name a program may report for itself, as shown.
const MAX_NAME: usize = 32;

/// What to call the agent in `info`'s terminal: the name of the agent it is
/// identified as, else, for a program Herding does not know that reports a
/// status of its own, the name it reports, as safe display text. None for a
/// shell or any program that reports nothing, or once the program is gone.
pub fn agent_name(info: &TerminalInfo) -> Option<String> {
    if info.state != RunState::Running {
        return None;
    }
    if let Some(kind) = agent_kind(info) {
        return Some(kind.name().to_owned());
    }
    let reported = info.reporter.as_deref()?;
    let name = sheprd_client::text::safe_text(reported, MAX_NAME);
    let name = name.trim();
    (!name.is_empty() && info.agent != AgentStatus::Unknown).then(|| name.to_owned())
}
