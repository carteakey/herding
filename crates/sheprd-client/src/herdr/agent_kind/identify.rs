//! Which agent a terminal runs when no Herdr 1 integration says so, as on
//! herdr-core: first the name the program reports for itself (OSC 7501's
//! `app`), then the process in front. Names follow Herdr's own lookup
//! (`detect::lookup_agent`), so both generations agree on what an agent is.

use super::AgentKind;

/// Runtimes that run an agent's script rather than being one.
const RUNTIMES: &[&str] = &["bun", "deno", "node", "nodejs"];
/// Runtime options whose value is the next argument, not the script.
const VALUED: &[&str] = &[
    "-r",
    "--require",
    "--import",
    "--loader",
    "--experimental-loader",
    "--env-file",
    "--title",
    "--inspect-port",
    "--conditions",
    "-C",
    "--preload",
    "--config",
];
/// Options after which the runtime runs code from the command line, so no
/// script follows.
const INLINE: &[&str] = &["-e", "--eval", "-p", "--print"];
/// Packages whose entry point is not named after the agent, such as
/// Claude Code's `cli.js`, found by their folder.
const PACKAGES: &[(&str, AgentKind)] = &[
    ("@anthropic-ai/claude-code", AgentKind::Claude),
    ("@github/copilot", AgentKind::Copilot),
    ("@letta-ai/letta-code", AgentKind::Letta),
    ("@openai/codex", AgentKind::Codex),
    ("@qwen-code/qwen-code", AgentKind::Qwen),
    ("mastracode", AgentKind::Mastracode),
    ("opencode-ai", AgentKind::Opencode),
];

impl AgentKind {
    /// The agent behind `name`: a reported app such as `claude-code`, or a
    /// program such as `/usr/local/bin/cursor-agent` or `codex.exe`.
    pub fn lookup(name: &str) -> Option<Self> {
        Some(match base(name).to_ascii_lowercase().as_str() {
            "claude" | "claude-code" => Self::Claude,
            "codex" => Self::Codex,
            "opencode" | "opencode2" | "open-code" => Self::Opencode,
            "pi" => Self::Pi,
            "omp" => Self::Omp,
            "copilot" | "github-copilot" | "ghcs" => Self::Copilot,
            "devin" | "devin-cli" => Self::Devin,
            "droid" => Self::Droid,
            "kimi" | "kimi-code" => Self::Kimi,
            "mastracode" | "mastra-code" => Self::Mastracode,
            "hermes" | "hermes-agent" => Self::Hermes,
            "qodercli" | "qoderclicn" | "qoder" | "qodercn" => Self::Qodercli,
            "qwen" | "qwen-code" => Self::Qwen,
            "kilo" | "kilo-code" => Self::Kilo,
            "cursor" | "cursor-agent" => Self::Cursor,
            "agy" | "antigravity" | "antigravity-cli" => Self::Antigravity,
            "grok" | "grok-build" => Self::Grok,
            "letta" | "letta-code" => Self::Letta,
            _ => return None,
        })
    }

    /// The agent a terminal runs: the one its program reports being, else
    /// the one its foreground process is, looking past a runtime such as
    /// `node` to the script it runs. Shells and other programs are none.
    pub fn identify(reporter: Option<&str>, name: &str, argv: &[String]) -> Option<Self> {
        if let Some(kind) = reporter.and_then(Self::lookup) {
            return Some(kind);
        }
        let program = argv.first().map_or(name, String::as_str);
        if let Some(kind) = Self::lookup(program).or_else(|| Self::lookup(name)) {
            return Some(kind);
        }
        let runtime = base(program).to_ascii_lowercase();
        if !RUNTIMES.contains(&runtime.as_str()) {
            return None;
        }
        // `bun run x.js` and `deno run x.js`; to Node, `run` is a script.
        let subcommand = !matches!(runtime.as_str(), "node" | "nodejs");
        let script = script(argv, subcommand)?;
        package(script).or_else(|| Self::lookup(script))
    }
}

/// The script a runtime's command line runs: the first argument that is
/// neither an option nor an option's value, past a leading `run` when the
/// runtime takes `subcommand`s.
fn script(argv: &[String], subcommand: bool) -> Option<&str> {
    let mut args = argv.iter().skip(1);
    let mut first = true;
    while let Some(arg) = args.next() {
        if !arg.starts_with('-') {
            if subcommand && first && arg == "run" {
                first = false;
                continue;
            }
            return Some(arg);
        }
        if INLINE.contains(&arg.as_str()) {
            return None;
        }
        if VALUED.contains(&arg.as_str()) {
            args.next();
        }
    }
    None
}

/// The agent whose package folder holds `script`.
fn package(script: &str) -> Option<AgentKind> {
    let script = script.replace('\\', "/");
    PACKAGES
        .iter()
        .find(|(folder, _)| script.contains(&format!("/node_modules/{folder}/")))
        .map(|(_, kind)| *kind)
}

/// A program's name without its folder, a login shell's leading dash, or a
/// Windows or script extension.
fn base(name: &str) -> &str {
    let name = name.trim();
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let name = name.trim_start_matches('-');
    [".exe", ".cmd", ".js", ".mjs", ".cjs"]
        .iter()
        .find_map(|extension| {
            let cut = name.len().checked_sub(extension.len())?;
            name.get(cut..)
                .filter(|tail| tail.eq_ignore_ascii_case(extension))
                .and_then(|_| name.get(..cut))
        })
        .unwrap_or(name)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
