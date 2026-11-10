---
title: Browser tabs
blurb: Pages beside the terminal, with notes for the agent.
status: shipped
platforms: macOS, Windows
---

# Browser tabs

Browser tabs show web pages and local files next to a workspace's terminals. This page covers opening and using them, annotating a page to send notes to an agent, how local files are served, and the commands agents use to open pages for you.

> **Linux.** Linux has no embedded pages yet. Browser tab requests open your system browser, and local files and annotations are unavailable.

## Opening a browser tab

Browser tabs belong to Sheprd alone: the Herdr daemon, its TUI and other clients never see them. Every window lists the same tabs for a workspace. Tabs are saved and come back after a restart; closing a workspace in Herdr removes its tabs.

Open one:

- from an editor group's **…** menu, or **New Browser Tab** in the command palette (Cmd-Shift-B on macOS, Ctrl+Alt+Shift+B on Linux and Windows)
- by Cmd-clicking (Ctrl-clicking elsewhere) a link in a terminal, with `open_links_in = "browser-tab"` (see [Terminals](terminals.md#terminal-links))
- from an agent, with `sheprd browser open` (below)

A new tab starts blank with its address field focused, and lists the workspace's listening ports: click one to load its page.

## Using a tab

- The toolbar has back, forward, reload, the address field, and a button that opens the page in your system browser. Bare hosts work: `localhost:3000` becomes `http://localhost:3000/`.
- The tab shows the page's favicon, or a globe until it has one.
- While a browser tab is focused, Increase and Decrease Font Size (Cmd-= and Cmd-- on macOS) zoom the page a step, from 50% to 300%, and Reset Font Size (Cmd-0) returns it to 100%. Each tab keeps its own zoom. Click the zoom shown in the toolbar to return to 100%.
- Close Tab and Close Pane close the browser tab being shown, without asking.
- Next Tab, Previous Tab and the tab numbers count browser tabs too, after the workspace's Herdr tabs.

Only `http` and `https` pages open, plus local files an agent shows. Pages cannot navigate to other schemes, downloads are refused, and a page's new windows open as new browser tabs.

Pages are native web views: WebKit on macOS and WebView2 on Windows. On Windows, a page steps aside while a menu, popover, dialog, toast or notice card would fall over it, and comes back when it closes.

To place a browser tab beside a terminal, split the view into editor groups; see [Editor groups](editors.md#editor-groups).

## Annotating a page

**Annotate** in a browser tab's toolbar lets you pin notes to a page and send them to the agent that opened it, so it can change the page. Bind `toggle_annotate` under `[keybindings]` to toggle it from the keyboard; it has no default key.

- Hover to outline an element and click to pick it, or select text. For anything else, draw a region: turn on **Region** in the notes panel and drag, or Shift-drag at any time. Links do not follow while you annotate.
- **Note on page** writes a note about the whole page.
- Escape drops the current pick; a second Escape stops annotating.
- On macOS, each pick also takes a screenshot of that part of the page, attached to the note. Windows notes have no screenshots.

Notes are written in a panel beside the page, never in the page itself. Queued notes are numbered on the page; up to 20 wait per tab.

### Sending notes

**Send to agent** turns your notes into one prompt: the page, then for each note the element's selector path, its text and a short HTML snippet (or, for a region, its position and what it covers), its screenshot, and your note. **Copy** puts the same prompt on the clipboard instead. Tabs you opened yourself have no agent, so only **Copy** is offered.

The prompt reaches the agent one way only:

- An agent waiting in `sheprd browser feedback --wait` receives it there.
- Otherwise it is typed into the agent's pane and submitted once Herdr reports the agent idle (or still working after two minutes).
- It is never typed into a pane where no agent is running, or into an agent that is asking you a question. Those notes wait for `browser feedback`.

When the agent takes the notes, its tab comes forward so you see it pick them up. While a batch waits for a busy agent, **Withdraw** takes it back so you can edit it. Sent notes stay listed and dimmed: edit one and Send delivers only the new or changed notes, and **Clear sent** ends the round.

### Results from a page

A local page can queue a result for its agent, such as the mockup variant you picked, with `herdr.send(summary, data)`. It appears in the notes panel as **Page result** and goes to the agent only when you press Send. The summary is one line of at most 300 characters; data over 8 KB is refused.

## Local pages

`sheprd browser open ./mockup.html` shows a local file. Sheprd serves it, with the other files in its folder, so relative links to CSS, scripts and images work:

- Only that folder is served. Hidden files and folders, and files a link leads out of the folder to, are never served. A folder holding your home directory is refused.
- Web pages cannot load the folder's files.
- Files are never cached, so `sheprd browser reload` shows an edited file.
- A Markdown file (`.md`, `.markdown`) opens rendered. Its scripts never run.

Local pages can load a small kit at `/.herdr/kit.css` and `/.herdr/kit.js` for interactive explainers and mockups, and `/.herdr/theme.css` for the window's theme colors. `sheprd browser example` prints a complete page built on it.

## Letting agents open pages

Agents running in Sheprd's panes can open a page for you:

```sh
sheprd browser open http://localhost:3000
sheprd browser open design/mockup.html      # in a split beside the agent's terminal
sheprd browser open --no-focus https://example.com/docs
sheprd browser open --tab design/mockup.html # a tab, like a web address
sheprd browser reload                       # reload the pages this pane opened
sheprd browser feedback --wait 600          # wait for the notes you send it
sheprd browser --help
```

The tab joins the agent's own workspace, and notes on the page go back to that agent's pane.

- A local file opens in a split beside the agent's terminal, which keeps the keyboard so you can keep talking to the agent. A web address opens as a tab. `--beside` and `--tab` choose the other way.
- `--no-focus` adds the tab without switching to it.
- Opening the same file again from the same pane reuses its tab.

Other `sheprd browser` commands list tabs, navigate, zoom, read a page's text, or save a screenshot (macOS); see [CLI](cli.md). The command exits with 3 when Sheprd is not running.

### Agents on SSH hosts

An agent on a saved SSH host cannot reach this machine directly. Sheprd puts a small shim on the host at `~/.local/state/herdr/gpui/bin/sheprd`, and a window connected to that host opens the page:

```sh
~/.local/state/herdr/gpui/bin/sheprd browser open localhost:3000
~/.local/state/herdr/gpui/bin/sheprd browser open --no-focus https://example.com/docs
~/.local/state/herdr/gpui/bin/sheprd browser reload
```

- An address on the host's own loopback (`localhost`, `127.0.0.1`, `[::1]`, `0.0.0.0`) is forwarded to this machine first.
- Only `browser open` and `browser reload` work from a host; every other command exits 8. Files on the host are not shown: serve their folder (for example `python3 -m http.server 8000`) and open its port.
- Notes you send on such a page are typed into the agent's pane once it is idle; `browser feedback` cannot reach the app from the host.
- In a Herdr 2 terminal on the host, the shim leaves the request as labels on that terminal, and a window showing the terminal opens the page in the tab's workspace and removes the labels. `browser open` works there; `browser reload` exits 8, and notes do not return to the agent. Requests left while no window showed the terminal are not opened later.

### The agent skill

Agents only use what they know about, so Sheprd offers, once, to install a `herdr-gpui-browser` skill that teaches them these commands. It goes into `~/.claude/skills/` and `~/.agents/skills/`, for whichever of `~/.claude` and `~/.agents` exists. Claude Code reads the first, Codex the second, and opencode both. Once you agree, Sheprd keeps it up to date, and also puts it and the shim on each saved SSH host it connects to.

- **Not now** is remembered, and the offer is not repeated.
- **Settings > General**, or **Install Browser Skill for Agents** in the command palette, installs or removes it at any time.
- `[agent_skill] install` in `config-gpui.local.toml` overrides your answer: `"ask"` (the default), `"local"` (never touches SSH hosts), or `"off"` (never offers, refreshes or removes it).

A skill of the same name that you wrote yourself is left alone. `sheprd browser skill` prints the skill's text.
