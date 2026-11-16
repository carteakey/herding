# Upstream Synchronization Strategy: Merge and Rebrand at the Edge

**Status**: Active Fork Policy
**Date**: 2026-11-10
**Merge Base**: `389a55138` (upstream `main`, the `sheprd` rename)
**Proof Merge**: `9a8893775` (merge of `389a55138` into the Herding fork)

---

## 1. Context

Upstream renamed the project from `herdr` to `sheprd` in PR #1021
(`3b9800ff7cff`): crates, binaries, package names, and website copy changed
across thousands of files. The fork's original policy rejected that merge and
cherry-picked individual fixes instead, to keep the `herdr` crate layout and
the Herding layer untouched.

That policy has been retired. Upstream now carries the terminal, daemon,
browser, review, chat, and platform work Herding depends on, and isolating
each change as a cherry-pick costs more than one merge per upstream release.
The fork therefore merges upstream `main` directly and accepts its internal
`sheprd-*` names.

## 2. Identity Boundary

Herding is a product identity layered on the upstream codebase, not a second
rename of it.

**Changed to Herding:**

- Product name, window title, About text, notifications, and user-facing docs.
- The application binary: `herding`.
- Release and development paths for that binary (`target/{debug,release}/herding`,
  `Herding.app`, `Contents/MacOS/herding`).
- CLI command examples: `herding <command>`, including the Team commands.
- `Source::Herding` in the app's configuration model.

**Kept from upstream:**

- Crate, package, and type names (`sheprd-gpui`, `SheprdWindow`,
  `Owner::Sheprd`), so future merges remain mechanical.
- Companion binaries and helper paths (`sheprd-acp`, `sheprd-automate`,
  `sheprd-remote-control`, `sheprd-nearby`, `sheprd-launcher.exe`,
  `~/.local/state/herdr/gpui/bin/herdr-gpui`).
- Stable state, config, and protocol identifiers: `~/.local/state/herdr`,
  `~/.config/herdr`, `HERDR_*` environment variables, socket names, the
  `sheprd://` URL scheme, skill names, and macOS app and bundle IDs.
- Upstream release packaging, installers, website, and the iOS companion. Those
  still say Sheprd/Herdr UI and build the `sheprd` executable, and are a
  separate follow-up; they are not part of the application identity change.

The upstream workflows gate every code job on `penso/herdr-gpui` or
`sheprd-labs/sheprd`, so they do not run on this fork. The fork ships with
`scripts/ship.sh`, which builds and deploys `herding`. Rebranding the
packaging therefore means updating the executable paths in
`.github/workflows/{ci,release}.yml` and `scripts/release/*` (the app binary
becomes `herding`, the `sheprd-*` companions keep their names) together with
the bundle, installer, tap, and winget identities. Until then those scripts
describe upstream's artifacts, not this fork's.

## 3. Sync Procedure

1. Fetch `upstream/main` and inspect the range since the last sync.
2. Create a dedicated branch and merge upstream into it, never rebase the
   fork's history onto upstream.
3. Resolve conflicts by responsibility:
   - Keep upstream refactors and new modules wholesale.
   - Reapply Herding-only additions on top: the Team view, control commands,
     orchestrator records, localization keys, and docs.
   - Preserve the identity boundary in section 2. Do not rename internal
     `sheprd-*` items while resolving conflicts.
4. Run the workspace gates (`just format`, `just ci`, plus the platform lints
   and file-size, i18n, and sorted checks the change touches).
5. Run focused Team and CLI tests, then merge the sync branch into `main`
   through the normal pull request flow.

## 4. Conflict Hotspots

- `crates/sheprd-gpui/src/app.rs`, `cli.rs`, and `control/client/help.rs`:
  upstream adds commands and flags continuously; Herding adds the Team
  commands and identity strings. Keep upstream's structure and append the Team
  rows in their own match arms and help sections.
- `crates/sheprd-gpui/locales/en-US/`: add Herding strings as new keys in the
  owning catalog rather than rewriting upstream keys; the `settings-source-*`
  keys stay upstream-owned.
- `crates/sheprd-orchestrator/`: Herding's Team records sit beside upstream's
  issue and pull request records. Keep the two schemas separate.
- `docs/cli.md`: rebuild from upstream and re-add the Team section, rather than
  resolving the file line by line.
- `Cargo.lock`: regenerate with `cargo metadata` after resolving
  `Cargo.toml`; do not hand-merge.

## 5. Why Not Cherry-Pick?

Cherry-picking fixed individual commits no longer isolates the fork from the
rename: every subsequent upstream change already assumes the `sheprd` layout,
so each pick needs the same path and symbol adaptation. A merge resolves that
adaptation once per sync and keeps the fork's history connected to upstream,
which is what makes the next sync tractable.
