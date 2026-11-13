# Teams on herdr-core: plan

Status: revised 2026-11-13 after a review of the Teams code and of
`crates/sheprd-core-client`. It replaces the service-and-socket porting plan
of the same date. Spike 1 passed; four smaller spikes below gate phases 3 and
4. It extends `docs/herding-team-review.md`.

## The pivot in plain words

The redesign changes three things: where the team's records live, how a
member knows who it is, and how a member hears it has new work.

**1. The records stop living inside the app.** Today the board, bulletin, and
roster sit inside the Herding window, and every `herding team` command is a
request to the running app. If the app is closed, restarting, or another app
holds its socket (#85), the whole team stops. In the redesign the records are
a database file on disk, and every team command reads and writes that file
directly. Nothing has to be running for an agent to claim a task or post a
message. The Teams window becomes one more program reading the same file.

**2. A member is told who it is when it starts, not asked to prove it
later.** Today an agent starts in a pane, registers, and the app works out
which pane and session it is. After a reboot the pane ids change, the match
fails, and the window shows "Session not attached" (#81 and the fixes before
it). In the redesign `team spawn` starts the worker's terminal with its team
and member name already attached, in its environment and as labels that
herdr-core keeps on the terminal. Its identity comes with every command it
runs. An agent that was not spawned by the team can still register its own
terminal once.

**3. A wake becomes a doorbell, and the messages wait in an inbox.** Today a
wake pastes the whole message into the agent's pane. That cut ZCode prompts
down to "f" and "in" and typed into whatever was on screen, and acks,
retries, and several queues were built to cope. In the redesign:

- Anything addressed to a member (a task, a mention, a result to review) is
  saved to that member's inbox in the same step that records it.
- A small background program rings the member with one short line: "you have
  3 items, run `sheprd-team inbox`". It uses herdr-core's own send, which
  types only while the agent is idle and reports whether the agent took it.
- A missed doorbell loses nothing: the next one says "4 items". Reading the
  inbox marks the items read, so there is no separate ack step.

What gets simpler as a result:

- **Roles.** The supervisor is a member with a role, not a special slot.
- **Liveness.** Each member has a "last seen" time, updated by every command
  it runs, in place of a guess from 30 lines of terminal output.
- **Pausing.** A pause is saved in the database, not lost with the window.
- **Code.** Most of today's window-side team code (wake delivery, identity
  checks, prompt builders, queues) is deleted, not ported.

Machines:

- **Other machines.** `spawn` starts a worker on m1 through herdr-core. That
  worker's commands reach the supervisor's database over the SSH tunnel that
  exists today.
- **Herdr 1 panes.** New members run on herdr-core, which is what provides
  the labels and the idle-checked send. A member left on a Herdr 1 pane can
  read its inbox but gets no doorbell. Moving the current workers is a
  one-time restart with `--resume`, which keeps their conversations.

The trade-off: today, on Linux only, the app checks which process is calling.
In the redesign identity comes from the terminal's environment and labels, so
any program running as you could pretend to be another member. That is why
removing members and changing roles need your approval, every write is
logged, and one spike checks whether herdr-core can confirm which terminal a
command came from.

Where it lives: a new crate in this repository, free of the app's
dependencies so it builds quickly. Whether it moves to its own repository is
decided once the design has settled.

## Target shape

```
 herding team <op> ─┐
 sheprd-team <op>  ─┼─ in-process ─▶ sheprd-team (lib): apply(actor, op) → reply
 Teams window      ─┘                     │
                                          ▼
                          SQLite (WAL): records + inbox + doorbell state
                                          ▲
 sheprd-team daemon ── reads undelivered inbox rows
        │  sheprd-core-client: terminal.send { if: idle }, events.subscribe
        ▼
 herdr-core (Herdr's, unmodified): terminals, status, machines

 remote member ── the same op as JSON, over the tunnel ──▶ daemon ──▶ apply
```

## What is ours

| Piece | Owner | What it brings |
| --- | --- | --- |
| Sheprd (`penso/herdr-gpui`) | upstream community app this fork tracks | the GPUI window, browser, git and PR views, cloud machines, automations, phone remote control, and `sheprd-core-client` |
| **Teams** | **this fork** | the store, board, bulletin, roles, inbox, the team CLI, the delivery daemon, the Teams window, and the supervisor/worker workflow |
| Herdr's official app and herdr-core (`hc`) | Herdr ([herdr.dev](https://herdr.dev/), Apache-2.0) | the terminal runtime, machines over SSH or Tailscale, agent status over OSC 7501, gated send and wait |

Herdr is never modified (`AGENTS.md`). The daemon is a client of herdr-core.

## What herdr-core provides

herdr-core is built to be driven by other programs: anything that connects to
its socket can start, label, message, and watch terminals. Herding already
does through `crates/sheprd-core-client`. The design uses calls that client
has; it adds nothing to Herdr.

