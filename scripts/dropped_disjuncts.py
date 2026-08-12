"""Third pass: count Rust disjunction the way Rust spells it.

Pass two ranked `isPrimitiveTypeName` (5 disjuncts upstream, 0 in ours) at the
top and it is a PERFECT port — Rust writes that as
`matches!(name, "any" | "string" | ...)`, which contains no `||` at all. The
`||` heuristic measures a Go idiom against a Rust file.

So the Rust side counts `||` plus the `|` separators inside `matches!` patterns
and inside `match` arms. Still approximate — a bitwise `|` on flags will be
miscounted — but flags-or is itself usually a ported disjunction, so the error
is in the safe direction: it makes a pair look MORE ported, hiding candidates
rather than inventing them.
"""

import json
import re
from pathlib import Path

ROOT = Path("/Users/mohebifar/dev/codemod/tsr/.claude/worktrees/checker-1-printing")
GO_DIR = ROOT / "vendor/typescript-go/internal/checker"
SCRATCH = Path(
    "/private/tmp/claude-501/-Users-mohebifar-dev-codemod-tsr--claude-worktrees-checker-1-printing/"
    "180a6fb4-fb9a-4c6c-bea1-c731201c4547/scratchpad"
)

funcs = json.loads((SCRATCH / "funcs.json").read_text())
go_lines = {p.name: p.read_text(errors="replace").splitlines() for p in GO_DIR.glob("*.go")}


def snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower().replace("__", "_")


def strip_comments(text, marker="//"):
    return "\n".join(
        line.split(marker)[0] for line in text.splitlines() if not line.strip().startswith(marker)
    )


def go_disjuncts(text):
    return strip_comments(text).count("||")


def rust_disjuncts(text):
    body = strip_comments(text, "//")
    # `||` is a disjunction; so is every `|` that separates match/matches! arms.
    # A closure's `|a, b|` is not, so require whitespace on both sides and no
    # comma inside — crude, and biased towards over-counting, which hides
    # candidates rather than inventing them.
    ors = body.count("||")
    pipes = len(re.findall(r"[^|&]\s\|\s[^|]", body))
    return ors + pipes


rust_funcs = {}
for rs in (ROOT / "crates").rglob("*.rs"):
    if "/generated/" in str(rs) or "/tests/" in str(rs):
        continue
    text = rs.read_text(errors="replace")
    starts = [
        (m.start(), m.group(1))
        for m in re.finditer(r"\n\s*(?:pub(?:\([^)]*\))? )?fn ([a-z_0-9]+)", text)
    ]
    for i, (pos, name) in enumerate(starts):
        end = starts[i + 1][0] if i + 1 < len(starts) else len(text)
        rust_funcs.setdefault(name, []).append((rs.name, text[pos:end]))

rows = []
for name, info in funcs.items():
    ls = go_lines[info["file"]]
    body = "\n".join(ls[info["line"] - 1 : info["line"] - 1 + info["len"]])
    ups = go_disjuncts(body)
    if ups < 2:
        continue
    key = snake(name)
    cands = rust_funcs.get(key) or rust_funcs.get(key.removeprefix("get_")) or []
    if not cands:
        continue
    best_file, best = max(((f, rust_disjuncts(b)) for f, b in cands), key=lambda x: x[1])
    gap = ups - best
    if gap >= 2:
        rows.append((gap, ups, best, name, info["file"], info["line"], key, best_file))

rows.sort(reverse=True)
print(f"{'gap':>4} {'up':>3} {'ours':>4}  upstream -> port")
for gap, ups, best, name, gofile, goline, key, rfile in rows[:20]:
    print(f"{gap:>4} {ups:>3} {best:>4}  {name} ({gofile}:{goline}) -> {rfile}::{key}")
print(f"\ncandidates after correcting for Rust idiom: {len(rows)}")
