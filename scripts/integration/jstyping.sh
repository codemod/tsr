#!/usr/bin/env bash
# jstyping.sh <base-tsr> <cand-tsr>: diff jsTyping diagnostics against tsgo-pinned.
cd /home/user/workspace/repo
J=vendor/typescript-go/_submodules/TypeScript/src/jsTyping/tsconfig.perf.json
run() { "$1" -p $J --noEmit --incremental false --composite false --pretty false 2>&1 | grep 'error TS' | sort -u; }
run "$1" > /tmp/integ/js-base.txt; run "$2" > /tmp/integ/js-cand.txt
sort -u /tmp/integ/jstyping-tsgo.txt | grep 'error TS' > /tmp/integ/js-tsgo.txt
newfalse=$(comm -13 /tmp/integ/js-base.txt /tmp/integ/js-cand.txt | comm -23 - /tmp/integ/js-tsgo.txt | wc -l)
losttrue=$(comm -23 /tmp/integ/js-base.txt /tmp/integ/js-cand.txt | comm -12 - /tmp/integ/js-tsgo.txt | wc -l)
echo "JSTYPING base=$(wc -l < /tmp/integ/js-base.txt) cand=$(wc -l < /tmp/integ/js-cand.txt) tsgo=$(wc -l < /tmp/integ/js-tsgo.txt) new_false=$newfalse lost_true=$losttrue"
comm -13 /tmp/integ/js-base.txt /tmp/integ/js-cand.txt | comm -23 - /tmp/integ/js-tsgo.txt | head -5
