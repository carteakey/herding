//! Bounded, serialized access to the durable team store.
//!
//! Construct this worker off the GPUI thread so the SQLite file is opened on
//! its worker thread. Register the returned response receiver with the
//! server's `Watches` before calling `submit`. Dropping the response receiver
//! after an accepted mutation does not cancel that mutation; the caller's
//! outcome is unknown until it reads the reply.
use sheprd_orchestrator::team_store::{
    self, Acknowledgement, Author, Confidence, Finding, HostContext, Member, Post, RecipientStatus,
    Recipients, SessionTarget, Task, Team, TeamStore,
};
use std::{
    path::PathBuf,
    sync::{
        Arc, mpsc,
        mpsc::{Receiver, SyncSender, TrySendError},
    },
    thread,
};
use thiserror::Error as ThisError;

const QUEUE_CAPACITY: usize = 16;

pub(crate) enum Operation {
    SaveTeam(Team),
    DeleteTeam {
        team_id: String,
    },
    RemoveMember {
        team_id: String,
        member_id: String,
    },
    SetGoal {
        team_id: String,
        goal: String,
    },
    ListTeams {
        after: Option<String>,
        limit: usize,
    },
    AddPost {
        post_id: String,
        team_id: String,
        author: Author,
        reply_to: Option<String>,
        body: String,
        recipients: Recipients,
    },
    CreateTask {
        task_id: String,
        team_id: String,
        creator: Author,
        title: String,
        brief: String,
        assignee: Option<String>,
    },
    UpdateTask {
        task_id: String,
        team_id: String,
        actor: Author,
        title: String,
        brief: String,
        assignee: Option<String>,
    },
    CompleteTask {
        task_id: String,
        team_id: String,
        actor: Author,
        result: String,
    },
    CancelTask {
        task_id: String,
        team_id: String,
        actor: Author,
    },
    ImportGithubIssue {
        task_id: String,
        team_id: String,
        creator: Author,
        issue_ref: String,
        as_goal: bool,
    },
    AddFinding {
        finding_id: String,
        team_id: String,
        author: Author,
        task_id: Option<String>,
        description: String,
        evidence: String,
        confidence: Confidence,
        supersedes_finding_id: Option<String>,
    },
    /// The Team view's read: a team with its newest posts and delivery states.
    Snapshot {
        team_id: String,
        limit: usize,
    },
    /// The Team view's older task page, behind the board's show-earlier
    /// control. A plain operation because the view has no agent session to
    /// prove; the board cursor is the store's `latest_tasks` shape.
    TaskPage {
        team_id: String,
        before: i64,
        limit: usize,
    },
    /// The Team view's older finding page, same shape as [`Operation::TaskPage`].
    FindingPage {
        team_id: String,
        before: i64,
        limit: usize,
    },
    /// An agent call. The caller's freshly proven target must match a stored
    /// member's target; the worker never trusts a member ID from the wire.
    /// `caller_boot` is the boot the caller's endpoint currently reports, so
    /// a member recorded under that boot (a pre-stable-key row) still
    /// matches and migrates to the stable id. `endpoint_ids` names every
    /// known endpoint's stable id, connected or not, so migration never
    /// adopts a row that names a merely disconnected machine. `live_panes`
    /// names the panes that currently hold a live agent session, so a
    /// same-host re-bind crosses panes only when the holder's pane is gone.
    Scoped {
        team_id: String,
        caller: SessionTarget,
        caller_boot: String,
        endpoint_ids: Vec<String>,
        live_panes: Vec<String>,
        action: ScopedAction,
        live_hosts: Vec<String>,
    },
}

