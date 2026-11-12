# Upstream proposal: per-pane content revision in `ClientShellSnapshot`

Status: draft for the user to decide whether it goes upstream. Nothing here
has been sent to `penso/herdr-gpui`; upstream stays a read-only reference
(AGENTS.md). Written from Herding's fork after the multi-machine wake work
(issues #40, #62) and stall detection (#56), which needed a signal the
client protocol does not currently carry.

## The ask

Add the daemon's per-pane content revision to the client-shell snapshot:
`ClientShellPane` (crates/sheprd-protocol/src/wire.rs) gains one field,
`content_revision: u64`, carried with every snapshot the daemon already
pushes. The daemon tracks this counter per pane today - `herdr pane list`
reports it as `revision` for every pane, rendered or not - and the surface
frame reports the same counter (`PaneSurfacePane::content_revision`) for the
panes it renders. The ask is only to widen an existing push by one number per
pane.

```diff
 pub struct ClientShellPane {
     pub pane_id: String,
     pub workspace_id: String,
     pub tab_id: String,
     pub label: Option<String>,
     pub cwd: Option<String>,
     pub foreground_cwd: Option<String>,
     pub focused: bool,
     pub right_click_passthrough: bool,
+    /// The pane's terminal content revision: bumped on every redraw of this
+    /// pane's content, whatever the client renders. Same counter
+    /// `pane copy_search` validates staleness against.
+    #[serde(default)]
+    pub content_revision: u64,
 }
```

## Why the client cannot observe unrendered panes today

Verified against herdr 0.9.1 (the daemon this fork's fleet runs) and this
fork's protocol source:

1. `ClientShellSnapshot` pushes no per-pane counter. `ClientShellPane` is
   ids, labels, cwd and focus; the snapshot's own `revision` field is
   whole-snapshot, so it says nothing about which pane moved.
2. `PaneSurfaceFrame` carries `content_revision` per pane, but the daemon
   only renders the panes the window shows. A member pane in a background
   tab, or on a remote daemon whose panes this window never displays, has no
   frame at all.
3. The client protocol has no pane-list method: the `Method` catalog
   (crates/sheprd-client/src/method.rs) carries pane input, copy, focus and
   scroll methods, but nothing that asks the daemon for pane state. The
   daemon's own CLI answers `pane list` in ~3ms with one `{pane_id,
   revision}` per pane - but it speaks the daemon's JSON API on a different
   socket (`herdr.sock`), not the client-shell protocol
   (`herdr-client.sock`), so a client cannot reach it over the connection it
   already holds.

## What this unblocks

- **Team stall detection without a per-host poll.** Herding wakes a
  supervisor when a member holding assigned work goes quiet (issue #56's
  stall detection, landed on the fork as team/stall-on-wakes-zcode2). The
  signal is a per-pane content counter - status is only a hint, because
  harnesses report `idle` mid-turn (issue #61), and terminal text is never
  read (issue #55). With the revision in the snapshot, the client-side
  CLI-per-host poll (`herdr pane list` every few seconds per endpoint,
  subprocess + JSON parse) becomes unnecessary; the counter arrives pushed,
  for every pane, on the connection the client already holds.
- **Cheap pane liveness anywhere in the client.** Anything that wants to
  know "did this pane produce output since I last looked" - attention
  markers, notification dedupe, activity summaries - can read one field
  instead of diffing rendered frames or asking the CLI.

## Compatibility

- The field is additive and `#[serde(default)]`: older clients reading newer
  daemons ignore unknown fields (the protocol already relies on this); newer
  clients reading older daemons see 0. No handshake or version bump needed
  beyond what additive snapshot fields already do.
- The counter is the same one the daemon keeps for staleness checks; clients
  must treat it as per-pane and unordered across panes, never as a clock.
- Bandwidth: one integer per pane per snapshot push, and the daemon already
  pushes snapshots on change - it does not repaint for the field.

## Evidence

`herdr pane list` on herdr 0.9.1 (2026-11-12), abridged:

```json
{"id":"cli:pane:list","result":{"panes":[
  {"pane_id":"w22:p3","agent":"zcode","agent_status":"working",
   "revision":74837,...},
  {"pane_id":"w22:p6","agent":"claude","agent_status":"idle",
   "revision":4,...}
]},"type":"pane_list"}
```

The first pane is a background tab of this very session; its revision climbs
on every redraw although nothing renders it. The second is an idle pane that
has moved four times since boot. Both are exactly the counters a client
cannot see in the snapshot today.
