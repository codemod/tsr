"""Source-qualified resolver construction locator; no speed acceptance."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import sys

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    'metadata', Path(__file__).with_name('metadata-origin-controls.py'))
metadata = importlib.util.module_from_spec(spec)
spec.loader.exec_module(metadata)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def input_snapshot(paths):
    """Local-only file bytes/kinds/realpaths; no directory-content claim."""
    rows = []
    for name in sorted(set(paths)):
        path = Path(name)
        row = {'path': name, 'realpath': os.path.realpath(name)}
        try:
            mode = path.stat().st_mode
        except (FileNotFoundError, NotADirectoryError):
            row['kind'] = 'missing'
        else:
            if stat.S_ISREG(mode):
                row.update(kind='file', sha256=sha(path))
            elif stat.S_ISDIR(mode):
                row['kind'] = 'directory'
            else:
                row['kind'] = 'other'
        if path.is_symlink():
            row['symlink_target'] = os.readlink(path)
        rows.append(row)
    return rows


def loaded_paths(stdout):
    paths = []
    ended = False
    for line in stdout.splitlines():
        if Path(line).is_absolute() and Path(line).is_file():
            assert not ended, 'unexpected file appended after diagnostics'
            paths.append(line)
        elif line:
            ended = True
    return paths


def scope_controls(args, out):
    """Reuse shared parsing, with private queried-input checks per process."""
    perf = metadata.perf
    out.mkdir()
    config = args.project.resolve()
    binaries = {'normal': args.normal.resolve(), 'scope': args.scope.resolve(),
                'disabled': args.probe.resolve(), 'enabled': args.probe.resolve(),
                'repeat': args.probe.resolve()}
    flags = ['--project', str(config), '--noEmit', '--incremental', 'false',
             '--composite', 'false', '--pretty', 'false']
    state = {'binary_sha256': {role: sha(binary) for role, binary in binaries.items()},
             'config_sha256_before': sha(config), 'flags': flags,
             'runs': [], 'config_controls': [], 'complete': False,
             'queried_input_coverage': 'loaded sources, root config, observed file/directory metadata queries; file bytes/kinds/realpaths',
             'complete_input_equivalence_verified': False,
             'input_limit': 'No directory-entry, unobserved read, environment or transient-between-snapshot proof. Fingerprinting runs outside process timing and warms OS caches.'}

    def save():
        path = out / 'results.json'
        path.write_text(json.dumps(state, indent=2) + '\n')
        assert json.loads(path.read_text()) == state

    def run(binary, command, profile=False):
        saved = dict(os.environ)
        env = {k: v for k, v in saved.items() if not k.startswith('TSR_')}
        env['TSR_LIB_PATH'] = str(ROOT / 'vendor/typescript-go/internal/bundled/libs')
        env['TSR_META_SCOPE'] = '1'
        if profile:
            env.update(TSR_META_PROFILE='1', TSR_META_INPUT_PATHS='1')
        try:
            os.environ.clear()
            os.environ.update(env)
            result = perf.process([str(binary)] + command, config.parent, 90)
        finally:
            os.environ.clear()
            os.environ.update(saved)
        assert not result['timed_out'] and result['exit_code'] in (0, 1, 2)
        return result

    def queries(result):
        return [line.split('\t', 1)[1] for line in result['stderr'].splitlines()
                if line.startswith('TSR_META_INPUT\t')]

    save()
    listing = run(binaries['normal'], flags + ['--listFilesOnly'])
    paths = listing['stdout'].splitlines()
    assert paths and all(Path(p).is_absolute() and Path(p).is_file() for p in paths)
    preflight = run(binaries['enabled'], flags + ['--listFiles'], profile=True)
    for stream in ('stdout', 'stderr'):
        (out / ('preflight.' + stream)).write_text(preflight[stream])
    query_paths = queries(preflight)
    assert query_paths and query_paths == sorted(set(query_paths))
    assert all(Path(p).is_absolute() for p in query_paths)
    assert loaded_paths(preflight['stdout']) == paths
    manifest = input_snapshot([*paths, str(config), *query_paths])
    manifest_paths = [row['path'] for row in manifest]
    (out / 'private-input-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    assert json.loads((out / 'private-input-manifest.json').read_text()) == manifest
    input_hash = perf.fingerprint(manifest)
    state.update(loaded_paths=paths, loaded_order_fingerprint=perf.fingerprint(paths),
                 input_fingerprint_before=perf.input_fingerprint(paths),
                 queried_input_fingerprint=input_hash, queried_input_count=len(manifest),
                 queried_input_kind_counts={kind: sum(row['kind'] == kind for row in manifest)
                                           for kind in ('file', 'directory', 'missing', 'other')})
    expected_diagnostics = perf.diagnostics(preflight['stdout'], config.parent)
    expected_checked = [line.split('\t', 1)[1] for line in preflight['stderr'].splitlines()
                        if line.startswith('TSR_META_CHECKED\t')]
    assert expected_checked
    save()
    for role, binary in binaries.items():
        assert input_snapshot(manifest_paths) == manifest, 'inputs changed before config'
        configuration = run(binary, flags + ['--showConfig'])
        assert input_snapshot(manifest_paths) == manifest, 'inputs changed during config'
        effective = json.loads(configuration['stdout'])
        state['config_controls'].append({'role': role, 'fingerprint': perf.fingerprint(effective)})
        if role == 'normal':
            state['effective_config'] = effective
        else:
            assert effective == state['effective_config']
        assert sha(binary) == state['binary_sha256'][role]
        result = run(binary, flags + ['--listFiles'], profile=role in ('enabled', 'repeat'))
        for stream in ('stdout', 'stderr'):
            (out / (role + '.' + stream)).write_text(result[stream])
        observed_paths = loaded_paths(result['stdout'])
        assert observed_paths == paths
        checked = [line.split('\t', 1)[1] for line in result['stderr'].splitlines()
                   if line.startswith('TSR_META_CHECKED\t')]
        counters = [json.loads(line.split('\t', 1)[1]) for line in result['stderr'].splitlines()
                    if line.startswith('TSR_META_COUNTS\t')]
        assert len(counters) == int(role in ('enabled', 'repeat'))
        observed_queries = queries(result)
        assert observed_queries == (query_paths if counters else [])
        after = input_snapshot(manifest_paths)
        diagnostics = perf.diagnostics(result['stdout'], config.parent)
        row = {k: result[k] for k in ('wall_seconds', 'user_seconds', 'system_seconds',
                                    'peak_rss_bytes', 'exit_code', 'timed_out', 'pid')}
        row.update(role=role, diagnostics=diagnostics, checked_paths=checked,
                   loaded_order_fingerprint=perf.fingerprint(observed_paths),
                   checked_order_fingerprint=perf.fingerprint(checked) if checked else None,
                   checked_input_fingerprint=perf.input_fingerprint(checked) if checked else None,
                   counts=counters[0] if counters else None,
                   queried_input_fingerprint_after=perf.fingerprint(after),
                   input_fingerprint_after=perf.input_fingerprint(paths))
        state['runs'].append(row)
        save()
        assert after == manifest, 'queried inputs changed during full check'
        assert diagnostics == expected_diagnostics
        assert checked == (expected_checked if role != 'normal' else [])
        assert row['input_fingerprint_after'] == state['input_fingerprint_before']
        print(json.dumps({'role': role, 'loaded': len(paths), 'checked': len(checked),
                          'diagnostics': diagnostics['count']}), flush=True)
    def stable(counts):
        return {'rows': [{k: v for k, v in row.items() if k != 'stat_ns'}
                         for row in counts['rows']], 'global_unique_paths': counts['global_unique_paths']}
    assert stable(state['runs'][-2]['counts']) == stable(state['runs'][-1]['counts'])
    state['config_sha256_after'] = sha(config)
    assert state['config_sha256_after'] == state['config_sha256_before']
    state['input_fingerprint_after'] = perf.input_fingerprint(paths)
    assert state['input_fingerprint_after'] == state['input_fingerprint_before']
    state['complete'] = True
    save()
    return state


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('normal', 'scope', 'probe', 'project', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    assert not out.exists(), 'output must be a new directory'
    out.mkdir(parents=True)
    state = {'kind': __doc__, 'harness_sha256': sha(Path(__file__)),
             'shared_harness_sha256': {
                 'docs/architecture/metadata-origin-controls.py': sha(Path(metadata.__file__)),
                 'scripts/whole_project_perf.py': sha(ROOT / 'scripts/whole_project_perf.py')},
             'complete': False, 'whole_project_speed_verified': False,
             'constructor_capacity_unknown_kinds': ['package_cache_key'],
             'sizes_are_output_only_not_allocator_totals': True, 'runs': []}

    def save():
        path = out / 'results.json'
        path.write_text(json.dumps(state, indent=2) + '\n')
        assert json.loads(path.read_text()) == state

    save()
    # Reuse exact effective-option/full-output/loaded+checked/input controls.
    # The normal binary intentionally has no checked marker. Scope-only and
    # off/on controls provide separate direct evidence; do not relabel it.
    control = scope_controls(args, out / 'scope-controls')
    assert control['complete']
    # Check the entire stdout, including an unexpected file appended after the
    # diagnostic section, rather than accepting a matching initial prefix.
    for run in control['runs']:
        lines = (out / 'scope-controls' / (run['role'] + '.stdout')).read_text().splitlines()
        observed = []
        ended = False
        for line in lines:
            if Path(line).is_absolute() and Path(line).is_file():
                assert not ended, 'unexpected file appended after diagnostics'
                observed.append(line)
            elif line:
                ended = True
        assert observed == control['loaded_paths']
    state['scope_control'] = control
    save()
    for run in control['runs']:
        role = run['role']
        stderr = (out / 'scope-controls' / (role + '.stderr')).read_text()
        images = [json.loads(line.split('\t', 1)[1]) for line in stderr.splitlines()
                  if line.startswith('TSR_RESOLVE_CONSTRUCTION\t')]
        assert len(images) == int(role in ('enabled', 'repeat'))
        if not images:
            continue
        counts = images[0]
        assert counts['rows']
        by_kind = {}
        for row in counts['rows']:
            entry = by_kind.setdefault(row['kind'], {
                'calls': 0, 'ns': 0, 'payload_bytes': 0, 'output_capacity_bytes': 0})
            for key in entry:
                entry[key] += row[key]
        state['runs'].append({'role': role, 'counts': counts, 'by_kind': by_kind,
                              'process': {k: run[k] for k in
                                  ('wall_seconds', 'user_seconds', 'system_seconds', 'peak_rss_bytes')}})
        save()
    assert len(state['runs']) == 2
    def stable(counts):
        return [{k: v for k, v in row.items() if k != 'ns'} for row in counts['rows']]
    assert stable(state['runs'][0]['counts']) == stable(state['runs'][1]['counts'])
    binaries = {'normal': args.normal, 'scope': args.scope,
                'disabled': args.probe, 'enabled': args.probe, 'repeat': args.probe}
    assert all(sha(binaries[role]) == value for role, value in control['binary_sha256'].items())
    assert all(sha(ROOT / p) == expected for p, expected in state['shared_harness_sha256'].items())
    state['complete'] = True
    save()
    print(json.dumps({'loaded': len(control['loaded_paths']),
                      'checked': len(control['runs'][1]['checked_paths']),
                      'diagnostics': control['runs'][0]['diagnostics']['count'],
                      'kinds': sorted(state['runs'][0]['by_kind'])}), flush=True)


if __name__ == '__main__':
    main()
