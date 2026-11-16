//! A workspace on another host gets its herdr-core terminals from that host's
//! core, reached over the bridge the app runs, never from this computer's. A
//! host without herdr-core keeps its new tabs in its Herdr. These tests give
//! the host a stand-in hub, so none dials a real machine.

use super::*;
use crate::{
    browser::Scope,
    core_terminals::{Error, Host, Hub, Start, State},
};

fn ssh() -> ConnectTarget {
    ConnectTarget::Ssh {
        target: "fixture".into(),
        session: "default".into(),
    }
}

fn scope() -> Scope {
    Scope::endpoint("ssh:fixture")
}

fn missing(_: Start) -> std::result::Result<sheprd_core_client::CoreBackend, Error> {
    Err(Error::Bridge(
        sheprd_client::core_bridge::Error::Missing.into(),
    ))
}

fn unreachable(_: Start) -> std::result::Result<sheprd_core_client::CoreBackend, Error> {
    Err(Error::NotBundled)
}

/// Waits for `host`'s hub to finish its attempt, as the window's tick does.
fn settle(
    view: &mut SheprdWindow,
    host: &Host,
    window: &mut gpui::Window,
    cx: &mut Context<SheprdWindow>,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(
        view.core_terminals.hub_of(host).map(Hub::state),
        Some(State::Connecting(_))
    ) {
        assert!(Instant::now() < deadline, "the attempt never finished");
        view.poll_core_terminals(window, cx);
        std::thread::yield_now();
    }
    view.poll_core_terminals(window, cx);
}

#[gpui::test]
fn a_tab_on_any_host_follows_the_herdr_2_setting(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let (mut remote, _server) = connected_endpoint("ssh:fixture");
    remote.connection.target = ssh();
    view.update(cx, |view, cx| {
        view.endpoints.push(remote);
        for (selected, setting, core) in [
            (0, true, true),
            (0, false, false),
            (1, true, true),
            (1, false, false),
        ] {
            view.selected_endpoint = selected;
            view.reset_selected(cx);
            view.config.herdr_2_terminals = setting;
            assert_eq!(view.new_tabs_use_core(), core, "{selected} {setting}");
        }
    });
}

#[gpui::test]
fn the_herdr_2_command_on_an_ssh_host_dials_that_hosts_core(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, _server) = connected_endpoint("ssh:fixture");
    endpoint.connection.target = ssh();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(endpoint);
            view.selected_endpoint = 1;
            view.reset_selected(cx);
            view.core_terminals.set_remote_hub_for_test(
                scope(),
                ssh(),
                Hub::with_join(unreachable),
            );
            view.command(Command::NewCoreTerminal, window, cx);
            // This computer's core is never asked for the host's terminal.
            assert!(matches!(view.core_terminals.hub.state(), State::Idle));
            let host = Host::Remote(scope());
            assert!(!matches!(
                view.core_terminals.hub_of(&host).map(Hub::state),
                Some(State::Idle) | None
            ));
            settle(view, &host, window, cx);
            let (flash, _) = view.flash.as_ref().unwrap();
            let reason = Error::NotBundled.to_string();
            assert_eq!(
                flash.text,
                crate::i18n::fl!("core-terminals-unavailable", reason = reason)
            );
        })
    });
}

#[gpui::test]
fn a_new_tab_on_a_host_without_herdr_core_is_created_by_its_herdr(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, mut server) = connected_endpoint("ssh:fixture");
    endpoint.connection.target = ssh();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(endpoint);
            view.selected_endpoint = 1;
            view.options = ConnectOptions::default();
            view.reset_selected(cx);
            view.activation_deadline = None;
            view.config.herdr_2_terminals = true;
            view.settings.shared =
                Some(crate::herdr_settings::Settings::parse_text(NO_TAB_PROMPT).unwrap());
            view.core_terminals
                .set_remote_hub_for_test(scope(), ssh(), Hub::with_join(missing));
            view.command(Command::Tab, window, cx);
            let host = Host::Remote(scope());
            settle(view, &host, window, cx);
            assert!(view.core_terminals.lacks_core(&host));
            assert!(!view.new_tabs_use_core());
            assert!(matches!(view.core_terminals.hub.state(), State::Idle));
            let (flash, _) = view.flash.as_ref().unwrap();
            assert_eq!(
                flash.text,
                crate::i18n::fl!("core-terminals-host-without-core")
            );
        })
    });
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing creation");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], Method::TabCreate.as_str());
    assert_eq!(
        request["params"],
        serde_json::json!({"workspace_id": "w1", "focus": true})
    );
}

