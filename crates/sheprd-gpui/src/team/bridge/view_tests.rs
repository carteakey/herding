//! The snapshot never shows delivery receipts for members who left.

use std::collections::{HashMap, HashSet};

use super::*;
use crate::team::model::DeliveryState;
use sheprd_orchestrator::team_store::{
    Confidence, Finding, Member, Post, RecipientStatus, SessionTarget, TaskStatus,
};

fn member(id: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: id.into(),
        model: None,
        account: None,
        target: SessionTarget {
            host: "local".into(),
            session: format!("session-{id}"),
            workspace: Some("ws".into()),
            pane: Some(format!("pane-{id}")),
        },
    }
}

#[test]
fn view_posts_omits_deliveries_from_departed_members() {
    let team = Team {
        team_id: "t".into(),
        name: "T".into(),
        supervisor: member("lead"),
        members: vec![member("present")],
    };
    let post = Post {
        post_id: "p".into(),
        team_id: "t".into(),
        author: Author::Human {
            user_id: "human".into(),
        },
        reply_to: None,
        body: "Please inspect".into(),
        mentions: vec!["departed".into(), "present".into()],
        created_at: chrono::DateTime::UNIX_EPOCH,
    };
    let statuses = vec![
        ("departed".to_owned(), RecipientStatus::Pending),
        ("present".to_owned(), RecipientStatus::Pending),
    ];

    let posts = view_posts(&team, &[(post, statuses)]);

    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].deliveries.len(), 1);
    assert_eq!(posts[0].deliveries[0].0, "present");
    assert_eq!(posts[0].deliveries[0].1.as_ref(), "present");
    assert_eq!(posts[0].deliveries[0].2, DeliveryState::Pending);
}

#[test]
fn view_posts_resolves_reply_authors_and_falls_back_to_raw_ids() {
    let team = Team {
        team_id: "t".into(),
        name: "T".into(),
        supervisor: member("lead"),
        members: vec![member("peer")],
    };
    let root = Post {
        post_id: "root".into(),
        team_id: "t".into(),
        author: Author::Member {
            member_id: "peer".into(),
        },
        reply_to: None,
        body: "root".into(),
        mentions: Vec::new(),
        created_at: chrono::DateTime::UNIX_EPOCH,
    };
    let threaded = Post {
        post_id: "threaded".into(),
        team_id: "t".into(),
        author: Author::Human {
            user_id: "human".into(),
        },
        reply_to: Some("root".into()),
        body: "reply".into(),
        mentions: Vec::new(),
        created_at: chrono::DateTime::UNIX_EPOCH,
    };
    // The parent is not in the projected list: no author can be resolved.
    let orphan = Post {
        post_id: "orphan".into(),
        team_id: "t".into(),
        author: Author::Member {
            member_id: "peer".into(),
        },
        reply_to: Some("missing".into()),
        body: "orphan reply".into(),
        mentions: Vec::new(),
        created_at: chrono::DateTime::UNIX_EPOCH,
    };

    let posts = view_posts(
        &team,
        &[
            (root, Vec::new()),
            (threaded, Vec::new()),
            (orphan, Vec::new()),
        ],
    );

    assert_eq!(posts[0].author.as_ref(), "peer");
    assert_eq!(posts[1].author.as_ref(), fl!("team-author-human").as_str());
    assert_eq!(
        posts[1].reply_to_author.as_deref(),
        Some("peer"),
        "a threaded post shows its parent's author"
    );
    assert_eq!(
        posts[2].reply_to_author, None,
        "a reply whose parent is outside the list resolves to nothing"
    );
}

#[test]
fn projections_fall_back_to_raw_ids_for_unknown_members() {
    let team = Team {
        team_id: "t".into(),
        name: "T".into(),
        supervisor: member("lead"),
        members: Vec::new(),
    };
    let mut task = task_fixture("task-1");
    task.assignee = Some("departed".into());
    let tasks = view_tasks(&team, &[task]);

    assert_eq!(
        tasks[0].assignee.as_deref(),
        Some("departed"),
        "a departed assignee keeps their raw id, not a wrong name"
    );

    let finding = Finding {
        finding_id: "f".into(),
        team_id: "t".into(),
        author: Author::Member {
            member_id: "departed".into(),
        },
        task_id: None,
        description: "d".into(),
        evidence: "e".into(),
        confidence: Confidence::Medium,
        supersedes: None,
        created_at: chrono::DateTime::UNIX_EPOCH,
    };
    let findings = view_findings(&team, &[finding]);
    assert_eq!(findings[0].author.as_ref(), "departed");
}

#[test]
fn members_carry_the_configured_target_even_when_offline() {
    let team = Team {
        team_id: "t".into(),
        name: "T".into(),
        supervisor: member("lead"),
        members: vec![member("peer")],
    };
    let members = members(&team, &[], false, &HashSet::new(), &HashMap::new());

    assert_eq!(members.len(), 2);
    let peer = match members.iter().find(|m| m.member_id == "peer") {
        Some(p) => p,
        None => panic!("peer projected"),
    };
    assert_eq!(peer.pane, None, "no candidates: the member is not live");
    assert_eq!(
        peer.target_pane.as_ref().map(|p| p.pane_id.as_str()),
        Some("pane-peer"),
        "the configured target survives being offline"
    );
}

fn task_fixture(id: &str) -> Task {
    Task {
        task_id: id.into(),
        team_id: "t".into(),
        creator: Author::Member {
            member_id: "lead".into(),
        },
        title: "title".into(),
        brief: "brief".into(),
        assignee: None,
        status: TaskStatus::Open,
        blocker_reason: None,
        result: None,
        created_at: chrono::DateTime::UNIX_EPOCH,
        updated_at: chrono::DateTime::UNIX_EPOCH,
    }
}
