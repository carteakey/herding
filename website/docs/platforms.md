---
title: Platforms
blurb: What differs on macOS, Linux and Windows.
platforms: macOS, Linux, Windows
---

# Platforms

Most of Sheprd works the same on macOS, Linux and Windows. This page lists the requirements for each platform and the features that differ, so you know what to expect before you install.

## Requirements

| Platform | Requirement |
| --- | --- |
| macOS | macOS 14.2 Sonoma or newer, Apple Silicon or Intel. Signed, notarized universal app. |
| Linux | x86_64 or ARM64 with glibc 2.39 or newer (Ubuntu 24.04, Debian 13, Fedora 40, current Arch, or later), and a Vulkan driver for your GPU. |
| Windows | Windows 10 or 11, x86_64 or ARM64. |
| Phones | Any modern browser for the remote control web app. The native iPhone and iPad app needs iOS 26 and is in TestFlight beta. |

## Feature matrix

| Capability | macOS | Linux | Windows |
| --- | --- | --- | --- |
| Browser tabs, page annotations, VS Code and px0 tabs | Yes | No: pages open in the system browser | Yes |
| Page screenshots in browser notes | Yes | No | No |
| Live HTML in chat messages | Yes | No | Yes |
| Teleport, fan-out, Smart Dispatch, moving panes to other devices | Yes | Yes | Not yet |
| Starting an agent in a new worktree | Yes | Yes | Not yet: the worktree opens a terminal |
| Saved SSH hosts | Yes | Yes | Yes, with the differences below |
| Uploading dropped files and images to SSH hosts | Yes | Yes | Not yet |
| Forward port on an SSH host | Yes | Yes | Not yet |
| WSL distributions as devices | No | No | Yes |
| Containers, Coder, Azure and other cloud devices | Yes | Yes | Not yet |
| Count of agents that need you | Dock badge | None | Taskbar overlay, plus progress |
| Launch at login setting | Not yet | Not yet | Yes |
| Editing settings shared with Herdr | Yes | Yes | Read-only |
| Opening a file at a line from a printed path or Go to File | In your terminal editor, in a split | In your terminal editor, in a split | Paths open in the system's app; Go to File and Go to Symbol open code tabs |
| Window blur | Yes | Wayland compositors with KDE's blur protocol | Acrylic or Mica on Windows 11 |

Herdr 1 sessions, herdr-core terminals, agent chats, worktrees, review, the orchestrator, plan usage, mods, automations, the command line, and remote control work on all three.

## macOS

- Shortcuts use Cmd. Cmd-, opens Settings and Cmd-/ the shortcut reference.
- The Dock icon shows how many agents are done or blocked. macOS draws it only when **Badges** is allowed in System Settings > Notifications for the app.
- System notifications ask for permission the first time.
- Browser tabs use WebKit, and each note you pin on a page also saves a screenshot of that part of the page.

## Linux

- Application shortcuts use **Ctrl+Shift**, such as Ctrl+Shift+T for a new tab and Ctrl+Shift+C/V for copy and paste. Super stays free for the desktop and plain Ctrl keys for terminal programs. Ctrl+Shift+U is left unbound for Unicode entry.
- Rendering uses Vulkan, so a Vulkan driver for your GPU is required. On a laptop with two GPUs, install the driver for the one your display uses.
- There are no embedded web pages yet. Browser tab requests open the system browser, and local files and page annotations are unavailable.
- There is no badge with the agent count.
- System notifications use the freedesktop notification service; without a notification daemon nothing appears. Each event shows separately.
- Sound needs the system ALSA library and a default audio device. Custom notification sounds are MP3 only.
- Terminal clipboard paste uses `wl-paste` on Wayland or `xclip` on X11.

## Windows

- Shortcuts start from the Linux profile: Ctrl+Shift+T, Ctrl+Shift+P, Ctrl+Shift+Q. No default uses the Win key. Reset text size is Ctrl+0.
- Windows keep the system title bar, so Snap Layouts and drag-to-snap are the system's own. The title bar follows the app's light or dark theme, and `accent = "system"` takes the accent color from Windows.
- The taskbar button shows the agent count as an overlay and fills in for long operations. The jump list offers recent workspaces and devices.
- Settings shared with Herdr can be read but not saved; Sheprd's own settings stay editable.
- Narrator and NVDA read the terminal's text and the main window's controls. Some larger dialogs and pickers are not fully named yet.
- Agent checkpoints are not available for SSH hosts from a Windows client. They work in WSL distributions.

### SSH hosts on Windows

Saved SSH hosts work through OpenSSH for Windows, with these differences:

- **No terminal setup.** A host that needs a password prompt, your key installed, or Herdr installed or updated cannot be set up in the app. The dialog says what is needed and asks you to run `herdr machine add TARGET` in a terminal, then add the device again.
- **No connection sharing.** Windows OpenSSH has no `ControlMaster`, so each connection authenticates on its own. Keep a key in the OpenSSH Authentication Agent service (`ssh-agent`).
- **No Forward port** in a device's menu. A listening port found on the host still opens.
- **No uploads** of files and images dropped or pasted on an SSH host's pane.
- Agent checkpoints, Teleport, fan-out, process lists and pane moves are unavailable from Windows.

See [Windows](windows.md) for WSL distributions, the installers and the rest of the platform's details.

## iPhone and iPad

The native app is a [TestFlight beta](https://sheprd.org/testflight/) for iOS 26. It pairs with Sheprd on your computer; nothing runs on the phone itself. Any phone can also use the remote control web app in its browser. See [Remote control](remote-control.md).
