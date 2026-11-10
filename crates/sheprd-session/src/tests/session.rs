use crate::{CoreTab, CoreWorkspace, Keeper, Session, panes::Panes};
use sheprd_backend::{AgentStatus, RunState, TerminalId, TerminalInfo};
use std::collections::BTreeMap;

fn terminal(id: &str) -> TerminalId {
    TerminalId::new(id).unwrap()
}

fn info(id: &str, agent: AgentStatus) -> TerminalInfo {
    TerminalInfo {
        agent,
        cwd: None,
        foreground: None,
        id: terminal(id),
        labels: BTreeMap::new(),
        name: Some(format!("{id} name")),
        reporter: None,
        state: RunState::Running,
        title: None,
    }
}

#[test]
fn core_tabs_gather_in_their_workspaces_with_their_agents() {
    let one = Panes::single(terminal("t1"));
    let mut two = Panes::single(terminal("t2"));
    two.split(&terminal("t2"), crate::panes::Axis::Row, terminal("t3"))
        .unwrap();
    let elsewhere = Panes::single(terminal("t4"));
    let infos = [
        info("t1", AgentStatus::Done),
        info("t2", AgentStatus::Unknown),
        info("t3", AgentStatus::Blocked),
        info("t4", AgentStatus::Working),
    ];
    let workspaces = [
        CoreWorkspace {
            branch: None,
            custom_label: false,
            worktree: None,
            cwd: "/a",
            id: "a",
            label: "A",
        },
        CoreWorkspace {
            branch: None,
            custom_label: false,
            worktree: None,
            cwd: "/b",
            id: "b",
            label: "B",
        },
    ];
    let tabs = [
        CoreTab {
            id: 1,
            panes: &one,
            title: "one",
            workspace_id: "a",
        },
        CoreTab {
            id: 2,
            panes: &two,
            title: "two",
            workspace_id: "a",
        },
        CoreTab {
            id: 3,
            panes: &elsewhere,
            title: "far",
            workspace_id: "w_1",
        },
    ];
    let session = Session::core(workspaces, tabs, Some("b"), |id| {
        infos.iter().find(|info| &info.id == id)
    });
    assert_eq!(session.focused.as_deref(), Some("b"));
    let a = session.workspace("a").unwrap();
    assert_eq!(a.keeper, Keeper::App);
    assert_eq!(a.status, AgentStatus::Blocked);
    let tabs: Vec<_> = a
        .tabs
        .iter()
        .map(|tab| (tab.id.as_str(), tab.status))
        .collect();
    assert_eq!(
        tabs,
        [("1", AgentStatus::Done), ("2", AgentStatus::Blocked)]
    );
    assert!(session.workspace("b").unwrap().tabs.is_empty());
    assert!(session.workspace("w_1").is_none());
    let agents: Vec<_> = session
        .agents
        .iter()
        .map(|agent| (agent.pane.as_str(), agent.workspace.as_str()))
        .collect();
    assert_eq!(agents, [("t1", "a"), ("t3", "a"), ("t4", "w_1")]);
    assert_eq!(session.agents_in("a").count(), 2);
    assert_eq!(session.agents[0].name, "t1 name");
}

#[test]
fn focus_falls_back_to_the_first_workspace() {
    let workspaces = [
        CoreWorkspace {
            branch: None,
            custom_label: false,
            worktree: None,
            cwd: "",
            id: "a",
            label: "A",
        },
        CoreWorkspace {
            branch: None,
            custom_label: false,
            worktree: None,
            cwd: "",
            id: "b",
            label: "B",
        },
    ];
    let session = Session::core(workspaces, [], Some("gone"), |_| None);
    assert_eq!(session.focused.as_deref(), Some("a"));
    assert_eq!(session.focused_workspace().unwrap().id, "a");
    let empty = Session::core([], [], Some("gone"), |_| None);
    assert_eq!(empty.focused, None);
}

#[test]
fn extending_keeps_the_first_focus() {
    let mut first = Session::core(
        [CoreWorkspace {
            branch: None,
            custom_label: false,
            worktree: None,
            cwd: "",
            id: "a",
            label: "A",
        }],
        [],
        None,
        |_| None,
    );
    let second = Session::core(
        [CoreWorkspace {
            branch: None,
            custom_label: false,
            worktree: None,
            cwd: "",
            id: "b",
            label: "B",
        }],
        [],
        None,
        |_| None,
    );
    first.extend(second);
    assert_eq!(first.focused.as_deref(), Some("a"));
    assert_eq!(first.workspaces.len(), 2);
    let mut none = Session::default();
    none.extend(first);
    assert_eq!(none.focused.as_deref(), Some("a"));
}

#[test]
fn the_most_pressing_state_wins() {
    use crate::session::most_pressing;
    assert_eq!(most_pressing([]), AgentStatus::Unknown);
    assert_eq!(
        most_pressing([AgentStatus::Idle, AgentStatus::Working, AgentStatus::Done]),
        AgentStatus::Working
    );
}
