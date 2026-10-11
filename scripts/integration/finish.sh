#!/usr/bin/env bash
# finish.sh <name>: merge origin/main into integ/<name>, run coverage, write /tmp/integ/<name>/coverage.txt
cd /home/user/workspace/repo
git checkout -q integ/$1 && git fetch -q origin && git merge -q --no-edit origin/main
cargo run -q --release -p tsr-conformance --bin coverage > /tmp/integ/$1/coverage.tmp 2>&1 && mv /tmp/integ/$1/coverage.tmp /tmp/integ/$1/coverage.txt
echo FINISH_DONE > /tmp/integ/$1/finish.done
