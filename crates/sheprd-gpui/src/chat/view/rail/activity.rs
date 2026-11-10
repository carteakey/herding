//! The rail's sections for what runs beside the conversation: sub-agents and
//! background shells, running first, with what ended folded under them.
//! Which agents report these, and which can stop them, is the bridges' to
//! say; nothing here knows which agent it shows.

use super::{ROWS, chat_section_rows};
use crate::{
    SheprdWindow,
    chat::{chats::ChatPane, rail_folds::RailSection},
    config::corners,
    fonts::StyledFont,
    i18n::fl,
    motion,
    notifications::safe_text,
};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::{Activity, ActivityKind, ActivityState, Command, EntryKind, Transcript};

/// A running row's hover group, which swaps its time for Stop.
const ROW: &str = "chat-rail-row";
/// The height of a running row, for growing a fold in and out.
const ROW_HEIGHT: f32 = 40.;

/// One section's activities: what runs in the order it started, then what
/// ended, most recent first.
#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct Group<'a> {
    pub(super) ended: Vec<&'a Activity>,
    pub(super) running: Vec<&'a Activity>,
}

impl<'a> Group<'a> {
    /// `section`'s share of `transcript`. A kind from a newer program is
    /// listed with the shells, as something left running.
    pub(super) fn of(transcript: &'a Transcript, section: RailSection) -> Self {
        let mine = |activity: &&Activity| match activity.kind {
            ActivityKind::Subagent => section == RailSection::Subagents,
            ActivityKind::Shell | ActivityKind::Other => section == RailSection::Shells,
            ActivityKind::Monitor => section == RailSection::Monitors,
        };
        let (running, mut ended): (Vec<_>, Vec<_>) = transcript
            .activities
            .iter()
            .filter(mine)
            .partition(|activity| activity.is_running());
        ended.sort_by_key(|activity| std::cmp::Reverse(activity.finished_ms));
        Self { ended, running }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.ended.is_empty() && self.running.is_empty()
    }

    /// What the section's header says.
    pub(super) fn summary(&self) -> String {
        match self.running.len() {
            0 => fl!("chat-rail-ended", count = self.ended.len()),
            count => fl!("chat-rail-running", count = count),
        }
    }
}

/// The time an activity has run, or ran.
fn elapsed(activity: &Activity, now_ms: u64) -> String {
    crate::chat::text::duration(
        activity
            .finished_ms
            .unwrap_or(now_ms)
            .saturating_sub(activity.started_ms),
    )
}

impl SheprdWindow {
    pub(super) fn render_chat_rail_activities(
        &self,
        chat: &ChatPane,
        section: RailSection,
        group: &Group<'_>,
        cx: &mut Context<Self>,
    ) -> Div {
        let now_ms = crate::chat::now_ms();
        let read_only = chat.session.transcript().read_only;
        let mut list = div().flex().flex_col().gap(px(4.));
        for activity in &group.running {
            list = list.child(self.render_chat_rail_running(chat, activity, read_only, now_ms, cx));
        }
        if group.ended.is_empty() {
            return list;
        }
        let now = motion::now();
        let shown = chat.rail_folds.ended_shown(section, now);
        let mut past = div().flex().flex_col().gap(px(4.));
        if shown > 0. {
            for activity in group.ended.iter().take(ROWS) {
                past = past.child(self.render_chat_rail_ended(chat, activity, now_ms, cx));
            }
        }
        list.child(self.render_chat_rail_ended_fold(chat, section, group.ended.len(), cx))
            .when(shown > 0., |list| {
                let full = chat_section_rows(group.ended.len()) * 24.;
                list.child(
                    div()
                        .overflow_hidden()
                        .when(shown < 1., |fold| fold.max_h(px(full * shown)))
                        .opacity(shown)
                        .child(past),
                )
            })
            .children(group.ended.len().gt(&ROWS).then(|| {
                div()
                    .text_size(self.chat_small_text())
                    .text_color(rgb(self.theme.muted))
                    .child(fl!("chat-more", count = group.ended.len() - ROWS))
            }))
    }

