use super::*;
use crate::{
    control::protocol::work::{NoteInfo, Reply},
    endpoint::Endpoint,
    state::ConnectionStatus,
    worktree_notes::Checkout,
};
use herdr_client::ConnectTarget;

#[gpui::test]
fn callers_resolve_to_their_checkout_by_repository_and_branch(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            connected(view);
            let found = find(view, &caller(Some("w4"))).ok().unwrap();
            assert_eq!(found.index, 0);
            assert_eq!(found.workspace.workspace_id, "w4");
            assert_eq!(repository(found.workspace), "agent-launcher");
            assert_eq!(
                notes::checkout_of(&view.endpoints[found.index].id, found.workspace),
                Some(Checkout {
                    endpoint: crate::endpoint::LOCAL.into(),
                    repo_key: REPO_KEY.into(),
                    branch: "worktree/sidebar-child".into(),
                })
            );
            // Outside Git there is no checkout to keep a note on.
            let outside = find(view, &caller(Some("w0"))).ok().unwrap();
            assert_eq!(notes::checkout_of("local", outside.workspace), None);
            assert_eq!(
                find(view, &caller(Some("w9")))
                    .err()
                    .as_ref()
                    .and_then(error_code),
                Some(ErrorCode::WorkspaceNotFound)
            );
            // Without a workspace, the one the window focuses.
            std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_workspace_id =
                Some("w3".into());
            let focused = find(view, &caller(None)).ok().unwrap();
            assert_eq!(focused.workspace.workspace_id, "w3");
            assert_eq!(
                selected_workspace(view, &caller(None)).ok().as_deref(),
                Some("w3")
            );
            view.live.status = ConnectionStatus::Disconnected;
            assert_eq!(
                selected_workspace(view, &caller(None))
                    .err()
                    .as_ref()
                    .and_then(error_code),
                Some(ErrorCode::Busy),
                "dialogs act through a live connection"
            );
        })
    });
}

#[gpui::test]
fn notes_are_written_and_read_for_the_callers_checkout(cx: &mut gpui::TestAppContext) {
    let (view, window) = cx.add_window_view(fixture_window);
    window.update(|_, cx| view.update(cx, |view, _| connected(view)));
    // Outside the window's update, as the control socket serves requests.
    cx.update(|cx| {
        let caller = caller(Some("w4"));
        assert_eq!(
            notes::get(&caller, cx),
            Response::Work {
                reply: Reply::Note { note: None }
            }
        );
        assert_eq!(
            notes::set(&caller, "  waiting on\nreview ", cx),
            Response::done("Saved the note on worktree/sidebar-child")
        );
        let expected = NoteInfo {
            host: crate::endpoint::LOCAL.into(),
            repository: REPO_KEY.into(),
            branch: "worktree/sidebar-child".into(),
            text: "waiting on review".into(),
        };
        assert_eq!(
            notes::get(&caller, cx),
            Response::Work {
                reply: Reply::Note {
                    note: Some(expected.clone())
                }
            }
        );
        // The same checkout through its sibling workspace is another branch.
        assert_eq!(
            notes::get(&super::caller(Some("w5")), cx),
            Response::Work {
                reply: Reply::Note { note: None }
            }
        );
        assert_eq!(
            notes::list(cx),
            Response::Work {
                reply: Reply::Notes {
                    notes: vec![expected],
                    omitted: 0,
                }
            }
        );
        assert_eq!(
            error_code(&notes::set(&super::caller(Some("w0")), "x", cx)),
            Some(ErrorCode::InvalidRequest),
            "a workspace outside Git keeps no note"
        );
        assert_eq!(
            notes::set(&caller, "", cx),
            Response::done("Cleared the note on worktree/sidebar-child")
        );
        assert_eq!(
            notes::list(cx),
            Response::Work {
                reply: Reply::Notes {
                    notes: Vec::new(),
                    omitted: 0,
                }
            }
        );
    });
}

