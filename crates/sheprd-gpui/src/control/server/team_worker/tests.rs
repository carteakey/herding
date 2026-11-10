#![allow(clippy::unwrap_used)]
use super::*;
use sheprd_orchestrator::team_store::{Author, Member, Recipients, SessionTarget};
use std::{sync::mpsc, time::Duration};

fn team() -> Team {
    let member = Member {
        member_id: "member-1".into(),
        display_name: "lead".into(),
        target: SessionTarget {
            host: "local".into(),
            session: "session-1".into(),
            workspace: Some("workspace-1".into()),
            pane: Some("pane-1".into()),
        },
    };
    let second = Member {
        member_id: "member-2".into(),
        display_name: "reviewer".into(),
        target: SessionTarget {
            host: "local".into(),
            session: "session-2".into(),
            workspace: Some("workspace-2".into()),
            pane: Some("pane-2".into()),
        },
    };
    Team {
        team_id: "team-1".into(),
        name: "review".into(),
        supervisor: member,
        members: vec![second],
    }
}

fn paused(path: PathBuf) -> (Worker, SyncSender<()>, Receiver<()>) {
    let (release, gate) = mpsc::sync_channel(1);
    let (stopped, done) = mpsc::sync_channel(1);
    let worker = Worker::spawn(path, Some(gate), Some(stopped)).unwrap();
    (worker, release, done)
}

fn watch(
    receivers: &mut Vec<Receiver<Result<Reply, Error>>>,
) -> impl FnOnce(Receiver<Result<Reply, Error>>) -> bool + '_ {
    |receiver| {
        receivers.push(receiver);
        true
    }
}

#[test]
fn queue_is_bounded_and_full_is_delivered_to_registered_watch() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("team").join("teams.sqlite");
    let (worker, release, _) = paused(path);
    let mut answers = Vec::new();
    for _ in 0..QUEUE_CAPACITY {
        worker
            .submit(
                Operation::Snapshot {
                    team_id: "team-1".into(),
                    limit: 1,
                },
                watch(&mut answers),
            )
            .unwrap();
    }
    let error = worker
        .submit(
            Operation::Snapshot {
                team_id: "team-1".into(),
                limit: 1,
            },
            watch(&mut answers),
        )
        .unwrap_err();
    assert!(matches!(error, Error::Busy));
    assert!(matches!(
        answers.last().unwrap().recv().unwrap(),
        Err(Error::Busy)
    ));
    release.send(()).unwrap();
    for answer in answers.into_iter().take(QUEUE_CAPACITY) {
        let _ = answer.recv_timeout(Duration::from_secs(3)).unwrap();
    }
    worker.finish();
}

#[test]
fn missing_response_does_not_cancel_an_accepted_mutation_and_drop_drains_queue() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("team").join("teams.sqlite");
    let (worker, release, stopped) = paused(path.clone());
    let mut answer = None;
    worker
        .submit(Operation::SaveTeam(team()), |receiver| {
            answer = Some(receiver);
            true
        })
        .unwrap();
    drop(answer.take());
    drop(worker);
    release.send(()).unwrap();
    stopped.recv_timeout(Duration::from_secs(3)).unwrap();

    let reopened = TeamStore::open(&path).unwrap();
    assert_eq!(reopened.team("team-1").unwrap().unwrap().name, "review");
}

#[test]
fn rejected_watch_never_queues_a_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("team").join("teams.sqlite");
    let (worker, release, stopped) = paused(path.clone());
    assert!(matches!(
        worker.submit(Operation::SaveTeam(team()), |_| false),
        Err(Error::WatchUnavailable)
    ));
    drop(worker);
    release.send(()).unwrap();
    stopped.recv_timeout(Duration::from_secs(3)).unwrap();
    let store = TeamStore::open(&path).unwrap();
    assert!(store.team("team-1").unwrap().is_none());
}