| Call | What it does | What Teams uses it for | Evidence |
| --- | --- | --- | --- |
| `terminal.run` | Starts a program in a new terminal with environment, labels, and a working directory; `on` starts it on a machine the core has added | `team spawn` | in the client; `sheprd-automate` uses it locally. Environment and labels with `on`: spike B |
| `terminal.label`, the `labels` field | Labels are kept on the terminal by the core and listed with it | The member's identity outlives our processes | in the client. Survival across a core restart: spike A |
| `terminal.list` | Every terminal with labels, state, foreground program, `pid`, and the reported status | Roster and liveness; finding a member by label | in the client |
| `terminal.send` | Types text and submits it. `if` makes the core type only while the status still matches; a program blocked on a dialog is refused unless forced | The doorbell | plain send tested live (spike 1); `if` and the dialog refusal: spike C |
| `Sent`, the reply to a send | Whether the program went working within a few seconds, its status, and the turn | Delivery receipt (#62) | in the client; per agent: spike C |
| `terminal.wait` | Waits until the program stops working, exits, shows expected output, or a timeout passes | Following a turn without reading the screen | in the client; `sheprd-automate` uses it |
| `events.subscribe` | A stream of changes: terminals created, finished, lost, restarted; labels, status, and turn changes; document changes; and a `Missed` notice after a gap | The delivery loop reacts when a member goes idle | in the client. Remote terminals: spike B |
| `hc machine add` | Adds a machine over SSH; its terminals get addresses like `m1/t7` | Remote members | tested live (spike 1), by LAN name |

`HC_TERMINAL_ID` is read by the `herding` CLI (`control/client.rs`,
`caller_from`) and by `sheprd-automate`, whose comments say a herdr-core
terminal names itself there. `sheprd-core-client` itself does not read it,
and this revision did not confirm on a live core that herdr-core sets it:
spike D.

Not used: herdr-core's versioned document store (`doc.get`, `doc.set`, with
change events). SQLite already does what the team records need; a document
could later publish a read-only roster to other clients.

Herdr 1 has no labels, gated send, or event stream, which is why new members
are herdr-core terminals and a Herdr 1 pane only pulls.

### One cycle

1. `team spawn --name zcode2 --machine m1` calls `terminal.run` with `on`,
   the member labels, and the team's environment. The agent starts knowing
   who it is.
2. The supervisor assigns a task. `apply` writes the task and an inbox row
   in one transaction. herdr-core is not involved.
3. The daemon, on `events.subscribe`, sees zcode2 idle and calls
   `terminal.send` with `if: idle` and the doorbell text.
4. `Sent` says zcode2 started working. The daemon sets `delivered_at`.
5. zcode2 runs `sheprd-team inbox`, reads the task, and the row is marked
   read.

## Why the earlier plan changed

The earlier plan moved the control-socket handlers to a service socket,
rewrote the bridge against the `hc` command line, and opened a spike on
proving caller identity through Herdr's API. Reading the code showed three
things:

- **The typed client exists.** `crates/sheprd-core-client/src/wire.rs` already
  models what delivery needs: `SendText.only_if` (the core checks the status
  as it types and refuses with `status_mismatch`), a refusal when the program
  is blocked on a dialog unless `force` is set, `Sent { started, status, turn }`
  as a receipt, `Run { env, labels, on }` to start a terminal with environment
  and labels on another machine, and `events.subscribe`. `sheprd-automate`
  already starts and prompts agents with it (`agent::launch`).
- **The socket only exists because the window is in the path.** `scoped()` in
  `control/server/team_worker.rs` is already a GPUI-free "apply this op as
  this actor". The UI-thread identity proof, `Watches`, the job queue, and a
  second validation layer in `control/server/team.rs` surround it only to
  cross from the socket into the window and back.
- **Identity proof costs more than it gives.** `docs/herding-team-review.md`
  records that process pinning holds on Linux only; macOS, Windows, and the
  tunnel are uid-only. It produced #3, #42, and #43. Wake prompts meanwhile
  tell the agent to assert identity with `HERDR_PANE_ID=… /abs/path/herding
  team` (`team_cli_prefix`), which is also the cause of #86.

`docs/session-model.md` says to add a daemon only for something that must run
while the app is closed and that neither `doc.*` nor `sheprd-automate` can
hold. Doorbells qualify: they must ring with no window, a core document cannot
type into a terminal, and `sheprd-automate` is upstream's crate, so putting
team delivery in it would conflict on every merge.

## Design

### The crate

`crates/sheprd-team`, a library and a `sheprd-team` binary:

| Module | Holds |
| --- | --- |
| `types` | `Op`, `Reply`, `Actor`, `Address`, record types; serde only, built without the `store` feature |
| `store` | the SQLite store moved from `sheprd-orchestrator/src/team_store*`, schema versions, the inbox |
| `apply` | `apply(&mut Store, &Actor, Op) -> Result<Reply>`: resolution, permission, the write, and its inbox rows in one transaction |
| `identity` | the caller's member from the environment |
| `deliver` | the doorbell loop and the delivery enum |
| `remote` | the bounded one-request socket for remote members |
| `main.rs` | the CLI and `daemon` |

