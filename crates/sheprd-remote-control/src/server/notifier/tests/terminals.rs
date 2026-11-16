//! Agents in terminal tabs, read from the layout every tick.

use super::*;
use crate::hub::{TerminalAgent, Workspace, layout_terminal};
use sheprd_client::protocol::AgentStatus;

fn agent(pane: &str, name: Option<&str>, status: AgentStatus) -> TerminalAgent {
    TerminalAgent {
        name: name.map(str::to_owned),
        status,
        pane: pane.to_owned(),
        title: Some("\u{2733} Fix the flaky test".to_owned()),
        changed_ms: None,
    }
}

fn layout(name: Option<&str>) -> Layout {
    let mut terminal = layout_terminal("t1", "server");
    terminal.agent = Some(agent("t1-p1", name, AgentStatus::Blocked));
    terminal.agents = vec![agent("t1-p1", name, AgentStatus::Blocked)];
    let plain = layout_terminal("t2", "shell");
    Layout {
        workspaces: vec![Workspace {
            id: "w1".to_owned(),
            label: "herdr-gpui".to_owned(),
            branch: None,
            cwd: String::new(),
            worktree: None,
            session: None,
            chats: Vec::new(),
            terminals: vec![terminal, plain],
        }],
        ..Layout::default()
    }
}

#[test]
fn only_tabs_with_an_agent_count_named_as_people_read_them() {
    let layout = layout(Some("claude"));
    let names = agent_names(&layout);
    let seen = terminal_agents(&layout, &names);
    assert_eq!(seen.len(), 1);
    let agent = &seen[0];
    assert_eq!(agent.chat.agent, "Claude");
    assert_eq!(agent.chat.pane, "t1-p1");
    assert_eq!(agent.chat.title, "server");
    assert_eq!(agent.chat.workspace, "herdr-gpui");
    assert_eq!(agent.status, AgentStatus::Blocked);
    assert_eq!(
        agent.task,
        Some("Fix the flaky test"),
        "without the spinner"
    );
}

#[test]
fn each_pane_of_a_split_is_its_own_agent() {
    let mut layout = layout(Some("claude"));
    let terminal = &mut layout.workspaces[0].terminals[0];
    terminal.agents = vec![
        agent("t1-p1", Some("claude"), AgentStatus::Done),
        agent("t1-p2", Some("codex"), AgentStatus::Working),
    ];
    let names = agent_names(&layout);
    let seen = terminal_agents(&layout, &names);
    let panes: Vec<_> = seen
        .iter()
        .map(|agent| (agent.chat.pane, agent.chat.agent, agent.status))
        .collect();
    assert_eq!(
        panes,
        [
            ("t1-p1", "Claude", AgentStatus::Done),
            ("t1-p2", "Codex", AgentStatus::Working),
        ]
    );
}

#[test]
fn an_unnamed_agent_is_the_agent() {
    let layout = layout(None);
    let names = agent_names(&layout);
    assert_eq!(terminal_agents(&layout, &names)[0].chat.agent, "The agent");
}
