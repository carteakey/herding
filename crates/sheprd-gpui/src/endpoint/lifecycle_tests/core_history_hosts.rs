//! Editing a Herdr 2 pane's scrollback saves it to a file on this computer,
//! which an editor on another host's core could not open, so it is refused
//! there rather than opening an empty editor.

use super::*;
use crate::core_terminals::{Host, Hub, Start, State};

fn ssh() -> ConnectTarget {
    ConnectTarget::Ssh {
        target: "fixture".into(),
        session: "default".into(),
    }
}

fn unreachable(
    _: Start,
) -> std::result::Result<sheprd_core_client::CoreBackend, crate::core_terminals::Error> {
    Err(crate::core_terminals::Error::NotBundled)
}

#[gpui::test]
fn editing_scrollback_on_another_hosts_core_is_refused(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let (mut endpoint, _server) = connected_endpoint("ssh:fixture");
    endpoint.connection.target = ssh();
    view.update(cx, |view, cx| {
        view.endpoints.push(endpoint);
        view.selected_endpoint = 1;
        view.reset_selected(cx);
        let scope = crate::browser::Scope::endpoint("ssh:fixture");
        view.core_terminals.set_remote_hub_for_test(
            scope.clone(),
            ssh(),
            Hub::with_join(unreachable),
        );
        assert!(view.browser_key().is_some_and(|(key, _)| key == scope));
        view.edit_core_history(None, "history".into(), cx);
        let (flash, _) = view.flash.as_ref().unwrap();
        assert_eq!(flash.text, crate::i18n::fl!("core-terminals-edit-remote"));
        // Neither core is asked for an editor.
        assert!(matches!(view.core_terminals.hub.state(), State::Idle));
        assert!(matches!(
            view.core_terminals
                .hub_of(&Host::Remote(scope))
                .map(Hub::state),
            Some(State::Idle)
        ));
    });
}
