//! A `notify` step on a host without Herdr 1, whose server would show it:
//! the system's own notification, from the daemon, so it reaches the user
//! while Herding is closed too. Title and body go as arguments, never as
//! script text, so neither can run as code.

use std::{
    io::Read,
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

/// The most of a failed notifier's complaint kept.
const MAX_COMPLAINT_CHARS: usize = 300;
/// The most of its error output read.
const MAX_COMPLAINT_BYTES: u64 = 4096;
/// How long a notifier may take; showing one is quick, a hung one is not.
const TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{program} could not show the notification: {complaint}")]
    Failed {
        program: &'static str,
        complaint: String,
    },
    #[error("could not run {program}")]
    Launch {
        program: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("{program} did not show the notification in time")]
    TimedOut { program: &'static str },
    #[error("notifications need Herdr 1 on this system")]
    Unsupported,
}

/// Shows `body` under `title` as a system notification, giving up (and
/// ending the notifier) after [`TIMEOUT`] or once `give_up` says so.
pub fn show(title: &str, body: &str, give_up: impl Fn() -> bool) -> Result<(), Error> {
    let (program, args) = command(title, body).ok_or(Error::Unsupported)?;
    let launch = |source| Error::Launch { program, source };
    let mut child = sheprd_client::new_command(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(launch)?;
    let stderr = child.stderr.take();
    // Read to the end, so a long complaint never meets a closed pipe.
    let complaint = thread::spawn(move || {
        let mut kept = Vec::new();
        if let Some(stderr) = stderr {
            let mut limited = stderr.take(MAX_COMPLAINT_BYTES);
            let _ = limited.read_to_end(&mut kept);
            let _ = std::io::copy(&mut limited.into_inner(), &mut std::io::sink());
        }
        String::from_utf8_lossy(&kept).into_owned()
    });
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(launch)? {
            break status;
        }
        if give_up() || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::TimedOut { program });
        }
        thread::sleep(POLL);
    };
    if status.success() {
        return Ok(());
    }
    let complaint = complaint.join().unwrap_or_default();
    Err(Error::Failed {
        program,
        complaint: complaint.trim().chars().take(MAX_COMPLAINT_CHARS).collect(),
    })
}

/// The program and arguments that show a notification here.
fn command(title: &str, body: &str) -> Option<(&'static str, Vec<String>)> {
    if cfg!(target_os = "macos") {
        let script = "on run argv\ndisplay notification (item 2 of argv) with title (item 1 of argv)\nend run";
        return Some((
            "osascript",
            vec!["-e".into(), script.into(), title.into(), body.into()],
        ));
    }
    if cfg!(all(unix, not(target_os = "macos"))) {
        // `--` keeps a title starting with a dash from reading as an option.
        return Some((
            "notify-send",
            vec![
                "--app-name=Herding".into(),
                "--".into(),
                title.into(),
                body.into(),
            ],
        ));
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
