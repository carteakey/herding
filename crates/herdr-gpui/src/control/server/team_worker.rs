//! Bounded, serialized access to the durable team store.
//!
//! Construct this worker off the GPUI thread so the SQLite file is opened on
//! its worker thread. Register the returned response receiver with the
//! server's `Watches` before calling `submit`. Dropping the response receiver
//! after an accepted mutation does not cancel that mutation; the caller's
//! outcome is unknown until it reads the reply.
use herdr_orchestrator::team_store::{
    self, Acknowledgement, Author, Post, RecipientStatus, Recipients, SessionTarget, Team,
    TeamStore,
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
    /// The Team view's read: a team with its newest posts and delivery states.
    Snapshot {
        team_id: String,
        limit: usize,
    },
    /// An agent call. The caller's freshly proven target must equal a stored
    /// member's target; the worker never trusts a member ID from the wire.
    Scoped {
        team_id: String,
        caller: SessionTarget,
        action: ScopedAction,
    },
}

pub(crate) enum ScopedAction {
    Roster,
    BulletinList {
        after: i64,
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
}

/// A post with each recipient's delivery state.
pub(crate) type PostDelivery = (Post, Vec<(String, RecipientStatus)>);

pub(crate) enum Reply {
    Snapshot {
        team: Option<Team>,
        posts: Vec<PostDelivery>,
    },
    ScopedRoster {
        team: Team,
    },
    ScopedBulletin {
        team: Team,
        member_id: String,
        entries: Vec<(i64, Post, Option<RecipientStatus>)>,
    },
    ScopedPosted {
        team: Team,
        post: Post,
    },
    ScopedAcknowledged {
        acknowledgement: Acknowledgement,
    },
    Saved,
    Teams(Vec<Team>),
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
                | Operation::AddPost { .. }
                | Operation::Scoped {
                    action: ScopedAction::BulletinPost { .. } | ScopedAction::BulletinAck { .. },
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
        Operation::Snapshot { team_id, limit } => snapshot(store, &team_id, limit)?,
        Operation::Scoped {
            team_id,
            caller,
            action,
        } => scoped(store, &team_id, &caller, action)?,
    };
    Ok(reply)
}

/// Pages are bounded; a team with more than `MAX_SCANNED_POSTS` posts shows
/// its newest `limit` among the first scanned, which a later pass will page.
const MAX_SCANNED_POSTS: usize = 1000;

fn snapshot(store: &mut TeamStore, team_id: &str, limit: usize) -> Result<Reply, Error> {
    let Some(team) = store.team(team_id)? else {
        return Ok(Reply::Snapshot {
            team: None,
            posts: Vec::new(),
        });
    };
    let mut scanned = Vec::new();
    let mut cursor = 0;
    while scanned.len() < MAX_SCANNED_POSTS {
        let page = store.posts(team_id, cursor, team_store::MAX_PAGE_SIZE)?;
        let Some((last, _)) = page.last() else { break };
        cursor = *last;
        let full = page.len() == team_store::MAX_PAGE_SIZE;
        scanned.extend(page);
        if !full {
            break;
        }
    }
    let skip = scanned.len().saturating_sub(limit);
    let mut posts = Vec::new();
    for (_, post) in scanned.into_iter().skip(skip) {
        let statuses = store.recipient_statuses(&post.post_id)?;
        posts.push((post, statuses));
    }
    Ok(Reply::Snapshot {
        team: Some(team),
        posts,
    })
}

fn scoped(
    store: &mut TeamStore,
    team_id: &str,
    caller: &SessionTarget,
    action: ScopedAction,
) -> Result<Reply, Error> {
    let team = store.team(team_id)?.ok_or(Error::NotMember)?;
    let member_id = std::iter::once(&team.supervisor)
        .chain(team.members.iter())
        .find(|member| member.target == *caller)
        .map(|member| member.member_id.clone())
        .ok_or(Error::NotMember)?;
    Ok(match action {
        ScopedAction::Roster => Reply::ScopedRoster { team },
        ScopedAction::BulletinList { after, limit } => {
            let mut entries = Vec::new();
            for (sequence, post) in store.posts(team_id, after, limit)? {
                let status = store
                    .recipient_statuses(&post.post_id)?
                    .into_iter()
                    .find(|(id, _)| *id == member_id)
                    .map(|(_, status)| status);
                entries.push((sequence, post, status));
            }
            Reply::ScopedBulletin {
                team,
                member_id,
                entries,
            }
        }
        ScopedAction::BulletinPost {
            post_id,
            reply_to,
            text,
            mentions,
        } => {
            let recipients = if mentions.is_empty() {
                Recipients::All
            } else {
                Recipients::Explicit(mentions)
            };
            let post = store.add_post_with_recipients(
                &post_id,
                team_id,
                &Author::Member { member_id },
                reply_to.as_deref(),
                &text,
                &recipients,
            )?;
            Reply::ScopedPosted { team, post }
        }
        ScopedAction::BulletinAck { message_id } => Reply::ScopedAcknowledged {
            acknowledgement: store.acknowledge(&message_id, &member_id)?,
        },
    })
}

#[cfg(test)]
#[path = "team_worker/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "team_worker/scoped_tests.rs"]
mod scoped_tests;
