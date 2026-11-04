use super::{
    MAX_VISIBLE_TASKS, RECENT_ACTIVE_TASKS_LIMIT, RECENT_COMPLETED_TASKS_LIMIT, TeamAction,
    TeamView,
};
use crate::i18n::fl;
use gpui::{prelude::*, *};
use herdr_orchestrator::team_store::TaskStatus;

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let mut s: String = text.chars().take(max_chars).collect();
        s.push_str("...");
        s
    }
}

impl TeamView {
    pub(super) fn render_goal(&self, team_id: &str, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let team_id = team_id.to_owned();
        let has_goal = self.snapshot.team_goal.is_some();
        let show_editor = self.editing_goal || !has_goal;
        let current_goal = self
            .snapshot
            .team_goal
            .as_ref()
            .map(|g| g.to_string())
            .unwrap_or_default();

        let mut card = div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active));

        let mut header = div().flex().items_center().justify_between().gap_2().child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(13.))
                .child(fl!("team-goal-title").to_uppercase()),
        );
        if has_goal && !self.editing_goal {
            header = header.child(
                self.icon_button(
                    "team-goal-edit",
                    "icons/pencil.svg",
                    fl!("team-action-edit"),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let current_goal = current_goal.clone();
                    this.editing_goal = true;
                    this.team_goal.update(cx, |input, cx| {
                        input.set_text_at(&current_goal, current_goal.len(), cx);
                    });
                    cx.notify();
                })),
            );
        }
        card = card.child(header);

        card = card.child(match self.snapshot.team_goal.as_ref() {
            Some(goal) => div().text_size(px(13.)).child(goal.clone()),
            None => div()
                .text_size(px(12.))
                .text_color(rgb(theme.muted))
                .child(fl!("team-goal-empty")),
        });

        if show_editor {
            let mut controls = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(div().flex_1().min_w(px(160.)).child(self.team_goal.clone()))
                .child(
                    div()
                        .id("team-goal-save")
                        .px_3()
                        .py_1()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.active))
                        .text_size(px(12.))
                        .cursor_pointer()
                        .role(Role::Button)
                        .child(fl!("team-action-save-goal"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let goal = this.team_goal.read(cx).text().trim().to_owned();
                            if goal.is_empty() {
                                return;
                            }
                            this.editing_goal = false;
                            this.team_goal.update(cx, |input, cx| input.clear(cx));
                            cx.emit(TeamAction::SetGoal {
                                team_id: team_id.clone(),
                                goal,
                            });
                        })),
                );
            if self.editing_goal {
                controls = controls.child(
                    div()
                        .id("team-goal-cancel")
                        .px_2()
                        .py_1()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.background))
                        .text_size(px(12.))
                        .cursor_pointer()
                        .role(Role::Button)
                        .child(fl!("team-action-cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.editing_goal = false;
                            cx.notify();
                        })),
                );
            }
            card = card.child(controls);
        }
        card
    }

    pub(super) fn render_tasks(&self, team_id: &str, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let team_id_for_create = team_id.to_owned();
        let active_count = self
            .snapshot
            .tasks
            .iter()
            .filter(|t| !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled))
            .count();

        let mut section = div().flex().flex_col().gap_2();
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(14.))
                        .child(format!(
                            "{} ({}/{})",
                            fl!("team-tasks-title"),
                            active_count,
                            self.snapshot.tasks.len()
                        )),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .id("team-issue-toggle-form")
                                .px_2()
                                .py(px(2.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(theme.active))
                                .text_size(px(11.))
                                .cursor_pointer()
                                .role(Role::Button)
                                .child(if self.issue_form_open {
                                    fl!("team-action-cancel")
                                } else {
                                    fl!("team-action-import-issue")
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.issue_form_open = !this.issue_form_open;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("team-task-toggle-form")
                                .px_2()
                                .py(px(2.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(theme.active))
                                .text_size(px(11.))
                                .cursor_pointer()
                                .role(Role::Button)
                                .child(if self.task_form_open || self.editing_task_id.is_some() {
                                    fl!("team-action-cancel")
                                } else {
                                    fl!("team-action-add-task")
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.task_form_open || this.editing_task_id.is_some() {
                                        this.task_form_open = false;
                                        this.editing_task_id = None;
                                        this.task_title.update(cx, |i, cx| i.clear(cx));
                                        this.task_brief.update(cx, |i, cx| i.clear(cx));
                                        this.task_assignee.update(cx, |i, cx| i.clear(cx));
                                    } else {
                                        this.task_form_open = true;
                                    }
                                    cx.notify();
                                })),
                        ),
                ),
        );

        if self.issue_form_open {
            let team_id_issue_task = team_id.to_owned();
            let team_id_issue_goal = team_id.to_owned();
            section = section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .child(self.issue_input.clone())
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("team-issue-import-task")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(crate::config::corners::CONTROL))
                                    .bg(rgb(theme.active))
                                    .text_size(px(11.))
                                    .cursor_pointer()
                                    .role(Role::Button)
                                    .child(fl!("team-action-import-issue-task"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let issue_ref =
                                            this.issue_input.read(cx).text().trim().to_owned();
                                        if issue_ref.is_empty() {
                                            return;
                                        }
                                        this.issue_input.update(cx, |i, cx| i.clear(cx));
                                        this.issue_form_open = false;
                                        cx.emit(TeamAction::ImportGithubIssue {
                                            team_id: team_id_issue_task.clone(),
                                            issue_ref,
                                            as_goal: false,
                                        });
                                    })),
                            )
                            .child(
                                div()
                                    .id("team-issue-import-goal")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(crate::config::corners::CONTROL))
                                    .bg(rgb(theme.background))
                                    .text_size(px(11.))
                                    .cursor_pointer()
                                    .role(Role::Button)
                                    .child(fl!("team-action-import-issue-goal"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let issue_ref =
                                            this.issue_input.read(cx).text().trim().to_owned();
                                        if issue_ref.is_empty() {
                                            return;
                                        }
                                        this.issue_input.update(cx, |i, cx| i.clear(cx));
                                        this.issue_form_open = false;
                                        cx.emit(TeamAction::ImportGithubIssue {
                                            team_id: team_id_issue_goal.clone(),
                                            issue_ref,
                                            as_goal: true,
                                        });
                                    })),
                            ),
                    ),
            );
        }

        if self.task_form_open || self.editing_task_id.is_some() || self.snapshot.tasks.is_empty() {
            let is_editing = self.editing_task_id.is_some();
            section = section.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .child(self.task_title.clone())
                    .child(self.task_brief.clone())
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(140.))
                                    .child(self.task_assignee.clone()),
                            )
                            .child(
                                div()
                                    .id("team-task-create")
                                    .px_3()
                                    .py_1()
                                    .rounded(px(crate::config::corners::CONTROL))
                                    .bg(rgb(theme.active))
                                    .text_size(px(12.))
                                    .cursor_pointer()
                                    .role(Role::Button)
                                    .child(if is_editing {
                                        fl!("team-action-save-task")
                                    } else {
                                        fl!("team-action-add-task")
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let title =
                                            this.task_title.read(cx).text().trim().to_owned();
                                        if title.is_empty() {
                                            return;
                                        }
                                        let brief =
                                            this.task_brief.read(cx).text().trim().to_owned();
                                        let assignee =
                                            this.task_assignee.read(cx).text().trim().to_owned();
                                        let assignee = (!assignee.is_empty()).then_some(assignee);
                                        this.task_title.update(cx, |input, cx| input.clear(cx));
                                        this.task_brief.update(cx, |input, cx| input.clear(cx));
                                        this.task_assignee.update(cx, |input, cx| input.clear(cx));
                                        this.task_form_open = false;
                                        if let Some(task_id) = this.editing_task_id.take() {
                                            cx.emit(TeamAction::UpdateTask {
                                                team_id: team_id_for_create.clone(),
                                                task_id,
                                                title,
                                                brief,
                                                assignee,
                                            });
                                        } else {
                                            cx.emit(TeamAction::CreateTask {
                                                team_id: team_id_for_create.clone(),
                                                title,
                                                brief,
                                                assignee,
                                            });
                                        }
                                    })),
                            ),
                    ),
            );
        }

        if self.snapshot.tasks.is_empty() {
            section = section.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(theme.muted))
                    .child(fl!("team-tasks-empty")),
            );
        } else {
            // The window reads from the newest end so current work stays
            // visible; the show-earlier control widens it toward older rows.
            let total = self.snapshot.tasks.len();
            let visible = (MAX_VISIBLE_TASKS + self.earlier_tasks_rows).min(total);
            let (active_tasks, completed_tasks): (Vec<_>, Vec<_>) = self.snapshot.tasks
                [total - visible..]
                .iter()
                .partition(|t| !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled));

            let active_total = active_tasks.len();
            let visible_active_count = if self.active_tasks_show_all {
                active_total
            } else {
                active_total.min(RECENT_ACTIVE_TASKS_LIMIT)
            };
            for task in active_tasks.iter().take(visible_active_count) {
                section = section.child(self.render_task_card(task, team_id, cx));
            }

            if active_total > RECENT_ACTIVE_TASKS_LIMIT {
                let (label, show_all) = if self.active_tasks_show_all {
                    (
                        format!("Show fewer open tasks ({RECENT_ACTIVE_TASKS_LIMIT})"),
                        false,
                    )
                } else {
                    let more = active_total - RECENT_ACTIVE_TASKS_LIMIT;
                    (
                        format!("Show all {active_total} open tasks ({more} more)"),
                        true,
                    )
                };
                section = section.child(
                    div()
                        .id("team-tasks-active-toggle")
                        .debug_selector(|| "team-tasks-active-toggle".into())
                        .px_2()
                        .py_1()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.surface))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .role(Role::Button)
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.active_tasks_show_all = show_all;
                            cx.notify();
                        })),
                );
            }

            if !completed_tasks.is_empty() {
                let count = completed_tasks.len();
                let toggle_label = if self.completed_tasks_open {
                    format!("▼ Completed ({count})")
                } else {
                    format!("▶ Completed ({count})")
                };
                section = section.child(
                    div()
                        .id("team-tasks-completed-toggle")
                        .px_2()
                        .py_1()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.surface))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .role(Role::Button)
                        .child(toggle_label)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.completed_tasks_open = !this.completed_tasks_open;
                            cx.notify();
                        })),
                );
                if self.completed_tasks_open {
                    let visible_completed = if self.completed_tasks_show_all {
                        count
                    } else {
                        count.min(RECENT_COMPLETED_TASKS_LIMIT)
                    };
                    for task in completed_tasks.iter().take(visible_completed) {
                        section = section.child(self.render_task_card(task, team_id, cx));
                    }
                    if count > RECENT_COMPLETED_TASKS_LIMIT {
                        let (label, show_all) = if self.completed_tasks_show_all {
                            (
                                format!(
                                    "Show fewer completed tasks ({RECENT_COMPLETED_TASKS_LIMIT})"
                                ),
                                false,
                            )
                        } else {
                            let more = count - RECENT_COMPLETED_TASKS_LIMIT;
                            (
                                format!("Show all {count} completed tasks ({more} more)"),
                                true,
                            )
                        };
                        section = section.child(
                            div()
                                .id("team-tasks-completed-show-more")
                                .debug_selector(|| "team-tasks-completed-show-more".into())
                                .px_2()
                                .py_1()
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(theme.surface))
                                .border_1()
                                .border_color(rgb(theme.active))
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .role(Role::Button)
                                .child(label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.completed_tasks_show_all = show_all;
                                    cx.notify();
                                })),
                        );
                    }
                }
            }

            if total > MAX_VISIBLE_TASKS + self.earlier_tasks_rows || self.snapshot.tasks_older {
                section = section.child(
                    div()
                        .id("team-tasks-load-earlier")
                        .debug_selector(|| "team-tasks-load-earlier".into())
                        .px_2()
                        .py_1()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.surface))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .role(Role::Button)
                        .child("Load earlier tasks")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.earlier_tasks_rows += MAX_VISIBLE_TASKS;
                            if this.snapshot.tasks_older {
                                cx.emit(TeamAction::LoadEarlierTasks);
                            }
                            cx.notify();
                        })),
                );
            }
        }
        section
    }

    fn render_task_card(
        &self,
        task: &crate::team::model::TeamTask,
        team_id: &str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let task_id = task.id.clone();
        let is_expanded = self.expanded_task_ids.contains(&task.id);
        let status_label = match task.status {
            TaskStatus::Open => fl!("team-task-status-open"),
            TaskStatus::InProgress => fl!("team-task-status-in-progress"),
            TaskStatus::Blocked => fl!("team-task-status-blocked"),
            TaskStatus::Done => fl!("team-task-status-done"),
            TaskStatus::Cancelled => fl!("team-task-status-cancelled"),
        };
        let assignee_text = task
            .assignee
            .as_ref()
            .map(|a| fl!("team-task-assignee", assignee = a.as_ref()))
            .unwrap_or_else(|| fl!("team-task-unassigned"));
        let task_id_sel = task_id.clone();
        let mut card = div()
            .id(SharedString::from(format!("team-task-{}", task.id)))
            .debug_selector(move || format!("team-task-{task_id_sel}"))
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(13.))
                            .child(task.title.clone()),
                    )
                    .child(
                        div()
                            .px_2()
                            .py(px(1.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.background))
                            .text_size(px(11.))
                            .text_color(rgb(theme.muted))
                            .child(status_label),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(theme.muted))
                    .child(assignee_text),
            );
        if !task.brief.is_empty() {
            let is_long = task.brief.len() > 120 || task.brief.lines().count() > 2;
            let tid = task_id.clone();
            if is_long && !is_expanded {
                let preview = truncate_text(&task.brief, 120);
                card = card.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_size(px(12.)).child(preview))
                        .child(
                            div()
                                .id(SharedString::from(format!("task-brief-more-{tid}")))
                                .text_size(px(10.))
                                .text_color(rgb(theme.muted))
                                .cursor_pointer()
                                .role(Role::Button)
                                .child("More")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.expanded_task_ids.insert(tid.clone());
                                    cx.notify();
                                })),
                        ),
                );
            } else if is_long && is_expanded {
                card = card.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_size(px(12.)).child(task.brief.clone()))
                        .child(
                            div()
                                .id(SharedString::from(format!("task-brief-less-{tid}")))
                                .text_size(px(10.))
                                .text_color(rgb(theme.muted))
                                .cursor_pointer()
                                .role(Role::Button)
                                .child("Less")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.expanded_task_ids.remove(&tid);
                                    cx.notify();
                                })),
                        ),
                );
            } else {
                card = card.child(div().text_size(px(12.)).child(task.brief.clone()));
            }
        }
        if let Some(reason) = task.blocker_reason.as_ref() {
            card = card.child(
                div()
                    .p_2()
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.background))
                    .text_size(px(11.))
                    .text_color(rgb(theme.muted))
                    .child(fl!("team-task-blocked-by", reason = reason.as_ref())),
            );
        }
        if let Some(result) = task.result.as_ref() {
            let is_long = result.len() > 120 || result.lines().count() > 2;
            let tid = task_id.clone();
            if is_long && !is_expanded {
                let preview = truncate_text(result.as_ref(), 120);
                card = card.child(
                    div()
                        .p_2()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.background))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(theme.muted))
                                .child(fl!("team-task-result", result = preview.as_str())),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("task-result-more-{tid}")))
                                .text_size(px(10.))
                                .text_color(rgb(theme.muted))
                                .cursor_pointer()
                                .role(Role::Button)
                                .child("More")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.expanded_task_ids.insert(tid.clone());
                                    cx.notify();
                                })),
                        ),
                );
            } else if is_long && is_expanded {
                card = card.child(
                    div()
                        .p_2()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.background))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(theme.muted))
                                .child(fl!("team-task-result", result = result.as_ref())),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("task-result-less-{tid}")))
                                .text_size(px(10.))
                                .text_color(rgb(theme.muted))
                                .cursor_pointer()
                                .role(Role::Button)
                                .child("Less")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.expanded_task_ids.remove(&tid);
                                    cx.notify();
                                })),
                        ),
                );
            } else {
                card = card.child(
                    div()
                        .p_2()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.background))
                        .text_size(px(11.))
                        .text_color(rgb(theme.muted))
                        .child(fl!("team-task-result", result = result.as_ref())),
                );
            }
        }
        if !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled) {
            let dispatch_action = TeamAction::DispatchTask {
                task_id: task.id.clone(),
            };
            let complete_action = TeamAction::CompleteTask {
                team_id: team_id.to_owned(),
                task_id: task.id.clone(),
                result: String::new(),
            };
            let cancel_action = TeamAction::CancelTask {
                team_id: team_id.to_owned(),
                task_id: task.id.clone(),
            };
            let edit_id = task.id.clone();
            let edit_title = task.title.to_string();
            let edit_brief = task.brief.to_string();
            let edit_assignee = task
                .assignee
                .as_ref()
                .map(|a| a.to_string())
                .unwrap_or_default();

            card = card.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .pt_1()
                    .child(self.action_button(
                        SharedString::from(format!("team-task-dispatch-{}", task.id)),
                        fl!("team-action-dispatch-task"),
                        dispatch_action,
                        cx,
                    ))
                    .child(
                        div()
                            .id(SharedString::from(format!("team-task-edit-{}", task.id)))
                            .px_2()
                            .py_1()
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.active))
                            .text_size(px(12.))
                            .cursor_pointer()
                            .role(Role::Button)
                            .child(fl!("team-action-edit"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.editing_task_id = Some(edit_id.clone());
                                this.task_form_open = true;
                                let t = edit_title.clone();
                                let b = edit_brief.clone();
                                let a = edit_assignee.clone();
                                this.task_title
                                    .update(cx, |i, cx| i.set_text_at(&t, t.len(), cx));
                                this.task_brief
                                    .update(cx, |i, cx| i.set_text_at(&b, b.len(), cx));
                                this.task_assignee
                                    .update(cx, |i, cx| i.set_text_at(&a, a.len(), cx));
                                cx.notify();
                            })),
                    )
                    .child(self.action_button(
                        SharedString::from(format!("team-task-complete-{}", task.id)),
                        fl!("team-action-complete-task"),
                        complete_action,
                        cx,
                    ))
                    .child(self.action_button(
                        SharedString::from(format!("team-task-cancel-{}", task.id)),
                        fl!("team-action-cancel-task"),
                        cancel_action,
                        cx,
                    )),
            );
        }
        card
    }
}
