use super::model::{PaneCandidate, PaneIdentity, TeamLoadState, TeamSnapshot};
use crate::{HerdrWindow, config::Theme, i18n::fl, search_input::SearchInput};
use gpui::{prelude::*, *};
use herdr_orchestrator::team_store::{Author, Confidence};

mod board;

const MAX_VISIBLE_MEMBERS: usize = 64;
const MAX_VISIBLE_CANDIDATES: usize = 64;
const MAX_VISIBLE_TEAMS: usize = 64;
const MAX_VISIBLE_POSTS: usize = 200;

/// Intent emitted by the view. The subscriber must validate current identities
/// and perform writes, focus changes, and refreshes outside rendering.
#[derive(Clone, Debug)]
pub(crate) enum TeamAction {
    SwitchTeamView,
    SelectTeam {
        team_id: String,
    },
    LoadMoreTeams {
        after: String,
    },
    CreateTeam {
        name: String,
        supervisor: PaneIdentity,
    },
    SetGoal {
        team_id: String,
        goal: String,
    },
    SelectSupervisor {
        team_id: String,
        member_id: String,
    },
    AddWorker {
        team_id: String,
        name: String,
        pane: PaneIdentity,
    },
    Post {
        team_id: String,
        author: Author,
        body: String,
    },
    Reply {
        team_id: String,
        author: Author,
        reply_to: String,
        body: String,
    },
    CreateTask {
        team_id: String,
        title: String,
        brief: String,
        assignee: Option<String>,
    },
    CompleteTask {
        team_id: String,
        task_id: String,
        result: String,
    },
    CancelTask {
        team_id: String,
        task_id: String,
    },
    AddFinding {
        team_id: String,
        author: Author,
        description: String,
        evidence: String,
        confidence: Confidence,
    },
    FocusWorker {
        pane: PaneIdentity,
    },
}

pub(crate) struct TeamView {
    snapshot: TeamSnapshot,
    team_name: Entity<SearchInput>,
    team_goal: Entity<SearchInput>,
    worker_name: Entity<SearchInput>,
    post_body: Entity<SearchInput>,
    task_title: Entity<SearchInput>,
    task_brief: Entity<SearchInput>,
    task_assignee: Entity<SearchInput>,
    finding_description: Entity<SearchInput>,
    finding_evidence: Entity<SearchInput>,
    finding_confidence: Confidence,
    reply_to: Option<String>,
    action_warning: Option<SharedString>,
    font: crate::config::FontConfig,
    theme: Theme,
}

