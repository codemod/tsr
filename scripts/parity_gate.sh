#!/usr/bin/env bash
# Legacy-suite transition gate for parity commits (docs/parity/box-protocol.md §5).
#
#   scripts/parity_gate.sh freeze <dir>          build release, dump verdicts of the current tree
#   scripts/parity_gate.sh compare <before> <after>
#   scripts/parity_gate.sh oracle-native <native-dir> [--workers N] [--deadline S] [--filter SUBSTRING]
#   scripts/parity_gate.sh oracle-tsr <native-dir> <report-dir> [--workers N] [--deadline S] [--filter S] [--per-process]
#   scripts/parity_gate.sh oracle-compare <base-report-dir> <candidate-report-dir>
#
# `compare` fails when any previously RIGHT type assertion or RIGHT/EMPTY_RIGHT
# diagnostic case changes verdict OR disappears from the after dump: a vanished
# key is a loss, never a silent join drop (tsr-2zk.47.2). It measures only the
# legacy expected-driven suites; it does not certify exact full-corpus parity.
# `oracle-native` freezes the pinned native artifacts once; `oracle-tsr` runs the
# committed tree's TSR against a frozen native run of the same identity, and
# `oracle-compare` lists exact gains, losses and missing cases and fails when a
# case exact in the base report is not exact, or is absent, in the candidate
# (docs/parity/notes/oracle.md).
set -euo pipefail

cmd=${1:-}
case "$cmd" in
freeze)
  out=${2:?usage: parity_gate.sh freeze <dir>}
  mkdir -p "$out"
  root=$(git rev-parse --show-toplevel)
  cd "$root"
  target=${CARGO_TARGET_DIR:-$root/target}
  cargo build -q --release -p tsr-conformance -p tsr --examples --bins
  {
    echo "source $(git rev-parse HEAD)"
    echo "dirty $(git status --porcelain --untracked-files=no -- crates xtask Cargo.toml Cargo.lock | shasum -a 256 | cut -c1-16)"
    echo "corpus $(git -C vendor/typescript-go rev-parse HEAD)"
    shasum -a 256 "$target/release/tsr" "$target/release/examples/verdictdump" "$target/release/examples/diagverdictdump"
  } > "$out/identity.txt"
  cp -f "$target/release/tsr" "$out/tsr"
  "$target/release/examples/diagverdictdump" > "$out/diag.tsv" 2> "$out/diag.err"
  "$target/release/examples/verdictdump" > "$out/types.tsv" 2> "$out/types.err"
  tail -n 3 "$out/types.err"
  echo "frozen $out"
  ;;
compare)
  b=${2:?usage: parity_gate.sh compare <before> <after>}
  a=${3:?usage: parity_gate.sh compare <before> <after>}
  status=0
  awk -F'\t' '
    FNR == NR { if ($2 == "RIGHT") before[$1]++; next }
    { if ($2 == "RIGHT") after[$1]++; seen[$1]++ }
    END {
      for (k in before) {
        if (!(k in seen)) { missing++; print "TYPE_MISSING\t" k }
        else if (after[k] < before[k]) { lost += before[k] - after[k]; print "TYPE_LOSS\t" k }
      }
      for (k in after) if (after[k] > before[k]) gained += after[k] - before[k]
      printf "types: right_gained=%d right_lost=%d right_missing=%d\n", gained, lost, missing > "/dev/stderr"
      exit (lost + missing) > 0
    }' "$b/types.tsv" "$a/types.tsv" || status=1
  awk -F'\t' '
    function ok(v) { return v == "RIGHT" || v == "EMPTY_RIGHT" }
    FNR == NR { if (ok($2)) before[$1] = $2; was[$1] = $2; next }
    { now[$1] = $2 }
    END {
      for (k in before) {
        if (!(k in now)) { missing++; print "DIAG_MISSING\t" k }
        else if (now[k] != before[k]) { lost++; print "DIAG_LOSS\t" k "\t" before[k] "->" now[k] }
      }
      for (k in now) if (ok(now[k]) && !(k in before)) gained++
      printf "diagnostics: cases_gained=%d cases_lost=%d cases_missing=%d\n", gained, lost, missing > "/dev/stderr"
      exit (lost + missing) > 0
    }' "$b/diag.tsv" "$a/diag.tsv" || status=1
  exit $status
  ;;
oracle-native|oracle-tsr)
  [ $# -ge $([ "$cmd" = oracle-native ] && echo 2 || echo 3) ] \
    || { echo "usage: parity_gate.sh oracle-native <native-dir> | oracle-tsr <native-dir> <report-dir> [args]" >&2; exit 2; }
  shift
  root=$(git rev-parse --show-toplevel)
  target=${CARGO_TARGET_DIR:-$root/target}
  (cd "$root" && cargo build -q --release -p tsr-conformance --example full_oracle_run --example full_oracle_actual)
  "$target/release/examples/full_oracle_run" "${cmd#oracle-}" "$@"
  ;;
oracle-compare)
  b=${2:?usage: parity_gate.sh oracle-compare <base> <candidate>}
  a=${3:?usage: parity_gate.sh oracle-compare <base> <candidate>}
  root=$(git rev-parse --show-toplevel)
  target=${CARGO_TARGET_DIR:-$root/target}
  (cd "$root" && cargo build -q --release -p tsr-conformance --example full_oracle_run)
  "$target/release/examples/full_oracle_run" gate "$b" "$a"
  ;;
*)
  sed -n '2,18p' "$0"
  exit 2
  ;;
esac
