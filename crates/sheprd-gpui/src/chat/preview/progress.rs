//! The `progress` and `progress-before` cases: a command still running with
//! the output streamed so far, and a background task with its reported
//! progress, both open. `progress-before` is the same turn as the adapters
//! sent it before live progress: titles only until each call ends.

use sheprd_chat_protocol::{Entry, EntryId, EntryKind, Tool, ToolContent, ToolKind, ToolStatus};

const OUTPUT: &str = "running 454 tests\ntest codex::bridge::tests::command_output::deltas_stream_into_the_card ... ok\ntest harness::streaming::tests::the_first_change_is_published_at_once ... ok\ntest harness::streaming::tests::later_changes_share_one_deadline ... ok\ntest pi::bridge::tests::streaming::partial_results_replace_the_snapshot ... ok"; // i18n: data

const TASK: &str = "Reported progress: Reading the reconnect path in sheprd-client\nLatest tool: Grep\nTools used: 7\nElapsed: 42s"; // i18n: data

fn tool(id: u64, kind: ToolKind, title: &str, content: Option<&str>) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: format!("call-{id}"),
            content: content
                .map(|text| vec![ToolContent::Text { text: text.into() }])
                .unwrap_or_default(),
            kind,
            locations: Vec::new(),
            parent: None,
            started_ms: None,
            status: ToolStatus::InProgress,
            title: title.into(),
        }),
    }
}

pub(super) fn entries(live: bool) -> Vec<Entry> {
    let command = "`cargo test -p sheprd-acp --locked`"; // i18n: data
    let task = "Task: Explore the attach reconnect path"; // i18n: data
    vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: "Run the adapter tests, and have a sub-agent look at why the attach socket drops after a restart.".into(), // i18n: data
                files: 0,
            },
        },
        Entry {
            id: EntryId(2),
            kind: EntryKind::Agent {
                text: "Starting the tests and an explorer in the background.".into(), // i18n: data
            },
        },
        tool(3, ToolKind::Execute, command, live.then_some(OUTPUT)),
        tool(4, ToolKind::Think, task, live.then_some(TASK)),
    ]
}
