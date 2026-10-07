#!/usr/bin/env python3
"""Opt-in TSR/TSR confirmation; raw output stays at the caller-selected path."""
import importlib.util
import json
import os
import re
import statistics
from pathlib import Path
import argparse
parser=argparse.ArgumentParser(description="Compare two TSR revisions with frozen inputs and fresh processes")
parser.add_argument("--baseline", type=Path, required=True)
parser.add_argument("--candidate", type=Path, required=True)
parser.add_argument("--baseline-source", required=True)
parser.add_argument("--candidate-source", required=True)
parser.add_argument("--project", type=Path, required=True, help="directory containing tsconfig.json")
parser.add_argument("--output", type=Path, required=True, help="local raw evidence directory")
parser.add_argument("--samples", type=int, default=5)
parser.add_argument("--checkers", type=int, help="use the same positive checker count for both TSR builds")
parser.add_argument("--extended-diagnostics", action="store_true")
args=parser.parse_args()
if args.samples < 1: parser.error("--samples must be positive")
if args.checkers is not None and args.checkers < 1: parser.error("--checkers must be positive")
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location("perf",ROOT/"scripts/whole_project_perf.py")
perf=importlib.util.module_from_spec(spec);spec.loader.exec_module(perf)
OUT=args.output.resolve();OUT.mkdir(parents=True,exist_ok=True)
BINS={"baseline":args.baseline.resolve(strict=True),"candidate":args.candidate.resolve(strict=True)}
FLAGS=["--noEmit","--incremental","false","--composite","false","--pretty","false"]
if args.checkers is not None:
    FLAGS += ["--checkers", str(args.checkers)]
report={"source_sha":args.candidate_source,"baseline_source":args.baseline_source,"binary_sha256":{k:perf.inputs.file_hash(v) for k,v in BINS.items()},"machine":{"platform":perf.platform.platform(),"cpu_count":os.cpu_count(),"load_average":os.getloadavg()},"flags":FLAGS,"workloads":{},"limitations":["Complete resolver queries and transient edits are not captured.","Bundled libraries are pinned by binary hash; source query union is partial.","Generated fixtures suppress declaration checking; real projects retain their input configuration.","Run timing without concurrent builds/corpus work; this harness does not isolate the host.","This compares two TSR builds; it does not establish the native release target."]}
def save():
    (OUT/"confirmation.json").write_text(json.dumps(report,indent=2)+"\n")
for name,cwd in [("project",args.project.resolve(strict=True))]:
    w={"cwd":str(cwd),"preflight":{},"warmups":[],"samples":[]};report["workloads"][name]=w;save()
    names={}
    for kind,binary in BINS.items():
        config=perf.process([str(binary),*FLAGS,"--showConfig"],cwd,120)
        listing=perf.process([str(binary),*FLAGS,"--listFilesOnly"],cwd,120)
        assert config["exit_code"]==listing["exit_code"]==0,(kind,config,listing)
        names[kind]=[perf.file_identity(line,cwd) for line in listing["stdout"].splitlines() if line.strip()]
        w["preflight"][kind]={"config":config,"listing":listing,"loaded_order_sha256":perf.fingerprint(names[kind]),"count":len(names[kind])};save()
    assert names["baseline"]==names["candidate"],name
    assert json.loads(w["preflight"]["baseline"]["config"]["stdout"])==json.loads(w["preflight"]["candidate"]["config"]["stdout"]),name
    paths=sorted({str(cwd/"tsconfig.json"),*(str(p) for p in BINS.values()),*(n for n in names["baseline"] if not n.startswith("<typescript-lib>/"))})
    reference=perf.inputs.snapshot(paths);assert perf.inputs.valid_snapshot(reference)
    w["input_path_count"]=len(paths);w["input_reference_fingerprint"]=perf.fingerprint(reference);save()
    for i in range(args.samples+1):
        order=["baseline","candidate"] if i%2==0 else ["candidate","baseline"]
        for kind in order:
            before=perf.inputs.snapshot(paths);assert before==reference,(name,"input changed")
            row=perf.process([str(BINS[kind]),*FLAGS,*( ["--extendedDiagnostics"] if args.extended_diagnostics else [] )],cwd,120)
            after=perf.inputs.snapshot(paths);assert after==reference,(name,"input changed")
            row.update(tool=kind,pair=i-1,input_before=perf.fingerprint(before),input_after=perf.fingerprint(after))
            row["diagnostics"]=perf.diagnostics(row["stdout"],cwd)
            row["phases"]={m.group(1).strip():float(m.group(2)) for m in re.finditer(r"^(Loader time|Parse time|Bind time|Check time):\s*([0-9.]+)s",row["stdout"],re.M)}
            row["counts"]={m.group(1).strip():int(m.group(2)) for m in re.finditer(r"^(Files|Parsed files|Checked files):\s*([0-9]+)",row["stdout"],re.M)}
            assert row["exit_code"] in (0,1,2) and not row["timed_out"]
            w["warmups" if i==0 else "samples"].append(row);save()
            print(name,kind,round(row["wall_seconds"],4),row["phases"],row["counts"],flush=True)
    w["summary"]={k:perf.summary([r for r in w["samples"] if r["tool"]==k]) for k in BINS}
    w["phase_medians"]={k:{phase:statistics.median([r["phases"][phase] for r in w["samples"] if r["tool"]==k]) for phase in w["samples"][0]["phases"]} for k in BINS}
    w["wall_ratio"]=w["summary"]["candidate"]["wall_seconds"]["median"]/w["summary"]["baseline"]["wall_seconds"]["median"]
    w["diagnostics_identical"]=len({(r["diagnostics"]["fingerprint"],r["exit_code"]) for r in w["samples"]})==1
    w["counts_identical"]=(len({json.dumps(r["counts"],sort_keys=True) for r in w["samples"]})==1) if args.extended_diagnostics else None
    assert w["diagnostics_identical"] and w["counts_identical"] is not False;save()
    print(name,"ratio",w["wall_ratio"],"summary",w["summary"],flush=True)
report["status"]="complete";save()