impl TeamView {
    pub(crate) fn new(
        font: crate::config::FontConfig,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let team_name = cx.new(SearchInput::new);
        let team_goal = cx.new(SearchInput::new);
        let worker_name = cx.new(SearchInput::new);
        let post_body = cx.new(SearchInput::new);
        let task_title = cx.new(SearchInput::new);
        let task_brief = cx.new(SearchInput::new);
        let task_assignee = cx.new(SearchInput::new);
        let finding_description = cx.new(SearchInput::new);
        let finding_evidence = cx.new(SearchInput::new);
        team_name.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-name-placeholder"), cx);
            input.set_label(&fl!("team-field-name-placeholder"), cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        team_goal.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-goal-placeholder"), cx);
            input.set_label(&fl!("team-field-goal-placeholder"), cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        worker_name.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-worker-placeholder"), cx);
            input.set_label(&fl!("team-field-worker-placeholder"), cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        post_body.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-post-placeholder"), cx);
            input.set_label(&fl!("team-field-post-placeholder"), cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        task_title.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-task-title-placeholder"), cx);
            input.set_label(&fl!("team-field-task-title-placeholder"), cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        task_brief.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-task-brief-placeholder"), cx);
            input.set_label(&fl!("team-field-task-brief-placeholder"), cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        task_assignee.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-task-assignee-placeholder"), cx);
            input.set_label(&fl!("team-field-task-assignee-placeholder"), cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        finding_description.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-finding-description-placeholder"), cx);
            input.set_label(&fl!("team-field-finding-description-placeholder"), cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        finding_evidence.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-finding-evidence-placeholder"), cx);
            input.set_label(&fl!("team-field-finding-evidence-placeholder"), cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        for input in [
            &team_name,
            &team_goal,
            &worker_name,
            &post_body,
            &task_title,
            &task_brief,
            &task_assignee,
            &finding_description,
            &finding_evidence,
        ] {
            cx.subscribe(input, |_, _, _: &crate::search_input::Changed, cx| {
                cx.notify();
            })
            .detach();
        }
        Self {
            snapshot: TeamSnapshot::default(),
            team_name,
            team_goal,
            worker_name,
            post_body,
            task_title,
            task_brief,
            task_assignee,
            finding_description,
            finding_evidence,
            finding_confidence: Confidence::High,
            reply_to: None,
            action_warning: None,
            font,
            theme,
        }
    }

    pub(crate) fn set_snapshot(&mut self, snapshot: TeamSnapshot, cx: &mut Context<Self>) {
        if self.snapshot.team_id != snapshot.team_id {
            self.reply_to = None;
        }
        self.snapshot = snapshot;
        if self.snapshot.team_name.is_none() {
            self.reply_to = None;
        }
        cx.notify();
    }

    pub(crate) fn set_action_warning(
        &mut self,
        warning: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        if self.action_warning != warning {
            self.action_warning = warning;
            cx.notify();
        }
    }

    pub(crate) fn set_appearance(
        &mut self,
        font: crate::config::FontConfig,
        theme: Theme,
        cx: &mut Context<Self>,
    ) {
        let changed = self.font != font || self.theme != theme;
        self.theme = theme.clone();
        self.font = font.clone();
        for input in [
            &self.team_name,
            &self.team_goal,
            &self.worker_name,
            &self.post_body,
            &self.task_title,
            &self.task_brief,
            &self.task_assignee,
            &self.finding_description,
            &self.finding_evidence,
        ] {
            input.update(cx, |input, cx| {
                input.set_appearance(font.clone(), theme.clone(), cx);
            });
        }
        if changed {
            cx.notify();
        }
    }

    fn candidate_button(
        &self,
        candidate: &PaneCandidate,
        label: impl Into<SharedString>,
        action: TeamAction,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let name = label.into();
        div()
            .id(SharedString::from(format!(
                "team-pane-{}",
                candidate.identity.pane_id
            )))
            .flex()
            .items_center()
            .gap_2()
            .py_2()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(div().flex_1().child(candidate.label.clone()))
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(candidate.status.clone()),
            )
            .child(self.action_button(
                SharedString::from(format!("team-action-{}", candidate.identity.pane_id)),
                name,
                action,
                cx,
            ))
    }

    fn action_button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        action: TeamAction,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let label = label.into();
        div()
            .id(id.into())
            .px_3()
            .py_1()
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(theme.active))
            .cursor_pointer()
            .role(Role::Button)
            .child(label)
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(action.clone())))
    }

    fn render_create_team(&self, cx: &mut Context<Self>) -> Div {
        let name = self.team_name.read(cx).text().to_owned();
        let candidates = self
            .snapshot
            .pane_candidates
            .iter()
            .take(MAX_VISIBLE_CANDIDATES);
        let mut body = div().flex().flex_col().gap_2();
        body = body.child(div().text_size(px(19.)).child(fl!("team-create-title")));
        body = body.child(self.team_name.clone());
        for (index, candidate) in candidates.enumerate() {
            let action = TeamAction::CreateTeam {
                name: name.clone(),
                supervisor: candidate.identity.clone(),
            };
            if !name.trim().is_empty() && candidate.identity.verified() {
                body = body.child(self.candidate_button(
                    candidate,
                    fl!("team-action-create-supervisor"),
                    action,
                    cx,
                ));
            }
            if !candidate.identity.verified() {
                body = body.child(
                    div()
                        .id(SharedString::from(format!(
                            "team-candidate-unverified-{index}"
                        )))
                        .text_size(px(11.))
                        .text_color(rgb(self.theme.muted))
                        .child(fl!("team-candidate-no-session")),
                );
            }
        }
        if self.snapshot.pane_candidates.is_empty() {
            body = body.child(
                div()
                    .text_color(rgb(self.theme.muted))
                    .child(fl!("team-candidates-empty")),
            );
        }
        body
    }

    fn render_roster(&self, cx: &mut Context<Self>) -> Div {
        let mut rows = div().flex().flex_col().gap_1();
        for (index, member) in self
            .snapshot
            .members
            .iter()
            .take(MAX_VISIBLE_MEMBERS)
            .enumerate()
        {
            let mut row = div()
                .id(SharedString::from(format!("team-member-{index}")))
                .flex()
                .items_center()
                .gap_2()
                .py_2()
                .border_b_1()
                .border_color(rgb(self.theme.active))
                .child(div().flex_1().child(member.name.clone()))
                .when(!member.agent.is_empty(), |r| {
                    r.child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(self.theme.muted))
                            .child(member.agent.clone()),
                    )
                })
                .child(
                    div()
                        .text_color(rgb(self.theme.muted))
                        .child(member.role.clone()),
                )
                .child(
                    div()
                        .text_color(rgb(self.theme.muted))
                        .child(member.status.clone()),
                );
            if let Some(pane) = member.pane.as_ref().filter(|pane| pane.verified()) {
                let focus = TeamAction::FocusWorker { pane: pane.clone() };
                row = row.child(self.action_button(
                    SharedString::from(format!("team-focus-{index}")),
                    fl!("team-action-focus"),
                    focus,
                    cx,
                ));
            } else if member.pane.is_some() {
                row = row.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(self.theme.muted))
                        .child(fl!("team-roster-session-unavailable")),
                );
            }
            if !member.supervisor
                && let Some(team_id) = self.snapshot.team_id.as_ref()
            {
                let select = TeamAction::SelectSupervisor {
                    team_id: team_id.clone(),
                    member_id: member.member_id.clone(),
                };
                row = row.child(self.action_button(
                    SharedString::from(format!("team-supervisor-{index}")),
                    fl!("team-action-select-supervisor"),
                    select,
                    cx,
                ));
            }
            rows = rows.child(row);
        }
        rows
    }

