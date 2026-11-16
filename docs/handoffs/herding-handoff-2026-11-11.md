# Herding handoff — 2026-11-11 (end of day)

**Main:** `9f77a669c` (pushed). **Release tag:** `v20261110.1` on `50f1c130e`.
**Repo:** `/home/kchauhan/repos/forks/herding` (renamed from `herdr-gpui`).
**Team:** `6aa93cf4-691f-475d-a7e5-e7d793210e5d` ("herding"), supervisor `super`.

## Landed today

- **Upstream merge + rebrand** `50f1c130e` (penso/herdr-gpui, herdr→sheprd) plus
  seven follow-ups (`5059d9ab6`): fleet doctor, zcode skill install, team-layer
  review doc, fleet path fix.
- **#35 wake-delivery fix** `f5196915a` (rebased onto the merge):
  - `Delivered::Unreachable` for a pane no snapshot proves.
  - 5s `WAKE_RETRY` backoff instead of retrying every tick.
  - Expire queued wakes for panes outside the roster and every connected
    snapshot; surface an `undeliverable` count in the roster.
  - Regression tests at bridge and GPUI levels (128 team tests passed).
  - This fixes the hot loop (42k+ `Held` lines) and the silent direct-drop.
- **Mascot icon set** `7442619f8`: `Herding.png`, `Herding-pixel.png`,
  `Herding-smooth.png`, `Herding.icns`, `Herding.ico`, source under
  `assets/brand/`.
- **Platform icon tiles** `84ff7d26d`: macOS rounded square, Windows square,
  Linux circle, on the charcoal `#1E1A17` tile.
- **Per-platform app icon** `071a7ef3f`: `app_icon.rs` includes the macOS,
  Windows, or Linux tile instead of one PNG everywhere.
- **Linux launcher** `9f77a669c`: the circle mascot SVG replaces the old Sheprd
  paw/gradient; the desktop entry says Herding and runs `herding`; the Linux
  package installs `bin/herding` with `sheprd`/`herdr-gpui` compatibility
  symlinks and a `Herding-*` archive.
- **Docs:** wake workaround, supervisor vs worker commit authority,
  multi-machine wake delivery, fork-only issue policy (committed earlier).

## Fleet — deployed

| Host | Version | UI |
| --- | --- | --- |
| fx505 | `herding v20261110.1 (071a7ef3f)` | pid 1655497 |
| m1 | `herding v20261110.1 (071a7ef3f)` | pid 72751 |
| m2 | `herding v20261110.1 (071a7ef3f)` | pid 24469 |
| yeti-cachy | `herding v20261110.1 (071a7ef3f)` | pid 28863 |

- All four checkouts are `~/repos/forks/herding`.
- The binaries are `071a7ef3f`; the launcher fix `9f77a669c` is packaging-only
  and appears on the next Linux package build.
- m1/m2 use the pinned 1.99.0 toolchain. m1 needs
  `RUSTC=~/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin/rustc` because
  `/opt/homebrew/bin/rustc` shadows it.
- yeti's binary was copied from fx505 (same x86_64/glibc 2.44) and swapped over
  the running binary via a temp name + rename (`scp` hits `ETXTBSY` otherwise).
- yeti was briefly unreachable because it was asleep/off Tailscale; it came
  back and deployed.

## In flight / pushed branches

| Branch | Tip | State |
| --- | --- | --- |
| `fix/stale-wake-targets` | `f5196915a` | landed on main |
| `feat/zero-touch-dispatch-rebased` | `1fab6b517` | #4; gates deferred to CI |
| `feat/supervisor-handoff-rebased` | `90ea9bbcf` | #15; gates pending |
| `feat/v2-slice1-team-home` | `5059d9ab6` | V2 slice 1 prepared, no commits |
| `feat/v2-brand` | `a8f2a9d00` | branding WIP (brand doc/mark) |

## Team board

| Task | Status | Owner |
| --- | --- | --- |
| `31e170e5` #35 stale wake targets | done (landed) | zcode3 |
| `19425d6c` #4 zero-touch dispatch | in_progress | zcode1 |
| `021fa4b9` #15 supervisor handoff | in_progress | zcode2 |
| `616e7ab4` V2 slice 1 | open | zcode1 |
| `080b99a6` post-merge verification | open | zcode3 |
| `11947e30` auto-sync | open | zcode3 |
| `93470ea1` m1 POC | open | zcode2 |
| `f0b1e67d` #49 supervisor add/claim approval | open | zcode2 |
| `ab146a5c` branding | blocked | — (asset) |

## Workers

The zcode processes are alive but their turns are dead / context ~100%.
Resume from `~/.config/herdr/session.json`:

- wB:p3: `zcode --resume sess_46114f51-7594-44e8-82f8-3aac2492b97c`
- wB:p4: `zcode --resume sess_13441378-d01b-4c74-9760-95b042a21ade`
- wB:p5: `zcode --resume sess_7edef442-c950-41d7-8b86-9e7355ad0413`

After starting the UI, each worker must `team register` from its own pane if
`team status` refuses the caller.

## Wake regression (#35) — fixed, workaround still documented

The fix is on main. The temporary workaround remains in
`skills/herdr-team/SKILL.md` for older binaries:

```sh
herdr pane run <PANE_ID> "<the full team prompt>"
```

For an SSH pane, run it on that host (`herdr --remote <target> pane run`, or SSH
in and use the host's local `herdr`). The local `herdr pane run` only reaches
the local daemon.

## Repo guidance change

`AGENTS.md` now says: run quick checks only (under ~10s each) before a commit or
push; defer long tests, full workspace gates, cross-platform lints, and release
builds to CI. This is why the 30-minute local `cargo test` runs were stopped.

## Next steps

1. Let CI run the #4 and #15 gates; land them when green.
2. Restart the workers, re-register, and re-wake with the queues:
   - zcode1: V2 slice 1 (`616e7ab4`)
   - zcode2: #15 gates → #49 (`f0b1e67d`) → m1 POC (`93470ea1`)
   - zcode3: post-merge verification (`080b99a6`) → auto-sync (`11947e30`)
3. Build/install the Linux package to see the circle launcher icon.
4. Rebrand the macOS packaging: `build-macos.sh`/`package-macos.sh` still produce
   `Sheprd.app`/`Contents/MacOS/sheprd`; `dispatch.sh` still targets
   `sheprd-labs/sheprd`. The Linux packaging is done.
5. Port the brand doc/mark from `feat/v2-brand` if it is still wanted; the
   mascot icon set is already on main.
6. Branding `ab146a5c` is no longer blocked on artwork: the mascot is in.

## Risks

- OpenCode server restarts under load; the Herding UI dies with it unless
  relaunched. Use `setsid -f ./target/release/herding` with the Wayland env.
- Worker contexts at ~100%; they may need fresh sessions.
- Disk 81% on fx505 (22G free); yeti 97% (18G free).
- The macOS `.icns` is hand-packed; verify it on the next macOS bundle build.
- The release pipeline still has Sheprd-named macOS artifacts and the upstream
  `dispatch.sh` repo target.

## Commands

- Binary: `/home/kchauhan/repos/forks/herding/target/release/herding`
- Team: `HERDR_PANE_ID=wB:p2 HERDR_WORKSPACE_ID=wB <binary> team ...`
- Ship: read `.agents/skills/herding-ship/SKILL.md` first; `scripts/ship.sh` is a helper.
- Checkpoint from earlier: `/home/kchauhan/herding-checkpoint-2026-11-10.md`
