//! Team membership and bulletin operations carried over the app control socket.
//! Teams are created by the UI, which assigns the supervisor; agent callers
//! can only register with an existing team and cannot claim that role.
use serde::{Deserialize, Serialize};

/// Maximum UTF-8 byte lengths. The server must enforce these at the handler
/// boundary; serde intentionally keeps the protocol structs straightforward.
pub const MAX_TEAM_ID_BYTES: usize = 128;
pub const MAX_TEAM_NAME_BYTES: usize = 128;
pub const MAX_MEMBER_ID_BYTES: usize = 128;
pub const MAX_MEMBER_NAME_BYTES: usize = 128;
pub const MAX_AGENT_BYTES: usize = 64;
pub const MAX_MESSAGE_ID_BYTES: usize = 128;
pub const MAX_BULLETIN_TEXT_BYTES: usize = 16 * 1024;
/// Maximum number of explicitly mentioned member IDs on one entry.
pub const MAX_MENTIONS: usize = 64;
/// Maximum entries returned by a single roster or bulletin-list reply.
pub const MAX_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeamOp {
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
        /// Explicit recipient member IDs, each at most [`MAX_MEMBER_ID_BYTES`] bytes;
        /// at most [`MAX_MENTIONS`]. Empty means a team-wide post.
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Registered {
        member: MemberInfo,
    },
    Roster {
        team: TeamInfo,
        members: Vec<MemberInfo>,
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
}

#[cfg(test)]
mod tests;
