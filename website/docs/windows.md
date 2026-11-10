---
title: Windows
blurb: Install, WSL, and what differs on Windows.
platforms: Windows
---

# Windows

Sheprd runs natively on Windows 10 and 11, on x86_64 and ARM64. This page covers installing it, WSL distributions as devices, SSH hosts, the window and taskbar, screen readers, saved sign-ins, and what is not available on Windows yet.

## Install

In PowerShell, on x86_64 or ARM64:

```powershell
irm https://sheprd.org/install.ps1 | iex
```

The script downloads the zip for your architecture, checks its published SHA-256, installs `sheprd.exe` and `sheprd-launcher.exe` under `%LOCALAPPDATA%\Programs\Herdr UI`, adds that directory to your user `PATH`, and creates a **Sheprd** Start menu shortcut. It never starts or stops terminal daemons. The folder keeps its old name so existing installs keep working.

Prefer a regular installer? Each [release](https://github.com/sheprd-labs/sheprd/releases/latest) has `Sheprd-VERSION-x86_64-pc-windows-msvc-setup.exe` and `Sheprd-VERSION-aarch64-pc-windows-msvc-setup.exe`. They install the same files to the same place for the current user, without administrator rights, and add an uninstaller under **Settings > Apps**.

Both add **Open in Sheprd** to folders in File Explorer. On Windows 11 it is under **Show more options**. To remove everything the script added (settings and daemon sessions are kept):

```powershell
& ([scriptblock]::Create((irm https://sheprd.org/install.ps1))) -Uninstall
```

### Updates

A signed copy updates itself from **Check for Updates**: it downloads the new installer, checks it against the release's signed manifest and its Authenticode publisher, runs it silently, and reopens. Only the copy in `%LOCALAPPDATA%\Programs\Herdr UI` updates itself. See [Updates](updates.md).

### The launcher and the command line

`sheprd.exe` is a console program, because it is also the command line agents and scripts run. Open the app with `sheprd-launcher.exe`, the Start menu shortcut's target, which starts it without a console window. Pinning a running window to the taskbar pins the launcher as **Sheprd**.

The app runs once per user: a second plain launch brings the running window forward. **Settings > General > Launch at login**, off by default, opens it when you sign in. Task Manager's **Startup apps** can switch that off too, and the setting then says where to turn it back on.

## Local Herdr

Sheprd connects to the Herdr daemon on this machine through the named pipe the Windows daemon binds. When it starts a local `herdr server`, it looks for `herdr.exe`; the daemon survives the app, and no console window appears. Configuration and state live in `%APPDATA%\herdr` and `%LOCALAPPDATA%\herdr`.

`sheprd COMMAND` reaches the running app over a named pipe that only your Windows user can open and that rejects remote clients. See the [command line](cli.md).

If the app reports an access error connecting to the local daemon or locking its config, open the log (`sheprd logs`) and include the nearby `operation`, `kind`, and `raw_os_error` fields in an issue. This is a known, still unresolved report ([issue #357](https://github.com/sheprd-labs/sheprd/issues/357)).

## WSL distributions

A Herdr daemon inside a WSL distribution is reachable as a device. Choose **Add Device…** in the device picker: it lists the installed distributions (Docker Desktop's own excluded) and, for the one you pick, checks Herdr inside it and saves it once a compatible copy answers. Install Herdr inside the distribution first; Sheprd never installs or upgrades it there. A stopped server is started on the first connection.

Terminals, the sidebar, and the session list then work as they do for an SSH host. A distribution also gets:

- **CPU and memory** in the status bar and on its sidebar row. Its CPU figure is what its processes use, and its memory is what WSL has given it.
- **Plan usage**, read from the distribution's own agent sign-ins, which never leave it.
- **Listening ports**, opened at `http://localhost:<port>` with no tunnel, since WSL forwards them to Windows. The device menu offers no port forwarding.
- **Agent checkpoints**, run with Git inside the distribution.
- **File drops** pasted as the distribution's own paths: `C:\Users\me\a.txt` becomes `/mnt/c/Users/me/a.txt`. Nothing is copied, so a file of any size is ready at once.

Right-click a distribution's sidebar header to remove it, which only forgets it here. Teleport is not offered for distributions. Worktree scripts do not run on WSL.

## SSH hosts

Saved [SSH hosts](ssh-hosts.md) work through OpenSSH for Windows: the optional-feature copy at `%SystemRoot%\System32\OpenSSH\ssh.exe` first, otherwise the first `ssh.exe` on your `PATH`. **Add Device** lists **SSH host** first, then **WSL**. It asks before trusting an unknown host key and saves a host that already runs a compatible Herdr. Sessions on the host can be listed, created, and deleted. CPU and memory, plan usage, and listening ports work as on macOS and Linux.

Where Windows differs:

- **No terminal setup.** A host that needs a password prompt, your key installed, or Herdr installed, updated, or restarted cannot be set up from the dialog. It says what is needed and asks you to run `herdr machine add TARGET` in a terminal, then add the device again.
- **No connection sharing.** Windows OpenSSH has no `ControlMaster`, so each connection authenticates on its own. Keep a key in the OpenSSH Authentication Agent service (`ssh-agent`).
- **`ProxyJump` and `ProxyCommand`** come from your SSH config and run as Windows OpenSSH runs them.
- **No Forward port…** in a device's menu. A listening port found on the host still opens.
- **No uploads.** Files and images dropped or pasted on an SSH pane are not copied yet.

## The window and taskbar

Every window keeps the title bar Windows draws, so its buttons, Snap Layouts, drag-to-snap, and the window menu are the system's own. The title bar follows the app's light or dark theme. On Windows 11 a translucent window blurs with acrylic or Mica (**Settings > Theme > Window**), and `accent = "system"` takes the accent from Windows. See [Appearance](appearance.md).

Shortcuts use Ctrl, not the Win key, with the Linux defaults (Ctrl+Shift+T, Ctrl+Shift+P, Ctrl+Shift+Q). See [Keyboard](keyboard.md).

- **Taskbar badge.** While agents are done or blocked, the taskbar button shows their number, `9+` past nine.
- **Taskbar progress.** The button fills while a window copies files to an SSH host, teleports a worktree, moves a pane, or installs an update.
- **Jump list.** Right-click the taskbar button or Start menu entry for **Recent workspaces** and a **Show DEVICE** task per device. Recent workspaces appear only while Windows' **Show recently opened items in Start, Jump Lists, and File Explorer** setting is on.
- **Links.** The installers register `sheprd://` links for your user, and the older `herdr-gpui://` ones keep working. `sheprd://open?path=C:\src\repo` shows a local checkout, and `sheprd://device?id=ID` switches to a device. A link never runs a command, and network paths are refused.

## Screen readers

Narrator and NVDA can read the main window. They get:

- the terminal as a document named **Terminal**, with the rows on screen and the selection;
- the menu bar and its menus, with each item's name, shortcut, and state;
- the title bar buttons, the tab strip as a tab list named **Tabs**, and the activity bar;
- the sidebar's search field, Workspaces and Agents lists, and buttons;
- the status bar's readings and buttons;
- context menus and dialogs by their titles.

Settings, Logs, and the Orchestrator name their controls too. With a Windows contrast theme on, the app's own marks and labels use its **High contrast** level; terminal colors are unchanged.

Tab belongs to the terminal, so the rest of the window is reached a region at a time. **F6** moves from the terminal to the title bar, the sidebar, and the status bar, then back; **Shift+F6** goes the other way. Inside a region, Tab and Shift+Tab move between controls, Enter or Space uses one, and Escape returns to the terminal. **F10** opens the menu bar.

Still missing: most rows and buttons inside the larger dialogs and pickers (Add Device, the command palette's results, themes, checkpoints, fan-out, Teleport, pull requests), the Orchestrator's detail pane and dispatch dialogs, and the font and color pickers in Settings have no screen reader roles yet.

## Saved sign-ins

GitHub sign-ins are kept in Windows Credential Manager, listed under **Windows Credentials** in Control Panel. They stay on this computer for your Windows account only and never roam to other computers in a domain. Signing out in the app removes them. There is no private-file store on Windows, so `allow_plaintext_credentials` changes nothing, and `GH_TOKEN` or `GITHUB_TOKEN` still take precedence for the main account.

A crash leaves a minidump and a text report beside the log. Nothing is uploaded.

## Not available on Windows yet

- [Cloud machines](cloud.md), from every provider, containers included.
- Teleport, Smart Dispatch, fan-out, and pane moves to other devices ([Work across machines](teleport.md)).
- From SSH hosts: terminal setup, connection sharing, port forwarding from the menu, file and image uploads, agent checkpoints, and process lists.
- Choosing an agent in the New worktree dialog's **Opens in** row.
- Scanning this machine's own listening ports.
