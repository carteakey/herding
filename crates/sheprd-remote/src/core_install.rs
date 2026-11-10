//! Installing the pinned herdr-core (Herdr 2) on another machine: the build
//! every release bundles beside the app (`scripts/release/herdr-core.json`),
//! downloaded on the machine and checked against its pinned checksum. It goes
//! in only where the bridge (`sheprd_client::core_bridge`) finds no core, so a
//! user's own core is never replaced, upgraded, or stopped. Only an approved
//! setup or install runs it; a connection never does.

use sheprd_client::{ScriptHost, ScriptLimits, core_bridge::CANDIDATES, shell_quote};
use std::{collections::BTreeMap, process::Command, sync::atomic::AtomicBool, time::Duration};

mod error;
pub use error::Error;

/// The release every build bundles, read at compile time so the app and the
/// machines it sets up run the same core.
const PINNED: &str = include_str!("../../../scripts/release/herdr-core.json");

/// What the script prints when it found a core and changed nothing.
const PRESENT: &str = "herdr-core-install:present";
/// What it prints once the pinned core is in place.
const INSTALLED: &str = "herdr-core-install:installed";

// The script's own exit codes for failures it can tell apart.
const NO_CURL: i32 = 4;
const NO_BUILD: i32 = 5;
const NO_CHECKSUM_TOOL: i32 = 6;
const CHECKSUM: i32 = 7;

/// A download, checking, and unpacking a few megabytes, may stall this long.
/// The script bounds its download itself too, below this, so a stalled
/// server fails the step where no idle deadline watches it, such as an
/// image's build.
const IDLE: Duration = Duration::from_secs(120);
const OUTPUT_LIMIT: u64 = 4 << 10;

/// How the pinned release is published.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Pinned {
    pub version: String,
    pub targets: BTreeMap<String, Build>,
}

/// One target's archive and its SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Build {
    pub url: String,
    pub sha256: String,
}

/// The POSIX hosts the script installs on: `uname -s` and `uname -m`, and
/// herdr-core's name for the build. Windows hosts have no `/bin/sh`.
const PLATFORMS: &[(&str, &[&str], &str)] = &[
    ("Darwin", &["arm64", "aarch64"], "macos-aarch64"),
    ("Darwin", &["x86_64"], "macos-x86_64"),
    ("Linux", &["aarch64", "arm64"], "linux-aarch64"),
    ("Linux", &["x86_64", "amd64"], "linux-x86_64"),
];

/// The pinned release.
pub fn pinned() -> Result<Pinned, Error> {
    serde_json::from_str(PINNED).map_err(Error::Pinned)
}

/// Where the core goes on a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir<'a> {
    /// `~/.local/bin`, which needs no root and the bridge searches.
    UserBin,
    /// An absolute folder, such as an image's `/usr/local/bin`.
    At(&'a str),
}

/// What installing did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A core was already there; nothing changed.
    Present,
    /// The pinned core is in place.
    Installed,
}

/// The installer as a script for `/bin/sh`: nothing when a core is found,
/// else the pinned build for this machine, checked, into `dir`, with `hc`
/// beside it unless something already has that name.
pub fn script(dir: Dir<'_>) -> Result<String, Error> {
    render(&pinned()?, dir)
}

