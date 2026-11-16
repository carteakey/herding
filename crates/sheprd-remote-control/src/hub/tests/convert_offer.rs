//! Which terminal pane a phone is offered to move into a chat.

use super::*;
use crate::hub::layout::project;

#[test]
fn a_terminal_agent_that_can_move_is_offered_with_its_pane() {
    let layout = project(&snapshot(json!([agent("p2", "t2", json!([]))])));
    let (_, terminal) = layout.terminal("p2").unwrap();
    // Busy agents stay offered; the move itself says to wait.
    assert_eq!(terminal.convert, Some(("p2".to_owned(), Agent::Claude)));
    let (_, other) = layout.terminal("p1").unwrap();
    assert_eq!(
        other.convert, None,
        "a tab without the agent offers nothing"
    );
}

#[test]
fn a_tab_without_an_agent_that_can_move_offers_nothing() {
    let mut gemini = agent("p2", "t2", json!([]));
    gemini["agent"] = json!("gemini");
    let mut unknown = agent("p2", "t2", json!([]));
    unknown["agent"] = json!("cursor");
    for agents in [json!([]), json!([gemini]), json!([unknown])] {
        let layout = project(&snapshot(agents));
        assert_eq!(layout.terminal("p2").unwrap().1.convert, None);
    }
}
