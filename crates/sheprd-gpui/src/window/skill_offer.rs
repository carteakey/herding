//! The one-time offer of the agent skill for browser tabs, and its install.

use super::{Flash, SheprdWindow};
use crate::i18n::{Localize, fl};
use gpui::{Context, Window};

impl SheprdWindow {
    /// Installs the agent skill for browser tabs where Claude Code and other
    /// agents look for skills, and keeps it current from now on. Only on
    /// request: the one-time offer, the palette, or Settings.
    pub(crate) fn install_browser_skill(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::agent_skill::{AgentSkill, Choice};
        let home = match crate::config::home() {
            Ok(home) => home,
            Err(error) => {
                self.show_flash(Flash::warning(error.localize()), cx);
                return;
            }
        };
        AgentSkill::choose(Choice::Installed, cx);
        if self.menu.page == Some(crate::menu::Page::AgentSkill) {
            self.dismiss_menu(window, cx);
        }
        let install = cx.background_executor().spawn(async move {
            let text = crate::agent_skill::text(crate::agent_skill::executable().as_deref());
            crate::agent_skill::install(&home, &text)
        });
        cx.spawn(async move |this, cx| {
            let result = install.await;
            this.update(cx, |this, cx| {
                let flash = match result {
                    Ok(paths) => Flash::success(match paths.as_slice() {
                        [path] => fl!(
                            "window-notices-skill-installed-at",
                            path = path.display().to_string()
                        ),
                        _ => fl!("window-notices-skill-installed-places", count = paths.len()),
                    }),
                    Err(error) => Flash::warning(error.localize()),
                };
                this.show_flash(flash, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Offers the agent skill once, when this is the first window ready to
    /// show a dialog. Runs every tick; asking claims the question app-wide.
    pub(crate) fn offer_browser_skill(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ready = self.menu.page.is_none() && self.active && self.live.snapshot.is_some();
        if !ready || !crate::agent_skill::AgentSkill::take_ask(cx) {
            return;
        }
        if self.open_menu(window, cx) {
            self.menu.page = Some(crate::menu::Page::AgentSkill);
            cx.notify();
        }
    }

    pub(crate) fn render_agent_skill_offer(&self, cx: &mut Context<Self>) -> gpui::Div {
        use gpui::{prelude::*, *};
        let theme = &self.theme;
        let button = |id: &'static str, label: String, primary: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .p(px(8.))
                .rounded(px(crate::config::corners::CONTROL))
                .when(primary, |button| button.bg(rgb(theme.active)))
                .when(!primary, |button| button.hover(|s| s.bg(rgb(theme.active))))
                .cursor_pointer()
                .child(label)
        };
        div()
            .debug_selector(|| "agent-skill-offer".into())
            .child(
                div()
                    .p(px(8.))
                    .child(fl!("window-notices-skill-offer-title")),
            )
            .child(
                div()
                    .p(px(8.))
                    .child(fl!("window-notices-skill-offer-body")),
            )
            .child(
                div()
                    .p(px(8.))
                    .text_color(rgb(theme.muted))
                    .child(fl!("window-notices-skill-offer-detail")),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .p(px(8.))
                    .child(
                        button(
                            "agent-skill-install",
                            fl!("window-notices-skill-offer-install"),
                            true,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.install_browser_skill(window, cx);
                        })),
                    )
                    .child(
                        button(
                            "agent-skill-decline",
                            fl!("window-notices-skill-offer-decline"),
                            false,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.dismiss_menu(window, cx);
                        })),
                    ),
            )
    }
}
