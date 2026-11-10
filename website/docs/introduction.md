---
title: Introduction
blurb: What Sheprd is and how it works with Herdr.
---

# Introduction

Sheprd is a native desktop app for running terminals and coding agents on macOS, Linux and Windows. This page explains how it relates to Herdr, what you can do with it, and what you need to run it.

## What Sheprd is

Sheprd is a workspace for terminals and AI agents, written in Rust with GPUI. You organize repositories, worktrees, tabs and splits; chat with agents, review their changes, track pull requests and plan usage, and work across local, SSH and cloud machines. Browser previews, automations, mods and phone remote control connect those workflows in one app.

Sheprd was previously called Herdr UI (and Herdr GPUI). Existing settings, sign-ins and installs carried over with the rename.

> **Note.** Sheprd is an independent project. It is not affiliated with, endorsed by, or supported by Herdr or [herdr.dev](https://herdr.dev/).

## How it works with Herdr

Sheprd does not run your terminals itself. A Herdr daemon owns the terminal processes, and Sheprd draws them natively and sends your input back. Two kinds of daemon are supported, side by side in the same workspaces:

- **Herdr 1**, the `herdr server` daemon from [herdr.dev](https://herdr.dev/). If it is installed, Sheprd connects to its sessions and may start your installed `herdr server` when the session it wants is not running.
- **herdr-core** ("Herdr 2"), which streams raw terminal output that Sheprd emulates and draws with the same painter. Every release bundles a pinned herdr-core, so you do not need to install Herdr separately. If Herdr 1 is not installed, Sheprd opens a herdr-core shell.

Because the daemons own the terminals, closing Sheprd leaves your sessions and agents running. Reopen the app and they are still there. Sheprd never upgrades or migrates your personal Herdr daemon, and it never moves existing Herdr 1 sessions to Herdr 2.

## What you can do

- **Terminals and agents.** Native panes for the agent CLIs you already use, with agent status on every tab and sidebar row. See [Terminals](terminals.md).
- **Native agent chat.** Run Claude, Codex, Copilot, Gemini, OpenCode or Pi as a chat drawn by the app. See [Chat](chat.md).
- **Worktrees.** One sidebar row per checkout, with pull request number, diff counts and agent status. See [Worktrees](worktrees.md).
- **Review.** Read an agent's diff, leave notes, commit, push and open the pull request. See [Review](review.md).
- **Issues and pull requests.** Dispatch an agent from an issue and track pull requests across repositories. See [Orchestrator](orchestrator.md).
- **Other machines.** SSH hosts, WSL distributions on Windows, containers and cloud sandboxes, and moving work between them. See [SSH hosts](ssh-hosts.md), [Cloud](cloud.md) and [Teleport](teleport.md).
- **Browser tabs.** Web pages beside your terminals, with notes you send to the agent. See [Browser](browser.md).
- **Usage and load.** Plan limits for AI services and CPU and memory per machine in the status bar. See [Usage](usage.md).
- **Extend it.** Luau [mods](mods.md), scheduled [automations](automations.md), and a [command line](cli.md) agents can use to drive the app.
- **From your phone.** [Remote control](remote-control.md) from a phone browser, and a native iPhone and iPad app in TestFlight beta.

Some features differ by platform. [Platforms](platforms.md) lists the differences.

## Requirements

| Platform | Requirement |
| --- | --- |
| macOS | macOS 14.2 Sonoma or newer, Apple Silicon or Intel |
| Linux | x86_64 or ARM64, glibc 2.39 or newer, a Vulkan driver for your GPU |
| Windows | Windows 10 or 11, x86_64 or ARM64 |
| iPhone and iPad | iOS 26, through the TestFlight beta |

Herdr 1 is optional. Releases include herdr-core for local terminals. To use SSH hosts, Herdr must be installed on each host.

## Next steps

- [Install](install.md) Sheprd on your platform.
- Follow the [quick start](quick-start.md) from first launch to your first agent.
- Read [Configuration](configuration.md) to see where settings live.
