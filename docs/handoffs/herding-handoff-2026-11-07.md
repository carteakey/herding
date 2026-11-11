## Handoff 2026-11-07 (end of day)

**Role:** ds412 is supervisor (team `6aa93cf4`). **main:** `04b46f3a`. **Release:** `v20261107.1` (tag on `8f647973`; the ship-skill commit came after, docs-only). **Fleet:** fx505, m1, m2, yeti-cachy all on `v20261107.1`.

### Landed today
- `94fd2d80` **#11 boot-drift re-bind** — a daemon restart no longer locks the team out; no more manual SQLite repairs.
- `2ce2bce2` + `1f9c254f` **#14 bulletin newest page** — mentions past 100 posts stay visible; memory gauges wired.
- `1f10d68f` + `b9d2e1c7` **#26 prefix IDs** — unique hex prefixes accepted; a rebase exposed and fixed a real fixture bug.
- `8f647973` **upstream batch 1** — 6 cherry-picks (`-x`): terminal wide paths + emoji spaces; review open/return/focus. 1794 tests pass, clippy clean.
- `98f455d4` **V2 teams-first IA design doc** (#32): 4-view IA, 5 slices, slice-1 sketch.
- `c3656966` + `04b46f3a` **ship workflow** — `scripts/ship.sh`, `just ship`, and the authoritative **`herding-ship` skill** (`.agents/skills/herding-ship/SKILL.md`).

### In flight
- **workerbee1** — `67b5887c` #6 completion wake (verify/implement).
- **workerbee2** — `97edaf07` #12 aggregate memory (WIP committed on `feat/memory-cgroup-holders` = `86fd14a9`); then `616e7ab4` V2 slice 1.
- **workerbee3** — `5b7751f6` upstream batch 1: the 3 chat fixes (`a747543e`, `4cdb2101`, `1336e7e4`) remain; then batch 2.
- **Blocked:** `ab146a5c` V2 branding — waiting on the user's pixel-art mascot.

### Branches / worktrees
- Branches: `main`, `feat/v2-brand` (WIP), `feat/memory-cgroup-holders` (WIP).
- Worktrees: main, `herding-w2-cgroup`, `herding-wb-brand`.
- Kept: `v0-herding` checkpoint, docs PR branches (#18/#19). Deleted 10 local + 15 remote merged branches and 9 worktrees.

### Upstream lookback
Range `b93958f5` (merge base, 2026-10-31) → `upstream/main` `6a8d044c` (2026-11-07): **430 commits**, 171 `fix/perf/security`, 94 iOS/remote-control. Batch 1 took 6 of ~24 triaged candidates (#34). Remaining: 3 chat fixes, then batch 2 (remote-control/remote, sidebar/keymap/automations/vcs/orchestrator), Windows parity optional.

### Memory / OOM
- **8G btrfs swapfile** added (`/swapfile`, NOCOW, fstab) → 23G total swap.
- Session scope `ManagedOOMPreference=avoid` (protects the agents).
- **Do not cap the app's systemd unit** — it aggregates daemon + terminals + agents and killed 437 processes once. The app's own RSS is small (12–156MB); the aggregate (browser, Docker, agents, builds) is the driver.
- Sampler is Linux-only; macOS hosts have no memory diagnostics yet (follow-up on #12).

### Next session order
1. Finish upstream chat batch (`a747543e`, `4cdb2101`, `1336e7e4`), then batch 2.
2. V2 slice 1 (`616e7ab4`): Team home as the default body route.
3. #6 completion wake (`67b5887c`).
4. #12 aggregate memory (`97edaf07`) — wire the cgroup holders.
5. Branding (`ab146a5c`) when the pixel-art asset lands.
6. Then #16, #15, #5, #4, #7, #8.

### Risks
- Session-scope OOM can still recur; swap + oomd preference are mitigations, not a fix.
- Disk is ~91% full (swapfile); clean the build cache per `docs/build-cache.md` if builds fail on space.
- Upstream is 430 commits ahead; keep cherry-picking in gated batches.
- macOS sampler gap.

### Commands
- Binary: `/home/kchauhan/repos/forks/herdr-gpui/target/release/herdr-gpui`
- Team: `HERDR_PANE_ID=wA:p1 HERDR_WORKSPACE_ID=wA <binary> team ...`
- Ship: read `.agents/skills/herding-ship/SKILL.md` first; the script is a helper.