    fn render_add_worker(&self, cx: &mut Context<Self>) -> Div {
        let name = self.worker_name.read(cx).text().to_owned();
        let mut body = div().flex().flex_col().gap_1();
        body = body.child(
            div()
                .text_size(px(16.))
                .child(fl!("team-action-add-worker")),
        );
        body = body.child(self.worker_name.clone());
        for candidate in self
            .snapshot
            .pane_candidates
            .iter()
            .take(MAX_VISIBLE_CANDIDATES)
        {
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

    fn render_posts(&self, cx: &mut Context<Self>) -> Div {
        let mut body = div().flex().flex_col().gap_2();
        if self.snapshot.posts.is_empty() {
            return body.child(
                div()
                    .text_color(rgb(self.theme.muted))
                    .child(fl!("team-posts-empty")),
            );
        }
        for post in self.snapshot.posts.iter().take(MAX_VISIBLE_POSTS) {
            let post_id = post.id.clone();
            let mut deliveries = String::new();
            for (index, (_, recipient, state)) in post.deliveries.iter().enumerate() {
                if index > 0 {
                    deliveries.push_str(" · ");
                }
                let status = match state {
                    super::model::DeliveryState::Pending => fl!("team-delivery-pending"),
                    super::model::DeliveryState::Acknowledged => {
                        fl!("team-delivery-acknowledged")
                    }
                };
                deliveries.push_str(&fl!(
                    "team-delivery-recipient-status",
                    recipient = recipient.as_ref(),
                    status = status
                ));
            }
            if post.deliveries.is_empty() {
                deliveries.push_str(&fl!("team-delivery-unavailable"));
            }
            body = body.child(
                div()
                    .id(SharedString::from(format!("team-post-{}", post.id)))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(self.theme.surface))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(post.author.clone()),
                    )
                    .when_some(post.reply_to.as_ref(), |card, _| {
                        card.child(
                            div().text_color(rgb(self.theme.muted)).child(
                                post.reply_to_author
                                    .clone()
                                    .map(|author| fl!("team-reply-to", author = author.as_ref()))
                                    .unwrap_or_else(|| fl!("team-action-reply")),
                            ),
                        )
                    })
                    .child(div().child(post.body.clone()))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(self.theme.muted))
                            .child(deliveries),
                    )
                    .when(self.snapshot.post_author.is_some(), |post_card| {
                        post_card.child(
                            div()
                                .id(SharedString::from(format!("team-reply-{}", post.id)))
                                .px_3()
                                .py_1()
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(self.theme.active))
                                .cursor_pointer()
                                .role(Role::Button)
                                .child(fl!("team-action-reply"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.reply_to = Some(post_id.clone());
                                    let focus = this.post_body.read(cx).focus.clone();
                                    window.focus(&focus, cx);
                                    cx.notify();
                                })),
                        )
                    }),
            );
        }
        body
    }
}

impl EventEmitter<TeamAction> for TeamView {}