Dependencies stay at `rusqlite`, `serde`, `chrono`, `uuid`, `thiserror`,
`tracing`, and `sheprd-core-client`. No GPUI, `gix`, or HTTP client, so it
builds in minutes with `cargo build -p sheprd-team` and can later move to its
own repository with one `git filter-repo --path` (see "Repository").

Each type is defined once here. `ConfidenceLevel`/`Confidence` and
`TaskState`/`TaskStatus` collapse to one enum each.

### Records

- Every caller opens `<state_dir>/team/team.sqlite3` itself. WAL and the
  5-second busy timeout already make that safe across processes, and
  `claim_task` is already a conditional update with one winner.
- Schema changes only add: new tables, and new columns that are nullable or
  have a default. No existing column changes its type or meaning while two
  binaries share the file. Today's window has no version check and will
  write to a newer file, so adding only is what keeps it from damaging new
  rows.
- From phase 2 on, every binary checks `PRAGMA user_version` and refuses a
  newer version with a clear message instead of writing.
- The window reads on its existing worker thread and watches
  `PRAGMA data_version` for changes instead of taking a snapshot every two
  seconds. Its writes go through `apply` as `Actor::Human`.
- Validation happens once, in `apply`.

### Identity

- `team spawn` starts the terminal with `HERDING_TEAM` and `HERDING_MEMBER`
  in its environment and `herding.team` / `herding.member` labels on the
  terminal, and stores the terminal as the member's address.
- The CLI resolves the caller in this order: `HERDING_MEMBER`, checked
  against `HC_TERMINAL_ID` when both are set; else the member whose address
  is the caller's `HC_TERMINAL_ID` (or `HERDR_PANE_ID` for a Herdr 1 pane).
- `register` remains for an agent that was not spawned by the team: it binds
  the caller's own terminal as the address.
- A member is `member_id` plus a mutable address:

  ```rust
  enum Address {
      Core { machine: Option<String>, terminal: String },
      Herdr1Pane { pane: String }, // identifies the caller; never rung
      None,
  }
  ```

  Rebinding an address is an ordinary update by the member itself, the
  supervisor, or the user. `MemberTargetChanged`, boot ids as hosts, and
  `live_hosts` go away, and with them the reattach gap in the skill and #52.
