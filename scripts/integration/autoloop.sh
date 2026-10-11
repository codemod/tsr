#!/usr/bin/env bash
# autoloop.sh: continuous integration loop for round 7 (docs/parity/round7.md).
# Each iteration: pick lane branches with new, cleanly-merging commits; run
# gate.sh; if every leg passes, refresh snapshots, append the batch record and
# push to main. Any failed leg stops the loop with /tmp/integ/AUTOLOOP_BLOCKED
# naming the batch; the integration owner resolves it and deletes the file.
set -uo pipefail
cd /home/user/workspace/repo
state=/tmp/integ/autoloop.state   # "<batch-number> <freeze-dir> <oracle-dir> <coverage-file>"
log=/tmp/integ/autoloop.log
lanes="calls contextual declared flow grammar parser perf printer reports shared"
note() { echo "$(date -u +%FT%TZ) $*" >> $log; }
while true; do
  if [ -f /tmp/integ/AUTOLOOP_BLOCKED ]; then sleep 120; continue; fi
  read -r n freeze oracle cov < $state
  git fetch -q origin
  sel=""
  for l in $lanes; do
    b=box/r7-$l
    git rev-parse -q --verify origin/$b >/dev/null || continue
    [ "$(git cherry origin/main origin/$b | grep -c '^+')" -gt 0 ] || continue
    git merge-tree --write-tree origin/main origin/$b >/dev/null 2>&1 && sel="$sel $b"
  done
  if [ -z "$sel" ]; then note "idle"; sleep 600; continue; fi
  m=$((n+1)); name=b$m
  note "batch $m:$sel"
  ORACLE_BASE=$oracle /tmp/integ/gate.sh $freeze $name $sel > /dev/null 2>&1
  g=/tmp/integ/$name/gate.log
  if grep -q '^FMT_EXIT 1' $g; then cargo fmt --all && git commit -qam "fmt: batch $m

Amp-Thread-ID: https://ampcode.com/threads/T-01a12477-cad4-767b-9d14-9bb9aa372e7c" && note "fmt fixed"; fi
  fail=""
  grep -q '^COMPARE_EXIT 0' $g || fail="$fail compare"
  grep -q '^CLIPPY_EXIT 0' $g || fail="$fail clippy"
  grep -q '^TEST_EXIT 0' $g || fail="$fail test"
  grep -q 'new_false=0 lost_true=0' $g || fail="$fail jstyping"
  grep -q '^ORACLE_EXIT 0' $g || fail="$fail oracle"
  grep -q '^CONFLICT b' $g && fail="$fail conflict"
  for r in $(grep '^IR ' $g | sed 's/.*ratio=//'); do
    python3 -c "import sys; sys.exit(0 if float('$r') <= 1.005 else 1)" || fail="$fail ir:$r"
  done
  if [ -n "$fail" ]; then echo "$name:$fail" > /tmp/integ/AUTOLOOP_BLOCKED; note "BLOCKED $name:$fail"; continue; fi
  git fetch -q origin && git merge -q --no-edit origin/main || { echo "$name: main merge" > /tmp/integ/AUTOLOOP_BLOCKED; continue; }
  cargo run -q --release -p tsr-conformance --bin coverage > /tmp/integ/$name/coverage.txt 2>&1
  flags=$(/tmp/integ/covdiff.sh $cov /tmp/integ/$name/coverage.txt | grep -v '^COVDIFF' | grep -v 'fail \([0-9]*\)->\1$' )
  if [ -n "$flags" ]; then echo "$name: covdiff $flags" > /tmp/integ/AUTOLOOP_BLOCKED; note "BLOCKED $name covdiff"; continue; fi
  python3 /tmp/integ/record.py $m $cov /tmp/integ/$name >> docs/parity/round7.md
  git add -A crates/tsr-conformance/snapshots docs/parity/round7.md
  git commit -q -m "parity: batch $m ($(echo $sel | sed 's#box/r7-##g')) — snapshots and record (tsr-2zk.47)

Amp-Thread-ID: https://ampcode.com/threads/T-01a12477-cad4-767b-9d14-9bb9aa372e7c"
  ok=""
  for t in 1 2 3; do
    git push -q origin HEAD:main 2>/dev/null && { ok=1; break; }
    git fetch -q origin && git merge -q --no-edit origin/main
  done
  [ -n "$ok" ] || { echo "$name: push" > /tmp/integ/AUTOLOOP_BLOCKED; continue; }
  rm -rf /tmp/integ/oracle/main-$name; cp -r /tmp/integ/$name/oracle /tmp/integ/oracle/main-$name
  echo "$m /tmp/integ/$name/freeze /tmp/integ/oracle/main-$name /tmp/integ/$name/coverage.txt" > $state
  note "landed $name $(git rev-parse --short HEAD)"
done
