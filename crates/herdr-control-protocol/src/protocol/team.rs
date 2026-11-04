//! Team membership, bulletin, task board, and finding operations carried over the app control socket.
use serde::{Deserialize, Serialize};

/// Maximum UTF-8 byte lengths. The server must enforce these at the handler
/// boundary; serde intentionally keeps the protocol structs straightforward.
pub const MAX_TEAM_ID_BYTES: usize = 128;
pub const MAX_TEAM_NAME_BYTES: usize = 128;
pub const MAX_MEMBER_ID_BYTES: usize = 128;
pub const MAX_MEMBER_NAME_BYTES: usize = 128;
pub const MAX_AGENT_BYTES: usize = 64;
pub const MAX_MESSAGE_ID_BYTES: usize = 128;
pub const MAX_TASK_ID_BYTES: usize = 128;
pub const MAX_TASK_TITLE_BYTES: usize = 256;
pub const MAX_FINDING_ID_BYTES: usize = 128;
pub const MAX_BULLETIN_TEXT_BYTES: usize = 16 * 1024;
/// Maximum number of explicitly mentioned member IDs on one entry.
pub const MAX_MENTIONS: usize = 64;
/// Maximum entries returned by a single roster, bulletin, task, or finding reply.
pub const MAX_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeamOp {
    /// Discover the caller session's active team, membership, goal, and pending counts.
    Status {
        /// Optional team identifier; when omitted, discovers the caller's registered team.
        #[serde(default)]
        team_id: Option<String>,
    },
    /// Register the caller's current agent session as a member of `team_id`.
    Register {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Member display name, at most [`MAX_MEMBER_NAME_BYTES`] bytes.
        name: String,
    },
    /// List explicitly registered members, bounded by [`MAX_PAGE_SIZE`].
    Roster {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
    },
    /// Set or update the team goal description.
    GoalSet {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Goal text, at most [`MAX_BULLETIN_TEXT_BYTES`] bytes.
        goal: String,
    },
    /// Read bulletin entries after an optional opaque cursor.
    BulletinList {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Last sequence already read; entries after this value are returned.
        #[serde(default)]
        after: Option<u64>,
    },
    /// Post a bulletin entry, at most [`MAX_BULLETIN_TEXT_BYTES`] bytes.
    BulletinPost {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Optional parent message ID, at most [`MAX_MESSAGE_ID_BYTES`] bytes.
        #[serde(default)]
        reply_to: Option<String>,
        /// Explicit recipient member IDs or names, each at most [`MAX_MEMBER_ID_BYTES`] bytes;
        /// at most [`MAX_MENTIONS`]. Empty means resolve `@mentions` in `text` or broadcast.
        #[serde(default)]
        mentions: Vec<String>,
        /// Entry body, at most [`MAX_BULLETIN_TEXT_BYTES`] UTF-8 bytes.
        text: String,
    },
    /// Acknowledge a bulletin entry for the caller's registered membership.
    BulletinAck {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Opaque message identifier, at most [`MAX_MESSAGE_ID_BYTES`] bytes.
        message_id: String,
    },
    /// List tasks on the team board.
    TaskList {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
    },
    /// Create a new task on the team board.
    TaskCreate {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Short task title, at most [`MAX_TASK_TITLE_BYTES`] bytes.
        title: String,
        /// Optional task brief/context, at most [`MAX_BULLETIN_TEXT_BYTES`] bytes.
        #[serde(default)]
        brief: String,
        /// Optional assignee member ID or display name.
        #[serde(default)]
        assignee: Option<String>,
    },
    /// Assign or reassign an open or blocked task to a member.
    TaskAssign {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Opaque task identifier, at most [`MAX_TASK_ID_BYTES`] bytes.
        task_id: String,
        /// Assignee member ID or display name.
        assignee: String,
    },
    /// Atomically claim an open task for the caller's member identity.
    TaskClaim {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Opaque task identifier, at most [`MAX_TASK_ID_BYTES`] bytes.
        task_id: String,
    },
    /// Mark a task blocked with a reason.
    TaskBlock {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Opaque task identifier, at most [`MAX_TASK_ID_BYTES`] bytes.
        task_id: String,
        /// Blocker explanation, at most [`MAX_BULLETIN_TEXT_BYTES`] bytes.
        reason: String,
    },
    /// Complete a task with a result summary or evidence reference.
    TaskComplete {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Opaque task identifier, at most [`MAX_TASK_ID_BYTES`] bytes.
        task_id: String,
        /// Completion result or evidence summary, at most [`MAX_BULLETIN_TEXT_BYTES`] bytes.
        result: String,
    },
    /// Cancel a task.
    TaskCancel {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Opaque task identifier, at most [`MAX_TASK_ID_BYTES`] bytes.
        task_id: String,
    },
    /// List durable findings recorded by the team.
    FindingList {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
    },
    /// Record a durable finding with evidence and confidence.
    FindingAdd {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Optional linked task ID, at most [`MAX_TASK_ID_BYTES`] bytes.
        #[serde(default)]
        task_id: Option<String>,
        /// Confidence level in the finding.
        confidence: ConfidenceLevel,
        /// Evidence or artifact reference, at most [`MAX_BULLETIN_TEXT_BYTES`] bytes.
        evidence: String,
        /// Optional prior finding ID that this finding supersedes.
        #[serde(default)]
        supersedes: Option<String>,
        /// Finding description, at most [`MAX_BULLETIN_TEXT_BYTES`] bytes.
        description: String,
    },
    /// Inspect team member liveliness and PTO rate limit status.
    Heartbeat {
        /// Opaque team identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
        team_id: String,
        /// Optional specific member ID to inspect.
        #[serde(default)]
        member_id: Option<String>,
        /// Whether to send a liveliness ping.
        #[serde(default)]
        ping: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamInfo {
    /// Opaque identifier, at most [`MAX_TEAM_ID_BYTES`] bytes.
    pub id: String,
    /// Display name, at most [`MAX_TEAM_NAME_BYTES`] bytes.
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberInfo {
    /// Opaque identifier, at most [`MAX_MEMBER_ID_BYTES`] bytes.
    pub id: String,
    /// Display name, at most [`MAX_MEMBER_NAME_BYTES`] bytes.
    pub name: String,
    /// Set by the team-creation UI; agent registration cannot claim this role.
    pub supervisor: bool,
    /// Agent kind label, at most [`MAX_AGENT_BYTES`] bytes.
    pub agent: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulletinEntry {
    /// Monotonic team-local sequence used as a read cursor.
    pub sequence: u64,
    /// Opaque identifier, at most [`MAX_MESSAGE_ID_BYTES`] bytes.
    pub id: String,
    /// Registered author's member identifier, at most [`MAX_MEMBER_ID_BYTES`] bytes.
    pub member_id: String,
    /// Author display name, at most [`MAX_MEMBER_NAME_BYTES`] bytes.
    pub author: String,
    /// Parent message ID when this entry is a reply.
    pub reply_to: Option<String>,
    /// Recipient member IDs, each at most [`MAX_MEMBER_ID_BYTES`] bytes.
    pub mentions: Vec<String>,
    /// Entry body, at most [`MAX_BULLETIN_TEXT_BYTES`] bytes.
    pub text: String,
    /// Delivery state relative to the requesting member. For a broadcast (empty
    /// `mentions`) every member is pending until acknowledged. For a directed
    /// entry, members outside `mentions` have `NotMentioned`.
    pub delivery_state: DeliveryState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    NotMentioned,
    Pending,
    Acknowledged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Open,
    InProgress,
    Blocked,
    Done,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskInfo {
    pub id: String,
    pub title: String,
    pub brief: String,
    pub creator: String,
    pub assignee_id: Option<String>,
    pub assignee: Option<String>,
    pub status: TaskState,
    pub blocker_reason: Option<String>,
    pub result: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingInfo {
    pub id: String,
    pub author_id: String,
    pub author: String,
    pub task_id: Option<String>,
    pub description: String,
    pub evidence: String,
    pub confidence: ConfidenceLevel,
    pub supersedes: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatInfo {
    pub member_id: String,
    pub name: String,
    pub role: String,
    pub agent: String,
    pub pane_id: Option<String>,
    pub status: String,
    pub health: String,
    pub details: String,
    #[serde(default)]
    pub rtt_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Status {
        team: TeamInfo,
        goal: Option<String>,
        member: Box<MemberInfo>,
        pending_mentions: usize,
        open_tasks: usize,
        assigned_tasks: usize,
    },
    Registered {
        member: Box<MemberInfo>,
    },
    Roster {
        team: TeamInfo,
        members: Vec<MemberInfo>,
    },
    GoalUpdated {
        team_id: String,
        goal: Option<String>,
    },
    Bulletin {
        entries: Vec<BulletinEntry>,
        /// Last sequence in this page; pass as `after` to continue.
        next: Option<u64>,
    },
    Posted {
        entry: Box<BulletinEntry>,
    },
    Acknowledged {
        message_id: String,
        delivery_state: DeliveryState,
    },
    Tasks {
        tasks: Vec<TaskInfo>,
    },
    Task {
        task: Box<TaskInfo>,
    },
    Findings {
        findings: Vec<FindingInfo>,
    },
    Finding {
        finding: Box<FindingInfo>,
    },
    Heartbeat {
        team_id: String,
        members: Vec<HeartbeatInfo>,
    },
}

#[cfg(test)]
mod delete_gate;

#[cfg(test)]
mod tests;