#[gpui::test]
fn with_herdr_2_off_a_new_tab_on_an_ssh_host_is_created_by_its_herdr(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, mut server) = connected_endpoint("ssh:fixture");
    endpoint.connection.target = ssh();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(endpoint);
            view.selected_endpoint = 1;
            view.options = ConnectOptions::default();
            view.reset_selected(cx);
            view.activation_deadline = None;
            view.config.herdr_2_terminals = false;
            view.settings.shared =
                Some(crate::herdr_settings::Settings::parse_text(NO_TAB_PROMPT).unwrap());
            view.command(Command::Tab, window, cx);
            assert!(view.core_terminals.hub_of(&Host::Remote(scope())).is_none());
        })
    });
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing creation");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], Method::TabCreate.as_str());
}

#[gpui::test]
fn an_edited_or_removed_device_drops_its_core_hub(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, _server) = connected_endpoint("ssh:fixture");
    endpoint.connection.target = ssh();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(endpoint);
            let host = Host::Remote(scope());
            view.core_terminals.set_remote_hub_for_test(
                scope(),
                ssh(),
                Hub::with_join(unreachable),
            );
            view.poll_core_terminals(window, cx);
            assert!(view.core_terminals.hub_of(&host).is_some());
            // A terminal the old machine started is still on its way.
            view.core_terminals.start_for_test(
                crate::core_terminals::Purpose::Tab {
                    scope: scope(),
                    workspace: "w1".into(),
                },
                host.clone(),
                Ok(sheprd_backend::TerminalInfo {
                    agent: AgentStatus::Unknown,
                    cwd: None,
                    foreground: None,
                    id: sheprd_backend::TerminalId::new("t1").unwrap(),
                    labels: Default::default(),
                    name: None,
                    reporter: None,
                    state: sheprd_backend::RunState::Running,
                    title: None,
                }),
            );
            // The device now reaches another machine: the hub that dials
            // the old one goes, with the terminal it was starting, and the
            // next ask makes one for the new one.
            view.endpoints[1].connection.target = ConnectTarget::Ssh {
                target: "elsewhere".into(),
                session: "default".into(),
            };
            view.poll_core_terminals(window, cx);
            assert!(view.core_terminals.hub_of(&host).is_none());
            assert!(!view.core_terminals.is_starting());
            let placed = cx
                .try_global::<crate::browser::Store>()
                .is_some_and(|store| store.in_workspace(&scope(), "w1").next().is_some());
            assert!(!placed);
            view.core_terminals.set_remote_hub_for_test(
                scope(),
                ssh(),
                Hub::with_join(unreachable),
            );
            view.endpoints.truncate(1);
            view.poll_core_terminals(window, cx);
            assert!(view.core_terminals.hub_of(&host).is_none());
        })
    });
}

#[gpui::test]
fn a_missing_core_never_opens_a_herdr_tab_in_another_workspace(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, _server) = connected_endpoint("ssh:fixture");
    endpoint.connection.target = ssh();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(endpoint);
            view.selected_endpoint = 1;
            view.reset_selected(cx);
            view.core_terminals
                .set_remote_hub_for_test(scope(), ssh(), Hub::with_join(missing));
            // Asked for in a workspace the user has since left.
            view.start_core_program(
                crate::core_terminals::Purpose::Tab {
                    scope: scope(),
                    workspace: "w-left".into(),
                },
                sheprd_backend::Spawn::default(),
                cx,
            );
            settle(view, &Host::Remote(scope()), window, cx);
            let (flash, _) = view.flash.as_ref().unwrap();
            let reason = missing(Start::Join).err().unwrap().to_string();
            assert_eq!(
                flash.text,
                crate::i18n::fl!("core-terminals-unavailable", reason = reason)
            );
        })
    });
}

/// Creating at once, rather than asking for the tab's name first.
const NO_TAB_PROMPT: &str = "[ui]\nprompt_new_tab_name = false\n";
