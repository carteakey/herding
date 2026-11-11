//! A supervisor wake is only worth marking as reported once it can reach the
//! supervisor. These pin what happens to a wake that is held - because the
//! supervisor's pane or the whole team is paused, or because another team is
//! showing - and that it arrives exactly once.
use super::*;
use crate::team::bridge::queue::DrainedPrompt;
use sheprd_orchestrator::team_store::{Author, Member, SessionTarget};
use std::collections::HashSet;

fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(secs, 0).unwrap_or(chrono::DateTime::UNIX_EPOCH)
}

fn member(id: &str, name: &str, pane: &str) -> Member {
    Member {
        member_id: id.into(),
        display_name: name.into(),
        target: SessionTarget {
            host: "local".into(),
            session: format!("sess-{id}"),
            workspace: Some("ws".into()),
            pane: Some(pane.into()),
        },
    }
}

fn team(id: &str) -> Team {
    Team {
        team_id: id.into(),
        name: format!("team-{id}"),
        supervisor: member("sup", "supervisor", "p_sup"),
        members: vec![member("worker", "worker", "p1")],
    }
}

fn stored_task(status: TaskStatus, updated_secs: i64) -> Task {
    Task {
        task_id: "task1".into(),
        team_id: "t1".into(),
        creator: Author::Human {
            user_id: "human".into(),
        },
        title: format!("report {updated_secs}"),
        brief: String::new(),
        assignee: Some("worker".into()),
        status,
        blocker_reason: None,
        result: None,
        created_at: at(0),
        updated_at: at(updated_secs),
    }
}

/// A team whose supervisor's result went unreported, so the fallback is due.
fn board() -> Bridge {
    Bridge {
        selected: Some("t1".into()),
        team: Some(team("t1")),
        tasks: vec![stored_task(TaskStatus::Done, 10)],
        ..Bridge::default()
    }
}

/// The window's own wake step: ask the bridge, queue the prompt for the
/// supervisor's pane with its signals, and do NOT mark it seen. A held wake is
/// recorded only when the drain actually delivers it.
fn queue_wake(bridge: &mut Bridge) -> SupervisorWake {
    bridge.last_fallback = None;
    let wake = match bridge.supervisor_fallback_due(Instant::now()) {
        Some(wake) => wake,
        None => panic!("expected a supervisor wake"),
    };
    bridge.enqueue_prompt(
        wake.team_id.clone(),
        wake.host.clone(),
        wake.pane_id.clone(),
        wake.prompt.clone(),
        wake.summary.clone(),
        wake.signals.clone(),
    );
    wake
}

#[test]
fn a_wake_held_for_a_paused_supervisor_arrives_once_when_the_pane_resumes() {
    let mut bridge = board();
    bridge.paused_members.insert("sup".into());
    let wake = queue_wake(&mut bridge);
    assert_eq!(wake.pane_id, "p_sup");
    assert_eq!(bridge.queued_count(), 1, "the wake waits for its pane");
    assert!(
        !bridge.wake_seen(&wake.team_id, &wake.signals[0]),
        "a held wake is not marked seen"
    );

    assert!(
        bridge.drain_unpaused_prompts().is_empty(),
        "a paused supervisor pane is not woken"
    );

    // While the wake is still held and unrecorded, the fallback can offer it
    // again; the queue must not grow a duplicate.
    bridge.last_fallback = None;
    assert!(
        bridge.supervisor_fallback_due(Instant::now()).is_some(),
        "a held wake stays pending for the fallback"
    );
    queue_wake(&mut bridge);
    assert_eq!(
        bridge.queued_count(),
        1,
        "a still-held wake is queued once, not once per offer"
    );

    bridge.paused_members.clear();
    let delivered = bridge.drain_unpaused_prompts();
    assert_eq!(delivered.len(), 1, "unpausing delivers the held wake");
    assert_eq!(delivered[0].pane_id, "p_sup");
    assert!(
        delivered[0].prompt.contains("report 10") && delivered[0].summary.contains("unseen"),
        "the wake names the result it is about: {:?} / {:?}",
        delivered[0].prompt,
        delivered[0].summary
    );
    assert_eq!(
        delivered[0].signals, wake.signals,
        "the held prompt carries its wake signals"
    );

    // The window marks the signal only after the delivery succeeds.
    bridge.mark_supervisor_woken(&wake.team_id, delivered[0].signals.clone());
    assert!(
        bridge.drain_unpaused_prompts().is_empty(),
        "a held wake is delivered once, not every tick after it"
    );
    bridge.last_fallback = None;
    assert!(
        bridge.supervisor_fallback_due(Instant::now()).is_none(),
        "the delivered result stays marked as reported"
    );
}

