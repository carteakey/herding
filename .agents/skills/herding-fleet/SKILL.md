---
name: herding-fleet
description: Check the ZCode and Herding plugin state of every fleet host. Use when the user asks whether the fleet is healthy, whether zcode or the herdr-zcode plugin is installed everywhere, why a host's ZCode/Herdr integration is not working, or after a deploy or reinstall.
---

# Herding Fleet Doctor

`scripts/fleet-doctor.sh` reports, per host, the three things that make ZCode a
first-class Herdr agent. Run it instead of checking hosts by hand.

```sh
scripts/fleet-doctor.sh                 # HERDR_FLEET_HOSTS, else this machine
scripts/fleet-doctor.sh local fx505 m1 m2
scripts/fleet-doctor.sh --json local fx505
```

Exit status is 0 only when every host is healthy, so it can gate a deploy.

## What it checks

| Column | Healthy value | Meaning |
| --- | --- | --- |
| `ZCODE` | `fork X+herding.N` | The fork build, not upstream. Upstream has no Herdr integration. |
| `HERDR-ZCODE` | `0.1.1 skills=0 hooks=6` | The state plugin, with no bundled skill. |
| `HERDR-TEAM` | `agents` and/or `claude` | The team skill in `~/.agents/skills` and `~/.claude/skills`. |

`skills=0` on the plugin is the healthy value: `herdr-team` is a Herding skill
the app installs for every agent, so a copy inside the plugin is duplication
that drifts.

## Reading failures

- **`zcode: upstream X`** — the fork build was replaced by an upstream install.
  Reinstall the fork tarball; `--version` must end in `+herding.N`.
- **`zcode: not installed`** — nothing to attach the plugin to. Ask before
  installing it on a machine that never had it.
- **`plugin: not installed`** — add the marketplace and install:
  `zcode plugins marketplace add <zcode-cli>/integrations/herdr --yes` then
  `zcode plugins install herdr-zcode@herdr-zcode --yes`.
- **`plugin: X skills=1`** — an old combined plugin. Remove the marketplace and
  re-add it from `integrations/herdr`.
- **`skills: missing`** — the app has not run since the team skill existed, or
  `[agent_skill] install` is `off`. Start the current `herding` binary once; it
  places the skill itself (see `agent_skill.rs`).
- **`unreachable`** — the host is down or ssh is refused. Report it; do not
  improvise access.

## Rules

- Read-only. The doctor never installs, edits, or restarts anything.
- Never report a host as healthy from a partial row; `unreachable` is not
  healthy.
- After fixing a host, re-run the doctor and quote the new row as evidence.
- `local` means this machine; every other name is an ssh target.
