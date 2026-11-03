# Herding: implementation handoff

Status: work in progress, 2026-11-03. This branch contains an unfinished native
Team feature, not a feature-complete release. The product intent and milestones
are in [herding-spec.md](herding-spec.md). The earlier Python experiment under
`experiments/team/` is separate from the app's Rust team store.

## Intended first experience

In the existing Herdr UI window, create a team with a supervisor, add real
agent sessions, and use a shared bulletin with replies and `@mentions`. Agents
need tools to inspect the roster and bulletin, post, and acknowledge messages;
the UI should show which deliveries are pending or acknowledged. The chosen
supervisor must actually coordinate workers and report to the user. Coding,
research, and binary analysis should all work, without requiring a Git repo.

## Where the current implementation lives

- `crates/herdr-gpui/src/team/`: native Team view, session candidates, and
  background bridge. The activity bar opens the Team section.
- `crates/herdr-orchestrator/src/team_store.rs` and `team_store/`: bounded,
  local SQLite team, member, post, recipient, and acknowledgement records.
- `crates/herdr-gpui/src/control/server/team*.rs` and `team_worker/`: app
  control and serialized background storage work.
- `crates/herdr-control-protocol/src/protocol/team.rs` and
  `crates/herdr-gpui/src/cli/control/team.rs`: agent-facing roster and bulletin
  control protocol and commands. See `docs/cli.md`.
- `crates/herdr-gpui/locales/en-US/team.ftl`: current Team strings.

The UI and agent tools use the Rust store; they do not use the Python prototype
database. UI work is unverified in a live app. A previous isolated snapshot
passed `cargo check --locked --offline -p herdr-gpui --bin herdr-gpui` and
`cargo check --locked --offline -p herdr-gpui --tests`; those checks did not
execute tests or prove the exact final branch state.

## Complete the first vertical slice

1. **Finish and validate the native view.** Create/select a team, choose a
   supervisor, add workers, open each selected session, post and reply, and
   visibly distinguish pending from acknowledged messages. Handle loading,
   failures, team switching, stale responses, long content, and narrow views.
   Add the requested recognizable Team icon and verify the actual UI in light
   and dark themes. Confirm how a user finds the view.
2. **Connect real agent sessions.** The identity check accepts attached native
   ACP chats, verified CLI conversation bindings, and active agent sessions
   reported in the selected daemon snapshot. The ZCode plugin in
   [herding-zcode.md](herding-zcode.md) reports its active state and resume
   metadata through Herdr's supported agent-report command. The Team path can
   recognize the resulting active agent record without a daemon patch, but its
   fallback identity is based on agent kind and pane, not proof of a stable
   session or the caller's identity. This is an open issue in the
   [review notes](herding-review-notes.md). The plugin does not provide a native
   transcript view or message injection. Exercise it with ZCode, including
   resume after a Herdr restart; then test Codex and OpenCode through their
   supported session-report paths.
3. **Make the supervisor functional.** Provide agent instructions or tools to
   discover their team, read the roster and bulletin, delegate and follow up,
   exchange evidence, and synthesize a result. A selected supervisor is not
   automatically an orchestrator. Tasks and findings still need a native
   agent-facing workflow or an intentional integration with an existing one.
4. **Complete message delivery.** A persisted post is not a delivered prompt.
   Specify how workers discover pending messages, acknowledge them, and receive
   safe notifications. Do not type into a busy/blocked terminal based on a
   stale status. Use an atomic idle-check capability if actually supported;
   otherwise leave the message queued. Make timeouts and uncertain delivery
   explicit and never blindly retry a potentially submitted message.
5. **Validate an end-to-end investigation.** In an isolated Herdr session,
   have a supervisor direct two different CLI workers through a bounded
   reverse-engineering or research task. Send a message while a worker is busy,
   verify it remains pending until processed, and inspect the supervisor's
   evidence-backed final report. Do not use personal sessions as test fixtures.

The broader spec also calls for pluggable agent adapters, local inference and
NIM through compatible harnesses, configurable account profiles, and a path
to workers across multiple machines. Keep cross-host identity, authentication,
artifact transfer, and offline recovery as a separately designed later phase.
An A2A bridge and pipeline designer are optional later work.

## Verification and operational notes

- Read the current repository `AGENTS.md` and relevant crate instructions.
  Preserve other work and use the existing worker, control, and UI patterns.
- Run focused tests for store isolation, member identity, message delivery and
  acknowledgement, concurrent operations, and the Team view. Validate the
  native interface in a real window; a compile check does not prove UI behavior.
- Run the repository's required `just format` and `just ci` gates for Rust
  changes, and platform-specific checks where applicable. Note exactly what
  ran; do not claim a live-agent or native UI test without performing it.
- The prior full CI run was interrupted due to disk pressure. Rust `target/`
  build output was cleaned to recover space, so subsequent builds will take
  time and consume disk again. Check free space and use a disposable target
  directory; do not delete user worktrees or uncommitted files for space.
- The Team feature has not been installed into the user's existing Herdr app.
  A checked build does not by itself update that application.

## Completion criteria

The user can run Herding in one window, form a team with a supervisor and two
real heterogeneous agents, give the supervisor a task, observe agents exchange
messages and findings, and receive a result with evidence. The UI reports
session and delivery state accurately. The flow is tested with real agents,
including ZCode or an explicitly documented compatibility gap, and the source,
commands, tests, and known limitations are available on the branch.
