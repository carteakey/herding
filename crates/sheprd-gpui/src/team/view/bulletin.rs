use super::{ButtonVariant, EXPANDED_POSTS_LIMIT, MAX_VISIBLE_POSTS, TeamAction, TeamView};
use crate::{
    SheprdWindow, backdrop::Fill, direction::Logical, i18n::fl, team::model::DeliveryState,
};
use gpui::{prelude::*, *};

const BULLETIN_WIDTH: f32 = 340.;
const MIN_TERMINAL_WIDTH: f32 = 260.;
pub(super) const RECENT_POSTS_LIMIT: usize = 20;

impl TeamView {
    /// The live bulletin feed, composer included. This runs on every frame
    /// the sidebar is open, so it borrows the snapshot directly: the old
    /// SheprdWindow version deep-cloned every post and member onto the UI
    /// thread once per frame because the entity borrow fought the listeners.
    /// Actions are emitted and reach the same bridge queue through the
    /// SheprdWindow subscription.
    pub(crate) fn render_bulletin_sidebar(
        &self,
        fill: Fill,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let snapshot = &self.snapshot;
        let post_body = self.post_body.clone();
        let reply_to = self.reply_to.clone();

        let pending_count: usize = snapshot
            .posts
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
                    .child(self.chip(fl!(
                        "team-bulletin-summary",
                        posts = snapshot.posts.len(),
                        unacknowledged = pending_count
                    ))),
            )
            .child(
                self.button(
                    "team-bulletin-hide",
                    fl!("team-action-hide"),
                    ButtonVariant::Primary,
                )
                .on_click(cx.listener(|_, _, _, cx| {
                    cx.emit(TeamAction::ToggleBulletinSidebar);
                })),
            );

