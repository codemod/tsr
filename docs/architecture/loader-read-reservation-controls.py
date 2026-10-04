"""Physical leased-preparation controls; logical request accounting, no speed gate."""
from __future__ import annotations

import argparse
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import threading

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    'allocation_controls', Path(__file__).with_name('loader-read-allocation-controls.py'))
controls = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controls)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('prepared', 'reference', 'native-reader', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    assert not out.exists(), 'output must be a new directory'
    out.mkdir(parents=True)
    binaries = {name: getattr(args, name.replace('-', '_')).resolve()
                for name in ('prepared', 'reference', 'native-reader')}
    state = {'kind': __doc__, 'binary_sha256': {name: sha(p) for name, p in binaries.items()},
             'harness_sha256': sha(Path(__file__)), 'cases': [], 'complete': False,
             'shared_source_sha256': {str(p.relative_to(ROOT)): sha(p) for p in
                 (Path(__file__).with_name('loader-read-allocation-controls.py'),
                  ROOT / 'scripts/whole_project_perf.py')},
             'capacity_model_is_portable': False, 'rss_bound_verified': False,
             'whole_project_speed_verified': False}

    def save():
        p = out / 'results.json'
        p.write_text(json.dumps(state, indent=2) + '\n')
        assert json.loads(p.read_text()) == state

    def capture(command, cwd, label, producer=None):
        stop = threading.Event()
        errors = []

        def write_fifo():
            descriptor = None
            try:
                while not stop.is_set():
                    try:
                        descriptor = os.open(producer[0], os.O_WRONLY | os.O_NONBLOCK)
                        break
                    except OSError as error:
                        if error.errno != errno.ENXIO:
                            raise
                        stop.wait(0.01)
                offset = 0
                while descriptor is not None and offset < len(producer[1]) and not stop.is_set():
                    try:
                        offset += os.write(descriptor, producer[1][offset:])
                    except BlockingIOError:
                        stop.wait(0.01)
            except OSError as error:
                errors.append(repr(error))
            finally:
                if descriptor is not None:
                    os.close(descriptor)

        writer = threading.Thread(target=write_fifo) if producer else None
        if writer:
            writer.start()
        try:
            result = controls.perf.process(command, cwd, 30)
        finally:
            stop.set()
            if writer:
                writer.join(2)
                assert not writer.is_alive() and not errors, errors
        for stream in ('stdout', 'stderr'):
            (cwd / (label + '.' + stream)).write_text(result[stream])
        (cwd / (label + '.json')).write_text(json.dumps({
            k: v for k, v in result.items() if k not in ('stdout', 'stderr')}, indent=2) + '\n')
        assert not result['timed_out']
        return result

    save()
    for name, contents, initial, hint_kind in controls.fixtures():
        cwd = out / name
        cwd.mkdir()
        source = cwd / 'input.ts'
        if hint_kind == 'fifo':
            os.mkfifo(source, 0o600)
        elif hint_kind == 'directory':
            source.mkdir()
        elif contents is not None:
            source.write_bytes(initial if initial is not None else contents)
        hint = source.stat().st_size if hint_kind in ('known', 'stale', 'fifo') else None
        if initial is not None:
            source.write_bytes(contents)
        fingerprint = sha(source) if source.is_file() else None
        producer = (source, contents) if hint_kind == 'fifo' else None
        row = {'name': name, 'metadata_hint': hint, 'input_sha256': fingerprint,
               'fifo_producer_sha256': hashlib.sha256(contents).hexdigest() if producer else None,
               'audits': []}
        state['cases'].append(row)
        save()
        reference = capture([str(binaries['reference']), str(source)], cwd, 'reference', producer)
        prepared = capture([str(binaries['prepared']), str(source)], cwd, 'prepared', producer)
        assert reference['exit_code'] == prepared['exit_code'] == 0
        assert not reference['stderr'] and not prepared['stderr']
        assert reference['stdout'] == prepared['stdout'], 'complete read/parse image changed'
        row['complete_payload_sha256'] = hashlib.sha256(prepared['stdout'].encode()).hexdigest()
        native = capture([str(binaries['native-reader']), str(source)], cwd, 'native', producer)
        assert native['exit_code'] == 0 and not native['stderr']
        if contents is None:
            assert native['stdout'] == reference['stdout'] == 'missing\n'
        else:
            native_bytes = (controls.decoded(contents) if contents.startswith((b'\xff\xfe', b'\xfe\xff'))
                            else contents[3:] if contents.startswith(b'\xef\xbb\xbf') else contents)
            assert native['stdout'] == 'text\t' + native_bytes.hex() + '\n'
            assert bytes.fromhex(prepared['stdout'].splitlines()[0].split('\t')[1]) == controls.decoded(contents)
        row['native_read_match'] = native['stdout'] == prepared['stdout'].splitlines()[0] + '\n'
        assert row['native_read_match'] == (name != 'malformed-utf8-comment')
        for limit in (1024 * 1024, 4096, 0) if name == 'empty' else (1024 * 1024, 4096):
            result = capture([str(binaries['prepared']), str(source), str(limit), 'audit'], cwd, str(limit), producer)
            lines = result['stderr'].splitlines()
            released = next(line for line in lines if line.startswith('released\t')).split('\t')
            live, peak, acquired, releases, observed_limit = map(int, released[1:])
            assert live == 0 and acquired == releases and peak <= limit == observed_limit
            if contents is None:
                assert result['exit_code'] == 2 and lines[-1].startswith('io\t')
                outcome = 'io'
            elif len(contents) > limit:
                assert result['exit_code'] == 2 and lines[-1].startswith('budget\t')
                assert not result['stdout'], 'budget refusal must not become missing/partial output'
                outcome = 'budget'
            else:
                assert result['exit_code'] == 0 and result['stdout'] == reference['stdout']
                retained = next(line for line in lines if line.startswith('retained\t')).split('\t')
                assert int(retained[1]) == int(retained[2])
                outcome = 'prepared'
            row['audits'].append({'limit': limit, 'outcome': outcome, 'live_after_drop': live,
                                  'peak_requested_layout_bytes': peak, 'acquired': acquired,
                                  'released': releases, 'process': {k: result[k] for k in
                                      ('wall_seconds', 'user_seconds', 'system_seconds', 'peak_rss_bytes')}})
            save()
        assert (sha(source) if source.is_file() else None) == fingerprint
        print(json.dumps({'case': name, 'outcomes': [a['outcome'] for a in row['audits']]}), flush=True)
    assert all(sha(binaries[name]) == expected for name, expected in state['binary_sha256'].items())
    assert all(sha(ROOT / path) == expected for path, expected in state['shared_source_sha256'].items())
    state['complete'] = True
    save()


if __name__ == '__main__':
    main()