impl Render for TeamView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let mut body = div()
            .id("team-section")
            .debug_selector(|| "team-section".into())
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .gap_4()
            .p_4()
            .overflow_y_scroll()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground));
        if let Some(warning) = self.action_warning.as_ref() {
            body = body.child(div().text_color(rgb(theme.muted)).child(warning.clone()));
        }
        match &self.snapshot.state {
            TeamLoadState::Loading => {
                body = body.child(div().child(fl!("team-loading")));
                return body;
            }
            TeamLoadState::Failed(error) => {
                body = body.child(div().text_color(rgb(theme.muted)).child(error.clone()));
                return body;
            }
            TeamLoadState::Ready => {}
        }
        let Some(team_name) = self.snapshot.team_name.as_ref() else {
            body = body.child(self.render_team_choices(cx));
            return body.child(self.render_create_team(cx));
        };
        let Some(team_id) = self.snapshot.team_id.as_ref() else {
            return body.child(div().child(fl!("team-error-id-missing")));
        };
        let team_id = team_id.clone();
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(div().flex_1().text_size(px(21.)).child(team_name.clone()))
                .child(self.action_button(
                    "team-switch",
                    fl!("team-action-switch"),
                    TeamAction::SwitchTeamView,
                    cx,
                ))
                .child(
                    div()
                        .text_color(rgb(theme.muted))
                        .child(fl!("sidebar-activity-team")),
                ),
        );
        body = body.child(self.render_goal(&team_id, cx));
        body = body.child(if self.snapshot.members.is_empty() {
            div()
                .text_color(rgb(theme.muted))
                .child(fl!("team-roster-empty"))
        } else {
            self.render_roster(cx)
        });
        body = body.child(self.render_add_worker(cx));
        body = body.child(self.render_tasks(&team_id, cx));
        body = body.child(self.render_findings(&team_id, cx));
        body = body.child(div().text_size(px(16.)).child(fl!("team-bulletin")));
        body = body.child(self.render_posts(cx));
        if let Some((author, author_label)) = self.snapshot.post_author.as_ref() {
            let author = author.clone();
            body = body
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(theme.muted))
                        .child(fl!("team-posting-as", author = author_label.as_ref())),
                )
                .child(self.post_body.clone())
                .child(
                    div()
                        .id("team-post-submit")
                        .px_3()
                        .py_1()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.active))
                        .cursor_pointer()
                        .role(Role::Button)
                        .child(if self.reply_to.is_some() {
                            fl!("team-action-send-reply")
                        } else {
                            fl!("team-action-post")
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let body = this.post_body.read(cx).text().trim().to_owned();
                            if body.is_empty() {
                                return;
                            }
                            let action = match this.reply_to.clone() {
                                Some(reply_to) => TeamAction::Reply {
                                    team_id: team_id.clone(),
                                    author: author.clone(),
                                    reply_to,
                                    body,
                                },
                                None => TeamAction::Post {
                                    team_id: team_id.clone(),
                                    author: author.clone(),
                                    body,
                                },
                            };
                            this.reply_to = None;
                            this.post_body.update(cx, |input, cx| input.clear(cx));
                            cx.emit(action);
                        })),
                );
        }
        body
    }
}

impl TeamView {
    fn render_team_choices(&self, cx: &mut Context<Self>) -> Div {
        let mut choices = div().flex().flex_col().gap_1();
        for (index, team) in self
            .snapshot
            .teams
            .iter()
            .take(MAX_VISIBLE_TEAMS)
            .enumerate()
        {
            choices = choices.child(self.action_button(
                SharedString::from(format!("team-select-{index}")),
                team.name.clone(),
                TeamAction::SelectTeam {
                    team_id: team.team_id.clone(),
                },
                cx,
            ));
        }
        if self.snapshot.teams.len() > MAX_VISIBLE_TEAMS && self.snapshot.teams_after.is_none() {
            choices = choices
                .child(div().child(fl!("team-list-limit-reached", count = MAX_VISIBLE_TEAMS)));
        }
        if let Some(after) = self.snapshot.teams_after.as_ref() {
            choices = choices.child(self.action_button(
                "team-load-more",
                fl!("team-action-load-more"),
                TeamAction::LoadMoreTeams {
                    after: after.clone(),
                },
                cx,
            ));
        }
        choices
    }
}

impl HerdrWindow {
    pub(crate) fn set_team_snapshot(&mut self, snapshot: TeamSnapshot, cx: &mut Context<Self>) {
        self.team_view
            .update(cx, |view, cx| view.set_snapshot(snapshot, cx));
    }

    pub(crate) fn take_team_actions(&mut self, cx: &mut Context<Self>) -> Vec<TeamAction> {
        let actions = self.team_actions.drain(..).collect();
        if self.team_action_overflow {
            self.team_action_overflow = false;
            self.team_view
                .update(cx, |view, cx| view.set_action_warning(None, cx));
        }
        actions
    }

    pub(crate) fn render_team_section(&self, cx: &mut Context<Self>) -> AnyElement {
        self.team_view.update(cx, |view, cx| {
            view.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
        AnyView::from(self.team_view.clone()).into_any_element()
    }
}
