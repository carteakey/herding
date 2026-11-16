//! The rail beside the entries: what runs beside the conversation, the
//! agent's plan, and the files its tool calls changed, each a section that
//! folds to one line. It hides on request and in panes too narrow for it, and
//! the header then keeps the plan's progress at hand.

mod activity;

use self::activity::{Group, running_height};
use super::{
    super::{chats::ChatPane, prepared::total_lines},
    Hues,
    disclosure::chevron,
};
use crate::i18n::fl;
use crate::{
    SheprdWindow, chat::rail_folds::RailSection, config::corners, motion, notifications::safe_text,
};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::PlanStatus;

/// Narrowest pane that shows the rail beside the entries.
const MIN_PANE: f32 = 640.;
const WIDTH: f32 = 240.;
/// Plan steps, changed files, and ended work listed; the rest are counted.
const ROWS: usize = 24;
/// The height of a plan step or a changed file, for growing a fold.
const LINE: f32 = 26.;

/// The rows a section of `count` lists.
fn chat_section_rows(count: usize) -> f32 {
    count.min(ROWS) as f32
}

/// What a section's header says.
struct Heading {
    /// How tall the section's body grows to, for its fold.
    full: f32,
    /// The plan's current step while the plan is folded. It is the agent's
    /// text, so it stays apart from the words around it.
    step: Option<String>,
    summary: String,
    title: String,
}

/// Whether a pane of `bounds` is wide enough for the rail.
pub(super) fn fits(bounds: Bounds<Pixels>) -> bool {
    bounds.size.width >= px(MIN_PANE)
}

/// The last two parts of a path, which name a file well enough in the rail.
pub(super) fn short_path(path: &str) -> &str {
    let mut cuts = path.rmatch_indices(['/', '\\']).map(|(index, _)| index);
    cuts.nth(1).map_or(path, |cut| &path[cut + 1..])
}

impl SheprdWindow {
    fn chat_rail_has_content(chat: &ChatPane) -> bool {
        let transcript = chat.session.transcript();
        !transcript.plan.is_empty()
            || !transcript.activities.is_empty()
            || !chat.prepared.changed().is_empty()
    }

    /// Whether the rail is drawn in a pane of `bounds`.
    pub(super) fn chat_rail_shows(chat: &ChatPane, bounds: Bounds<Pixels>) -> bool {
        fits(bounds) && chat.rail && Self::chat_rail_has_content(chat)
    }

