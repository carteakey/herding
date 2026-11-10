use super::*;
use crate::sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw};
use gpui::{AppContext as _, Modifiers, px, size};

#[gpui::test]
fn team_panel_routes_to_the_native_view(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1000.), px(700.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });

    let bounds = cx.debug_bounds("activity-team").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });

    assert_eq!(
        cx.update(|_, cx| view.read(cx).activity.panel),
        ActivityPanel::Team
    );
    assert!(cx.debug_bounds("sidebar").is_none());
    assert!(cx.debug_bounds("team-section").is_some());
}
