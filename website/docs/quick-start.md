---
title: Quick start
blurb: From first launch to your first agent.
---

# Quick start

This page walks you from opening Sheprd for the first time to an agent running in its own worktree. It assumes you have already [installed](install.md) the app.

## First launch

Open Sheprd from the Applications folder, your applications menu, or the Start menu. It connects to a terminal daemon on its own:

- If [Herdr 1](https://herdr.dev/) is installed, Sheprd connects to its session and starts your installed `herdr server` when the session is not running.
- If it is not, Sheprd opens a herdr-core shell using the herdr-core bundled with the app.

The status dot in the status bar pulses amber while the local daemon starts and turns red when it is disconnected. A healthy connection leaves the status bar quiet.

Closing the window or quitting the app only detaches. Your terminals and agents keep running in the daemon, and Sheprd reopens the windows you had open, at the same size and position.

## Open a project

Each repository or folder you work in is a workspace in the sidebar.

- Click **+ New Workspace** in the sidebar, or use **File > New Workspace**.
- Or open the [command palette](#the-command-palette) on **Projects** and pick a folder.
- From a terminal, run `sheprd open DIR`. It focuses the workspace already rooted in that folder or starts one, and starts the app first when it is not running.

On Windows, the installers also add **Open in Sheprd** to folders in File Explorer.

The sidebar shows each workspace with its branch, the cached pull request number, diff counts, and a status dot for the agents in it. The dot comes from the daemon itself, so Sheprd and the Herdr terminal client always show the same state.

## Create a worktree

Worktrees let several agents work on the same repository without stepping on each other. Linked worktrees nest under their main checkout in the sidebar.

1. Focus the repository's workspace and press Cmd-N on macOS or Ctrl+Shift+N on Linux and Windows (**File > New Worktree...**).
2. Type a branch name, or give the worktree a name and let the dialog propose a branch such as `worktree/fix-login`.
3. In the **Opens in** row, choose **Terminal** or an agent installed on the host, such as `claude` or `codex`.
4. Confirm. The new checkout opens as its own workspace.

When you pick an agent, Sheprd starts it in the new checkout's shell once the worktree exists. This choice is offered on macOS and Linux clients; on Windows the new worktree opens a terminal. See [Worktrees](worktrees.md) for scripts, notes and cleanup.

## Start an agent

There are three ways to get an agent running:

- **In a terminal.** Open a tab (Cmd-T on macOS, Ctrl+Shift+T on Linux and Windows) and run your agent CLI as usual, for example `claude` or `codex`. Split with **Split Right** or **Split Down** to run several side by side.
- **In a new worktree.** Pick the agent in the **Opens in** row, as above.
- **As a chat.** A tab group's **…** menu lists **New Chat** with one row per supported agent (Claude, Codex, Copilot, Gemini, OpenCode, Pi). Sheprd draws the conversation natively. The agent itself must be installed. See [Chat](chat.md).

Agents report their state to the daemon: working, blocked (waiting on you), done, or idle. You see it on tabs and sidebar rows, and as a count on the macOS Dock icon or the Windows taskbar button. [Notifications](notifications.md) covers toasts, system notifications and sounds.

## The activity bar

The icon column at the window's left edge picks what the window shows:

- **Workspaces** is the sidebar beside your terminals. Click it again to fold the sidebar.
- **Overview** lists what needs you on every connected host: agents waiting on you and failed runs first, then finished agents to review, working agents, and idle ones.
- **Pull requests** and **Issues** list every repository your workspaces hold. See [Orchestrator](orchestrator.md).
- **Usage** shows each host's plan limits. See [Usage](usage.md).

**Settings** sits at the foot of the bar. To hide the bar, turn off **Settings > Sidebar > Show activity bar**.

## The command palette

Double-tap Shift, or press Cmd-Shift-P on macOS (Ctrl+Shift+P on Linux and Windows), to open the command palette. It searches workspaces on every connected host, agents and terminal panes, app commands, daemon commands, and local project folders.

- Switch between **All**, **Navigation**, **Commands** and **Projects**, or cycle them with Ctrl-Tab.
- Search terms work like fzf: every space-separated term must match.
- **Navigation** previews the selected pane beside the list when the window has room.
- **Resume Palette** (Cmd-Shift-R on macOS, Ctrl+Alt+Shift+R on Linux and Windows) reopens it with your last query.

Every shortcut can be changed in **Sheprd > Keyboard Shortcuts**. See [Keyboard](keyboard.md).

## Settings

Open Settings with Cmd-, on macOS or Ctrl+Shift+, on Linux and Windows. It opens in its own window, and the terminal stays usable while it is open. A search box above the pages finds any setting.

Each row is tagged with the app that reads it: **Herdr** for settings shared with Herdr's own terminal client, and **Sheprd** for settings only this app reads. Changes save automatically. See [Configuration](configuration.md) for the files behind them.

## Where to go next

- Add another machine: [SSH hosts](ssh-hosts.md) and [Cloud](cloud.md).
- Review what an agent changed: [Review](review.md).
- Pick a theme and layout: [Appearance](appearance.md).
- Let agents drive the app: [Command line](cli.md).
