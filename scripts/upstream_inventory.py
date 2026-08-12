"""Upstream checker functions vs what this port anchors.

Builds three columns for every function in `internal/checker/*.go`:
  - upstream line count
  - whether any Rust file names it (the port's anchor convention)
  - the size of the Rust item that names it

The interesting rows are ANCHORED-AND-MUCH-SMALLER: a 40-line upstream
predicate cited by a 6-line Rust function is the shape that produced §235–§238.
"""

import json
import re
import subprocess
from pathlib import Path

ROOT = Path("/Users/mohebifar/dev/codemod/tsr/.claude/worktrees/checker-1-printing")
GO_DIR = ROOT / "vendor/typescript-go/internal/checker"
RUST_DIRS = [ROOT / "crates"]

# ---- 1. upstream functions and their sizes -------------------------------
funcs = {}  # name -> (file, startline, linecount)
for go in sorted(GO_DIR.glob("*.go")):
    if go.name.endswith("_test.go"):
        continue
    lines = go.read_text(errors="replace").splitlines()
    starts = []
    for i, line in enumerate(lines):
        m = re.match(r"^func (?:\([^)]*\) )?([A-Za-z_][A-Za-z0-9_]*)\(", line)
        if m:
            starts.append((i, m.group(1)))
    for idx, (i, name) in enumerate(starts):
        end = starts[idx + 1][0] if idx + 1 < len(starts) else len(lines)
        # trim trailing blank lines
        j = end
        while j > i and not lines[j - 1].strip():
            j -= 1
        funcs.setdefault(name, (go.name, i + 1, j - i))

# ---- 2. every identifier our Rust source mentions -------------------------
rust_text = []
for d in RUST_DIRS:
    for rs in d.rglob("*.rs"):
        if "/generated/" in str(rs):
            continue
        rust_text.append((rs, rs.read_text(errors="replace")))

mentions = {}
for name in funcs:
    if len(name) < 6:
        continue  # too short to be a reliable citation
    for path, text in rust_text:
        if name in text:
            mentions.setdefault(name, []).append(path.name)

print(json.dumps({
    "upstream_functions": len(funcs),
    "cited_by_port": len(mentions),
    "uncited": len(funcs) - len(mentions),
}, indent=2))

Path("/private/tmp/claude-501/-Users-mohebifar-dev-codemod-tsr--claude-worktrees-checker-1-printing/180a6fb4-fb9a-4c6c-bea1-c731201c4547/scratchpad/funcs.json").write_text(
    json.dumps({k: {"file": v[0], "line": v[1], "len": v[2],
                    "cited": mentions.get(k, [])} for k, v in funcs.items()})
)
print("written")
