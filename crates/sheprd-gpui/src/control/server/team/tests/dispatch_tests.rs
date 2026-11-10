use super::*;

fn worker_actor() -> Author {
    Author::Member {
        member_id: "m_w1".into(),
    }
}

fn wake_task(status: TaskStatus) -> Task {
    Task {
        task_id: "task1".into(),
        team_id: "t1".into(),
        creator: Author::Human {
            user_id: "human".into(),
        },
        title: "Write report".into(),
        brief: String::new(),
        assignee: Some("m_w1".into()),
        status,
        blocker_reason: None,
        result: None,
        created_at: at(0),
        updated_at: at(10),
    }
}

fn wake_team() -> Team {
    Team {
        team_id: "t1".into(),
        name: "team1".into(),
        supervisor: Member {
            member_id: "m_sup".into(),
            display_name: "supervisor".into(),
            target: SessionTarget {
                host: "local".into(),
                session: "default".into(),
                workspace: Some("ws".into()),
                pane: Some("p_sup".into()),
            },
        },
        members: vec![Member {
            member_id: "m_w1".into(),
            display_name: "worker1".into(),
            target: SessionTarget {
                host: "local".into(),
                session: "default".into(),
                workspace: Some("ws".into()),
                pane: Some("p_w1".into()),
            },
        }],
    }
}

#[test]
fn task_info_carries_created_and_updated_timestamps() {
    let info = projection::task_info(wake_task(TaskStatus::Open), &wake_team());
    assert_eq!(info.created_at, "1970-01-01T00:00:00.000Z");
    assert_eq!(info.updated_at, "1970-01-01T00:00:10.000Z");
}

#[test]
fn completed_task_wakes_supervisor_with_result_and_signal() {
    let mut task = wake_task(TaskStatus::Done);
    task.result = Some("Report drafted and reviewed".into());
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task,
        actor: worker_actor(),
    });
    assert_eq!(dispatches.len(), 1);
    let dispatch = &dispatches[0];
    assert_eq!(dispatch.team_id, "t1");
    assert_eq!(dispatch.pane_id, "p_sup");
    assert!(dispatch.prompt.contains("completed by worker1"));
    assert!(dispatch.prompt.contains("Report drafted and reviewed"));
    assert!(dispatch.prompt.contains("task list t1"));
    assert!(dispatch.prompt.contains("go idle"));
    assert_eq!(dispatch.signals, vec!["task1|done|10000"]);
}

#[test]
fn blocked_task_wakes_supervisor_with_reason() {
    let mut task = wake_task(TaskStatus::Blocked);
    task.blocker_reason = Some("waiting on credentials".into());
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task,
        actor: worker_actor(),
    });
    assert_eq!(dispatches.len(), 1);
    assert!(dispatches[0].prompt.contains("waiting on credentials"));
    assert!(dispatches[0].prompt.contains("blocked"));
}

#[test]
fn supervisors_own_completion_does_not_wake_itself() {
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::Done),
        actor: Author::Member {
            member_id: "m_sup".into(),
        },
    });
    assert!(dispatches.is_empty());
}

#[test]
fn human_completion_wakes_supervisor() {
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::Done),
        actor: Author::Human {
            user_id: "human".into(),
        },
    });
    assert_eq!(dispatches.len(), 1);
}

#[test]
fn open_task_wakes_the_assignee_with_a_dedupe_signal() {
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::Open),
        actor: worker_actor(),
    });
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0].pane_id, "p_w1");
    assert_eq!(
        dispatches[0].signals,
        vec!["assignee|task1|m_w1|10000"],
        "the wake dedupes on one stored assignment state"
    );
}

#[test]
fn a_reassigned_task_wakes_once_per_assignment_state() {
    // The same stored state redelivered - identical signal, so the wake
    // ledger dedupes it and the assignee is never woken twice for it.
    let first = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::Open),
        actor: worker_actor(),
    });
    let redelivered = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::Open),
        actor: worker_actor(),
    });
    assert_eq!(first[0].signals, redelivered[0].signals);

    // A genuine re-assign is a new state: a new signal, one further wake.
    let mut reassigned = wake_task(TaskStatus::Open);
    reassigned.updated_at = at(11);
    let again = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: reassigned,
        actor: worker_actor(),
    });
    assert_ne!(first[0].signals, again[0].signals);
}

#[test]
fn a_claim_wakes_the_supervisor_with_who_is_working() {
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::InProgress),
        actor: worker_actor(),
    });
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0].pane_id, "p_sup");
    assert!(dispatches[0].prompt.contains("was claimed by worker1"));
    assert!(dispatches[0].prompt.contains("No action is needed"));
    assert_eq!(
        dispatches[0].signals,
        vec!["task1|in_progress|10000"],
        "one stored claimed state, one supervisor wake"
    );
}

#[test]
fn supervisors_own_claim_does_not_wake_itself() {
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task: wake_task(TaskStatus::InProgress),
        actor: Author::Member {
            member_id: "m_sup".into(),
        },
    });
    assert!(dispatches.is_empty());
}

#[test]
fn a_block_by_someone_else_wakes_the_assignee_with_the_reason() {
    let mut task = wake_task(TaskStatus::Blocked);
    task.blocker_reason = Some("waiting on credentials".into());
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task,
        actor: Author::Member {
            member_id: "m_sup".into(),
        },
    });
    // The supervisor recorded the block, so their own write does not wake
    // them; the assignee still learns, because the block came from someone
    // other than them.
    assert_eq!(
        dispatches.len(),
        1,
        "only the assignee wakes about a block the supervisor recorded"
    );
    let assignee = &dispatches[0];
    assert_eq!(assignee.pane_id, "p_w1");
    assert!(assignee.prompt.contains("waiting on credentials"));
    assert!(assignee.prompt.contains("do not poll"));
    assert_eq!(
        assignee.signals,
        vec!["assignee|task1|m_w1|10000"],
        "one stored blocked state, one assignee wake"
    );
}

#[test]
fn an_assignees_own_block_does_not_wake_itself() {
    let mut task = wake_task(TaskStatus::Blocked);
    task.blocker_reason = Some("waiting on credentials".into());
    let dispatches = prompts_for_scoped_reply(&Reply::ScopedTask {
        team: wake_team(),
        task,
        actor: worker_actor(),
    });
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0].pane_id, "p_sup");
}
