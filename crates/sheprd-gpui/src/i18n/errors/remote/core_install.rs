//! `sheprd_remote::core_install::Error`: installing the pinned herdr-core on
//! another machine, from `error-remote-core-install.ftl`.

use super::Localize;
use crate::i18n::fl;
use sheprd_remote::core_install::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Checksum => fl!("error-remote-core-install-checksum"),
            Self::Dir => fl!("error-remote-core-install-dir"),
            Self::NoBuild => fl!("error-remote-core-install-no-build"),
            Self::NoChecksumTool => fl!("error-remote-core-install-no-checksum-tool"),
            Self::NoCurl => fl!("error-remote-core-install-no-curl"),
            Self::NoPinnedBuild(target) => {
                fl!(
                    "error-remote-core-install-no-pinned-build",
                    target = *target
                )
            }
            Self::Pinned(_) => fl!("error-remote-core-install-pinned"),
            Self::Run(error) => error.localize(),
            Self::Unfinished => fl!("error-remote-core-install-unfinished"),
            // keep-sorted end
        }
    }
}
