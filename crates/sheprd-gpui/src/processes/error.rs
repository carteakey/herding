//! Why a pane's process list could not be read or acted on.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Herdr's answer about the pane's process was not understood.")]
    Answer(#[source] serde_json::Error),
    #[error("Still ending the last processes. Try again in a moment.")]
    Busy,
    #[error("Could not ask herdr-core which process the terminal runs.")]
    Core(#[source] sheprd_core_client::Error),
    #[error("The herdr executable's path is not valid UTF-8.")]
    Executable,
    #[error("Herdr did not name a process for this pane.")]
    NoRoot,
    #[error("Could not ask Herdr which process the pane runs.")]
    Query(#[source] sheprd_client::Error),
    #[error("This terminal runs on another machine, whose processes this computer cannot list.")]
    RemoteTerminal,
    #[error("The pane's process has exited.")]
    RootExited,
    #[error("The process list stopped updating. Reopen it to try again.")]
    Stopped,
    #[error("herdr-core no longer runs this terminal.")]
    TerminalGone,
    #[error("Could not start watching the pane's processes.")]
    Worker(#[source] std::io::Error),
}
