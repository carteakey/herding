---
title: Keyboard and palette
blurb: Default shortcuts, the command palette, and rebinding.
status: shipped
platforms: macOS, Linux, Windows
---

# Keyboard and palette

Every Sheprd command can run from the command palette and be bound to a key. This page lists the most useful default shortcuts on each platform, explains the command palette, and shows how to rebind keys.

## Defaults per platform

Each platform has its own default profile:

- **macOS** uses Cmd.
- **Linux** uses Ctrl+Shift for app actions, leaving Super to the desktop and plain Ctrl keys to terminal programs. Ctrl+Shift+U stays free for GNOME and IBus Unicode entry.
- **Windows** starts from the Linux profile, matching Windows Terminal: no default uses the Win key, and plain Ctrl keys still reach terminal programs. Reset Font Size is Ctrl+0, because Windows can take Ctrl+Shift+0 for switching input languages. Alt+F4 closes a window.

The in-app reference lists every command, its default on your platform, and your overrides. Open it with **Sheprd > Keyboard Shortcuts**, or **Keyboard Shortcuts** in the command palette.

## Common shortcuts

| Action | macOS | Linux and Windows |
| --- | --- | --- |
| Command palette | Cmd-Shift-P, or Shift twice | Ctrl+Shift+P, or Shift twice |
| Go To (palette on Navigation) | Cmd-P | Ctrl+Alt+Shift+P |
| Resume palette | Cmd-Shift-R | Ctrl+Alt+Shift+R |
| New tab | Cmd-T | Ctrl+Shift+T |
| Reopen closed tab | Cmd-Shift-T | Ctrl+Alt+Shift+T |
| New worktree | Cmd-N | Ctrl+Shift+N |
| New workspace | Cmd-Shift-N | Ctrl+Alt+Shift+N |
| New window | Cmd-Alt-Shift-N | Ctrl+Alt+N |
| Split right / Split down | Cmd-D / Cmd-Shift-D | Ctrl+Shift+D / Ctrl+Alt+Shift+D |
| Focus pane left, right, up, down | Cmd-Alt-Arrow | Ctrl+Alt+Shift+Arrow |
| Next / previous pane | Cmd-Alt-] / Cmd-Alt-[ | Ctrl+Alt+Shift+] / Ctrl+Alt+Shift+[ |
| Toggle pane zoom | Cmd-Shift-Enter | Ctrl+Shift+Enter |
| Next / previous tab | Cmd-Shift-] / Cmd-Shift-[ | Ctrl+PageDown / Ctrl+PageUp |
| Focus tab 1 to 9 | Cmd-1 to Cmd-9 | Ctrl+Shift+1 to Ctrl+Shift+9 |
| Back / forward through focused panes | Cmd-[ / Cmd-] | Ctrl+Shift+[ / Ctrl+Shift+] |
| Close pane / Close tab | Cmd-W / Cmd-Shift-W | Ctrl+Shift+W / Ctrl+Alt+Shift+W |
| Find / next / previous | Cmd-F / Cmd-G / Cmd-Shift-G | Ctrl+Shift+F / Ctrl+Shift+G / Ctrl+Alt+Shift+G |
| Copy mode | Cmd-Shift-C | Ctrl+Alt+Shift+C |
| Clear pane | Cmd-K | Ctrl+Shift+K |
| Annotate selection for the agent | Cmd-Shift-A | Ctrl+Alt+Shift+A |
| Go to File / Go to Symbol | Cmd-O / Cmd-Shift-O | Ctrl+Shift+O / Ctrl+Alt+Shift+O |
| Split editor | `cmd-\` | `ctrl-shift-\` |
| New browser tab | Cmd-Shift-B | Ctrl+Alt+Shift+B |
| Toggle sidebar / status bar | Cmd-B / Cmd-Alt-B | Ctrl+Shift+B / Ctrl+Alt+B |
| Toggle zen mode | Ctrl-Cmd-Z | Ctrl+Alt+Z |
| Larger / smaller text | Cmd-= / Cmd-- | Ctrl+Shift+= / Ctrl+Shift+- |
| Reset text size | Cmd-0 | Ctrl+Shift+0 (Linux), Ctrl+0 (Windows) |
| Sessions | Cmd-Shift-S | Ctrl+Shift+S |
| Settings / Keyboard Shortcuts | Cmd-, / Cmd-/ | Ctrl+Shift+, / Ctrl+Shift+/ |
| Full screen | Ctrl-Cmd-F | F11 |
| Close window | Cmd-Alt-Shift-W | Ctrl+Alt+W |
| Quit | Cmd-Q | Ctrl+Shift+Q |

These are native actions, not keys sent to the terminal. Clipboard shortcuts (Copy and Paste) are fixed Edit actions; see [Terminals](terminals.md#selection-and-copy).

Some commands have no default key, such as **Review Changes** (`review_changes`), **Git Menu** (`git_menu`), **Toggle Annotate** (`toggle_annotate`), **Move Pane…** (`move_pane`), **Worktree Notes** (`worktree_notes`), **Open VS Code** (`open_code`) and **Open px0** (`open_px0`). Bind them as shown below.

## The command palette

Tap Shift twice, or press Cmd-Shift-P (Ctrl+Shift+P on Linux and Windows), to open the command palette. It searches workspaces on every connected host, agents and terminal panes, Sheprd's commands, the daemon's configured commands, and local project folders. Cmd-P opens it on **Navigation**.

- Choose **All**, **Navigation**, **Commands** or **Projects**, or cycle filters with Ctrl-Tab and Ctrl-Shift-Tab, without clearing the search.
- Search works like fzf: every space-separated term must match, and `'exact`, `^prefix`, `suffix$` and `!negate` apply per term.
- Rows you choose often and recently rise among the matches.
- In Navigation, filter agents by status: **All agents**, **Blocked**, **Working**, **Idle** or **Done**, or press `alt-b`, `alt-w`, `alt-i`, `alt-d` or `alt-a`.
- When the window has room, Navigation shows a preview of the selected pane. Alt-P toggles it.
- Up and Down, Tab and Shift-Tab, Ctrl-N and Ctrl-P, or Ctrl-J and Ctrl-K select. Enter or a click activates; Escape dismisses.
- **Resume Palette** reopens the palette with its last query, filters and selection.

The Projects filter lists the folders directly inside your project roots. Choosing one focuses the workspace rooted there, or creates one:

```toml
[palette]
double_shift = true # false disables only this gesture
project_roots = ["~/Code", "$HOME/Projects"]
```

## Rebinding keys

Open **Sheprd > Keyboard Shortcuts** to search every command, including those without a binding. Click **Edit**, enter a shortcut such as `ctrl-shift-t`, and choose **Save** (or press Enter). Separate alternatives with spaces, for example `ctrl-shift-t alt-t`.

- **Unbind** removes every shortcut for that command.
- **Reset** removes your override and restores the platform default and any inherited daemon bindings.
- Escape cancels the edit. Invalid shortcuts and conflicts are shown beside the editor.

Edits are saved in `config-gpui.local.toml` under `[keybindings]` and apply at once in menus, the palette and open windows. You can edit the same table by hand, keyed by command name:

```toml
[keybindings]
new_tab = "ctrl-shift-t"
split_right = ["ctrl-shift-d", "alt-d"]
close_tab = [] # unbind; remove this line to restore inherited defaults
```

A value is one keystroke or a list; an empty string or list unbinds the command. A keystroke needs a cmd, ctrl, alt or fn modifier, or must be a function key (F1 to F24). Assigning a default shortcut to another command moves it there, and two configured commands cannot share a shortcut. Unknown command names and unparseable keys are reported as config errors.

On macOS, Cmd-H, Cmd-Option-H and Cmd-M are Hide, Hide Others and Minimize. A command you bind to one of them runs instead, and the menu item loses its shortcut.

From a pane, `sheprd commands` lists every command name, `sheprd run COMMAND` runs one as the palette does, and `sheprd keybinds` lists the shortcuts.

## Herdr's keys and prefix chords

Herdr's own `[keys]` table in its `config.toml` applies in Sheprd too, Herdr's defaults included, so a habit from the Herdr TUI such as `prefix+v` works in both. The prefix (`ctrl+b` unless Herdr's `prefix` setting says otherwise) arms the window, shown by a keycap in the status bar; the next key runs its chord. Typing the prefix twice sends it to the terminal, and Escape cancels. Configure these in Herdr's `[keys]` and `[[keys.command]]` tables, not in `[keybindings]`.

## Keys sent to the pane

`[pane_keys]` maps a keystroke to the key the focused pane receives instead. By default:

- On macOS, Cmd-Left, Cmd-Right and Cmd-Backspace send Ctrl-A, Ctrl-E and Ctrl-U, and Option-Left and Option-Right send Esc-b and Esc-f to move by word.
- On every platform, Shift-Enter sends Alt-Enter, which prompt_toolkit apps and zsh read as a newline.

A value is a key a terminal can receive (`ctrl-a`, `home`, `alt-b`, `shift-enter`), or `text:` followed by text to send. An empty string removes a default, so `"shift-enter" = ""` sends Shift-Enter unchanged.

```toml
[pane_keys]
"shift-enter" = ""
```
