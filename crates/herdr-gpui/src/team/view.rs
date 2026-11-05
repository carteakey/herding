use super::model::{PaneCandidate, PaneIdentity, TeamLoadState, TeamSnapshot};
use crate::{HerdrWindow, config::Theme, i18n::fl, search_input::SearchInput};
use gpui::{prelude::*, *};
use herdr_orchestrator::team_store::{Author, Confidence};
use std::collections::HashSet;

mod board;
mod bulletin;
mod findings;
mod roster;
mod task_icons;

#[cfg(test)]
mod perf_tests;

#[cfg(test)]
mod tests;

pub(super) const MAX_VISIBLE_MEMBERS: usize = 64;
pub(super) const MAX_VISIBLE_CANDIDATES: usize = 64;
const MAX_VISIBLE_TEAMS: usize = 64;
pub(super) const MAX_VISIBLE_POSTS: usize = 200;
pub(super) const MAX_VISIBLE_TASKS: usize = 64;
pub(super) const MAX_VISIBLE_FINDINGS: usize = 64;
pub(super) const RECENT_ACTIVE_TASKS_LIMIT: usize = 16;
pub(super) const RECENT_COMPLETED_TASKS_LIMIT: usize = 16;
pub(super) const RECENT_FINDINGS_LIMIT: usize = 16;
pub(super) const EXPANDED_POSTS_LIMIT: usize = 30;

/// The look of a [`TeamView::button`], matched to what a click does: a
/// primary action, a quiet cancel, or a disclosure that only re-windows rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ButtonVariant {
    Primary,
    Ghost,
    Toggle,
}

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
    /// Fetch the board's next older task page behind the show-earlier control.
    LoadEarlierTasks,
    /// Fetch the board's next older finding page behind its show-earlier
    /// control.
    LoadEarlierFindings,
    CreateTeam {
        name: String,
        supervisor: PaneIdentity,
    },
    RenameTeam {
        team_id: String,
        name: String,
    },
    DeleteTeam {
        team_id: String,
    },
    SetGoal {
        team_id: String,
        goal: String,
    },
    SelectSupervisor {
        team_id: String,
        member_id: String,
    },
    RenameMember {
        team_id: String,
        member_id: String,
        name: String,
    },
    RemoveMember {
        team_id: String,
        member_id: String,
    },
    NudgeMember {
        member_id: String,
    },
    TogglePauseTeam,
    TogglePauseMember {
        member_id: String,
    },
    SpawnAgentSession {
        agent: herdr_chat_protocol::Agent,
    },
    SplitNewPane,
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
    UpdateTask {
        team_id: String,
        task_id: String,
        title: String,
        brief: String,
        assignee: Option<String>,
    },
    DispatchTask {
        task_id: String,
    },
    ImportGithubIssue {
        team_id: String,
        issue_ref: String,
        as_goal: bool,
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
    ToggleBulletinSidebar,
    FocusWorker {
        pane: PaneIdentity,
    },
}

pub(crate) struct TeamView {
    pub(super) snapshot: TeamSnapshot,
    team_name: Entity<SearchInput>,
    pub(super) rename_input: Entity<SearchInput>,
    pub(super) team_goal: Entity<SearchInput>,
    pub(super) worker_name: Entity<SearchInput>,
    pub(super) post_body: Entity<SearchInput>,
    pub(super) task_title: Entity<SearchInput>,
    pub(super) task_brief: Entity<SearchInput>,
    pub(super) task_assignee: Entity<SearchInput>,
    pub(super) issue_input: Entity<SearchInput>,
    pub(super) finding_description: Entity<SearchInput>,
    pub(super) finding_evidence: Entity<SearchInput>,
    pub(super) finding_confidence: Confidence,
    pub(super) reply_to: Option<String>,
    pub(super) editing_team_name: bool,
    pub(super) editing_member_id: Option<String>,
    pub(super) editing_goal: bool,
    pub(super) worker_form_open: bool,
    pub(super) task_form_open: bool,
    pub(super) issue_form_open: bool,
    pub(super) editing_task_id: Option<String>,
    pub(super) finding_form_open: bool,
    pub(super) confirming_delete_team: bool,
    pub(super) completed_tasks_open: bool,
    pub(super) expanded_task_ids: HashSet<String>,
    pub(super) findings_collapsed: bool,
    pub(super) expanded_finding_ids: HashSet<String>,
    pub(super) expanded_bulletin_ids: HashSet<String>,
    pub(super) bulletin_following: bool,
    pub(super) bulletin_scroll: ScrollHandle,
    pub(super) bulletin_show_all: bool,
    pub(super) bulletin_expanded_limit: usize,
    pub(super) active_tasks_show_all: bool,
    pub(super) completed_tasks_show_all: bool,
    pub(super) findings_show_all: bool,
    /// Older board rows revealed beyond the default window by the
    /// show-earlier controls; grows by a page per use.
    pub(super) earlier_tasks_rows: usize,
    pub(super) earlier_findings_rows: usize,
    action_warning: Option<SharedString>,
    font: crate::config::FontConfig,
    pub(super) theme: Theme,
}

