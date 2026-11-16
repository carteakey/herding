---
title: Review and Git
blurb: Read the diff, leave notes, commit, push, open the PR.
status: shipped
---

# Review and Git

This page covers reviewing an agent's changes in Sheprd, sending review notes back to the agent, rolling a checkout back with checkpoints, and committing, pushing and opening a pull request from the title bar's Git menu.

## Reviewing an agent's changes

**Review Changes** in an editor group's **…** menu opens a review tab on the focused checkout's changes, like a pull request diff. Untracked text files show as wholly added. It has no default key; bind `review_changes` under `[keybindings]` to open it from the keyboard (see [Keyboard](keyboard.md)). An agent can open it for you with `sheprd review`.

> **Note.** Only checkouts on the local daemon can be reviewed.

The review is a tab like a browser tab: move it to a group of its own beside the terminal (see [Editor groups](editors.md#editor-groups)), give it the whole window, or close it. Opening the review again brings back the checkout's tab and reads its changes afresh. It also reads them again whenever the checkout's change counts move, unless you are writing a note.

### What it compares

- **Uncommitted** shows what is not committed yet (`git diff HEAD`).
- **Branch** shows everything the branch's pull request will hold: its commits and uncommitted work together, against where the branch left its base. The base is the pull request's base branch when GitHub reported one, else `origin/HEAD`, then `main` or `master`. Nothing is fetched.
- In a branch review, click the base after "against" to compare with another branch. **Automatic** goes back to the default base.

### Reading the diff

- The header switches between **Unified** and **Side by side**. Side by side, drag the line between the halves to resize them; double-click it to even them again.
- A file list on the left shows a folder tree, each file's change (A, M, D, R), its added and removed lines, its note count, and a check once viewed. Filter it by path, or turn on **Hide viewed**. Click a file, or pick it with the arrows and Enter, to bring it to the top of the diff.
- Each file's header folds it and marks it **Viewed**. Deleted files, lockfiles and files marked `linguist-generated` in `.gitattributes` start folded. A viewed file that changes again is no longer viewed. Viewed marks are kept per checkout and branch across restarts.
- A hunk header shows how many unchanged lines it leaves out; click it to show them, up to 200 at a time.
- Changed words are highlighted within edited lines. The whitespace icon hides whitespace-only changes.
- Find with the search icon, `/`, or Cmd-F (Ctrl-F on Linux and Windows). Enter and Shift-Enter, or `n` and `N`, step through matches.
- Very large changes stay reviewable: a file with more than 20,000 changed lines or larger than 4 MiB shows **Load diff** instead of its lines, and binary files are listed but not shown.

### Keys in the diff

| Key | Action |
| --- | --- |
| `j` / `k`, arrows | Scroll a line |
| Space, Page Up / Page Down | Scroll a page |
| `]` / `[` | Next or previous hunk |
| `.` / `,` | Next or previous file |
| `x` | Fold the file at the top |
| `v` | Mark the file at the top viewed |
| `e` | Open the file in your editor |

`e`, or the pencil on a file's header, opens the file in your terminal editor beside the agent's pane (see [Editors](editors.md)). Quitting the editor brings the review back and reads your edits.

The code selects like an editor's: drag, double-click a word, triple-click a line, Shift-click to extend, and Cmd-A (Ctrl-A elsewhere) for the whole file. Copying never includes line numbers, `+`/`-` signs or hunk headers. The copy icon on a file header copies its path; the one on a hunk header copies the hunk as a patch.

## Sending review notes to the agent

Click a line's gutter (added, removed or unchanged), or a file name, write what should change, and press Enter or **Add note**. Escape drops the note you are writing. Noted lines carry the note's number, and unsent notes stay while the tab is open.

Notes go to the agent in the focused pane, or else to the first agent in the focused workspace; the header names it. **Send to agent** turns all notes into one prompt: the checkout, then for each note the `path:line`, the quoted line, and your note. Without an agent, **Copy** puts the same prompt on the clipboard.

- An agent waiting in `sheprd browser feedback --wait` (or `sheprd feedback --wait`) receives the notes there.
- Otherwise they are typed into the agent's pane and submitted once Herdr reports the agent idle. Notes are never typed into a pane with no running agent, or into an agent that is asking you a question; those wait for the agent to collect them.
- While a batch waits for a busy agent, **Withdraw** takes it back so you can edit it.
- Sent notes stay listed, dimmed. Click one to edit it; Send then delivers only new or edited notes (**Send 2 new**), or every note again once all were sent (**Resend all**). **Clear sent** removes the sent notes.

The same notes work on web pages; see [Browser tabs](browser.md#annotating-a-page). You can also annotate text selected in a terminal; see [Terminals](terminals.md#annotating-terminal-text).

## Checkpoints

Sheprd takes a checkpoint of a checkout each time one of its agents starts or finishes a turn, so you can roll its files back. Checkpoints are hidden Git refs (under `refs/herdr-gpui/checkpoints/`) that include untracked, non-ignored files. The newest 50 per branch are kept. Taking one never changes the index or your files.

Checkpoints are on by default. Turn them off with **Settings > General > Checkpoint agent turns**, or:

```toml
agent_checkpoints = false
```

They work for checkouts on this machine and on SSH hosts. On a Windows client, they work for WSL distributions but not for SSH hosts.

### Restoring one

Choose **Checkpoints...** in a workspace's menu to list the checkout's checkpoints, newest first. Restoring one puts the checkout's files back as they were then and moves the branch back to the commit it was on. Files created since are removed; ignored files are left alone. The current state is saved as a checkpoint first, so a restore can be undone. A restore is refused while a merge, rebase, cherry-pick or revert is in progress.

### From the command line

```sh
sheprd checkpoint list
sheprd checkpoint create before refactor
sheprd checkpoint diff ID
sheprd checkpoint restore ID
```

`list`, `create` and `diff` work on the current directory's checkout without the app running. `checkpoint diff A B` compares two checkpoints; with one, it shows what changed since. `restore` only opens the checkpoints dialog: you confirm in the app.

## Commit, push and pull requests

The Git button in the title bar opens the Git menu for the focused checkout. It shows the working tree's changes (`+added -removed` and untracked entries), then:

- **Commit...** stages every change in the checkout, then commits with the message you write.
- **Push** pushes the branch to `origin` and sets it as the upstream.
- **Create pull request** pushes the branch, then opens a pull request against the repository's default branch, titled with the last commit's subject. In a fork checkout whose `upstream` remote is on GitHub.com, the pull request is opened against upstream's default branch.
- **Open pull request #N** once the branch has one, with its draft or ready state, review decision, merge status, and check counts.
- **Checks and comments** lists the pull request's checks and conversation; **Comment...** posts to it as your GitHub account; **Merge...** merges it after you choose a method.

None of these is retried or replayed automatically. Bind `git_menu` under `[keybindings]` to open the menu from the keyboard.

A branch with commits past the default branch and no pull request shows `3 commits, no PR` in the title bar (or a yellow `+PR` in the sidebar), which opens the Git menu on **Create pull request**. Pushing does not clear it; only a pull request does.

> **Note.** Pull request details need a GitHub account signed in to Sheprd. A `gh` or browser login is not picked up by itself.

## Pull request status

Each worktree row in the sidebar, and the title bar, show the branch's pull request number in a readiness color:

| Color | Meaning |
| --- | --- |
| Green | Clean merge |
| Red | Conflicts, failing checks, or requested changes; also a closed PR |
| Yellow | Pending checks or reviews, or unknown merge status |
| Orange | Blocked or behind, with no more specific check or review status |
| Gray | Draft |
| Purple | Merged |

Pull requests are refreshed in the background every 90 seconds. Opening the Git menu asks for a fresh lookup of the focused branch.

From a pane, `sheprd pr` shows your branch's pull request, `sheprd pr checks` its checks, and `sheprd pr comments` its comments (with your workspace focused). For issues and pull requests across repositories, see [Orchestrator](orchestrator.md).