fn render(pinned: &Pinned, dir: Dir<'_>) -> Result<String, Error> {
    let mut cases = String::new();
    for (system, machines, target) in PLATFORMS {
        let build = pinned
            .targets
            .get(*target)
            .ok_or(Error::NoPinnedBuild(target))?;
        if !build.sha256.bytes().all(|b| b.is_ascii_hexdigit()) || build.sha256.len() != 64 {
            return Err(Error::NoPinnedBuild(target));
        }
        for machine in *machines {
            cases.push_str(&format!(
                "    {}) url={}; sum={};;\n",
                shell_quote(&format!("{system} {machine}")),
                shell_quote(&build.url),
                build.sha256,
            ));
        }
    }
    // A user's install looks where the bridge does. An image looks only in
    // its own folder: a core in the building user's home is not one every
    // user of the image can run.
    let (dir, present) = match dir {
        Dir::UserBin => (
            "\"$HOME/.local/bin\"".to_owned(),
            format!(
                "{CANDIDATES}\n    if [ -n \"$path\" ] && [ -x \"$path\" ]; then echo {PRESENT}; exit 0; fi\ndone"
            ),
        ),
        Dir::At(dir) if dir.starts_with('/') && !dir.chars().any(char::is_control) => {
            let dir = shell_quote(dir);
            let present = format!("if [ -x {dir}/herdr-core ]; then echo {PRESENT}; exit 0; fi");
            (dir, present)
        }
        Dir::At(_) => return Err(Error::Dir),
    };
    Ok(format!(
        r#"{present}
command -v curl >/dev/null 2>&1 || {{ echo "curl is required to install herdr-core" >&2; exit {NO_CURL}; }}
platform="$(PATH=/usr/bin:/bin uname -s) $(PATH=/usr/bin:/bin uname -m)"
case "$platform" in
{cases}    *) echo "herdr-core has no build for $platform" >&2; exit {NO_BUILD};;
esac
if command -v sha256sum >/dev/null 2>&1; then check() {{ sha256sum "$1" | cut -d' ' -f1; }}
elif command -v shasum >/dev/null 2>&1; then check() {{ shasum -a 256 "$1" | cut -d' ' -f1; }}
else echo "sha256sum or shasum is required to check herdr-core" >&2; exit {NO_CHECKSUM_TOOL}; fi
work=$(mktemp -d) || exit 1
trap 'rm -rf "$work"' EXIT
curl -fsSL --connect-timeout 30 --speed-limit 1024 --speed-time 60 --max-time 600 "$url" -o "$work/core.tar.gz" || exit 1
[ "$(check "$work/core.tar.gz")" = "$sum" ] || {{ echo "herdr-core download does not match its pinned checksum" >&2; exit {CHECKSUM}; }}
tar -xzf "$work/core.tar.gz" -C "$work" herdr-core || exit 1
dir={dir}
mkdir -p "$dir" || exit 1
chmod 755 "$work/herdr-core" && mv -f "$work/herdr-core" "$dir/herdr-core.new.$$" && mv -f "$dir/herdr-core.new.$$" "$dir/herdr-core" || exit 1
[ -e "$dir/hc" ] || [ -L "$dir/hc" ] || ln -s herdr-core "$dir/hc" 2>/dev/null || :
echo {INSTALLED}"#
    ))
}

/// Where [`install`] runs the script.
pub enum Target<'a> {
    /// An SSH host or a WSL distribution.
    Host(ScriptHost<'a>),
    /// A cloud provider's authenticated command for a machine; the script
    /// is appended as its last argument.
    Command(Command),
}

/// Installs the pinned core into `~/.local/bin` unless the machine has one.
/// Blocking: call it from a background thread; `cancelled` stops it.
pub fn install(target: Target<'_>, cancelled: &AtomicBool) -> Result<Outcome, Error> {
    run(&script(Dir::UserBin)?, target, cancelled)
}

fn run(script: &str, target: Target<'_>, cancelled: &AtomicBool) -> Result<Outcome, Error> {
    let limits = ScriptLimits {
        output: OUTPUT_LIMIT,
        idle: IDLE,
    };
    let mut output = Vec::new();
    let ran = match target {
        Target::Host(host) => sheprd_client::run_script(
            host,
            script,
            std::io::empty(),
            &mut output,
            limits,
            cancelled,
        ),
        Target::Command(mut command) => {
            command.arg(format!("/bin/sh -c {}", shell_quote(script)));
            sheprd_client::run_script_command(
                command,
                std::io::empty(),
                &mut output,
                limits,
                cancelled,
            )
        }
    };
    if let Err(error) = ran {
        return Err(failure(error));
    }
    outcome(&output)
}

/// What the script's last line says it did.
fn outcome(output: &[u8]) -> Result<Outcome, Error> {
    let text = String::from_utf8_lossy(output);
    match text.lines().map(str::trim).rfind(|line| !line.is_empty()) {
        Some(PRESENT) => Ok(Outcome::Present),
        Some(INSTALLED) => Ok(Outcome::Installed),
        _ => Err(Error::Unfinished),
    }
}

/// The script's own exit codes, told apart from the run's failure.
fn failure(error: sheprd_client::Error) -> Error {
    let sheprd_client::Error::ScriptExit { status, .. } = &error else {
        return Error::Run(error);
    };
    match status.code() {
        Some(NO_CURL) => Error::NoCurl,
        Some(NO_BUILD) => Error::NoBuild,
        Some(NO_CHECKSUM_TOOL) => Error::NoChecksumTool,
        Some(CHECKSUM) => Error::Checksum,
        _ => Error::Run(error),
    }
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests;
