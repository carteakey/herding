//! Scoped request handling: the caller-pinned operations a registered
//! member runs from a shell, each resolved against the caller's own team
//! membership before the store sees it.
use super::*;
use sheprd_orchestrator::team_store::IssueWatch;

pub(super) fn scoped(
    store: &mut TeamStore,
    team_id: &str,
    caller: &SessionTarget,
    action: ScopedAction,
    live_hosts: &[String],
) -> Result<Reply, Error> {
    if let ScopedAction::Register { name } = action {
        let member_id = uuid::Uuid::new_v4().to_string();
        let member = store.register_member(team_id, &member_id, &name, caller, live_hosts)?;
        let team = store.team(team_id)?.ok_or(Error::NotMember)?;
        return Ok(Reply::ScopedRegistered { team, member });
    }
    if let ScopedAction::Status { team_id: hint } = action {
        let filter = hint
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .or_else(|| (!team_id.trim().is_empty()).then_some(team_id));
        let (team, member) = store
            .find_member_team(filter, caller)?
            .ok_or(Error::NotMember)?;
        let goal = store.team_goal(&team.team_id)?;
        // One COUNT over the mention table instead of scanning every post
        // with a statuses query per post.
        let pending_mentions = store.pending_mention_count(&team.team_id, &member.member_id)?;
        // Counts rather than a page of tasks: a team with more tasks than one
        // page would report its oldest 100 as its workload.
        let counts = store.task_counts(&team.team_id, &member.member_id)?;
        return Ok(Reply::ScopedStatus {
            team,
            member,
            goal,
            pending_mentions,
            open_tasks: counts.open,
            assigned_tasks: counts.assigned,
        });
    }
    let team = store.team(team_id)?.ok_or(Error::NotMember)?;
    let member_id = std::iter::once(&team.supervisor)
        .chain(team.members.iter())
        .find(|member| member.target == *caller)
        .map(|member| member.member_id.clone())
        .ok_or(Error::NotMember)?;
    let actor = Author::Member {
        member_id: member_id.clone(),
    };
    Ok(match action {
        ScopedAction::Status { .. } | ScopedAction::Register { .. } => unreachable!(),
        ScopedAction::Roster => Reply::ScopedRoster { team },
        ScopedAction::GoalSet { goal } => {
            store.set_team_goal(team_id, &goal)?;
            Reply::ScopedGoalUpdated { team, goal }
        }
        ScopedAction::BulletinList { after, limit } => {
            // The default read is the newest page so posts past the page
            // bound stay visible; `--after` pages forward from a cursor.
            let page = match after {
                Some(cursor) => store.posts(team_id, cursor, limit)?,
                None => store.latest_posts(team_id, None, limit)?,
            };
            let next = if page.len() == limit {
                page.last().map(|(sequence, _)| *sequence)
            } else {
                None
            };
            let ids: Vec<String> = page.iter().map(|(_, post)| post.post_id.clone()).collect();
            let statuses = store.recipient_statuses_for_posts(&ids)?;
            let mut entries = Vec::new();
            for (sequence, post) in page {
                let status = statuses
                    .get(&post.post_id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .find(|(id, _)| *id == member_id)
                    .map(|(_, status)| status);
                entries.push((sequence, post, status));
            }
            Reply::ScopedBulletin {
                team,
                member_id,
                entries,
                next,
            }
        }
        ScopedAction::BulletinPost {
            post_id,
            reply_to,
            text,
            mentions,
        } => {
            let reply_to = reply_to
                .as_deref()
                .map(|raw| store.resolve_post_ref(team_id, raw))
                .transpose()?;
            let recipients = team_store::resolve_recipients(&team, &actor, &mentions, &text)?;
            let post = store.add_post_with_recipients(
                &post_id,
                team_id,
                &actor,
                reply_to.as_deref(),
                &text,
                &recipients,
            )?;
            Reply::ScopedPosted { team, post }
        }
        ScopedAction::BulletinAck { message_id } => {
            let message_id = store.resolve_post_ref(team_id, &message_id)?;
            Reply::ScopedAcknowledged {
                acknowledgement: store.acknowledge(&message_id, &member_id)?,
            }
        }
        ScopedAction::TaskList { before, limit } => {
            let page = store.latest_tasks(team_id, before, limit)?;
            let older = page_cursor(&page, limit);
            Reply::ScopedTasks {
                team,
                tasks: page,
                older,
            }
        }
        ScopedAction::TaskCreate {
            task_id,
            title,
            brief,
            assignee,
        } => {
            let assignee_id = assignee
                .as_deref()
                .map(|raw| team_store::resolve_member_ref(&team, raw))
                .transpose()?;
            let task = store.create_task(
                &task_id,
                team_id,
                &actor,
                &title,
                &brief,
                assignee_id.as_deref(),
            )?;
            Reply::ScopedTask {
                team,
                task,
                actor: actor.clone(),
            }
        }
        ScopedAction::TaskAssign { task_id, assignee } => {
            let task_id = store.resolve_task_ref(team_id, &task_id)?;
            let assignee_id = team_store::resolve_member_ref(&team, &assignee)?;
            let task = store.assign_task(team_id, &task_id, &actor, &assignee_id)?;
            Reply::ScopedTask {
                team,
                task,
                actor: actor.clone(),
            }
        }
        ScopedAction::TaskClaim { task_id } => {
            let task_id = store.resolve_task_ref(team_id, &task_id)?;
            let task = store.claim_task(team_id, &task_id, &member_id)?;
            Reply::ScopedTask {
                team,
                task,
                actor: actor.clone(),
            }
        }
        ScopedAction::TaskBlock { task_id, reason } => {
            let task_id = store.resolve_task_ref(team_id, &task_id)?;
            let task = store.block_task(team_id, &task_id, &actor, &reason)?;
            Reply::ScopedTask {
                team,
                task,
                actor: actor.clone(),
            }
        }
        ScopedAction::TaskComplete { task_id, result } => {
            let task_id = store.resolve_task_ref(team_id, &task_id)?;
            let task = store.complete_task(team_id, &task_id, &actor, &result)?;
            Reply::ScopedTask {
                team,
                task,
                actor: actor.clone(),
            }
        }
        ScopedAction::TaskCancel { task_id } => {
            let task_id = store.resolve_task_ref(team_id, &task_id)?;
            let task = store.cancel_task(team_id, &task_id, &actor)?;
            Reply::ScopedTask {
                team,
                task,
                actor: actor.clone(),
            }
        }
        ScopedAction::FindingList { before, limit } => {
            let page = store.latest_findings(team_id, before, limit)?;
            let older = page_cursor(&page, limit);
            Reply::ScopedFindings {
                team,
                findings: page,
                older,
            }
        }
        ScopedAction::FindingAdd {
            finding_id,
            task_id,
            description,
            evidence,
            confidence,
            supersedes,
        } => {
            let task_id = task_id
                .as_deref()
                .map(|raw| store.resolve_task_ref(team_id, raw))
                .transpose()?;
            let supersedes = supersedes
                .as_deref()
                .map(|raw| store.resolve_finding_ref(team_id, raw))
                .transpose()?;
            let finding = store.add_finding(
                &finding_id,
                team_id,
                &actor,
                task_id.as_deref(),
                &description,
                &evidence,
                confidence,
                supersedes.as_deref(),
            )?;
            Reply::ScopedFinding { team, finding }
        }
        ScopedAction::Heartbeat { member_id } => Reply::ScopedHeartbeat {
            team,
            target_member_id: member_id,
        },
        ScopedAction::IssueWatch { repo, numbers } => {
            let watches = store.watch_issues(team_id, &actor, &repo, &numbers)?;
            Reply::IssueWatches {
                watches: watches
                    .into_iter()
                    .map(|IssueWatch { repo, number }| (repo, number))
                    .collect(),
            }
        }
        ScopedAction::IssueUnwatch { repo, numbers } => {
            let watches = store.unwatch_issues(team_id, &repo, &numbers)?;
            Reply::IssueWatches {
                watches: watches
                    .into_iter()
                    .map(|IssueWatch { repo, number }| (repo, number))
                    .collect(),
            }
        }
        ScopedAction::IssueWatches => {
            let watches = store
                .issue_watches(team_id)?
                .into_iter()
                .map(|IssueWatch { repo, number }| (repo, number))
                .collect();
            Reply::IssueWatches { watches }
        }
    })
}
