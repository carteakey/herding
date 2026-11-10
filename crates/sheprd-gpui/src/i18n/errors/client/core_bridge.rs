//! `sheprd_client::core_bridge::Error`: reaching herdr-core on another host,
//! from `error-client-core-bridge.ftl`.

use super::Localize;
use crate::i18n::fl;
use sheprd_client::core_bridge::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Missing => fl!("error-client-core-bridge-missing"),
            // keep-sorted end
        }
    }
}
