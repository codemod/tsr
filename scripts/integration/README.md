# Integration gate scripts (round 7)

The integration owner's batch gate (`docs/parity/round7.md`). The scripts keep
their working state under `/tmp/integ` and expect the repository at
`/home/user/workspace/repo`, the orb layout. Adjust both paths before using them
elsewhere.

| script | does |
|---|---|
| `gate.sh <base-freeze> <name> <branch>...` | merge the lane branches onto `origin/main` as `integ/<name>` (lane-note conflicts take the lane's copy); `parity_gate.sh freeze` + `compare`; fmt, strict clippy, workspace tests; 21-sample child CPU; jsTyping; callgrind Ir |
| `jstyping.sh <base-tsr> <cand-tsr>` | jsTyping's unique error lines against `target/tsgo-pinned`; refuse on `new_false` or `lost_true` > 0. Needs the untracked `src/jsTyping/tsconfig.perf.json` in the TypeScript submodule (see the script) |
| `ir.sh <base-tsr> <cand-tsr>` | callgrind instruction counts on generic-imports and domain-model, `--singleThreaded`; the deterministic hot-path guard (batch 13) |
| `covdiff.sh <prev> <curr>` | per-suite coverage diff; flags any suite whose passes drop or failures rise |
| `heads.sh` | lane branches with commits not on main, and whether each merges cleanly |
| `autoloop.sh` | the continuous loop: pick lanes with new clean commits, `gate.sh`, and on an all-green gate refresh snapshots, append the record (`record.py`) and push to main; any failed leg writes `/tmp/integ/AUTOLOOP_BLOCKED` and the loop waits for the integration owner. State: `/tmp/integ/autoloop.state` = `<batch> <freeze-dir> <oracle-report> <coverage-file>`. Run it as an orb service (`amp orb service start autoloop --command /tmp/integ/autoloop.sh`) |
| `autoloop.exclude` | lane names (one per line) the loop skips while a refused commit is being fixed |
| `record.py` | the batch record for `docs/parity/round7.md` from a gate log, the oracle report and both coverage runs |

The gate's legs and thresholds: zero previously RIGHT legacy keys lost or missing; zero EXACT
oracle cases lost or missing; jsTyping `new_false` and `lost_true` both 0; strict clippy, tests
and fmt green; callgrind Ir at most 1.005 on generic-imports and domain-model; no suite's
failure count rising.
