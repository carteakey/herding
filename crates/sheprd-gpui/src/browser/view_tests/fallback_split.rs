use crate::browser::groups::Shown;

#[gpui::test]
fn the_stand_in_shell_keeps_its_group_when_a_page_splits_beside_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = None;
        view.live.missing_installation = true;
        view
    });
    let home = view.read_with(cx, |view, _| view.group_slots()[0].id);
    cx.update(|window, cx| view.update(cx, |view, cx| view.split_group(home, window, cx)));
    view.read_with(cx, |view, cx| {
        let slots = view.group_slots();
        assert_eq!(slots.len(), 2);
        assert_eq!(view.primary_group(), Some(home));
        // No Herdr tab is ever focused, yet the group holding the window's
        // connection still shows its terminal: the herdr-core shell.
        assert_eq!(view.group_shown(home, cx), Shown::Terminal);
        let other = slots.iter().find(|slot| slot.id != home).unwrap().id;
        assert_eq!(view.group_shown(other, cx), Shown::Empty);
    });
}
