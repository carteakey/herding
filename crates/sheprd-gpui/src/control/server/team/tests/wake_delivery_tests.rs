use super::*;

/// A done task completed by its worker on a team whose supervisor pane is
/// `p_sup`, which the fixture window's daemon does not show.
fn supervisor_wake_team() -> Team {
    Team {
        team_id: "t-x".into(),
        name: "wake-test".into(),
        supervisor: Member {
            member_id: "m_sup".into(),
            display_name: "supervisor".into(),
            target: SessionTarget {
                host: "local".into(),
                session: "default".into(),
                workspace: Some("w1X".into()),
                pane: Some("p_sup".into()),
            },
        },
        members: vec![Member {
            member_id: "m_w1".into(),
            display_name: "worker1".into(),
            target: SessionTarget {
                host: "local".into(),
                session: "default".into(),
                workspace: Some("w1X".into()),
                pane: Some("w1X:p9".into()),
            },
        }],
    }
}

fn supervisor_wake_reply() -> Reply {
    Reply::ScopedTask {
        team: supervisor_wake_team(),
        task: Task {
            task_id: "task9".into(),
            team_id: "t-x".into(),
            creator: Author::Human {
                user_id: "human".into(),
            },
            title: "Write report".into(),
            brief: String::new(),
            assignee: Some("m_w1".into()),
            status: TaskStatus::Done,
            blocker_reason: None,
            result: Some("Report drafted".into()),
            created_at: at(0),
            updated_at: at(10),
        },
        actor: Author::Member {
            member_id: "m_w1".into(),
        },
    }
}

#[gpui::test]
fn supervisor_wake_on_unreachable_pane_stays_pending_and_honest(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    window
        .update(cx, |view, _, _| {
            view.live.status = ConnectionStatus::Connected;
            view.live.snapshot = Some(Arc::new(snapshot(
                "boot-local",
                "w1X",
                "w1X:p9",
                Some("opencode"),
            )));
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            view.endpoints[0].live.snapshot = view.live.snapshot.clone();
        })
        .expect("window open");

    cx.update(|app| notify_team_mutation(&supervisor_wake_reply(), app));

    let signal = "task9|done|10000";
    window
        .update(cx, |view, _, _| {
            let activity = view
                .team_bridge
                .snapshot(&[])
                .last_activity
                .expect("the wake attempt is recorded");
            assert!(
                activity.contains("Held") && activity.contains("queued"),
                "an unreachable pane keeps the wake queued: {activity}"
            );
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                1,
                "the wake waits for a reachable pane"
            );
            // Fail closed: nothing was marked woken, so the fallback or a
            // later event can still deliver the result.
            assert!(!view.team_bridge.wake_seen("t-x", signal));

            // The event is not repeated once a wake is known to have landed.
            view.team_bridge.mark_supervisor_woken("t-x", [signal]);
        })
        .expect("window open");

    let before = window
        .update(cx, |view, _, _| {
            view.team_bridge.snapshot(&[]).last_activity
        })
        .expect("window open");
    cx.update(|app| notify_team_mutation(&supervisor_wake_reply(), app));
    window
        .update(cx, |view, _, _| {
            assert_eq!(
                view.team_bridge.snapshot(&[]).last_activity,
                before,
                "a seen signal dispatches nothing"
            );
        })
        .expect("window open");
}

#[gpui::test]
fn a_held_wake_is_recorded_only_after_it_is_delivered(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    let reply = supervisor_wake_reply();
    let signal = "task9|done|10000";
    window
        .update(cx, |view, _, _| {
            view.live.status = ConnectionStatus::Connected;
            view.live.snapshot = Some(Arc::new(snapshot_with_status(
                "boot-local",
                "w1X",
                "p_sup",
                Some("opencode"),
                "idle",
            )));
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            view.endpoints[0].live.snapshot = view.live.snapshot.clone();
            view.team_bridge.set_team_for_test(supervisor_wake_team());
            view.team_bridge.pause_member_for_test("m_sup");
        })
        .expect("window open");

    cx.update(|app| notify_team_mutation(&reply, app));
    window
        .update(cx, |view, _, _| {
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                1,
                "the paused supervisor's wake is held"
            );
            assert!(
                !view.team_bridge.wake_seen("t-x", signal),
                "a held wake is not recorded as delivered"
            );
        })
        .expect("window open");

    // The pause clears; once the wake's retry waits out, the drain delivers
    // to the reachable supervisor pane and only then records the signal.
    window
        .update(cx, |view, _, cx| {
            view.team_bridge.unpause_all_for_test();
            view.team_bridge.force_retry_due_for_test();
            view.dispatch_drained_team_prompts(cx);
        })
        .expect("window open");
    window
        .update(cx, |view, _, _| {
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                0,
                "the held wake is released once"
            );
            assert!(
                view.team_bridge.wake_seen("t-x", signal),
                "a delivered wake is recorded"
            );
        })
        .expect("window open");

    // A repeated event and a further tick deliver nothing further.
    cx.update(|app| notify_team_mutation(&reply, app));
    window
        .update(cx, |view, _, cx| {
            view.dispatch_drained_team_prompts(cx);
            assert_eq!(view.team_bridge.queued_count_for_test(), 0);
        })
        .expect("window open");
}