- One terminal holds one membership. An agent in two teams uses two terminals
  (#48).

### Trust model

Same user, on every platform. Any process of the user can act as any member,
the supervisor included, by setting the environment. Identity protects
against mistakes, not against a hostile local process. This gives up the
Linux process pinning; macOS, Windows, and the tunnel never had it.
`docs/cli.md` must say so. Three things bound what a wrong or borrowed
identity can do:

- **Role changes and removals need the user.** No member, the supervisor
  included, can change a role or remove or replace a member on its own word
  (see "Approvals").
- **Every write is logged** with the actor and the terminal it came from
  (see "Audit").
- **Check the environment against the core where possible.** Spike D asks
  whether the CLI can confirm the terminal it runs in from herdr-core
  instead of trusting `HC_TERMINAL_ID`. If it can, a mismatch is refused.

### Roles

`members.role` is `supervisor` or `worker`. A handoff is one update, run only
once the user approves it (#15, #49). `Team { supervisor, members }` becomes
`Team { members }`.

### Audit

```
audit(seq, team_id, actor, terminal, op, ref_id, at)
```

`apply` appends one row per write in the same transaction: who acted, from
which terminal, which op, on which record. Rows are never updated or deleted
by the CLI. `sheprd-team log TEAM` reads them. A borrowed identity then
shows up as a member writing from a terminal that is not its address.

### Inbox and doorbell

One table replaces mention acknowledgements, assignee signals, supervisor
wake signals, and the three in-memory queues:

```
inbox(seq, team_id, member_id, kind, ref_id, created_at, delivered_at, read_at)
kind: assigned | mention | task_done | task_blocked | notice
```

- `apply` writes inbox rows in the same transaction as the record that causes
  them. Write-then-wake is then atomic, not a hold queue.
- Who gets a row: the assignee on assign; each mentioned member on a post;
  the supervisor when a task is completed or blocked by someone else (#50);
  everyone on a `notice`, which is how a freeze is broadcast (#53).
- `sheprd-team inbox` prints the caller's unread rows with enough to act on
  (task title and brief, post text and author, result excerpt) and marks them
  read. `--peek` leaves them unread.
- `bulletin ack` stays as an alias that marks that row read.
  `acknowledgements` becomes a view over `inbox.read_at`.
- The bulletin is visible to the whole team. Mentions decide only who gets an
  inbox row (#83).
- The doorbell text is fixed and carries no ids, paths, or content:
  `[Herding Team] You have N new item(s). Run: sheprd-team inbox`. A late or
  repeated doorbell cannot be stale (#60), and no path can go missing (#86).

### Delivery

The daemon is the only process that rings. Per member it keeps one row:

```
doorbell(team_id, member_id, rung_through_seq, rung_at, attempts, outcome)
```

Loop, driven by `events.subscribe` with a slow timer as a fallback:

1. For each member with rows where `delivered_at` is null, not paused, with
   an address:
2. `Core`: `terminal.send { if: "idle", text: doorbell }`.

   | Result | Action |
   | --- | --- |
   | `started: true` | set `delivered_at` on the rows covered |
   | `started: null` (program reports no status) | set `delivered_at`, record `outcome = unconfirmed` |
   | `started: false` | count the attempt, back off, try again |
   | refused `status_mismatch` | wait for the terminal's next idle event |
   | refused `blocked` | record `outcome = blocked`; shown on the roster; wait |
   | terminal gone | record `outcome = unreachable`; rows stay for a rebind |
3. `Herdr1Pane` and `None`: nothing. The member pulls.
4. Rows delivered but unread for 10 minutes while the member is idle ring
   again, at most three times. After that the member is shown as stalled and
   the supervisor gets a `notice` (#16, #23).

A `Missed` notice, or a reconnect, means events were skipped: the daemon
reads `terminal.list` again before it decides anything.

Inbox rows are never dropped. Only doorbells are retried, and every outcome is
a stored value logged at info (#35, #59, #62).

- One instance per database, held by a lock file beside it. `team create` and
  `team spawn` start it detached when the lock is free; a user service is
  optional. `team status` says when no daemon holds the lock.
- Pause is `paused_at` on the team or member, honoured for every team (#41).
- Liveness is read, not guessed: the core's reported status, the terminal's
  state, `last_seen_at` stamped by every CLI call, and the age of the oldest
  undelivered row. `team heartbeat` reports those and stops reading
  scrollback (#45, #47, #55).

### Spawn

`sheprd-team spawn TEAM --name NAME --agent KIND --cwd PATH [--machine M]
[--brief TEXT] [--resume ID]`:

1. checks limits (member cap, allowed machines);
2. checks that `sheprd-team` resolves on the target's `PATH` (#51);
3. inserts the member, then starts the terminal with `agent::launch`, the
   environment, the labels, and the brief as its first prompt;
4. stores the terminal address. A failed start removes the member row.

After a reboot, `spawn --resume` on an existing member name rebinds that
member, keeping its role, assignments, and inbox.

### Remote members

- The store has one authority: the host that holds the database.
- The daemon serves `remote`: one bounded JSON `Op` in, one `Reply` out, on a
  Unix socket.
- `scripts/control-tunnel.sh` forwards that socket to the member's host
  instead of the app's control socket. One forwarded socket per machine, so
  the daemon knows the machine a request came from.
- `spawn --machine` sets `HERDING_TEAM_SOCKET` in the member's environment.
  The CLI sends the op there when it is set, and opens the database when it
  is not.
- Outbound doorbells go through the local core to `machine/terminal`
  addresses, as spike 1 showed.

### Approvals

Removing or replacing a member and changing a role are never run on a
member's word, the supervisor's included. They are recorded:

```
requests(request_id, team_id, kind, payload, requested_by, created_at, decided_at, decision)
```

`sheprd-team pending` lists them and `sheprd-team approve REQUEST_ID` runs
one. `approve` refuses a caller that resolves to a member. Under the same-user
trust model that stops an agent approving by mistake, not on purpose.

Decided 2026-11-13: that is enough for now. Agents in a team are the user's
own and the risk is mistakes, not malice, so there is no out-of-band
confirmation (no phone, no window dialog). The audit log records every
request and approval, which is how a wrong one is found and undone. Revisit
if teams ever include agents the user does not run.

### Command line

`herding team …` and `sheprd-team …` run the same code. The second exists so
a host without the app needs only a small binary.

| Command | Change |
| --- | --- |
| `status`, `roster`, `goal set`, `bulletin list/post`, `task list/create/assign/claim/block/complete/cancel`, `finding list/add` | unchanged, in-process |
| `inbox [--peek]` | new |
| `bulletin ack` | alias for reading that inbox row |
| `create`, `spawn`, `rebind`, `pause`, `resume`, `log` | new |
| `handoff`, `remove` | new; record a request the user approves |
| `heartbeat` | reads liveness; `--ping` removed |
| `pending`, `approve` | new, phase 3 |
| `watch` | new, phase 6 |
| `daemon` | new |
| `adopt TEAM` | new, moves a team to daemon delivery |

`--json` stays on every command.

## What is removed

At phase 5, once no team uses the old path:

- `control/server/team.rs`, `team/identity.rs`, `team/dispatch.rs`,
  `team/heartbeat.rs`, `team_service.rs`, and the queueing half of
  `team_worker.rs`;
- `team/bridge/queue.rs`, `team/bridge/prompts.rs` (all but pane focus and
  spawn glue), and the wake, pause, and ledger state in `team/bridge.rs`;
- `TeamOp` from the control protocol, and `control/client/team.rs`;
- `PeerBindings`, `ProcessIdentity`, `agent_lineage`, `check_peer`;
- the legacy `host`, `session`, `workspace`, `pane` columns and
  `teams.supervisor_id`;
- the skill's wake workaround, multi-machine, and reattach sections.

The Teams window (`team/view*`) stays, fed by a thin reader.

## Phases

Each phase leaves a working system. Until phase 5 the running team stays on
today's path, per team: `teams.delivery` is `legacy` or `daemon`. The window
rings only legacy teams, the daemon only daemon teams, and `apply` writes
inbox rows for both.

**0. Prerequisite.** #81 merges. After it, today's Teams gets bug fixes only.

**1. Extract.** Create `sheprd-team` from `team_store*` and `scoped()`, with
one set of types and one validation layer. `apply` takes an `Actor` from the
start. `sheprd-control-protocol` uses the `types` module without the `store`
feature; the wire stays byte-identical. The app's window `Reply` is split
from `apply`'s `Reply`. Add the crate to the table in `AGENTS.md`.
*Gate:* existing team tests pass with only import changes (1.5 is the one
behavior change, and a test pins the error ordering); wire fixtures
unchanged; `cargo tree -p sheprd-team` shows no GPUI or `gix`.

**2. Schema and inbox.** `user_version`; `role`, address, `last_seen_at`,
`paused_at`, `delivery`, `model`, and `account` columns; the `inbox`,
`doorbell`, and `audit` tables; `apply` writes inbox and audit rows. Legacy
teams behave as before. (`requests` moves to phase 3 with `pending` and
`approve`.)
*Gate:* a database from today opens, migrates, and the legacy team works
unchanged, including with v1.0.0 writing to the migrated file; assign,
mention, complete, and block each produce exactly the rows above, tested; a
newer `user_version` is refused.
*Closes:* #48, #80.

**3. The new path.** The `sheprd-team` binary: in-process ops, environment
identity, `create`, `spawn`, `inbox`, `rebind`, `pause`, `log`, `handoff`
and `remove` as requests with `pending` and `approve`, and `daemon` with
`Core` delivery. Daemon teams only; ops on a legacy team are refused with
"adopt first". Needs spikes A, C, and D.
*Gate:* with the Herding window closed, on core terminals on one host: create
a team, spawn two workers, assign a task; the doorbell lands only when idle
and records a receipt; the worker reads its inbox, completes; the supervisor
is rung. Kill the daemon mid-run and restart it: no inbox row lost or
duplicated. A send into an open dialog is refused and shown as blocked. A
worker's `handoff` changes nothing until the user approves it.
*Closes for daemon teams:* #35, #41, #42, #43, #50, #53, #59, #60, #62, #84,
#86.

**4. Remote.** `spawn --machine`, the `remote` socket, the tunnel change.
Needs spike B.
*Gate:* a worker on m1 receives a doorbell, completes a task, and posts to
the bulletin, with the supervisor's window closed.
*Closes:* #8, #40.

**5. Cut over and delete.** `adopt` backfills inbox rows for unacknowledged
mentions and open assigned tasks and flips a team to `daemon`. Today's team
panes are all Herdr 1, so each member is started once on herdr-core with
`spawn --resume`, which keeps its conversation; a member left on a Herdr 1
pane pulls. `herding team` becomes the in-process path. Delete
the list above. Rewrite the skill and `docs/cli.md`.
*Gate:* the live team runs a full day on the new path; the old modules are
gone; `just check-file-size` passes without the #39 exceptions.
*Closes:* #39, #45, #47, #52, #55, #83.

**6. Headless and UI.** `watch`, spawn limits, and a cost budget (#73). Then the Teams mod spike in Herdr's official app, as a
client that runs `sheprd-team … --json`. Then the repository decision.

**Team templates.** `team create --template NAME` starts a preset roster:
each member with a name, role, agent kind, model and account, worktree
policy, and a standing brief. Two shapes to start: `small` (a supervisor and
3 workers) and `review crew` (a supervisor, a reviewer, a planner, and 3
workers - today's team). It depends on phase 3's `spawn`, phase 2.2's `role`
column (decide whether reviewer and planner are roles or
supervisor/worker plus a brief), and the model/account columns folded into
2.2 from the parked roster profile.

### Phase 1 and 2 tasks

Each task is one day or less at its review tier. AGENTS.md's tiering applies:
a full diff is read for the store, delivery, protocol, and security, and a
cheaper worker may do the rest with the full diff reviewed.

Before 1.1, every branch that touches `team_store*` must land or be parked:
PR 88 (issue sync store) and PR 90 (layer D audit) land first; #82 stall
detection, #57 zero-touch dispatch, #58 supervisor handoff, layers A and B,
and the roster profile are parked as `archive/` tags. Never start the
extraction while one is in flight.

#### 1.1 Scaffold `sheprd-team` and move the record types

- **Scope.** `crates/sheprd-team/Cargo.toml` (workspace deps: `serde`,
  `serde_json`, `thiserror`, `chrono`, `uuid`, `tracing`; `rusqlite` behind a
  default `store` feature), `src/lib.rs`, `src/types.rs`. Move the record
  types out of `sheprd-orchestrator/src/team_store.rs` and
  `sheprd-control-protocol/src/protocol/team.rs`: `Member`, `Team`,
  `SessionTarget`, `Author`, `Post`, `Task`, `Finding`, `RecipientStatus`,
  `Acknowledgement`, `Confidence`, `TaskStatus`, `DeliveryState`,
  `ConfidenceLevel`, `TaskState`. `sheprd-orchestrator` re-exports them so its
  callers change by import only. `sheprd-core-client` is not a dependency
  until phase 3 needs it.
- **Acceptance.** `cargo build -p sheprd-team --no-default-features`;
  `cargo test -p sheprd-orchestrator --lib`; `cargo test -p
  sheprd-control-protocol`; `cargo tree -p sheprd-team --edges normal | grep
  -Ei 'gpui|gix'` prints nothing; `cargo fmt --all -- --check`; `bash
  scripts/check-file-size.sh`.
- **Depends on.** The pre-phase-1 branches.
- **Review.** Full (protocol/store types). A cheaper worker can do it with
  the full diff reviewed. Decision: `sheprd-control-protocol` depends on
  `sheprd-team` with default features off (types only, no rusqlite); revisit
  if the tree grows.

#### 1.2 Move the wire `Op` and `Reply`

- **Scope.** `Op` (from `TeamOp`), `Reply`, `MemberInfo`, `TaskInfo`,
  `FindingInfo`, `BulletinEntry`, `TeamInfo` into `sheprd-team::types`;
  `protocol/team.rs` keeps `Call`, `Request`, and `Response` and re-exports
  the moved types; `delete_gate.rs` follows. No field, tag, or `rename_all`
  change.
- **Acceptance.** `cargo test -p sheprd-control-protocol` (including
  `team_requests_have_stable_wire_forms` and the reply round trip); `cargo
  test -p sheprd-gpui --bin herding team`; `cargo clippy -p sheprd-team -p
  sheprd-control-protocol --all-targets -- -D warnings`.
- **Depends on.** 1.1.
- **Review.** Full (wire). A cheaper worker can do it with the full diff
  reviewed; any serde attribute change is a stop.

#### 1.3 Move the store into `sheprd-team::store`

- **Scope.** `sheprd-orchestrator/src/team_store.rs` and `team_store/*`
  (`findings`, `mentions`, `tasks`, `schema`, `watch`, every `*_tests`) into
  `sheprd-team/src/store*`; `sheprd-orchestrator` re-exports or its callers
  switch to `sheprd_team::store`; tests stay with the module.
- **Acceptance.** `cargo test -p sheprd-team`; `cargo test -p
  sheprd-orchestrator --lib`; `cargo test -p sheprd-gpui --bin herding team`;
  `cargo tree -p sheprd-team --edges normal | grep -Ei 'gpui|gix'` empty;
  `bash scripts/check-file-size.sh`.
- **Depends on.** 1.1.
- **Review.** Full (store). A cheaper worker can do it; the move is
  mechanical.

#### 1.4 Move `scoped()` into `apply`, with an `Actor`

- **Scope.** `ScopedAction`, the scoped `Reply` variants, and `scoped()`
  (`control/server/team_worker.rs`) into `apply(&mut Store, &Actor, Op) ->
  Reply`. Split the app's window `Reply` (`Snapshot`, `TaskPage`,
  `FindingPage`) from `apply`'s `Reply`; the window's non-scoped `Operation`s
  stay app-side. `Actor` is the proven member the app already has at this
  point.
- **Acceptance.** `cargo test -p sheprd-gpui --bin herding team`; `cargo test
  -p sheprd-team`; `cargo clippy -p sheprd-team -p sheprd-gpui
  --all-targets -- -D warnings`.
- **Depends on.** 1.3.
- **Review.** Full (delivery/identity). A cheaper worker can do it with the
  full diff reviewed; the `Reply` split is decided (app versus `apply`).

#### 1.5 One validation layer, app wiring, docs

- **Scope.** Move the `TeamOp` bounds checks from `control/server/team.rs`
  into `apply`; keep the app's identity proof where it is; wire
  `team_worker`; the `Cargo.toml`s; the `AGENTS.md` crate table; `docs/cli.md`.
  This is phase 1's only behavior change: validation first, as today, pinned
  by a test.
- **Acceptance.** `cargo test -p sheprd-gpui --bin herding`; `cargo test -p
  sheprd-control-protocol`; `cargo clippy -p sheprd-team -p sheprd-gpui -p
  sheprd-control-protocol --all-targets -- -D warnings`; `cargo fmt --all --
  --check`; `python3 scripts/check-sorted.py`; `python3
  scripts/check-i18n.py`; `git diff --check`. Phase gate: `cargo tree -p
  sheprd-team --edges normal | grep -Ei 'gpui|gix'` empty.
- **Depends on.** 1.4.
- **Review.** Full (security: error ordering). Not for a cheaper worker.

#### 2.1 `user_version` and the migration framework

- **Scope.** `store/schema.rs`: `SCHEMA_VERSION`, read, check, and write
  `PRAGMA user_version` in the open transaction, `Error::SchemaTooNew {
  found, supported }`, and stepwise additive migrations. Version 1 is the
  baseline PR 88 created: the four `tasks` issue columns and
  `team_issue_watch`; a database without a version migrates to it.
- **Acceptance.** `cargo test -p sheprd-team store::schema`; a test opens a
  file with `PRAGMA user_version = 99` and expects `SchemaTooNew`; `cargo
  clippy -p sheprd-team --all-targets -- -D warnings`.
- **Depends on.** 1.3.
- **Review.** Full (store). A cheaper worker can do it with review.

#### 2.2 Member and team columns

- **Scope.** Additive DDL:
  - `members.role TEXT NOT NULL DEFAULT 'worker'`; the migration sets the
    supervisor's role from `teams.supervisor_id`.
  - `members.address_kind TEXT NOT NULL DEFAULT 'none'` (`none`, `core`, or
    `herdr1`; no CHECK, so a new kind never needs a table rebuild),
    `members.address_machine TEXT` (the core machine name; the endpoint id
    for a Herdr 1 row), and `members.address_terminal TEXT` (a core terminal
    id or a Herdr 1 pane id). The migration makes a row with a pane
    `('herdr1', host, pane)` and a row without `('none', NULL, NULL)`. The
    legacy `host`, `session`, `workspace`, and `pane` columns stay until
    phase 5.
  - `members.last_seen_at TEXT`, `members.paused_at TEXT`.
  - `members.model TEXT`, `members.account TEXT` (from the parked roster
    profile branch).
  - `teams.delivery TEXT NOT NULL DEFAULT 'legacy'`.
  - Types: `Member.role`, `Address`. `Team` keeps `{ supervisor, members }`
    through phase 2.
  - **Legacy teams keep reading the legacy columns.** The address columns are
    backfilled once at migration, but v1.0.0 keeps writing
    `host`/`session`/`workspace`/`pane` afterwards (`insert_member` is an
    upsert that leaves the new columns alone), so a v1.0.0 re-register, rebind,
    or new member would leave `address_*` stale or `none`. While
    `teams.delivery = 'legacy'`, the legacy columns stay authoritative and the
    new binary resolves the member from them; `address_*` is read only when
    `delivery != 'legacy'` (or the address is derived at read time). Phase 5's
    `adopt` is where a team flips to the address columns.
- **Acceptance.** `cargo test -p sheprd-team` (migration: old database gains
  the supervisor role, `delivery = 'legacy'`, and the address backfill);
  `cargo test -p sheprd-control-protocol` (wire unchanged); `cargo test -p
  sheprd-gpui --bin herding team`.
- **Depends on.** 2.1.
- **Review.** Full (store). Not for a cheaper worker.

#### 2.3 Inbox table and `apply` writes

- **Scope.** `inbox(seq, team_id, member_id, kind, ref_id, created_at,
  delivered_at, read_at)` with indexes; `apply` writes one row per event: the
  assignee on assign, each mentioned member on a post, the supervisor on a
  complete or block by someone else, and everyone on a notice; store read and
  mark-read.
- **Acceptance.** `cargo test -p sheprd-team inbox`; `cargo test -p
  sheprd-gpui --bin herding team`; `cargo clippy -p sheprd-team
  --all-targets -- -D warnings`.
- **Depends on.** 2.1, 1.4.
- **Review.** Full (store/delivery). Not for a cheaper worker.

#### 2.4 Audit table, unified with layer D

- **Scope.** Start from PR 90, which already creates `audit(seq, team_id,
  actor TEXT NOT NULL DEFAULT 'unknown', terminal, op, ref_id, at,
  display_name, host, session, workspace, pane)` with `(team_id, seq)` and
  `(team_id, ref_id, seq)` indexes and `ref_id = member_id` for identity ops.
  Phase 2 fills `actor` and `terminal` from `apply`, logs `delete_team`, and
  edits this plan's Audit section to add the snapshot columns and the delete
  row so the plan and the schema do not diverge. `identity_history` reads
  `audit WHERE ref_id = member AND op IN ('registered', 'rebound',
  'removed')`.
- **Acceptance.** `cargo test -p sheprd-team audit`; PR 90's six identity
  tests adapted and green; `cargo test -p sheprd-gpui --bin herding team`; the
  plan's Audit section matches the DDL.
- **Depends on.** 2.1, 1.4, PR 90 landed.
- **Review.** Full (store/security). Not for a cheaper worker.

#### 2.5 Doorbell table (schema only)

- **Scope.** `doorbell(team_id, member_id, rung_through_seq, rung_at,
  attempts, outcome)` and its store types; no loop and no command (phase 3).
  `requests` moves to phase 3 with `pending` and `approve`.
- **Acceptance.** `cargo test -p sheprd-team` (a fresh database has the
  table; the migration adds it); `bash scripts/check-file-size.sh`.
- **Depends on.** 2.1.
- **Review.** Full (store), small. A cheaper worker can do it.

#### 2.6 Legacy compatibility gate

- **Scope.** Tests plus a scripted check: a v1.0.0 database migrates; the
  legacy team path works; v1.0.0 writes to the migrated file; a v1.0.0
  re-register on the migrated file leaves the legacy columns authoritative
  and the new binary still resolves the right pane (2.2's rule); assign,
  mention, complete, and block each leave exactly the inbox and audit rows.
- **Acceptance.** `cargo test -p sheprd-team legacy`; check `df -h /home` for
  the 15G floor, then build v1.0.0 once in its own worktree with its own
  `CARGO_TARGET_DIR` on disk (never `/tmp`, which is tmpfs): `git worktree add
  ~/repos/forks/herding-v1 v1.0.0 && CARGO_TARGET_DIR=~/.cache/herding-v1-target
  cargo build --manifest-path ~/repos/forks/herding-v1/Cargo.toml -p
  sheprd-gpui --bin herding`; run that binary's `team register`, `team task
  list`, and `team task claim` against a copy of the migrated database, then
  the new binary's `team status` against the same copy and confirm it resolves
  the registered pane; delete the target directory and the worktree when done;
  `cargo test -p sheprd-gpui --bin herding`.
- **Depends on.** 2.2, 2.3, 2.4, 2.5.
- **Review.** Full (store). Not for a cheaper worker.

Later, on a delivery layer that holds: structured results and scope (#28,
#24), `land` (#22), priority and WIP limits (#27), waiting on a task (#54),
pruning (#74).

## Spikes

Research only, in a scratch `HOME` with a private `HC_SOCKET`. A, C, and D
gate phase 3; B gates phase 4.

- **A. Restart.** Do a terminal's id and labels survive a herdr-core restart
  while its holder process lives? If ids change, the label is the lookup key
  and the stored address is a cache.
- **B. Remote spawn.** Do `Run.env` and `Run.labels` reach a terminal started
  with `on` another machine, and does `events.subscribe` report that
  terminal's status changes?
- **C. Receipts per agent.** For ZCode, Claude Code, and Codex: does
  `send { if: "idle" }` return `started: true`, and is a send into a
  permission dialog refused as `blocked`? Spike 1 found ZCode reports status
  only with its plugin's environment set.

- **D. Terminal from the core.** First confirm that herdr-core sets
  `HC_TERMINAL_ID` in every terminal it starts, locally and with `on`. Then:
  can the CLI learn which terminal it runs in from herdr-core, not from its
  environment? Candidates: the core answers
  for the connecting process, or `terminal.list` gives each terminal's `pid`
  and the CLI checks that its own ancestry reaches it. Say what holds on
  Linux, macOS, and through the tunnel. If nothing holds, the trust model
  stands on approvals and the audit log alone.

## Repository

Start as a crate in this workspace. The types will change in phases 2 and 3,
and a second repository would turn each change into a tag and a version bump
here. A crate here does not build GPUI: `sheprd-automate`, the same shape,
resolves to 121 crates against 914 for the app.

Decide on a separate repository (`herding-teams`) at phase 6, when the mod
needs it and the wire has settled. If it splits, it depends on upstream
`penso/herdr-gpui` for `sheprd-core-client`, not on this fork.

Split earlier only if fork CI time, or cloning this repository onto small
machines, becomes the actual obstacle.

## Decisions

- The store is a library with one authority host. Per-host stores that sync
  can follow if partitions need them.
- Trust is same-user, on every platform, and documented as such.
- New members are herdr-core terminals. A Herdr 1 pane can be a member but
  is never rung: it pulls. Nobody's session is moved automatically; moving
  the current team is one `spawn --resume` per member at phase 5.
- Merge #81: the team needs it to re-register on today's path. After it, no
  identity or wake work in the window.
- Role changes and removals always need the user's approval.
- Findings stay a separate record for now. Folding them into posts is open.

## Risks

- A wrong status from a harness still decides when a doorbell lands. The
  gated send moves the check into the core but cannot fix a harness that
  reports idle mid-turn (#61) or working after a failure (#56). ZCode is
  reported to need a fix to its Herdr plugin (cited as upstream PR #275,
  not checked for this revision) before its status can be relied on.
- Several binaries on one database: an old `herding` beside a new
  `sheprd-team`. `user_version` refuses the unsafe direction; rollouts must
  update both.
- No daemon running means no doorbells. `status` reports it, and the pull
  contract in the skill still holds.
- A reboot ends every terminal. Recovery is `spawn --resume` per member, by
  the supervisor or the user.

## Not verified

- Everything said about herdr-core comes from `crates/sheprd-core-client`
  (types and doc comments) and spike 1, not from herdr-core's source or a
  run made for this revision.
- Whether a doorbell typed into a `sheprd-acp` chat pane is taken as a
  prompt. Today those members are prompted through the window's chat
  connection, which the daemon does not have.
- Whether the per-machine forwarded socket identifies the machine reliably
  when two tunnels are open.
- That `spawn --resume` keeps the conversation for each harness in use
  (`claude --resume`, `zcode --resume`) when started on herdr-core.
- The official app's UI stack and what its JavaScript mods can run.

## History

**Spike 1 (2026-11-13, zcode3, board task `bfe3cff1`).** `hc send` passed:
prompts over 2,700 bytes arrived intact in idle ZCode and Claude agents,
locally and on m1 through `hc machine add`. Two catches: a send into an open
dialog types into the dialog unless status is checked, and machines must be
added by LAN name (m1's Tailscale name did not answer; m2 is unreachable).
