"""Source-qualified locating controls; sampling counts do not establish a speed gain."""
from __future__ import annotations

import argparse
from collections import Counter
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import threading
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('perf', ROOT / 'scripts/whole_project_perf.py')
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)


def parse_sample(text: str) -> dict:
    graph = text.split('Call graph:\n', 1)[1].split('\nTotal number', 1)[0]
    nodes, stack = [], []
    root_depth = None
    ignored_thread_roots = 0
    for line in graph.splitlines():
        match = re.match(r'^[ +!:|]*(\d+) (.+)$', line)
        if not match:
            continue
        depth, count, raw = match.start(1), int(match[1]), match[2]
        if root_depth is None:
            if 'com.apple.main-thread' not in raw:
                continue
            root_depth = depth
        elif depth <= root_depth:
            if raw.startswith('Thread_'):
                ignored_thread_roots += 1
            continue
        if ignored_thread_roots:
            continue
        label = re.split(r'\s+\(in ', raw, maxsplit=1)[0]
        label = re.sub(r'::_\$LT\$impl.*?\$GT\$::', '::', label)
        label = re.sub(r'::h[0-9a-f]+$', '', label)
        while stack and nodes[stack[-1]]['depth'] >= depth:
            stack.pop()
        node = dict(depth=depth, count=count, children=0, label=label, raw=raw,
                    parent=stack[-1] if stack else None)
        if stack:
            nodes[stack[-1]]['children'] += count
        nodes.append(node)
        stack.append(len(nodes) - 1)
    if not nodes:
        raise ValueError('sample has no main-thread call tree')
    self_counts, owners, allocators, phases = (Counter() for _ in range(4))
    for index, node in enumerate(nodes):
        amount = node['count'] - node['children']
        if amount < 0:
            raise ValueError('sample child counts exceed parent count')
        if not amount:
            continue
        ancestry = []
        current = index
        while current is not None:
            ancestry.append(nodes[current]['label'])
            current = nodes[current]['parent']
        owner = next((label for label in ancestry if label.startswith('tsr_')), '<no TSR frame>')
        phase = 'checker' if any(label.startswith('tsr_checker::') for label in ancestry) else 'other'
        self_counts[node['label']] += amount
        owners[owner] += amount
        phases[phase] += amount
        if 'libsystem_malloc' in node['raw']:
            allocators[owner] += amount
    total = nodes[0]['count']
    if any(sum(counts.values()) != total for counts in (self_counts, owners, phases)):
        raise ValueError('main-thread sample accounting is incomplete')
    return {'total_samples': total, 'phases': dict(phases),
            'ignored_thread_roots': ignored_thread_roots,
            'top_self': [list(item) for item in self_counts.most_common(25)],
            'nearest_tsr_owner': [list(item) for item in owners.most_common(40)],
            'allocator_nearest_owner': [list(item) for item in allocators.most_common(25)]}


