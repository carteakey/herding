# Herding team checkpoint — 2026-11-05 (end of day)

Full durable record: GitHub issue #30 comment; team bulletin seq 54.
Team: 6aa93cf4-691f-475d-a7e5-e7d793210e5d. Supervisor: ds41 (681b0ff8), pane w8:p8.

## Landed (origin/main)
- 57042af6 resume re-bind (same pane)
- 1d298bc5 task timestamps + drop phantom --role (#10)
- f9668424 / d506c732 / 25a2d7d1 wake pipeline (#13)
- fb174e0e task icons (#9)
- 9c19abfa team wakes bypass the notes queue (#17)

## Local only (verify + push tomorrow)
- efd8d988 lineage wrapper-shell test arm (local main, rebased onto 9c19abfa).
  Verify: `cargo test -p herdr-gpui team::tests::lineage` with `-j1`.
  Closes #3, un-gates 93470ea1.

## In flight
- worker1: feat/memory-instrumentation 463ac2ee (#12) — gates running with CARGO_BUILD_JOBS=1.
- worker2: bfa79512 (#11) — WIP is stash@{0} "boot-drift wip" on
  fix/team-boot-drift-rebind in /home/kchauhan/repos/forks/herding-icons. Do not drop.

## OOM mitigation (re-apply after a unit restart)
```
systemctl --user set-property app-herding@<unit> MemoryHigh=2500M MemoryMax=3G
```
Current unit: app-herding@bfabc6d765924a7982f34b666af42dd4.service

## Boot-drift repair (until #11 lands)
```
sqlite3 ~/.local/state/herdr/gpui/team/team.sqlite3 "UPDATE members SET host='<current boot_id>';"
```
Probe the boot id with a throwaway `team register` and read `members`. Backups: team-backup-*.sqlite3.

## Next session order
1. Land 463ac2ee after gates.
2. Verify/push efd8d988; close #3; un-gate 93470ea1.
3. worker2 finishes #11 from the stash, then #6.
4. New backlog first wave: #14, #26, #22, #23, #25 (tracker #30).
