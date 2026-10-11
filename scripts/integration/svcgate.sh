#!/usr/bin/env bash
# svcgate.sh <base-freeze> <name> <branch>... : run gate.sh as a supervised orb service (survives session ends)
base=$1; name=$2; shift 2
amp orb service stop gate-$name >/dev/null 2>&1
amp orb service start gate-$name --command "bash -c '${ORACLE_BASE:+ORACLE_BASE=$ORACLE_BASE }/tmp/integ/gate.sh $base $name $* > /dev/null 2>&1; sleep infinity'"
