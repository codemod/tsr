"""Observe pinned completion workers without treating observer time as gain."""
from pathlib import Path
import argparse, hashlib, importlib.util, json, os, sys
sys.dont_write_bytecode=True
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[2])
parser.add_argument('--out',type=Path,required=True)
parser.add_argument('--clean',type=Path,required=True)
parser.add_argument('--probe',type=Path,required=True)
parser.add_argument('--tsr',type=Path,required=True)
parser.add_argument('--tsr-source',required=True)
parser.add_argument('--ledger',type=Path)
parser.add_argument('--fixtures',type=Path,default=Path(__file__).with_name('checker-native-member-completion-fixtures.py'))
args=parser.parse_args()
ROOT=args.root.resolve();OUT=args.out.resolve();OUT.mkdir(parents=True,exist_ok=True)
def load(name,path):
    s=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
perf=load('perf',ROOT/'scripts/whole_project_perf.py')
fixtures=load('member_fixtures',args.fixtures).fixtures()
bins={'clean':args.clean.resolve(),'probe':args.probe.resolve(),'tsr':args.tsr.resolve()}
env={k:v for k,v in os.environ.items() if not k.startswith(('TSR_','TSGO_'))}
env['TSR_LIB_PATH']=str(ROOT/'vendor/typescript-go/internal/bundled/libs')
initial_state={'source':args.tsr_source,'native_source':'5b1047d10d32e7d5b446be4de56b126ff42f82bb','binaries':{k:perf.inputs.file_hash(v) for k,v in bins.items()},'probe_identity':json.loads((OUT/'probe-identity.json').read_text()) if (OUT/'probe-identity.json').exists() else None,'cases':[],'complete':False,'native_target_verified':False,'scope':'public single-worker controls; counter enablement not a production speed experiment'}
state=json.loads((OUT/'results.json').read_text()) if (OUT/'results.json').exists() else initial_state
assert state['source']==initial_state['source'] and state['binaries']==initial_state['binaries']
def save():
    p=OUT/'results.json';p.write_text(json.dumps(state,indent=2)+'\n');assert json.loads(p.read_text())==state
    (OUT/'result.yaml').write_text(json.dumps(state,indent=2)+'\n');assert json.loads((OUT/'result.yaml').read_text())==state
    if args.ledger:
        p=args.ledger;x=json.loads(p.read_text());x['contract_observations']['native-member-completion-5b']['controls']=state;p.write_text(json.dumps(x,indent=2)+'\n');assert json.loads(p.read_text())['contract_observations']['native-member-completion-5b']['controls']==state
