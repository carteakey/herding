use super::*;

fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn every_kind_is_found_by_its_own_name() {
    for kind in AgentKind::ALL {
        assert_eq!(AgentKind::lookup(kind.name()), Some(kind), "{kind:?}");
        assert_eq!(AgentKind::lookup(kind.binary()), Some(kind), "{kind:?}");
    }
}

#[test]
fn reported_apps_use_their_long_names() {
    for (app, kind) in [
        ("claude-code", AgentKind::Claude),
        ("Claude-Code", AgentKind::Claude),
        ("github-copilot", AgentKind::Copilot),
        ("cursor-agent", AgentKind::Cursor),
        ("antigravity", AgentKind::Antigravity),
    ] {
        assert_eq!(
            AgentKind::identify(Some(app), "sh", &[]),
            Some(kind),
            "{app}"
        );
    }
}

#[test]
fn the_report_wins_over_the_process() {
    assert_eq!(
        AgentKind::identify(Some("codex"), "claude", &argv(&["claude"])),
        Some(AgentKind::Codex)
    );
}

#[test]
fn an_unknown_report_falls_back_to_the_process() {
    assert_eq!(
        AgentKind::identify(Some("my-tool"), "claude", &argv(&["/opt/bin/claude", "-c"])),
        Some(AgentKind::Claude)
    );
}

#[test]
fn programs_are_named_without_folder_or_extension() {
    for (name, program) in [
        ("cursor-agent", "/usr/local/bin/cursor-agent"),
        ("codex.exe", r"C:\tools\codex.exe"),
        ("copilot", "copilot.cmd"),
        ("-claude", "-claude"),
    ] {
        assert!(
            AgentKind::identify(None, name, &argv(&[program])).is_some(),
            "{program}"
        );
    }
}

#[test]
fn a_runtime_is_looked_past_to_its_script() {
    assert_eq!(
        AgentKind::identify(
            None,
            "node",
            &argv(&["node", "--no-warnings", "/a/bin/codex"])
        ),
        Some(AgentKind::Codex)
    );
    assert_eq!(
        AgentKind::identify(None, "bun", &argv(&["bun", "/a/opencode.js"])),
        Some(AgentKind::Opencode)
    );
    assert_eq!(
        AgentKind::identify(None, "node", &argv(&["node", "server.js"])),
        None
    );
}

#[test]
fn a_package_names_an_entry_point_that_does_not() {
    for (script, kind) in [
        (
            "/opt/node_modules/@anthropic-ai/claude-code/cli.js",
            AgentKind::Claude,
        ),
        (
            r"C:\npm\node_modules\@qwen-code\qwen-code\dist\index.js",
            AgentKind::Qwen,
        ),
        (
            "/g/lib/node_modules/@openai/codex/bin/codex.js",
            AgentKind::Codex,
        ),
    ] {
        assert_eq!(
            AgentKind::identify(None, "node", &argv(&["node", script])),
            Some(kind),
            "{script}"
        );
    }
}

#[test]
fn runtime_options_and_their_values_are_not_the_script() {
    assert_eq!(
        AgentKind::identify(
            None,
            "node",
            &argv(&["node", "--require", "/work/setup.js", "/a/bin/codex"])
        ),
        Some(AgentKind::Codex)
    );
    assert_eq!(
        AgentKind::identify(None, "node", &argv(&["node", "-e", "codex"])),
        None,
        "inline code runs no script"
    );
}

#[test]
fn bun_and_deno_run_is_looked_past_to_the_script() {
    for line in [
        &["bun", "run", "/a/opencode.js"][..],
        &["deno", "run", "/a/opencode.js"],
        &["deno", "run", "-A", "/a/opencode.js"],
        &["/usr/local/bin/bun", "--smol", "run", "/a/opencode.js"],
    ] {
        assert_eq!(
            AgentKind::identify(None, line[0], &argv(line)),
            Some(AgentKind::Opencode),
            "{line:?}"
        );
    }
    // To `node`, `run` is the script itself.
    assert_eq!(
        AgentKind::identify(None, "node", &argv(&["node", "run", "/a/opencode.js"])),
        None
    );
}

#[test]
fn nodejs_is_node_by_another_name() {
    assert_eq!(
        AgentKind::identify(
            None,
            "nodejs",
            &argv(&["nodejs", "/opt/node_modules/@openai/codex/bin/codex.js"])
        ),
        Some(AgentKind::Codex)
    );
    assert_eq!(
        AgentKind::identify(
            None,
            "nodejs",
            &argv(&["/usr/bin/nodejs", "run", "/a/codex.js"])
        ),
        None,
        "to Node, `run` is the script"
    );
}

#[test]
fn shells_and_other_programs_are_not_agents() {
    for program in ["zsh", "-bash", "vim", "python3", "cargo", "node"] {
        assert_eq!(
            AgentKind::identify(None, program, &argv(&[program])),
            None,
            "{program}"
        );
    }
    // A shell running a script named like an agent is still the shell.
    assert_eq!(
        AgentKind::identify(None, "sh", &argv(&["sh", "-c", "claude"])),
        None
    );
    assert_eq!(AgentKind::identify(None, "", &[]), None);
}
