//! The layer over a workspace row while a workspace menu is open, so a right
//! click on another row moves the menu there rather than only dismissing it.

use crate::{NavigationTarget, SheprdWindow};
use gpui::{prelude::*, *};

impl SheprdWindow {
    pub(super) fn workspace_menu_overlay(
        &self,
        endpoint: String,
        workspace: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let bounds = bounds.intersect(&window.content_mask().bounds);
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    // The overlay dismisses first in bubble order. Use
                    // clipped row geometry because it occludes our hitbox.
                    if phase == DispatchPhase::Bubble
                        && event.button == MouseButton::Right
                        && bounds.contains(&event.position)
                    {
                        let _ = view.update(cx, |this, cx| {
                            cx.stop_propagation();
                            if this.navigate_endpoint(
                                &endpoint,
                                NavigationTarget::Workspace(&workspace),
                                cx,
                            ) {
                                this.open_workspace_menu(&workspace, event.position, window, cx);
                                this.menu.opening_right_click =
                                    this.menu.page == Some(crate::menu::Page::Workspace);
                            }
                        });
                    }
                });
            },
        )
        .absolute()
        .inset_0()
        .size_full()
    }
}
