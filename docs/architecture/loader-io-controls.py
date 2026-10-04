"""Physical encoding controls for the temporary I/O probe; no timing gate.

See loader-io-performance.md for measured source and binary identities.
"""
import argparse
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("perf", ROOT / "scripts/whole_project_perf.py")
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)


def fixtures():
    base = '// encoding control\nconst wrong: number = "s";\n'
    return {
        "utf8": base.encode(),
        "utf8-bom": b"\xef\xbb\xbf" + base.encode(),
        "utf16-le": b"\xff\xfe" + base.encode("utf-16-le"),
        "utf16-be": b"\xfe\xff" + base.encode("utf-16-be"),
        "utf16-odd": b"\xff\xfe" + base.encode("utf-16-le") + b"\x42",
        "utf16-surrogate": b"\xff\xfe" + b"/\x00/\x00 \x00\x00\xd8\n\x00"
                            + base.split("\n", 1)[1].encode("utf-16-le"),
        "malformed-utf8-comment": b'// bad UTF-8: \xff \xed\xa0\x80 \xe2\x82\nconst wrong: number = "s";\n',
        "empty": b"",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("normal", "probe", "tsgo", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    if out.exists():
        parser.error("output must be a new directory")
    out.mkdir(parents=True)
    binaries = {"normal": args.normal.resolve(), "disabled": args.probe.resolve(),
                "enabled": args.probe.resolve(), "repeat": args.probe.resolve(),
                "native": args.tsgo.resolve()}
    state = {"kind": "public physical I/O controls; no speed claim",
             "binary_sha256": {role: hashlib.sha256(p.read_bytes()).hexdigest()
                               for role, p in binaries.items()}, "cases": []}

    def persist():
        p = out / "results.json"
        p.write_text(json.dumps(state, indent=2) + "\n")
        assert json.loads(p.read_text()) == state

    persist()
    for name, contents in fixtures().items():
        cwd = out / name
        cwd.mkdir()
        source = cwd / "input.ts"
        source.write_bytes(contents)
        config = cwd / "tsconfig.json"
        config.write_text(json.dumps({"compilerOptions": {
            "strict": True, "target": "es2020", "types": [], "skipLibCheck": True,
            "noEmit": True, "incremental": False, "composite": False,
        }, "files": ["input.ts"]}, indent=2) + "\n")
        case = {"name": name, "source_base64": base64.b64encode(contents).decode(),
                "input_sha256": hashlib.sha256(contents).hexdigest(),
                "config_sha256": hashlib.sha256(config.read_bytes()).hexdigest(), "runs": []}
        state["cases"].append(case)
        for role, binary in binaries.items():
            enabled = role in ("enabled", "repeat")
            saved = dict(os.environ)
            env = {k: v for k, v in saved.items() if not k.startswith("TSR_")}
            env["TSR_LIB_PATH"] = str(ROOT / "vendor/typescript-go/internal/bundled/libs")
            env["TSR_IO_SCOPE"] = "1"
            if enabled:
                env["TSR_IO_PROFILE"] = "1"
            command = [str(binary), "--project", str(config), "--pretty", "false"]
            if role == "native":
                command.append("--singleThreaded")
            try:
                os.environ.clear()
                os.environ.update(env)
                run = perf.process(command, cwd, 60)
            finally:
                os.environ.clear()
                os.environ.update(saved)
            for stream in ("stdout", "stderr"):
                (cwd / f"{role}.{stream}").write_text(run[stream])
            checked = [perf.file_identity(line.split("\t", 1)[1], cwd)
                       for line in run["stderr"].splitlines() if line.startswith("TSR_IO_CHECKED\t")]
            counts = [json.loads(line.split("\t", 1)[1]) for line in run["stderr"].splitlines()
                      if line.startswith("TSR_IO_COUNTS\t")]
            row = {k: run[k] for k in ("wall_seconds", "user_seconds", "system_seconds",
                                      "peak_rss_bytes", "exit_code", "timed_out")}
            row.update(role=role, diagnostics=perf.diagnostics(run["stdout"], cwd), checked=checked,
                       checked_input_fingerprint=perf.input_fingerprint(checked) if checked else None,
                       counts=counts[0] if counts else None)
            case["runs"].append(row)
            persist()
            assert not run["timed_out"] and run["exit_code"] in (0, 1, 2)
            assert len(counts) == int(enabled)
        runs = case["runs"]
        assert all(r["diagnostics"] == runs[0]["diagnostics"] for r in runs[:4])
        assert all(r["checked"] == [str(source)] for r in runs[1:4])
        assert all(r["checked_input_fingerprint"] == runs[1]["checked_input_fingerprint"]
                   for r in runs[1:4])
        assert {k: v for k, v in runs[2]["counts"].items() if not k.endswith("_ns")} == {
            k: v for k, v in runs[3]["counts"].items() if not k.endswith("_ns")}
        case["native_diagnostics_match"] = runs[0]["diagnostics"] == runs[-1]["diagnostics"]
        assert runs[-1]["diagnostics"]["count"] == int(name != "empty")
        persist()
        print(json.dumps({"case": name, "native_match": case["native_diagnostics_match"]}), flush=True)
    state["complete"] = True
    state["equivalent_diagnostics_gate"] = all(c["native_diagnostics_match"] for c in state["cases"])
    persist()


if __name__ == "__main__":
    main()
