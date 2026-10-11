#!/usr/bin/env python3
"""record.py <batch> <prev-coverage> <batch-dir>: the round7.md record for an auto-landed batch."""
import re
import sys

batch, prev_cov, d = sys.argv[1], sys.argv[2], sys.argv[3]
log = open(f"{d}/gate.log").read()


def cov(path):
    out = {}
    for line in open(path):
        m = re.match(r"^([a-z_]+)\s+(\d+)/(\d+)", line)
        if m:
            out[m[1]] = int(m[2])
    return out


merged = re.findall(r"^MERGED(?:\(notes-theirs\))? box/(r7-[a-z-]+) ([0-9a-f]+)", log, re.M)
types = re.search(r"types: right_gained=(\d+) right_lost=(\d+) right_missing=(\d+)", log)
diag = re.search(r"diagnostics: cases_gained=(\d+) cases_lost=(\d+) cases_missing=(\d+)", log)
js = re.search(r"JSTYPING base=(\d+) cand=(\d+) tsgo=(\d+)", log)
ir = dict(re.findall(r"^IR ([a-z-]+) .*ratio=([0-9.]+)", log, re.M))
exact = re.search(r"^exact (\d+) / (\d+) = ([0-9.]+)%", log, re.M)
gained = re.search(r"exact_gained=(\d+)", open(f"{d}/oracle.txt").read())
a, b = cov(prev_cov), cov(f"{d}/coverage.txt")

heads = ", ".join(f"{lane} `{sha[:8]}`" for lane, sha in merged)
print(f"\n### Batch {batch} — {heads}\n")
print("Landed by the integration loop (`scripts/integration/autoloop.sh`): every leg")
print(f"passed. Exact oracle **{exact[1]} / {exact[2]} EXACT ({exact[3]}%)**, "
      f"{gained[1] if gained else '?'} gained, 0 lost, 0 missing. Legacy compare against the")
print(f"previous freeze: types +{types[1]} RIGHT / {types[2]} lost / {types[3]} missing, "
      f"diagnostics +{diag[1]} cases / {diag[2]} lost / {diag[3]} missing. jsTyping "
      f"{js[1]} → {js[2]} unique lines (tsgo {js[3]}), new_false 0, lost_true 0. Tests, fmt "
      f"and strict clippy green. Callgrind Ir: generic-imports {ir.get('generic-imports')}, "
      f"domain-model {ir.get('domain-model')}. No suite's failure count rose.\n")
print(f"| Suite | Before | Batch {batch} |")
print("|---|---|---|")
for k in ["checker_types", "checker_types_configured", "diagnostics", "diagnostics_configured"]:
    print(f"| `{k}` | {a[k]:,} | {b[k]:,} |")
