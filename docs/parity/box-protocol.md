# Parity box protocol

How a parallel box works on one lane of epic `tsr-2zk`, and the gates its
commits must pass before the integrator merges them. The lane brief names the
issue, the root-cause hypotheses, the tsgo source to mirror, and the files the
box owns. Everything else is here, so every box is judged the same way.

## 1. Setup (fresh cloud container)

```bash
scripts/offline-cargo/bootstrap.sh                 # crates.io is blocked; see its README
git submodule update --init --recursive --depth 1
export RUSTUP_TOOLCHAIN=stable                     # pinned 1.96.0 cannot be installed
cargo build --release -p tsr-conformance -p tsr
```

`bd` cannot be installed in these containers (its module proxy and release
hosts are blocked). Reference the lane's issue id in every commit message; the
integrator records status in Beads.

## 2. Freeze the baseline before editing

```bash
B=/tmp/box/base; mkdir -p $B
cargo run -q --release -p tsr-conformance --example diagverdictdump > $B/diag.tsv
cargo run -q --release -p tsr-conformance --example verdictdump     > $B/types.tsv
cp target/release/tsr $B/tsr                        # perf reference binary
```

`verdictdump` accepts `TSR_FILTER=caseA,caseB` for a fast inner loop on the
lane's cases. The scoring run before a commit is always unfiltered.

## 3. Port, do not patch

- Mirror the pinned tsgo function (`vendor/typescript-go` @ `5b1047d`): same
  algorithm, diagnostic code, message, span, ordering and type printing.
  Name the counterpart in a doc comment (`docs/conventions.md`, anchors).
- Never special-case a test, add a heuristic, or suppress output to make a
  baseline match. A fix that only matches because of a coincidence is wrong.
- A new cache, side table, mapper, member image or traversal follows the
  checker port convention in `docs/conventions.md`: record the pinned native
  operation, key identity and owner, publication states, receiver/alias
  context, and where the expensive work happens.
- Judgment calls are documented in `docs/` in the same commit: the subsystem's
  `docs/architecture/*.md`, or the lane's own `docs/parity/notes/<lane>.md`
  (create it; no other box writes it).

## 3a. What the integrator rejects

Measured on round-1 reviews (2026-10-05); each was reverted, not merged:

- **Guessing whether a TSR `any` is upstream's `any`** from declaration syntax
  (calls `48ce04a1`). Upstream asks `IsTypeAny(t)`. Where TSR answers `any`
  for "could not compute", that producer is the bug (`tsr-2zk.31`): fix it
  in an owned file or report it.
- **A side pass re-deriving a decision upstream makes inside another
  algorithm** (misc `5a517c22`: call-site TS2344 outside `chooseOverload`).
  If the faithful home is another lane's file, report it; the integrator
  routes it.

Narrowing or removing an existing decline toward upstream is welcome; adding
a new one needs the upstream reason it mirrors, or a filed issue naming the
missing upstream piece it waits for.

## 4. Ownership

Edit only the files the brief lists as owned. Hub files (`check.rs`,
`expressions.rs`, `checker.rs`, `lib.rs`) may be edited only inside functions
specific to the lane, or by adding functions. A needed change to a shared
helper or another lane's file is **not made**: describe it in the final report
(function, file, why, which cases it unlocks) and the integrator serializes it.

Never commit: `crates/tsr-conformance/snapshots/*` (the integrator regenerates
them after each merge), `STATUS.md`, `docs/parity/README.md`, `docs/parity/lanes/*`,
`.beads/*`, `Cargo.lock`, `rust-toolchain.toml`.

## 5. Gates for every pushed commit

1. **Target cases.** The lane's cases converted are listed in the commit message.
2. **Zero losses.** Re-run both dumps unfiltered into `/tmp/box/after` and
   compare with the baseline:
   ```bash
   # diagnostics: no case that was RIGHT or EMPTY_RIGHT may change verdict
   join -t$'\t' <(cut -f1,2 $B/diag.tsv|sort) <(cut -f1,2 /tmp/box/after/diag.tsv|sort) \
     | awk -F'\t' '($2=="RIGHT"||$2=="EMPTY_RIGHT") && $3!=$2'
   # types: no aligned line that was RIGHT may become non-RIGHT
   join -t$'\t' <(cut -f1,2 $B/types.tsv|sort) <(cut -f1,2 /tmp/box/after/types.tsv|sort) \
     | awk -F'\t' '$2=="RIGHT" && $3!="RIGHT"'
   ```
   Both must print nothing. A loss is fixed, never accepted.
3. **Full parity run.** `cargo run --release -p tsr-conformance --bin coverage`
   completes; report the `checker_types` and `diagnostics` rows (then
   `git checkout crates/tsr-conformance/snapshots`).
4. **Tests and lint.** `cargo test --workspace --release` passes;
   `cargo clippy --workspace --all-targets -- -D warnings` reports nothing in
   code you touched (stable may flag pre-existing code; report, don't fix);
   `cargo fmt --all` leaves no diff.
5. **Performance.** TSR against the frozen baseline binary on the public
   projects, using the repository harness with the old binary in the `--tsgo`
   slot (a self-comparison; the ratio is new/old):
   ```bash
   cargo build --release -p tsr
   for p in domain-model generic-imports; do
     python3 scripts/whole_project_perf.py --project benches/projects/$p/tsconfig.json \
       --tsgo $B/tsr --samples 9 --output /tmp/box/perf-$p.json
   done
   ```
   Judge by **median child CPU time** (`user_seconds + system_seconds` of
   `tools.<name>.samples` in the JSON), not `observed_wall_ratio`: the bench
   runs last ~0.1–0.4 s and per-pair wall ratios on a 4-core box range
   0.71–1.75 between identical binaries (integrator measurement,
   2026-10-05), while median CPU ratios of identical binaries stay within
   about ±2%. Use `--samples 21`; above 1.03 on either project, re-run with
   41; still above 1.03 is a regression and is fixed before pushing. A
   hot-path change also needs a measured reason it is not slower.
   Diagnostics must match the baseline binary's (`diagnostics_match: true`)
   unless the change is the port.
6. **Commit** one root cause per commit, message `<lane>: <what was ported> (<issue id>)`,
   naming the tsgo function and the cases converted. Push to the branch the
   session was created with (`git push -u origin HEAD`).

## 6. Rhythm and the final report

Work the lane's clusters in order of cases unlocked. Push after each verified
commit so the integrator can merge early. Stop after roughly three to four
hours of work, or when the lane's remaining clusters are blocked on files you
do not own. The final message is the report the integrator reads:

- commits pushed (hash, cases converted, issue id);
- before/after `checker_types` and `diagnostics` counts, both loss checks
  empty, perf ratios per project;
- remaining clusters in the lane with root-cause hypothesis and case counts;
- every needed change outside owned files (function, file, reason, cases).
