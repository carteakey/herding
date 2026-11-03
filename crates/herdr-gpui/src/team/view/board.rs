use super::{TeamAction, TeamView};
use crate::i18n::fl;
use gpui::{prelude::*, *};
use herdr_orchestrator::team_store::{Confidence, TaskStatus};

const MAX_VISIBLE_TASKS: usize = 64;
const MAX_VISIBLE_FINDINGS: usize = 64;

impl TeamView {
    pub(super) fn render_goal(&self, team_id: &str, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let team_id = team_id.to_owned();
        let mut section = div().flex().flex_col().gap_1();
        section = section.child(div().text_size(px(16.)).child(fl!("team-goal-title")));
        section = section.child(match self.snapshot.team_goal.as_ref() {
            Some(goal) => div()
                .p_2()
                .rounded(px(crate::config::corners::CONTROL))
                .bg(rgb(theme.surface))
                .child(goal.clone()),
            None => div()
                .text_color(rgb(theme.muted))
                .child(fl!("team-goal-empty")),
        });
        section = section.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_1().child(self.team_goal.clone()))
                .child(
                    div()
                        .id("team-goal-save")
                        .px_3()
                        .py_1()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.active))
                        .cursor_pointer()
                        .role(Role::Button)
                        .child(fl!("team-action-save-goal"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let goal = this.team_goal.read(cx).text().trim().to_owned();
                            if goal.is_empty() {
                                return;
                            }
                            this.team_goal.update(cx, |input, cx| input.clear(cx));
                            cx.emit(TeamAction::SetGoal {
                                team_id: team_id.clone(),
                                goal,
                            });
                        })),
                ),
        );
        section
    }

    pub(super) fn render_tasks(&self, team_id: &str, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let team_id_for_create = team_id.to_owned();
        let mut section = div().flex().flex_col().gap_2();
        section = section.child(div().text_size(px(16.)).child(fl!("team-tasks-title")));
        if self.snapshot.tasks.is_empty() {
            section = section.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("team-tasks-empty")),
            );
        } else {
            for task in self.snapshot.tasks.iter().take(MAX_VISIBLE_TASKS) {
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
                let mut card = div()
                    .id(SharedString::from(format!("team-task-{}", task.id)))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.surface))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(task.title.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(theme.muted))
                                    .child(assignee_text),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(theme.muted))
                                    .child(status_label),
                            ),
                    );
                if !task.brief.is_empty() {
                    card = card.child(div().text_size(px(13.)).child(task.brief.clone()));
                }
                if let Some(reason) = task.blocker_reason.as_ref() {
                    card = card.child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme.muted))
                            .child(fl!("team-task-blocked-by", reason = reason.as_ref())),
                    );
                }
                if let Some(result) = task.result.as_ref() {
                    card = card.child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme.muted))
                            .child(fl!("team-task-result", result = result.as_ref())),
                    );
                }
                if !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled) {
                    let complete_action = TeamAction::CompleteTask {
                        team_id: team_id.to_owned(),
                        task_id: task.id.clone(),
                        result: String::new(),
                    };
                    let cancel_action = TeamAction::CancelTask {
                        team_id: team_id.to_owned(),
                        task_id: task.id.clone(),
                    };
                    card = card.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
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
                section = section.child(card);
            }
        }
        section = section
            .child(self.task_title.clone())
            .child(self.task_brief.clone())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(self.task_assignee.clone()))
                    .child(
                        div()
                            .id("team-task-create")
                            .px_3()
                            .py_1()
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.active))
                            .cursor_pointer()
                            .role(Role::Button)
                            .child(fl!("team-action-add-task"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let title = this.task_title.read(cx).text().trim().to_owned();
                                if title.is_empty() {
                                    return;
                                }
                                let brief = this.task_brief.read(cx).text().trim().to_owned();
                                let assignee = this.task_assignee.read(cx).text().trim().to_owned();
                                let assignee = (!assignee.is_empty()).then_some(assignee);
                                this.task_title.update(cx, |input, cx| input.clear(cx));
                                this.task_brief.update(cx, |input, cx| input.clear(cx));
                                this.task_assignee.update(cx, |input, cx| input.clear(cx));
                                cx.emit(TeamAction::CreateTask {
                                    team_id: team_id_for_create.clone(),
                                    title,
                                    brief,
                                    assignee,
                                });
                            })),
                    ),
            );
        section
    }

    pub(super) fn render_findings(&self, team_id: &str, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let mut section = div().flex().flex_col().gap_2();
        section = section.child(div().text_size(px(16.)).child(fl!("team-findings-title")));
        if self.snapshot.findings.is_empty() {
            section = section.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("team-findings-empty")),
            );
        } else {
            for finding in self.snapshot.findings.iter().take(MAX_VISIBLE_FINDINGS) {
                let confidence_label = match finding.confidence {
                    Confidence::Low => fl!("team-confidence-low"),
                    Confidence::Medium => fl!("team-confidence-medium"),
                    Confidence::High => fl!("team-confidence-high"),
                };
                let mut card = div()
                    .id(SharedString::from(format!("team-finding-{}", finding.id)))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.surface))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(finding.author.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(theme.muted))
                                    .child(confidence_label),
                            ),
                    )
                    .child(div().child(finding.description.clone()))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme.muted))
                            .child(fl!(
                                "team-finding-evidence",
                                evidence = finding.evidence.as_ref()
                            )),
                    );
                if let Some(superseded) = finding.supersedes.as_ref() {
                    card = card.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme.muted))
                            .child(fl!("team-finding-supersedes", id = superseded.as_str())),
                    );
                }
                section = section.child(card);
            }
        }
        if let Some((author, _)) = self.snapshot.post_author.as_ref() {
            let author = author.clone();
            let team_id = team_id.to_owned();
            let current_confidence = match self.finding_confidence {
                Confidence::Low => fl!("team-confidence-low"),
                Confidence::Medium => fl!("team-confidence-medium"),
                Confidence::High => fl!("team-confidence-high"),
            };
            section = section
                .child(self.finding_description.clone())
                .child(self.finding_evidence.clone())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .id("team-finding-confidence")
                                .px_3()
                                .py_1()
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(theme.active))
                                .cursor_pointer()
                                .role(Role::Button)
                                .child(current_confidence)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.finding_confidence = match this.finding_confidence {
                                        Confidence::High => Confidence::Medium,
                                        Confidence::Medium => Confidence::Low,
                                        Confidence::Low => Confidence::High,
                                    };
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("team-finding-add")
                                .px_3()
                                .py_1()
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(theme.active))
                                .cursor_pointer()
                                .role(Role::Button)
                                .child(fl!("team-action-add-finding"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let description =
                                        this.finding_description.read(cx).text().trim().to_owned();
                                    let evidence =
                                        this.finding_evidence.read(cx).text().trim().to_owned();
                                    if description.is_empty() || evidence.is_empty() {
                                        return;
                                    }
                                    let confidence = this.finding_confidence;
                                    this.finding_description
                                        .update(cx, |input, cx| input.clear(cx));
                                    this.finding_evidence
                                        .update(cx, |input, cx| input.clear(cx));
                                    cx.emit(TeamAction::AddFinding {
                                        team_id: team_id.clone(),
                                        author: author.clone(),
                                        description,
                                        evidence,
                                        confidence,
                                    });
                                })),
                        ),
                );
        }
        section
    }
}
