# Build cache and disk use

`target/` is shared by every agent and `cargo` invocation in this checkout. A
debug build of `herdr-gpui` is large on its own, and the incremental cache
(`target/debug/incremental`) can reach tens of gigabytes. Treat it as a
disposable speed cache, never as source: removing it costs rebuild time only.

## Why it grows

- `herdr-gpui` is one bin target with thousands of source files and thousands
  of tests; GPUI's generics and the test harness make its dependency graph and
  object files large. Individual incremental dep graphs have been observed at
  roughly 0.7 GB each.
- rustc keeps several recent incremental sessions per crate configuration so
  edits can reuse work.
- Every configuration gets its own cache root: feature sets (`--all-features`,
  `--no-default-features`, `--features mockup`), target kinds (`--all-targets`,
  tests, clippy), and even a different source path or worktree. Changing one
  leaves the old roots behind.
- Several agents building different flags against the same `target/` multiply
  those roots. The debug profile keeps line tables, so artifacts stay large.

A 53 GB incremental cache (about two-thirds of an 81 GB `target/`) has been
seen here; it is a usage pattern, not a bug.

## Inspect before deleting

```sh
du -sh target target/debug target/debug/deps target/debug/incremental
du -sh target/debug/* | sort -h | tail
find target -type f -size +200M
```

## Levers, least disruptive first

1. **Drop old incremental sessions.**

   ```sh
   rm -rf target/debug/incremental
   ```

   This is safe: cargo recreates it. It only makes the next compile slower.
   Do it while no `cargo`/`rustc` process is running, or the build that owns a
   deleted session may fail and need to be rerun. Do not kill other agents'
   builds.

2. **Disable incremental compilation for batch or agent builds.**

   ```sh
   CARGO_INCREMENTAL=0 cargo test -p herdr-gpui --bin herdr-gpui
   CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features
   CARGO_INCREMENTAL=0 cargo build -p herdr-gpui
   ```

   Keep incremental on for short edit/test loops where it pays for itself.

3. **Stay on the shared feature set.** Routine checks should use the `just`
   recipes (`just ci`, `just format`, `just lint-*`) instead of ad-hoc mixes of
   features, targets, or target directories. Fewer configurations means fewer
   cache roots and more reuse.

4. **Reclaim only the app's artifacts.**

   ```sh
   cargo clean -p herdr-gpui
   ```

   This removes `herdr-gpui`'s artifacts, including `target/debug/herdr-gpui`.
   The next team CLI call or `just run` rebuilds them; warn anyone actively
   using the binary. It does not remove dependencies or other crates.

5. **Wipe the whole target.** `cargo clean` frees the most space but forces a
   full rebuild for everyone sharing this checkout. Do it only when the disk is
   genuinely full and coordinate first; it is not a routine cleanup.

## Do not

- Delete another worktree, another agent's files, or `target/` while builds are
  running.
- Add `incremental = false` to `Cargo.toml` to work around disk pressure; local
  edit loops depend on it. Disable it per command instead.
- Commit anything under `target/`.
