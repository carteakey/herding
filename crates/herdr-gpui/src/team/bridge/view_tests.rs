//! The snapshot never shows delivery receipts for members who left.

use super::*;
use crate::team::model::DeliveryState;
use herdr_orchestrator::team_store::{Member, Post, RecipientStatus, SessionTarget};

fn member(id: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: id.into(),
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
