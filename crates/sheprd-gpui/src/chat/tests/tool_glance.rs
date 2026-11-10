//! A folded tool call still shows what it changed and the start of its
//! output; opening it trades the preview for the full text.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{Modifiers, TestAppContext};
use sheprd_chat_protocol::{Tool, ToolContent, ToolKind, ToolStatus};
use sheprd_client::ConnectTarget;

#[gpui::test]
fn folded_tools_keep_diff_counts_and_an_output_preview(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![Entry {
                id: EntryId(1),
                kind: EntryKind::Tool(Tool {
                    call_id: "call".into(),
                    content: vec![
                        ToolContent::Diff {
                            path: "/work/src/lib.rs".into(),
                            old: Some("a\nb\n".into()),
                            new: "a\nc\nd\n".into(),
                        },
                        ToolContent::Text {
                            text: "compiled".into(),
                        },
                    ],
                    kind: ToolKind::Edit,
                    locations: vec!["/work/src/lib.rs".into()],
                    parent: None,
                    started_ms: None,
                    status: ToolStatus::Completed,
                    title: "Edit".into(),
                }),
            }],
        ),
        cx,
    );
    assert!(cx.debug_bounds("chat-tool-details-1").is_none());
    assert!(cx.debug_bounds("chat-tool-diff-1-0").is_some());
    assert!(cx.debug_bounds("chat-tool-preview-1").is_some());
    let header = cx.debug_bounds("chat-tool-1").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-tool-details-1").is_some());
    assert!(cx.debug_bounds("chat-tool-diff-1-0").is_some());
    assert!(cx.debug_bounds("chat-tool-preview-1").is_none());
}

#[gpui::test]
fn an_edit_too_large_to_compare_stays_listed_without_a_count(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let edit = |id, old: String| Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: format!("call-{id}"),
            content: vec![ToolContent::Diff {
                path: "/work/src/lib.rs".into(),
                old: Some(old),
                new: "new\n".into(),
            }],
            kind: ToolKind::Edit,
            locations: Vec::new(),
            parent: None,
            started_ms: None,
            status: ToolStatus::Completed,
            title: "Edit".into(),
        }),
    };
    let entries = vec![edit(1, "old\n".into()), edit(2, "x".repeat(128 * 1024))];
    let _stream = attach_with(&view, &program, snapshot(cursor(1, 0), entries), cx);
    assert!(cx.debug_bounds("chat-tool-diff-2-0").is_some());
    view.read_with(cx, |view, _| {
        let changed = view.chats.pane("w1:p1").unwrap().prepared.changed();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].path, "/work/src/lib.rs");
        assert_eq!(
            changed[0].lines, None,
            "a partial total would read as whole"
        );
    });
}

#[gpui::test]
fn a_finished_edit_sent_without_its_text_stays_listed(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let call = |id, kind, status, location: &str| Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: format!("call-{id}"),
            content: vec![ToolContent::Text {
                text: "Original edit context exceeds live preview limits.".into(),
            }],
            kind,
            locations: vec![location.into()],
            parent: None,
            started_ms: None,
            status,
            title: "Edit".into(),
        }),
    };
    let entries = vec![
        call(
            1,
            ToolKind::Edit,
            ToolStatus::Completed,
            "/work/report:2026",
        ),
        call(2, ToolKind::Edit, ToolStatus::Failed, "/work/failed.csv"),
        call(3, ToolKind::Read, ToolStatus::Completed, "/work/read.csv"),
    ];
    let _stream = attach_with(&view, &program, snapshot(cursor(1, 0), entries), cx);
    view.read_with(cx, |view, _| {
        let changed = view.chats.pane("w1:p1").unwrap().prepared.changed();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].path, "/work/report:2026", "a name is never cut");
        assert_eq!(changed[0].lines, None);
    });
}

#[gpui::test]
fn files_of_an_edit_with_a_fragment_left_out_have_no_count(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let entries = vec![Entry {
        id: EntryId(1),
        kind: EntryKind::Tool(Tool {
            call_id: "call".into(),
            content: vec![
                ToolContent::Diff {
                    path: "/work/a.rs".into(),
                    old: Some("a\n".into()),
                    new: "b\n".into(),
                },
                ToolContent::Text {
                    text: sheprd_chat_protocol::EDIT_OMITTED.into(),
                },
            ],
            kind: ToolKind::Edit,
            locations: vec!["/work/a.rs".into(), "/work/b.rs".into()],
            parent: None,
            started_ms: None,
            status: ToolStatus::Completed,
            title: "MultiEdit".into(),
        }),
    }];
    let _stream = attach_with(&view, &program, snapshot(cursor(1, 0), entries), cx);
    view.read_with(cx, |view, _| {
        let changed = view.chats.pane("w1:p1").unwrap().prepared.changed();
        let paths: Vec<_> = changed.iter().map(|diff| diff.path.as_str()).collect();
        assert_eq!(paths, ["/work/a.rs", "/work/b.rs"]);
        assert!(changed.iter().all(|diff| diff.lines.is_none()));
    });
}
