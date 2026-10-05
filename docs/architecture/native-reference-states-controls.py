#!/usr/bin/env python3
"""Run pinned private reference-state controls in a clean isolated native checkout."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time

PIN = "5b1047d10d32e7d5b446be4de56b126ff42f82bb"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-source", type=Path, required=True)
    parser.add_argument("--go", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--go-cache", type=Path)
    args = parser.parse_args()
    directory = Path(__file__).resolve().parent
    source, output = args.native_source.resolve(), args.output.resolve()
    repository = directory.parents[1]
    if source == repository or source.is_relative_to(repository):
        parser.error("use an isolated native checkout outside the artifact repository")
    if output.exists() or output.is_relative_to(source):
        parser.error("output must be new and outside the native checkout")
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    if head != PIN:
        parser.error("native source does not match the pinned revision")
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=source):
        parser.error("native checkout has local changes")
    module = source / "internal/checker/reference_states_test.go"
    if module.exists():
        parser.error("test module already exists")
    checker = source / "internal/checker/checker.go"
    original = checker.read_bytes()
    committed = subprocess.check_output(["git", "show", "HEAD:internal/checker/checker.go"], cwd=source)
    if original != committed:
        parser.error("native checker source differs from its commit")
    go = args.go.resolve()
    version = subprocess.check_output([str(go), "version"], text=True).strip()
    output.mkdir(parents=True)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith("TSR_") and key not in
                   ("GOFLAGS", "GOCACHE", "GOTOOLCHAIN", "GOPROXY", "GOSUMDB", "GOMAXPROCS")}
    environment.update(GOCACHE=str((args.go_cache or output / "go-cache").resolve()),
                       GOTOOLCHAIN="local", GOPROXY="off", GOSUMDB="off", GOMAXPROCS="2")
    report = {
        "native_source": head, "go_version": version, "go_sha256": digest(go),
        "driver_sha256": digest(Path(__file__)),
        "test_sha256": digest(directory / "native-reference-states-test.go"),
        "checker_original_sha256": hashlib.sha256(original).hexdigest(),
        "environment": {key: environment[key] for key in
                        ("GOCACHE", "GOTOOLCHAIN", "GOPROXY", "GOSUMDB", "GOMAXPROCS")},
        "children": [], "restored": False,
        "coverage": "private API state controls; actual Rust/public/pool work remains unverified",
    }

    def save() -> None:
        path = output / "results.json"
        path.write_text(json.dumps(report, indent=2) + "\n")
        assert json.loads(path.read_text()) == report

    def run(label: str, command: list[str], expected_success: bool = True) -> None:
        row = {"stage": label, "command": [str(go), *command],
               "expected_success": expected_success, "status": "running",
               "checker_sha256": digest(checker), "test_sha256": digest(module)}
        report["children"].append(row)
        with (output / f"{label}.stdout").open("wb") as stdout, (output / f"{label}.stderr").open("wb") as stderr:
            started = time.monotonic()
            child = subprocess.Popen(row["command"], cwd=source, env=environment,
                                     stdout=stdout, stderr=stderr, start_new_session=True)
            row["pid"] = child.pid
            save()
            print("START", label, child.pid, flush=True)
            try:
                code = child.wait(timeout=600)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
                row.update(status="timed_out", exit_code=child.returncode)
                save()
                raise
            except BaseException:
                if child.poll() is None:
                    os.killpg(child.pid, signal.SIGTERM)
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(child.pid, signal.SIGKILL)
                        child.wait()
                row.update(status="interrupted", exit_code=child.returncode)
                save()
                raise
        row.update(status="complete" if code == 0 else "failed", exit_code=code,
                   wall_seconds=time.monotonic() - started,
                   stdout_sha256=digest(output / f"{label}.stdout"),
                   stderr_sha256=digest(output / f"{label}.stderr"))
        save()
        if expected_success:
            assert code == 0, f"{label} failed; retain stdout/stderr"
        else:
            text = (output / f"{label}.stdout").read_text()
            assert code == 1 and "--- FAIL: TestReferenceState" in text, "mutation did not fail an intended test"
        print("END", label, code, flush=True)

    save()
    module.write_bytes((directory / "native-reference-states-test.go").read_bytes())
    try:
        run("focused", ["test", "-mod=readonly", "-p", "1", "./internal/checker", "-run", "^TestReferenceState", "-count=5", "-v"])
        run("race", ["test", "-mod=readonly", "-race", "-p", "1", "./internal/checker", "-run", "^TestReferenceState", "-count=1", "-v"])
        run("suite", ["test", "-mod=readonly", "-p", "1", "./internal/checker"])
        run("vet", ["vet", "-mod=readonly", "./internal/checker"])
        mutations = (
            ("false-members-completion", "\td.resolvedTypeArguments = typeArguments\n\tintf.instantiations[id] = t",
             "\td.resolvedTypeArguments = typeArguments\n\tt.objectFlags |= ObjectFlagsMembersResolved\n\tintf.instantiations[id] = t"),
            ("error-cache-value", "\t\tcache[key] = result\n\t}\n\tc.instantiationDepth--",
             "\t\tif result == c.errorType {\n\t\t\tcache[key] = c.unknownType\n\t\t} else {\n\t\t\tcache[key] = result\n\t\t}\n\t}\n\tc.instantiationDepth--"),
        )
        for label, before, after in mutations:
            text = original.decode()
            assert text.count(before) == 1, "mutation anchor must be unique"
            mutant = text.replace(before, after).encode()
            checker.write_bytes(mutant)
            try:
                run(label, ["test", "-mod=readonly", "-p", "1", "./internal/checker", "-run", "^TestReferenceState", "-count=1", "-v"], False)
            finally:
                assert checker.read_bytes() == mutant, "unexpected concurrent checker edit"
                checker.write_bytes(original)
        report["qualified"] = True
    finally:
        assert checker.read_bytes() == original, "native checker restoration failed"
        assert digest(module) == report["test_sha256"], "unexpected concurrent test edit"
        module.unlink()
        assert not subprocess.check_output(["git", "status", "--porcelain"], cwd=source), "native checkout not clean"
        report["restored"] = True
        save()


if __name__ == "__main__":
    main()
