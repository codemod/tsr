"""Verify the frozen native-member control report without running compilers."""
from pathlib import Path
import argparse
import hashlib
import json
import runpy


def final_rows(run):
    # Each file dump is cumulative: use the final dump for each private checker.
    owners = {dump["owner"]: dump["rows"] for dump in run["snapshots"]}
    return [row for rows in owners.values() for row in rows]


def semantic_rows(run):
    result = []
    for row in final_rows(run):
        value = {k: v for k, v in row.items()
                 if k not in ("id", "owner", "type_owner", "references")}
        value["reference_shapes"] = [
            {"parameters": len(ref["parameters"]), "arguments": len(ref["arguments"]),
             "padded": len(ref["padded"]), "this_appended": ref["this_appended"],
             "self_receiver": bool(ref["padded"]) and ref["padded"][-1] == row["id"]}
            for ref in row["references"] or []]
        result.append(json.dumps(value, sort_keys=True))
    return sorted(result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", nargs="?", type=Path,
                        default=Path(__file__).with_name("checker-native-member-completion.json"))
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    fixtures = runpy.run_path(str(args.report.with_name("checker-native-member-completion-fixtures.py")))["fixtures"]()
    controls = report["controls"]
    assert controls["complete"] and len(controls["cases"]) == len(fixtures) == 22
    assert report["temporary_probe_restored"] and report["shared_vendor_clean"]
    assert report["analysis"]["internal_api_controls"]["pass"]
    assert report["analysis"]["internal_api_controls"]["tests"] == 3
    matches = 0
    for case in controls["cases"]:
        files = fixtures[case["name"]]
        assert {k: hashlib.sha256(v.encode()).hexdigest() for k, v in files.items()} == case["source_sha256"]
        config = {"compilerOptions": {"strict": True, "target": "es2020", "types": [],
                  "skipLibCheck": True, "noEmit": True, "incremental": False, "composite": False},
                  "files": list(files)}
        assert hashlib.sha256((json.dumps(config, indent=2) + "\n").encode()).hexdigest() == case["config_sha256"]
        assert case["complete"] and len(case["runs"]) == 5
        clean, disabled, enabled, repeat, tsr = case["runs"]
        assert [r["role"] for r in case["runs"]] == ["clean", "disabled", "enabled", "repeat", "tsr"]
        assert len({r["pid"] for r in case["runs"]}) == 5
        assert all(r["input_before"] == r["input_after"] for r in case["runs"])
        assert all(not r["timed_out"] and r["exit_code"] in (0, 1, 2) for r in case["runs"][:4])
        assert not clean["snapshots"] and not disabled["snapshots"]
        assert all(r["diagnostics"] == clean["diagnostics"] and r["loaded"] == clean["loaded"]
                   for r in (disabled, enabled, repeat))
        assert enabled["checked"] == repeat["checked"]
        assert sorted(Path(p).name for p in enabled["checked"]) == sorted(files)
        assert semantic_rows(enabled) == semantic_rows(repeat)
        assert all(r["owner"] == r["type_owner"] and r["active_depth"] == 0 for r in final_rows(enabled))
        valid_tsr = not tsr["timed_out"] and tsr["exit_code"] in (0, 1, 2)
        matched = valid_tsr and tsr["diagnostics"] == clean["diagnostics"]
        assert case["tsr_native_diagnostics_equal"] == matched
        matches += matched
    assert matches == 17
    by_name = {c["name"]: c for c in controls["cases"]}
    def named(case, name):
        return [r for r in final_rows(by_name[case]["runs"][2]) if r["name"] == name]
    box = next(r for r in named("repeat-32", "MC_Box") if r["queries"] == 32)
    assert box["workers"] == {"object": 1, "reference": 1} and box["final_flag_hits"] == 31
    assert box["references"][0]["this_appended"]
    empty = named("empty-interface", "MC_Empty")[0]
    assert empty["workers"] == {"class": 1, "object": 1} and empty["final_flag_hits"] == 4
    assert empty["final_view"]["names"] == []
    assert named("diamond-order", "MC_Derived")[0]["final_view"]["names"] == ["ownA", "ownB", "base", "left", "right"]
    assert named("shadow-order", "MC_Derived")[0]["final_view"]["names"] == ["shared", "own", "base"]
    assert sorted(r["final_view"]["names"] for r in named("same-spelled", "MC_Shape")) == [["left"], ["right"]]
    assert sum(r["active_flag_hits"] + r["active_misses"] + r["resolved_resets"] for c in controls["cases"]
               for r in final_rows(c["runs"][2])) == 0
    assert by_name["recursive-default"]["runs"][-1]["exit_code"] == -6
    for filename, digest in report["artifact_sha256"].items():
        assert hashlib.sha256(args.report.with_name(filename).read_bytes()).hexdigest() == digest
    print("Verified 22 native-output/counter controls, 17 TSR diagnostic matches, 3 native API-state tests; no speed claim.")


if __name__ == "__main__":
    main()