pub(crate) enum ScopedAction {
    Status {
        team_id: Option<String>,
    },
    Register {
        name: String,
    },
    Roster,
    GoalSet {
        goal: String,
    },
    BulletinList {
        /// `None` reads the newest page; `Some(cursor)` pages forward from it.
        after: Option<i64>,
        limit: usize,
    },
    BulletinPost {
        post_id: String,
        reply_to: Option<String>,
        text: String,
        mentions: Vec<String>,
    },
    BulletinAck {
        message_id: String,
    },
    TaskList {
        /// Sequence to page back from; `None` starts at the newest page.
        before: Option<i64>,
        limit: usize,
    },
    TaskCreate {
        task_id: String,
        title: String,
        brief: String,
        assignee: Option<String>,
    },
    TaskAssign {
        task_id: String,
        assignee: String,
    },
    TaskClaim {
        task_id: String,
    },
    TaskBlock {
        task_id: String,
        reason: String,
    },
    TaskComplete {
        task_id: String,
        result: String,
    },
    TaskCancel {
        task_id: String,
    },
    FindingList {
        /// Sequence to page back from; `None` starts at the newest page.
        before: Option<i64>,
        limit: usize,
    },
    FindingAdd {
        finding_id: String,
        task_id: Option<String>,
        description: String,
        evidence: String,
        confidence: Confidence,
        supersedes: Option<String>,
    },
    Heartbeat {
        member_id: Option<String>,
    },
}

/// A post with each recipient's delivery state.
pub(crate) type PostDelivery = (Post, Vec<(String, RecipientStatus)>);

pub(crate) enum Reply {
    Snapshot {
        team: Option<Team>,
        goal: Option<String>,
        posts: Vec<PostDelivery>,
        tasks: Vec<Task>,
        findings: Vec<Finding>,
        /// The board sequence to page back from when the task and finding
        /// pages came back full; `None` means the board is fully loaded.
        task_older: Option<i64>,
        finding_older: Option<i64>,
    },
    ScopedStatus {
        team: Team,
        member: Member,
        goal: Option<String>,
        pending_mentions: usize,
        open_tasks: usize,
        assigned_tasks: usize,
    },
    ScopedRegistered {
        team: Team,
        member: Member,
    },
    ScopedRoster {
        team: Team,
    },
    ScopedGoalUpdated {
        team: Team,
        goal: String,
    },
    ScopedBulletin {
        team: Team,
        member_id: String,
        entries: Vec<(i64, Post, Option<RecipientStatus>)>,
        /// Pass as `after` for the next page when this one came back full.
        next: Option<i64>,
    },
    ScopedPosted {
        team: Team,
        post: Post,
    },
    ScopedAcknowledged {
        acknowledgement: Acknowledgement,
    },
    ScopedTasks {
        team: Team,
        /// The requested page, oldest first, each with its board sequence;
        /// the first entry is the caller's next `before` cursor.
        tasks: Vec<(i64, Task)>,
        /// The page to request next when this one came back full.
        older: Option<i64>,
    },
    ScopedTask {
        team: Team,
        task: Task,
        actor: Author,
    },
    ScopedFindings {
        team: Team,
        /// The requested page, oldest first, each with its sequence;
        /// the first entry is the caller's next `before` cursor.
        findings: Vec<(i64, Finding)>,
        /// The page to request next when this one came back full.
        older: Option<i64>,
    },
    ScopedFinding {
        team: Team,
        finding: Finding,
    },
    ScopedHeartbeat {
        team: Team,
        target_member_id: Option<String>,
    },
    Saved,
    Teams(Vec<Team>),
    TaskPage {
        tasks: Vec<(i64, Task)>,
        older: Option<i64>,
    },
    FindingPage {
        findings: Vec<(i64, Finding)>,
        older: Option<i64>,
    },
}

#[derive(Debug, ThisError)]
pub(crate) enum Error {
    #[error("The team worker queue is full")]
    Busy,
    #[error("The team response watch was not registered; no operation was queued")]
    WatchUnavailable,
    #[error("The team worker has stopped")]
    Stopped,
    #[error("The proven caller session is not a member of this team")]
    NotMember,
    #[error("{0}")]
    External(String),
    #[error("The team store failed")]
    Store(#[source] Arc<team_store::Error>),
}

impl From<team_store::Error> for Error {
    fn from(error: team_store::Error) -> Self {
        Self::Store(Arc::new(error))
    }
}

struct Job {
    operation: Operation,
    response: SyncSender<Result<Reply, Error>>,
}

/// One connection, used serially on one worker thread. Dropping this value is
/// nonblocking: accepted work drains before the closed queue exits. Call
/// `finish` only off the UI thread when synchronous shutdown is needed.
pub(crate) struct Worker {
    sender: SyncSender<Job>,
    /// Kept only so tests can join the worker; production drops it to detach.
    #[cfg(test)]
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    pub(crate) fn start(path: PathBuf) -> std::io::Result<Self> {
        #[cfg(test)]
        {
            Self::spawn(path, None, None)
        }
        #[cfg(not(test))]
        {
            Self::spawn(path)
        }
    }

