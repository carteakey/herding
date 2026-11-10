---
title: Troubleshooting
blurb: Logs, crash reports, connection and permission fixes.
---

# Troubleshooting

When something goes wrong, start with the app's log and the status bar. This page covers where logs and crash reports are kept, common connection and permission problems, and how to report a bug.

## Logs

Sheprd writes its own log, one JSON record per line, to:

| Platform | Path |
| --- | --- |
| macOS and Linux | `$XDG_STATE_HOME/herdr/gpui/logs/herdr-gpui.jsonl`, by default `~/.local/state/herdr/gpui/logs/herdr-gpui.jsonl` |
| Windows | The same path under `HOME` when it is set, otherwise under `%LOCALAPPDATA%` |

On macOS and Linux the file is readable only by you. Past 16 MiB it is rotated to `herdr-gpui.1.jsonl`, so at most two files are kept. Debug records are captured in release builds too; you do not need a debug build or `RUST_LOG`.

To read it:

- **Window > Logs** shows the newest 5,000 records, including earlier runs, and lets you filter, copy or export them.
- From a terminal, `sheprd logs` prints the log without the app running. `sheprd logs --lines N` limits the output and `sheprd logs --path` prints where it is.

Nothing in the log is uploaded. Herdr's daemons keep their own logs; see [Herdr's troubleshooting guide](https://herdr.dev/docs/troubleshooting/) for those.

## Crash reports

If Sheprd panics, it writes `crash-YYYYMMDD-HHMMSS.mmm-PID.txt` beside the log before it exits, with the version, platform, panic message, source location and a backtrace. The five newest reports are kept.

On Windows, a crash also leaves a minidump, `crash-….dmp`, including crashes such as access violations or stack overflows. The three newest dumps are kept. On macOS and Linux, a crash from a signal such as `SIGSEGV` leaves no report here.

Crash files never leave your machine. A minidump holds part of the app's memory, so attach one only when asked.

## Reporting a bug

File issues at [github.com/sheprd-labs/sheprd/issues](https://github.com/sheprd-labs/sheprd/issues). To attach the useful details, create a diagnostic bundle:

```sh
sheprd diagnostics bundle PATH
```

It writes a redacted report with the three newest crash reports, redacted like the log, and lists any minidumps by name and size without including them. It works without the app running. Include your platform, how you installed Sheprd, and whether you use Herdr 1 or herdr-core.

Problems with the Herdr daemon itself, rather than Sheprd, belong with [Herdr](https://herdr.dev/). Sheprd is an independent project and is not supported by Herdr.

## Connection problems

Connection failures show in the status bar and on the host's row. The status dot pulses amber while the local daemon starts and is red when disconnected.

- **Retry now.** Each connection reconnects on its own with backoff. **Terminal > Reconnect** retries the selected host immediately. The command palette's **Detach** disconnects without closing the window or stopping the daemon, and pauses retries until you reconnect.
- **A host that drops** stays selected while it reconnects. Its last picture stays on screen, dimmed, under a card with the reason and a **Reconnect now** button. Keys you type meanwhile are not sent, and a flash says so.
- **SSH hosts** need Herdr installed on the host, authentication that does not prompt, and a host key you already trust. The reconnect card names why SSH refused: an untrusted host key, failed authentication, an unreachable host, or no Herdr installed. For hosts that need MFA or a password, set `ControlMaster auto` with a `ControlPath` in `~/.ssh/config` and run `ssh HOST` once in a terminal; Sheprd reuses that connection while it lives (not on Windows). See [SSH hosts](ssh-hosts.md).
- **After sleep,** every SSH link must answer within three seconds or reconnects, and dropped hosts are dialled at once.

Closing or reconnecting the app never stops your daemon, so your terminals are safe while you retry. For daemon-side problems, such as a session that will not start, use [Herdr's troubleshooting guide](https://herdr.dev/docs/troubleshooting/).

## Settings that will not load

If a settings change does not take effect, Sheprd keeps the last valid configuration and shows a load error naming the problem. `sheprd config check` lists invalid settings.

On macOS and Linux, a config file or folder that other users can write to is refused, and the error names the path and its mode. Remove the write permission, then click **Reload** in Settings:

```sh
chmod go-w ~/.config/herdr ~/.config/herdr/config.toml
```

If the next error names another path, fix that one too. Sheprd never changes permissions for you. See [Configuration](configuration.md).

## Git says "Operation not permitted" on macOS

If Git in a local Sheprd terminal reports `Operation not permitted` even though the relevant Files & Folders permission is enabled, restarting Sheprd may not help. The terminals run in the daemon, which keeps running when the app quits.

One reported case was fixed by restarting the default local daemon. The cause is not established, so this is not a guaranteed fix, and it ends the processes running in that daemon's panes:

1. Save your work and stop any running tasks.
2. Quit Sheprd.
3. From another terminal, stop the default local daemon:

   ```sh
   herdr server stop
   ```

4. Reopen Sheprd and retry `git status`.

If the error remains, look into the permission or repository separately. These steps apply to the default local Herdr session only, not to named sessions, SSH hosts or other devices. For those, see [Herdr's troubleshooting guide](https://herdr.dev/docs/troubleshooting/).

## Platform-specific issues

- **Linux: blank window for a moment.** On a laptop with two GPUs, install the Vulkan driver for the GPU your display uses. See [Install](install.md).
- **Linux: no notifications.** System notifications need a freedesktop notification daemon.
- **macOS: no Dock badge or notifications.** Allow notifications, and **Badges**, for the app in System Settings > Notifications.
- **Windows: local connection fails.** Include the log records for the failure in your report. They name the failing operation, such as `pipe_connect`, and the native error code.

See [Platforms](platforms.md) for features that are unavailable on some platforms.