#[test]
fn a_team_pause_holds_a_wake_and_unpausing_releases_it() {
    let mut bridge = board();
    let wake = queue_wake(&mut bridge);
    bridge.paused_teams.insert("t1".into());

    assert_eq!(bridge.queued_count(), 1, "the pause queue keeps the wake");
    assert!(
        bridge.drain_unpaused_prompts().is_empty(),
        "a paused team dispatches nothing"
    );
    bridge.last_fallback = None;
    assert!(
        bridge.supervisor_fallback_due(Instant::now()).is_none(),
        "a paused team offers no fallback wake"
    );

    bridge.paused_teams.clear();
    let delivered = bridge.drain_unpaused_prompts();
    assert_eq!(delivered.len(), 1, "the team's wake waits out its pause");
    assert_eq!(delivered[0].pane_id, wake.pane_id);
    bridge.mark_supervisor_woken(&wake.team_id, delivered[0].signals.clone());
    bridge.last_fallback = None;
    assert!(
        bridge.supervisor_fallback_due(Instant::now()).is_none(),
        "the delivered wake is marked reported"
    );
}

/// The stale-queue shape: a wake sits in the queue for a pane that has left
/// the roster and that no snapshot shows. It waits out its retry once - a
/// pane that returns in time still gets it - and is then marked undeliverable
/// and counted in the status surface, instead of being re-offered every tick.
#[test]
fn an_unreachable_wake_waits_out_its_retry_then_is_marked_undeliverable() {
    let mut bridge = board();
    bridge.paused_members.insert("sup".into());
    queue_wake(&mut bridge);
    // The roster re-binds elsewhere, the way re-registration moves every
    // member: no roster pane is p_sup any more.
    bridge.team = Some(Team {
        supervisor: member("sup", "supervisor", "p_moved"),
        ..team("t1")
    });
    bridge.paused_members.clear();

    // A due wake to a gone pane is dropped and counted, not re-offered.
    let dropped = bridge.expire_unreachable_prompts(&HashSet::new());
    assert_eq!(
        dropped,
        vec!["p_sup".to_string()],
        "the vanished pane is named"
    );
    assert_eq!(bridge.queued_count(), 0, "the wake is not kept forever");
    assert_eq!(bridge.undeliverable, 1, "the status surface counts it");

    // A wake whose retry is still in the future survives the sweep: the
    // pane may return before the retry comes due.
    bridge.hold_prompt(
        "t1".into(),
        "local".into(),
        "p_sup".into(),
        "late wake".into(),
        "summary".into(),
        Vec::new(),
    );
    assert!(
        bridge
            .expire_unreachable_prompts(&HashSet::new())
            .is_empty(),
        "a backed-off wake is not expired early"
    );
    assert_eq!(bridge.queued_count(), 1);
    assert_eq!(bridge.undeliverable, 1, "the sweep did not double-count");

    // A pane a connected snapshot still shows is never expired, roster or
    // not, so a reconnecting daemon keeps its wakes.
    let item = bridge
        .queued_prompts
        .pop()
        .unwrap_or_else(|| panic!("the held wake is still queued"));
    bridge.requeue_prompt(
        DrainedPrompt {
            team_id: item.team_id,
            host: item.host,
            pane_id: item.pane_id,
            prompt: item.prompt,
            summary: item.summary,
            signals: item.signals,
        },
        Instant::now() - Duration::from_secs(1),
    );
    let live: HashSet<String> = ["p_sup".to_string()].into();
    assert!(
        bridge.expire_unreachable_prompts(&live).is_empty(),
        "a live pane keeps its wake"
    );
    assert_eq!(bridge.queued_count(), 1);
    assert_eq!(bridge.undeliverable, 1);
}