    /// Registers a one-slot response watch before queueing. `register_watch`
    /// must return true only when the receiver is retained by a live watch.
    /// Queue saturation is delivered through that already-registered watch.
    pub(crate) fn submit(
        &self,
        operation: Operation,
        register_watch: impl FnOnce(Receiver<Result<Reply, Error>>) -> bool,
    ) -> Result<(), Error> {
        let (response, answer) = mpsc::sync_channel(1);
        if !register_watch(answer) {
            return Err(Error::WatchUnavailable);
        }
        let job = Job {
            operation,
            response,
        };
        match self.sender.try_send(job) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(job)) => {
                let _ = job.response.try_send(Err(Error::Busy));
                Err(Error::Busy)
            }
            Err(TrySendError::Disconnected(job)) => {
                let _ = job.response.try_send(Err(Error::Stopped));
                Err(Error::Stopped)
            }
        }
    }

    /// Joins after closing the queue.
    #[cfg(test)]
    pub(crate) fn finish(self) {
        let Self { sender, thread } = self;
        drop(sender);
        if let Some(thread) = thread
            && thread.join().is_err()
        {
            tracing::warn!("Team worker panicked");
        }
    }

    fn spawn(
        path: PathBuf,
        #[cfg(test)] gate: Option<Receiver<()>>,
        #[cfg(test)] stopped: Option<SyncSender<()>>,
    ) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let _thread = thread::Builder::new()
            .name("herdr-team-store".into())
            .spawn(move || {
                #[cfg(test)]
                if let Some(gate) = gate {
                    let _ = gate.recv();
                }
                run(path, receiver);
                #[cfg(test)]
                if let Some(stopped) = stopped {
                    let _ = stopped.send(());
                }
            })?;
        Ok(Self {
            sender,
            #[cfg(test)]
            thread: Some(_thread),
        })
    }
}

fn run(path: PathBuf, queue: Receiver<Job>) {
    let mut store = TeamStore::open(&path).map_err(Arc::new);
    for job in queue {
        let mutation = matches!(
            &job.operation,
            Operation::SaveTeam(_)
                | Operation::DeleteTeam { .. }
                | Operation::RemoveMember { .. }
                | Operation::SetGoal { .. }
                | Operation::AddPost { .. }
                | Operation::CreateTask { .. }
                | Operation::UpdateTask { .. }
                | Operation::CompleteTask { .. }
                | Operation::CancelTask { .. }
                | Operation::ImportGithubIssue { .. }
                | Operation::AddFinding { .. }
                | Operation::Scoped {
                    action: ScopedAction::Register { .. }
                        | ScopedAction::GoalSet { .. }
                        | ScopedAction::BulletinPost { .. }
                        | ScopedAction::BulletinAck { .. }
                        | ScopedAction::TaskCreate { .. }
                        | ScopedAction::TaskAssign { .. }
                        | ScopedAction::TaskClaim { .. }
                        | ScopedAction::TaskBlock { .. }
                        | ScopedAction::TaskComplete { .. }
                        | ScopedAction::TaskCancel { .. }
                        | ScopedAction::FindingAdd { .. },
                    ..
                }
        );
        let result = match store.as_mut() {
            Ok(store) => execute(store, job.operation),
            Err(error) => Err(Error::Store(error.clone())),
        };
        if job.response.send(result).is_err() && mutation {
            tracing::warn!(
                "Accepted team mutation completed without a delivered response; caller outcome is unknown"
            );
        }
    }
}

