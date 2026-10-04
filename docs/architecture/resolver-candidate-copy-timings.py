"""Two rotated fresh-process TSR comparisons; this is not a native speed ratio."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import statistics
import sys
sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('perf',ROOT/'scripts/whole_project_perf.py')
perf=importlib.util.module_from_spec(spec);spec.loader.exec_module(perf)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('baseline','candidate','baseline-scope','candidate-scope','controls','output'):
        p.add_argument('--'+name,type=Path,required=True)
    a=p.parse_args();out=a.output.resolve();assert not out.exists();out.mkdir(parents=True)
    controls=json.loads(a.controls.read_text());assert controls['complete']
    flags=controls['flags'];cwd=Path(flags[1]).parent;project=Path(flags[1])
    paths=controls['loaded_paths'];inputs=controls['input_fingerprint']
    expected=controls['runs'][0]['diagnostics']
    expected_checked=controls['runs'][1]['checked_paths']
    state={'kind':__doc__,'source':'143b2eedd47fd837775bc8904f864f9b4a444f51',
           'flags':flags+['--listFiles'],'input_fingerprint':inputs,
           'root_config_sha256':hashlib.sha256(project.read_bytes()).hexdigest(),
           'loaded':len(paths),'checked':len(expected_checked),'diagnostics':expected,
           'binary_sha256':{name:hashlib.sha256(getattr(a,name).read_bytes()).hexdigest()
                            for name in ('baseline','candidate')},
           'rounds':[],'scope_controls':[],'complete':False}
    def save():
        f=out/'results.json';f.write_text(json.dumps(state,indent=2)+'\n');assert json.loads(f.read_text())==state
    def run(binary,scope=False):
        saved=dict(os.environ);env={k:v for k,v in saved.items() if not k.startswith('TSR_')}
        env['TSR_LIB_PATH']=str(ROOT/'vendor/typescript-go/internal/bundled/libs')
        if scope:env['TSR_META_SCOPE']='1'
        try:
            os.environ.clear();os.environ.update(env)
            r=perf.process([str(binary.resolve()),*flags,'--listFiles'],cwd,90)
        finally:
            os.environ.clear();os.environ.update(saved)
        assert not r['timed_out'] and r['exit_code'] in (0,1,2)
        listed=[];ended=False
        for line in r['stdout'].splitlines():
            if perf.DIAGNOSTIC_START.match(line):ended=True;continue
            is_file=Path(line).is_absolute() and Path(line).is_file()
            if is_file:
                assert not ended,'unexpected appended file'
                listed.append(line)
            elif line:ended=True
        assert listed==paths
        assert perf.diagnostics(r['stdout'],cwd)==expected
        assert perf.input_fingerprint(paths)==inputs
        assert hashlib.sha256(project.read_bytes()).hexdigest()==state['root_config_sha256']
        return r
    def scope_controls(round_,position):
        for role,exe in (('baseline',a.baseline_scope),('candidate',a.candidate_scope)):
            r=run(exe,True)
            checked=[x.split('\t',1)[1] for x in r['stderr'].splitlines()
                     if x.startswith('TSR_META_CHECKED\t')]
            assert checked==expected_checked
            state['scope_controls'].append({'round':round_,'position':position,'role':role,
                'checked_order_fingerprint':perf.fingerprint(checked),
                'checked_input_fingerprint':perf.input_fingerprint(checked),
                'diagnostics_fingerprint':expected['fingerprint']})
            save()
    save()
    for number in (1,2):
        scope_controls(number,'before')
        round_={'number':number,'warmups':[],'samples':[]}
        state['rounds'].append(round_);save()
        for role in ('baseline','candidate'):
            r=run(getattr(a,role))
            round_['warmups'].append({k:r[k] for k in ('wall_seconds','user_seconds','system_seconds','peak_rss_bytes')})
        for pair in range(5):
            order=('baseline','candidate') if (pair+number)%2 else ('candidate','baseline')
            for role in order:
                r=run(getattr(a,role));assert not r['stderr']
                label=f'round{number}-pair{pair}-{role}'
                for stream in ('stdout','stderr'):(out/(label+'.'+stream)).write_text(r[stream])
                row={k:r[k] for k in ('wall_seconds','user_seconds','system_seconds','peak_rss_bytes','exit_code','timed_out')}
                row.update(role=role,pair=pair,loaded_order_fingerprint=perf.fingerprint(paths),
                           diagnostics_fingerprint=expected['fingerprint'],input_fingerprint=inputs)
                round_['samples'].append(row);save()
                print(json.dumps({'round':number,'pair':pair,'role':role,'wall':r['wall_seconds']}),flush=True)
        scope_controls(number,'after')
        round_['medians']={role:{key:statistics.median(r[key] for r in round_['samples'] if r['role']==role)
                                for key in ('wall_seconds','user_seconds','system_seconds','peak_rss_bytes')}
                          for role in ('baseline','candidate')}
        round_['wall_saved_seconds']=round_['medians']['baseline']['wall_seconds']-round_['medians']['candidate']['wall_seconds']
        print(json.dumps({'round':number,'wall_saved':round_['wall_saved_seconds']}),flush=True);save()
    assert all(hashlib.sha256(getattr(a,role).read_bytes()).hexdigest()==h for role,h in state['binary_sha256'].items())
    state['complete']=True
    state['confirmed_above_20ms']=all(r['wall_saved_seconds']>0.020 for r in state['rounds'])
    save()
if __name__=='__main__':main()
