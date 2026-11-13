//! Hex-prefix ID resolution through the scoped CLI verb path (issue #26).
#![allow(clippy::unwrap_used)]
use super::*;

fn hex_member(id: &str, session: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: format!("bee-{session}"),
        model: None,
        account: None,
        target: SessionTarget {
            host: "boot".into(),
            session: session.into(),
            workspace: Some("w".into()),
            pane: Some(format!("p-{session}")),
        },
    }
}

fn hex_store() -> (
    tempfile::TempDir,
    TeamStore,
    SessionTarget,
    SessionTarget,
    String,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = TeamStore::open(&dir.path().join("team").join("t.sqlite3")).unwrap();
    let supervisor = hex_member("aabb0000-1111-4222-8333-444444444444", "sup");
    let worker = hex_member("aabb0000-5555-4666-8777-888888888888", "wrk");
    store
        .save_team(&Team {
            team_id: "t".into(),
            name: "t".into(),
            supervisor: supervisor.clone(),
            members: vec![worker.clone()],
        })
        .unwrap();
    let worker_id = worker.member_id;
    (dir, store, supervisor.target, worker.target, worker_id)
}

fn created_task_id(store: &mut TeamStore, caller: &SessionTarget, title: &str) -> String {
    let Reply::ScopedTask { task, .. } = scoped(
        store,
        "t",
        caller,
        &caller.host,
        &[],
        &[],
        ScopedAction::TaskCreate {
            task_id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            brief: "brief".into(),
            assignee: None,
        },
        &[],
    )
    .unwrap() else {
        panic!("expected a created task");
    };
    task.task_id
}

fn store_message(error: Error) -> String {
    let Error::Store(source) = error else {
        panic!("expected a store error: {error}");
    };
    source.to_string()
}

#[test]
fn task_verbs_accept_a_unique_prefix_and_refuse_an_ambiguous_one() {
    let (_dir, mut store, sup, worker, _worker_id) = hex_store();
    let task_a = created_task_id(&mut store, &sup, "A");
    // A second task sharing task_a's first seven hex chars: distinguishable
    // by an eight-char prefix, ambiguous at the shared head.
    let shared_head = &task_a[..7];
    // Keep the first seven chars shared and change the eighth, so the
    // seven-char head is ambiguous and the eight-char prefix is unique.
    let eighth = task_a.as_bytes()[7];
    let mut task_b = task_a.clone();
    task_b.replace_range(7..8, if eighth == b'0' { "1" } else { "0" });
    let Reply::ScopedTask {
        task: created_b, ..
    } = scoped(
        &mut store,
        "t",
        &sup,
        &sup.host,
        &[],
        &[],
        ScopedAction::TaskCreate {
            task_id: task_b,
            title: "B".into(),
            brief: "brief".into(),
            assignee: None,
        },
        &[],
    )
    .unwrap()
    else {
        panic!("expected a created task");
    };
    let task_b = created_b.task_id;

    // Claim by a unique prefix: the reply carries the full ID.
    let Reply::ScopedTask { task, .. } = scoped(
        &mut store,
        "t",
        &worker,
        &worker.host,
        &[],
        &[],
        ScopedAction::TaskClaim {
            task_id: task_a[..8].to_owned(),
        },
        &[],
    )
    .unwrap() else {
        panic!("expected a claimed task");
    };
    assert_eq!(task.task_id, task_a);

    // The shared head matches two tasks and names both candidates.
    let Err(error) = scoped(
        &mut store,
        "t",
        &worker,
        &worker.host,
        &[],
        &[],
        ScopedAction::TaskClaim {
            task_id: shared_head.to_owned(),
        },
        &[],
    ) else {
        panic!("expected an ambiguous-prefix refusal");
    };
    let message = store_message(error);
    assert!(
        message.contains(&task_a) && message.contains(&task_b),
        "{message}"
    );

    // Complete by prefix; a full ID still works for the second task.
    scoped(
        &mut store,
        "t",
        &worker,
        &worker.host,
        &[],
        &[],
        ScopedAction::TaskComplete {
            task_id: task_a[..8].to_owned(),
            result: "done".into(),
        },
        &[],
    )
    .unwrap();
    let Reply::ScopedTask { task, .. } = scoped(
        &mut store,
        "t",
        &sup,
        &sup.host,
        &[],
        &[],
        ScopedAction::TaskCancel { task_id: task_b },
        &[],
    )
    .unwrap() else {
        panic!("expected a cancelled task");
    };
    assert_eq!(task.status, team_store::TaskStatus::Cancelled);
}

