# Upstream Synchronization Strategy: Cherry-Picking vs. Rebrand Churn

**Status**: Active Fork Policy  
**Date**: 2026-11-04  
**Base Commit**: `b93958f5` (Pre-rebrand upstream fork point)  
**Historical Milestone**: `v0-herding` (Tag `v0`)

---

## 1. Context & Architectural Decision

Upstream landed PR #1021 (`3b9800ff7cff`), executing an exhaustive repo-wide renaming from `herdr` to `sheprd` across 6,051 files. It renamed all crates (`crates/herdr-gpui` -> `crates/sheprd-gpui`, `herdr-orchestrator` -> `sheprd-orchestrator`), binaries, config directories (`~/.config/herdr` -> `~/.config/sheprd`), environment variables (`HERDR_*`), and website assets.

**Decision**:
We reject a blind git merge of upstream `main`. Merging PR #1021 directly introduces massive merge conflicts, breaks existing daemon socket paths and agent skills across our homelab fleet (KPC, FX505, M1 Mac), and offers zero functional or performance improvement to the core workbench engine.

Instead, our repository policy follows the **Strata / Ada Headroom Policy**:
1. Maintain our stable fork foundation on the canonical `herdr` architecture.
2. Isolate functional improvements from upstream via selective cherry-picks.
3. Merge `v0-herding` cleanly into `main` after cherry-picking functional fixes.

---

## 2. Upstream Audit: Commits Landed Since PR #1021

An audit of upstream commits between `3b9800ff` and `d8db7e83` reveals that 95% of commits were website and marketing copy. Only 8 commits contain actual bugfixes or functional improvements:

### A. Candidate Commits for Cherry-Picking

#### 1. Remote SSH Agent Discovery on macOS
- **Commit**: `56f6bad9` (`fix(chat): show the agent view for chats on a Mac reached over SSH`)
- **Impact**: Resolves `herdr-acp` inside `~/Applications` and `/Applications` app bundles when reaching an M1/M2 Mac over SSH. Fixes exit 127 when starting new chats or listing remote agents.
- **Verdict**: **Recommended for cherry-pick**. Directly benefits our cross-host M1 Mac agent fleet.

#### 2. Linux Packaging CI Checks
- **Commit**: `ee3d3f63` (`ci: install diffutils for Fedora package smoke checks`)
- **Commit**: `3c12204b` (`ci: install diffutils for Arch package smoke checks`)
- **Impact**: Installs `diffutils` (`cmp`) in minimal container smoke checks before testing binary aliases.
- **Verdict**: **Cherry-pick if building native packages**.

#### 3. iOS Remote Client UX & Bluetooth Stability
- **Commit**: `2b55f1ac` (`fix(ios): keep nearby pairing progress when an iPad sheet closes`)
- **Commit**: `021c3140` (`fix(ios): keep the pairing card reachable on iPad and stop idle Bluetooth scans`)
- **Commit**: `c960bd55` (`feat(ios): offer nearby computers in a banner and keep pairing in Settings`)
- **Commit**: `c193dbf0` (`feat(ios): collapse computers and scroll workspaces with a section index`)
- **Commit**: `9f473930` (`fix(ios): close the connection toast with a tap anywhere on it`)
- **Impact**: Significant stability and UX improvements for iPad/iPhone pairing and Bluetooth battery drain.
- **Verdict**: **Cherry-pick as an isolated iOS batch** when testing the iOS remote companion.

### B. Rejected Commits (Marketing & Rebrand Churn)
- Website landing pages and mockups (`6d545a58`, `a00410ae`, `5829a753`, `83f1ddd5`, `2efa54a9`, `d3aa69be`, `8e5c027c`, `bf22ed94`, `f47cb88c`, `80f995f1`, `2139f2b2`).
- Cloudflare worker sources for `herdrui.dev` redirects.

---

## 3. Execution Plan: Cherry-Pick & Merge to Main

1. **Step 1: Reset `main` to `b93958f5` (Fork Point)**
   Reset `origin/main` back to the pre-rebrand fork base `b93958f5`.

2. **Step 2: Cherry-Pick Functional Fixes**
   Cherry-pick commit `56f6bad9` (Mac SSH agent discovery) onto `main` (adapting path names to `herdr-gpui` if necessary).

3. **Step 3: Merge `v0-herding` to `main`**
   Merge the complete, verified Herding multi-agent engine (`v0-herding`, commit `5f4bd41a`) cleanly into `main`.

4. **Step 4: Tag & Deploy**
   Push `origin/main` as the authoritative, stable release of Herding.
