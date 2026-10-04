"""Same-source TSR controls; partial input coverage cannot prove native speed."""
from pathlib import Path
import argparse, importlib.util, json, os, statistics, sys
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('perf', ROOT/'scripts/whole_project_perf.py')
perf = importlib.util.module_from_spec(spec); spec.loader.exec_module(perf)
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--pairs',type=int,default=1)
p.add_argument('--rounds',type=int,default=1)
for name in ('baseline','candidate','project','input-manifest','candidate-patch'):
    p.add_argument('--'+name,type=Path,required=True)
for name in ('baseline-scope','candidate-scope'):
    p.add_argument('--'+name,type=Path)
p.add_argument('--source',required=True)
a = p.parse_args()
out=a.output.resolve(); out.mkdir(parents=True); assert a.pairs>0 and a.rounds>0
bins={role:getattr(a,role).resolve(strict=True) for role in ('baseline','candidate')}
assert bool(a.baseline_scope)==bool(a.candidate_scope)
a.scope=bool(a.baseline_scope)
scope_bins={role:getattr(a,role+'_scope').resolve(strict=True) for role in ('baseline','candidate')} if a.scope else {}
project=a.project.resolve(strict=True)
cwd=project.parent
flags=['--project',str(project),'--noEmit','--incremental','false','--composite','false','--pretty','false']
manifest=a.input_manifest.resolve(strict=True)
query,provenance=perf.inputs.load_manifest(manifest,cwd)
state={'kind':__doc__,'source':a.source,'candidate_patch_sha256':perf.inputs.file_hash(a.candidate_patch.resolve(strict=True)),'driver_sha256':perf.inputs.file_hash(Path(__file__)),'binary_sha256':{k:perf.inputs.file_hash(v) for k,v in bins.items()},'flags':flags,'scope_binary_sha256':{k:perf.inputs.file_hash(v) for k,v in scope_bins.items()},'manifest':provenance,'complete_cross_tool_inputs_verified':False,'native_target_verified':False,'runs':[],'rounds':[],'complete':False}
os.environ.update({'TSR_LIB_PATH':str(ROOT/'vendor/typescript-go/internal/bundled/libs'),'NO_COLOR':'1'})
for k in list(os.environ):
    if k.startswith('TSR_') and k!='TSR_LIB_PATH': del os.environ[k]
paths=sorted(set(query+[str(project),str(manifest),str(a.candidate_patch.resolve(strict=True)),*map(str,bins.values()),*map(str,scope_bins.values())]))
reference=perf.inputs.snapshot(paths)
expected=None; loaded=None
def save():
    f=out/'results.json'; f.write_text(json.dumps(state,indent=2)+'\n'); assert json.loads(f.read_text())==state
    (out/'result.yaml').write_text(json.dumps(state,indent=2)+'\n')
    assert json.loads((out/'result.yaml').read_text())==state
def run(role,label,extra,scope=False):
    before=perf.inputs.snapshot(paths); assert before==reference and perf.inputs.valid_snapshot(before)
    if scope: os.environ['TSR_META_SCOPE']='1'
    try: r=perf.process([str((scope_bins if scope else bins)[role]),*flags,*extra],cwd,90)
    finally: os.environ.pop('TSR_META_SCOPE',None)
    after=perf.inputs.snapshot(paths)
    (out/(label+'.stdout')).write_text(r['stdout']); (out/(label+'.stderr')).write_text(r['stderr'])
    row={k:v for k,v in r.items() if k not in ('stdout','stderr','command')}
    row.update(role=role,label=label,checked=[s.split('\t',1)[1] for s in r['stderr'].splitlines() if s.startswith('TSR_META_CHECKED\t')],diagnostics=perf.diagnostics(r['stdout'],cwd),loaded=[s for s in r['stdout'].splitlines() if s.startswith('/')],input_before=perf.fingerprint(before),input_after=perf.fingerprint(after))
    state['runs'].append(row); save()
    assert before==after and perf.inputs.valid_snapshot(after)
    assert not r['timed_out'] and r['exit_code'] in (0,1,2)
    assert all(perf.inputs.file_hash(v)==state['binary_sha256'][k] for k,v in bins.items())
    assert all(perf.inputs.file_hash(v)==state['scope_binary_sha256'][k] for k,v in scope_bins.items())
    return r,row
save()
for role in bins:
    r,row=run(role,'listing-'+role,['--listFilesOnly'])
    assert r['exit_code']==0 and row['loaded'] and all(Path(s).is_file() for s in row['loaded'])
    if loaded is None: loaded=row['loaded']
    assert row['loaded']==loaded
previous={v['path']:v for v in reference}
paths=sorted(set(paths+loaded));reference=perf.inputs.snapshot(paths)
assert perf.inputs.valid_snapshot(reference)
assert all(v==previous[v['path']] for v in reference if v['path'] in previous)
state.update(loaded_count=len(loaded),loaded_order_fingerprint=perf.fingerprint(loaded),input_count=len(reference),input_fingerprint=perf.fingerprint(reference),input_coverage='historical query manifest plus current loaded files/config/binaries; incomplete cross-tool queries')
configs=[]
for role in bins:
    r,row=run(role,'config-'+role,['--showConfig']); configs.append(json.loads(r['stdout']))
assert configs[0]==configs[1]; state['config_fingerprint']=perf.fingerprint(configs[0]); save()
checked=None
def scope_control(label):
    global checked
    if not a.scope:return
    for role in scope_bins:
        _,row=run(role,label+'-'+role,['--listFiles'],True)
        assert row['loaded']==loaded and row['diagnostics']==expected
        if checked is None:checked=row['checked']
        assert checked and row['checked']==checked
        state['independent_checked_order_fingerprint']=perf.fingerprint(checked)
        state['independent_checked_count']=len(checked)
        state['scope_provenance']='Caller supplies separate scope-only binaries; their source/probe provenance must be verified independently.'
        save()
for n in range(a.rounds):
    round_={'number':n+1,'samples':[]};state['rounds'].append(round_);save()
    for role in bins:
        _,row=run(role,f'round{n+1}-warmup-{role}',['--listFiles'])
        if expected is None:expected=row['diagnostics']
        assert row['diagnostics']==expected and row['loaded']==loaded
    scope_control(f'round{n+1}-scope-before')
    for pair in range(a.pairs):
        order=('baseline','candidate') if (pair+n)%2==0 else ('candidate','baseline')
        for role in order:
            _,row=run(role,f'round{n+1}-pair{pair}-{role}',['--listFiles'])
            round_['samples'].append(row);save()
            assert row['diagnostics']==expected and row['loaded']==loaded
            print(json.dumps({'round':n+1,'pair':pair,'role':role,'wall':row['wall_seconds']}),flush=True)
    scope_control(f'round{n+1}-scope-after')
    round_['medians']={role:{k:statistics.median(v[k] for v in round_['samples'] if v['role']==role) for k in ('wall_seconds','user_seconds','system_seconds','peak_rss_bytes')} for role in bins}
    round_['saved_wall_seconds']=round_['medians']['baseline']['wall_seconds']-round_['medians']['candidate']['wall_seconds'];save()
    print(json.dumps({'round':n+1,'saved_wall_seconds':round_['saved_wall_seconds']}),flush=True)
state['all_scored_pids_distinct']=len({v['pid'] for r in state['rounds'] for v in r['samples']})==a.pairs*a.rounds*2
state['diagnostics']={k:v for k,v in expected.items() if k!='entries'}
state['complete']=True;save()