#[test]
fn broadcast_is_resolved_by_the_store_and_excludes_member_author() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("team").join("teams.sqlite");
    let worker = Worker::start(path).unwrap();
    let mut saved = None;
    worker
        .submit(Operation::SaveTeam(team()), |receiver| {
            saved = Some(receiver);
            true
        })
        .unwrap();
    assert!(matches!(
        saved
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap(),
        Ok(Reply::Saved)
    ));

    let mut posted = None;
    worker
        .submit(
            Operation::AddPost {
                post_id: "post-1".into(),
                team_id: "team-1".into(),
                author: Author::Member {
                    member_id: "member-1".into(),
                },
                reply_to: None,
                body: "status".into(),
                recipients: Recipients::All,
            },
            |receiver| {
                posted = Some(receiver);
                true
            },
        )
        .unwrap();
    let Ok(Reply::Saved) = posted
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
    else {
        panic!("expected posted entry");
    };
    worker.finish();
}

#[test]
fn snapshot_batches_statuses_and_drops_departed_members() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("team").join("teams.sqlite");
    let mut store = TeamStore::open(&path).unwrap();
    store.save_team(&team()).unwrap();
    let human = Author::Human {
        user_id: "human".into(),
    };
    store
        .add_post_with_recipients(
            "p1",
            "team-1",
            &human,
            None,
            "one",
            &Recipients::Explicit(vec!["member-2".into(), "member-1".into()]),
        )
        .unwrap();
    store
        .add_post_with_recipients("p2", "team-1", &human, None, "everybody", &Recipients::All)
        .unwrap();
    store
        .add_post_with_recipients(
            "p3",
            "team-1",
            &human,
            None,
            "for the departing member",
            &Recipients::Explicit(vec!["member-2".into()]),
        )
        .unwrap();
    store.acknowledge("p1", "member-1").unwrap();
    store.acknowledge("p1", "member-2").unwrap();

    store.remove_member("team-1", "member-2").unwrap();

    let reply = execute(
        &mut store,
        Operation::Snapshot {
            team_id: "team-1".into(),
            limit: 50,
        },
    )
    .unwrap();
    let Reply::Snapshot { posts, .. } = reply else {
        panic!("snapshot reply expected");
    };
    let statuses_of = |posts: &[PostDelivery], post_id: &str| {
        posts
            .iter()
            .find(|(post, _)| post.post_id == post_id)
            .map(|(_, statuses)| statuses.clone())
            .unwrap_or_default()
    };
    // The member still in the team keeps its dated receipt, ordered by
    // recipient id. The departed member's mention and acknowledgement rows
    // were deleted with it (abb045ee), so the batched read returns none.
    let p1 = statuses_of(&posts, "p1");
    assert_eq!(p1.len(), 1, "only the remaining member's receipt");
    assert_eq!(p1[0].0, "member-1");
    assert!(
        matches!(p1[0].1, RecipientStatus::Acknowledged(_)),
        "the surviving receipt stays dated: {:?}",
        p1[0].1
    );
    // A broadcast from a human mentions every member, so the member still in
    // the team is still pending on it.
    let p2 = statuses_of(&posts, "p2");
    assert_eq!(p2, vec![("member-1".to_owned(), RecipientStatus::Pending)]);
    // The departed member's only mention was on p3, and it is gone.
    assert!(statuses_of(&posts, "p3").is_empty());
}

#[test]
fn status_reports_pending_mentions_with_one_count_query() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("team").join("teams.sqlite");
    let mut store = TeamStore::open(&path).unwrap();
    store.save_team(&team()).unwrap();
    let human = Author::Human {
        user_id: "human".into(),
    };
    for index in 0..3 {
        store
            .add_post_with_recipients(
                &format!("p{index}"),
                "team-1",
                &human,
                None,
                "please look",
                &Recipients::Explicit(vec!["member-2".into()]),
            )
            .unwrap();
    }
    store.acknowledge("p0", "member-2").unwrap();

    let reply = execute(
        &mut store,
        Operation::Scoped {
            team_id: "team-1".into(),
            caller: SessionTarget {
                host: "local".into(),
                session: "session-2".into(),
                workspace: Some("workspace-2".into()),
                pane: Some("pane-2".into()),
            },
            action: ScopedAction::Status { team_id: None },
            live_hosts: Vec::new(),
        },
    )
    .unwrap();
    let Reply::ScopedStatus {
        pending_mentions, ..
    } = reply
    else {
        panic!("status reply expected");
    };
    assert_eq!(pending_mentions, 2, "three mentions, one acknowledged");
}
