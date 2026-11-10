use super::{ButtonVariant, MAX_VISIBLE_CANDIDATES, MAX_VISIBLE_MEMBERS, TeamAction, TeamView};
use crate::{i18n::fl, team::model::DeliveryState};
use gpui::{prelude::*, *};
use sheprd_orchestrator::team_store::TaskStatus;

impl TeamView {
    pub(super) fn render_status_banner(&self) -> Div {
        let theme = &self.theme;
        // The localized labels are bound once: comparing each member's status
        // against a fresh `fl!` lookup rebuilt them for every member, every
        // frame. One pass also replaces the three separate filter scans.
        let working_label = fl!("team-status-working");
        let idle_label = fl!("team-status-idle");
        let live_label = fl!("team-status-live");
        let done_label = fl!("team-status-done");
        let mut working = 0usize;
        let mut idle = 0usize;
        for member in &self.snapshot.members {
            let status = member.status.as_ref();
            if status == working_label.as_str() {
                working += 1;
            } else if status == idle_label.as_str()
                || status == live_label.as_str()
                || status == done_label.as_str()
            {
                idle += 1;
            }
        }
        let active_tasks = self
            .snapshot
            .tasks
            .iter()
            .filter(|t| !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled))
            .count();
        let pending_deliveries: usize = self
            .snapshot
            .posts
            .iter()
            .flat_map(|p| p.deliveries.iter())
            .filter(|(_, _, s)| *s == DeliveryState::Pending)
            .count();

        let activity_text = self
            .snapshot
            .last_activity
            .as_ref()
            .map(|act| fl!("team-last-activity", activity = act.as_ref()))
            .unwrap_or_else(|| fl!("team-activity-ready"));

        let summary_base = fl!(
            "team-status-summary",
            working = working,
            idle = idle,
            tasks = active_tasks,
            unread = pending_deliveries
        );
        let summary = if self.snapshot.paused || self.snapshot.queued_count > 0 {
            format!(
                "{} · {} ({} queued)",
                summary_base,
                fl!("team-status-paused"),
                self.snapshot.queued_count
            )
        } else {
            summary_base
        };

