"""Physical resolver controls for metadata attribution; no throughput claim."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("perf", ROOT / "scripts/whole_project_perf.py")
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)


def fixtures():
    return {
        "relative-extension": {
            "main.ts": 'import { value } from "./dep.js";\nconst wrong: number = value;\n',
            "dep.ts": "export declare const value: string;\n",
        },
        "package-exports-imports": {
            "package.json": json.dumps({"name": "scope", "imports": {"#local": "./src/local.ts"}}),
            "src/main.ts": 'import { local } from "#local";\n'
                           'import { value } from "thing";\n'
                           'import { missing } from "absent-package";\n'
                           'const wrong: number = local + value;\nexport const held = missing;\n',
            "src/local.ts": "export declare const local: string;\n",
            "node_modules/thing/package.json": json.dumps({
                "name": "thing", "version": "1.0.0", "exports": {
                    ".": {"types": "./types/index.d.ts", "default": "./dist/index.js"}}}),
            "node_modules/thing/types/index.d.ts": "export declare const value: string;\n",
        },
        "symlink-package": {
            "main.ts": 'import type { Box } from "linked";\n'
                       'declare const box: Box;\nconst wrong: number = box.value;\n',
            "store/linked/package.json": json.dumps({
                "name": "linked", "version": "1.0.0", "types": "./index.d.ts"}),
            "store/linked/index.d.ts": "export interface Box { value: string }\n",
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("normal", "scope", "probe", "tsgo", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    if out.exists():
        parser.error("output must be a new directory")
    out.mkdir(parents=True)
    state = {"kind": "public physical metadata/resolution controls; no speed claim",
             "native_binary_sha256": hashlib.sha256(args.tsgo.read_bytes()).hexdigest(),
             "cases": []}

    def save():
        p = out / "results.json"
        p.write_text(json.dumps(state, indent=2) + "\n")
        assert json.loads(p.read_text()) == state

    save()
    for name, files in fixtures().items():
        cwd = out / name
        cwd.mkdir()
        for relative, text in files.items():
            p = cwd / relative
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
        if name == "symlink-package":
            (cwd / "node_modules").mkdir()
            (cwd / "node_modules/linked").symlink_to("../store/linked", target_is_directory=True)
        root = "src/main.ts" if name == "package-exports-imports" else "main.ts"
        config = cwd / "tsconfig.json"
        config.write_text(json.dumps({"compilerOptions": {
            "strict": True, "target": "es2020", "module": "esnext",
            "moduleResolution": "bundler", "types": [], "skipLibCheck": True,
            "noEmit": True, "incremental": False, "composite": False,
        }, "files": [root]}, indent=2) + "\n")
        command = [sys.executable, str(ROOT / "docs/architecture/metadata-origin-controls.py")]
        for role in ("normal", "scope", "probe"):
            command.extend(["--" + role, str(getattr(args, role).resolve())])
        command.extend(["--project", str(config), "--output", str(cwd / "control-runs")])
        subprocess.run(command, check=True)
        result = json.loads((cwd / "control-runs/results.json").read_text())
        saved = dict(os.environ)
        try:
            os.environ.clear()
            os.environ.update({k: v for k, v in saved.items() if not k.startswith("TSR_")})
            native = perf.process([str(args.tsgo.resolve()), "--project", str(config),
                                   "--noEmit", "--incremental", "false", "--composite", "false",
                                   "--pretty", "false", "--singleThreaded"], cwd, 60)
        finally:
            os.environ.clear()
            os.environ.update(saved)
        for stream in ("stdout", "stderr"):
            (cwd / f"native.{stream}").write_text(native[stream])
        assert not native["timed_out"] and native["exit_code"] in (0, 1, 2)
        expected = perf.diagnostics(native["stdout"], cwd)
        actual = result["runs"][0]["diagnostics"]
        case = {"name": name, "files": files,
                "config_sha256": hashlib.sha256(config.read_bytes()).hexdigest(),
                "native_diagnostics": expected, "actual_diagnostics": actual,
                "native_diagnostics_match": expected == actual,
                "loaded": len(result["loaded_paths"]),
                "actually_checked": len(result["runs"][1]["checked_paths"]),
                "counter_repeat_match": True}
        state["cases"].append(case)
        save()
        print(json.dumps({"case": name, "native_match": case["native_diagnostics_match"]}), flush=True)
    state["complete"] = True
    state["native_diagnostics_match"] = all(c["native_diagnostics_match"] for c in state["cases"])
    save()


if __name__ == "__main__":
    main()
