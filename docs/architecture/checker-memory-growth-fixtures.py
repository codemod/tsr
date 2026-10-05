#!/usr/bin/env python3
"""Skewed mapped properties and a cross-file generic cycle for memory controls."""
import argparse
import importlib.util
from pathlib import Path
import sys

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("memory", Path(__file__).with_name("checker-memory-controls.py"))
memory = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(memory)


def generate(directory, width=128, files=16):
    memory.require(width > 0 and files > 0, "positive dimensions required")
    memory.fixtures(directory, width, files)
    for index in range(files):
        path = directory / f"file{index}.ts"
        count = width * (8 if index == 0 else 1)
        values = ", ".join(f"p{column}: {column}" for column in range(count))
        path.write_text(path.read_text() + f"export const image{index}: Mapped{index} = {{{values}}};\n")
    (directory / "a.ts").write_text("import type {B} from './b';\nexport interface A<T> {value:T; next?:B<T>}\n")
    (directory / "b.ts").write_text("import type {A} from './a';\nexport interface B<T> {value:T; next?:A<T>}\n")
    (directory / "cycle.ts").write_text("import type {A} from './a';\n"
                                        "export const chain:A<number>={value:1,next:{value:2,next:{value:3}}};\n")
    manifest = {path.name: memory.file_hash(path) for path in directory.iterdir()
                if path.name != "fixture-manifest.json"}
    memory.write(directory / "fixture-manifest.json", manifest)
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--width", type=int, default=128)
    parser.add_argument("--files", type=int, default=16)
    args = parser.parse_args()
    print({"generated_files": len(generate(args.directory, args.width, args.files))})
