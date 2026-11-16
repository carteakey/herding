use super::*;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn session(agent: &str, kind: SessionKind, value: &str) -> AgentSession {
    AgentSession {
        agent: agent.into(),
        kind,
        source: format!("herdr:{agent}"),
        value: value.into(),
    }
}

#[test]
fn resume_commands_match_each_agent() {
    let flags = strings(&["--model", "x"]);
    assert_eq!(
        AgentKind::Claude.resume_argv("id", &flags),
        strings(&["claude", "--resume", "id", "--model", "x"])
    );
    assert_eq!(
        AgentKind::Codex.resume_argv("id", &[]),
        strings(&["codex", "resume", "id"])
    );
    assert_eq!(
        AgentKind::Opencode.resume_argv("ses_1", &[]),
        strings(&["opencode", "--session", "ses_1"])
    );
    assert_eq!(
        AgentKind::Pi.resume_argv("/h/s.jsonl", &[]),
        strings(&["pi", "--session", "/h/s.jsonl"])
    );
    assert_eq!(
        AgentKind::Omp.resume_argv("/h/s.jsonl", &[]),
        strings(&["omp", "--resume=/h/s.jsonl"])
    );
}

#[test]
fn resume_commands_match_herdr_for_every_agent() {
    // Herdr's `agent_resume::plan`, agent by agent.
    for (agent, reference, argv) in [
        ("copilot", "c-1", &["copilot", "--resume=c-1"][..]),
        ("devin", "d-1", &["devin", "--resume", "d-1"]),
        ("droid", "d-1", &["droid", "--resume", "d-1"]),
        ("kimi", "k-1", &["kimi", "--session", "k-1"]),
        ("mastracode", "m-1", &["mastracode", "--thread", "m-1"]),
        ("hermes", "h-1", &["hermes", "--resume", "h-1"]),
        ("qodercli", "q-1", &["qodercli", "--resume", "q-1"]),
        ("qwen", "q-1", &["qwen", "--resume", "q-1"]),
        ("kilo", "k-1", &["kilo", "--session", "k-1"]),
        ("cursor", "c-1", &["cursor-agent", "--resume", "c-1"]),
        ("agy", "a-1", &["agy", "--conversation", "a-1"]),
        ("grok", "g-1", &["grok", "--resume", "g-1"]),
        ("letta", "conv-1", &["letta", "--conversation", "conv-1"]),
        (
            "letta",
            "default:agent-1",
            &["letta", "--conversation", "default", "--agent", "agent-1"],
        ),
    ] {
        let kind = AgentKind::parse(agent).unwrap();
        assert_eq!(kind.name(), agent);
        assert_eq!(kind.resume_argv(reference, &[]), strings(argv), "{agent}");
    }
    assert_eq!(AgentKind::parse("cursor-agent"), None);
    assert_eq!(AgentKind::parse("antigravity_cli"), None);
}

#[test]
fn sessions_are_accepted_only_from_herdr_with_safe_references() {
    let accepts = |agent: &str, source: &str, kind: SessionKind, value: &str| {
        let mut reported = session(agent, kind, value);
        reported.source = source.into();
        AgentKind::parse(agent).unwrap().accepts(&reported)
    };
    assert!(accepts(
        "agy",
        "herdr:antigravity_cli",
        SessionKind::Id,
        "a"
    ));
    assert!(!accepts("agy", "herdr:agy", SessionKind::Id, "a"));
    assert!(accepts("cursor", "herdr:cursor", SessionKind::Id, "a"));
    assert!(accepts(
        "letta",
        "herdr:letta",
        SessionKind::Id,
        "default:agent-1"
    ));
    assert!(!accepts(
        "letta",
        "herdr:letta",
        SessionKind::Id,
        "default:"
    ));
    assert!(!accepts("copilot", "herdr:copilot", SessionKind::Id, ""));
    assert!(!accepts(
        "copilot",
        "herdr:copilot",
        SessionKind::Id,
        "--yolo"
    ));
    assert!(!accepts(
        "copilot",
        "herdr:copilot",
        SessionKind::Id,
        "a\nb"
    ));
    assert!(accepts(
        "copilot",
        "herdr:copilot",
        SessionKind::Id,
        &"a".repeat(512)
    ));
    assert!(!accepts(
        "copilot",
        "herdr:copilot",
        SessionKind::Id,
        &"a".repeat(513)
    ));
    // Only pi and omp report session files, and only absolute ones.
    assert!(!accepts(
        "copilot",
        "herdr:copilot",
        SessionKind::Path,
        "/s"
    ));
    assert!(accepts("omp", "herdr:omp", SessionKind::Path, "/h/s.jsonl"));
    assert!(!accepts("omp", "herdr:omp", SessionKind::Path, "s.jsonl"));
    assert!(accepts("pi", "herdr:pi", SessionKind::Id, "01a0"));
}

#[test]
fn copilot_flags_keep_their_values() {
    assert_eq!(
        AgentKind::Copilot.kept_flags(&strings(&[
            "copilot",
            "--model",
            "gpt-5",
            "--allow-tool",
            "shell(git)",
            "write",
            "--alt-screen",
            "off",
            "--resume",
            "old",
            "--share=/tmp/s.md",
            "-i",
            "fix it",
            "--log-dir",
            "/tmp/l",
            "--continue",
            "--yolo",
        ])),
        strings(&[
            "--model",
            "gpt-5",
            "--allow-tool",
            "shell(git)",
            "write",
            "--alt-screen",
            "off",
            "--yolo"
        ])
    );
    // Flags of agents whose options are unknown are never replayed.
    assert!(
        AgentKind::Letta
            .kept_flags(&strings(&["letta", "--agent", "a", "--yolo"]))
            .is_empty()
    );
}

#[test]
fn kept_flags_drop_sessions_positionals_and_directories() {
    assert_eq!(
        AgentKind::Codex.kept_flags(&strings(&[
            "codex",
            "-C",
            "/src",
            "--cd=/src",
            "-m",
            "o3",
            "resume",
            "--yolo",
            "hi"
        ])),
        strings(&["-m", "o3", "--yolo"])
    );
    assert_eq!(
        AgentKind::Opencode.kept_flags(&strings(&[
            "opencode",
            "/src",
            "--session",
            "s",
            "-c",
            "--model=a/b"
        ])),
        strings(&["--model=a/b"])
    );
    assert!(
        AgentKind::Claude
            .kept_flags(&strings(&["claude", "--model"]))
            .is_empty()
    );
}

#[test]
fn every_kind_is_listed_once_and_parses_back() {
    for (index, kind) in AgentKind::ALL.iter().enumerate() {
        assert_eq!(AgentKind::parse(kind.name()), Some(*kind));
        assert!(!AgentKind::ALL[..index].contains(kind), "{kind:?}");
    }
}
