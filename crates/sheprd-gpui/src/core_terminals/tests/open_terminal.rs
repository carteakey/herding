//! Showing a herdr-core terminal something outside the window started, such
//! as an automation's agent.

use crate::{
    SheprdWindow,
    browser::{Location, Store},
    core_terminals::panes::Panes,
};
use gpui::{Context, prelude::*};
use sheprd_backend::TerminalId;

struct Host(gpui::Entity<SheprdWindow>);

impl Render for Host {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
    }
}

fn core_tabs(cx: &gpui::App, terminal: &TerminalId) -> Vec<(String, bool)> {
    cx.global::<Store>()
        .tabs()
        .filter_map(|tab| match &tab.location {
            Some(Location::CoreTerminal { panes, observe, .. }) if panes.contains(terminal) => {
                Some((tab.workspace_id.clone(), *observe))
            }
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn a_terminal_opens_once_in_the_focused_workspace(cx: &mut gpui::TestAppContext) {
    let (host, cx) = cx.add_window_view(|window, cx| {
        Host(cx.new(|cx| crate::palette::fixture_window(window, cx)))
    });
    let view = host.update(cx, |host, _| host.0.clone());
    let terminal = TerminalId::new("t7").unwrap();
    cx.update(|window, cx| {
        Store::default().install(cx);
        view.update(cx, |view, cx| {
            let workspace = view.browser_key().unwrap().1;
            view.open_core_terminal(terminal.clone(), window, cx);
            assert_eq!(core_tabs(cx, &terminal), [(workspace.clone(), false)]);
            // Asking again shows the same tab rather than a second driver.
            view.open_core_terminal(terminal.clone(), window, cx);
            assert_eq!(core_tabs(cx, &terminal), [(workspace, false)]);
        });
    });
}

#[gpui::test]
fn a_terminal_another_workspace_drives_opens_watch_only(cx: &mut gpui::TestAppContext) {
    let (host, cx) = cx.add_window_view(|window, cx| {
        Host(cx.new(|cx| crate::palette::fixture_window(window, cx)))
    });
    let view = host.update(cx, |host, _| host.0.clone());
    let terminal = TerminalId::new("t7").unwrap();
    cx.update(|window, cx| {
        Store::default().install(cx);
        view.update(cx, |view, cx| {
            let (scope, workspace) = view.browser_key().unwrap();
            let location = Location::core_terminal(Panes::single(terminal.clone()), false);
            Store::update(cx, |store| {
                store.open(scope, "elsewhere", Some(location), None)
            })
            .unwrap();
            view.open_core_terminal(terminal.clone(), window, cx);
            let mut tabs = core_tabs(cx, &terminal);
            tabs.sort();
            assert_eq!(tabs, [("elsewhere".to_owned(), false), (workspace, true)]);
        });
    });
}

#[gpui::test]
fn a_split_tab_holding_the_terminal_shows_it_as_the_pane_in_use(cx: &mut gpui::TestAppContext) {
    let (host, cx) = cx.add_window_view(|window, cx| {
        Host(cx.new(|cx| crate::palette::fixture_window(window, cx)))
    });
    let view = host.update(cx, |host, _| host.0.clone());
    let first = TerminalId::new("t1").unwrap();
    let terminal = TerminalId::new("t7").unwrap();
    cx.update(|window, cx| {
        Store::default().install(cx);
        view.update(cx, |view, cx| {
            let (scope, workspace) = view.browser_key().unwrap();
            let mut panes = Panes::single(first.clone());
            panes
                .split(
                    &first,
                    crate::core_terminals::panes::Axis::Row,
                    terminal.clone(),
                )
                .unwrap();
            // A watch-only tab counts too: asking again must not add one.
            let location = Location::core_terminal(panes, true);
            let tab = Store::update(cx, |store| {
                store.open(scope, &workspace, Some(location), None)
            })
            .unwrap();
            view.sync_core_views(cx);
            view.open_core_terminal(terminal.clone(), window, cx);
            view.open_core_terminal(terminal.clone(), window, cx);
            assert_eq!(core_tabs(cx, &terminal), [(workspace, true)]);
            assert_eq!(view.core_terminals.tabs[&tab].active, terminal);
        });
    });
}
