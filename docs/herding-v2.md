# Herding V2: Teams-first information architecture

Design doc for [issue #32](https://github.com/carteakey/herding/issues/32). Docs
only — no code changes. Base: `origin/main` @ `1f9c254f` (2026-11-07).

**Direction (user, 2026-11-07):** Herding's core product is *the team*, not the
terminal workspace. V2 opens to the team and designs the whole app around it;
terminals are a drill-down from a member or task, not the root. Team activity
becomes the logging/observability surface. Existing features are retained and
re-homed under team context — nothing is deleted.

---

## 1. Thesis and the problem with the current shell

Herding's durable objects — goal, tasks, bulletin, findings, members, fleet
health — live in the team store. Terminals, browser pages, code views, and
review diffs are instrumentation an operator picks up *while supervising a
team*. V1's shell inverts that relationship: it is a terminal multiplexer with
a team panel bolted to one sidebar slot.

Concretely, in today's shell:

- **The team is the smallest surface in its own app.** `ActivityPanel`
  (`crates/sheprd-control-protocol/src/protocol/app.rs:8`) lists eight equal
  entries, and Team is the only one that does not even own the body:
  `render_activity` (`crates/sheprd-gpui/src/sidebar/activity.rs:383`) swaps the
  *sidebar column* for `render_team_section` while the body stays terminal
  editor-groups (`window/render.rs:96`). The optional bulletin adds a 340 px
  right panel (`team/view/bulletin.rs:465`) — beside the terminals, again.
- **The default route is infrastructure.** Startup lands on
  `ActivityPanel::Workspaces` (`protocol/app.rs:12`), a view of the daemon's
  shell state — workspaces, tabs, panes — not of the work being done.
- **There is no "you are working team X" context.** The team bridge polls a
  2 s snapshot (`team/bridge.rs:28`) only while the panel is shown
  (`team/bridge/prompts.rs:179`); every other surface (Overview, PullRequests,
  Issues, Usage) is team-agnostic.
- **Observability is scattered.** "What happened while I was away" spans
  bulletins (store), the notes queue (`agent_notes.rs`), native agent chats
  (`chat/`), automation run pages (`automations/`), and review notes
  (`review/notes.rs`). No surface joins them.
- **The hierarchy is inverted at the leaf.** The one team→terminal link is
  `TeamAction::FocusWorker` (`team/view/roster.rs:175` →
  `team/bridge/prompts.rs:242` → daemon `focus_pane` RPC). Terminal→team
  requires knowing to open the rail. The panes are the root; the team is a
  leaf.

## 2. Proposed information architecture

A four-view shell. Everything below is a re-homing of existing components —
the store, the bridge, and the pane model do not change.

### 2.1 Team home — the default route

One full-body view, assembled from parts that already render today:

| Zone | Content | Existing source |
| --- | --- | --- |
| Header | team name, goal (inline editor), pause/queue state, switch-team | `view.rs` header card + `view/board.rs render_goal` |
| Board | tasks by status with dispatch/complete/cancel | `view/board.rs render_tasks` |
| Bulletin | live conversation, composer, replies, delivery states | `view/bulletin.rs` (promoted from the 340 px side panel) |
| Roster | members + agent/role/status chips, Focus/Nudge/Pause | `view/roster.rs render_roster` |
| Fleet strip | hosts, health, deployed version → full Fleet view | new, fed by endpoint catalog (§2.4) |

`TeamSnapshot` (`team/model.rs:99`) already carries every field these zones
need; the bridge is unchanged in the first slice.

### 2.2 Member view

Route: `Team › Member`. Identity card (name, agent kind, role, status,
pane target), current task card, activity list (posts by/about the member,
deliveries and acks, recent prompts), and the member's terminal as the primary
drill-down. Reuses: roster member card, task card, `FocusWorker` navigation
path, bulletin reply threading. This is where the notes queue and chat history
for that member eventually surface (§2.6).

### 2.3 Task view

Route: `Team › Task`. Brief and result wells, status/assignee, dispatch and
completion controls — i.e. today's `render_task_card` as its own view — plus:
linked issue (when issue-linkage lands), gate/CI state from findings and
heartbeat, and the assignee's terminal as a drill-down. Task view is the
natural home for review context (the checkout's diff) once tasks carry a
worktree ref.

### 2.4 Fleet view

Route: `Team › Fleet`. Hosts, health, deployed version, deploy action. Built
on what exists: the endpoint catalog (`endpoint.rs`), the devices overview
(`devices_overview/`, `Location::Devices`), and the version-mismatch path
(`window.rs announce_version_mismatch`, `update_panel.rs`). The new piece is
*deployed version per host* — source TBD (§6).

### 2.5 Terminals and the workspace

The daemon-owned pane grid stays exactly as it is (workspace → tab → pane
focus RPCs, splits, carry) but becomes a **drill-down**, reached from a member
(terminal button) or a task (assignee terminal), with a persistent way back.
The existing group/tab system (`browser/groups.rs`, `group_terminals.rs`) is
untouched; only the entry points change.

### 2.6 Where every retained feature moves

| Feature (today) | V2 home |
| --- | --- |
| Terminal grid + tabs (`Workspaces` panel, editor groups) | `Route::Terminals`, kept first-class; drill-downs from member/task |
| Browser pages (`Location::Web` per workspace) | Per-workspace tool inside member/task context; still openable from Terminals |
| Code tabs (`Location::Code`, `code_view/`) | Attached to task (worktree) and member context |
| Review (`review/`, `Location::Review`) | Task view's diff surface; still openable standalone |
| Orchestrator PRs/issues (`orchestrator/`, `PullRequests`/`Issues` sections) | Cross-team section reachable from home; per-task issue links from Task view |
| Overview / Security / Automations / Usage sections | Global surfaces in a secondary "workspace drawer" (rail overflow), not primary nav |
| Devices overview (`Location::Devices`) | Merged into Fleet view |
| Notes queue (`agent_notes.rs`) | Member view activity + delivery log |
| Native chats (`chat/`) | Member view, alongside terminal |
| px0 / core terminals / fan-out / mods | Unchanged surfaces, launched from the same places as today |
| Settings window | Unchanged |

Nothing is deleted. The `Location` tab-kind enum
(`browser/location.rs:55`) and `ActivityPanel` survive as embedded views; V2
re-routes to them instead of replacing them.

## 3. Navigation and default route

- **Shell:** the icon rail becomes the team rail — Home, Members, Tasks
  (board), Bulletin, Fleet — plus a global overflow (drawer) for the
  workspace/PR/issue/usage surfaces. Body = the routed view; terminals open
  as a full-body route or split panes inside it.
- **Default route:** Team home of the active team, persisted in chrome
  preferences. No active team → a zero-state (create or import a team) — the
  store already supports zero-team windows (`CreateTeam` action exists).
- **Breadcrumbs:** `Team › Member › Terminal`, `Team › Task › Review` —
  Back/Forward ride the existing per-endpoint history trail
  (`endpoint/history.rs`) and `activity.return_to`.
- **Deep links:** extend the three current `herdr-gpui://` forms
  (`link.rs:1`) with `team/<id>`, `team/<id>/member/<member_id>`,
  `team/<id>/task/<task_id>`.
- **Daemon focus stays daemon-owned.** Terminal drill-down keeps using
  `navigate(NavigationTarget::Pane)` (`window/commands.rs:143`); no new daemon
  RPCs are needed for slices 1–2.

## 4. Migration plan (slices)

Each slice lands independently on main, keeping the app green and usable.

1. **Team home as a body view.** Promote the team panel from sidebar
   replacement to a routed body view (`Route::TeamHome`) while Terminals
   remains one click away. All zones reuse existing render functions.
2. **Member and Task views.** Extract the roster card and task card into
   routable views with terminal drill-down; bulletin side panel folds into
   the member/task context.
3. **Fleet view.** Merge devices overview + endpoint health + per-host
   deployed version.
4. **Activity/observability.** Unify bulletin + notes + chats + runs into a
   per-team activity log (store-backed durable events — design decided in
   slice 4, §6).
5. **Re-home the rest.** Browser/code/review/orchestrator become
   team-context tools; `Workspaces` loses default-route status; rail
   cleanup and the global drawer land together.

Slices 1–2 are client-only. Slice 3 needs a version-report data source.
Slice 4 needs a store decision. Slice 5 is the only one that reshuffles the
rail and can be deferred indefinitely without harm.

## 5. First implementation slice — concrete sketch

**Goal:** Team home as the default route; terminals reachable from a member
in one click (already true via FocusWorker).

**Route model.** There is no router today — selection is flat fields on
`HerdrWindow` (`window.rs:98`). Add the smallest thing that can carry V2:

```rust
// crates/sheprd-gpui/src/window.rs
enum Route {
    Terminals,                    // today's body, bit-for-bit
    TeamHome,
    Member(String),               // member_id
    Task(String),                 // task_id
    Fleet,
}
// HerdrWindow gains `route: Route`; `activity.panel` keeps its meaning
// only while `route == Route::Terminals`.
```

**Render dispatch.** The body switch at `window/render.rs:719` gains `Route`
arms; `Route::Terminals` delegates to the existing `Pick`/`Shown` machinery
unchanged. `Route::TeamHome` renders `team/view.rs` sections full-width —
`render_goal`, `render_tasks`, `render_findings`, `render_roster` — with the
bulletin composer as a column instead of the 340 px side panel.

**Rail.** `sidebar/activity.rs` gains Home/Fleet entries; the Team button
now sets `Route::TeamHome` instead of swapping the sidebar column. Workspaces
button returns to `Route::Terminals`.

**Default.** On window build (`window/construct.rs`) and on
`apply_stored_chrome` (`window.rs:516`): `Route::TeamHome` if the bridge has
an active team snapshot, else `Route::Terminals`.

**Files touched (estimate):**

| File | Change | ~Lines |
| --- | --- | --- |
| `window.rs` | `Route` enum + field + persistence | +60 |
| `window/render.rs` | dispatch arms | +120 |
| `team/view.rs` | full-width layout variant of existing sections | +200 |
| `team/view/bulletin.rs` | column variant beside composer | +80 |
| `sidebar/activity.rs` | rail entries → route setters | +60 |
| tests (`team/view/tests.rs`, headless layout) | route rendering + default-route logic | +250 |

**Rough size: 600–900 lines net, client-only.** No store, bridge, or
protocol changes; `TeamSnapshot` already carries everything. Est. 1–2 days
plus gates. Risk is low because `Route::Terminals` preserves the current
shell exactly — the change is additive routing, not a rewrite.

## 6. Open questions

1. **The truncated constraint.** The direction ends at "we will retain
   features but…" — the rest of the sentence is TBD (captured on #32 when it
   lands). Working assumption: retain = re-home with no parity cuts, and
   terminals stay reachable within one click from any team context. If the
   missing clause tightens that (e.g. "but the terminal grid is
   power-user-only"), only §2.5 and slice 5 change.
2. **One team or many?** The store supports multiple teams
   (`TeamSnapshot` teams page, `SwitchTeamView`, `LoadMoreTeams`). Proposal:
   single-team shell with a header switcher for V2.0; a multi-team home
   (cross-team board/activity) is a larger IA change deferred until the
   single-team shell proves out.
3. **Terminal grid status.** Kept first-class (`Route::Terminals`) for power
   users — which is also what makes the migration safe. Long-term it is a
   member/task drill-down; revisit after slices 1–2.
4. **Activity log scope (slice 4).** Project the activity log from existing
   posts/tasks/findings (cheap, lossy) vs. new durable-events table in
   `team_store` (needs a write-path design and retention policy).
5. **Deployed-version reporting (slice 3).** Heartbeat/status payload vs.
   a store-level deploy log; depends on what the fleet rollout (v-tag)
   already records.
6. **Does the daemon need a "current team"?** Wake routing, default
   endpoints, and notifications are daemon-side; the client's active-team
   mapping may want to live there too (e.g. `herdr-gpui://team/…` links
   resolving without a window). Defer until a concrete need appears.
