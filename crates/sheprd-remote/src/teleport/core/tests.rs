use super::*;
use sheprd_backend::TerminalId;
use sheprd_session::panes::Share;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod destination;

fn terminal(json: &str) -> Terminal {
    serde_json::from_str(json).unwrap()
}

#[test]
fn an_idle_shell_is_a_shell() {
    // What herdr-core 0.1.17 reports for a shell at its prompt.
    let idle = terminal(
        r#"{"address":"t1","argv":["/bin/zsh","-l"],"pid":63492,"cwd":"/w",
            "foreground":{"argv":["/bin/zsh","-l"],"cwd":"/w/sub","name":"zsh","pid":63492},
            "status":null}"#,
    );
    assert_eq!(idle.work(), Work::Shell);
    assert_eq!(idle.cwd().as_deref(), Some("/w/sub"));
    // Nothing in the foreground reads the same.
    assert_eq!(terminal(r#"{"cwd":"/w"}"#).work(), Work::Shell);
    assert_eq!(terminal(r#"{"cwd":"/w"}"#).cwd().as_deref(), Some("/w"));
}

#[test]
fn agents_and_commands_are_named_by_their_program() {
    let claude = terminal(
        r#"{"argv":["/bin/zsh","-l"],"pid":1,
            "foreground":{"argv":["claude","--dangerously-skip-permissions"],"cwd":"/w","pid":2},
            "status":{"app":"claude-code","state":"idle"}}"#,
    );
    assert_eq!(
        claude.work(),
        Work::Agent {
            name: "claude".into(),
            argv: vec!["claude".into(), "--dangerously-skip-permissions".into()],
        }
    );
    // An agent run through a wrapper is known by what herdr-core recognized.
    let wrapped = terminal(
        r#"{"pid":1,"foreground":{"argv":["node","cli.js"],"pid":2},
            "status":{"app":"claude-code"}}"#,
    );
    assert!(matches!(wrapped.work(), Work::Agent { name, .. } if name == "claude"));
    let server =
        terminal(r#"{"pid":1,"foreground":{"argv":["npm","run","dev"],"pid":2},"status":null}"#);
    assert_eq!(
        server.work(),
        Work::Command(vec!["npm".into(), "run".into(), "dev".into()])
    );
    // A terminal started on a program, not a shell, runs it as its leader.
    let program = terminal(r#"{"argv":["codex"],"pid":5,"foreground":{"argv":["codex"],"pid":5}}"#);
    assert!(matches!(program.work(), Work::Agent { name, .. } if name == "codex"));
}

#[test]
fn reports_are_split_by_terminal_and_gone_ones_left_out() {
    let output = "\u{1e}t1\n{\"pid\":1,\"cwd\":\"/a\"}\n\u{1e}t2\n";
    let found = parse_terminals(output).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found["t1"].cwd.as_deref(), Some("/a"));
    // A record that cannot be read fails the review rather than passing a
    // running program off as an idle shell.
    assert!(matches!(
        parse_terminals("\u{1e}t3\nnot json\n"),
        Err(crate::teleport::Error::Decode {
            step: Step::Review,
            ..
        })
    ));
}

fn id(name: &str) -> TerminalId {
    TerminalId::new(name).unwrap()
}

#[test]
fn pane_trees_become_split_trees() {
    let mut panes = Panes::single(id("t1"));
    panes.split(&id("t1"), Axis::Row, id("t2")).unwrap();
    panes.split(&id("t2"), Axis::Column, id("t3")).unwrap();
    panes.set_share(&[], Share::from(700));
    assert_eq!(
        tree(&panes),
        Node::Split {
            direction: SplitDirection::Right,
            ratio: 0.7,
            first: Box::new(Node::Pane("t1".into())),
            second: Box::new(Node::Split {
                direction: SplitDirection::Down,
                ratio: 0.5,
                first: Box::new(Node::Pane("t2".into())),
                second: Box::new(Node::Pane("t3".into())),
            }),
        }
    );
    let tabs = [panes, Panes::single(id("t9"))];
    assert_eq!(ids(&tabs), ["t1", "t2", "t3", "t9"]);
}

/// A host whose `herdr-core` is a script logging its arguments: `get`
/// answers for `t1` only, `kill` does not know `t9`, `run` starts `t5`, and
/// `--version` fails once `$HOME/broken` exists.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn fake_core() -> (tempfile::TempDir, Host) {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let program = bin.join("herdr-core");
    std::fs::write(
        &program,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/calls"
case "$1" in
get) case "$3" in
    t1) printf '{"pid":1,"cwd":"/w","foreground":{"argv":["vim"],"pid":2}}\n' ;;
    t7) printf '{"error":{"kind":"io"}}\n'; exit 1 ;;
    *) printf '{"error":{"kind":"not_found"}}\n'; exit 3 ;;
    esac ;;
kill) [ "$3" = t9 ] && exit 3; [ "$3" = t8 ] && exit 1; printf '{}\n' ;;
send) printf '{}\n' ;;
run) printf '{"address":"t5","agent":"zsh","state":"live"}\n' ;;
--version) [ -e "$HOME/broken" ] && exit 1; printf 'hc 0.1.13\n' ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut host = Host::new(&sheprd_client::ConnectTarget::Local).unwrap();
    host.env = vec![("HOME".into(), home.path().to_string_lossy().into_owned())];
    (home, host)
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn terminals_are_read_and_stopped_through_the_core_cli() {
    let (home, host) = fake_core();
    let cancelled = AtomicBool::new(false);
    let found = terminals(&host, &["t1".into(), "t2".into()], &cancelled).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found["t1"].work(), Work::Command(vec!["vim".into()]));
    // Only a terminal the core does not know is taken as gone.
    assert!(terminals(&host, &["t7".into()], &cancelled).is_err());
    // A terminal already gone counts as stopped; any other failure does not.
    stop(&host, &["t1".into(), "t9".into()], &cancelled).unwrap();
    assert!(stop(&host, &["t8".into()], &cancelled).is_err());
    prompt(
        &host,
        "t1",
        "write a note",
        Duration::from_secs(300),
        &cancelled,
    )
    .unwrap();
    let calls = std::fs::read_to_string(home.path().join("calls")).unwrap();
    assert_eq!(
        calls.lines().collect::<Vec<_>>(),
        [
            "get --json t1",
            "get --json t2",
            "get --json t7",
            "kill --json t1",
            "kill --json t9",
            "kill --json t8",
            "send --json --wait --timeout 300 t1 write a note",
        ]
    );
}
