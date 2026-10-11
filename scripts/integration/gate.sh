#!/usr/bin/env bash
# Integration gate: gate.sh <base-freeze-dir> <name> branch1 [branch2 ...]
# Builds candidate integ/<name> = main + merged branches, freezes it, compares
# against the base freeze, runs workspace tests, clippy, fmt and perf.
set -uo pipefail
base=${1:?base}; name=${2:?name}; shift 2
cd /home/user/workspace/repo
out=/tmp/integ/$name; mkdir -p $out
log=$out/gate.log; : > $log
say() { echo "$*" | tee -a $log; }
git fetch -q origin
git checkout -q -B integ/$name origin/main
for b in "$@"; do
  if git merge -q --no-edit "origin/$b" >>$log 2>&1; then say "MERGED $b $(git rev-parse --short origin/$b)"
  else
    conf=$(git diff --name-only --diff-filter=U)
    if [ -n "$conf" ] && ! echo "$conf" | grep -qv '^docs/parity/notes/'; then
      for f in $conf; do git checkout --theirs -- "$f"; git add "$f"; done
      git commit -q --no-edit && say "MERGED(notes-theirs) $b $(git rev-parse --short origin/$b)"
    else say "CONFLICT $b: $(echo $conf)"; git merge --abort; fi
  fi
done
say "HEAD $(git rev-parse --short HEAD)"
scripts/parity_gate.sh freeze $out/freeze >>$log 2>&1 || say "FREEZE_FAIL"
scripts/parity_gate.sh compare $base $out/freeze > $out/compare.txt 2> $out/compare.err; say "COMPARE_EXIT $?"
cat $out/compare.err | tee -a $log
head -40 $out/compare.txt | tee -a $log
cargo fmt --all -- --check >/dev/null 2>&1; say "FMT_EXIT $?"
cargo clippy -q --workspace --all-targets -- -D warnings > $out/clippy.txt 2>&1; say "CLIPPY_EXIT $?"; grep -E "^error" -A3 $out/clippy.txt | grep -E "^error|-->" | head -10 | tee -a $log
cargo test -q --workspace --release > $out/test.txt 2>&1; say "TEST_EXIT $?"
grep -E '^test result|FAILED|panicked' $out/test.txt | grep -v ' 0 failed' | head -20 | tee -a $log
for p in domain-model generic-imports; do
  python3 scripts/whole_project_perf.py --project benches/projects/$p/tsconfig.json \
    --tsr target/release/tsr --tsgo $base/tsr --samples 21 --output $out/perf-$p.json >/dev/null 2>&1
  python3 - "$out/perf-$p.json" "$p" <<'EOF' | tee -a $log
import json,sys,statistics as st
d=json.load(open(sys.argv[1]))
t=d['tools']
def cpu(n): return st.median(s['user_seconds']+s['system_seconds'] for s in t[n]['samples'])
def wall(n): return st.median(s.get('wall_seconds',0) for s in t[n]['samples'])
names=list(t.keys())
print(sys.argv[2], 'cpu new/old', round(cpu('tsr')/cpu('tsgo'),4), 'wall', round(wall('tsr')/max(wall('tsgo'),1e-9),4), 'diag_match', d.get('diagnostics_match'))
EOF
done
/tmp/integ/jstyping.sh $base/tsr target/release/tsr | tee -a $log
/tmp/integ/ir.sh $base/tsr target/release/tsr | tee -a $log
say "GATE_DONE"
