//! `sheprd_client::herdr::worktree::Error`: a new worktree for herdr-core
//! work, from `error-client-worktree.ftl`.

use super::Localize;
use crate::i18n::fl;
use sheprd_client::herdr::worktree::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::GaveUp => fl!("error-client-worktree-gave-up"),
            Self::NoHome => fl!("error-client-worktree-no-home"),
            Self::NoRepositoryName => fl!("error-client-worktree-no-repository-name"),
            Self::Refused(complaint) => fl!(
                "error-client-worktree-refused",
                complaint = complaint.as_str()
            ),
            Self::Start(_) => fl!("error-client-worktree-start"),
            // keep-sorted end
        }
    }
}
