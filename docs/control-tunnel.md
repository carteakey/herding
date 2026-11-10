# Control tunnel: `team` commands from SSH hosts

Herding serves team coordination through a Unix domain socket on the
machine it runs on:

    ${XDG_STATE_HOME:-$HOME/.local/state}/herdr/gpui/control.sock

An agent in a pane on this machine reaches it because the socket sits in
its own filesystem. An agent in a pane on an SSH host (`m1`, `m2`, ...)
does not: its `herdr-gpui team ...` dies with "Herding is not running,
or its control socket is unreachable". `scripts/control-tunnel.sh` closes
that gap: it binds the host's own control socket path and carries every
request back to the UI here, so the remote agent runs the native CLI
subcommands against the team as itself.

## Quick start

On the machine Herding runs on:

```sh
scripts/control-tunnel.sh m1
```

Leave it running. In any Herdr pane on `m1`:

```sh
herdr-gpui team status
herdr-gpui team task list <TEAM_ID>
```

The remote `herdr-gpui` binary resolves its default socket path
(`<state>/herdr/gpui/control.sock`), which the tunnel now serves, so
nothing on the remote host needs configuring. A per-host tunnel is one
script instance; run several side by side for a fleet.

## What the tunnel proves about a caller

Peer credentials at the UI's end of the tunnel name sshd, not the remote
agent: the forwarded connection terminates at sshd here, and the remote
agent's process identity does not survive the hop. The tunnel caller is
recognized honestly by that credential (the sshd process runs as root
here, while a local caller runs as you), and it is not required to hold a
per-process binding. What stands in for it: the forwarded socket path is
mode 0600 and owned by the host's user, so the remote side is bounded by
the same local-user trust the main socket model documents, and every
request is still proved against the daemon snapshot here (pane, workspace,
agent session). This is the documented downgrade for tunneled callers; a
session-token handshake over the tunnel would bind the remote agent's own
process and remains open if the fleet needs it.

## How it works

One ssh connection, no extra tools:

1. OpenSSH 6.7+ (2014) forwards Unix domain sockets natively. The script
   runs `ssh -N -R <remote_socket>:<local_socket> HOST`.
2. sshd on the host binds `<remote_socket>` at the path the remote CLI
   resolves by default, mode 0600, owned by the connecting user.
3. Every CLI call there flows over the ssh channel to the UI's socket
   here. `Ctrl-C` (or any connection drop) tears the tunnel down. This
   OpenSSH build leaves the remote socket file behind on close; the next
   run's pre-flight proves it dead (a connect is refused) and removes it,
   the same recovery the app's own bind does for its socket.

The remote socket path is read from the host itself (`HOME` and
`XDG_STATE_HOME` there may differ from here). Override it with
`--remote-socket PATH`; pass ssh options through with `--ssh-arg`
(repeatable), and use any destination your ssh config understands:

```sh
scripts/control-tunnel.sh --ssh-arg -p 2222 ops@jump.example
```

## Identity and security

- Both ends are Unix domain sockets owned by the connecting user, mode
  0600: nothing listens on TCP anywhere, and no other remote user can
  reach the tunnel's path.
- The tunnel is transport, not trust. The control protocol still proves
  every caller: `HERDR_PANE_ID` and `HERDR_WORKSPACE_ID` travel with each
  request, and the UI attributes them to the pane's session exactly as it
  does for local panes. A forged pane id from a stray process on the host
  has the same standing the audit logged (finding c2686543): it can act
  as that pane's member, locally and through the tunnel alike. The sprint
  tracks hardening that separately.
- If a Herding actually runs on the host, its socket is live and the
  script refuses to replace it. Stop that UI or forward a different path
  with `--remote-socket`.

## Failure modes

| Symptom | Cause and fix |
| --- | --- |
| `no control socket at ...; is Herding running?` | The UI here is down. Start it, rerun. |
| `Administratively prohibited` / forward refused | sshd on the host forbids forwarding (`AllowTcpForwarding`, `PermitListen` for TCP; unix forwards need OpenSSH 6.7+ sshd). Ask the host's admin or check `sshd_config`. |
| `ExitOnForwardFailure` right after connecting | The remote path could not be bound: a stale socket created between pre-flight and bind, or a non-socket file there. The script prints the path to remove; delete it on the host, rerun. |
| Requests time out through the tunnel | The ssh link dropped mid-session; the script exits on link loss, rerun it. |
| `team status` returns but shows the wrong caller | The pane's `HERDR_PANE_ID` / `HERDR_WORKSPACE_ID` are not set in that shell; the CLI proves the caller from the pane environment. |

## Wake delivery to remote panes

The tunnel makes the `team` CLI work from a remote pane; it does not change how
the UI delivers a wake. The UI app owns the wake queue and targets the member's
`host + workspace + pane` tuple. For a remote member, the UI must be connected
to that host's endpoint and the pane must be in that endpoint's snapshot.

- The local `herdr pane run` / `send-text` only reaches the local daemon. For a
  pane on an SSH host, run the command on that host:
  `herdr --remote <ssh-target> pane run <PANE_ID> "<text>"` (key auth), or SSH
  in and use that host's local `herdr pane run <PANE_ID> "<text>"`.
  `pane run` sends the text and Enter in one call.
- Register each member from its own pane **through the tunnel**, so the member
  row lands in the supervisor's team store. Without the tunnel, a remote
  `team register` writes to that host's own store and the supervisor's UI
  cannot see the member.
- The wake queue is in-memory in the UI app. A UI restart clears it; a stale
  remote target can retry forever and flood the log until the UI restarts
  (issue #35). The fix distinguishes unreachable from busy and logs the
  endpoint.
- After any UI/daemon restart, the member's `host` (daemon boot id) changes:
  re-register from the pane and re-check `team roster` / `team heartbeat`.

Multi-machine checklist:

1. Start the tunnel from the supervisor's machine to each worker host.
2. Register each member from its own pane through the tunnel.
3. Confirm `team roster` / `team heartbeat` shows the member and the UI's
   `status --json` shows the host online.
4. Deliver a test wake and verify the pane; if it does not arrive, use the
   host-local `herdr pane run` workaround above.
5. After a restart, re-register and repeat step 3.

## What this does not cover

- The remote host still needs a `herdr-gpui` binary for the CLI (the
  app-managed shim at `~/.local/state/herdr/gpui/bin/herdr-gpui` serves
  `browser open` only). Fleet provisioning of the full binary is a
  separate piece of the v0.2.0 sprint.
- Windows hosts: their control transport is a named pipe, not this
  socket, and sshd on Windows has no Unix socket forwarding. The script
  targets Unix hosts on both ends.
