//! Swapping, stepping through, and evening out a herdr-core tab's panes from
//! the keyboard, as Herdr's own shortcuts do for its panes.

use super::commands::{id, run, window_with_split};
use crate::{SheprdWindow, browser::TabId, controls::Command};
use gpui::{Entity, TestAppContext, VisualTestContext};
use sheprd_backend::TerminalId;

fn active(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: TabId) -> TerminalId {
    cx.update(|_, cx| view.read(cx).core_terminals.tabs[&tab].active.clone())
}

fn order(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: TabId) -> Vec<String> {
    cx.update(|_, cx| {
        view.read(cx).core_terminals.tabs[&tab]
            .panes
            .terminals()
            .iter()
            .map(ToString::to_string)
            .collect()
    })
}

#[gpui::test]
fn next_previous_and_last_pane_move_the_keyboard(cx: &mut TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["t1", "t2", "t3"]);
    run(&view, cx, Command::NextPane);
    assert_eq!(active(&view, cx, tab), id("t2"));
    run(&view, cx, Command::NextPane);
    run(&view, cx, Command::NextPane);
    assert_eq!(active(&view, cx, tab), id("t1"), "wraps around");
    run(&view, cx, Command::PreviousPane);
    assert_eq!(active(&view, cx, tab), id("t3"));
    run(&view, cx, Command::LastPane);
    assert_eq!(active(&view, cx, tab), id("t1"));
    run(&view, cx, Command::LastPane);
    assert_eq!(active(&view, cx, tab), id("t3"));
}

#[gpui::test]
fn a_swap_trades_places_and_the_pane_keeps_the_keyboard(cx: &mut TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["t1", "t2"]);
    run(&view, cx, Command::SwapRight);
    assert_eq!(order(&view, cx, tab), ["t2", "t1"]);
    assert_eq!(active(&view, cx, tab), id("t1"));
    // Nothing lies further right.
    run(&view, cx, Command::SwapRight);
    assert_eq!(order(&view, cx, tab), ["t2", "t1"]);
    let unsaved = cx.update(|_, cx| view.read(cx).core_terminals.tabs[&tab].unsaved);
    assert!(unsaved, "the new order is saved");
}

#[gpui::test]
fn equalize_evens_out_the_splits(cx: &mut TestAppContext) {
    let (view, cx, tab) = window_with_split(cx, &["t1", "t2", "t3"]);
    run(&view, cx, Command::ResizeRight);
    run(&view, cx, Command::EqualizeSplits);
    let widths: Vec<f32> = cx.update(|_, cx| {
        view.read(cx).core_terminals.tabs[&tab]
            .panes
            .rects()
            .iter()
            .map(|(_, rect)| rect.width)
            .collect()
    });
    for width in widths {
        assert!((width - 1. / 3.).abs() < 0.01, "{width}");
    }
}

#[test]
fn a_drag_held_through_equalize_never_moves_another_split() {
    use crate::core_terminals::{panes::Axis, split::SplitDrag};
    let tab = TabId::test(1);
    // t1 | (t2 | (t3 | t4)): the split at [second] divides t2 from t3 and t4.
    let mut panes = crate::core_terminals::panes::Panes::single(id("t1"));
    for pair in [("t1", "t2"), ("t2", "t3"), ("t3", "t4")] {
        panes.split(&id(pair.0), Axis::Row, id(pair.1)).unwrap();
    }
    let held = SplitDrag::at(tab, panes.root(), &[true]).unwrap();
    assert!(panes.equalize());
    // Balanced as (t1 | t2) | (t3 | t4), the same place now divides t3 from
    // t4: the drag held from before is not its drag.
    let now = SplitDrag::at(tab, panes.root(), &[true]).unwrap();
    assert_ne!(held, now);
}
