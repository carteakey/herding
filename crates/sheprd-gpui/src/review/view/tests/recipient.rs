use super::*;
use crate::review::view::review_recipient;

#[test]
fn a_review_of_a_herdr_2_pane_sends_to_no_herdr_agent() {
    let shown = snapshot(base(
        Some("w1:p1"),
        vec![agent("w1:p1", "w1", Some("Claude Code"))],
    ));
    // Herdr's agent works in the hidden workspace, maybe another checkout.
    assert_eq!(review_recipient(Some(&shown), true), None);
    // Without a herdr-core pane shown, Herdr's focused agent receives them.
    assert_eq!(
        review_recipient(Some(&shown), false).map(|agent| agent.pane_id),
        Some("w1:p1".into())
    );
    assert_eq!(review_recipient(None, false), None);
}

#[gpui::test]
fn reopening_a_review_from_herdr_1_forgets_the_herdr_2_pane(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, Some("working"));
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    view.update(cx, |view, _| {
        let id = the(view);
        let pane = (
            TabId::test(9),
            sheprd_backend::TerminalId::new("t1").unwrap(),
        );
        // Opened from a Herdr 2 pane: its editor goes beside that pane.
        view.aim_review(id, Some(pane.clone()));
        let review = &view.reviews[&id];
        assert_eq!(review.core, Some(pane));
        assert_eq!(review.agent, None);
        // Opened again from Herdr 1: beside Herdr's agent, not the old pane.
        view.aim_review(id, None);
        assert_eq!(view.reviews[&id].core, None);
    });
}
