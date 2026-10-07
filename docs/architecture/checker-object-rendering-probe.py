import argparse,hashlib,json,os,sys,subprocess
from collections import Counter
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
from whole_project_perf import process
from benchmark_inputs import snapshot
from checker_work_trace import validate_trace,validate_worker_activity
parser=argparse.ArgumentParser(description="Qualify the archived object allocator probe; never certify speed.")
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--project',type=Path,required=True)
parser.add_argument('--probe',type=Path,required=True)
parser.add_argument('--ordinary',type=Path,required=True)
options=parser.parse_args()
OUT=options.output.resolve();OUT.mkdir(parents=True,exist_ok=True)
PROJECT=options.project.resolve()
PROBE=options.probe.resolve()
ORDINARY=options.ordinary.resolve()
TRACE_KEYS=['crates/tsr-execute/src/work_trace.rs','crates/tsr-execute/src/checker_pool.rs','crates/tsr-execute/src/compile.rs','scripts/checker_work_trace.py']
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def child(binary,args,role,trace=None):
    env={key:os.environ.get(key) for key in ('TSR_OBJECT_ORIGIN_PROBE','TSR_WORK_TRACE')}
    try:
        for key in env:os.environ.pop(key,None)
        if role in ('on','repeat'):os.environ['TSR_OBJECT_ORIGIN_PROBE']='1'
        if trace is not None:os.environ['TSR_WORK_TRACE']=str(trace)
        result=process([str(binary),*args],PROJECT,300)
        (OUT/f'{mode}-{role}-child.json').write_text(json.dumps(result,indent=2))
        assert not result['timed_out'] and result['exit_code'] in (0,1,2),result
        return result
    finally:
        for key,value in env.items():
            os.environ.pop(key,None)
            if value is not None:os.environ[key]=value
before=snapshot([str(p) for p in PROJECT.rglob('*') if p.is_file()]+[str(p) for p in (ROOT/'vendor/typescript-go/internal/bundled/libs').iterdir() if p.is_file()])
hashes={str(p):digest(p) for p in (PROBE,ORDINARY)}
source_hashes={str(ROOT/p):digest(ROOT/p) for p in TRACE_KEYS}
all_results=[]
for mode,flags in [('default',[]),('single',['--singleThreaded'])]:
    args=['--project','tsconfig.json','--pretty','false',*flags]
    config=child(PROBE,[*args,'--showConfig'],'config')
    listing=child(PROBE,[*args,'--listFilesOnly'],'listing')
    assert config['exit_code']==0 and listing['exit_code']==0
    children=[]; streams=[]; qualifications=[]; selected=[]
    for role in ('ordinary','off','on','repeat'):
        trace=OUT/f'{mode}-{role}.ndjson' if role in ('on','repeat') else None
        result=child(ORDINARY if role=='ordinary' else PROBE,args,role,trace)
        clean=''.join(line for line in result['stderr'].splitlines(keepends=True) if not line.startswith(('ORIGIN\t','ORIGIN_TOTAL\t','OBJECT_PAYLOAD\t','OBJECT_EVENT\t')))
        children.append((result['exit_code'],result['stdout'],clean))
        if trace is None:continue
        rows=[json.loads(line) for line in trace.read_text().splitlines()];streams.append(rows)
        receipt={'schema_version':1,'current_directory':str(PROJECT),'child':result,'invocation_id':rows[0]['invocation_id'],'source_sha':None,'binary_sha256':hashes[str(PROBE)],'source_files_sha256':source_hashes,'inputs_before':before,'inputs_after':snapshot([row['path'] for row in before]),'show_config':json.loads(config['stdout']),'loaded_files':listing['stdout'].splitlines(),'requested_checkers':None,'requested_single_threaded':True if flags else None,'trace_sha256':digest(trace)}
        integrity=validate_trace(trace,receipt);activity=validate_worker_activity(trace,receipt,'tsr')
        (OUT/f'{mode}-{role}-receipt.json').write_text(json.dumps(receipt,indent=2))
        assert integrity['artifact_integrity_valid'],integrity
        assert activity['worker_activity_valid'],activity
        qualifications.append({'integrity':integrity,'activity':activity})
        origins=[line.split('\t') for line in result['stderr'].splitlines() if line.startswith('ORIGIN\t')]
        payload=[line.split('\t') for line in result['stderr'].splitlines() if line.startswith('OBJECT_PAYLOAD\t')]
        selected.append({'origins':[(r[1:7]) for r in origins if int(r[1])>1],'payload':payload,'events':[line.split('\t') for line in result['stderr'].splitlines() if line.startswith('OBJECT_EVENT\t')]})
    assert len(set(children))==1,'complete outputs changed'
    def work(rows):return Counter((r['checker_id'],r['operation'],tuple(r['file_ids']),tuple(r['unmapped_source_node_ids'])) for r in rows if r['event']=='work_begin')
    assert work(streams[0])==work(streams[1]),'owner work changed'
    assert selected[0]==selected[1],'selected traffic/payload changed'
    all_results.append({'mode':mode,'qualifications':qualifications,'selected':selected[0],'complete_outputs_match':True,'owner_work_multisets_repeat':True})
    (OUT/'controls.json').write_text(json.dumps({'results':all_results,'binary_sha256':hashes,'source_files_sha256':source_hashes,'target_verified':False,'native_work_equivalence_verified':False,'full_corpus_verified':False,'resources':'Raw unpaired observations only; ordinary debug and probe release are not throughput-comparable.'},indent=2))
    print(mode,'passed',flush=True)
assert before==snapshot([row['path'] for row in before]),'inputs changed'
assert hashes=={path:digest(Path(path)) for path in hashes},'binaries changed'
