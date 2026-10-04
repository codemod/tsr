"""Two five-pair normal CLI rounds; input snapshots run outside timing."""
from pathlib import Path
import argparse, importlib.util, json, hashlib, statistics, os, sys
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('construction', ROOT / 'docs/architecture/resolver-construction-controls.py')
construction = importlib.util.module_from_spec(spec); spec.loader.exec_module(construction)
perf = construction.metadata.perf
parser = argparse.ArgumentParser(description='Serialized normal CLI retention controls; no native target claim.')
for name in ('baseline', 'candidate', 'project', 'input-manifest', 'output'):
    parser.add_argument('--' + name, type=Path, required=True)
parser.add_argument('--source', type=Path)
args = parser.parse_args()
out = args.output.resolve()
assert not out.exists(), 'output must be a new directory'
out.mkdir(parents=True)
project = args.project.resolve()
flags = ['--project', str(project), '--noEmit', '--incremental', 'false', '--composite', 'false', '--pretty', 'false', '--extendedDiagnostics']
binaries = {name: getattr(args, name).resolve() for name in ('baseline', 'candidate')}
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
state = {'kind':'isolated TSR whole-project comparison; fresh processes, warmed OS inputs; no native target claim', 'source_identity':json.loads(args.source.read_text()) if args.source else None, 'binary_sha256':{name:sha(path) for name,path in binaries.items()}, 'driver_sha256':sha(Path(__file__)), 'flags':flags, 'rounds':[], 'complete':False, 'native_target_verified':False, 'complete_input_equivalence_verified':False}
result_path = out / 'paired.json'
assert not result_path.exists()
def save():
    result_path.write_text(json.dumps(state, indent=2)+'\n')
    assert json.loads(result_path.read_text()) == state
def run(binary, extra=()):
    assert all(sha(path)==state['binary_sha256'][name] for name,path in binaries.items())
    saved = dict(os.environ)
    try:
        os.environ.clear(); os.environ.update({k:v for k,v in saved.items() if not k.startswith('TSR_')})
        os.environ['TSR_LIB_PATH'] = str(ROOT / 'vendor/typescript-go/internal/bundled/libs')
        result = perf.process([str(binary), *flags, *extra], project.parent, 90)
    finally:
        os.environ.clear(); os.environ.update(saved)
    assert not result['timed_out'] and result['exit_code'] in (0,1,2)
    return result
def stats(result):
    return {line.split(':',1)[0]:line.split(':',1)[1].strip() for line in result['stdout'].splitlines() if line.startswith(('Files:','Checked files:','Parsed files:'))}
save()
paths = run(binaries['baseline'], ['--listFilesOnly'])['stdout'].splitlines()
assert paths and all(Path(path).is_file() for path in paths)
assert run(binaries['candidate'], ['--listFilesOnly'])['stdout'].splitlines()==paths
old = json.loads(args.input_manifest.read_text())
manifest = construction.input_snapshot([row['path'] for row in old])
assert manifest==old, 'supplied input snapshot changed; refresh preflight'
assert str(project) in {row['path'] for row in manifest}
manifest_paths = [row['path'] for row in manifest]
assert set(paths).issubset(manifest_paths)
state.update(loaded_count=len(paths), loaded_order_fingerprint=perf.fingerprint(paths), queried_input_count=len(manifest), queried_input_fingerprint=perf.fingerprint(manifest), input_coverage='supplied preflight paths, kinds, file bytes and realpaths; excludes directory entries/unobserved reads and transient changes')
configs={name:json.loads(run(binary,['--showConfig'])['stdout']) for name,binary in binaries.items()}
assert configs['baseline']==configs['candidate']
state['effective_config_fingerprint']=perf.fingerprint(configs['baseline'])
expected_diagnostics=None; expected_stats=None
for name,binary in binaries.items():
    assert construction.input_snapshot(manifest_paths)==manifest
    result=run(binary); diagnostic=perf.diagnostics(result['stdout'],project.parent)
    (out / (name+'.stdout')).write_text(result['stdout']); (out / (name+'.stderr')).write_text(result['stderr'])
    if expected_diagnostics is None: expected_diagnostics=diagnostic; expected_stats=stats(result)
    assert diagnostic==expected_diagnostics and stats(result)==expected_stats
    assert construction.input_snapshot(manifest_paths)==manifest
    print('warmup',name,result['wall_seconds'],flush=True)
state.update(diagnostics={k:v for k,v in expected_diagnostics.items() if k!='entries'}, checked_parsed_counts=expected_stats)
save()
for round_index in range(2):
    round_state={'round':round_index+1, 'samples':[]}; state['rounds'].append(round_state); save()
    for pair in range(5):
        order=('baseline','candidate') if (pair+round_index)%2==0 else ('candidate','baseline')
        for name in order:
            assert construction.input_snapshot(manifest_paths)==manifest
            result=run(binaries[name]); diagnostic=perf.diagnostics(result['stdout'],project.parent)
            row={k:result[k] for k in ('wall_seconds','user_seconds','system_seconds','peak_rss_bytes','exit_code','timed_out','pid')}
            row.update(role=name,pair=pair,diagnostics={k:v for k,v in diagnostic.items() if k!='entries'},stats=stats(result))
            round_state['samples'].append(row); save()
            assert diagnostic==expected_diagnostics and row['stats']==expected_stats
            assert construction.input_snapshot(manifest_paths)==manifest
            print(round_index+1,pair,name,result['wall_seconds'],flush=True)
    medians={name:{metric:statistics.median(row[metric] for row in round_state['samples'] if row['role']==name) for metric in ('wall_seconds','user_seconds','system_seconds','peak_rss_bytes')} for name in binaries}
    round_state.update(medians=medians, saved_wall_seconds=medians['baseline']['wall_seconds']-medians['candidate']['wall_seconds'])
    save(); print('SUMMARY',round_index+1,json.dumps(round_state['medians']),round_state['saved_wall_seconds'],flush=True)
assert all(run(binary,['--listFilesOnly'])['stdout'].splitlines()==paths for binary in binaries.values())
assert construction.input_snapshot(manifest_paths)==manifest
state['all_measured_pids_distinct']=len({row['pid'] for r in state['rounds'] for row in r['samples']})==20
assert state['all_measured_pids_distinct']
assert all(sha(path)==state['binary_sha256'][name] for name,path in binaries.items())
state['both_rounds_exceed_existing_20ms_gate']=all(r['saved_wall_seconds']>0.020 for r in state['rounds'])
state['complete']=True; save()
print('COMPLETE',state['both_rounds_exceed_existing_20ms_gate'],flush=True)
