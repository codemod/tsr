import argparse,importlib.util,json,os,statistics,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--repo',type=Path,required=True);p.add_argument('--preflight',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--cohorts',type=int,default=5);a=p.parse_args()
spec=importlib.util.spec_from_file_location('perf',a.repo/'scripts/whole_project_perf.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
pre=json.loads(a.preflight.read_text());project=Path(pre['project']);cwd=project.parent;bins={k:Path(v['binary']) for k,v in pre['tools'].items()}
paths=sorted({str(project),*(str(x) for x in bins.values()),*(x for v in pre['tools'].values() for x in v['loaded_files'] if not x.startswith('<typescript-lib>/'))})
ref=m.inputs.snapshot(paths);assert m.inputs.valid_snapshot(ref)
assert m.fingerprint(ref)==pre['input_reference']['fingerprint'],'preflight input bytes or metadata changed'
flags=['--project',str(project),'--noEmit','--incremental','false','--composite','false','--pretty','false']
r={'source_sha':m.revision(a.repo),'preflight_sha256':m.inputs.file_hash(a.preflight),'project':str(project),'flags':flags,'samples':[],'cohorts':[],'input_fingerprint':m.fingerprint(ref),'loaded_counts':{k:v['loaded_file_count'] for k,v in pre['tools'].items()},'performance_qualified':False,'reason':'Concurrent compiler load; complete cross-tool performed semantic work unverified.'}
def save():a.output.write_text(json.dumps(r,indent=2)+'\n')
save()
for n in range(a.cohorts+1):
 order=[('tsr',4),('tsgo',4),('tsgo',1),('tsr',1)] if n%2==0 else [('tsgo',1),('tsr',1),('tsr',4),('tsgo',4)]
 c={'number':n,'warmup':n==0,'load_average':os.getloadavg(),'order':order};r['cohorts'].append(c);save()
 for tool,count in order:
  assert m.inputs.snapshot(paths)==ref,'input changed before child'
  row=m.process([str(bins[tool]),*flags,'--checkers',str(count)],cwd,120)
  row['input_stable']=m.inputs.snapshot(paths)==ref;assert row['input_stable'];row.update(tool=tool,checkers=count,cohort=n,warmup=n==0,cpu_seconds=row['user_seconds']+row['system_seconds']);row['diagnostics']=m.diagnostics(row.pop('stdout'),cwd)
  assert not row['timed_out'] and row['exit_code'] in [0,1,2]
  expected=pre['tools'][tool]['samples'][0];assert row['diagnostics']==expected['diagnostics'] and row['exit_code']==expected['exit_code'] and row['stderr']==expected['stderr'],'full output changed'
  r['samples'].append(row);save();print(n,tool,count,round(row['wall_seconds'],3),round(row['cpu_seconds'],3),flush=True)
r['summaries']={}
for tool in bins:
 for count in [1,4]:
  rows=[x for x in r['samples'] if not x['warmup'] and x['tool']==tool and x['checkers']==count];s=m.summary(rows);s['cpu_seconds_median']=statistics.median(x['cpu_seconds'] for x in rows);r['summaries'][f'{tool}-{count}']=s
r['completed']=True;save()
print(json.dumps(r['summaries']))
