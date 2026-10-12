#!/usr/bin/env bash
# jstyping.sh <base-tsr> <cand-tsr>: diff jsTyping diagnostics against tsgo-pinned.
# new_false: a candidate line tsgo does not report whose location+code is new versus the base
# (a changed message on an already-false line is not new); lost_true: a tsgo line the base
# matched exactly and the candidate no longer prints.
cd /home/user/workspace/repo
J=vendor/typescript-go/_submodules/TypeScript/src/jsTyping/tsconfig.perf.json
run() { "$1" -p $J --noEmit --incremental false --composite false --pretty false 2>&1 | grep 'error TS' | sort -u; }
run "$1" > /tmp/integ/js-base.txt; run "$2" > /tmp/integ/js-cand.txt
sort -u /tmp/integ/jstyping-tsgo.txt | grep 'error TS' > /tmp/integ/js-tsgo.txt
key() { sed -E 's/^(.*\([0-9]+,[0-9]+\): error TS[0-9]+):.*/\1/' "$1" | sort -u; }
key /tmp/integ/js-base.txt > /tmp/integ/js-base.keys
comm -23 /tmp/integ/js-cand.txt /tmp/integ/js-tsgo.txt > /tmp/integ/js-cand-false.txt
newfalse=$(key /tmp/integ/js-cand-false.txt | comm -23 - /tmp/integ/js-base.keys | wc -l)
losttrue=$(comm -23 /tmp/integ/js-base.txt /tmp/integ/js-cand.txt | comm -12 - /tmp/integ/js-tsgo.txt | wc -l)
echo "JSTYPING base=$(wc -l < /tmp/integ/js-base.txt) cand=$(wc -l < /tmp/integ/js-cand.txt) tsgo=$(wc -l < /tmp/integ/js-tsgo.txt) new_false=$newfalse lost_true=$losttrue"
key /tmp/integ/js-cand-false.txt | comm -23 - /tmp/integ/js-base.keys | head -5
