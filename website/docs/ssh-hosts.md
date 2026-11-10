---
title: SSH hosts
blurb: Your build box in the same window.
status: shipped
platforms: macOS, Linux, Windows
---

# SSH hosts

Sheprd shows the terminals and agents of other machines in the same window as your own. This page covers adding a machine over SSH, how the connection works, what stays local, ports, machine load, and what happens when a connection drops.

## How it works

Each SSH host runs its own Herdr daemon, and that daemon owns the host's terminals and agents. Sheprd connects to it by running Herdr's remote bridge over SSH from your computer; the remote daemon never proxies anything, and nothing on your computer has to stay open for the host's agents to keep working. Closing Sheprd leaves every daemon and its terminals running.

A host needs:

- an installed POSIX Herdr,
- noninteractive SSH authentication (a key or an agent),
- a host key you already trust.

For hosts that need MFA or a password, set `ControlMaster auto` with a `ControlPath` in `~/.ssh/config` and log in once with `ssh HOST` in a terminal. Sheprd reuses that master connection while it lives.

On macOS and Linux, Sheprd's own short commands on a host (Git reads, avatars, usage, scripts) share one SSH login per host, so hosts that rate-limit new SSH connections are not flooded. Large transfers such as [Teleport](teleport.md) get a second login of their own, and terminals keep their own connection. These shared logins never touch a master of yours, and they end with the app.

Set `ForwardAgent yes` only for hosts you trust. Herdr servers that support it then keep remote panes' `SSH_AUTH_SOCK` working across reconnects.

## Adding a machine

Choose **Add Device…** in the device picker at the bottom left of the sidebar. One dialog adds every kind of device: a rail on its left lists SSH hosts (then WSL distributions on Windows) and, under **Cloud**, the cloud providers you use (see [Cloud machines](cloud.md)).

The SSH form takes an SSH target, an optional label, and an optional remote session (default: `default`). An empty label names the device after the target as you typed it, such as `user@host`.

### Nearby devices

Under the SSH target, **Nearby devices** suggests hosts to add. Opening the dialog starts a search of about ten seconds; **Search again** repeats it. Suggestions come from:

- **SSH config**: concrete `Host` aliases in `~/.ssh/config` and the files it `Include`s. Patterns, `Match` blocks, and git hosting services are skipped.
- **Tailscale**: online Linux, macOS, and BSD peers from `tailscale status --json`, by their MagicDNS name.
- **Bonjour**: hosts that advertise `_ssh._tcp` on the local network, such as a Mac with Remote Login on.

Nothing is scanned, so a host that runs SSH without announcing it appears only through your SSH config or Tailscale. Typing filters the list; **Tab** completes the dimmed rest of a match, and **Up** and **Down** pick a row. **Enter** otherwise adds exactly what the field shows.

> **macOS 15 and later.** The first search asks for Local Network access. If you denied it, **Open Local Network Settings** in the dialog goes to the setting; turn on Herdr there and search or add again, without restarting the app.

### What Add device checks

**Add device** first checks the host over non-interactive SSH. It never installs or starts anything while checking.

- **Herdr running, or installed but stopped**: the device is saved. A stopped server is started.
- **Herdr missing**: the dialog asks "Herdr was not detected on the host. Should we install it?" An outdated Herdr asks to update it instead.
- **SSH needs a prompt** (unknown host key, password, passphrase), or the check failed: the dialog offers to continue in a terminal.

Accepting the setup opens a new workspace on your local Herdr and runs the setup there, with your own `ssh`, SSH config, and keys, exactly as a terminal would. If the host refused every key, it runs `ssh-copy-id` first. Answer any prompts in that workspace. Sheprd collects no credentials; the saved device appears on its own once setup succeeds.

### Skipping host key verification

**Skip host key verification (dangerous)**, off by default, connects without checking the host's key, now and on every later connection. It is meant for hosts that keep no key to check, such as the `*.coder` aliases `coder config-ssh` writes. The choice is saved under `[ssh] skip_host_key_verification` in the local config, by target. To verify the host again, delete the target from that list, or remove the device and add it again with the box clear.

## Managing saved hosts

Each device's header in the Spaces list starts with a mark: a monitor for this machine, a provider logo for a cloud device, and a terminal for an SSH host or WSL distribution. A host whose name ends in `.coder` shows the Coder logo, and a `*.ts.net` name or Tailscale address shows the Tailscale logo. Turn off **Show device icons** under Settings > Sidebar, or set `show_device_icons = false`, to hide the marks.

Right-click a saved SSH device's header to:

- **Rename** it. An empty name falls back to the SSH target.
- **Remove device…** to forget it. This only edits your local list; the host's own Herdr keeps running.
- **Forward port…** (see below).
- **Use server keybindings** (see below).

The device picker lists **All Devices**, **Local**, and your saved devices. `Cmd`-click (`Ctrl`-click on Linux and Windows) several devices to show only those. Each device row has a **Session** chip: click it to see that device's sessions, attach to one, or **Add session…**.

