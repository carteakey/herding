use super::*;
use crate::sidebar::layout_tests::snapshot;
use sheprd_client::protocol::AgentStatus;
use sheprd_session::CoreWorkspace;

#[test]
fn a_herdr_1_snapshot_reads_as_its_session() {
    let snapshot = snapshot(4);
    let session = of_snapshot(&snapshot);
    assert_eq!(session.focused.as_deref(), Some("w0"), "marked focused");
    let ids: Vec<_> = session.workspaces.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, ["w0", "w1", "w2", "w3"]);
    let first = session.workspace("w0").unwrap();
    assert_eq!(first.keeper, Keeper::Herdr1);
    assert_eq!(first.label, "herdr");
    assert_eq!(first.branch.as_deref(), Some("main"));
    let tabs: Vec<_> = first
        .tabs
        .iter()
        .map(|tab| (tab.id.as_str(), tab.focused))
        .collect();
    assert_eq!(tabs, [("t0", true), ("t1", false)]);
    assert!(session.workspace("w1").unwrap().tabs.is_empty());
    let checkout = session.workspace("w3").unwrap().worktree.as_ref().unwrap();
    assert_eq!(checkout.label, "agent-launcher");
    assert!(!checkout.linked);
    let agents: Vec<_> = session
        .agents
        .iter()
        .map(|agent| {
            (
                agent.pane.as_str(),
                agent.name.as_str(),
                agent.workspace.as_str(),
            )
        })
        .collect();
    assert_eq!(agents, [("p0", "Claude Code", "w0"), ("p1", "agent", "w1")]);
    assert_eq!(session.agents[0].identity.as_deref(), Some("claude"));
}

#[test]
fn a_snapshot_session_is_built_once_per_snapshot() {
    let cache = SnapshotSession::default();
    let first = Arc::new(snapshot(1));
    let built = cache.get(&first);
    assert!(Arc::ptr_eq(&built, &cache.get(&first)));
    let second = Arc::new(snapshot(2));
    let rebuilt = cache.get(&second);
    assert!(!Arc::ptr_eq(&built, &rebuilt));
    assert_eq!(rebuilt.workspaces.len(), 2);
}

#[test]
fn a_host_lists_herdr_1_then_core_workspaces() {
    let herdr_1 = Arc::new(of_snapshot(&snapshot(2)));
    let core = Arc::new(Session::core(
        [CoreWorkspace {
            branch: None,
            custom_label: false,
            worktree: None,
            cwd: "",
            id: "herdr-2",
            label: "Herdr 2",
        }],
        [],
        None,
        |_| None,
    ));
    let sessions = Sessions {
        core: Some(core),
        core_answers: true,
        herdr_1: Some(herdr_1),
    };
    let ids: Vec<_> = sessions.workspaces().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, ["w0", "w1", "herdr-2"]);
    assert_eq!(sessions.workspace("herdr-2").unwrap().keeper, Keeper::App);
    assert_eq!(sessions.agents().count(), 2);
    assert!(
        sessions
            .agents()
            .all(|agent| agent.status == AgentStatus::Working)
    );
    assert!(!sessions.is_empty());
    assert!(Sessions::default().is_empty());
}

#[test]
fn each_session_names_its_keeper() {
    let sessions = Sessions {
        core: Some(Arc::new(Session::default())),
        core_answers: false,
        herdr_1: Some(Arc::new(of_snapshot(&snapshot(1)))),
    };
    let keepers: Vec<_> = sessions.keyed().map(|(keeper, _)| keeper).collect();
    assert_eq!(keepers, [Keeper::Herdr1, Keeper::App]);
}

#[test]
fn core_agents_in_herdr_1_workspaces_bring_their_checkouts() {
    use sheprd_session::Agent;
    let herdr_1 = Arc::new(of_snapshot(&snapshot(4)));
    let agent = |workspace: &str| Agent {
        focused: false,
        identity: None,
        name: "claude".into(),
        pane: "t1".into(),
        status: AgentStatus::Working,
        tab: "1".into(),
        workspace: workspace.into(),
    };
    let alone = Arc::new(Session {
        agents: vec![agent("herdr-2")],
        ..Session::default()
    });
    let sessions = Sessions {
        core: Some(alone.clone()),
        core_answers: true,
        herdr_1: Some(herdr_1.clone()),
    };
    let same = core_with_checkouts(&sessions).unwrap();
    assert!(Arc::ptr_eq(&same, &alone), "nothing missing, nothing built");
    let sessions = Sessions {
        core: Some(Arc::new(Session {
            agents: vec![agent("w3")],
            ..Session::default()
        })),
        core_answers: true,
        herdr_1: Some(herdr_1),
    };
    let joined = core_with_checkouts(&sessions).unwrap();
    let ids: Vec<_> = joined.workspaces.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, ["w3"]);
    assert!(joined.workspace("w3").unwrap().worktree.is_some());
    assert!(core_with_checkouts(&Sessions::default()).is_none());
}