/// A remote host the window shows, with a workspace of its own. The SSH
/// session is what a caller on that host names its daemon by.
fn remote_showing(session: &str, workspace: &str) -> Endpoint {
    let mut remote = Endpoint::new(
        "m2".into(),
        "m2".into(),
        ConnectTarget::Ssh {
            target: "box".into(),
            session: session.into(),
        },
        true,
    );
    remote.never_connect();
    let mut snapshot = snapshot(1);
    snapshot.workspaces[0].workspace_id = workspace.into();
    remote.live.snapshot = Some(std::sync::Arc::new(snapshot));
    remote.live.status = ConnectionStatus::Connected;
    remote
}

fn remote_caller(workspace: &str, daemon_socket: &str) -> Caller {
    Caller {
        pane_id: None,
        workspace_id: Some(workspace.to_owned()),
        daemon_socket: Some(daemon_socket.to_owned()),
        window: None,
    }
}

const FLEET_SOCKET: &str = "/home/alice/.local/state/herdr/sessions/fleet-1/herdr-client.sock";

/// A caller on a remote daemon resolves to ITS endpoint, never to a local
/// workspace that happens to carry the same ID: the session component of its
/// own socket path names its daemon, which has no local socket here.
#[gpui::test]
fn a_remote_daemons_caller_resolves_by_its_socket_paths_session(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            connected(view);
            view.endpoints.push(remote_showing("fleet-1", "w4"));

            let found = find(view, &remote_caller("w4", FLEET_SOCKET)).ok().unwrap();
            assert_eq!(found.index, 1, "the remote endpoint, not local w4");
            assert_eq!(found.workspace.workspace_id, "w4");

            // Another session's socket never reaches this host's workspace.
            let other = remote_caller(
                "w4",
                "/home/alice/.local/state/herdr/sessions/other/herdr-client.sock",
            );
            assert_eq!(
                find(view, &other).err().and_then(|r| error_code(&r)),
                Some(ErrorCode::WorkspaceNotFound)
            );
        })
    });
}

/// An exact local socket match wins even when a remote endpoint shares the
/// session name that socket path would parse to, and two shown hosts with
/// the same session are refused rather than guessed between.
#[gpui::test]
fn exact_socket_matches_outrank_remote_session_matches(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            connected(view);
            view.endpoints[0].connection.target =
                ConnectTarget::Socket("/state/herdr/herdr-client.sock".into());
            view.endpoints.push(remote_showing("default", "w4"));

            let found = find(view, &remote_caller("w4", "/state/herdr/herdr-client.sock"))
                .ok()
                .unwrap();
            assert_eq!(found.index, 0, "the exact local match, not remote default");
        })
    });
}

#[gpui::test]
fn two_shown_hosts_with_one_session_refuse_the_caller(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            connected(view);
            view.endpoints.push(remote_showing("fleet-1", "w4"));
            let mut twin = remote_showing("fleet-1", "w4");
            twin.live.snapshot = Some(std::sync::Arc::new(snapshot(1)));
            view.endpoints.push(twin);

            assert_eq!(
                find(view, &remote_caller("w4", FLEET_SOCKET))
                    .err()
                    .and_then(|r| error_code(&r)),
                Some(ErrorCode::WorkspaceNotFound),
                "one session on two hosts is ambiguous, never guessed"
            );
        })
    });
}

/// The window-reconnect fallback recognizes a remote daemon by its session,
/// the same way workspace resolution does.
#[gpui::test]
fn a_window_shows_a_remote_daemon_by_session(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            connected(view);
            view.endpoints.push(remote_showing("fleet-1", "w4"));
            view.selected_endpoint = 1;
            view.live = view.endpoints[1].live.clone();

            assert!(view.shows_daemon(std::path::Path::new(FLEET_SOCKET)));
            assert!(!view.shows_daemon(std::path::Path::new(
                "/home/alice/.local/state/herdr/sessions/other/herdr-client.sock",
            )));
            assert!(!view.shows_daemon(std::path::Path::new("/state/herdr/herdr-client.sock")));
        })
    });
}