fn execute(store: &mut TeamStore, operation: Operation) -> Result<Reply, Error> {
    let reply = match operation {
        Operation::SaveTeam(team) => {
            store.save_team(&team)?;
            Reply::Saved
        }
        Operation::DeleteTeam { team_id } => {
            store.delete_team(&team_id)?;
            Reply::Saved
        }
        Operation::RemoveMember { team_id, member_id } => {
            store.remove_member(&team_id, &member_id)?;
            Reply::Saved
        }
        Operation::SetGoal { team_id, goal } => {
            store.set_team_goal(&team_id, &goal)?;
            Reply::Saved
        }
        Operation::ListTeams { after, limit } => {
            Reply::Teams(store.list_teams(after.as_deref(), limit)?)
        }
        Operation::AddPost {
            post_id,
            team_id,
            author,
            reply_to,
            body,
            recipients,
        } => {
            store.add_post_with_recipients(
                &post_id,
                &team_id,
                &author,
                reply_to.as_deref(),
                &body,
                &recipients,
            )?;
            Reply::Saved
        }
        Operation::CreateTask {
            task_id,
            team_id,
            creator,
            title,
            brief,
            assignee,
        } => {
            store.create_task(
                &task_id,
                &team_id,
                &creator,
                &title,
                &brief,
                assignee.as_deref(),
            )?;
            Reply::Saved
        }
        Operation::UpdateTask {
            task_id,
            team_id,
            actor,
            title,
            brief,
            assignee,
        } => {
            store.update_task(
                &team_id,
                &task_id,
                &actor,
                &title,
                &brief,
                assignee.as_deref(),
            )?;
            Reply::Saved
        }
        Operation::CompleteTask {
            task_id,
            team_id,
            actor,
            result,
        } => {
            store.complete_task(&team_id, &task_id, &actor, &result)?;
            Reply::Saved
        }
        Operation::CancelTask {
            task_id,
            team_id,
            actor,
        } => {
            store.cancel_task(&team_id, &task_id, &actor)?;
            Reply::Saved
        }
        Operation::ImportGithubIssue {
            task_id,
            team_id,
            creator,
            issue_ref,
            as_goal,
        } => {
            github_import::import_github_issue(
                store, &team_id, &task_id, &creator, &issue_ref, as_goal,
            )?;
            Reply::Saved
        }
        Operation::AddFinding {
            finding_id,
            team_id,
            author,
            task_id,
            description,
            evidence,
            confidence,
            supersedes_finding_id,
        } => {
            store.add_finding(
                &finding_id,
                &team_id,
                &author,
                task_id.as_deref(),
                &description,
                &evidence,
                confidence,
                supersedes_finding_id.as_deref(),
            )?;
            Reply::Saved
        }
        Operation::Snapshot { team_id, limit } => snapshot(store, &team_id, limit)?,
        Operation::TaskPage {
            team_id,
            before,
            limit,
        } => {
            let page = store.latest_tasks(&team_id, Some(before), limit)?;
            let older = page_cursor(&page, limit);
            Reply::TaskPage { tasks: page, older }
        }
        Operation::FindingPage {
            team_id,
            before,
            limit,
        } => {
            let page = store.latest_findings(&team_id, Some(before), limit)?;
            let older = page_cursor(&page, limit);
            Reply::FindingPage {
                findings: page,
                older,
            }
        }
        Operation::Scoped {
            team_id,
            caller,
            caller_boot,
            endpoint_ids,
            live_panes,
            action,
            live_hosts,
        } => scoped(
            store,
            &team_id,
            &caller,
            &caller_boot,
            &endpoint_ids,
            &live_panes,
            action,
            &live_hosts,
        )?,
    };
    Ok(reply)
}

