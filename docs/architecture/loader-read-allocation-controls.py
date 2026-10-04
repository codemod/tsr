"""Physical allocation boundary controls; no byte admission or speed gate."""
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
spec = importlib.util.spec_from_file_location("perf", ROOT / "scripts/whole_project_perf.py")
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fixtures():
    text = '// allocation control\nconst wrong: number = "s";\n'
    expanded = '// ' + '\u0800' * 4096 + '\n' + text
    surrogate = b'\xff\xfe/\x00/\x00 \x00\x00\xd8\n\x00' + text.encode('utf-16-le')
    large = ('// ' + 'x' * 65536 + '\n' + text).encode()
    return [
        ("unknown-size", text.encode(), None, None),
        ("unknown-stream", text.encode(), None, "fifo"),
        ("empty", b"", None, "known"),
        ("utf8-bom", b'\xef\xbb\xbf' + text.encode(), None, "known"),
        ("utf16-le", b'\xff\xfe' + text.encode('utf-16-le'), None, "known"),
        ("utf16-be", b'\xfe\xff' + text.encode('utf-16-be'), None, "known"),
        ("utf16-expansion", b'\xff\xfe' + expanded.encode('utf-16-le'), None, "known"),
        ("utf16-surrogate", surrogate, None, "known"),
        ("utf16-odd", b'\xff\xfe' + text.encode('utf-16-le') + b'\x42', None, "known"),
        ("malformed-utf8-comment", b'// ' + b'\xff' * 32 + b'\n' + text.encode(), None, "known"),
        ("oversized", large, None, "known"),
        ("size-growth", large, b'// small\n', "stale"),
        ("missing", None, None, None),
        ("directory", None, None, "directory"),
    ]


