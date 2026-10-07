#!/usr/bin/env python3
"""Generate the public dependency workload used by parallel-front-end.md."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
parser.add_argument("--files", type=int, default=768, choices=(192, 768))
args = parser.parse_args()
args.directory.mkdir(parents=True, exist_ok=True)
# Preserve the measured lexicographic source-template order.
templates = [
    ("export {};\n" + "".join(
        f"export interface I{i}_{j} {{ a: string; b: number; c?: boolean; fn(x: number): string; }}\n"
        for j in range(240)
    )).rstrip("\n")
    for i in sorted(range(192), key=lambda i: f"f{i}.d.ts")
]
for i in range(args.files):
    (args.directory / f"f{i:04}.d.ts").write_text(templates[i % 192])
(args.directory / "index.ts").write_text(
    "".join(f'/// <reference path="./f{i:04}.d.ts" />\n' for i in range(args.files))
    + "export {};\n"
)
(args.directory / "tsconfig.json").write_text(json.dumps({
    "compilerOptions": {"strict": True, "skipLibCheck": True, "noEmit": True},
    "files": ["index.ts"],
}))
