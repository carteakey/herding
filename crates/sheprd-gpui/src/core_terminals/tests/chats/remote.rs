//! A tab on another host runs no chat: `herdr-acp` and its socket are this
//! computer's, and another core's `t1` is not this core's `t1`.

use super::*;
use crate::browser::Scope;

#[test]
fn a_tab_on_another_host_holds_no_chat_even_where_ids_match() {
    let mut terminals = terminals();
    // The remote core also has a t1, in its own tab.
    terminals.tabs.insert(
        TabId::test(3),
        TabPanes::new(
            Host::Remote(Scope::endpoint("ssh:box")),
            Panes::single(id("t1")),
            false,
        ),
    );
    let shown: Vec<&str> = terminals
        .chat_terminals()
        .map(|info| info.id.as_str())
        .collect();
    assert_eq!(shown.iter().filter(|id| **id == "t1").count(), 1);
    assert_eq!(terminals.tab_of(&id("t1")), Some(TabId::test(1)));
    assert!(!terminals.drives(TabId::test(3)));
    assert_eq!(terminals.chat_split_at(TabId::test(3)), None);
    assert!(terminals.visible_chat_panes(TabId::test(3)).is_empty());
    assert_eq!(terminals.info(TabId::test(3), &id("t1")), None);
}
