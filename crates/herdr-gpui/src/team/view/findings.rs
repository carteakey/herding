use super::{ButtonVariant, MAX_VISIBLE_FINDINGS, RECENT_FINDINGS_LIMIT, TeamAction, TeamView};
use crate::i18n::fl;
use gpui::{prelude::*, *};
use herdr_orchestrator::team_store::Confidence;

impl TeamView {
    pub(super) fn render_findings(&self, team_id: &str, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let mut section = div().flex().flex_col().gap_2();
        let findings_count = self.snapshot.findings.len();
        let toggle_symbol = if self.findings_collapsed {
            "▶"
        } else {
            "▼"
        };
        // The title itself toggles the collapse, so this header stays custom
        // rather than using `section_header`.
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .id("team-findings-toggle")
                        .flex()
                        .items_center()
                        .gap_1()
                        .min_w_0()
                        .cursor_pointer()
                        .role(Role::Button)
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(14.))
                                .truncate()
                                .child(format!(
                                    "{} {} ({})",
                                    toggle_symbol,
                                    fl!("team-findings-title"),
                                    findings_count
                                )),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.findings_collapsed = !this.findings_collapsed;
                            cx.notify();
                        })),
                )
                .child(
                    self.button(
                        "team-finding-toggle-form",
                        if self.finding_form_open {
                            fl!("team-action-cancel")
                        } else {
                            fl!("team-action-add-finding")
                        },
                        ButtonVariant::Primary,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.finding_form_open = !this.finding_form_open;
                        cx.notify();
                    })),
                ),
        );

        if self.finding_form_open
            && let Some((author, _)) = self.snapshot.post_author.as_ref()
        {
            let author = author.clone();
            let team_id = team_id.to_owned();
            let current_confidence = match self.finding_confidence {
                Confidence::Low => fl!("team-confidence-low"),
                Confidence::Medium => fl!("team-confidence-medium"),
                Confidence::High => fl!("team-confidence-high"),
            };
            section = section.child(
                self.card()
                    .gap_2()
                    .p_3()
                    .child(self.finding_description.clone())
                    .child(self.finding_evidence.clone())
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                self.button(
                                    "team-finding-confidence",
                                    current_confidence,
                                    ButtonVariant::Primary,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.finding_confidence = match this.finding_confidence {
                                            Confidence::High => Confidence::Medium,
                                            Confidence::Medium => Confidence::Low,
                                            Confidence::Low => Confidence::High,
                                        };
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                self.button(
                                    "team-finding-add",
                                    fl!("team-action-add-finding"),
                                    ButtonVariant::Primary,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        let description = this
                                            .finding_description
                                            .read(cx)
                                            .text()
                                            .trim()
                                            .to_owned();
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
                                        this.finding_form_open = false;
                                        cx.emit(TeamAction::AddFinding {
                                            team_id: team_id.clone(),
                                            author: author.clone(),
                                            description,
                                            evidence,
                                            confidence,
                                        });
                                    },
                                )),
                            ),
                    ),
            );
        }

        if !self.findings_collapsed {
            if self.snapshot.findings.is_empty() {
                section = section.child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(theme.muted))
                        .child(fl!("team-findings-empty")),
                );
            } else {
                // The window reads from the newest end so current findings
                // stay visible; the show-earlier control widens it toward
                // older rows.
                let total = self.snapshot.findings.len();
                let visible = (MAX_VISIBLE_FINDINGS + self.earlier_findings_rows).min(total);
                let window = &self.snapshot.findings[total - visible..];
                let limit = if self.findings_show_all {
                    window.len()
                } else {
                    RECENT_FINDINGS_LIMIT
                };
                let count = window.len().min(limit);
                for finding in window.iter().take(count) {
                    let confidence_label = match finding.confidence {
                        Confidence::Low => fl!("team-confidence-low"),
                        Confidence::Medium => fl!("team-confidence-medium"),
                        Confidence::High => fl!("team-confidence-high"),
                    };
                    let expanded = self.expanded_finding_ids.contains(&finding.id);
                    let fid = finding.id.clone();
                    let fid_tag = fid.clone();
                    let mut card = self
                        .card()
                        .id(SharedString::from(format!("team-finding-{}", finding.id)))
                        .debug_selector(|| "team-finding-card".into())
                        .child(
                            div()
                                .size_0()
                                .debug_selector(move || format!("team-finding-{fid_tag}")),
                        )
                        .gap_1()
                        .p_2();

                    let fid_click = fid.clone();
                    let row = div()
                        .id(SharedString::from(format!(
                            "team-finding-row-{}",
                            finding.id
                        )))
                        .debug_selector(|| "team-finding-row".into())
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .cursor_pointer()
                        .role(Role::Button)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.expanded_finding_ids.contains(&fid_click) {
                                this.expanded_finding_ids.remove(&fid_click);
                            } else {
                                this.expanded_finding_ids.insert(fid_click.clone());
                            }
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .debug_selector(|| "team-finding-author".into())
                                        .flex_none()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_size(px(12.))
                                        .child(finding.author.clone()),
                                )
                                .when(!expanded, |summary| {
                                    summary.child(
                                        div()
                                            .debug_selector(|| "team-finding-description".into())
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(12.))
                                            .child(finding.description.clone()),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap_2()
                                .child(self.chip(confidence_label))
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(rgb(theme.muted))
                                        .child(if expanded { "▼" } else { "▶" }),
                                ),
                        );
                    card = card.child(row);
                    if expanded {
                        card = card.child(
                            div()
                                .debug_selector(|| "team-finding-description".into())
                                .text_size(px(12.))
                                .child(finding.description.clone()),
                        );
                        card = card.child(self.well().child(fl!(
                            "team-finding-evidence",
                            evidence = finding.evidence.as_ref()
                        )));
                        if let Some(superseded) = finding.supersedes.as_ref() {
                            card =
                                card.child(
                                    div().text_size(px(11.)).text_color(rgb(theme.muted)).child(
                                        fl!("team-finding-supersedes", id = superseded.as_str()),
                                    ),
                                );
                        }
                    }
                    section = section.child(card);
                }

                if findings_count > RECENT_FINDINGS_LIMIT {
                    let (label, show_all) = if self.findings_show_all {
                        (
                            format!("Show fewer findings ({RECENT_FINDINGS_LIMIT})"),
                            false,
                        )
                    } else {
                        let more = findings_count - RECENT_FINDINGS_LIMIT;
                        (
                            format!("Show all {findings_count} findings ({more} more)"),
                            true,
                        )
                    };
                    section = section.child(
                        self.button("team-findings-show-more", label, ButtonVariant::Toggle)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.findings_show_all = show_all;
                                cx.notify();
                            })),
                    );
                }

                if total > MAX_VISIBLE_FINDINGS + self.earlier_findings_rows
                    || self.snapshot.findings_older
                {
                    section = section.child(
                        div()
                            .id("team-findings-load-earlier")
                            .debug_selector(|| "team-findings-load-earlier".into())
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
                            .child("Load earlier findings")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.earlier_findings_rows += MAX_VISIBLE_FINDINGS;
                                if this.snapshot.findings_older {
                                    cx.emit(TeamAction::LoadEarlierFindings);
                                }
                                cx.notify();
                            })),
                    );
                }
            }
        }
        section
    }
}
