use super::*;
use crate::reorder::Beside;

fn ids<'a>(store: &'a Store, scope: &'a Scope) -> Vec<&'a str> {
    store
        .kept_workspaces(scope)
        .map(|workspace| workspace.id.as_str())
        .collect()
}

#[test]
fn kept_workspaces_are_numbered_per_app_and_listed_per_host() {
    let (here, there) = (Scope::endpoint("here"), Scope::endpoint("there"));
    let mut store = Store::default();
    assert!(store.keep_default(&here, "herdr-2", "Herdr 2"));
    assert!(
        !store.keep_default(&here, "herdr-2", "Herdr 2"),
        "kept once"
    );
    let first = store
        .keep_workspace(&here, "api", "/src/api", false)
        .unwrap();
    let second = store
        .keep_workspace(&there, "web", "/src/web", false)
        .unwrap();
    assert_eq!((first.as_str(), second.as_str()), ("space-1", "space-2"));
    assert_eq!(ids(&store, &here), ["herdr-2", "space-1"]);
    assert_eq!(ids(&store, &there), ["space-2"]);
    assert_eq!(
        store.kept_workspace(&here, "space-1").unwrap().cwd,
        "/src/api"
    );
    assert!(store.kept_workspace(&there, "space-1").is_none());
}

#[test]
fn a_kept_workspace_renames_moves_and_closes_with_its_tabs() {
    let scope = Scope::endpoint("here");
    let mut store = Store::default();
    let a = store.keep_workspace(&scope, "a", "", false).unwrap();
    let b = store.keep_workspace(&scope, "b", "", false).unwrap();
    let c = store.keep_workspace(&scope, "c", "", false).unwrap();
    assert!(store.rename_kept(&scope, &a, "  api\n"));
    assert_eq!(store.kept_workspace(&scope, &a).unwrap().label, "api");
    assert!(!store.rename_kept(&scope, &a, " "), "a name is kept");
    assert!(store.move_kept(&scope, &c, Beside::Before(a.as_str())));
    assert_eq!(ids(&store, &scope), [c.as_str(), &a, &b]);
    assert!(store.move_kept(&scope, &c, Beside::After(b.as_str())));
    assert_eq!(ids(&store, &scope), [a.as_str(), &b, &c]);
    assert!(
        !store.move_kept(&scope, &c, Beside::After(b.as_str())),
        "already there"
    );
    store
        .open(scope.clone(), &b, url("https://example.com"), None)
        .unwrap();
    store
        .open(scope.clone(), &a, url("https://example.org"), None)
        .unwrap();
    let closed = store.forget_kept(&scope, &b);
    assert_eq!(closed.len(), 1);
    assert_eq!(ids(&store, &scope), [a.as_str(), &c]);
    assert_eq!(store.in_workspace(&scope, &b).count(), 0);
    assert_eq!(store.in_workspace(&scope, &a).count(), 1);
}

#[test]
fn kept_workspaces_survive_a_restart_and_a_damaged_one_costs_only_itself() {
    let saved = r#"{"workspaces": [
        {"id": "space-1", "scope": "s", "label": "api", "cwd": "/a"},
        {"id": "", "scope": "s", "label": "bad", "cwd": ""},
        {"id": "space-2", "scope": "s", "label": 7},
        {"id": "space-3"}
    ], "tabs": []}"#;
    let restored = parse(saved.as_bytes()).unwrap();
    assert_eq!(restored.workspaces.len(), 1);
    let bytes = serde_json::to_vec(&restored).unwrap();
    let reread = sheprd_session::SavedTabs::parse(&bytes).unwrap();
    assert_eq!(reread.workspaces("s").next().unwrap().label, "api");
    let without = serde_json::to_string(&Saved {
        tabs: Vec::new(),
        workspaces: Vec::new(),
    })
    .unwrap();
    assert!(
        !without.contains("workspaces"),
        "older readers see no change"
    );
}

#[test]
fn the_app_keeps_a_bounded_number_of_workspaces() {
    let scope = Scope::endpoint("here");
    let mut store = Store::default();
    for _ in 0..super::super::kept::MAX_WORKSPACES {
        store.keep_workspace(&scope, "w", "", false).unwrap();
    }
    assert!(store.keep_workspace(&scope, "w", "", false).is_none());
    assert!(
        store
            .keep_workspace(&scope, "w", &"x".repeat(5000), false)
            .is_none()
    );
}

#[test]
fn a_closed_workspace_id_is_not_given_again() {
    let scope = Scope::endpoint("here");
    let mut store = Store::default();
    let first = store.keep_workspace(&scope, "a", "", false).unwrap();
    store.forget_kept(&scope, &first);
    let second = store.keep_workspace(&scope, "b", "", false).unwrap();
    assert_ne!(first, second);
    assert!(is_kept_id(&second) && is_kept_id(crate::core_terminals::WORKSPACE));
    assert!(!is_kept_id("w_1"));
}
