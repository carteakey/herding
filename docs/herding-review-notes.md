# Herding review notes

Reviewed 2026-11-03 against the Herding specification, the repository's
`AGENTS.md`, and the implementation and uncommitted work present at the time.
The requested comparison was with local `main`; that branch was far behind, so
the review focused on Herding rather than unrelated changes. Other work was in
progress during the review. **Recheck each finding against the current code**
before implementing a fix; an unchecked item records a review finding, not a
claim that the latest working tree still has the defect.

The references below are starting points and may move as the code changes. The
product requirements are in [herding-detailed-spec.md](herding-detailed-spec.md)
and the existing work plan is in [herding-roadmap.md](herding-roadmap.md).

## Immediate review findings

### Identity and delivery

- [ ] **[P1] Authenticate the caller, not just the supplied pane.**
  `control/server/team.rs::current_identity` checks that the caller's pane and
  workspace exist, but the caller fields originate from client-controlled
  `HERDR_PANE_ID` and `HERDR_WORKSPACE_ID` values. Another local process can
  provide those identifiers and make requests attributed to a member. A
  snapshot proving that a pane exists does not prove who sent the request.
  Establish an enforceable caller/session association before granting scoped
  access; if a harness cannot provide one, expose the limitation honestly.
  Test impersonation attempts against reading, posting, acknowledgement, and
  task mutations. See spec sections 7.1 and 8.1.

- [ ] **[P1] Do not use pane identity as a substitute for session identity.**
  The generic terminal path in `control/server/team.rs::prove_pane_session`
  constructed an identity from agent kind, workspace, and pane. A replacement
  agent of the same kind in the same pane can inherit the old member's identity.
  Require a verifiable session or process identity, and mark unsupported or
  stale sessions unavailable instead of silently reusing membership. Test an
  agent exit and same-kind replacement, pane reuse, and daemon restart.

- [ ] **[P1] Fence every prompt and roster action to the original session.**
  `team/bridge/prompts.rs::dispatch_team_prompt` targeted the selected endpoint
  and pane ID rather than the member's complete stored target. The roster's
  `team/bridge.rs::members` also fell back to matching pane ID alone. A switch,
  reconnect, or replacement can make a different session look live or receive
  another member's messages and tasks. Revalidate daemon, workspace, pane, and
  session at action and delivery time; show a stale or unavailable state when
  proof is missing. Test endpoint switching, reconnect, and pane reuse.

- [ ] **[P1] Deliver only to the resolved recipients.**
  `prompts_for_team_action` sent the full text of a UI bulletin post or reply to
  every roster pane, even when `resolve_recipients` restricted the stored post
  to mentioned members. Deliver or notify only the fixed, persisted recipient
  set; don't disclose a targeted message to others through a session prompt.
  Test targeted mentions, team-wide posts, replies, and failed resolution.

- [ ] **[P1] Persist changes before notifying agents.**
  `poll_team` constructed prompts, queued a team mutation, and dispatched the
  prompts without waiting for the store's completion. The agent could be told
  about a team, membership, task, or post before it existed, including when the
  queue or database rejected the mutation. Associate delivery with a successful
  completion and report failures or uncertain outcomes. Test queue saturation,
  failed writes, and an agent checking the store immediately after notification.

- [ ] **[P1] Never inject into a busy or permission-blocked agent.**
  The Team prompt path called `deliver_notes`; in `agent_notes.rs`, a delivery
  held while an agent is `Working` becomes typable after its hold expires and
  can then be pasted while it is still working. The native ACP prompt path also
  sent without an explicit readiness check. A snapshot check alone cannot make
  a check-and-submit operation atomic. Use a verified harness capability that
  safely handles submission, or leave the message queued for an explicit
  check-in; do not assume an unknown session is idle. Preserve pending, failed,
  and uncertain states and do not blindly retry an ambiguous submission. Test
  long-running, blocked, unknown, disconnected, and permission-prompt states.

