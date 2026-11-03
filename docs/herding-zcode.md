# ZCode CLI integration

This repository includes a ZCode plugin that reports an interactive ZCode CLI session to
the Herdr daemon and teaches the agent how to use the Herdr Team CLI. It uses
ZCode's public `zcode` resume command and plugin hooks; it does not read the
terminal display or depend on ZCode's private runtime files.

## Install the plugin

In ZCode, open **Settings → Plugins → Create → Add marketplace** and select the
`integrations/zcode` directory from this checkout. Install and enable
`herdr-zcode`, then start a new ZCode session in a Herdr pane. The plugin only
reports when `HERDR_ENV=1`, `HERDR_BIN_PATH`, and `HERDR_PANE_ID` are present.
Outside Herdr, the hook exits without running a command.

The plugin reports `idle` at session start and stop, `working` when a prompt is
submitted or a tool finishes, and `blocked` when ZCode requests a permission
decision. It sends the ZCode session ID and `zcode --resume <session-id>` to
Herdr so the pane can be restored with that conversation. The Team path proves
the caller through the active ZCode agent record in the selected daemon
snapshot, bound to its host, workspace, and pane. Reports run as a background
child process with no terminal streams; failures are ignored so Herdr cannot
interrupt ZCode. Prompt and tool contents are not forwarded.

The plugin includes the `herdr-team` skill. From a Herdr pane, the agent can use
`herdr-gpui team status`, inspect tasks and bulletin messages, claim and
complete work, and record findings. The Team CLI currently derives its caller
from pane environment values and daemon state; those values alone do not
authenticate the calling process or prove that a replacement agent is the
original member. See the [review notes](herding-review-notes.md).

This integration provides daemon agent identity, activity state, resume
metadata, and Team CLI guidance. It does not attach ZCode's transcript to the native
Herdr chat view or deliver Team messages by injecting terminal input. Herdr
clears an agent report when the CLI exits and the pane returns to its shell.

## Compatibility

The marketplace and plugin use the ZCode `.zcode-plugin/plugin.json` layout
and standard `hooks/hooks.json` location. The hook invokes
`$HERDR_BIN_PATH pane report-agent` with a stable source ID, a timestamp-based
sequence, the session ID, and the public resume command. Herdr added resume
commands in 0.9.2; older versions can still use state reporting but ignore the
resume command.

For the command contract and hook protocol, see the [ZCode CLI host integration
guide](https://github.com/kingsword09/zcode-cli/blob/main/docs/HOST_INTEGRATION.md),
[ZCode hooks](https://zcode.z.ai/en/docs/hooks), and [Add Herdr support to your
agent](https://herdr.dev/tui/docs/add-herdr-support/).
