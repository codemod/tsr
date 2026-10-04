"""Finite fixed-plan leased-read controls, not a production throughput gate."""
from __future__ import annotations

import argparse
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import threading

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    'allocation_controls', Path(__file__).with_name('loader-read-allocation-controls.py'))
controls = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controls)
MODES = ('direct', '1', '2', '4')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('pool', 'reference', 'native-reader', 'test-binary', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    assert not out.exists(), 'output must be a new directory'
    out.mkdir(parents=True)
    binaries = {name: getattr(args, name.replace('-', '_')).resolve()
                for name in ('pool', 'reference', 'native-reader', 'test-binary')}
    source_paths = (
        'Cargo.toml',
        'crates/tsr-compiler/examples/read_budget_pool.rs',
        'crates/tsr-compiler/examples/read_budget_pool/policy.rs',
        'crates/tsr-compiler/examples/read_preparation/budget.rs',
        'crates/tsr-compiler/examples/read_allocations.rs',
        'docs/architecture/loader-read-allocation-controls.py',
        'scripts/whole_project_perf.py',
    )
    state = {
        'kind': __doc__, 'checkout_source': subprocess.check_output(
            ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_sha256': {p: sha(ROOT / p) for p in source_paths},
        'harness_sha256': sha(Path(__file__)),
        'binary_sha256': {name: sha(p) for name, p in binaries.items()},
        'native_pin': '5b1047d10d32e7d5b446be4de56b126ff42f82bb',
        'timeout_seconds': 30, 'cases': [], 'fault_tests': [], 'resource_samples': [],
        'complete': False, 'dynamic_loader_replay_verified': False,
        'whole_project_speed_verified': False, 'rss_bound_verified': False,
        'portable_capacity_bound_verified': False,
    }

    def save():
        path = out / 'results.json'
        path.write_text(json.dumps(state, indent=2) + '\n')
        assert json.loads(path.read_text()) == state

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
                        stop.wait(0.005)
                offset = 0
                while descriptor is not None and offset < len(producer[1]) and not stop.is_set():
                    try:
                        offset += os.write(descriptor, producer[1][offset:])
                    except BlockingIOError:
                        stop.wait(0.005)
                    except BrokenPipeError:
                        # Early explicit budget refusal can close the reader.
                        break
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
        assert not result['timed_out'], label
        return result

    def audit(result, limit):
        lines = result['stderr'].splitlines()
        released = next(x for x in lines if x.startswith('released\t')).split('\t')
        live, peak, acquired, released_count, observed_limit = map(int, released[1:])
        assert live == 0 and peak <= limit == observed_limit and acquired == released_count
        held = list(map(int, next(x for x in lines if x.startswith('held\t')).split('\t')[1:]))
        assert held[0] == held[1], 'consumer-held text must retain its exact charge'
        pool_lines = [x for x in lines if x.startswith('pool\t')]
        stats = dict(zip(('attempts', 'refusals', 'retry_batches', 'retry_reads',
                          'decoded_bytes', 'startup_ns', 'wall_ns', 'max_batch_text_bytes',
                          'consumed'), map(int, pool_lines[0].split('\t')[1:]))) if pool_lines else None
        return {'logical_reserved_peak_bytes': peak, 'acquired': acquired,
                'released': released_count, 'live_after_drop': live, 'held_before_drop': held[0],
                'pool': stats, 'process': {k: result[k] for k in
                    ('pid', 'started_at_unix_ns', 'wall_seconds', 'user_seconds',
                     'system_seconds', 'peak_rss_bytes')}}

    def case(name, inputs, limit, outcome='prepared', action='payload', fifo=False, stale=False):
        cwd = out / name
        cwd.mkdir()
        paths = []
        hints = []
        producer = None
        for index, raw in enumerate(inputs):
            source = cwd / f'input-{index}.ts'
            if fifo and index == 0:
                os.mkfifo(source, 0o600)
                producer = (source, raw)
            elif raw == 'directory':
                source.mkdir()
            elif raw is not None:
                source.write_bytes(b'// small\n' if stale else raw)
            hints.append(source.stat().st_size if source.exists() else None)
            if stale and isinstance(raw, bytes):
                source.write_bytes(raw)
            paths.append(source)
        hashes = [sha(p) if p.is_file() else None for p in paths]
        manifest = cwd / 'manifest.txt'
        manifest.write_text(''.join(str(p) + '\n' for p in paths))
        expected = ''
        row = {'name': name, 'limit': limit, 'outcome': outcome, 'action': action,
               'input_sha256': hashes, 'initial_size_hints': hints, 'runs': []}
        if producer:
            row['fifo_producer_sha256'] = hashlib.sha256(producer[1]).hexdigest()
        state['cases'].append(row)
        save()
        if outcome == 'prepared' and action == 'payload':
            for index, path in enumerate(paths):
                reference = capture([str(binaries['reference']), str(path)], cwd, f'reference-{index}',
                                    producer if index == 0 else None)
                assert reference['exit_code'] == 0 and not reference['stderr']
                expected += f'file\t{index}\n' + reference['stdout']
                native = capture([str(binaries['native-reader']), str(path)], cwd, f'native-{index}',
                                 producer if index == 0 else None)
                assert native['exit_code'] == 0 and not native['stderr']
                raw = inputs[index]
                native_bytes = (controls.decoded(raw) if raw.startswith((b'\xff\xfe', b'\xfe\xff'))
                                else raw[3:] if raw.startswith(b'\xef\xbb\xbf') else raw)
                assert native['stdout'] == 'text\t' + native_bytes.hex() + '\n'
                rust_bytes = bytes.fromhex(reference['stdout'].splitlines()[0].split('\t')[1])
                assert rust_bytes == controls.decoded(raw)
                if b'\xff' * 32 not in raw:
                    assert rust_bytes == native_bytes
                else:
                    row['tracked_native_invalid_utf8_gap'] = 'tsr-34z'
        direct = None
        for mode in MODES:
            result = capture([str(binaries['pool']), str(manifest), mode, str(limit), action],
                             cwd, mode, producer)
            observed = audit(result, limit)
            if outcome == 'prepared':
                assert result['exit_code'] == 0
                assert observed['pool']['consumed'] == len(paths)
                if action == 'payload':
                    assert result['stdout'] == expected, (name, mode, 'complete payload mismatch')
                else:
                    assert not result['stdout']
            else:
                assert result['exit_code'] == 2 and result['stderr'].splitlines()[-1].startswith(outcome)
                if outcome == 'consumer-unwind':
                    assert observed['pool'] is None, 'unwind counters are incomplete'
                else:
                    assert observed['pool']['attempts'] <= max(len(paths) * 2, 1)
            if mode == 'direct':
                direct = result['stdout']
            elif outcome == 'io':
                assert result['stdout'] == direct
            if fifo:
                assert observed['pool']['retry_reads'] == 0, 'FIFO must never be reopened for retry'
            row['runs'].append({'mode': mode, **observed,
                                'payload_sha256': hashlib.sha256(result['stdout'].encode()).hexdigest()})
            save()
        assert [sha(p) if p.is_file() else None for p in paths] == hashes
        print(json.dumps({'case': name, 'outcome': outcome}), flush=True)

    save()
    fixtures = {name: raw for name, raw, _, _ in controls.fixtures()}
    tiny = b'const value: number = 1;\n'
    case('empty-plan', [], 0)
    case('tiny-burst', [tiny + f'// {n}\n'.encode() for n in range(17)], 4096)
    case('encoding-and-skew', [raw for name, raw in fixtures.items()
                             if raw is not None and name != 'unknown-stream'], 1024 * 1024)
    case('contention', [b' ' * 32] * 9, 64)
    case('oversized', [fixtures['oversized']], 4096, 'budget')
    case('stale-size', [fixtures['size-growth']], 4096, 'budget', stale=True)
    case('encoding-expansion-refusal', [fixtures['utf16-expansion']], 4096, 'budget')
    case('missing', [tiny, None, tiny], 4096, 'io')
    case('directory', [tiny, 'directory', tiny], 4096, 'io')
    case('fifo-success', [tiny, tiny], 4096, fifo=True)
    case('fifo-refusal', [b' ' * 32], 16, 'budget', fifo=True)
    case('consumer-retention', [b' ' * 32] * 7, 64, 'budget', action='retain')
    # Repository release builds abort on panic; catch_unwind cannot promise
    # cleanup in that mode. Verify explicit refusal, then test actual unwind
    # cleanup below with the Rust test binary (which uses panic=unwind).
    unwind_dir = out / 'unwind-profile'
    unwind_dir.mkdir()
    unused_manifest = unwind_dir / 'must-not-be-read.txt'
    for mode in MODES:
        result = capture([str(binaries['pool']), str(unused_manifest), mode, '4096', 'unwind'],
                         unwind_dir, mode)
        assert result['exit_code'] == 2 and result['stderr'].startswith('unwind-unsupported\t')
        assert not result['stdout']
    state['release_unwind'] = 'unsupported panic=abort, explicitly refused before manifest/read work'
    save()

    # Process timeouts cover fault controls that could otherwise deadlock cargo
    # tests. The inherited API tests cover partial read failure/raw-buffer unwind.
    test_names = (
        'policy::tests::queued_success_is_dropped_before_bounded_canonical_retry',
        'policy::tests::consumer_unwind_disconnects_idle_workers_and_drops_pending_text',
        'policy::tests::worker_panic_and_nonrepeatable_refusal_are_not_retried',
        'budget::tests::read_failure_and_unwind_release_live_raw_storage',
    )
    for index, name in enumerate(test_names):
        result = capture([str(binaries['test-binary']), '--exact', name], out, f'fault-{index}')
        assert result['exit_code'] == 0 and '1 passed' in result['stdout']
        state['fault_tests'].append({'name': name, 'passed': True, 'wall_seconds': result['wall_seconds']})
        save()

    # Read-only cost control, three samples per mode. Setup/builds and parser
    # witness allocations stay outside these timed read/drop-only processes.
    perf_dir = out / 'read-cost'
    perf_dir.mkdir()
    paths = []
    for index in range(128):
        p = perf_dir / f'{index}.ts'
        p.write_bytes(b' ' * (32768 if index % 4 else 262144))
        paths.append(p)
    manifest = perf_dir / 'manifest.txt'
    manifest.write_text(''.join(str(p) + '\n' for p in paths))
    state['resource_workload'] = {'files': len(paths), 'raw_bytes': sum(p.stat().st_size for p in paths),
                                  'input_sha256': [sha(p) for p in paths], 'budget': 8 * 1024 * 1024}
    save()
    for round_index in range(3):
        rotated = MODES[round_index:] + MODES[:round_index]
        for mode in rotated:
            result = capture([str(binaries['pool']), str(manifest), mode, str(8 * 1024 * 1024), 'drop'],
                             perf_dir, f'{round_index}-{mode}')
            assert result['exit_code'] == 0 and not result['stdout']
            observed = audit(result, 8 * 1024 * 1024)
            assert observed['pool']['attempts'] == observed['pool']['consumed'] == len(paths)
            assert observed['pool']['retry_reads'] == 0
            state['resource_samples'].append({'round': round_index, 'mode': mode, **observed})
            save()
    assert [sha(p) for p in paths] == state['resource_workload']['input_sha256']
    assert all(sha(binaries[k]) == v for k, v in state['binary_sha256'].items())
    assert all(sha(ROOT / k) == v for k, v in state['source_sha256'].items())
    state['complete'] = True
    save()


if __name__ == '__main__':
    main()
