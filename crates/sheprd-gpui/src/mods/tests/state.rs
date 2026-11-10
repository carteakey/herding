//! What mods read of the window's hosts and their workspaces.

use crate::sidebar::layout_tests::{fixture_window, snapshot};
use std::sync::Arc;

#[gpui::test]
fn another_hosts_workspace_keeps_the_focus_its_herdr_reports(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:test".into(),
                "Remote".into(),
                sheprd_client::ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            let mut reported = snapshot(2);
            reported.focused_workspace_id = Some("w1".into());
            remote.live.snapshot = Some(Arc::new(reported));
            view.endpoints.push(remote);
            let state = view.mods_state();
            let focused: Vec<_> = state
                .workspaces
                .iter()
                .filter(|workspace| workspace.host == "ssh:test" && workspace.focused)
                .map(|workspace| workspace.id.as_str())
                .collect();
            assert_eq!(focused, ["w1"]);
        })
    });
}
