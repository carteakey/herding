use self::activity::Group;
use super::*;
use sheprd_chat_protocol::{Activity, ActivityKind, ActivityState, Transcript};

#[core::prelude::v1::test]
fn a_path_is_named_by_its_last_two_parts() {
    assert_eq!(
        short_path("crates/sheprd-gpui/src/chat/view.rs"),
        "chat/view.rs"
    );
    assert_eq!(short_path("view.rs"), "view.rs");
    assert_eq!(short_path("src/view.rs"), "src/view.rs");
    assert_eq!(short_path(r"C:\work\src\view.rs"), r"src\view.rs");
}

#[core::prelude::v1::test]
fn the_rail_needs_a_wide_pane() {
    let pane = |width: f32| Bounds::new(point(px(0.), px(0.)), size(px(width), px(400.)));
    assert!(fits(pane(MIN_PANE)));
    assert!(!fits(pane(MIN_PANE - 1.)));
}

fn activity(call: &str, kind: ActivityKind, state: ActivityState, ended: Option<u64>) -> Activity {
    Activity {
        call_id: call.to_owned(),
        detail: None,
        finished_ms: ended,
        kind,
        label: call.to_owned(),
        started_ms: 1,
        state,
        stoppable: false,
    }
}

fn calls<'a>(list: &[&'a Activity]) -> Vec<&'a str> {
    list.iter()
        .map(|activity| activity.call_id.as_str())
        .collect()
}

#[core::prelude::v1::test]
fn a_section_lists_what_runs_in_order_then_what_ended_latest_first() {
    let transcript = Transcript {
        activities: vec![
            activity("old", ActivityKind::Shell, ActivityState::Finished, Some(5)),
            activity("first", ActivityKind::Shell, ActivityState::Running, None),
            activity("new", ActivityKind::Shell, ActivityState::Killed, Some(9)),
            activity("second", ActivityKind::Shell, ActivityState::Running, None),
            activity(
                "agent",
                ActivityKind::Subagent,
                ActivityState::Running,
                None,
            ),
        ],
        ..Transcript::default()
    };
    let shells = Group::of(&transcript, RailSection::Shells);
    assert_eq!(calls(&shells.running), ["first", "second"]);
    assert_eq!(calls(&shells.ended), ["new", "old"]);
    assert_eq!(
        calls(&Group::of(&transcript, RailSection::Subagents).running),
        ["agent"]
    );
}

#[core::prelude::v1::test]
fn a_kind_from_a_newer_program_is_still_listed() {
    let transcript = Transcript {
        activities: vec![activity(
            "new",
            ActivityKind::Other,
            ActivityState::Running,
            None,
        )],
        ..Transcript::default()
    };
    assert!(!Group::of(&transcript, RailSection::Shells).is_empty());
    assert!(Group::of(&transcript, RailSection::Subagents).is_empty());
}

#[core::prelude::v1::test]
fn monitors_have_a_section_of_their_own() {
    let transcript = Transcript {
        activities: vec![activity(
            "watch",
            ActivityKind::Monitor,
            ActivityState::Running,
            None,
        )],
        ..Transcript::default()
    };
    assert!(!Group::of(&transcript, RailSection::Monitors).is_empty());
    assert!(Group::of(&transcript, RailSection::Shells).is_empty());
}
