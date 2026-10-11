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