        self.card()
            .gap_1()
            .p_2()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(summary),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(theme.muted))
                    .child(activity_text),
            )
    }

    pub(super) fn render_roster(&self, team_id: &str, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let mut section = div().flex().flex_col().gap_2();
        section = section.child(
            self.section_header(format!(
                "{} ({})",
                fl!("team-roster-title"),
                self.snapshot.members.len()
            ))
            .child(
                self.icon_button(
                    "team-worker-toggle-form",
                    if self.worker_form_open {
                        "icons/close.svg"
                    } else {
                        "icons/plus.svg"
                    },
                    if self.worker_form_open {
                        fl!("team-action-cancel")
                    } else {
                        fl!("team-action-add-worker")
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.worker_form_open = !this.worker_form_open;
                    cx.notify();
                })),
            ),
        );

        if self.worker_form_open {
            section = section.child(self.render_add_worker(cx));
        }

        if self.snapshot.members.is_empty() {
            return section.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(theme.muted))
                    .child(fl!("team-roster-empty")),
            );
        }

        for (index, member) in self
            .snapshot
            .members
            .iter()
            .take(MAX_VISIBLE_MEMBERS)
            .enumerate()
        {
            let is_renaming = self.editing_member_id.as_deref() == Some(member.member_id.as_str());
            let mut card = self
                .card()
                .id(SharedString::from(format!("team-member-{index}")))
                .debug_selector(move || format!("team-member-{index}"))
                .gap_1()
                .px_3()
                .py_2();

            let info = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(13.))
                        .truncate()
                        .child(member.name.clone()),
                )
                .when(!member.agent.is_empty(), |r| {
                    r.child(self.chip(member.agent.clone()))
                })
                .child(self.chip(member.role.clone()))
                .child(self.chip(if member.paused {
                    fl!("team-status-paused").into()
                } else {
                    member.status.clone()
                }));

            let mut actions = div().flex().items_center().justify_end().gap_1();
            if let Some(pane) = member.pane.as_ref().filter(|pane| pane.verified()) {
                actions = actions
                    .child(self.icon_action_button(
                        SharedString::from(format!("team-focus-{index}")),
                        "icons/terminal.svg",
                        fl!("team-action-focus"),
                        TeamAction::FocusWorker { pane: pane.clone() },
                        cx,
                    ))
                    .child(self.icon_action_button(
                        SharedString::from(format!("team-nudge-{index}")),
                        "icons/pulse.svg",
                        fl!("team-action-nudge"),
                        TeamAction::NudgeMember {
                            member_id: member.member_id.clone(),
                        },
                        cx,
                    ))
                    .child(self.icon_action_button(
                        SharedString::from(format!("team-pause-member-{index}")),
                        if member.paused {
                            "icons/play.svg"
                        } else {
                            "icons/coffee.svg"
                        },
                        if member.paused {
                            fl!("team-action-resume")
                        } else {
                            fl!("team-action-pause")
                        },
                        TeamAction::TogglePauseMember {
                            member_id: member.member_id.clone(),
                        },
                        cx,
                    ));
            } else if member.pane.is_some() {
                actions = actions.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(theme.muted))
                        .child(fl!("team-roster-session-unavailable")),
                );
            }

            let edit_member_id = member.member_id.clone();
            let edit_member_name = member.name.to_string();
            actions = actions.child(
                self.icon_button(
                    SharedString::from(format!("team-rename-member-{index}")),
                    "icons/pencil.svg",
                    fl!("team-action-rename"),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.editing_team_name = false;
                    this.editing_member_id = Some(edit_member_id.clone());
                    let current = edit_member_name.clone();
                    this.rename_input
                        .update(cx, |i, cx| i.set_text_at(&current, current.len(), cx));
                    cx.notify();
                })),
            );

            if !member.supervisor {
                actions = actions
                    .child(self.icon_action_button(
                        SharedString::from(format!("team-supervisor-{index}")),
                        "icons/user.svg",
                        fl!("team-action-select-supervisor"),
                        TeamAction::SelectSupervisor {
                            team_id: team_id.to_owned(),
                            member_id: member.member_id.clone(),
                        },
                        cx,
                    ))
                    .child(self.icon_action_button(
                        SharedString::from(format!("team-remove-{index}")),
                        "icons/trash.svg",
                        fl!("team-action-remove-member"),
                        TeamAction::RemoveMember {
                            team_id: team_id.to_owned(),
                            member_id: member.member_id.clone(),
                        },
                        cx,
                    ));
            }

            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(info)
                    .child(actions),
            );

            if is_renaming {
                let team_id_rename = team_id.to_owned();
                let member_id_rename = member.member_id.clone();
                card = card.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().child(self.rename_input.clone()))
                        .child(
                            self.button(
                                SharedString::from(format!("team-member-save-{index}")),
                                fl!("team-action-save"),
                                ButtonVariant::Primary,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    let name = this.rename_input.read(cx).text().trim().to_owned();
                                    if name.is_empty() {
                                        return;
                                    }
                                    this.editing_member_id = None;
                                    this.rename_input.update(cx, |i, cx| i.clear(cx));
                                    cx.emit(TeamAction::RenameMember {
                                        team_id: team_id_rename.clone(),
                                        member_id: member_id_rename.clone(),
                                        name,
                                    });
                                },
                            )),
                        )
                        .child(
                            self.button(
                                SharedString::from(format!("team-member-cancel-{index}")),
                                fl!("team-action-cancel"),
                                ButtonVariant::Ghost,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.editing_member_id = None;
                                cx.notify();
                            })),
                        ),
                );
            }
            section = section.child(card);
        }
        section
    }

    fn render_add_worker(&self, cx: &mut Context<Self>) -> Div {
        let name = self.worker_name.read(cx).text().to_owned();
        let mut body = self.card().gap_2().p_2();
        body = body.child(self.worker_name.clone());
        body = body.child(self.render_spawn_bar("team-worker", cx));
        let eligible = self.snapshot.pane_candidates.iter().filter(|c| {
            !self.snapshot.members.iter().any(|m| {
                m.target_pane.as_ref().is_some_and(|target| {
                    target.pane_id == c.identity.pane_id
                        || (target.session_id.is_some()
                            && target.session_id == c.identity.session_id)
                }) || m.pane.as_ref().is_some_and(|p| {
                    p.pane_id == c.identity.pane_id
                        || (p.session_id.is_some() && p.session_id == c.identity.session_id)
                })
            })
        });
        for candidate in eligible.take(MAX_VISIBLE_CANDIDATES) {
            if !name.trim().is_empty()
                && candidate.identity.verified()
                && let Some(team_id) = self.snapshot.team_id.as_ref()
            {
                body = body.child(self.candidate_button(
                    candidate,
                    fl!("team-action-add"),
                    TeamAction::AddWorker {
                        team_id: team_id.clone(),
                        name: name.clone(),
                        pane: candidate.identity.clone(),
                    },
                    cx,
                ));
            }
        }
        body
    }
}