impl TeamView {
    pub(crate) fn new(
        font: crate::config::FontConfig,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let team_name = cx.new(SearchInput::new);
        let rename_input = cx.new(SearchInput::new);
        let team_goal = cx.new(SearchInput::new);
        let worker_name = cx.new(SearchInput::new);
        let post_body = cx.new(SearchInput::new);
        let task_title = cx.new(SearchInput::new);
        let task_brief = cx.new(SearchInput::new);
        let task_assignee = cx.new(SearchInput::new);
        let issue_input = cx.new(SearchInput::new);
        let finding_description = cx.new(SearchInput::new);
        let finding_evidence = cx.new(SearchInput::new);
        team_name.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-name-placeholder"), cx);
            input.set_label(&fl!("team-field-name-placeholder"), cx);
            input.set_appearance(font.clone(), theme.clone(), cx);
        });
        rename_input.update(cx, |input, cx| {
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
        issue_input.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("team-field-issue-placeholder"), cx);
            input.set_label(&fl!("team-field-issue-placeholder"), cx);
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
            &rename_input,
            &team_goal,
            &worker_name,
            &post_body,
            &task_title,
            &task_brief,
            &task_assignee,
            &issue_input,
            &finding_description,
            &finding_evidence,
        ] {
            cx.observe(input, |_, _, cx| cx.notify()).detach();
        }
        Self {
            snapshot: TeamSnapshot::default(),
            team_name,
            rename_input,
            team_goal,
            worker_name,
            post_body,
            task_title,
            task_brief,
            task_assignee,
            issue_input,
            finding_description,
            finding_evidence,
            finding_confidence: Confidence::High,
            reply_to: None,
            editing_team_name: false,
            editing_member_id: None,
            editing_goal: false,
            worker_form_open: false,
            task_form_open: false,
            issue_form_open: false,
            editing_task_id: None,
            finding_form_open: false,
            confirming_delete_team: false,
            completed_tasks_open: false,
            expanded_task_ids: HashSet::new(),
            findings_collapsed: false,
            expanded_finding_ids: HashSet::new(),
            expanded_bulletin_ids: HashSet::new(),
            bulletin_following: true,
            bulletin_scroll: ScrollHandle::new(),
            bulletin_show_all: false,
            bulletin_expanded_limit: EXPANDED_POSTS_LIMIT,
            active_tasks_show_all: false,
            completed_tasks_show_all: false,
            findings_show_all: false,
            earlier_tasks_rows: 0,
            earlier_findings_rows: 0,
            action_warning: None,
            font,
            theme,
        }
    }

    pub(crate) fn set_appearance(
        &mut self,
        font: crate::config::FontConfig,
        theme: Theme,
        cx: &mut Context<Self>,
    ) {
        if self.font == font && self.theme == theme {
            return;
        }
        self.font = font.clone();
        self.theme = theme.clone();
        for input in [
            &self.team_name,
            &self.rename_input,
            &self.team_goal,
            &self.worker_name,
            &self.post_body,
            &self.task_title,
            &self.task_brief,
            &self.task_assignee,
            &self.issue_input,
            &self.finding_description,
            &self.finding_evidence,
        ] {
            input.update(cx, |input, cx| {
                input.set_appearance(font.clone(), theme.clone(), cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn set_snapshot(&mut self, snapshot: TeamSnapshot, cx: &mut Context<Self>) {
        if self.snapshot.team_id != snapshot.team_id {
            self.reply_to = None;
            self.editing_team_name = false;
            self.editing_member_id = None;
            self.editing_goal = false;
            self.editing_task_id = None;
            self.confirming_delete_team = false;
            self.action_warning = None;
            self.active_tasks_show_all = false;
            self.completed_tasks_show_all = false;
            self.findings_show_all = false;
            self.earlier_tasks_rows = 0;
            self.earlier_findings_rows = 0;
            self.bulletin_expanded_limit = EXPANDED_POSTS_LIMIT;
        } else if let Some(reply_to) = self.reply_to.as_ref()
            && !snapshot.posts.iter().any(|post| post.id == *reply_to)
        {
            self.reply_to = None;
        }
        self.snapshot = snapshot;
        cx.notify();
    }

    pub(crate) fn set_action_warning(
        &mut self,
        warning: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.action_warning = warning;
        cx.notify();
    }

    pub(super) fn candidate_button(
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
            .flex_wrap()
            .items_center()
            .gap_2()
            .p_2()
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .child(candidate.label.clone()),
            )
            .child(
                div()
                    .text_size(px(11.))
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

    /// The standard surface card: padded by the caller, cards round at
    /// [`crate::config::corners::PANEL`] like the orchestrator views, while
    /// buttons and chips keep [`crate::config::corners::CONTROL`] and
    /// [`crate::config::corners::SMALL`].
    pub(super) fn card(&self) -> Div {
        let theme = &self.theme;
        div()
            .flex()
            .flex_col()
            .rounded(px(crate::config::corners::PANEL))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active))
    }

    /// A muted inline tag for status, role, and delivery metadata.
    pub(super) fn chip(&self, label: impl Into<SharedString>) -> Div {
        let theme = &self.theme;
        div()
            .px_2()
            .py(px(1.))
            .rounded(px(crate::config::corners::SMALL))
            .bg(rgb(theme.background))
            .text_size(px(11.))
            .text_color(rgb(theme.muted))
            .child(label.into())
    }

    /// A clickable chip, for the bulletin's mention picker.
    pub(super) fn chip_button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let id = id.into();
        let selector = id.to_string();
        div()
            .id(id)
            .debug_selector(move || selector.clone())
            .px_2()
            .py(px(1.))
            .rounded(px(crate::config::corners::SMALL))
            .bg(rgb(theme.surface))
            .text_size(px(11.))
            .text_color(rgb(theme.muted))
            .cursor_pointer()
            .role(Role::Button)
            .child(label.into())
    }

    /// A quiet text toggle, such as a card's "More" / "Show more" link.
    pub(super) fn text_button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let id = id.into();
        let selector = id.to_string();
        div()
            .id(id)
            .debug_selector(move || selector.clone())
            .text_size(px(10.))
            .text_color(rgb(theme.muted))
            .cursor_pointer()
            .role(Role::Button)
            .child(label.into())
    }

    /// An inset box for quoted content inside a card: task results, blocked
    /// reasons, finding evidence.
    pub(super) fn well(&self) -> Div {
        let theme = &self.theme;
        div()
            .p_2()
            .rounded(px(crate::config::corners::SMALL))
            .bg(rgb(theme.background))
            .text_size(px(11.))
            .text_color(rgb(theme.muted))
    }

    /// A section header row: a semibold title on the left, room for the
    /// caller's action buttons on the right.
    pub(super) fn section_header(&self, title: impl Into<SharedString>) -> Div {
        div().flex().items_center().justify_between().gap_2().child(
            div()
                .flex_1()
                .min_w_0()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(14.))
                .truncate()
                .child(title.into()),
        )
    }

    pub(super) fn action_button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        action: TeamAction,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.button(id, label, ButtonVariant::Primary)
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(action.clone())))
    }

    /// The text-button styles used across the team views. Every variant
    /// carries its id as a debug selector so headless tests can reach it.
    pub(super) fn button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        variant: ButtonVariant,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let id = id.into();
        let selector = id.to_string();
        let base = div()
            .id(id)
            .debug_selector(move || selector.clone())
            .px_2()
            .py_1()
            .rounded(px(crate::config::corners::CONTROL))
            .text_size(px(12.))
            .cursor_pointer()
            .role(Role::Button);
        let styled = match variant {
            ButtonVariant::Primary => base.bg(rgb(theme.active)).text_color(rgb(theme.foreground)),
            ButtonVariant::Ghost => base.bg(rgb(theme.background)).text_color(rgb(theme.muted)),
            ButtonVariant::Toggle => base
                .bg(rgb(theme.surface))
                .border_1()
                .border_color(rgb(theme.active))
                .text_color(rgb(theme.foreground))
                .font_weight(FontWeight::SEMIBOLD),
        };
        styled.child(label.into())
    }

    pub(super) fn icon_button(
        &self,
        id: impl Into<SharedString>,
        icon: &'static str,
        tooltip: impl Into<SharedString>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let id = id.into();
        let tooltip = tooltip.into();
        let selector = id.to_string();
        div()
            .id(id)
            .debug_selector(move || selector.clone())
            .size(px(22.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::CONTROL))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(theme.active)))
            .role(Role::Button)
            .aria_label(tooltip)
            .child(
                svg()
                    .path(icon)
                    .size(px(13.))
                    .text_color(rgb(theme.foreground)),
            )
    }

    pub(super) fn icon_action_button(
        &self,
        id: impl Into<SharedString>,
        icon: &'static str,
        tooltip: impl Into<SharedString>,
        action: TeamAction,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.icon_button(id, icon, tooltip)
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(action.clone())))
    }

    pub(super) fn render_spawn_bar(&self, prefix: &str, cx: &mut Context<Self>) -> Div {
        let mut row = div().flex().flex_wrap().items_center().gap_1();
        for (id_suffix, agent) in [
            ("claude", herdr_chat_protocol::Agent::Claude),
            ("codex", herdr_chat_protocol::Agent::Codex),
            ("gemini", herdr_chat_protocol::Agent::Gemini),
            ("opencode", herdr_chat_protocol::Agent::Opencode),
        ] {
            row = row.child(self.action_button(
                SharedString::from(format!("{prefix}-spawn-{id_suffix}")),
                format!("+ {}", agent.display_name()),
                TeamAction::SpawnAgentSession { agent },
                cx,
            ));
        }
        row = row.child(self.action_button(
            SharedString::from(format!("{prefix}-split-pane")),
            fl!("team-action-split-pane"),
            TeamAction::SplitNewPane,
            cx,
        ));
        row
    }

    fn render_create_team(&self, cx: &mut Context<Self>) -> Div {
        let name = self.team_name.read(cx).text().to_owned();
        let candidates = self
            .snapshot
            .pane_candidates
            .iter()
            .take(MAX_VISIBLE_CANDIDATES);
        let mut body = div().flex().flex_col().gap_2();
        body = body.child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(15.))
                .child(fl!("team-create-title")),
        );
        body = body.child(self.team_name.clone());
        body = body.child(self.render_spawn_bar("team-create", cx));
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
                    .text_size(px(12.))
                    .text_color(rgb(self.theme.muted))
                    .child(fl!("team-candidates-empty")),
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
            .gap_3()
            .p_3()
            .overflow_y_scroll()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground));
        if let Some(warning) = self.action_warning.as_ref() {
            body = body.child(
                self.card()
                    .p_2()
                    .text_size(px(12.))
                    .text_color(rgb(theme.muted))
                    .child(warning.clone()),
            );
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
        let current_team_name = team_name.to_string();

        let mut header_card = self.card().gap_2().px_3().py_2().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(16.))
                        .truncate()
                        .child(team_name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_1()
                        .child(
                            self.icon_button(
                                "team-rename-toggle",
                                "icons/pencil.svg",
                                fl!("team-action-rename"),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.editing_member_id = None;
                                    this.editing_team_name = !this.editing_team_name;
                                    if this.editing_team_name {
                                        let current = current_team_name.clone();
                                        this.rename_input.update(cx, |i, cx| {
                                            i.set_text_at(&current, current.len(), cx)
                                        });
                                    }
                                    cx.notify();
                                },
                            )),
                        )
                        .child(self.icon_action_button(
                            "team-toggle-pause",
                            if self.snapshot.paused {
                                "icons/play.svg"
                            } else {
                                "icons/coffee.svg"
                            },
                            if self.snapshot.paused {
                                fl!("team-action-resume")
                            } else {
                                fl!("team-action-pause")
                            },
                            TeamAction::TogglePauseTeam,
                            cx,
                        ))
                        .child(self.icon_action_button(
                            "team-toggle-bulletin",
                            "icons/panel-right.svg",
                            if self.snapshot.bulletin_open {
                                fl!("team-action-hide-bulletin")
                            } else {
                                fl!("team-action-toggle-bulletin")
                            },
                            TeamAction::ToggleBulletinSidebar,
                            cx,
                        ))
                        .child(self.icon_action_button(
                            "team-switch",
                            "icons/sessions.svg",
                            fl!("team-action-switch"),
                            TeamAction::SwitchTeamView,
                            cx,
                        ))
                        .child(
                            self.icon_button(
                                "team-delete",
                                "icons/trash.svg",
                                fl!("team-action-delete-team"),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.confirming_delete_team = !this.confirming_delete_team;
                                    cx.notify();
                                },
                            )),
                        ),
                ),
        );

        if self.editing_team_name {
            let team_id_save = team_id.clone();
            header_card = header_card.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(self.rename_input.clone()))
                    .child(
                        self.button(
                            "team-rename-save",
                            fl!("team-action-save"),
                            ButtonVariant::Primary,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let name = this.rename_input.read(cx).text().trim().to_owned();
                            if name.is_empty() {
                                return;
                            }
                            this.editing_team_name = false;
                            this.rename_input.update(cx, |i, cx| i.clear(cx));
                            cx.emit(TeamAction::RenameTeam {
                                team_id: team_id_save.clone(),
                                name,
                            });
                        })),
                    )
                    .child(
                        self.button(
                            "team-rename-cancel",
                            fl!("team-action-cancel"),
                            ButtonVariant::Ghost,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.editing_team_name = false;
                            cx.notify();
                        })),
                    ),
            );
        }

        if self.confirming_delete_team {
            let team_id_del = team_id.clone();
            let team_name_display = self.snapshot.team_name.clone().unwrap_or_default();
            header_card = header_card.child(
                self.card()
                    .id("team-delete-confirm-banner")
                    .p_3()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme.foreground))
                            .child(fl!(
                                "team-action-delete-confirm",
                                team = team_name_display.as_ref()
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                self.button(
                                    "team-delete-confirm-btn",
                                    fl!("team-action-delete-permanently"),
                                    ButtonVariant::Primary,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.confirming_delete_team = false;
                                        cx.emit(TeamAction::DeleteTeam {
                                            team_id: team_id_del.clone(),
                                        });
                                    },
                                )),
                            )
                            .child(
                                self.button(
                                    "team-delete-cancel-btn",
                                    fl!("team-action-cancel"),
                                    ButtonVariant::Ghost,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.confirming_delete_team = false;
                                        cx.notify();
                                    },
                                )),
                            ),
                    ),
            );
        }

        body = body.child(header_card);
        body = body.child(self.render_status_banner());
        body = body.child(self.render_goal(&team_id, cx));
        body = body.child(self.render_roster(&team_id, cx));
        body = body.child(self.render_tasks(&team_id, cx));
        body = body.child(self.render_findings(&team_id, cx));
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
