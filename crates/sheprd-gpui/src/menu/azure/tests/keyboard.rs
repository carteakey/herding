//! The page from the keyboard: Tab walks its controls and fields in the order
//! they show, and Enter or Space uses the selected control.
use super::*;
use crate::menu::azure::keys::Action;

fn saved_contoso(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        let kept = ["contoso".to_owned()].into_iter().collect();
        view.show_azure_organizations(vec!["contoso".into(), "fabrikam".into()], &kept, || {
            Lookup::None
        });
        cx.notify();
    });
    draw(cx);
}

fn selected(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Option<Action> {
    view.read_with(cx, |view, _| view.menu.azure.selected.clone())
}

fn message(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| {
        plain(view.menu.azure.message.clone().unwrap_or_default())
    })
}

#[gpui::test]
fn tab_walks_every_control_from_the_token_field(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    saved_contoso(&view, cx);
    let actions = view.read_with(cx, |view, _| view.menu.azure.actions());
    assert_eq!(
        actions,
        [
            Action::GitHub,
            Action::GitLab,
            Action::Forget("contoso".into()),
            Action::Organization,
            Action::Token,
            Action::CreateToken,
            Action::Save,
            Action::Close,
        ],
        "only an organization with a saved token offers Forget"
    );
    let start = actions.iter().position(|action| *action == Action::Token);
    for step in 1..=actions.len() {
        cx.simulate_keystrokes("tab");
        let at = (start.unwrap() + step) % actions.len();
        assert_eq!(selected(&view, cx).as_ref(), Some(&actions[at]));
    }
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(selected(&view, cx), Some(Action::Organization));
}

#[gpui::test]
fn a_field_takes_focus_and_keeps_its_own_space(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    cx.simulate_keystrokes("shift-tab");
    let focused = cx.update(|window, cx| {
        view.read(cx)
            .menu
            .azure
            .organization
            .as_ref()
            .is_some_and(|input| input.read(cx).focus.is_focused(window))
    });
    assert!(
        focused,
        "Shift-Tab from the token field reaches its neighbor"
    );
    cx.simulate_input("two words");
    let typed = view.read_with(cx, |view, cx| {
        let input = view.menu.azure.organization.as_ref().unwrap();
        input.read(cx).text().to_owned()
    });
    assert_eq!(typed, "two words");
    assert!(selected(&view, cx) == Some(Action::Organization));
}

#[gpui::test]
fn enter_uses_the_selected_control(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    saved_contoso(&view, cx);
    // Back from the token field: the organization, Forget, then GitLab.
    cx.simulate_keystrokes("shift-tab shift-tab");
    assert_eq!(selected(&view, cx), Some(Action::Forget("contoso".into())));
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert_eq!(
        message(&view, cx),
        "This window does not keep Azure DevOps tokens."
    );
    cx.simulate_keystrokes("shift-tab enter");
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.menu.page) == Some(Page::GitLab));
}

#[gpui::test]
fn space_uses_the_selected_link(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    cx.simulate_keystrokes("tab");
    assert_eq!(selected(&view, cx), Some(Action::CreateToken));
    cx.simulate_keystrokes("space");
    draw(cx);
    assert_eq!(message(&view, cx), "Type the organization's name first.");
}

#[gpui::test]
fn enter_in_a_field_saves_and_escape_closes(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert_eq!(message(&view, cx), "Type the organization's name first.");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.menu.page) != Some(Page::AzureDevOps));
}

#[gpui::test]
fn a_forgotten_organizations_control_is_not_used(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    saved_contoso(&view, cx);
    cx.simulate_keystrokes("shift-tab shift-tab");
    view.update(cx, |view, cx| {
        view.show_azure_organizations(vec!["fabrikam".into()], &Default::default(), || {
            Lookup::None
        });
        cx.notify();
    });
    draw(cx);
    cx.simulate_keystrokes("space");
    draw(cx);
    assert_eq!(message(&view, cx), "");
}

#[gpui::test]
fn the_selection_scrolls_into_view_both_ways(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    let names: Vec<String> = (0..30).map(|index| format!("org{index:02}")).collect();
    view.update(cx, |view, cx| {
        let kept = names.iter().cloned().collect();
        view.show_azure_organizations(names.clone(), &kept, || Lookup::None);
        cx.notify();
    });
    draw(cx);
    let shown = |cx: &mut VisualTestContext, selector: String| {
        let selector: &'static str = selector.leak();
        draw(cx);
        let body = cx.debug_bounds("azure-body").unwrap();
        let control = cx.debug_bounds(selector).unwrap();
        assert!(
            control.top() >= body.top() && control.bottom() <= body.bottom(),
            "{selector} at {control:?} is outside {body:?}"
        );
    };
    cx.simulate_keystrokes("shift-tab");
    shown(cx, "azure-organization-input".into());
    for index in (0..names.len()).rev() {
        cx.simulate_keystrokes("shift-tab");
        shown(cx, format!("azure-forget-{index}"));
    }
    for index in 1..names.len() {
        cx.simulate_keystrokes("tab");
        shown(cx, format!("azure-forget-{index}"));
    }
}

#[gpui::test]
fn each_stop_names_its_child_of_the_body(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    saved_contoso(&view, cx);
    view.update(cx, |view, _| {
        let azure = &mut view.menu.azure;
        let child = |azure: &crate::menu::azure::AzureMenu, action| azure.body_child(&action);
        // The help line, then two cards.
        assert_eq!(child(azure, Action::Forget("contoso".into())), Some(1));
        assert_eq!(child(azure, Action::Organization), Some(3));
        assert_eq!(child(azure, Action::Token), Some(4));
        assert_eq!(child(azure, Action::CreateToken), Some(5));
        assert_eq!(child(azure, Action::Save), None);
        azure.problem = Some("unreadable".into());
        assert_eq!(child(azure, Action::Forget("contoso".into())), Some(2));
    });
}
