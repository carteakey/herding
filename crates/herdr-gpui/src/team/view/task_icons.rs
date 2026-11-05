//! Self-describing icons for task rows (issue #9): status, assignee role,
//! and gate badges. Every badge names itself on hover and to screen
//! readers; none are interactive, so keyboard order is unchanged.

use super::board::truncate_text;
use crate::config::Theme;
use crate::i18n::fl;
use gpui::{prelude::*, *};
use herdr_orchestrator::team_store::TaskStatus;

use crate::team::model::{TeamMember, TeamTask};

/// The localized status label shared by the status chip and its tooltip.
pub(super) fn task_status_label(status: TaskStatus) -> String {
    match status {
        TaskStatus::Open => fl!("team-task-status-open"),
        TaskStatus::InProgress => fl!("team-task-status-in-progress"),
        TaskStatus::Blocked => fl!("team-task-status-blocked"),
        TaskStatus::Done => fl!("team-task-status-done"),
        TaskStatus::Cancelled => fl!("team-task-status-cancelled"),
    }
}

/// The status mark for a task, drawn from the 16px status-mark family so
/// task rows share one visual language with the agent badges: a hollow ring
/// for open work, a half-filled ring for claimed work, a crossed ring for
/// blocked, a check for done, and a bare cross for cancelled.
pub(super) fn task_status_icon(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "icons/status-mark-idle.svg",
        TaskStatus::InProgress => "icons/status-mark-working.svg",
        TaskStatus::Blocked => "icons/status-outline-blocked.svg",
        TaskStatus::Done => "icons/status-mark-done.svg",
        TaskStatus::Cancelled => "icons/status-mark-blocked.svg",
    }
}

/// The mark for the assignee's roster role: a star for a supervisor, a
/// person for a worker. An assignee outside the roster still gets the
/// person mark, with an assignee tooltip instead of a role.
pub(super) fn task_role_icon(supervisor: Option<bool>) -> &'static str {
    match supervisor {
        Some(true) => "icons/star-full.svg",
        _ => "icons/user.svg",
    }
}

/// The gate mark: a recorded blocker reason gates the task.
pub(super) fn task_gate_icon() -> &'static str {
    "icons/lock.svg"
}

fn status_tooltip(status: TaskStatus) -> String {
    fl!(
        "team-task-tooltip-status",
        status = task_status_label(status).as_str()
    )
}

/// The gate badge's tooltip: what gates the task, folded to one line.
fn gate_tooltip(reason: &str) -> String {
    fl!(
        "team-task-tooltip-gate",
        reason = truncate_text(reason, 80).as_str()
    )
}

/// One informational badge: an icon that names itself on hover and to
/// screen readers. Not interactive, so keyboard order is unchanged.
fn badge(
    id: SharedString,
    icon: &'static str,
    tooltip: String,
    foreground: u32,
    surface: u32,
    muted: u32,
) -> Stateful<Div> {
    let selector = id.to_string();
    let label: SharedString = tooltip.clone().into();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .aria_label(label)
        .tooltip(move |_, cx| {
            cx.new(|_| crate::menu::SettingsHint {
                text: tooltip.clone().into(),
                foreground,
                surface,
            })
            .into()
        })
        .child(svg().path(icon).size(px(12.)).text_color(rgb(muted)))
}

/// The row of self-describing badges for a task card: a status mark always,
/// the assignee's roster role when assigned, and the gate when a blocker
/// reason is recorded on the board.
pub(super) fn task_icon_badges(theme: &Theme, task: &TeamTask, members: &[TeamMember]) -> Div {
    let (foreground, surface, muted) = (theme.foreground, theme.surface, theme.muted);
    let mut row = div().flex().flex_none().items_center().gap_1();

    row = row.child(badge(
        SharedString::from(format!("team-task-status-icon-{}", task.id)),
        task_status_icon(task.status),
        status_tooltip(task.status),
        foreground,
        surface,
        muted,
    ));

    if let Some(assignee) = task.assignee.as_ref() {
        let supervisor = members
            .iter()
            .find(|m| &m.name == assignee)
            .map(|m| m.supervisor);
        let tooltip = match supervisor {
            Some(true) => fl!("team-task-tooltip-supervisor"),
            Some(false) => fl!("team-task-tooltip-worker"),
            None => fl!("team-task-assignee", assignee = assignee.as_ref()),
        };
        row = row.child(badge(
            SharedString::from(format!("team-task-role-icon-{}", task.id)),
            task_role_icon(supervisor),
            tooltip,
            foreground,
            surface,
            muted,
        ));
    }

    if let Some(reason) = task.blocker_reason.as_ref() {
        row = row.child(badge(
            SharedString::from(format!("team-task-gate-icon-{}", task.id)),
            task_gate_icon(),
            gate_tooltip(reason.as_ref()),
            foreground,
            surface,
            muted,
        ));
    }

    row
}

#[cfg(test)]
mod tests {
    use super::*;
    // `use super::*` re-exports gpui's glob, which carries gpui's own
    // `test` attribute; restore the built-in so `#[test]` stays std.
    use core::prelude::v1::test;
    use std::collections::HashSet;

    #[test]
    fn each_task_status_has_its_own_status_mark() {
        let statuses = [
            TaskStatus::Open,
            TaskStatus::InProgress,
            TaskStatus::Blocked,
            TaskStatus::Done,
            TaskStatus::Cancelled,
        ];
        let distinct: HashSet<_> = statuses.map(task_status_icon).into_iter().collect();
        assert_eq!(distinct.len(), statuses.len(), "one icon per status");
        assert_eq!(
            task_status_icon(TaskStatus::Open),
            "icons/status-mark-idle.svg"
        );
        assert_eq!(
            task_status_icon(TaskStatus::InProgress),
            "icons/status-mark-working.svg"
        );
        assert_eq!(
            task_status_icon(TaskStatus::Blocked),
            "icons/status-outline-blocked.svg"
        );
        assert_eq!(
            task_status_icon(TaskStatus::Done),
            "icons/status-mark-done.svg"
        );
        assert_eq!(
            task_status_icon(TaskStatus::Cancelled),
            "icons/status-mark-blocked.svg"
        );
    }

    #[test]
    fn the_role_mark_follows_the_roster_role() {
        assert_eq!(task_role_icon(Some(true)), "icons/star-full.svg");
        assert_eq!(task_role_icon(Some(false)), "icons/user.svg");
        assert_eq!(task_role_icon(None), "icons/user.svg");
        assert_eq!(task_gate_icon(), "icons/lock.svg");
    }

    // Tooltip text is not asserted here: `fl!` inside a `#[test]` blows the
    // macro expansion budget. Its compile-time check already pins the
    // message ids and argument names, and the layout test in view/tests.rs
    // exercises the badges end to end instead.
}
