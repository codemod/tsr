#!/usr/bin/env bash
# ir.sh <base-tsr> <cand-tsr>: callgrind Ir ratio on gi and dm, single-threaded.
cd /home/user/workspace/repo
for p in generic-imports domain-model; do
  for side in base cand; do
    bin=$1; [ $side = cand ] && bin=$2
    valgrind --tool=callgrind --callgrind-out-file=/tmp/integ/cg-$side.out "$bin" -p benches/projects/$p/tsconfig.json --noEmit --singleThreaded true --pretty false >/dev/null 2>&1
  done
  b=$(grep -E '^summary' /tmp/integ/cg-base.out | awk '{print $2}'); c=$(grep -E '^summary' /tmp/integ/cg-cand.out | awk '{print $2}')
  python3 -c "print('IR $p base=$b cand=$c ratio=%.4f' % ($c/$b))"
done