state['complete']=False;save()
for name,files in fixtures.items():
    existing=next((c for c in state['cases'] if c['name']==name),None)
    if existing:
        expected_sources={path:hashlib.sha256(source.encode()).hexdigest() for path,source in files.items()}
        expected_config=json.dumps({'compilerOptions':{'strict':True,'target':'es2020','types':[],'skipLibCheck':True,'noEmit':True,'incremental':False,'composite':False},'files':list(files)},indent=2)+'\n'
        assert existing['source_sha256']==expected_sources, 'Checkpoint fixtures changed; use a fresh output directory'
        assert existing['config_sha256']==hashlib.sha256(expected_config.encode()).hexdigest(), 'Checkpoint config changed'
        assert all(perf.inputs.file_hash((OUT/'controls'/name/path).resolve())==digest for path,digest in expected_sources.items())
        assert perf.inputs.file_hash((OUT/'controls'/name/'tsconfig.json').resolve())==existing['config_sha256']
    if existing and existing.get('complete'):continue
    if existing and len(existing['runs'])==5:
        clean,disabled,enabled,repeat,tsr=existing['runs']
        existing['observer_output_equal']=all(s['diagnostics']==clean['diagnostics'] and s['loaded']==clean['loaded'] for s in (disabled,enabled,repeat))
        existing['checked_equal']=enabled['checked']==repeat['checked']
        existing['tsr_process_valid']=not tsr['timed_out'] and tsr['exit_code'] in (0,1,2)
        existing['tsr_native_diagnostics_equal']=existing['tsr_process_valid'] and tsr['diagnostics']==clean['diagnostics']
        existing['complete']=True;save()
        assert existing['observer_output_equal'] and existing['checked_equal']
        continue
    assert existing is None, 'Partially measured fixture needs explicit per-role recovery'
    cwd=(OUT/'controls'/name).resolve();cwd.mkdir(parents=True)
    for filename,source in files.items():(cwd/filename).write_text(source)
    config=cwd/'tsconfig.json';config.write_text(json.dumps({'compilerOptions':{'strict':True,'target':'es2020','types':[],'skipLibCheck':True,'noEmit':True,'incremental':False,'composite':False},'files':list(files)},indent=2)+'\n')
    flags=['--project',str(config),'--noEmit','--incremental','false','--composite','false','--pretty','false','--singleThreaded','true','--listFiles']
    case={'name':name,'source_sha256':{p:perf.inputs.file_hash(cwd/p) for p in files},'config_sha256':perf.inputs.file_hash(config),'runs':[]};state['cases'].append(case);save()
    for role,kind,enabled in [('clean','clean',False),('disabled','probe',False),('enabled','probe',True),('repeat','probe',True),('tsr','tsr',False)]:
        saved=dict(os.environ);os.environ.clear();os.environ.update(env)
        if enabled:os.environ['TSGO_MEMBER_COMPLETION_PROBE']='1'
        before=perf.inputs.snapshot([str(config),*[str(cwd/p) for p in files],*map(str,bins.values())])
        try:r=perf.process([str(bins[kind]),*flags],cwd,90)
        finally:os.environ.clear();os.environ.update(saved)
        after=perf.inputs.snapshot([str(config),*[str(cwd/p) for p in files],*map(str,bins.values())])
        (cwd/(role+'.stdout')).write_text(r['stdout']);(cwd/(role+'.stderr')).write_text(r['stderr'])
        snapshots=[json.loads(line.removeprefix('MEMBER_COMPLETION ')) for line in r['stderr'].splitlines() if line.startswith('MEMBER_COMPLETION ')]
        row={k:v for k,v in r.items() if k not in ['stdout','stderr','command']};row.update(role=role,diagnostics=perf.diagnostics(r['stdout'],cwd),loaded=[perf.file_identity(s,cwd) for s in r['stdout'].splitlines() if s.startswith(('/','bundled:///'))],checked=[s['file'] for s in snapshots],snapshots=snapshots,input_before=perf.fingerprint(before),input_after=perf.fingerprint(after))
        case['runs'].append(row);save()
        assert before==after and perf.inputs.valid_snapshot(after)
        if role!='tsr':assert not r['timed_out'] and r['exit_code'] in (0,1,2)
        assert all(perf.inputs.file_hash(v)==state['binaries'][k] for k,v in bins.items())
        if enabled:assert snapshots
        else:assert not snapshots
    clean,disabled,enabled,repeat,tsr=case['runs']
    case['observer_output_equal']=all(s['diagnostics']==clean['diagnostics'] and s['loaded']==clean['loaded'] for s in (disabled,enabled,repeat))
    case['checked_equal']=enabled['checked']==repeat['checked']
    case['tsr_process_valid']=not tsr['timed_out'] and tsr['exit_code'] in (0,1,2)
    case['tsr_native_diagnostics_equal']=case['tsr_process_valid'] and tsr['diagnostics']==clean['diagnostics']
    case['complete']=True;save()
    assert case['observer_output_equal'] and case['checked_equal']
    print(json.dumps({'case':name,'observer_equal':True,'native_match':case['tsr_native_diagnostics_equal'],'checked':len(enabled['checked'])}),flush=True)
state['complete']=True;state['tsr_all_processes_valid']=all(not c['runs'][-1]['timed_out'] and c['runs'][-1]['exit_code'] in (0,1,2) for c in state['cases']);save()
