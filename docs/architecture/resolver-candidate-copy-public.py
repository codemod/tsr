"""Physical suffix/extension/failure controls for the candidate-copy experiment."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[2]
sp=importlib.util.spec_from_file_location('perf',ROOT/'scripts/whole_project_perf.py');perf=importlib.util.module_from_spec(sp);sp.loader.exec_module(perf)
CASES={
 'suffix-priority':({'main.ts':'import {value} from "./dep";\nconst bad: number = value;\n',
                    'dep.native.ts':'export declare const value: string;\n',
                    'dep.ts':'export declare const value: number;\n'},['.native',''],'dep.native.ts'),
 'suffix-fallback':({'main.ts':'import {value} from "./dep";\nconst bad: number = value;\n',
                    'dep.ts':'export declare const value: string;\n'},['.native',''],'dep.ts'),
 'arbitrary-extension':({'main.ts':'import {value} from "./style.css";\nconst bad: number = value;\n',
                        'style.d.css.ts':'export declare const value: string;\n'},[],'style.d.css.ts'),
 'failed-relative':({'main.ts':'import {value} from "./absent";\nexport const held = value;\n'},['.native',''],None),
}
def main():
 p=argparse.ArgumentParser(description=__doc__)
 for name in ('normal','probe','candidate','candidate-scope','tsgo','output'):
  p.add_argument('--'+name,type=Path,required=True)
 a=p.parse_args()
 out=a.output.resolve();assert not out.exists();out.mkdir()
 rows=[]
 for name,(files,suffixes,chosen) in CASES.items():
  cwd=out/name;cwd.mkdir()
  for file,text in files.items():(cwd/file).write_text(text)
  config=cwd/'tsconfig.json';config.write_text(json.dumps({'compilerOptions':{
   'strict':True,'allowArbitraryExtensions':True,'target':'es2020','module':'esnext','moduleResolution':'bundler','types':[],
   'skipLibCheck':True,'moduleSuffixes':suffixes,'noEmit':True,'incremental':False,'composite':False},
   'files':['main.ts']},indent=2)+'\n')
  command=[sys.executable,str(ROOT/'docs/architecture/resolver-candidate-copy-controls.py'),'--normal',str(a.normal.resolve()),
   '--probe',str(a.probe.resolve()),'--candidate',str(a.candidate.resolve()),
   '--candidate-scope',str(a.candidate_scope.resolve()),'--project',str(config),'--output',str(cwd/'roles')]
  subprocess.run(command,check=True)
  result=json.loads((cwd/'roles/results.json').read_text());assert result['complete']
  loaded=[Path(p).name for p in result['loaded_paths'] if p.startswith(str(cwd)+'/')]
  if name=='suffix-priority':assert 'dep.ts' not in loaded
  if name=='failed-relative':assert loaded==['main.ts']
  native=[]
  env={k:v for k,v in os.environ.items() if not k.startswith('TSR_')};env.update(NO_COLOR='1')
  for role,flags in (('native-one',['--singleThreaded']),('native-default',[])):
   r=subprocess.run([str(a.tsgo.resolve()),'--project',str(config),'--pretty','false','--listFiles',*flags],cwd=cwd,env=env,capture_output=True,text=True,timeout=60);assert r.returncode in (0,1,2) and not r.stderr
   (cwd/(role+'.stdout')).write_text(r.stdout)
   diag=perf.diagnostics(r.stdout,cwd)
   observed=[p[len(str(cwd))+1:] for p in r.stdout.splitlines() if p.startswith(str(cwd)+'/') and Path(p).is_file()]
   if chosen:assert chosen in observed
   native_match=(observed==loaded and diag==result['runs'][0]['diagnostics'])
   native.append({'role':role,'diagnostics':diag,'loaded_project':observed,'stdout_sha256':hashlib.sha256(r.stdout.encode()).hexdigest(),'native_match':native_match})
  counts=result['runs'][2]['counts']
  if suffixes:assert counts['calls'][4]>0 and counts['bytes'][3]>0
  rows.append({'case':name,'files':files,'module_suffixes':suffixes,'chosen':chosen,'config_sha256':hashlib.sha256(config.read_bytes()).hexdigest(),'loaded_project':loaded,'native':native,'matched_all_tsr_roles':True,'matched_native':all(r['native_match'] for r in native),'probe_counts':counts})
  print(json.dumps({'case':name,'native_match':all(r['native_match'] for r in native)}),flush=True)
  (out/'results.json').write_text(json.dumps({'cases':rows,'complete':False},indent=2)+'\n')
 d={'cases':rows,'complete':True,'all_match':all(c['matched_native'] for c in rows)};(out/'results.json').write_text(json.dumps(d,indent=2)+'\n');assert json.loads((out/'results.json').read_text())==d
 return 0 if d['all_match'] else 1
if __name__=='__main__':raise SystemExit(main())
