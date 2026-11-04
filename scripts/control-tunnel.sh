#!/usr/bin/env bash
# Forward a remote SSH host's Herdr control socket back to this machine's
# Herdr UI, so `herdr-gpui team ...` runs natively from panes on that host.
#
# The tunnel is one ssh connection: OpenSSH 6.7+ forwards Unix domain
# sockets directly, so `ssh -R remote_socket:local_socket` binds the remote
# path that the remote CLI resolves by default and carries every request to
# the UI's socket here. No socat, no TCP, no listener on either host.
#
# Requires: OpenSSH 6.7+ here and on the host (any release since 2014),
# key-based auth to the host, and the UI running on this machine.
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: scripts/control-tunnel.sh [--socket PATH] [--remote-socket PATH]
                                 [--ssh-arg OPT]... HOST

Keep a Herdr control tunnel to HOST running until you press Ctrl-C. While
it runs, `herdr-gpui team status` (and every other `team` subcommand) on
HOST reaches the Herdr UI on this machine through HOST's default control
socket path, so an agent in an SSH pane talks to the team as itself: its
HERDR_PANE_ID and HERDR_WORKSPACE_ID travel with the request and the UI
proves them as usual.

  --socket PATH        The UI's control socket here. Default:
                       ${XDG_STATE_HOME:-$HOME/.local/state}/herdr/gpui/control.sock
  --remote-socket PATH The socket path to bind on HOST. Default: the same
                       expansion using HOST's HOME and XDG_STATE_HOME.
  --ssh-arg OPT        Pass OPT through to ssh (repeatable): ports, jump
                       hosts, aliases in your ssh config all work as-is.

The script refuses to replace a socket something answers on. A stale
remote socket left by a dead tunnel or app run is removed first, exactly
as the app's own bind recovers one; without socat on HOST the liveness
check is skipped, and sshd's bind decides, with a hint on failure.

Security: both ends are Unix domain sockets owned by the connecting user,
mode 0600, so nothing is reachable over the network. The remote path is
only as trustworthy as the host's other local users; the UI still
authenticates every request through the control protocol.
EOF
}

die() {
    printf 'control-tunnel: %s\n' "$*" >&2
    exit 1
}

SOCKET=
REMOTE_SOCKET=
SSH_ARGS=()
HOST_ARGS=()
while [ $# -gt 0 ]; do
    case $1 in
        --socket=*) SOCKET=${1#--socket=} ;;
        --socket)
            [ $# -ge 2 ] || die "--socket needs a path"
            SOCKET=$2
            shift
            ;;
        --remote-socket=*) REMOTE_SOCKET=${1#--remote-socket=} ;;
        --remote-socket)
            [ $# -ge 2 ] || die "--remote-socket needs a path"
            REMOTE_SOCKET=$2
            shift
            ;;
        --ssh-arg=*) SSH_ARGS+=("${1#--ssh-arg=}") ;;
        --ssh-arg)
            [ $# -ge 2 ] || die "--ssh-arg needs a value"
            SSH_ARGS+=("$2")
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        --)
            shift
            HOST_ARGS+=("$@")
            break
            ;;
        -*) die "unknown option: $1 (see --help)" ;;
        *) HOST_ARGS+=("$1") ;;
    esac
    shift
done
[ ${#HOST_ARGS[@]} -ge 1 ] || {
    usage >&2
    exit 2
}
TARGET=${HOST_ARGS[0]}

state=${XDG_STATE_HOME:-$HOME/.local/state}
SOCKET=${SOCKET:-$state/herdr/gpui/control.sock}
[ -S "$SOCKET" ] || die "no control socket at $SOCKET; is Herdr UI running?"

# Ask the host where its own state directory is: its HOME or XDG_STATE_HOME
# may differ from this machine's.
REMOTE_SOCKET=${REMOTE_SOCKET:-$(ssh "${SSH_ARGS[@]}" "$TARGET" \
    'printf "%s\n" "${XDG_STATE_HOME:-$HOME/.local/state}/herdr/gpui/control.sock"')}
[ -n "$REMOTE_SOCKET" ] || die "could not read HOST's state directory over ssh"
case $REMOTE_SOCKET in
    /*) ;;
    *) die "remote socket path is not absolute: $REMOTE_SOCKET" ;;
esac

remote_dir=${REMOTE_SOCKET%/*}
dir_q=$(printf '%q' "$remote_dir")
sock_q=$(printf '%q' "$REMOTE_SOCKET")

# The UI's own bind recovers a stale socket (connect, then remove when
# nothing answers); mirror that here. A live remote UI is refused, never
# killed: forward over a different --remote-socket instead, and anything
# else occupying the path is left for the user to handle.
if ssh "${SSH_ARGS[@]}" "$TARGET" "test -e $sock_q"; then
    if ! ssh "${SSH_ARGS[@]}" "$TARGET" "test -S $sock_q"; then
        die "$REMOTE_SOCKET exists on $TARGET but is not a socket; remove it, or forward a different path with --remote-socket"
    fi
    if ssh "${SSH_ARGS[@]}" "$TARGET" "command -v socat >/dev/null 2>&1 && socat -u OPEN:/dev/null UNIX-CONNECT:$sock_q </dev/null"; then
        die "a live socket answers at $REMOTE_SOCKET; is a Herdr UI running on $TARGET? Stop it, or forward a different path with --remote-socket"
    fi
    printf 'control-tunnel: removing stale socket %s on %s\n' "$REMOTE_SOCKET" "$TARGET"
    ssh "${SSH_ARGS[@]}" "$TARGET" "rm -f -- $sock_q"
fi
ssh "${SSH_ARGS[@]}" "$TARGET" "mkdir -p -- $dir_q" ||
    die "cannot create $remote_dir on $TARGET"

printf 'control-tunnel: %s:%s -> %s; Ctrl-C stops it.\n' \
    "$TARGET" "$REMOTE_SOCKET" "$SOCKET"
printf 'control-tunnel: on %s run herdr-gpui team status (in a Herdr pane) to verify.\n' \
    "$TARGET"

# In the foreground on purpose: Ctrl-C (or a dropped link) ends it, and the
# next run's pre-flight removes the socket sshd leaves behind on this
# connection's close.
if ! ssh "${SSH_ARGS[@]}" \
    -N \
    -o ExitOnForwardFailure=yes \
    -o StreamLocalBindMask=0177 \
    -o ServerAliveInterval=15 \
    -o ServerAliveCountMax=3 \
    -R "$REMOTE_SOCKET:$SOCKET" \
    "$TARGET"; then
    status=$?
    printf 'control-tunnel: ssh exited %s. If it could not bind %s, a live UI or a stale socket holds it: stop that UI or remove the path on %s, or forward a different one with --remote-socket.\n' \
        "$status" "$REMOTE_SOCKET" "$TARGET" >&2
    exit "$status"
fi