### Devices overview

**View > Devices Overview**, the activity card at the top of the device picker, or **Devices Overview** in the command palette opens a tab listing every device the window connects to:

- agent activity for the last two hours, working and blocked, per device;
- totals of working, blocked, and idle agents, and how many devices are online;
- one row per device with CPU, memory, free disk space in the home volume, and uptime.

The history starts empty each time the window opens; nothing is saved. `sheprd devices` opens the same tab.

## Settings that stay local

Keybindings stay local on an SSH device, as with Herdr's default `--remote-keybindings local`. **Use server keybindings** in a device's right-click menu applies the `[keys]` profile that device's server publishes while it is selected, like `herdr --remote-keybindings server`. The choice is saved per device in `config-gpui.local.toml`:

```toml
[devices.ID]
keybindings = "server"
```

Only keybindings follow the server. Themes, the sidebar, and toasts stay local, and Sheprd never reads a remote host's `config.toml` for them. Pane borders, gaps, and scrollbars are drawn by each daemon, so they follow that host's own Herdr config.

One exception: **Settings > General > Session restore** lists a switch per connected SSH host. It edits only `[experimental] pane_history` in that host's Herdr config, keeping comments and other keys, then asks that host's daemons to reload. It applies to the host's Herdr tabs: a Herdr 2 terminal's output stays in the host's herdr-core while it runs, whatever the switch says, so a host reached only through herdr-core has no switch.

## Ports and forwarding

Workspaces that run a server show the ports it listens on as `:3000` chips under the workspace's sidebar row and, for the focused workspace, in the status bar. A port belongs to a workspace only when its process was started from one of that workspace's panes, so system services never show.

Clicking a chip on a remote host opens the page at `http://127.0.0.1:<port>` in a browser tab through an SSH tunnel started on the first click. The local port keeps the server's number when it is free; a privileged port moves up by 10000. The tunnel closes when the port stops listening, the host disconnects, or the window closes. Turn off **Show listening ports** in Settings, or set `show_listening_ports = false`, to hide them and stop scanning.

**Forward port…** in a device's menu forwards any port of the host to this computer, such as a dev server an agent started. The menu lists each forward as connecting, listening (with **Open**), or ended with the reason. Nothing reconnects: a forward ends when you stop it, when its host is disabled or removed, when the window closes, or when the app quits. From the command line:

```sh
sheprd ports
sheprd ports forward 5173
sheprd ports open 5173
sheprd ports unforward 5173
```

## Machine load

The status bar shows the selected host's CPU and memory, with cores, load averages, memory, free disk space, and uptime in its tooltip. With more than one host, each host row in the sidebar shows its own. Each connected Linux or macOS host is read every two seconds over its own SSH shell; other remote systems show it as unavailable. Turn off **Show CPU and memory** in Settings, or set `show_system_load = false`, to hide it.

GPU and video memory are off by default. Turn on **GPU and video memory** in Settings > Status bar (`show_gpu_load = true`) or **Show GPU and video memory** in Settings > Sidebar (`show_sidebar_gpu_load = true`). `sheprd system` prints the selected host's load.

## Dropped connections

Each host reconnects on its own, with backoff capped at 30 seconds. A selected host that drops stays selected: its last picture stays up, dimmed, under a card with the reason and a **Reconnect now** button, until the new connection draws its own frame. Keys typed meanwhile are not sent, and a flash says so.

The card names why SSH refused a host: an untrusted host key, failed authentication, an unreachable host, or no Herdr installed. Those need you, so such hosts wait the full 30 seconds between retries. After your computer sleeps, every live SSH link must answer within three seconds or reconnect, and dropped hosts are dialled at once.

**Terminal > Reconnect** retries the selected host immediately. **Detach** in the command palette disconnects it without stopping its daemon and pauses retries until you reconnect.

## Files dropped on a remote pane

Drop regular files onto an SSH pane to copy them to that host. A **Copying...** card shows progress and a Cancel button. Once every file arrives, their remote paths are pasted into the pane. Files land in a private `herdr-upload.*` directory under the remote `${TMPDIR:-/tmp}` and stay until you or the remote system removes them. Directories are rejected.

## On Windows

SSH hosts work through OpenSSH for Windows, with these differences:

- **No terminal setup.** A host that needs a password prompt, your key installed, or Herdr installed or updated cannot be set up from the dialog. It names what is needed; run `herdr machine add TARGET` in a terminal, then add the device again.
- **No connection sharing.** Windows OpenSSH has no `ControlMaster`, so each connection authenticates on its own. Keep a key in the OpenSSH Authentication Agent service (`ssh-agent`).
- **No Forward port… menu.** A listening port found on the host still opens.
- **No uploads.** Files and images dropped on an SSH pane are not copied yet.
- **No Teleport, fan-out, checkpoints, process lists, or pane moves** for SSH hosts from a Windows client.

See [Windows](windows.md) for the rest.
