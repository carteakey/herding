# Herding checkpoint — 2026-11-10 (evening)

**Main:** `7442619f8` (pushed). **Release:** `v20261110.1` on `50f1c130e`.
**Repo:** `/home/kchauhan/repos/forks/herding` (renamed from `herdr-gpui`).
**Team:** `6aa93cf4-691f-475d-a7e5-e7d793210e5d` ("herding"), supervisor `super`.

## Landed / pushed

- Upstream merge `50f1c130e` (penso/herdr-gpui, herdr→sheprd rename) + 7 follow-ups
  (`5059d9ab6` head): fleet doctor, zcode skill install, team-layer review doc,
  fleet path fix.
- Docs: wake workaround, supervisor/worker commit authority, multi-machine wake
  delivery, fork-only issue policy (`4608383b6` and earlier).
- **Mascot icon set** `7442619f8`: `Herding.png` (1024 pixelated),
  `Herding-pixel.png` (64px native), `Herding-smooth.png`, `Herding.icns`,
  `Herding.ico`, `Herding-worktree.icns`; replaced the About-box PNGs; source at
  `assets/brand/herding-mascot-source.png`.
- Pushed branches:
  - `fix/stale-wake-targets` `c61a93be8` (#35)
  - `feat/zero-touch-dispatch-rebased` `1fab6b517` (#4)
  - `feat/supervisor-handoff-rebased` `90ea9bbcf` (#15)

## In flight (gates)

A **detached** gate script survives server restarts:

- Script: `/tmp/opencode/gates-final.sh`
- Log: `/tmp/opencode/gates-final.log`
- It runs, serially: #35 fmt → #35 `cargo test -p sheprd-gpui team` → #35 clippy →
  #4 fmt → #4 test → #4 clippy.
- **#35 fmt is green.** The #35 team test was compiling at checkpoint time
  (`cargo test` pid 3058835). Check the log for `test_exit`, `clippy_exit`,
  `GATES_DONE`.

## Worktrees / branches

| Worktree | Branch | Tip |
| --- | --- | --- |
| `herding` | `main` | `7442619f8` |
| `herding-wake-targets` | `fix/stale-wake-targets` | `c61a93be8` (#35) |
| `herding-issue4` | `feat/zero-touch-dispatch-rebased` | `1fab6b517` (#4) |
| `herding-handoff` | `feat/supervisor-handoff-rebased` | `90ea9bbcf` (#15) |
| `herding-v2-slice1` | `feat/v2-slice1-team-home` | `5059d9ab6` (prepared, no commits) |

Old branches kept: `feat/v2-brand` `a8f2a9d00` (branding WIP), `feat/supervisor-handoff`
`7a9ce0a77` (pre-port), `feat/zero-touch-dispatch` `9c7ec5736` (pre-port).

## Team board

| Task | Status | Owner |
| --- | --- | --- |
| `31e170e5` #35 stale wake targets | in_progress | zcode3 |
| `19425d6c` #4 zero-touch dispatch | in_progress | zcode1 |
| `021fa4b9` #15 supervisor handoff | in_progress | zcode2 |
| `616e7ab4` V2 slice 1 | open | zcode1 |
| `080b99a6` post-merge verification | open | zcode3 |
| `11947e30` auto-sync | open | zcode3 |
| `93470ea1` m1 POC | open | zcode2 |
| `f0b1e67d` #49 supervisor add/claim approval | open | zcode2 |
| `ab146a5c` branding | blocked | — (user asset) |

## Workers

- `super` fbc0158d (wB:p2, supervisor).
- zcode1 `4a992ea7` (wB:p3), zcode2 `e3461b1f` (wB:p4), zcode3 `2322da27` (wB:p5).
- The zcode processes are alive but their turns are dead / context ~100%.
  Resume commands from `~/.config/herdr/session.json`:
  - wB:p3: `zcode --resume sess_46114f51-7594-44e8-82f8-3aac2492b97c`
  - wB:p4: `zcode --resume sess_13441378-d01b-4c74-9760-95b042a21ade`
  - wB:p5: `zcode --resume sess_7edef442-c950-41d7-8b86-9e7355ad0413`
- After starting the UI, each worker must `team register` from its own pane if
  `team status` refuses the caller.

## Fleet

| Host | Repo | Binary | UI |
| --- | --- | --- | --- |
| fx505 | `~/repos/forks/herding` | `v20261110.1 (3eafb2786)` | pid 3035364 |
| m1 | `~/repos/forks/herding` | `v20261110.1 (3eafb2786)` | pid 29062 |
| m2 | `~/repos/forks/herding` | `v20261110.1 (3eafb2786)` | pid 502 |
| yeti-cachy | `~/repos/forks/herding` | `v20261110.1 (19aedaa67)` | pid 3090142 |

The binaries predate the mascot commit `7442619f8`; a rebuild is needed to show
the new icon. m1/m2 use the pinned 1.99.0 toolchain (m1 needs
`RUSTC=~/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin/rustc` because
`/opt/homebrew/bin/rustc` shadows it).

## Wake regression (#35)

Two failure modes:
1. **Stale-target hot loop** — `deliver_team_prompt` returns `Kept` for a pane no
   snapshot proves; #17 maps that to `Held` and re-enqueues with `retry_at = now`,
   so it retries every tick forever. Live evidence: 42,865 `Held` lines in one
   session, ~180/sec.
2. **Silent direct-drop** — a fresh idle pane accepts `herdr pane send-text` but
   the app's paste never arrives, with no log line.

Workaround until #35 lands: `herdr pane run <PANE> "<prompt>"` locally; for an
SSH pane, `herdr --remote <target> pane run` or SSH in and use the host's local
`herdr pane run`. Documented in `skills/herdr-team/SKILL.md`.

Fix `c61a93be8`: `Delivered::Unreachable`, 5s `WAKE_RETRY` backoff, expiry of
queued wakes for panes outside the roster and every connected snapshot, an
`undeliverable` count in the roster, bridge + GPUI tests.

## Server instability

The OpenCode server was SIGTERM'd/restarted repeatedly (load 13–22, 15G RAM,
23G swap). The Herding UI dies with it unless relaunched. Mitigations used:
- Relaunch the UI detached: `setsid -f ./target/release/herding` with the Wayland
  env (`DISPLAY=:0 WAYLAND_DISPLAY=wayland-1 XAUTHORITY=/run/user/1000/xauth_aImyhJ
  XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus`).
- Run gates in a detached `setsid` script so they survive the restart.
- Keep one rustc at a time; load above ~15 risks OOM.

## Next steps

1. Check `/tmp/opencode/gates-final.log`. If #35 is green, rebase
   `fix/stale-wake-targets` onto `main` (`7442619f8`), fast-forward main, push.
2. Gate/land #4 the same way; then #15.
3. Rebuild the app with `7442619f8` + the landed fixes; swap the local UI;
   redeploy m1/m2/yeti.
4. Restart the zcode workers (resume sessions), re-register, and re-wake with the
   queues: zcode1 → V2 slice 1; zcode2 → #15 gates → #49 → m1 POC; zcode3 →
   post-merge verification → auto-sync.
5. Branding (`ab146a5c`) stays blocked on the user's pixel-art asset; the mascot
   icon set is now committed.

## Risks

- Server/OOM restarts; keep builds serialized and use detached scripts.
- Worker contexts at ~100%; they may need fresh sessions.
- Disk 81% (22G free); `target/` dirs are large.
- The hand-packed `Herding.icns` needs a macOS build check.
- The running fleet binaries are older than the mascot commit.

## Deploy 2026-11-10 late

- **fx505:** `herding v20261110.1 (f5196915a)`, UI pid `977782`.
- **m1:** `herding v20261110.1 (f5196915a)`, UI pid `64954`.
- **m2:** `herding v20261110.1 (f5196915a)`, UI pid `90818`.
- **yeti:** still `19aedaa67`; Tailscale offline (last seen 16m), LAN ping OK but
  SSH port 22 filtered. Needs a local update or sshd fix.
- **#35 landed and pushed** (`f5196915a`). **#4** test was stopped to free memory;
  branch pushed, gates deferred. **#15** branch pushed, gates pending.
- The mascot icon set is in the deployed binaries.

## Deploy final (all four hosts)

- fx505: `071a7ef3f`, UI pid 1655497.
- m1: `071a7ef3f`, UI pid 72751.
- m2: `071a7ef3f`, UI pid 24469.
- yeti-cachy: `071a7ef3f` (fx505 binary copied), UI pid 28863; repo fast-forwarded to `071a7ef3f`.
- Platform icon tiles: macOS rounded square, Windows square, Linux circle.
- yeti was briefly unreachable because it was asleep/off Tailscale; it came back and deployed.