def profiled_process(command: list[str], cwd: Path, output: Path,
                     delay: float, duration: int, timeout: float) -> dict:
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        started = time.perf_counter()
        child = subprocess.Popen(command, cwd=cwd, stdout=stdout, stderr=stderr,
                                 start_new_session=True, env={**os.environ, 'NO_COLOR': '1'})
        timed_out = threading.Event()

        def kill():
            try:
                os.killpg(child.pid, signal.SIGKILL)
                timed_out.set()
            except ProcessLookupError:
                pass

        timer = threading.Timer(timeout, kill)
        timer.start()
        sampler = None
        try:
            # Do not poll/reap before attachment: the launched child's PID
            # remains owned, including if it has already become a zombie.
            time.sleep(delay)
            sample_start = time.perf_counter() - started
            sampler = subprocess.Popen(
                ['/usr/bin/sample', str(child.pid), str(duration), '1', '-file', str(output)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            elapsed = time.perf_counter() - started
            sample_stdout, sample_stderr = sampler.communicate(timeout=duration + 15)
        except BaseException:
            kill()
            if child.returncode is None:
                _, status, _ = os.wait4(child.pid, 0)
                child.returncode = os.waitstatus_to_exitcode(status)
            raise
        finally:
            timer.cancel()
            timer.join()
            if sampler is not None and sampler.poll() is None:
                sampler.kill()
                sampler.communicate()
        stdout.seek(0)
        stderr.seek(0)
        return {'pid': child.pid, 'command': command, 'wall_seconds': elapsed,
                'user_seconds': usage.ru_utime, 'system_seconds': usage.ru_stime,
                'peak_rss_bytes': usage.ru_maxrss * (1 if sys.platform == 'darwin' else 1024),
                'exit_code': child.returncode, 'timed_out': timed_out.is_set(),
                'stdout': stdout.read().decode(errors='replace'),
                'stderr': stderr.read().decode(errors='replace'),
                'sample_start_seconds': sample_start, 'sample_exit': sampler.returncode,
                'sample_tool_output': (sample_stdout + sample_stderr).decode(errors='replace')}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('binary', 'project', 'source-root', 'input-manifest', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--delay', type=float, default=0.2)
    parser.add_argument('--duration', type=int, default=3)
    parser.add_argument('--resume', action='store_true', help='resume saved observations after validating identity')
    args = parser.parse_args()
    if args.delay < 0 or args.duration < 1 or args.delay + args.duration >= 90:
        parser.error('invalid sample interval')
    args.output = args.output.resolve()
    args.output.mkdir(exist_ok=args.resume)
    binary, project = args.binary.resolve(strict=True), args.project.resolve(strict=True)
    source = perf.revision(args.source_root)
    if not source:
        parser.error('source-root must have a recorded git revision')
    # Caller supplies a built immutable snapshot. No builds run in this driver.
    source_status = subprocess.check_output(
        ['git', '-C', str(args.source_root), 'diff', 'HEAD', '--stat', '--', '.',
         ':!vendor/typescript-go'], text=True)
    if source_status.strip():
        parser.error('source-root has tracked changes')
    for key in list(os.environ):
        if key.startswith('TSR_'):
            os.environ.pop(key)
    os.environ['TSR_LIB_PATH'] = str((ROOT / 'vendor/typescript-go/internal/bundled/libs').resolve())
    oracle = perf.revision(ROOT / 'vendor/typescript-go')
    flags = ['--project', str(project), '--noEmit', '--incremental', 'false',
             '--composite', 'false', '--pretty', 'false']
    paths, provenance = perf.inputs.load_manifest(args.input_manifest, project.parent)
    paths += [str(project), str(binary), str(args.input_manifest.resolve())]
    state = {'kind': __doc__, 'source': source, 'oracle_sha': oracle,
             'vendor_symlink_is_separately_identified': True,
             'binary_sha256': perf.inputs.file_hash(binary),
             'harness_sha256': perf.inputs.file_hash(Path(__file__)),
             'shared_harness_sha256': {name: perf.inputs.file_hash(ROOT / 'scripts' / name)
                                      for name in ('whole_project_perf.py', 'benchmark_inputs.py')},
             'flags': flags, 'input_manifest': provenance, 'runs': [], 'complete': False,
             'native_target_verified': False, 'complete_input_equivalence_verified': False,
             'limits': ['Historical/caller query paths are partial, not complete cross-tool inputs.',
                        'Only main-thread stacks inside the selected interval; other threads are omitted.',
                        'Stack samples include waiting and are not wall/CPU percentages or allocation counts.',
                        'Sampling overhead and unsampled startup/end work are not saved-wall ceilings.',
                        'Other agents may use the host; only this driver serializes its own work.']}
    if args.resume:
        previous_state = json.loads((args.output / 'results.json').read_text())
        for key in ('source', 'oracle_sha', 'binary_sha256', 'shared_harness_sha256', 'flags', 'input_manifest'):
            assert previous_state[key] == state[key], 'resume identity changed: ' + key
        current_driver = state['harness_sha256']
        state = previous_state
        state.setdefault('driver_versions', [state['harness_sha256']]).append(current_driver)

    def save():
        path = args.output / 'results.json'
        path.write_text(json.dumps(state, indent=2) + '\n')
        assert json.loads(path.read_text()) == state

    reference = perf.inputs.snapshot(paths)
    assert perf.inputs.valid_snapshot(reference)
    if args.resume:
        loaded = (args.output / 'listing.stdout').read_text().splitlines()
        paths = sorted(set(paths + loaded))
        reference = perf.inputs.snapshot(paths)
        assert perf.inputs.valid_snapshot(reference)
        assert perf.fingerprint(reference) == state['input_fingerprint'], 'resume inputs changed'
    save()

    def run(role, extra, profile=False):
        before_start = time.perf_counter()
        before = perf.inputs.snapshot(paths)
        before_seconds = time.perf_counter() - before_start
        assert before == reference and perf.inputs.valid_snapshot(before), 'inputs changed before child'
        command = [str(binary), *flags, *extra]
        sample = args.output / (role + '.sample')
        result = (profiled_process(command, project.parent, sample, args.delay, args.duration, 90)
                  if profile else perf.process(command, project.parent, 90))
        for stream in ('stdout', 'stderr'):
            (args.output / (role + '.' + stream)).write_text(result[stream])
        after_start = time.perf_counter()
        after = perf.inputs.snapshot(paths)
        row = {k: v for k, v in result.items() if k not in ('stdout', 'stderr', 'sample_tool_output')}
        row.update(role=role, input_fingerprint_before=perf.fingerprint(before),
                   input_fingerprint_after=perf.fingerprint(after),
                   input_capture_seconds={'before': before_seconds, 'after': time.perf_counter() - after_start},
                   diagnostics=perf.diagnostics(result['stdout'], project.parent),
                   statistics={m[1].strip(): m[2] for m in
                               re.finditer(r'^([^\n:]+):\s+([0-9]+(?:\.[0-9]+)?s?)$', result['stdout'], re.M)},
                   loaded_order_fingerprint=None, profile=None)
        listed = [line for line in result['stdout'].split('\n') if line.startswith('/')]
        if listed:
            row['loaded_order_fingerprint'] = perf.fingerprint(listed)
            row['loaded_count'] = len(listed)
        state['runs'].append(row)
        save()  # Save measurement even if a later scope/parser check fails.
        assert after == reference and perf.inputs.valid_snapshot(after), 'inputs changed during child'
        assert not result['timed_out'] and result['exit_code'] in (0, 1, 2)
        if profile:
            (args.output / (role + '.sample-tool.txt')).write_text(result['sample_tool_output'])
            assert result['sample_exit'] == 0
            row['profile'] = parse_sample(sample.read_text())
            row['sample_sha256'] = perf.inputs.file_hash(sample)
            save()
        print(json.dumps({'role': role, 'wall_seconds': row['wall_seconds'],
                          'diagnostics': row['diagnostics']['count']}), flush=True)
        return result, row

    if not args.resume:
        listing, _ = run('listing', ['--listFilesOnly'])
        loaded = listing['stdout'].splitlines()
        assert loaded and all(Path(name).is_absolute() and Path(name).is_file() for name in loaded)
        previous = {row['path']: row for row in reference}
        paths = sorted(set(paths + loaded))
        reference = perf.inputs.snapshot(paths)
        assert perf.inputs.valid_snapshot(reference)
        assert all(row == previous[row['path']] for row in reference if row['path'] in previous)
        state.update(loaded_count=len(loaded), loaded_order_fingerprint=perf.fingerprint(loaded),
                     input_count=len(reference), input_fingerprint=perf.fingerprint(reference))
        config, _ = run('configuration', ['--showConfig'])
        state['effective_config_fingerprint'] = perf.fingerprint(json.loads(config['stdout']))
        save()
    expected = None
    for role, extra, profile in (
        ('normal1', [], False), ('phase1', ['--listFiles', '--extendedDiagnostics'], False),
        ('sample1', ['--listFiles', '--extendedDiagnostics'], True),
        ('normal2', [], False), ('phase2', ['--listFiles', '--extendedDiagnostics'], False),
        ('sample2', ['--listFiles', '--extendedDiagnostics'], True)):
        existing = next((row for row in state['runs'] if row['role'] == role), None)
        if existing is not None:
            row = existing
            assert args.resume and not row['timed_out'] and row['exit_code'] in (0, 1, 2)
            assert row['input_fingerprint_before'] == row['input_fingerprint_after'] == state['input_fingerprint']
            if profile and row['profile'] is None:
                assert row['sample_exit'] == 0
                row['profile'] = parse_sample((args.output / (role + '.sample')).read_text())
                save()
        else:
            _, row = run(role, extra, profile)
        if expected is None:
            expected = row['diagnostics']
        assert row['diagnostics'] == expected
        if extra:
            assert row['loaded_order_fingerprint'] == state['loaded_order_fingerprint']
    state['complete'] = True
    save()


if __name__ == '__main__':
    main()
