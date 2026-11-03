use super::{MAX_VISIBLE_POSTS, TeamAction};
use crate::{HerdrWindow, direction::Logical, i18n::fl, team::model::DeliveryState};
use gpui::{prelude::*, *};

const BULLETIN_WIDTH: f32 = 340.;
const MIN_TERMINAL_WIDTH: f32 = 260.;

impl HerdrWindow {
    pub(crate) fn push_team_action(&mut self, action: TeamAction, cx: &mut Context<Self>) {
        if self.team_actions.len() < crate::team::ACTION_QUEUE_CAPACITY {
            self.team_actions.push_back(action);
            self.poll_team(cx);
        } else {
            self.team_action_overflow = true;
            self.team_view.update(cx, |view, cx| {
                view.set_action_warning(Some(fl!("team-action-queue-full").into()), cx);
            });
        }
        cx.notify();
    }

    /// Renders the right-hand Team Bulletin sidebar beside `content` when the
    /// Team sidebar is active and a team is selected.
    pub(crate) fn with_team_bulletin_panel(
        &self,
        content: AnyElement,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.team_panel_shown()
            || !self.team_bridge.has_selected_team()
            || !self.team_bridge.bulletin_open()
            || self.zen_layout()
        {
            return content;
        }
        let window_width = f32::from(window.viewport_size().width);
        let left_width = self.sidebar_column_width(window_width).unwrap_or(0.);
        if window_width - left_width - BULLETIN_WIDTH < MIN_TERMINAL_WIDTH {
            return content;
        }
        let panel = self.render_team_bulletin_sidebar(cx);
        div()
            .flex()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(div().flex().flex_1().min_w_0().min_h_0().child(content))
            .child(panel)
            .into_any_element()
    }

