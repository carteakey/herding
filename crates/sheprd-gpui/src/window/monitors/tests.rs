use super::*;
use std::io;

fn english(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

#[test]
fn a_failed_restore_keeps_the_cause_its_git_commands_or_worker_gave() {
    crate::i18n::show(&sheprd_i18n::Locale::FALLBACK);
    let script = sheprd_vcs::Error::Checkpoint(checkpoint::Error::Script(
        sheprd_client::Error::Disconnected,
    ));
    assert_eq!(
        english(not_restored(&script)),
        format!(
            "Checkpoint not restored: {script}: {}",
            sheprd_client::Error::Disconnected
        )
    );
    let thread = sheprd_vcs::Error::Checkpoint(checkpoint::Error::Thread(io::Error::other(
        "no threads left",
    )));
    assert_eq!(
        english(not_restored(&thread)),
        format!("Checkpoint not restored: {thread}: no threads left")
    );
    assert_eq!(
        english(not_restored(&sheprd_vcs::Error::Checkpoint(
            checkpoint::Error::Busy
        ))),
        format!("Checkpoint not restored: {}", checkpoint::Error::Busy)
    );
}

#[test]
fn only_a_daemon_proven_local_names_checkouts_git_reads_here() {
    use sheprd_client::ConnectTarget;
    let mut live = crate::state::LiveState::default();
    live.local_daemon_peer = true;
    let socket = ConnectTarget::Socket("/tmp/forwarded.sock".into());
    assert!(
        !names_local_checkouts(&socket, &live),
        "a socket may forward another machine's daemon"
    );
    assert_eq!(
        names_local_checkouts(&ConnectTarget::Local, &live),
        cfg!(any(target_os = "linux", target_os = "macos"))
    );
    live.local_daemon_peer = false;
    assert!(!names_local_checkouts(&ConnectTarget::Local, &live));
}
