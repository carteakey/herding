# Changelog

All notable changes to the Herding multi-agent workbench and coordination stack are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased] - v0.2.0

### Added
- Team calls from your own user are bound to the process that registered
  the member (Unix peer credentials: `SO_PEERCRED` on Linux, `getpeereid`
  on macOS); the control tunnel's callers are recognized by their sshd
  credential and proved by snapshot as before (`docs/control-tunnel.md`).
- **SSH Control Tunnel (`1b26ce05`)**:
  - `scripts/control-tunnel.sh`: forwards an SSH host's control socket path back to the local Herdr UI over a single `ssh -R` Unix socket forward (OpenSSH 6.7+, no socat, no TCP), so `herdr-gpui team ...` runs natively from panes on remote hosts with unmodified caller identity.
  - Preflight refuses a live remote UI, recovers stale remote sockets the way the app's own bind does, and passes ssh options through (`--ssh-arg`).
  - Setup, identity and failure modes documented in `docs/control-tunnel.md`.

### Planned & In Progress
- **Cross-Host Agent Fleet (Remote Coordination)**:
  - Wire remote SSH endpoints (`m1`, `m2`, `fx505`) into the active team roster.
  - Forward the team control socket across SSH so remote agents invoke native CLI subcommands.
  - Establish cross-host failover and heterogeneous workload routing (RTX 4070 for inference, Apple Silicon for lightweight execution).

---

## [0.1.0] - 2026-11-04

### Added
- **Native Heartbeat & PTO Inspection (`8400a692`)**:
  - Direct native CLI subcommand: `herdr-gpui team heartbeat <TEAM_ID> [--member <MEMBER_ID>] [--ping]`.
  - Terminal buffer scanner detecting HTTP 429 Too Many Requests, Gemini `RESOURCE_EXHAUSTED`, Anthropic 5-hour/daily limits, account balance exhaustion, ZCode worker usage limits, and crashed processes.
  - Live UI roster status badges: localized `[PTO (Rate Limited)]` and `[Crashed]` badges dynamically surfaced in the Team View.
  - Standalone inspection tool and test suite in `scripts/herding-heartbeat.py` and `scripts/test_herding_heartbeat.py`.
- **Wire Protocol & UI Cursor Pagination (`ab13d2a0`, `19c9d252`)**:
  - Added backwards cursor pagination (`--before SEQUENCE` and `--limit N`) across `team task list` and `team finding list` with `next_before` JSON tokens.
  - Reactive `#team-tasks-show-earlier` and `#team-findings-show-earlier` disclosure controls in the GPUI desktop interface.
  - Flipped board windowing to read newest-first so active tasks and findings remain visible when older pages prepend.
- **2026 UI Modernization Pass (`b16c24f5`)**:
  - Eliminated per-frame UI thread deep clones (`render_team_bulletin_sidebar`) by migrating the sidebar directly into `TeamView` with borrowed snapshots and `cx.emit`.
  - Added shared design helpers in `view.rs`: `card` (`PANEL` corner radius matching orchestrator views), `chip` (`SMALL`), `chip_button`, `text_button`, `well`, `section_header`, and `button` variants (`Primary`, `Ghost`, `Toggle`).
  - Cached localized `fl!` labels and status counts in `render_status_banner` to cut per-frame allocations.
  - Truncated task titles, author names, and member handles with ellipsis in `min_w_0` rows.
- **Durable Persist-Before-Notify Engine (`78e25d5b`, `64627f42`)**:
  - Gated prompt dispatches strictly behind SQLite storage completions via `action_request`.
  - Bounded prompt holds (`PROMPT_HOLD_CAPACITY = 64`) with oldest-first eviction and complete withdrawal on write failures.
  - Extracted prompt bookkeeping into `crates/herdr-gpui/src/team/bridge/queue.rs`.
  - Added regression test suite pinning supervisor wake persistence across pause/unpause cycles and two-step CLI team deletion confirmation.
- **macOS M1 Workspace Integration**:
  - Verified remote macOS arm64 workspace over SSH on host `m1` (`MacBookAir.ht.home`).
  - Symlinked `herdr-team` skill into `~/.gemini/config/skills` and `~/.codex/skills`.
  - Built and verified native binary on M1 Apple Silicon.
- **Documentation & Upstream Policy**:
  - Rewrote root `README.md` to center purely on Herding (architecture diagrams, supervisor/worker division, durable SQLite store, CLI workflows, and roadmap).
  - Documented upstream sync strategy in `docs/migration/upstream-sync-strategy.md`, establishing the policy to reject upstream rebrand churn while selectively cherry-picking functional fixes.
  - Created desktop shortcut launcher (`herding.desktop`).

---

## [0.0.1] - 2026-11-03

### Added
- Initial Herding multi-agent prototype with supervisor and worker role assignments.
- Embedded SQLite durable store for team metadata, members, tasks, findings, and bulletin records.
- Live auto-scrolling bulletin feed with Enter-to-submit chat and targeted `@mention` prompt routing.
- Virtualized task boards and collapsible findings ledger.
