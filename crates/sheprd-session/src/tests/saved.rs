use crate::{MAX_SAVED_BYTES, MAX_SAVED_TABS, SavedTabs, saved::Error};
use serde_json::{Value, json};

fn parse(tabs: Value) -> SavedTabs {
    SavedTabs::parse(&serde_json::to_vec(&json!({ "tabs": tabs })).unwrap()).unwrap()
}

fn tab(id: u64, location: Value) -> Value {
    json!({"id": id, "scope": "s", "workspace_id": "w", "title": "t", "location": location})
}

fn ids(saved: &SavedTabs) -> Vec<u64> {
    saved.core_tabs("s").map(|tab| tab.id).collect()
}

#[test]
fn only_typing_core_tabs_of_the_scope_are_listed() {
    let core = json!({"kind": "core_terminal", "panes": "t1"});
    let saved = parse(json!([
        tab(1, core.clone()),
        tab(2, json!({"kind": "core_terminal", "panes": "t2", "observe": true})),
        tab(3, json!({"kind": "web", "url": "https://example.com"})),
        tab(4, Value::Null),
        tab(7, json!({"kind": "review", "panes": "t3"})),
        {"id": 5, "scope": "other", "workspace_id": "w", "title": "t", "location": core.clone()},
        {"id": 6, "scope": "s", "workspace_id": "", "title": "t", "location": core},
    ]));
    assert_eq!(ids(&saved), [1]);
}

#[test]
fn a_tab_from_before_splits_names_one_terminal() {
    let saved = parse(json!([tab(
        1,
        json!({"kind": "core_terminal", "terminal": "t4"})
    )]));
    let tabs: Vec<_> = saved.core_tabs("s").collect();
    assert_eq!(tabs[0].panes.first().as_str(), "t4");
}

#[test]
fn an_unreadable_tab_costs_only_itself() {
    let twice = json!({"axis": "row", "first": "t1", "second": "t1"});
    let saved = parse(json!([
        tab(1, json!({"kind": "core_terminal", "panes": twice})),
        tab(2, json!({"kind": "core_terminal", "panes": "t2"})),
    ]));
    assert_eq!(ids(&saved), [2]);
}

#[test]
fn saved_tabs_are_bounded() {
    let large = vec![b' '; MAX_SAVED_BYTES + 1];
    assert!(matches!(SavedTabs::parse(&large), Err(Error::Large)));
    let tabs = vec![tab(1, Value::Null); MAX_SAVED_TABS + 1];
    let many = serde_json::to_vec(&json!({ "tabs": tabs })).unwrap();
    assert!(matches!(SavedTabs::parse(&many), Err(Error::Many)));
    assert!(matches!(SavedTabs::parse(b"{"), Err(Error::Format(_))));
}

#[test]
fn kept_workspaces_of_the_scope_are_listed_in_order() {
    let saved = SavedTabs::parse(
        &serde_json::to_vec(&json!({
            "tabs": [],
            "workspaces": [
                {"id": "space-2", "scope": "s", "label": "B", "cwd": "/b"},
                {"id": "space-1", "scope": "other", "label": "A", "cwd": "/a"},
                {"id": "", "scope": "s", "label": "bad", "cwd": ""},
                {"id": "space-3", "scope": "s", "label": "C", "cwd": "/c"},
                {"id": "space-4", "scope": "s", "label": 7},
                "not a workspace",
            ],
        }))
        .unwrap(),
    )
    .unwrap();
    let ids: Vec<_> = saved.workspaces("s").map(|w| w.id.as_str()).collect();
    assert_eq!(ids, ["space-2", "space-3"]);
}

#[test]
fn a_file_from_before_kept_workspaces_has_none() {
    let saved = parse(json!([]));
    assert_eq!(saved.workspaces("s").count(), 0);
}

#[test]
fn a_terminal_labelled_for_a_workspace_names_one_a_tab_can_be_in() {
    use crate::{Labelled, SESSION_LABEL, WORKSPACE_LABEL, labelled_workspace};
    use std::collections::BTreeMap;
    let labels = |id: &str| BTreeMap::from([(WORKSPACE_LABEL.to_owned(), id.to_owned())]);
    let primary = |id| Some(Labelled { id, session: None });
    assert_eq!(labelled_workspace(&labels("space-3")), primary("space-3"));
    assert_eq!(labelled_workspace(&labels("w1")), primary("w1"));
    let mut named = labels("w1");
    named.insert(SESSION_LABEL.to_owned(), "dev".to_owned());
    assert_eq!(
        labelled_workspace(&named),
        Some(Labelled {
            id: "w1",
            session: Some("dev")
        })
    );
    named.insert(SESSION_LABEL.to_owned(), "a:b".to_owned());
    assert_eq!(
        labelled_workspace(&named),
        None,
        "a session name has no colon"
    );
    assert_eq!(labelled_workspace(&labels("")), None);
    assert_eq!(labelled_workspace(&labels("core:terminals")), None);
    assert_eq!(labelled_workspace(&labels(&"x".repeat(257))), None);
    assert_eq!(labelled_workspace(&BTreeMap::new()), None);
}

#[test]
fn a_workspace_saved_before_names_were_marked_keeps_its_name() {
    let saved = SavedTabs::parse(
        &serde_json::to_vec(&json!({
            "tabs": [],
            "workspaces": [
                {"id": "space-1", "scope": "s", "label": "mine", "cwd": "/a"},
                {"id": "space-2", "scope": "s", "label": "b", "cwd": "/b", "custom": false},
            ],
        }))
        .unwrap(),
    )
    .unwrap();
    let custom: Vec<_> = saved.workspaces("s").map(|w| w.custom).collect();
    assert_eq!(custom, [true, false]);
    let written = serde_json::to_value(saved.workspaces("s").nth(1).unwrap()).unwrap();
    assert_eq!(written["custom"], false, "always written");
}
