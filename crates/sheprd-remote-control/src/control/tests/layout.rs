//! A phone's change to one of Herding's Herdr 2 tabs, renames included.

use crate::control::{
    changes::AskId,
    layout::{Ask, MAX_NAME, Op},
};

fn ask(tab: u64, terminal: &str, op: Op) -> Ask {
    Ask {
        id: AskId(1),
        tab,
        terminal: terminal.to_owned(),
        op,
    }
}

#[test]
fn a_rename_names_its_pane_or_workspace_within_bounds() {
    assert!(ask(4, "t1", Op::RenameTab(Some("build".into()))).valid());
    assert!(ask(4, "t1", Op::RenamePane(None)).valid());
    assert!(!ask(4, "", Op::RenameTab(Some("build".into()))).valid());
    assert!(!ask(4, "t1", Op::RenameTab(Some("x".repeat(MAX_NAME + 1)))).valid());
    let workspace = |workspace: &str, name: &str| {
        ask(
            0,
            "",
            Op::RenameWorkspace {
                workspace: workspace.into(),
                name: name.into(),
            },
        )
    };
    assert!(workspace("space-2", "api").valid());
    assert!(
        !workspace("space-2", "  ").valid(),
        "a workspace keeps a name"
    );
    assert!(!workspace("", "api").valid());
    // Every other change names a pane.
    assert!(!ask(4, "", Op::Zoom).valid());
}

#[test]
fn renames_read_on_the_wire_as_named() {
    let wire = serde_json::to_value(Op::RenameTab(Some("build".into()))).unwrap();
    assert_eq!(wire, serde_json::json!({"rename_tab": "build"}));
    let wire = serde_json::to_value(Op::RenameWorkspace {
        workspace: "space-2".into(),
        name: "api".into(),
    })
    .unwrap();
    assert_eq!(
        wire,
        serde_json::json!({"rename_workspace": {"workspace": "space-2", "name": "api"}})
    );
    assert_eq!(serde_json::to_value(Op::Zoom).unwrap(), "zoom");
}
