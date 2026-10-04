"""Physical native controls for resolver construction attribution."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    'public', Path(__file__).with_name('metadata-origin-public-controls.py'))
public = importlib.util.module_from_spec(spec)
spec.loader.exec_module(public)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('normal', 'scope', 'probe', 'tsgo', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    assert not out.exists()
    out.mkdir(parents=True)
    fixtures = public.fixtures()
    fixtures.update({
        'suffix-priority': {
            'main.ts': 'import { value } from "./dep.js"; const wrong: number = value;\n',
            'dep.native.ts': 'export declare const value: string;\n',
            'dep.ts': 'export declare const value: number;\n',
        },
        'empty-suffix-fallback': {
            'main.ts': 'import { value } from "./dep.js"; const wrong: number = value;\n',
            'dep.ts': 'export declare const value: string;\n',
        },
        'dot-directory-and-failed-relative': {
            'main.ts': 'import { value } from "./sub/."; import { missing } from "./absent.js";\n'
                       'const wrong: number = value; export const held = missing;\n',
            'sub/index.ts': 'export declare const value: string;\n',
        },
        'trace-forwarding': {
            'main.ts': 'import { value } from "./dep.js"; const wrong: number = value;\n',
            'dep.ts': 'export declare const value: string;\n',
        },
    })
    state = {'kind': __doc__, 'cases': [], 'complete': False,
             'native_binary_sha256': hashlib.sha256(args.tsgo.read_bytes()).hexdigest(),
             'harness_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
             'native_pin': '5b1047d10d32e7d5b446be4de56b126ff42f82bb'}

    def save():
        p = out / 'results.json'
        p.write_text(json.dumps(state, indent=2) + '\n')
        assert json.loads(p.read_text()) == state

    save()
    for name, files in fixtures.items():
        cwd = out / name
        cwd.mkdir()
        for relative, text in files.items():
            p = cwd / relative
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
        if name == 'symlink-package':
            (cwd / 'node_modules').mkdir()
            (cwd / 'node_modules/linked').symlink_to('../store/linked', target_is_directory=True)
        root = 'src/main.ts' if name == 'package-exports-imports' else 'main.ts'
        options = {'strict': True, 'target': 'es2020', 'module': 'esnext',
                   'moduleResolution': 'bundler', 'types': [], 'skipLibCheck': True,
                   'noEmit': True, 'incremental': False, 'composite': False}
        if name in ('suffix-priority', 'empty-suffix-fallback'):
            options['moduleSuffixes'] = ['.native', '']
        if name == 'trace-forwarding':
            options['traceResolution'] = True
        config = cwd / 'tsconfig.json'
        config.write_text(json.dumps({'compilerOptions': options, 'files': [root]}, indent=2) + '\n')
        command = [sys.executable, str(ROOT / 'docs/architecture/resolver-construction-controls.py')]
        for role in ('normal', 'scope', 'probe'):
            command += ['--' + role, str(getattr(args, role).resolve())]
        command += ['--project', str(config), '--output', str(cwd / 'controls')]
        subprocess.run(command, check=True)
        actual = json.loads((cwd / 'controls/results.json').read_text())
        observed = actual['scope_control']['runs'][0]['diagnostics']
        row = {'name': name, 'files': files, 'native': [],
               'loaded': len(actual['scope_control']['loaded_paths']),
               'checked': len(actual['scope_control']['runs'][1]['checked_paths']),
               'complete_tsr_control_match': True,
               'constructor_counts': actual['runs'][0]['by_kind'],
               'config_sha256': hashlib.sha256(config.read_bytes()).hexdigest()}
        state['cases'].append(row)
        save()
        for mode in ('default', 'single'):
            saved = dict(os.environ)
            try:
                os.environ.clear()
                os.environ.update({k: v for k, v in saved.items() if not k.startswith('TSR_')})
                native = public.perf.process([
                    str(args.tsgo.resolve()), '--project', str(config), '--noEmit',
                    '--incremental', 'false', '--composite', 'false', '--pretty', 'false',
                    *(['--singleThreaded'] if mode == 'single' else [])], cwd, 60)
            finally:
                os.environ.clear()
                os.environ.update(saved)
            for stream in ('stdout', 'stderr'):
                (cwd / (mode + '.' + stream)).write_text(native[stream])
            assert not native['timed_out'] and native['exit_code'] in (0, 1, 2)
            expected = public.perf.diagnostics(native['stdout'], cwd)
            match = expected == observed
            native_row = {'mode': mode, 'complete_diagnostics_match': match,
                          'native_diagnostic_count': expected['count'],
                          'native_diagnostic_fingerprint': expected['fingerprint'],
                          'tsr_diagnostic_count': observed['count'],
                          'tsr_diagnostic_fingerprint': observed['fingerprint']}
            if name == 'trace-forwarding':
                tsr_stdout = (cwd / 'controls/scope-controls/normal.stdout').read_text()
                native_row['native_resolution_trace_present'] = 'Resolving module' in native['stdout']
                native_row['tsr_resolution_trace_present'] = 'Resolving module' in tsr_stdout
                assert native_row['native_resolution_trace_present']
                assert not native_row['tsr_resolution_trace_present']
                native_row['tracked_trace_failure'] = 'tsr-1yb.1.1.1'
            assert match == (name != 'empty-suffix-fallback'), (name, mode, expected, observed)
            if not match:
                native_row['tracked_native_failure'] = 'tsr-6.59'
            row['native'].append(native_row)
            save()
        assert hashlib.sha256(config.read_bytes()).hexdigest() == row['config_sha256']
        print(json.dumps({'case': name, 'native_matches': [r['complete_diagnostics_match'] for r in row['native']]}), flush=True)
    assert hashlib.sha256(args.tsgo.read_bytes()).hexdigest() == state['native_binary_sha256']
    state['complete'] = True
    save()


if __name__ == '__main__':
    main()
