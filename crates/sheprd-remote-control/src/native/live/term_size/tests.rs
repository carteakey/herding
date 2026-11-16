use super::*;

const CORE: &str = "core:0000abcd-7431";

#[test]
fn a_herdr_2_view_scrolls_within_the_history_it_last_read() {
    let mut sizing = Sizing::default();
    // Before any read there is no history to scroll into.
    assert!(sizing.scroll(CORE, 5));
    assert_eq!(sizing.up, 0);
    sizing.history = 20;
    assert!(sizing.scroll(CORE, 5));
    assert!(sizing.scroll(CORE, 50));
    assert_eq!(sizing.up, 20);
    assert!(sizing.scroll(CORE, -8));
    assert_eq!(sizing.up, 12);
    assert!(sizing.scroll(CORE, -100));
    assert_eq!(sizing.up, 0);
}

#[test]
fn a_herdr_1_view_scrolls_only_through_a_held_attach() {
    let mut sizing = Sizing::default();
    assert!(!sizing.scroll("p2", 5));
    assert_eq!(sizing.up, 0);
}