#[test]
fn an_undeliverable_wake_is_requeued_and_retried() {
    let mut bridge = board();
    bridge.paused_members.insert("sup".into());
    queue_wake(&mut bridge);
    bridge.paused_members.clear();

    let item = bridge
        .drain_unpaused_prompts()
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("one held wake"));

    // A future retry time keeps it queued but not offered yet.
    bridge.requeue_prompt(item, Instant::now() + Duration::from_secs(5));
    assert_eq!(bridge.queued_count(), 1, "a failed wake is kept");
    assert!(
        bridge.drain_unpaused_prompts().is_empty(),
        "the retry waits for its delay"
    );

    // A past retry time offers it again.
    let item = bridge
        .queued_prompts
        .pop()
        .unwrap_or_else(|| panic!("the failed wake is still queued"));
    bridge.requeue_prompt(
        DrainedPrompt {
            team_id: item.team_id,
            host: item.host,
            pane_id: item.pane_id,
            prompt: item.prompt,
            summary: item.summary,
            signals: item.signals,
        },
        Instant::now() - Duration::from_secs(1),
    );
    let retried = bridge.drain_unpaused_prompts();
    assert_eq!(retried.len(), 1, "a past retry time releases the wake");
    assert_eq!(retried[0].pane_id, "p_sup");
}

#[test]
fn a_held_wake_waits_for_its_own_team() {
    let mut bridge = board();
    bridge.paused_members.insert("sup".into());
    queue_wake(&mut bridge);

    bridge.selected = Some("t2".into());
    bridge.team = Some(team("t2"));
    bridge.tasks = Vec::new();
    assert_eq!(
        bridge.queued_count(),
        0,
        "the showing team has nothing queued"
    );
    assert!(
        bridge.drain_unpaused_prompts().is_empty(),
        "another team's window does not receive t1's wake"
    );

    bridge.selected = Some("t1".into());
    bridge.team = Some(team("t1"));
    bridge.paused_members.clear();
    assert_eq!(
        bridge.drain_unpaused_prompts().len(),
        1,
        "switching back delivers the wake it was holding"
    );
}

#[test]
fn deleting_a_team_withdraws_the_wake_it_was_holding() {
    let mut bridge = board();
    bridge.paused_members.insert("sup".into());
    queue_wake(&mut bridge);

    bridge.act(
        TeamAction::DeleteTeam {
            team_id: "t1".into(),
        },
        &[],
    );
    bridge.selected = Some("t2".into());
    bridge.team = Some(team("t2"));

    assert!(
        !bridge.supervisor_wakes.contains_key("t1"),
        "a deleted team keeps no wake ledger"
    );
    assert!(
        bridge.drain_unpaused_prompts().is_empty(),
        "a deleted team's held wake is not sent to the team now showing"
    );
    assert_eq!(bridge.queued_count(), 0, "nothing is queued for t2");
}

/// The pause queue is bounded, oldest first: a supervisor that stays paused while
/// its team keeps producing prompts loses its oldest wakes. Each lost wake was
/// already marked as reported, so that result is never announced. The bound is
/// what keeps a silent pane from turning the bridge into a backlog; the miss is
/// filed as a finding, not blessed here.
#[test]
fn held_wakes_are_bounded_and_the_oldest_is_the_one_dropped() {
    let mut bridge = board();
    bridge.paused_members.insert("sup".into());
    for index in 0..PAUSE_QUEUE_CAPACITY {
        bridge.tasks = vec![stored_task(TaskStatus::Done, 10 + index as i64)];
        queue_wake(&mut bridge);
    }
    assert_eq!(bridge.queued_count(), PAUSE_QUEUE_CAPACITY);

    bridge.tasks = vec![stored_task(
        TaskStatus::Done,
        10 + PAUSE_QUEUE_CAPACITY as i64,
    )];
    queue_wake(&mut bridge);
    assert_eq!(
        bridge.queued_count(),
        PAUSE_QUEUE_CAPACITY,
        "the hold stays bounded"
    );

    bridge.paused_members.clear();
    let delivered = bridge.drain_unpaused_prompts();
    assert_eq!(delivered.len(), PAUSE_QUEUE_CAPACITY);
    let prompts: Vec<&str> = delivered.iter().map(|item| item.prompt.as_str()).collect();
    let newest = format!("report {}", 10 + PAUSE_QUEUE_CAPACITY);
    let oldest = "report 10".to_string();
    assert!(
        prompts.iter().any(|prompt| prompt.contains(&newest)),
        "the newest wake survives"
    );
    assert_eq!(
        prompts
            .iter()
            .filter(|prompt| prompt.contains(&oldest))
            .count(),
        0,
        "the oldest held wake is the one the bound dropped"
    );
    assert!(
        prompts
            .iter()
            .all(|prompt| prompt.contains("Supervisor catch-up for team t1")),
        "every held wake stays a catch-up for the team it was queued for"
    );
}