def decoded(raw):
    if raw.startswith((b'\xff\xfe', b'\xfe\xff')):
        body = raw[2:]
        body = body[:len(body) // 2 * 2]
        return body.decode('utf-16-le' if raw[:2] == b'\xff\xfe' else 'utf-16-be', errors='replace').encode()
    return (raw[3:] if raw.startswith(b'\xef\xbb\xbf') else raw).decode(errors='replace').encode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("normal", "probe", "native-reader", "normal-cli", "tsgo", "output"):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    assert not out.exists(), "output must be a new directory"
    out.mkdir(parents=True)
    binaries = {name: getattr(args, name.replace('-', '_')).resolve()
                for name in ("normal", "probe", "native-reader", "normal-cli", "tsgo")}
    state = {"kind": "read/decode allocation characterization; no speed claim",
             "harness_checkout_source": subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
             "binary_sha256": {name: sha(path) for name, path in binaries.items()},
             "harness_sha256": sha(Path(__file__)),
             "perf_helper_sha256": sha(ROOT / 'scripts/whole_project_perf.py'), "budget_bytes": 4096,
             "budget_enforced": False, "logical_capacity_is_rss_bound": False,
             "native_target_verified": False, "cases": [], "complete": False}

    def save():
        p = out / 'results.json'
        p.write_text(json.dumps(state, indent=2) + '\n')
        assert json.loads(p.read_text()) == state

    def capture(command, cwd, label, observe=False, producer=None):
        env = {k: v for k, v in os.environ.items() if not k.startswith('TSR_')}
        env['TSR_LIB_PATH'] = str(ROOT / 'vendor/typescript-go/internal/bundled/libs')
        if observe:
            env['TSR_READ_ALLOCATIONS'] = '1'
        saved = dict(os.environ)
        stop = threading.Event()
        errors = []

        def produce():
            descriptor = None
            try:
                while not stop.is_set():
                    try:
                        descriptor = os.open(producer[0], os.O_WRONLY | os.O_NONBLOCK)
                        break
                    except OSError as error:
                        if error.errno != errno.ENXIO:  # Reader has not opened the FIFO yet.
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

        writer = threading.Thread(target=produce) if producer else None
        if writer:
            writer.start()
        try:
            os.environ.clear()
            os.environ.update(env)
            result = perf.process(command, cwd, 60)
        finally:
            os.environ.clear()
            os.environ.update(saved)
            stop.set()
            if writer:
                writer.join(2)
                assert not writer.is_alive() and not errors, errors
        for stream in ('stdout', 'stderr'):
            (cwd / (label + '.' + stream)).write_text(result[stream])
        (cwd / (label + '.json')).write_text(json.dumps({
            key: value for key, value in result.items() if key not in ('stdout', 'stderr')
        }, indent=2) + '\n')
        assert not result['timed_out']
        return result

    save()
    for name, contents, initial, hint_kind in fixtures():
        cwd = out / name
        cwd.mkdir()
        source = cwd / 'input.ts'
        if hint_kind == 'fifo':
            os.mkfifo(source, 0o600)
        elif hint_kind == 'directory':
            source.mkdir()
        elif contents is not None:
            source.write_bytes(initial if initial is not None else contents)
        size_hint = source.stat().st_size if hint_kind in ('known', 'stale') else None
        initial_sha = sha(source) if source.is_file() else None
        if initial is not None:
            source.write_bytes(contents)
        input_sha = sha(source) if source.is_file() else None
        case = {"name": name, "input_sha256": input_sha, "initial_sha256": initial_sha,
                "metadata_hint_bytes": size_hint, "actual_raw_bytes": None if contents is None else len(contents),
                "expected_text_sha256": None if contents is None else hashlib.sha256(decoded(contents)).hexdigest(),
                "runs": []}
        if hint_kind == 'fifo':
            case['input_sha256'] = hashlib.sha256(contents).hexdigest()
            case['metadata_reported_size'] = source.stat().st_size
            case['input_kind'] = 'FIFO with controlled producer; no seekable size'
        if name in ('oversized', 'size-growth'):
            assert len(contents) > state['budget_bytes']
        if name == 'size-growth':
            assert size_hint < state['budget_bytes']
        state['cases'].append(case)
        save()
        expected = None
        producer = (source, contents) if hint_kind == 'fifo' else None
        for role in ('normal', 'disabled', 'enabled', 'repeat'):
            binary = binaries['normal' if role == 'normal' else 'probe']
            result = capture([str(binary), str(source)], cwd, role, role in ('enabled', 'repeat'), producer)
            assert result['exit_code'] == 0
            if expected is None:
                expected = result['stdout']
            assert result['stdout'] == expected, "complete read/parse payload changed"
            observations = {parts[0]: [int(value) for value in parts[1:]]
                            for parts in (line.split('\t') for line in result['stderr'].splitlines())}
            assert len(result['stderr'].splitlines()) == len(observations)
            assert set(observations) <= {'read_allocation', 'decode_units'}
            if role in ('normal', 'disabled') or contents is None:
                assert not observations
            else:
                raw_len, raw_capacity, text_len, text_capacity = observations['read_allocation']
                assert raw_len == len(contents) and text_len == len(decoded(contents))
                assert raw_capacity >= raw_len and text_capacity >= text_len
                case['raw_plus_decoded_live_capacity'] = raw_capacity + text_capacity
                if contents.startswith((b'\xff\xfe', b'\xfe\xff')):
                    units_len, units_capacity = observations['decode_units']
                    assert units_len == (len(contents) - 2) // 2 and units_capacity >= units_len
                    case['raw_plus_units_capacity_at_decoder_entry'] = raw_capacity + units_capacity * 2
            row = {key: result[key] for key in ('wall_seconds', 'user_seconds', 'system_seconds', 'peak_rss_bytes')}
            row.update(role=role, observations=observations,
                       complete_output_sha256=hashlib.sha256(result['stdout'].encode()).hexdigest())
            case['runs'].append(row)
            save()
        assert case['runs'][2]['observations'] == case['runs'][3]['observations']
        if contents is None:
            assert expected == 'missing\n'
        else:
            first = expected.splitlines()[0]
            assert bytes.fromhex(first.split('\t', 1)[1]) == decoded(contents)
        native = capture([str(binaries['native-reader']), str(source)], cwd, 'native-reader', producer=producer)
        assert native['exit_code'] == 0 and not native['stderr']
        if contents is None:
            native_expected = 'missing\n'
        else:
            native_bytes = (decoded(contents) if contents.startswith((b'\xff\xfe', b'\xfe\xff'))
                            else contents[3:] if contents.startswith(b'\xef\xbb\xbf') else contents)
            native_expected = 'text\t' + native_bytes.hex() + '\n'
        assert native['stdout'] == native_expected, 'native raw read/decode payload changed'
        case['native_read_bytes_match'] = native['stdout'] == expected.splitlines()[0] + '\n'
        assert case['native_read_bytes_match'] == (name != 'malformed-utf8-comment')
        if contents is not None and hint_kind != 'fifo':
            globals_file = cwd / 'globals.d.ts'
            globals_file.write_text('interface Array<T> { length: number; [n: number]: T }\n' +
                '\n'.join('interface ' + name + ' {}' for name in
                          ('Boolean', 'Function', 'IArguments', 'Number', 'Object', 'RegExp', 'String', 'CallableFunction', 'NewableFunction')))
            config = cwd / 'tsconfig.json'
            config.write_text(json.dumps({'compilerOptions': {'strict': True, 'noLib': True, 'types': []},
                                         'files': ['globals.d.ts', 'input.ts']}) + '\n')
            case['config_sha256'] = sha(config)
            case['globals_sha256'] = sha(globals_file)
            controls = [capture([str(binaries[role]), '--project', str(config), '--noEmit', '--pretty', 'false', '--listFiles'], cwd, role)
                        for role in ('normal-cli', 'tsgo')]
            assert all(not result['stderr'] for result in controls)
            diagnostics = [perf.diagnostics(result['stdout'], cwd) for result in controls]
            assert diagnostics[0] == diagnostics[1] and diagnostics[0]['count'] == int(name != 'empty')
            case['complete_diagnostics'] = diagnostics[0]
            loaded = [[line for line in result['stdout'].splitlines() if line.endswith('.ts')
                       and not perf.DIAGNOSTIC_START.match(line)] for result in controls]
            assert loaded[0] == loaded[1] == [str(globals_file), str(source)]
            case['loaded_count'] = len(loaded[0])
            case['loaded_identities'] = loaded[0]
            case['cli_exit_codes'] = [result['exit_code'] for result in controls]
            assert case['cli_exit_codes'] == [int(name != 'empty')] * 2
            case['cli_output_sha256'] = {role: sha(cwd / (role + '.stdout'))
                                         for role in ('normal-cli', 'tsgo')}
            assert sha(config) == case['config_sha256']
            assert sha(globals_file) == case['globals_sha256']
        assert (sha(source) if source.is_file() else None) == input_sha
        save()
        print(json.dumps({'case': name, 'native_read_match': case['native_read_bytes_match']}), flush=True)
    assert all(sha(binaries[name]) == value for name, value in state['binary_sha256'].items())
    assert sha(ROOT / 'scripts/whole_project_perf.py') == state['perf_helper_sha256']
    state['complete'] = True
    save()
    print(json.dumps({'cases': len(state['cases']), 'complete': True}))


if __name__ == '__main__':
    main()
