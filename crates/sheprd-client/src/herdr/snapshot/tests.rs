use super::*;

#[test]
fn decodes_the_cli_snapshot_envelope_and_ignores_unknown_fields() {
    let json = r#"{"id":"cli:api:snapshot","result":{"type":"session_snapshot","snapshot":{
        "version":"0.9.1","protocol":22,"future":true,
        "workspaces":[{"workspace_id":"w1","label":"feat","worktree":{"checkout_path":"/w/feat",
          "is_linked_worktree":true,"repo_key":"/r/.git","repo_name":"r","repo_root":"/r"}}],
        "tabs":[{"tab_id":"w1:t2","workspace_id":"w1","number":2,"label":"b"},
                {"tab_id":"w1:t1","workspace_id":"w1","number":1}],
        "panes":[{"pane_id":"w1:p1","tab_id":"w1:t1","cwd":"/w/feat","agent":"claude",
          "agent_session":{"agent":"claude","kind":"id","source":"herdr:claude","value":"abc"}}],
        "layouts":[{"tab_id":"w1:t1","zoomed":false,"focused_pane_id":"w1:p1",
          "area":{"x":0,"y":0,"width":10,"height":5},
          "panes":[{"pane_id":"w1:p1","focused":true,"rect":{"x":0,"y":0,"width":10,"height":5}}],
          "splits":[]}],
        "agents":[]}}}"#;
    let snapshot = serde_json::from_str::<Envelope<SnapshotResult>>(json)
        .unwrap()
        .result
        .snapshot;
    let tabs: Vec<_> = snapshot
        .tabs_of("w1")
        .iter()
        .map(|t| t.tab_id.as_str())
        .collect();
    assert_eq!(tabs, ["w1:t1", "w1:t2"]);
    let pane = snapshot.pane("w1:p1").unwrap();
    assert_eq!(pane.agent_session.as_ref().unwrap().kind, SessionKind::Id);
    assert!(
        snapshot
            .workspace("w1")
            .unwrap()
            .worktree
            .as_ref()
            .unwrap()
            .is_linked_worktree
    );
    assert_eq!(snapshot.layout("w1:t1").unwrap().panes[0].pane_id, "w1:p1");
}

#[test]
fn an_idle_shell_has_no_foreground_job() {
    let info: ProcessInfo = serde_json::from_str(
        r#"{"foreground_process_group_id":7,"shell_pid":7,
            "foreground_processes":[{"pid":7,"argv":["-zsh"],"cwd":"/w"}]}"#,
    )
    .unwrap();
    assert!(info.foreground_job().is_none());
    assert!(info.foreground_pids().is_empty());
    let info: ProcessInfo = serde_json::from_str(
        r#"{"foreground_process_group_id":9,"shell_pid":7,
            "foreground_processes":[{"pid":12,"argv":["node","x"]},{"pid":9,"argv":["npm","run","dev"]}]}"#,
    )
    .unwrap();
    assert_eq!(info.foreground_job().unwrap().argv, ["npm", "run", "dev"]);
    assert_eq!(info.foreground_pids(), [12, 9]);
    assert!(ProcessInfo::default().foreground_pids().is_empty());
}