    fn render_team_bulletin_sidebar(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.theme;
        let view = self.team_view.read(cx);
        let snapshot = &view.snapshot;
        let team_id = snapshot.team_id.clone().unwrap_or_default();
        let post_body = view.post_body.clone();
        let reply_to = view.reply_to.clone();
        let post_author = snapshot.post_author.clone();
        let posts = snapshot.posts.clone();
        let members = snapshot.members.clone();

        let pending_count: usize = posts
            .iter()
            .flat_map(|p| p.deliveries.iter())
            .filter(|(_, _, state)| *state == DeliveryState::Pending)
            .count();

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(13.))
                            .child(fl!("team-bulletin").to_uppercase()),
                    )
                    .child(
                        div()
                            .px_2()
                            .py(px(1.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.surface))
                            .text_size(px(11.))
                            .text_color(rgb(theme.muted))
                            .child(format!("{} · {} pending", posts.len(), pending_count)),
                    ),
            )
            .child(
                div()
                    .id("team-bulletin-hide")
                    .px_2()
                    .py(px(2.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.active))
                    .text_size(px(11.))
                    .cursor_pointer()
                    .role(Role::Button)
                    .child(fl!("team-action-hide"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.push_team_action(TeamAction::ToggleBulletinSidebar, cx);
                    })),
            );

        let mut feed = div()
            .id("team-bulletin-feed")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2();

        if posts.is_empty() {
            feed = feed.child(
                div()
                    .p_3()
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.surface))
                    .text_size(px(12.))
                    .text_color(rgb(theme.muted))
                    .child(fl!("team-posts-empty")),
            );
        } else {
            for (idx, post) in posts.iter().take(MAX_VISIBLE_POSTS).enumerate() {
                let post_id = post.id.clone();
                let mut card = div()
                    .id(SharedString::from(format!("bulletin-post-{idx}")))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(12.))
                                    .child(post.author.clone()),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("bulletin-reply-{idx}")))
                                    .px_2()
                                    .py(px(1.))
                                    .rounded(px(crate::config::corners::CONTROL))
                                    .bg(rgb(theme.active))
                                    .text_size(px(11.))
                                    .cursor_pointer()
                                    .role(Role::Button)
                                    .child(fl!("team-action-reply"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let post_id = post_id.clone();
                                        this.team_view.update(cx, |view, cx| {
                                            view.reply_to = Some(post_id);
                                            cx.notify();
                                        });
                                    })),
                            ),
                    );
                if let Some(parent) = post.reply_to_author.as_ref() {
                    card = card.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme.muted))
                            .child(fl!("team-reply-to", author = parent.as_ref())),
                    );
                }
                card = card.child(div().text_size(px(13.)).child(post.body.clone()));

                if !post.deliveries.is_empty() {
                    let mut pills = div().flex().flex_wrap().gap_1().pt_1();
                    for (_, recipient, state) in &post.deliveries {
                        let status = match state {
                            DeliveryState::Pending => fl!("team-delivery-pending"),
                            DeliveryState::Acknowledged => fl!("team-delivery-acknowledged"),
                        };
                        pills = pills.child(
                            div()
                                .px_2()
                                .py(px(1.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(theme.background))
                                .text_size(px(10.))
                                .text_color(rgb(theme.muted))
                                .child(fl!(
                                    "team-delivery-recipient-status",
                                    recipient = recipient.as_ref(),
                                    status = status
                                )),
                        );
                    }
                    card = card.child(pills);
                } else if post.body.contains('@') {
                    card = card.child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(theme.muted))
                            .child(fl!("team-delivery-unavailable")),
                    );
                }
                feed = feed.child(card);
            }
        }

        let mut column = div()
            .id("team-bulletin-sidebar")
            .debug_selector(|| "team-bulletin-sidebar".into())
            .w(px(BULLETIN_WIDTH))
            .flex_none()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(self.fill().panel(theme.sidebar_background()))
            .border_s(px(1.))
            .border_color(rgb(theme.active))
            .text_color(rgb(theme.foreground))
            .child(header)
            .child(feed);

        if let Some((author, author_label)) = post_author {
            let mut chips = div().flex().flex_wrap().gap_1();
            for (chip_idx, token) in [fl!("team-mention-team"), fl!("team-mention-supervisor")]
                .into_iter()
                .chain(members.iter().map(|m| format!("@{}", m.name)))
                .enumerate()
            {
                let token_click = token.clone();
                let post_body_chip = post_body.clone();
                chips = chips.child(
                    div()
                        .id(SharedString::from(format!("bulletin-chip-{chip_idx}")))
                        .px_2()
                        .py(px(1.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.surface))
                        .text_size(px(11.))
                        .text_color(rgb(theme.muted))
                        .cursor_pointer()
                        .role(Role::Button)
                        .child(token)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            let token_click = token_click.clone();
                            post_body_chip.update(cx, |input, cx| {
                                let current = input.text().to_owned();
                                let next = if current.trim().is_empty() {
                                    format!("{token_click} ")
                                } else {
                                    format!("{current} {token_click} ")
                                };
                                input.set_text_at(&next, next.len(), cx);
                            });
                        })),
                );
            }

            let mut composer = div()
                .flex_none()
                .p_3()
                .border_t_1()
                .border_color(rgb(theme.active))
                .flex()
                .flex_col()
                .gap_2()
                .child(chips);

            if let Some(reply_id) = reply_to.as_ref() {
                composer = composer.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_size(px(11.))
                        .text_color(rgb(theme.muted))
                        .child(format!("Replying to {reply_id}"))
                        .child(
                            div()
                                .id("bulletin-cancel-reply")
                                .px_2()
                                .py(px(1.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(theme.active))
                                .cursor_pointer()
                                .role(Role::Button)
                                .child(fl!("team-action-cancel"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.team_view.update(cx, |view, cx| {
                                        view.reply_to = None;
                                        cx.notify();
                                    });
                                })),
                        ),
                );
            }

            let post_body_submit = post_body.clone();
            composer = composer
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(theme.muted))
                        .child(fl!("team-posting-as", author = author_label.as_ref())),
                )
                .child(post_body)
                .child(
                    div()
                        .id("team-post-submit")
                        .px_3()
                        .py_1()
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.active))
                        .cursor_pointer()
                        .role(Role::Button)
                        .child(if reply_to.is_some() {
                            fl!("team-action-send-reply")
                        } else {
                            fl!("team-action-post")
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let body = post_body_submit.read(cx).text().trim().to_owned();
                            if body.is_empty() {
                                return;
                            }
                            let current_reply = this.team_view.read(cx).reply_to.clone();
                            let action = match current_reply {
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
                            this.team_view.update(cx, |view, cx| {
                                view.reply_to = None;
                                view.post_body.update(cx, |input, cx| input.clear(cx));
                            });
                            this.push_team_action(action, cx);
                        })),
                );
            column = column.child(composer);
        }
        column
    }
}
