//! The find bar's look and keys, shared by the window's bar over a Herdr 1
//! pane and a herdr-core view's own: a native search field, the match count,
//! and buttons to step between matches or close.

use super::Step;
use crate::{config::Theme, search_input::SearchInput};
use gpui::{prelude::*, *};

/// What a key or button in the bar asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Close,
    Step(Step),
}

/// What the bar shows besides its field.
pub(crate) struct Bar<'a> {
    pub(crate) input: &'a Entity<SearchInput>,
    /// The match count, or why the search failed.
    pub(crate) label: String,
    pub(crate) failed: bool,
    pub(crate) has_matches: bool,
    pub(crate) theme: &'a Theme,
    pub(crate) text_size: f32,
}

/// The action a key in the field asks for: Escape closes, Enter and the
/// arrows move between matches. `None` leaves the key to the field or the
/// platform, as it does while an input method composes.
pub(crate) fn key_action(event: &KeyDownEvent, input: &SearchInput) -> Option<Action> {
    if input.is_composing() {
        return None;
    }
    let keystroke = &event.keystroke;
    let modifiers = keystroke.modifiers;
    let step = match keystroke.key.as_str() {
        "escape" if !modifiers.modified() => return Some(Action::Close),
        "enter" if modifiers.shift => Step::Newer,
        "enter" => Step::Older,
        "up" if !modifiers.modified() => Step::Older,
        "down" if !modifiers.modified() => Step::Newer,
        "g" if crate::keymap::secondary(&modifiers) && modifiers.shift => Step::Newer,
        "g" if crate::keymap::secondary(&modifiers) => Step::Older,
        _ => return None,
    };
    Some(Action::Step(step))
}

/// The bar's box. `act` runs a button's action; `on_key_down` sees the
/// field's keys first.
pub(crate) fn render(
    bar: Bar,
    on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    act: impl Fn(Action, &mut Window, &mut App) + Clone + 'static,
) -> Stateful<Div> {
    let theme = bar.theme;
    let has_matches = bar.has_matches;
    let button = |id: &'static str, icon: &'static str, action: Action| {
        let active = action == Action::Close || has_matches;
        let act = act.clone();
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_none()
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::CONTROL))
            .when(active, |button| {
                button
                    .cursor_pointer()
                    .hover(|button| button.bg(rgba((theme.foreground << 8) | 0x14)))
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(if action == Action::Close { 12. } else { 14. }))
                    .text_color(rgb(if active {
                        theme.foreground
                    } else {
                        theme.muted
                    })),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                act(action, window, cx);
            })
    };
    div()
        .id("find-bar")
        .debug_selector(|| "find-bar".into())
        .occlude()
        .min_w_0()
        .w(px(320.))
        .flex_shrink(1.)
        .flex()
        .items_center()
        .gap(px(4.))
        .p(px(4.))
        .rounded(px(crate::config::corners::CONTROL))
        .border_1()
        .border_color(rgb(theme.active))
        .bg(rgb(theme.surface))
        .text_color(rgb(theme.foreground))
        .text_size(px(bar.text_size))
        .shadow_md()
        .on_key_down(on_key_down)
        // Presses anywhere on the bar stay off the terminal under it.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_scroll(|_, _, cx| cx.stop_propagation())
        .child(div().flex_1().min_w_0().child(bar.input.clone()))
        .child(
            div()
                .debug_selector(|| "find-count".into())
                .flex_none()
                .px(px(4.))
                .text_color(rgb(if bar.failed {
                    theme.ink(theme.palette[1])
                } else {
                    theme.muted
                }))
                .child(bar.label),
        )
        .child(button(
            "find-older",
            "icons/chevron-up.svg",
            Action::Step(Step::Older),
        ))
        .child(button(
            "find-newer",
            "icons/chevron-down.svg",
            Action::Step(Step::Newer),
        ))
        .child(button("find-close", "icons/close.svg", Action::Close))
}
