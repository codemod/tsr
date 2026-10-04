"""Source-exact resolver copy/scope controls; no speed claim from these roles."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('perf', ROOT / 'scripts/whole_project_perf.py')
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)
SITES = ['original-extension', 'extension-stem', 'candidate-directory', 'module-suffix-payload',
         'suffix-stem', 'failed-result-path', 'successful-result-path']

def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('normal','probe','project','output'):
        p.add_argument('--'+name, type=Path, required=True)
    p.add_argument('--candidate', type=Path)
    p.add_argument('--candidate-scope', type=Path)
    a = p.parse_args()
    out = a.output.resolve()
    assert not out.exists()
    out.mkdir(parents=True)
    project = a.project.resolve()
    cwd = project.parent
    flags = ['--project', str(project), '--noEmit', '--incremental', 'false',
             '--composite','false','--pretty','false']
    state = {'kind':'resolver candidate-copy controls; no speed claim',
             'flags':flags, 'sites':SITES, 'config_sha256':hashlib.sha256(project.read_bytes()).hexdigest(),
             'runs':[], 'config_controls':[]}
    def save():
        f = out/'results.json'
        f.write_text(json.dumps(state,indent=2)+'\n')
        assert json.loads(f.read_text()) == state
    def run(binary, commands, profile=False):
        saved = dict(os.environ)
        env = {k:v for k,v in saved.items() if not k.startswith('TSR_')}
        env['TSR_LIB_PATH'] = str(ROOT/'vendor/typescript-go/internal/bundled/libs')
        env['TSR_META_SCOPE']='1'
        if profile: env['TSR_RESOLVER_COPY']='1'
        try:
            os.environ.clear(); os.environ.update(env)
            r = perf.process([str(binary.resolve()), *commands], cwd, 90)
        finally:
            os.environ.clear(); os.environ.update(saved)
        assert not r['timed_out'] and r['exit_code'] in (0,1,2)
        return r
    save()
    listing = run(a.normal, flags+['--listFilesOnly'])
    paths = listing['stdout'].splitlines()
    assert paths and all(Path(f).is_absolute() and Path(f).is_file() for f in paths)
    state.update(loaded_paths=paths, input_fingerprint=perf.input_fingerprint(paths),
                 loaded_order_fingerprint=perf.fingerprint(paths))
    roles = [('normal',a.normal,False),('probe-off',a.probe,False),('probe-on',a.probe,True),
             ('probe-repeat',a.probe,True)]
    if a.candidate:
        assert a.candidate_scope
        roles += [('candidate',a.candidate,False),('candidate-scope',a.candidate_scope,False)]
    expected_diagnostics = None
    expected_checked = None
    for role,binary,profile in roles:
        config = run(binary,flags+['--showConfig'])
        effective=json.loads(config['stdout'])
        state['config_controls'].append({'role':role,'fingerprint':perf.fingerprint(effective)})
        if role=='normal':state['effective_config']=effective
        else:assert effective==state['effective_config']
        assert perf.input_fingerprint(paths)==state['input_fingerprint']
        r=run(binary,flags+['--listFiles'],profile)
        for stream in ('stdout','stderr'):(out/(role+'.'+stream)).write_text(r[stream])
        observed=[]; ended=False
        for line in r['stdout'].splitlines():
            if perf.DIAGNOSTIC_START.match(line):ended=True; continue
            is_file=Path(line).is_absolute() and Path(line).is_file()
            if is_file:
                assert not ended,'unexpected appended loaded file'
                observed.append(line)
            elif line:ended=True
        assert observed==paths
        checked=[line.split('\t',1)[1] for line in r['stderr'].splitlines()
                 if line.startswith('TSR_META_CHECKED\t')]
        counts=[json.loads(line.split('\t',1)[1]) for line in r['stderr'].splitlines()
                if line.startswith('TSR_RESOLVER_COPY\t')]
        assert bool(counts)==profile
        aggregate={k:[sum(x[k][i] for x in counts) for i in range(7)]
                   for k in ('calls','bytes','nanos')} if counts else None
        diagnostics=perf.diagnostics(r['stdout'],cwd)
        row={k:r[k] for k in ('wall_seconds','user_seconds','system_seconds','peak_rss_bytes',
                              'exit_code','timed_out')}
        row.update(role=role,binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                   diagnostics=diagnostics,checked_paths=checked,counts=aggregate,
                   checked_order_fingerprint=perf.fingerprint(checked) if checked else None,
                   checked_input_fingerprint=perf.input_fingerprint(checked) if checked else None,
                   loaded_order_fingerprint=perf.fingerprint(observed))
        state['runs'].append(row); save()
        if expected_diagnostics is None:expected_diagnostics=diagnostics
        assert diagnostics==expected_diagnostics
        if role not in ('normal','candidate'):
            assert checked
            if expected_checked is None:expected_checked=checked
            assert checked==expected_checked
        assert perf.input_fingerprint(paths)==state['input_fingerprint']
        print(json.dumps({'role':role,'loaded':len(paths),'checked':len(checked),
                          'diagnostics':diagnostics['count'],'counts':aggregate}),flush=True)
    enabled=[r['counts'] for r in state['runs'] if r['counts']]
    assert len(enabled)==2 and enabled[0]['calls']==enabled[1]['calls'] and enabled[0]['bytes']==enabled[1]['bytes']
    assert hashlib.sha256(project.read_bytes()).hexdigest()==state['config_sha256']
    state['complete']=True;save()
if __name__=='__main__':main()