- [ ] **[P1] Make the agent bootstrap instructions executable.**
  `team/bridge/prompts.rs` instructed agents to run `herdr team ...`, including
  `inbox`, `post`, and `task done`. The documented implementation exposes
  `herdr-gpui team ...`, `bulletin list`, `bulletin post`, and `task complete`,
  with required team IDs. Use the actual CLI syntax and supply or discover the
  required IDs. Update the managed agent skill as well as `docs/cli.md`, and
  exercise the instructions with real supported harnesses.

### Durable state and bounded work

- [ ] **[P1] Confirm permanent team deletion.**
  The Team header's Delete button immediately called `TeamStore::delete_team`,
  whose foreign-key cascades remove the team's messages, acknowledgements,
  tasks, and findings. Require a confirmation that describes the data being
  removed. Deleting a team or removing a member must not terminate the
  underlying Herdr session. Test cancellation, successful deletion, error
  handling, and survival of the agent sessions.

- [ ] **[P2] Prevent stale roster saves from undoing other changes.**
  `TeamStore::save_team` upserted the fields and members in a complete UI
  snapshot. A stale snapshot from another window, or from before refresh, can
  restore a removed worker or overwrite a name or supervisor change; removed
  worker assignments may already have been cleared. Use field-specific
  mutations or revision checks and test interleaved changes. When removing a
  worker, preserve intelligible task, author, and message history and handle
  their outstanding work explicitly.

- [ ] **[P2] Keep status queries bounded and propagate read failures.**
  `control/server/team_worker.rs::scoped` scanned every historical post and
  queried recipient status per post for `team status`, on the serialized team
  worker. A growing history can stall unrelated operations. Its `while let Ok`
  loop also treated a page-read error as the end of the scan, returning a
  partial count. Use an appropriately indexed aggregate or another bounded
  approach and propagate errors. Test a large history and a read failure.

- [ ] **[P2] Make later posts, tasks, and findings accessible.**
  The Team snapshot scanned posts forward from the beginning and stopped after
  1,000; later posts were never displayed. Task and finding queries returned the
  first bounded page, with no way to reach later entries. Provide bounded
  queries for the latest content and pagination or another explicit way to
  reach all records. Test beyond each limit, including status counts and both
  the UI and agent-facing lists.

## Follow-up product decisions

These recommendations extend the user's questions; they are not claims that
the following features were already implemented or are part of the fixes above.

- **Create supervisor and worker sessions:** Offer existing sessions and a
  supported new-session flow in Team setup and Add worker. Reuse the app's
  existing launch mechanisms, choose the agent and supported profile, and bind
  membership only after verifying the new session. Show launch and capability
  failures without changing unrelated sessions.
- **Pause and resume:** Start with team or member delivery pause. Keep new
  work pending without writing to the session; on resume, revalidate identity
  and readiness and do not replay uncertain submissions. Make clear that
  pausing coordination does not stop work already running. Consider a worker
  limit for overload. Offer interrupt only through a verified harness
  capability; do not blindly send Ctrl-C or Esc to an arbitrary pane.
- **Issue integration:** Keep the local task board as the coordination record,
  including for non-Git investigations. Later, allow an explicit issue import
  or link to seed a goal or task and retain its source. Do not automatically
  update or close external issues; defer two-way synchronization until needed.
- **Lifecycle:** Consider archive as a reversible way to set a completed team
  aside. A supervisor can be replaced before removing the former supervisor.

## Verification and handoff

During this review, `cargo test --locked -p herdr-orchestrator team_store`
passed 11 tests. A GPUI test attempt was stopped while waiting for another
build's lock; no GPUI test, native UI test, live-agent test, or full CI run is
claimed here. Another agent was running verification separately; consult its
actual results rather than assuming those checks passed.

For fixes, add focused regression tests for each changed invariant, run the
repository-required `just format` and `just ci` gates, and run platform gates
when the changes require them. Validate the actual native interface where a
desktop is available. Finally, exercise a supervisor and at least two real
heterogeneous workers in isolated sessions, including messages sent while a
worker is busy, and verify evidence, session identity, and delivery states.
Do not use personal sessions as fixtures or modify Herdr itself.
