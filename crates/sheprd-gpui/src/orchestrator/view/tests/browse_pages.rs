//! The Browse menu draws repositories a page at a time, so an organization
//! of thousands stays bounded and every repository stays reachable.
use crate::orchestrator::{
    RemoteScope, Request, Timing,
    azure::organization::{Catalog, Project},
    view::{Look, Menu, OrchestratorView, browse::MAX_ROWS, repo::Repo, rows::Tab},
};
use gpui::TestAppContext;
use std::sync::Arc;

#[gpui::test]
fn show_more_reaches_the_repositories_past_a_page(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let request = Request {
        target: sheprd_client::ConnectTarget::Local,
        checkout: dir.path().join("web").to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.path().join("data")),
        azure_store: Default::default(),
        gitlab_store: Default::default(),
        timing: Timing::default(),
    };
    let config = crate::config::Config::default();
    let look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    let (view, cx) = cx.add_window_view(|_, cx| OrchestratorView::with_repo(request, look, cx));
    cx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    let last = format!("r{}", MAX_ROWS + 9);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut mine = Repo::start_remote(
                RemoteScope::Mine {
                    organization: "contoso".into(),
                },
                Default::default(),
            );
            mine.snapshot.catalog = Some(Arc::new(Catalog {
                organization: "contoso".into(),
                projects: vec![Project {
                    name: "Fabrikam".into(),
                    repositories: (0..MAX_ROWS + 10).map(|at| format!("r{at}")).collect(),
                }],
                capped: false,
            }));
            view.repos.push(mine);
            view.tab = Tab::PullRequests;
            view.toggle_menu(Menu::Projects);
            view.refresh_rows();
            cx.notify();
        })
    });
    cx.run_until_parked();
    let selector: &'static str =
        Box::leak(format!("orchestrator-projects-repo-contoso/Fabrikam/{last}").into_boxed_str());
    assert!(cx.debug_bounds(selector).is_none());
    assert!(cx.debug_bounds("orchestrator-projects-more").is_some());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.show_more_repositories();
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds(selector).is_some());
    assert!(cx.debug_bounds("orchestrator-projects-more").is_none());
    // Closing the menu starts it over at one page.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.toggle_menu(Menu::Projects);
            assert_eq!(view.browse_rows, MAX_ROWS);
        })
    });
}
