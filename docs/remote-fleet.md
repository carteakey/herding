# Remote Fleet: Cross-Host Agent Coordination

Herding coordinates autonomous agent workers across multiple physical and virtual
machines in a unified team. This document details the cross-host architecture, the
SSH control tunnel runbook, and the live verification record on M1 Mac hardware.

## Architecture

```
+-------------------------------------------------------------+
| Local Supervisor Host (Linux x86_64)                        |
|                                                             |
|  - Herdr GPUI App (PID 1561831)                             |
|  - SQLite Team Store (team.sqlite3)                         |
|  - Team Bridge & Live Bulletin Channel                      |
|  - Unix Domain Socket: ~/.local/state/herdr/gpui/control.sock|
+------------------------------^------------------------------+
                               |
                   SSH Reverse Socket Forward
                  (-R remote_sock:local_sock)
                               |
+------------------------------v------------------------------+
| Remote Worker Host: m1 (Darwin arm64)                       |
|                                                             |
|  - Herdr Server Daemon (PID 1134, ~/.config/herdr/herdr.sock)|
|  - Forwarded Control Socket:                                |
|    ~/.local/state/herdr/gpui/control.sock                   |
|  - Workspace wA:                                            |
|      * wA:p1: ZCode worker ('zcode-m1')                     |
|      * wA:p4: Agy worker ('agy-m1')                         |
|  - CLI Binary: target/debug/herdr-gpui                      |
+-------------------------------------------------------------+
```

1. **Local Supervisor Host**:
   - Manages the durable team store (`team.sqlite3`), bulletin sequencing, and
     task state transitions.
   - Listens on `${XDG_STATE_HOME:-$HOME/.local/state}/herdr/gpui/control.sock`.
   - Proves worker caller identities from endpoint daemon snapshots (`m1`, `m2`,
     `fx505`, `Local`).

2. **Remote SSH Host (`m1`)**:
   - Runs headless `herdr server` managing persistent agent panes (`wA:p1` through
     `wA:p4`).
   - The remote `herdr-gpui` CLI binary resolves its default control socket path
     (`${HOME}/.local/state/herdr/gpui/control.sock`).
   - Requests made inside remote panes forward back to the local UI transparently.

## SSH Tunnel Runbook

### 1. Prerequisites
- SSH key authentication to remote host (`ssh m1` connects without prompts).
- OpenSSH 6.7+ on both client and host (supports native Unix domain socket forwarding).
- Local Herdr UI running with the active team.

### 2. Establishing the Control Tunnel
Run the tunnel script on the supervisor machine:

```sh
scripts/control-tunnel.sh m1
```

Or run directly via SSH in the background/systemd service:

```sh
ssh -N \
    -o ExitOnForwardFailure=yes \
    -o StreamLocalBindMask=0177 \
    -o ServerAliveInterval=15 \
    -o ServerAliveCountMax=3 \
    -R /Users/kchauhan/.local/state/herdr/gpui/control.sock:/home/kchauhan/.local/state/herdr/gpui/control.sock \
    m1
```

### 3. Preflight & Socket Cleanup
- If a stale socket file was left by a previous connection drop, the script removes
  it automatically before binding.
- If a live remote UI holds the socket, pass `--remote-socket` with an alternative
  directory and point `XDG_STATE_HOME` accordingly in the worker pane environment.

### 4. Running Commands on the Remote Host
From within any pane on the remote machine (or over SSH specifying environment variables):

```sh
ssh m1 'HERDR_PANE_ID=wA:p1 HERDR_WORKSPACE_ID=wA /Users/kchauhan/repos/forks/herdr-gpui/target/debug/herdr-gpui team status <TEAM_ID>'
```

## Live Verification Record (2026-11-04)

### Target Environment
- **Host**: `m1` (Darwin MacBookAir.ht.home 27.0.0 arm64)
- **Team ID**: `c48cfdff-0422-4299-b03c-804ac19f94f3` ("herding")
- **Active Endpoints**: `Local`, `fx505`, `m1`, `m2`

### Verification Steps & Results

1. **Registration of Remote Worker `zcode-m1`**:
   - Command:
     ```sh
     ssh m1 'HERDR_PANE_ID=wA:p1 HERDR_WORKSPACE_ID=wA /Users/kchauhan/repos/forks/herdr-gpui/target/debug/herdr-gpui team register c48cfdff-0422-4299-b03c-804ac19f94f3 --name zcode-m1'
     ```
   - Result: Registered `zcode-m1` with member ID `4975ba01-685e-4237-b417-010d24309f16`.

2. **Remote Status Query**:
   - Command:
     ```sh
     ssh m1 'HERDR_PANE_ID=wA:p1 HERDR_WORKSPACE_ID=wA /Users/kchauhan/repos/forks/herdr-gpui/target/debug/herdr-gpui team status c48cfdff-0422-4299-b03c-804ac19f94f3'
     ```
   - Result:
     ```
     herding (c48cfdff-0422-4299-b03c-804ac19f94f3)
       member: zcode-m1 (4975ba01-685e-4237-b417-010d24309f16, worker)
       pending mentions: 0  assigned tasks: 0  open tasks: 2
       goal: make herding the best multi agent system there is
     ```

3. **Roster Verification**:
   - Verified that both remote workers on `m1` appear in the team roster:
     - `884cde02-7bc5-45ac-8b46-be007a734a20` -> `agy-m1` (pane `wA:p4`)
     - `4975ba01-685e-4237-b417-010d24309f16` -> `zcode-m1` (pane `wA:p1`)

4. **Bidirectional Bulletin Delivery**:
   - **Outbound Ping**: `carteakeyblog` posted message `2a864f3d-c82a-4123-8e81-523b37c749aa` targeting `@zcode-m1`.
   - **Remote Receipt & Ack**: `zcode-m1` listed bulletin on `m1`, saw entry 163 as pending mention, and successfully acknowledged it via `team bulletin ack`.
   - **Inbound Pong**: `zcode-m1` replied from `m1` with message `3bca3901-b9a4-46d8-991d-06005d76a2b8` replying to `2a864f3d`.
   - **Supervisor Receipt & Ack**: `carteakeyblog` listed bulletin locally, saw entry 164 from `zcode-m1`, and acknowledged the message.
