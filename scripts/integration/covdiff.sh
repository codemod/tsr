#!/usr/bin/env bash
# covdiff.sh <prev-coverage.txt> <curr-coverage.txt>: per-suite failures (judged-passed); flags increases.
python3 - "$1" "$2" <<'PY'
import re,sys
def load(p):
    d={}
    for l in open(p):
        m=re.match(r'^([a-z_]+)\s+(\d+)/(\d+)',l)
        if m: d[m[1]]=(int(m[2]),int(m[3]))
    return d
a,b=load(sys.argv[1]),load(sys.argv[2])
bad=0
for k in b:
    if k in a:
        fa=a[k][1]-a[k][0]; fb=b[k][1]-b[k][0]
        if fb>fa or b[k][0]<a[k][0]:
            print(f"SUITE {k}: pass {a[k][0]}->{b[k][0]} judged {a[k][1]}->{b[k][1]} fail {fa}->{fb}"); bad+=1
print("COVDIFF_FLAGS", bad)
PY
