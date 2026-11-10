//! herdr-core (Herdr 2) on another machine, reached the way a remote Herdr 1
//! is: the app runs `ssh`, `wsl.exe`, or a cloud provider's command itself,
//! and the child's standard streams carry `herdr-core connect`, which speaks
//! the core's protocol over stdin and stdout and starts the host's core when
//! none is running. The core never proxies to another machine, and nothing
//! here installs, upgrades, or stops a core.
//!
//! The script prints the ready banner the Herdr 1 bridge prints before it
//! hands over the stream, so a login shell's stray output never reaches the
//! protocol, and exits 127 when the host has no `herdr-core`.

use crate::{
    Result,
    ssh::{self, Bridge, Closed, quote},
};
use std::{
    io::{self, Read, Write},
    process::Command,
    sync::atomic::AtomicBool,
    time::Instant,
};

mod error;
pub use error::Error;

/// The core's install locations, searched the way the Herdr 1 bridge searches
/// for `herdr`: PATH first, then the known roots. Paths stay in shell
/// variables; nothing discovered is eval'd. It opens a `for path in ...; do`
/// loop, which the script using it closes, so an installer checks the same
/// places the bridge runs from.
pub const CANDIDATES: &str = r#"candidate=$(command -v herdr-core 2>/dev/null || :)
case "$candidate" in /*) ;; *) candidate=;; esac
linuxbrew=
case "$(PATH=/usr/bin:/bin uname -s)" in Linux) linuxbrew=/home/linuxbrew/.linuxbrew/bin/herdr-core;; esac
for path in "$candidate" "$HOME/.local/bin/herdr-core" /opt/homebrew/bin/herdr-core /usr/local/bin/herdr-core "$linuxbrew" "$HOME/.nix-profile/bin/herdr-core" "/etc/profiles/per-user/$USER/bin/herdr-core" /nix/var/nix/profiles/default/bin/herdr-core /run/current-system/sw/bin/herdr-core; do"#;

/// Sets `herdr_core` to the first installed core, searched as the bridge
/// searches, or leaves it empty; for scripts that run the core's CLI.
pub fn locate_script() -> String {
    format!(
        r#"herdr_core=
{CANDIDATES}
    if [ -n "$path" ] && [ -x "$path" ]; then
        herdr_core=$path
        break
    fi
done"#
    )
}

/// The bridge as a script for `/bin/sh -c`: the first installed core, run as
/// `connect` after the ready banner.
pub(crate) fn script() -> String {
    let ready = String::from_utf8_lossy(ssh::READY.trim_ascii_end());
    format!(
        r#"{CANDIDATES}
    if [ -n "$path" ] && [ -x "$path" ]; then
        printf '\n%s\n' '{ready}'
        exec "$path" connect
    fi
done
exit 127"#
    )
}

/// [`script`] for a Windows host, whose login shell is `cmd.exe` or
/// PowerShell. `Start-Process` hands the core the console's own streams, so
/// PowerShell never decodes the binary frames.
pub(crate) fn powershell_command() -> String {
    let ready = String::from_utf8_lossy(ssh::READY.trim_ascii_end());
    ssh::powershell::encoded_command(&format!(
        "$core = Get-Command herdr-core.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1; $path = if ($null -ne $core) {{ $core.Source }} else {{ Join-Path $env:USERPROFILE '.local\\bin\\herdr-core.exe' }}; if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {{ exit 127 }}; [Console]::Out.WriteLine(); [Console]::Out.WriteLine('{ready}'); [Console]::Out.Flush(); $process = Start-Process -FilePath $path -ArgumentList connect -NoNewWindow -PassThru -ErrorAction Stop; $null = $process.Handle; $process.WaitForExit(); exit $process.ExitCode"
    ))
}

/// The host's core over SSH, started there when none is running. A host that
/// cannot run `/bin/sh` is tried through PowerShell. Blocking: call it from a
/// background thread; `stop` cancels it.
pub fn ssh(target: &str, stop: &AtomicBool) -> Result<Bridge> {
    crate::catalog::validate_target(target)?;
    let posix = ssh::command(target, &format!("/bin/sh -c {}", quote(&script())));
    match open(posix, stop)? {
        Ok(bridge) => Ok(bridge),
        Err(closed) if ssh::powershell::rejected_posix(closed.code) => {
            tracing::info!(code = ?closed.code, "host cannot run /bin/sh; trying PowerShell");
            open(ssh::command(target, &powershell_command()), stop)?.map_err(refused)
        }
        Err(closed) => Err(refused(closed)),
    }
}

/// The core through an application-built remote command, such as a cloud
/// provider's `coder ssh -- <workspace>`: the script is appended as its final
/// argument, as `connect_command` appends Herdr 1's.
pub fn command(mut command: Command, stop: &AtomicBool) -> Result<Bridge> {
    command.arg(format!("/bin/sh -c {}", quote(&script())));
    open(command, stop)?.map_err(refused)
}

/// The core inside a WSL distribution on this Windows machine.
#[cfg(windows)]
pub fn wsl(distro: &str, stop: &AtomicBool) -> Result<Bridge> {
    if !crate::valid_distro(distro) {
        return Err(crate::wsl::Error::InvalidDistro.into());
    }
    open(crate::wsl::command(distro, &script()), stop)?.map_err(refused)
}

#[cfg(not(windows))]
pub fn wsl(distro: &str, _stop: &AtomicBool) -> Result<Bridge> {
    if !crate::valid_distro(distro) {
        return Err(crate::wsl::Error::InvalidDistro.into());
    }
    Err(crate::wsl::Error::Unsupported.into())
}

/// A bridge is the stream to its child, so a protocol worker can own both:
/// the child lives as long as the worker reads.
impl Read for Bridge {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for Bridge {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

fn open(command: Command, stop: &AtomicBool) -> Result<std::result::Result<Bridge, Closed>> {
    let opened = ssh::start_bridge(command, stop, |stream, stop| {
        ssh::await_ready(stream, stop, Instant::now()).map(drop)
    })?;
    Ok(opened.map(|(stream, child)| Bridge { stream, child }))
}

/// Why the bridge closed before its banner: the script's own 127 says the
/// host has no core; anything else is what `ssh` reported.
fn refused(closed: Closed) -> crate::Error {
    match closed.code {
        Some(127) => Error::Missing.into(),
        _ => ssh::Error::Refused(closed.failure).into(),
    }
}

// The fixtures run `/bin/sh`.
#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests;
