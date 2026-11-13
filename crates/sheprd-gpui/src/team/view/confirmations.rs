//! The destructive-action confirmations the team view renders in place: a
//! member whose pane still reports a live agent (finding 7ad9beb0) and the
//! team deletion, each with the evidence or cost named before the click
//! lands.

use super::*;
use gpui::{Stateful, px};

impl TeamView {
    /// The team-deletion confirmation banner, when one is pending.
    pub(super) fn render_delete_team_banner(
        &self,
        team_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if !self.confirming_delete_team {
            return None;
        }
        let theme = &self.theme;
        let team_name_display = self.snapshot.team_name.clone().unwrap_or_default();
        Some(
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
                            .on_click(cx.listener({
                                let team_id = team_id.to_owned();
                                move |this, _, _, cx| {
                                    this.confirming_delete_team = false;
                                    cx.emit(TeamAction::DeleteTeam {
                                        team_id: team_id.clone(),
                                    });
                                }
                            })),
                        )
                        .child(
                            self.button(
                                "team-delete-cancel-btn",
                                fl!("team-action-cancel"),
                                ButtonVariant::Ghost,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirming_delete_team = false;
                                cx.notify();
                            })),
                        ),
                ),
        )
    }
}