#[gpui::test]
fn a_wake_waits_for_a_busy_agent_to_finish_its_turn(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    let reply = supervisor_wake_reply();
    let signal = "task9|done|10000";
    window
        .update(cx, |view, _, _| {
            view.live.status = ConnectionStatus::Connected;
            view.live.snapshot = Some(Arc::new(snapshot(
                "boot-local",
                "w1X",
                "p_sup",
                Some("opencode"),
            )));
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            view.endpoints[0].live.snapshot = view.live.snapshot.clone();
            view.team_bridge.set_team_for_test(supervisor_wake_team());
            view.team_bridge.pause_member_for_test("m_sup");
        })
        .expect("window open");

    cx.update(|app| notify_team_mutation(&reply, app));
    window
        .update(cx, |view, _, cx| {
            view.team_bridge.unpause_all_for_test();
            // The agent is mid-turn: the wake must wait, not paste into an
            // input the turn will clear.
            view.dispatch_drained_team_prompts(cx);
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                1,
                "a busy agent keeps the wake queued"
            );
            assert!(
                !view.team_bridge.wake_seen("t-x", signal),
                "a wake that has not landed is not recorded"
            );
        })
        .expect("window open");

    window
        .update(cx, |view, _, cx| {
            view.live.snapshot = Some(Arc::new(snapshot_with_status(
                "boot-local",
                "w1X",
                "p_sup",
                Some("opencode"),
                "idle",
            )));
            view.endpoints[0].live.snapshot = view.live.snapshot.clone();
            // The retry is due; the idle pane takes it now.
            view.team_bridge.force_retry_due_for_test();
            view.dispatch_drained_team_prompts(cx);
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                0,
                "the idle agent takes the wake"
            );
            assert!(
                view.team_bridge.wake_seen("t-x", signal),
                "the delivered wake is recorded"
            );
        })
        .expect("window open");
}

#[gpui::test]
fn a_wake_to_a_busy_pane_is_held_until_it_is_idle(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(fixture_window);
    let reply = supervisor_wake_reply();
    let signal = "task9|done|10000";
    window
        .update(cx, |view, _, _| {
            view.live.status = ConnectionStatus::Connected;
            view.live.snapshot = Some(Arc::new(snapshot(
                "boot-local",
                "w1X",
                "p_sup",
                Some("opencode"),
            )));
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            view.endpoints[0].live.snapshot = view.live.snapshot.clone();
            view.team_bridge.set_team_for_test(supervisor_wake_team());
        })
        .expect("window open");

    // The event path must hold for a busy pane too, not only the drain.
    cx.update(|app| notify_team_mutation(&reply, app));
    window
        .update(cx, |view, _, _| {
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                1,
                "a busy pane holds the event wake"
            );
            assert!(!view.team_bridge.wake_seen("t-x", signal));
        })
        .expect("window open");

    window
        .update(cx, |view, _, cx| {
            view.live.snapshot = Some(Arc::new(snapshot_with_status(
                "boot-local",
                "w1X",
                "p_sup",
                Some("opencode"),
                "idle",
            )));
            view.endpoints[0].live.snapshot = view.live.snapshot.clone();
            view.team_bridge.force_retry_due_for_test();
            view.dispatch_drained_team_prompts(cx);
            assert_eq!(view.team_bridge.queued_count_for_test(), 0);
            assert!(view.team_bridge.wake_seen("t-x", signal));
        })
        .expect("window open");
}

#[gpui::test]
fn a_wake_to_a_gone_pane_waits_out_its_retry_then_is_marked_undeliverable(
    cx: &mut gpui::TestAppContext,
) {
    let window = cx.add_window(fixture_window);
    let signal = "task9|done|10000";
    window
        .update(cx, |view, _, _| {
            view.live.status = ConnectionStatus::Connected;
            // The snapshot shows only the worker pane; the supervisor pane
            // the wake targets is gone - the stale-queue shape that once
            // hot-looped the drain.
            view.live.snapshot = Some(Arc::new(snapshot(
                "boot-local",
                "w1X",
                "w1X:p9",
                Some("opencode"),
            )));
            view.endpoints[0].live.status = ConnectionStatus::Connected;
            view.endpoints[0].live.snapshot = view.live.snapshot.clone();
            view.team_bridge.set_team_for_test(supervisor_wake_team());
        })
        .expect("window open");

    cx.update(|app| notify_team_mutation(&supervisor_wake_reply(), app));
    window
        .update(cx, |view, _, cx| {
            view.dispatch_drained_team_prompts(cx);
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                1,
                "a missing pane does not re-dispatch the wake on the next tick"
            );
            assert_eq!(
                view.team_bridge.snapshot(&[]).undeliverable_count,
                0,
                "the wake has not waited out its retry yet"
            );
            assert!(
                !view.team_bridge.wake_seen("t-x", signal),
                "a wake that has not landed is not recorded"
            );
        })
        .expect("window open");

    // The roster re-binds elsewhere - the members re-registered, the way the
    // stale wA queue entries lost their panes - so when the retry comes due
    // the pane is in neither the roster nor any snapshot.
    window
        .update(cx, |view, _, _| {
            let mut rebound = supervisor_wake_team();
            rebound.supervisor.target.pane = Some("w1X:p9".into());
            view.team_bridge.set_team_for_test(rebound);
        })
        .expect("window open");
    window
        .update(cx, |view, _, cx| {
            view.team_bridge.force_retry_due_for_test();
            view.dispatch_drained_team_prompts(cx);
            assert_eq!(
                view.team_bridge.queued_count_for_test(),
                0,
                "a wake whose pane is gone is not retried forever"
            );
            assert_eq!(
                view.team_bridge.snapshot(&[]).undeliverable_count,
                1,
                "the drain marks the stranded wake undeliverable"
            );
            let activity = view
                .team_bridge
                .snapshot(&[])
                .last_activity
                .expect("the expiry is recorded");
            assert!(
                activity.contains("undeliverable") && activity.contains("p_sup"),
                "the status names the pane it gave up on: {activity}"
            );
            assert!(
                !view.team_bridge.wake_seen("t-x", signal),
                "an undeliverable wake is never recorded as delivered"
            );
        })
        .expect("window open");
}