#[test]
fn bulletin_reply_ack_and_member_refs_accept_prefixes() {
    let (_dir, mut store, sup, worker, worker_id) = hex_store();
    let Reply::ScopedPosted { post: parent, .. } = scoped(
        &mut store,
        "t",
        &sup,
        &sup.host,
        &[],
        &[],
        ScopedAction::BulletinPost {
            post_id: uuid::Uuid::new_v4().to_string(),
            reply_to: None,
            text: "parent".into(),
            mentions: vec![],
        },
        &[],
    )
    .unwrap() else {
        panic!("expected a posted reply");
    };
    // Reply-to by prefix.
    let Reply::ScopedPosted { post: reply, .. } = scoped(
        &mut store,
        "t",
        &worker,
        &worker.host,
        &[],
        &[],
        ScopedAction::BulletinPost {
            post_id: uuid::Uuid::new_v4().to_string(),
            reply_to: Some(parent.post_id[..8].to_owned()),
            text: "child".into(),
            mentions: vec![],
        },
        &[],
    )
    .unwrap() else {
        panic!("expected a posted reply");
    };
    assert_eq!(reply.reply_to.as_deref(), Some(parent.post_id.as_str()));

    // Ack by prefix.
    scoped(
        &mut store,
        "t",
        &worker,
        &worker.host,
        &[],
        &[],
        ScopedAction::BulletinAck {
            message_id: parent.post_id[..8].to_owned(),
        },
        &[],
    )
    .unwrap();

    // Assign and mention by unique member prefix.
    let task = created_task_id(&mut store, &sup, "T");
    let Reply::ScopedTask { task, .. } = scoped(
        &mut store,
        "t",
        &sup,
        &sup.host,
        &[],
        &[],
        ScopedAction::TaskAssign {
            task_id: task[..8].to_owned(),
            assignee: "aabb0000-5555".into(),
        },
        &[],
    )
    .unwrap() else {
        panic!("expected an assigned task");
    };
    assert_eq!(task.assignee.as_deref(), Some(worker_id.as_str()));
    let Reply::ScopedPosted { post, .. } = scoped(
        &mut store,
        "t",
        &sup,
        &sup.host,
        &[],
        &[],
        ScopedAction::BulletinPost {
            post_id: uuid::Uuid::new_v4().to_string(),
            reply_to: None,
            text: "ping".into(),
            mentions: vec!["aabb0000-5555".into()],
        },
        &[],
    )
    .unwrap() else {
        panic!("expected a posted reply");
    };
    assert_eq!(post.mentions, std::slice::from_ref(&worker_id));

    // An ambiguous member prefix is refused, not guessed: both members named.
    let ambiguous_post = scoped(
        &mut store,
        "t",
        &sup,
        &sup.host,
        &[],
        &[],
        ScopedAction::BulletinPost {
            post_id: uuid::Uuid::new_v4().to_string(),
            reply_to: None,
            text: "ping".into(),
            mentions: vec!["aabb0000".into()],
        },
        &[],
    );
    let Err(error) = ambiguous_post else {
        panic!("expected an ambiguous-mention refusal");
    };
    let message = store_message(error);
    assert!(
        message.contains(&worker_id) && message.contains("aabb0000-1111"),
        "{message}"
    );
}

#[test]
fn finding_links_accept_prefixes_and_unknown_prefixes_stay_not_found() {
    let (_dir, mut store, sup, _worker, _worker_id) = hex_store();
    let task = created_task_id(&mut store, &sup, "T");
    let Reply::ScopedFinding { finding, .. } = scoped(
        &mut store,
        "t",
        &sup,
        &sup.host,
        &[],
        &[],
        ScopedAction::FindingAdd {
            finding_id: uuid::Uuid::new_v4().to_string(),
            task_id: Some(task[..8].to_owned()),
            description: "found".into(),
            evidence: "evidence".into(),
            confidence: Confidence::Medium,
            supersedes: None,
        },
        &[],
    )
    .unwrap() else {
        panic!("expected a recorded finding");
    };
    assert_eq!(finding.task_id.as_deref(), Some(task.as_str()));

    // An unmatched prefix passes through and the store reports not-found.
    let Err(error) = scoped(
        &mut store,
        "t",
        &sup,
        &sup.host,
        &[],
        &[],
        ScopedAction::TaskClaim {
            task_id: "cafe9".into(),
        },
        &[],
    ) else {
        panic!("expected a not-found refusal");
    };
    let message = store_message(error);
    assert!(message.contains("cafe9"), "{message}");
}