/// A snapshot shows its newest `limit` posts, read as page-sized slices from
/// the newest end, so the refresh pays for its page and not for how much
/// history the team has accumulated.
fn snapshot(store: &mut TeamStore, team_id: &str, limit: usize) -> Result<Reply, Error> {
    let Some(team) = store.team(team_id)? else {
        return Ok(Reply::Snapshot {
            team: None,
            goal: None,
            posts: Vec::new(),
            tasks: Vec::new(),
            findings: Vec::new(),
            task_older: None,
            finding_older: None,
        });
    };
    let goal = store.team_goal(team_id)?;
    let mut pages: Vec<Vec<(i64, Post)>> = Vec::new();
    let mut remaining = limit;
    while remaining > 0 {
        let want = remaining.min(team_store::MAX_PAGE_SIZE);
        let before = pages
            .last()
            .and_then(|page| page.first())
            .map(|(sequence, _)| *sequence);
        let page = store.latest_posts(team_id, before, want)?;
        let got = page.len();
        remaining = remaining.saturating_sub(got);
        pages.push(page);
        // A short page is the oldest end of the team: there is nothing older.
        if got < want {
            break;
        }
    }
    let returned: Vec<(i64, Post)> = pages.into_iter().rev().flatten().collect();
    // One query per 500-id chunk instead of one query per post: the single
    // worker thread served a refresh with up to POST_LIMIT serial round-trips
    // here before.
    let ids: Vec<String> = returned
        .iter()
        .map(|(_, post)| post.post_id.clone())
        .collect();
    let statuses = store.recipient_statuses_for_posts(&ids)?;
    let posts = returned
        .into_iter()
        .map(|(_, post)| {
            let statuses = statuses.get(&post.post_id).cloned().unwrap_or_default();
            (post, statuses)
        })
        .collect();
    let task_page = store.latest_tasks(team_id, None, team_store::MAX_PAGE_SIZE)?;
    let task_older = page_cursor(&task_page, team_store::MAX_PAGE_SIZE);
    let tasks = task_page.into_iter().map(|(_, task)| task).collect();
    let finding_page = store.latest_findings(team_id, None, team_store::MAX_PAGE_SIZE)?;
    let finding_older = page_cursor(&finding_page, team_store::MAX_PAGE_SIZE);
    let findings = finding_page
        .into_iter()
        .map(|(_, finding)| finding)
        .collect();
    Ok(Reply::Snapshot {
        team: Some(team),
        goal,
        posts,
        tasks,
        findings,
        task_older,
        finding_older,
    })
}

/// The `before` cursor for the next older page: the page's first sequence
/// when the page came back full, `None` when the store ran out first.
fn page_cursor(page: &[(i64, impl Sized)], limit: usize) -> Option<i64> {
    if page.len() < limit {
        return None;
    }
    page.first().map(|(sequence, _)| *sequence)
}

#[allow(clippy::too_many_arguments)]
fn scoped(
    store: &mut TeamStore,
    team_id: &str,
    caller: &SessionTarget,
    caller_boot: &str,
    endpoint_ids: &[String],
    live_panes: &[String],
    action: ScopedAction,
    live_hosts: &[String],
) -> Result<Reply, Error> {
    if let ScopedAction::Register { name } = action {
        let member_id = uuid::Uuid::new_v4().to_string();
        let hosts = HostContext {
            caller_boot: caller_boot.to_owned(),
            live_hosts: live_hosts.to_vec(),
            endpoint_ids: endpoint_ids.to_vec(),
            live_panes: live_panes.to_vec(),
        };
        let member = store.register_member(team_id, &member_id, &name, caller, &hosts)?;
        let team = store.team(team_id)?.ok_or(Error::NotMember)?;
        return Ok(Reply::ScopedRegistered { team, member });
    }
    if let ScopedAction::Status { team_id: hint } = action {
        let filter = hint
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .or_else(|| (!team_id.trim().is_empty()).then_some(team_id));
        let hosts = HostContext {
            caller_boot: caller_boot.to_owned(),
            live_hosts: live_hosts.to_vec(),
            endpoint_ids: endpoint_ids.to_vec(),
            live_panes: live_panes.to_vec(),
        };
        let (team, member) = store
            .find_member_team(filter, caller, &hosts)?
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
    let hosts = HostContext {
        caller_boot: caller_boot.to_owned(),
        live_hosts: live_hosts.to_vec(),
        endpoint_ids: endpoint_ids.to_vec(),
        live_panes: live_panes.to_vec(),
    };
    let (team, member) = store
        .find_member_team(Some(team_id), caller, &hosts)?
        .ok_or(Error::NotMember)?;
    let member_id = member.member_id;
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
    })
}

mod github_import;

#[cfg(test)]
#[path = "team_worker/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "team_worker/newest_board_tests.rs"]
mod newest_board_tests;

#[cfg(test)]
#[path = "team_worker/newest_page_tests.rs"]
mod newest_page_tests;

#[cfg(test)]
#[path = "team_worker/query_bounds_tests.rs"]
mod query_bounds_tests;

#[cfg(test)]
#[path = "team_worker/scoped_tests.rs"]
mod scoped_tests;

#[cfg(test)]
#[path = "team_worker/bulletin_page_tests.rs"]
mod bulletin_page_tests;

#[cfg(test)]
#[path = "team_worker/prefix_id_tests.rs"]
mod prefix_id_tests;

#[cfg(test)]
#[path = "team_worker/delete_gate_tests.rs"]
mod delete_gate_tests;