    /// A section's header, which folds it, over its `body`.
    fn render_chat_rail_section(
        &self,
        chat: &ChatPane,
        section: RailSection,
        busy: bool,
        heading: Heading,
        body: Div,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let active = theme.active;
        let heading_full = heading.full;
        let open = chat.rail_folds.is_open(section, busy);
        let shown = chat.rail_folds.shown(section, busy, motion::now());
        let pane = chat.target.pane_id.clone();
        let toggle = move |this: &mut Self, _: &mut Window, cx: &mut Context<Self>| {
            if let Some(chat) = this.chats.pane_mut(&pane) {
                chat.rail_folds.toggle(section, busy, motion::now());
            }
            cx.notify();
        };
        let view = cx.entity().downgrade();
        let header = div()
            .id(("chat-rail-section", section.index()))
            .debug_selector(move || format!("chat-rail-section-{}", section.index()));
        let header = crate::accessibility::expandable(header, open, view, toggle.clone())
            .role(Role::Button)
            .mx(px(-4.))
            .px(px(4.))
            .py(px(3.))
            .rounded(px(corners::SMALL))
            .cursor_pointer()
            .hover(move |row| row.bg(rgb(active)))
            .flex()
            .items_center()
            .gap(px(5.))
            .text_size(self.chat_small_text())
            .child(chevron(open, theme))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.subtext()))
                    .child(heading.title),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .justify_end()
                    .gap(px(4.))
                    .text_color(rgb(theme.muted))
                    .child(div().flex_none().child(heading.summary))
                    .children(heading.step.map(|step| {
                        div()
                            .flex()
                            .min_w_0()
                            .gap(px(4.))
                            .child(div().flex_none().child("\u{b7}"))
                            .child(div().min_w_0().truncate().child(step))
                    })),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                toggle(this, window, cx);
            }));
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(header)
            .when(shown > 0., |section| {
                section.child(
                    div()
                        .overflow_hidden()
                        .when(shown < 1., |fold| fold.max_h(px(heading_full * shown)))
                        .opacity(shown)
                        .child(body),
                )
            })
    }

    /// The header's rail button, and the plan's progress while the rail is
    /// not shown.
    pub(super) fn render_chat_rail_controls(
        &self,
        chat: &ChatPane,
        fits: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !Self::chat_rail_has_content(chat) {
            return Vec::new();
        }
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let plan = &chat.session.transcript().plan;
        let shown = fits && chat.rail;
        let mut controls = Vec::new();
        if !shown && !plan.is_empty() {
            let done = plan
                .iter()
                .filter(|step| step.status == PlanStatus::Completed)
                .count();
            let working = plan
                .iter()
                .any(|step| step.status == PlanStatus::InProgress);
            controls.push(
                div()
                    .debug_selector(|| "chat-plan-pill".into())
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(7.))
                    .h(px(20.))
                    .rounded_full()
                    .bg(rgb(theme.surface))
                    .text_size(px(self.config.ui.size))
                    .text_color(rgb(theme.subtext()))
                    .child(
                        svg()
                            .path(if working {
                                "icons/status-solid-working.svg"
                            } else {
                                "icons/status-solid-done.svg"
                            })
                            .size(px(9.))
                            .text_color(rgb(if working { hues.yellow } else { hues.green })),
                    )
                    .child(fl!("chat-plan-progress", done = done, total = plan.len()))
                    .into_any_element(),
            );
        }
        if fits {
            let pane_id = chat.target.pane_id.clone();
            controls.push(
                div()
                    .id("chat-rail-toggle")
                    .debug_selector(|| "chat-rail-toggle".into())
                    .flex_none()
                    .flex()
                    .items_center()
                    .px(px(6.))
                    .h(px(22.))
                    .rounded(px(corners::CONTROL))
                    .cursor_pointer()
                    .when(shown, |button| button.bg(rgb(theme.active)))
                    .hover(|button| button.bg(rgb(theme.active)))
                    .child(
                        svg()
                            .path("icons/panel-right.svg")
                            .size(px(13.))
                            .text_color(rgb(theme.foreground)),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        if let Some(pane) = this.chats.pane_mut(&pane_id) {
                            pane.rail = !pane.rail;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        controls
    }

    pub(super) fn render_chat_rail(
        &self,
        chat: &ChatPane,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if !Self::chat_rail_shows(chat, bounds) {
            return None;
        }
        let theme = &self.theme;
        let transcript = chat.session.transcript();
        let busy = transcript.running().total() > 0;
        let work = [
            (RailSection::Subagents, fl!("chat-rail-subagents")),
            (RailSection::Shells, fl!("chat-rail-shells")),
            (RailSection::Monitors, fl!("chat-rail-monitors")),
        ]
        .into_iter()
        .filter_map(|(section, title)| {
            let group = Group::of(transcript, section);
            (!group.is_empty()).then(|| {
                let body = self.render_chat_rail_activities(chat, section, &group, cx);
                // What ended unfolds on its own, so this only grows the rows
                // that run and the line that opens the rest.
                let full = running_height(group.running.len()) + LINE;
                self.render_chat_rail_section(
                    chat,
                    section,
                    busy,
                    Heading {
                        full,
                        step: None,
                        summary: group.summary(),
                        title,
                    },
                    body,
                    cx,
                )
            })
        })
        .collect::<Vec<_>>();
        let plan = self.render_chat_rail_plan(chat, busy, cx);
        let files = self.render_chat_rail_changed(chat, busy, cx);
        let moving = chat.rail_folds.moving(motion::now());
        Some(
            div()
                .id("chat-rail")
                .debug_selector(|| "chat-rail".into())
                .flex_none()
                .w(px(WIDTH))
                .h_full()
                .flex()
                .flex_col()
                .gap(px(8.))
                .p(px(12.))
                .overflow_y_scroll()
                .border_l_1()
                .border_color(rgb(theme.active))
                .bg(rgb(theme.surface))
                .children(work)
                .children(plan)
                .children(files)
                .when(moving, |rail| {
                    // Draws the next frame while a fold still moves.
                    rail.child(
                        canvas(
                            |_, window, _| window.request_animation_frame(),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_0(),
                    )
                }),
        )
    }

    fn chat_rail_more(&self, count: usize) -> Option<Div> {
        (count > ROWS).then(|| {
            div()
                .text_size(self.chat_small_text())
                .text_color(rgb(self.theme.muted))
                .child(fl!("chat-more", count = count - ROWS))
        })
    }

    fn render_chat_rail_plan(
        &self,
        chat: &ChatPane,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let plan = &chat.session.transcript().plan;
        if plan.is_empty() {
            return None;
        }
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let small = self.chat_small_text();
        let done = plan
            .iter()
            .filter(|step| step.status == PlanStatus::Completed)
            .count();
        let step = plan
            .iter()
            .find(|step| step.status == PlanStatus::InProgress)
            .filter(|_| !chat.rail_folds.is_open(RailSection::Plan, busy))
            .map(|step| safe_text(&step.content, 80));
        let body = div()
            .debug_selector(|| "chat-plan".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(plan.iter().take(ROWS).map(|step| {
                let (icon, color) = match step.status {
                    PlanStatus::Completed => ("icons/status-solid-done.svg", hues.green),
                    PlanStatus::InProgress => ("icons/status-solid-working.svg", hues.yellow),
                    PlanStatus::Pending => ("icons/status-outline-idle.svg", theme.muted),
                };
                div()
                    .flex()
                    .gap(px(6.))
                    .text_size(small)
                    .child(
                        svg()
                            .flex_none()
                            .mt(px(2.))
                            .path(icon)
                            .size(px(11.))
                            .text_color(rgb(color)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .when(step.status == PlanStatus::Completed, |text| {
                                text.text_color(rgb(theme.subtext())).line_through()
                            })
                            .when(step.status == PlanStatus::InProgress, |text| {
                                text.font_weight(FontWeight::SEMIBOLD)
                            })
                            .child(safe_text(&step.content, 300)),
                    )
            }))
            .children(self.chat_rail_more(plan.len()));
        Some(self.render_chat_rail_section(
            chat,
            RailSection::Plan,
            busy,
            Heading {
                full: chat_section_rows(plan.len()) * LINE,
                step,
                summary: format!("{done}/{}", plan.len()),
                title: fl!("chat-rail-plan"),
            },
            body,
            cx,
        ))
    }

    fn render_chat_rail_changed(
        &self,
        chat: &ChatPane,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let changed = chat.prepared.changed();
        if changed.is_empty() {
            return None;
        }
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let small = self.chat_small_text();
        let count = fl!("chat-rail-files", count = changed.len());
        // One uncounted file makes the total unknown: a partial sum would
        // read as the whole change.
        let summary = match total_lines(changed) {
            Some(lines) if !chat.rail_folds.is_open(RailSection::Changed, busy) => {
                format!("{count} +{} \u{2212}{}", lines.added, lines.removed)
            }
            _ => count,
        };
        let body = div()
            .debug_selector(|| "chat-changed".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(changed.iter().take(ROWS).map(|diff| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(small)
                    .child(
                        svg()
                            .flex_none()
                            .path("icons/file.svg")
                            .size(px(11.))
                            .text_color(rgb(theme.subtext())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(safe_text(short_path(&diff.path), 200)),
                    )
                    .children(diff.lines.iter().flat_map(|lines| {
                        [
                            div()
                                .flex_none()
                                .text_color(rgb(hues.green))
                                .child(format!("+{}", lines.added)),
                            div()
                                .flex_none()
                                .text_color(rgb(hues.red))
                                .child(format!("\u{2212}{}", lines.removed)),
                        ]
                    }))
            }))
            .children(self.chat_rail_more(changed.len()));
        Some(self.render_chat_rail_section(
            chat,
            RailSection::Changed,
            busy,
            Heading {
                full: chat_section_rows(changed.len()) * LINE,
                step: None,
                summary,
                title: fl!("chat-rail-changed"),
            },
            body,
            cx,
        ))
    }
}

#[cfg(test)]
mod tests;
