use crate::{browser::TabId, core_terminals::fallback::Holder};
use sheprd_backend::TerminalId;

fn id(name: &str) -> TerminalId {
    TerminalId::new(name).unwrap()
}

#[test]
fn the_shell_shown_in_place_of_herdr_holds_its_terminal() {
    let shell = id("t3");
    assert_eq!(
        Holder::of(None, Some(&shell), &shell),
        Some(Holder::Fallback)
    );
    // Another terminal of the same core is no caller of this window.
    assert_eq!(Holder::of(None, Some(&shell), &id("t4")), None);
    assert_eq!(Holder::of(None, None, &shell), None);
}

#[test]
fn a_tab_holding_the_terminal_comes_first() {
    let tab = TabId::test(7);
    let terminal = id("t3");
    assert_eq!(
        Holder::of(Some(tab), Some(&terminal), &terminal),
        Some(Holder::Tab(tab))
    );
}