    fn render_chat_rail_ended_fold(
        &self,
        chat: &ChatPane,
        section: RailSection,
        count: usize,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let active = theme.active;
        let open = chat.rail_folds.ended_is_open(section);
        let pane = chat.target.pane_id.clone();
        let toggle = move |this: &mut Self, _: &mut Window, cx: &mut Context<Self>| {
            if let Some(chat) = this.chats.pane_mut(&pane) {
                chat.rail_folds.toggle_ended(section, motion::now());
            }
            cx.notify();
        };
        let view = cx.entity().downgrade();
        let row = div()
            .id(("chat-rail-ended", section.index()))
            .debug_selector(move || format!("chat-rail-ended-{}", section.index()));
        crate::accessibility::expandable(row, open, view, toggle.clone())
            .role(Role::Button)
            .mx(px(-4.))
            .px(px(4.))
            .py(px(2.))
            .rounded(px(corners::SMALL))
            .cursor_pointer()
            .hover(move |row| row.bg(rgb(active)))
            .flex()
            .items_center()
            .gap(px(5.))
            .text_size(self.chat_small_text())
            .text_color(rgb(theme.muted))
            .child(super::super::disclosure::chevron(open, theme))
            .child(fl!("chat-rail-ended", count = count))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                toggle(this, window, cx);
            }))
    }

    /// A row that goes to the card of the call that started `activity`.
    fn chat_rail_row(
        &self,
        chat: &ChatPane,
        activity: &Activity,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let active = self.theme.active;
        let pane = chat.target.pane_id.clone();
        let call = activity.call_id.clone();
        div()
            .id(SharedString::from(format!(
                "chat-rail-row-{}",
                activity.call_id
            )))
            .mx(px(-4.))
            .px(px(4.))
            .py(px(2.))
            .rounded(px(corners::SMALL))
            .cursor_pointer()
            .hover(move |row| row.bg(rgb(active)))
            .flex()
            .gap(px(6.))
            .text_size(self.chat_small_text())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.go_to_chat_call(&pane, &call, cx);
            }))
    }

    fn go_to_chat_call(&mut self, pane_id: &str, call: &str, cx: &mut Context<Self>) {
        let index = self.chats.pane_mut(pane_id).and_then(|chat| {
            chat.session.transcript().entries.iter().position(
                |entry| matches!(&entry.kind, EntryKind::Tool(tool) if tool.call_id == call),
            )
        });
        // A card dropped to keep the transcript bounded has nowhere to go.
        if let Some(index) = index {
            self.go_to_chat_prompt(pane_id, index, cx);
        }
    }

    fn chat_rail_label(&self, activity: &Activity) -> Div {
        let text = div()
            .flex_1()
            .min_w_0()
            .truncate()
            .child(safe_text(&activity.label, 200));
        if activity.kind == ActivityKind::Shell {
            text.text_font(&self.config.terminal)
                .text_size(self.chat_small_text())
        } else {
            text
        }
    }

    fn render_chat_rail_running(
        &self,
        chat: &ChatPane,
        activity: &Activity,
        read_only: bool,
        now_ms: u64,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let time = div()
            .text_color(rgb(theme.muted))
            .child(elapsed(activity, now_ms));
        let stop =
            (activity.stoppable && !read_only).then(|| self.chat_rail_stop(chat, activity, cx));
        let slot = div()
            .relative()
            .flex_none()
            .min_w(px(18.))
            .h(px(18.))
            .flex()
            .items_center()
            .justify_end();
        let slot = match stop {
            Some(stop) => slot
                .child(time.group_hover(ROW, |time| time.opacity(0.)))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .opacity(0.)
                        .group_hover(ROW, |stop| stop.opacity(1.))
                        .child(stop),
                ),
            None => slot.child(time),
        };
        self.chat_rail_row(chat, activity, cx)
            .group(ROW)
            .child(self.chat_rail_mark(activity.state))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(self.chat_rail_label(activity))
                            .child(slot),
                    )
                    .children(activity.detail.as_ref().map(|detail| {
                        div()
                            .truncate()
                            .text_color(rgb(theme.muted))
                            .child(safe_text(detail, 200))
                    })),
            )
    }

    /// Stops at once: it ends one piece of work the user is pointing at, and
    /// the row says how it ended when the agent does.
    fn chat_rail_stop(
        &self,
        chat: &ChatPane,
        activity: &Activity,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let active = theme.active;
        let pane = chat.target.pane_id.clone();
        let command = Command::StopActivity {
            call_id: activity.call_id.clone(),
        };
        div()
            .id(SharedString::from(format!(
                "chat-rail-stop-{}",
                activity.call_id
            )))
            .debug_selector({
                let call = activity.call_id.clone();
                move || format!("chat-rail-stop-{call}")
            })
            .role(Role::Button)
            .aria_label(fl!(
                "chat-rail-stop",
                what = safe_text(&activity.label, 200)
            ))
            .size(px(18.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(corners::CONTROL))
            .cursor_pointer()
            .hover(move |button| button.bg(rgb(active)))
            .child(
                div()
                    .size(px(8.))
                    .rounded(px(1.5))
                    .bg(rgb(theme.ink(theme.palette[1]))),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.chat_send(&pane, command.clone(), cx);
            }))
    }

    fn render_chat_rail_ended(
        &self,
        chat: &ChatPane,
        activity: &Activity,
        now_ms: u64,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let (text, color) = match activity.state {
            ActivityState::Failed => (fl!("chat-running-failed"), theme.ink(theme.palette[1])),
            ActivityState::Killed => (fl!("chat-running-stopped"), theme.muted),
            ActivityState::Running | ActivityState::Finished => {
                (elapsed(activity, now_ms), theme.muted)
            }
        };
        self.chat_rail_row(chat, activity, cx)
            .items_center()
            .child(self.chat_rail_mark(activity.state).mt(px(0.)))
            .child(
                self.chat_rail_label(activity)
                    .text_color(rgb(theme.subtext())),
            )
            .child(div().flex_none().text_color(rgb(color)).child(text))
    }

    fn chat_rail_mark(&self, state: ActivityState) -> Svg {
        let theme = &self.theme;
        let (icon, color) = match state {
            ActivityState::Running => (
                "icons/status-solid-working.svg",
                theme.ink(theme.palette[3]),
            ),
            ActivityState::Finished => ("icons/status-solid-done.svg", theme.ink(theme.palette[2])),
            ActivityState::Failed => ("icons/x.svg", theme.ink(theme.palette[1])),
            ActivityState::Killed => ("icons/status-outline-idle.svg", theme.muted),
        };
        svg()
            .flex_none()
            .mt(px(2.))
            .size(px(11.))
            .path(icon)
            .text_color(rgb(color))
    }
}

/// How tall a section of `running` rows grows to, for its fold.
pub(super) fn running_height(running: usize) -> f32 {
    running as f32 * ROW_HEIGHT
}