        let mut feed = div()
            .id("team-bulletin-feed")
            .track_scroll(&self.bulletin_scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                let dy = match event.delta {
                    ScrollDelta::Lines(delta) => delta.y,
                    ScrollDelta::Pixels(delta) => f32::from(delta.y),
                };
                if dy > 0. && this.bulletin_following {
                    this.bulletin_following = false;
                    cx.notify();
                }
            }));

        if snapshot.posts.is_empty() {
            feed = feed.child(
                self.card()
                    .p_3()
                    .text_size(px(12.))
                    .text_color(rgb(theme.muted))
                    .child(fl!("team-posts-empty")),
            );
        } else {
            let expanded_limit = self.bulletin_expanded_limit.max(EXPANDED_POSTS_LIMIT);
            let (skip_count, visible_posts) =
                if !self.bulletin_show_all && snapshot.posts.len() > RECENT_POSTS_LIMIT {
                    let skip = snapshot.posts.len() - RECENT_POSTS_LIMIT;
                    (skip, &snapshot.posts[skip..])
                } else if self.bulletin_show_all {
                    let limit = expanded_limit.min(MAX_VISIBLE_POSTS);
                    let skip = snapshot.posts.len().saturating_sub(limit);
                    (skip, &snapshot.posts[skip..])
                } else {
                    let count = snapshot.posts.len().min(MAX_VISIBLE_POSTS);
                    (0, &snapshot.posts[..count])
                };

            if snapshot.posts.len() > RECENT_POSTS_LIMIT {
                if !self.bulletin_show_all {
                    let hidden_count = snapshot.posts.len() - RECENT_POSTS_LIMIT;
                    feed = feed.child(
                        self.button(
                            "bulletin-show-earlier",
                            format!("Show older messages ({} earlier)", hidden_count),
                            ButtonVariant::Toggle,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.bulletin_show_all = true;
                            this.bulletin_expanded_limit = EXPANDED_POSTS_LIMIT;
                            cx.notify();
                        })),
                    );
                } else {
                    if skip_count > 0 {
                        feed = feed.child(
                            self.button(
                                "bulletin-show-earlier",
                                format!("Show older messages ({} earlier)", skip_count),
                                ButtonVariant::Toggle,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.bulletin_expanded_limit += EXPANDED_POSTS_LIMIT;
                                cx.notify();
                            })),
                        );
                    }
                    feed = feed.child(
                        self.button(
                            "bulletin-collapse-earlier",
                            "Show recent messages only",
                            ButtonVariant::Toggle,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.bulletin_show_all = false;
                            this.bulletin_expanded_limit = EXPANDED_POSTS_LIMIT;
                            cx.notify();
                        })),
                    );
                }
            }

            for (i, post) in visible_posts.iter().enumerate() {
                let idx = skip_count + i;
                let post_id = post.id.clone();
                let mut card = self
                    .card()
                    .id(SharedString::from(format!("bulletin-post-{idx}")))
                    .debug_selector(move || format!("bulletin-post-{idx}"))
                    .gap_1()
                    .p_2()
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
                                    .truncate()
                                    .child(post.author.clone()),
                            )
                            .child({
                                let reply_post_id = post.id.clone();
                                self.button(
                                    SharedString::from(format!("bulletin-reply-{idx}")),
                                    fl!("team-action-reply"),
                                    ButtonVariant::Primary,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.reply_to = Some(reply_post_id.clone());
                                        cx.notify();
                                    },
                                ))
                            }),
                    );
                if let Some(parent) = post.reply_to_author.as_ref() {
                    card = card.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme.muted))
                            .child(fl!("team-reply-to", author = parent.as_ref())),
                    );
                }
                let is_long = post.body.lines().count() > 3 || post.body.chars().count() > 180;
                let is_expanded = self.expanded_bulletin_ids.contains(&post_id);
                if is_long && !is_expanded {
                    let preview: String = post.body.lines().take(3).collect::<Vec<_>>().join("\n");
                    let preview = if preview.chars().count() > 180 {
                        format!("{}...", preview.chars().take(180).collect::<String>())
                    } else {
                        format!("{preview}...")
                    };
                    let toggle_id = post_id.clone();
                    card = card.child(div().text_size(px(13.)).child(preview));
                    card = card.child(
                        self.text_button(
                            SharedString::from(format!("bulletin-expand-{idx}")),
                            "Show more",
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.expanded_bulletin_ids.insert(toggle_id.clone());
                            cx.notify();
                        })),
                    );
                } else if is_long && is_expanded {
                    let toggle_id = post_id.clone();
                    card = card.child(div().text_size(px(13.)).child(post.body.clone()));
                    card = card.child(
                        self.text_button(
                            SharedString::from(format!("bulletin-collapse-{idx}")),
                            "Show less",
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.expanded_bulletin_ids.remove(&toggle_id);
                            cx.notify();
                        })),
                    );
                } else {
                    card = card.child(div().text_size(px(13.)).child(post.body.clone()));
                }

                if !post.deliveries.is_empty() {
                    let mut pills = div().flex().flex_wrap().gap_1().pt_1();
                    for (_, recipient, state) in &post.deliveries {
                        let status = match state {
                            DeliveryState::Pending => fl!("team-delivery-pending"),
                            DeliveryState::Acknowledged => fl!("team-delivery-acknowledged"),
                        };
                        pills = pills.child(self.chip(fl!(
                            "team-delivery-recipient-status",
                            recipient = recipient.as_ref(),
                            status = status
                        )));
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

            if self.bulletin_following {
                let scroll_for_canvas = self.bulletin_scroll.clone();
                feed = feed.child(
                    canvas(
                        move |_, window, _| {
                            let max_y = scroll_for_canvas.max_offset().y;
                            if max_y > px(0.) {
                                let target = -max_y;
                                let current = scroll_for_canvas.offset().y;
                                let diff = target - current;
                                if diff < px(-1.) {
                                    if cfg!(test) {
                                        scroll_for_canvas.set_offset(point(px(0.), target));
                                    } else {
                                        let remaining = f32::from(diff);
                                        let step = (remaining * 0.35).min(-2.0);
                                        let next =
                                            (f32::from(current) + step).max(f32::from(target));
                                        scroll_for_canvas.set_offset(point(px(0.), px(next)));
                                        if next > f32::from(target) {
                                            window.request_animation_frame();
                                        }
                                    }
                                }
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_0(),
                );
            } else {
                feed = feed.child(
                    self.button(
                        "bulletin-jump-to-latest",
                        "Jump to latest ↓",
                        ButtonVariant::Primary,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.bulletin_following = true;
                        this.bulletin_scroll
                            .set_offset(point(px(0.), -this.bulletin_scroll.max_offset().y));
                        cx.notify();
                    })),
                );
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
            .bg(fill.panel(theme.sidebar_background()))
            .border_s(px(1.))
            .border_color(rgb(theme.active))
            .text_color(rgb(theme.foreground))
            .child(header)
            .child(feed);

        if let Some((_, author_label)) = snapshot.post_author.as_ref() {
            let mut chips = div().flex().flex_wrap().gap_1();
            for (chip_idx, token) in [fl!("team-mention-team"), fl!("team-mention-supervisor")]
                .into_iter()
                .chain(snapshot.members.iter().map(|m| format!("@{}", m.name)))
                .enumerate()
            {
                let token_click = token.clone();
                chips = chips.child(
                    self.chip_button(
                        SharedString::from(format!("bulletin-chip-{chip_idx}")),
                        token,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let token_click = token_click.clone();
                        this.post_body.update(cx, |input, cx| {
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
                            self.button(
                                "bulletin-cancel-reply",
                                fl!("team-action-cancel"),
                                ButtonVariant::Primary,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reply_to = None;
                                cx.notify();
                            })),
                        ),
                );
            }

            composer = composer
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(theme.muted))
                        .child(fl!("team-posting-as", author = author_label.as_ref())),
                )
                .child(
                    div()
                        .id("bulletin-composer-input")
                        .debug_selector(|| "bulletin-composer-input".into())
                        .w_full()
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                            if event.keystroke.key.as_str() == "enter" {
                                cx.stop_propagation();
                                if !event.keystroke.modifiers.shift {
                                    this.submit_bulletin_post(cx);
                                }
                            }
                        }))
                        .child(post_body),
                )
                .child(
                    self.button(
                        "team-post-submit",
                        if reply_to.is_some() {
                            fl!("team-action-send-reply")
                        } else {
                            fl!("team-action-post")
                        },
                        ButtonVariant::Primary,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.submit_bulletin_post(cx))),
                );
            column = column.child(composer);
        }
        column
    }

    fn submit_bulletin_post(&mut self, cx: &mut Context<Self>) {
        let body = self.post_body.read(cx).text().trim().to_owned();
        if body.is_empty() {
            return;
        }
        let Some(team_id) = self.snapshot.team_id.clone() else {
            return;
        };
        let Some((author, _)) = self.snapshot.post_author.as_ref() else {
            return;
        };
        let author = author.clone();
        let action = match self.reply_to.take() {
            Some(reply_to) => TeamAction::Reply {
                team_id,
                author,
                reply_to,
                body,
            },
            None => TeamAction::Post {
                team_id,
                author,
                body,
            },
        };
        self.post_body.update(cx, |input, cx| input.clear(cx));
        cx.emit(action);
    }
}

impl SheprdWindow {
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

    pub(crate) fn render_team_bulletin_sidebar(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let fill = self.fill();
        self.team_view
            .update(cx, |view, cx| view.render_bulletin_sidebar(fill, cx))
    }
}
