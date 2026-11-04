//! A supervisor wake is only worth marking as reported once it can reach the
//! supervisor. These pin what happens to a wake that is held - because the
//! supervisor's pane or the whole team is paused, or because another team is
//! showing - and that it arrives exactly once.
use super::*;
use herdr_orchestrator::team_store::{Author, Member, SessionTarget};

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

/// The window's own wake step: ask the bridge, mark the result as reported, and
/// queue the prompt for the supervisor's pane.
fn queue_wake(bridge: &mut Bridge) -> SupervisorWake {
    bridge.last_fallback = None;
    let wake = match bridge.supervisor_fallback_due(Instant::now()) {
        Some(wake) => wake,
        None => panic!("expected a supervisor wake"),
    };
    bridge.mark_supervisor_woken(&wake.team_id, wake.signals.clone());
    bridge.enqueue_prompt(
        wake.team_id.clone(),
        wake.pane_id.clone(),
        wake.prompt.clone(),
        wake.summary.clone(),
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
        bridge.drain_unpaused_prompts().is_empty(),
        "a paused supervisor pane is not woken"
    );

    bridge.paused_members.clear();
    let delivered = bridge.drain_unpaused_prompts();
    assert_eq!(delivered.len(), 1, "unpausing delivers the held wake");
    let (pane, prompt, summary) = &delivered[0];
    assert_eq!(pane, "p_sup");
    assert!(
        prompt.contains("report 10") && summary.contains("unseen"),
        "the wake names the result it is about: {prompt:?} / {summary:?}"
    );
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
        "the held wake already counted as reported, so pausing does not re-offer it"
    );

    bridge.paused_teams.clear();
    let delivered = bridge.drain_unpaused_prompts();
    assert_eq!(delivered.len(), 1, "the team's wake waits out its pause");
    assert_eq!(delivered[0].0, wake.pane_id);
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
    let prompts: Vec<&str> = delivered
        .iter()
        .map(|(_, prompt, _)| prompt.as_str())
        .collect();
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
