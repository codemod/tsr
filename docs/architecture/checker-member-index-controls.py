from __future__ import annotations
import argparse,importlib.util,json,os,sys
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[2]
def load(name,path):
 s=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
controls=load('member_controls',ROOT/'docs/architecture/checker-member-cost-controls.py')
perf=controls.perf
parser=argparse.ArgumentParser(description='Public controls for the ordered member-name experiment; no native speed claim')
for role in ('baseline','candidate','native','output'):
 parser.add_argument('--'+role,type=Path,required=True)
parser.add_argument('--source',required=True,help='Exact base revision for the supplied baseline/candidate')
a=parser.parse_args()
OUT=a.output.resolve();OUT.mkdir(parents=True,exist_ok=False)
bins={role:getattr(a,role).resolve(strict=True) for role in ('baseline','candidate','native')}
state={'source':a.source,'binaries':{k:perf.inputs.file_hash(v) for k,v in bins.items()},'cases':[],'complete':False,'target_verified':False}
def save():
 p=OUT/'results.json';p.write_text(json.dumps(state,indent=2)+'\n');assert json.loads(p.read_text())==state
os.environ.clear();os.environ.update({'PATH':'/usr/bin:/bin','NO_COLOR':'1','TSR_LIB_PATH':str(ROOT/'vendor/typescript-go/internal/bundled/libs')})
cases=controls.fixtures()
for width in (16,32,33,256):
 for degree in (1,4):
  fields='; '.join(f'p{i}:number' for i in range(width))
  bases=''.join(f'interface B{i} extends Root {{ e{i}:number }}\n' for i in range(degree))
  shape='; '.join([fields,*[f'e{i}:number' for i in range(degree)]])
  cases[f'wide-{width}-shared-{degree}']=f'interface Root {{ {fields} }}\n'+bases+f'interface Target extends '+','.join(f'B{i}' for i in range(degree))+f' {{ own:number }}\ndeclare const source: {{ {shape}; own:number; extra:string }};\n'+''.join(f'const x{i}:Target=source;\n' for i in range(8))
for name,source in cases.items():
 d=OUT/name;d.mkdir(exist_ok=True);(d/'input.ts').write_text(source)
 config=d/'tsconfig.json';config.write_text(json.dumps({'compilerOptions':{'strict':True,'target':'es2020','types':[],'skipLibCheck':True,'noEmit':True,'incremental':False,'composite':False},'files':['input.ts']},indent=2)+'\n')
 case={'name':name,'input_sha256':perf.inputs.file_hash(d/'input.ts'),'config_sha256':perf.inputs.file_hash(config),'runs':[]};state['cases'].append(case);save()
 for role,binary in bins.items():
  result=perf.process([str(binary),'--project',str(config),'--noEmit','--incremental','false','--composite','false','--pretty','false','--listFiles'],d,90)
  (d/(role+'.stdout')).write_text(result['stdout']);(d/(role+'.stderr')).write_text(result['stderr'])
  row={k:v for k,v in result.items() if k not in ['stdout','stderr','command']};row.update(role=role,diagnostics=perf.diagnostics(result['stdout'],d),loaded=[perf.file_identity(s,d) for s in result['stdout'].splitlines() if s.startswith('/') or s.startswith('bundled:///')]);case['runs'].append(row);save()
  assert not row['timed_out'] and row['exit_code'] in (0,1,2)
 a,b,n=case['runs'];case['candidate_equal']=a['diagnostics']==b['diagnostics'] and a['loaded']==b['loaded'];case['native_diagnostics_equal']=a['diagnostics']==n['diagnostics'];save()
 assert case['candidate_equal'],name
 if name.startswith('wide-'):assert case['native_diagnostics_equal'] and not n['diagnostics']['count'],name
 print(json.dumps({'case':name,'candidate_equal':case['candidate_equal'],'native_diagnostics_equal':case['native_diagnostics_equal']}),flush=True)
state['complete']=True;save()
