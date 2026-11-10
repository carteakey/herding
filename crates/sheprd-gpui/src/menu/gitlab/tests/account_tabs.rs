//! The header's other account tabs from the keyboard: the GitHub and
//! GitLab pages both reach the Azure DevOps page, and GitHub reaches GitLab.
use super::*;

/// The GitHub page, as its tab opens it, with the menu's focus back from
/// the Azure DevOps page's field.
fn show_github(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        view.menu.page = Some(Page::GitHub);
        window.focus(&view.menu.focus, cx);
        cx.notify();
    });
}

#[gpui::test]
fn the_github_and_gitlab_pages_reach_the_other_tabs_by_keyboard(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    // GitLab: its GitHub tab, then Azure DevOps.
    cx.simulate_keystrokes("tab tab");
    view.read_with(cx, |view, _| {
        assert!(
            view.menu
                .gitlab
                .is_selected(&crate::menu::gitlab::keys::Action::AzureDevOps)
        );
    });
    cx.simulate_keystrokes("enter");
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::AzureDevOps));
    });

    // GitHub: its GitLab tab, then Azure DevOps.
    show_github(&view, cx);
    draw(cx);
    cx.simulate_keystrokes("tab");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.menu.github_selected,
            Some(crate::menu::github::Action::GitLab)
        );
    });
    cx.simulate_keystrokes("tab enter");
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::AzureDevOps));
        assert_eq!(view.menu.github_selected, None);
    });
    show_github(&view, cx);
    draw(cx);
    cx.simulate_keystrokes("tab enter");
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::GitLab));
    });
}
