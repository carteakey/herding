# ZCode CLI integration

Two plugins make ZCode a first-class Herdr agent. They live in different
repositories because they belong to different projects:

| Plugin | Repository | What it does |
| --- | --- | --- |
| `herdr-zcode` | [`carteakey/zcode-cli`](https://github.com/carteakey/zcode-cli), `integrations/herdr` | Reports the pane's agent state to Herdr and the resume command |
| `herdr-team` | this repository, `integrations/zcode` | Teaches the agent to use the Herding Team CLI |

Install the state plugin first: it is what makes the pane show a ZCode agent at
all. The team plugin is optional and only matters inside a Herding team.

Both use ZCode's public `zcode` resume command and plugin hooks; neither reads
the terminal display or depends on ZCode's private runtime files.

## Install the state plugin

In ZCode, open **Settings → Plugins → Add marketplace** and select the
`integrations/herdr` directory from a `zcode-cli` checkout. Install and enable
`herdr-zcode`, then start a new ZCode session in a Herdr pane.

The launcher reports `unknown` when the TUI starts, because no session exists
yet and Herdr's `idle` means "ready for input". The plugin then reports `idle`
at session start and stop, `working` when a prompt is submitted or a tool
finishes, and `blocked` when ZCode requests a permission decision. It sends the
ZCode session ID and `zcode --resume <session-id>` to Herdr so the pane can be
restored with that conversation.

Everything is gated on `HERDR_ENV=1`, `HERDR_BIN_PATH`, and `HERDR_PANE_ID`.
Outside Herdr the hooks exit without running a command. Reports run as a
detached child process with no terminal streams; failures are ignored so Herdr
cannot interrupt ZCode. Prompt and tool contents are not forwarded.

Herdr clears an agent report when the CLI exits and the pane returns to its
shell.

## Install the team plugin

In ZCode, open **Settings → Plugins → Add marketplace** and select the
`integrations/zcode` directory from this checkout, then install and enable
`herdr-team`.

From a Herdr pane the agent can then use `herding team status`, inspect tasks and
bulletin messages, claim and complete work, and record findings. The Team CLI
proves the caller through the active ZCode agent record in the selected daemon
snapshot, bound to its host, workspace, and pane. Those values alone do not
authenticate the calling process or prove that a replacement agent is the
original member. See the [review notes](herding-review-notes.md).

## What this does not do

It does not attach ZCode's transcript to the native Herdr chat view, and it does
not deliver Team messages by injecting terminal input.

## Compatibility

Both marketplaces use the ZCode `.zcode-plugin/plugin.json` layout and the
standard `hooks/hooks.json` location. The hooks invoke
`$HERDR_BIN_PATH pane report-agent` with a stable source ID, a timestamp-based
sequence, the session ID, and the public resume command. Herdr added resume
commands in 0.9.2; older versions can still use state reporting but ignore the
resume command.

For the command contract and hook protocol, see the [ZCode CLI host integration
guide](https://github.com/kingsword09/zcode-cli/blob/main/docs/HOST_INTEGRATION.md),
[ZCode hooks](https://zcode.z.ai/en/docs/hooks), and [Add Herdr support to your
agent](https://herdr.dev/tui/docs/add-herdr-support/).
