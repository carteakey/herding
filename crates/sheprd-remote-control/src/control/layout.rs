//! A phone's change to the layout of one of Herding's Herdr 2 tabs: a split,
//! a zoom, or closing it. herdr-core keeps no layout; Herding keeps it in its saved tabs
//! and changes it as the desktop does, so the phone never writes it and
//! never fights the desktop over it. The question waits in the daemon with
//! the phones' questions about workspaces (`control::changes`), and Herding
//! answers it with `Request::LayoutAnswer`.

use super::changes::AskId;
use serde::{Deserialize, Serialize};

/// The longest terminal address sent.
pub const MAX_TERMINAL: usize = 256;
/// The longest name a rename sends, in bytes; Herding trims it further.
pub const MAX_NAME: usize = 1024;

/// One change to one of Herding's Herdr 2 tabs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    pub id: AskId,
    /// The saved tab, by the id Herding gave it; 0 for a workspace's rename.
    pub tab: u64,
    /// The pane's terminal, by its herdr-core address; empty for a
    /// workspace's rename.
    pub terminal: String,
    pub op: Op,
}

impl Ask {
    /// Whether it stays within this module's bounds.
    pub fn valid(&self) -> bool {
        let names_pane = !self.terminal.is_empty() && self.terminal.len() <= MAX_TERMINAL;
        match &self.op {
            Op::RenameWorkspace { workspace, name } => {
                !workspace.is_empty()
                    && workspace.len() <= MAX_TERMINAL
                    && !name.trim().is_empty()
                    && name.len() <= MAX_NAME
            }
            Op::RenamePane(name) | Op::RenameTab(name) => {
                names_pane && name.as_ref().is_none_or(|name| name.len() <= MAX_NAME)
            }
            Op::Close | Op::SplitDown | Op::SplitRight | Op::Zoom => names_pane,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    /// A new shell beside the pane, as Split Right does on the desktop.
    SplitRight,
    /// A new shell below it.
    SplitDown,
    /// The pane fills its tab, or the split shows again.
    Zoom,
    /// The tab goes. The daemon ends its terminals; Herding drops the tab,
    /// which would otherwise keep its last pane, ended.
    Close,
    /// Names the pane, as a chat's label; `None` gives back its title.
    RenamePane(Option<String>),
    /// Names the tab; `None` gives back its program's title.
    RenameTab(Option<String>),
    /// Names a workspace Herding keeps for Herdr 2, which has no tab here.
    RenameWorkspace { workspace: String, name: String },
}

/// Herding's answer to an [`Ask`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "answer", content = "detail")]
pub enum Answer {
    Done,
    Failed(Failure),
    /// A split landed: the new shell's terminal, by its herdr-core address,
    /// which the phone shows next.
    Split(String),
}

/// Why Herding did not change the tab.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    #[error("Herding is starting another terminal; try again in a moment")]
    Busy,
    #[error("no Herding window has this tab open")]
    Gone,
    #[error("this tab only watches its terminal; split it where it types")]
    Watching,
    /// Last: what a newer Herding answers that this daemon does not know.
    #[error("Herding could not change this tab")]
    #[serde(other)]
    Refused,
}
