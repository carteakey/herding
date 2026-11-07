# Herding brand brief

Herding V2's identity: the name, mark, palette, type, voice, and the OS-level
app identity. Written for issue herding#33; the direction it implements is
"team coordination, not terminal multiplexer", distinct from the upstream
Herdr client this fork came from.

## Name

**Decision: keep Herding.** Rationale:

- The name already says what V2 is: herding is coordinating many independent
  agents toward one goal — the product's entire premise. A rename would spend
  recognition we have not banked (one release, one team, one fleet) to buy a
  word we already own.
- The kinship is honest and useful: Herdr is the daemon; Herding is how you
  run a team on it. "A team console for Herdr" is a complete one-line story,
  and nothing in it implies the app *is* Herdr.
- V2's teams-first pivot (#32) changes what Herding does, not what it is
  called. If a future split (standalone product, app-store release) needs a
  clean-room name, that is a rename with a migration story — not a prerequisite
  for V2.

**Relationship to Herdr, in one line** (used in the desktop entry and README):
*Herding — a teams-first console for the Herdr daemon.*

**Sub-brands:** none for now. The team surface is the product, not a product
line; a "Herding Teams" prefix would imply a second, non-team Herding.

## Mark concept

Three chevrons in a flock — a V-formation pointing up and to the right,
converging on a goal: many independent agents, one direction, no shepherd's
crook and no cute animal. Geometry over illustration so it holds at 16 px
(task icons), 32 px (window), and 512 px (store/app icon).

- The **leading chevron** is the accent color; the two following chevrons are
  a muted tone of the same hue. In dark contexts the whole mark reads in one
  accent tone with reduced opacity on the trailing pair.
- The formation sits inside a rounded square (the app icon); the bare mark
  (no plate) is the favicon/avatar form.
- Files: `assets/brand/herding-mark.svg` (bare mark), `herding-icon-dark.svg`
  and `herding-icon-light.svg` (plate + mark). These are the concept
  deliverable; platform icon sets (`.icns`/`.ico`/hicolor PNGs) are generated
  from them in a follow-up slice, alongside the raster pipeline.

## Palette

A small token set, defined once and consumed by the GPUI theme layer. Values
are 24-bit RGB. Both modes keep the same hues; dark mode lightens surfaces and
keeps accents slightly desaturated so they do not vibrate against near-black.

| Token | Light | Dark | Role |
| --- | --- | --- | --- |
| `brand.accent` | `#C25E1E` | `#E8823C` | Primary actions, focus, the lead chevron, selection states |
| `brand.accent.muted` | `#E8C4A8` | `#8A4E22` | Trailing chevrons, secondary highlights, unfilled progress |
| `brand.surface.raised` | `#FFFFFF` | `#1C1F26` | Cards, the board, popovers |
| `brand.surface.base` | `#F5F2EE` | `#12151A` | App background (warm neutral, not pure gray) |
| `brand.status.running` | `#2F6FEB` | `#5B8DEF` | A task in progress; the pulse color |
| `brand.status.waiting` | `#B2730A` | `#D9A03F` | Assigned/unclaimed, held wakes, queued prompts |
| `brand.status.done` | `#1F7A43` | `#4CBB7F` | Completed tasks, acknowledged bulletins, green gates |
| `brand.status.blocked` | `#C03434` | `#E56868` | Blocked tasks, failed deliveries, permission prompts |

Status hues are colorblind-safe as a set (blue/amber/green/red differ in
luminance, not just hue) and every status color is paired with an icon mark
(#9), never color alone.

### Type

The product inherits the daemon-facing mono where it shows IDs and logs, but
brand voice lives in the UI face. Scale (px, at 1x):

| Step | Size | Weight | Used for |
| --- | --- | --- | --- |
| `display` | 22 | 600 | Team home greeting, empty states |
| `title` | 16 | 600 | Pane titles, section headers, window title |
| `body` | 13.5 | 400 | Bulletins, task briefs, board text |
| `caption` | 11.5 | 400 | Timestamps, status chip labels, hints |
| `mono` | 12 | 400 | IDs, SHAs, paths, CLI quotes |

Weights stop at 600 — no black/bold shouting; hierarchy comes from size and
color, in keeping with the calm-operational voice.

## Voice

Team-first, calm, operational. Rules of thumb:

- Speak about the **team's work**, not the terminal's: "workerbee1 claimed
  the deploy task", not "spawned process attached to pty 3".
- Declarative and specific over playful: statuses are facts ("Blocked — needs
  an approval click"), not moods.
- Verbs first in actions ("Claim task", "Post bulletin"); no jargon from the
  multiplexer layer (panes, ttys, sockets) on the team home — that detail
  belongs on machine/endpoint views where it is the point.
- Errors say what to do next: "m2 denied the SSH key — authorize fx505's
  pubkey or retry with the right user", not "publickey handshake failed".

## App identity

What the OS and the user see, versus what keys off identity internally:

| Property | Value | Notes |
| --- | --- | --- |
| Product name (user-visible) | **Herding** | Window title, desktop entry `Name`, menu bar |
| `WINDOW_TITLE` | `Herding` | `constants.rs`; the focused space follows it |
| `APP_ID` | `so.pen.herdr-gpui` → **`so.pen.herding`** (planned) | **Not changed in this slice**: the updater channel and the credential store key off it, so a change needs a data migration (re-key credentials, updater fallback). Tracked as the follow-up slice. |
| Bundle id (macOS) | `so.pen.herdr-gpui` → **`so.pen.herding`** (planned) | Same migration constraint as `APP_ID`. |
| Desktop entry | `scripts/release/so.pen.herdr-gpui.desktop` | File name tracks `APP_ID`; `Name=Herding`, `Comment` says "teams-first console for the Herdr daemon", `StartupWMClass` follows `APP_ID`. |
| `Icon=` | `herdr-gpui` | Keeps resolving until the hicolor icon set ships under a Herding name; then both the entry and the icon files move together. |
| Executable / `CFBundleExecutable` | `herdr-gpui` / `Herdr` | Kept: scripts, PATH lookups, and the updater reference the binary name; a rename is part of the APP_ID migration slice. |
| `--version` | `herdr-gpui v<calver> (<rev>)` | Kept in this slice: the version string doubles as the CLI identity agents parse in scripts. The rebrand of the CLI surface (binary name + version prefix) rides the APP_ID migration so it happens once, atomically. |
| `--build-info` | unchanged shape | Rev/version pairs, no product name inside. |
| Upstream attribution | intact | NOTICE/attribution for Herdr and bundled themes stays; Herding is a fork and says so in the desktop entry comment. |

## What is Herding's vs Herdr's

- **Herding's:** the team model (teams, board, bulletins, findings, wakes),
  the agent coordination UX, this brand, the app identity.
- **Herdr's:** the daemon, the control protocol, workspace/pane mechanics,
  the upstream client this fork is cut from.

## Slice status (this branch)

Implemented here: window title → "Herding"; desktop entry name/comment;
`Info.plist` display name; the palette token module (`brand.rs`) with the
eight colors above, light+dark, consumed by the team home header; the mark
SVGs under `assets/brand/`. Deferred with reasons: APP_ID/bundle-id/executable
rename (updater + keychain migration, follow-up slice), platform icon set
generation (needs a raster pipeline), full theme re-skin (token adoption is
per-surface).
