//! Which Herdr a host's automations drive: Herdr 1 through its command
//! line, or herdr-core through its socket. `terminals` in `config.toml`
//! chooses; without it, Herdr 1 when it is installed, as Herding leaves a
//! user's Herdr 1 sessions where they are, else herdr-core.

use crate::{herdr::Herdr, herdr_core::HerdrCore, settings::Settings};
use sheprd_automation::report::Terminals;
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

#[derive(Clone, Debug)]
pub enum Drive {
    Herdr(Herdr),
    HerdrCore(HerdrCore),
}

impl Drive {
    pub fn new(settings: &Settings) -> Self {
        let herdr = || Herdr::new(settings.herdr.clone(), settings.herdr_session.clone());
        let core = || HerdrCore::new(settings.herdr_core.clone());
        match settings.terminals {
            Some(Terminals::Herdr) => Self::Herdr(herdr()),
            Some(Terminals::HerdrCore) => Self::HerdrCore(core()),
            None if settings.herdr.is_some() || crate::herdr::find().is_absolute() => {
                Self::Herdr(herdr())
            }
            None => Self::HerdrCore(core()),
        }
    }

    pub fn terminals(&self) -> Terminals {
        match self {
            Self::Herdr(_) => Terminals::Herdr,
            Self::HerdrCore(_) => Terminals::HerdrCore,
        }
    }

    /// This drive for one step: each Herdr 1 call ends when `stop` is set or
    /// `deadline` passes. herdr-core's steps take the same limits as an
    /// `Until` with every call that can wait (see `steps::execute`).
    pub fn bounded(&self, stop: Arc<AtomicBool>, deadline: Instant) -> Self {
        match self {
            Self::Herdr(herdr) => Self::Herdr(herdr.bounded(stop, deadline)),
            Self::HerdrCore(core) => Self::HerdrCore(core.clone()),
        }
    }
}
