"""Mutation controls for the temporary attribution driver, without a compiler."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location(
    'construction', Path(__file__).with_name('resolver-construction-controls.py'))
construction = importlib.util.module_from_spec(spec)
spec.loader.exec_module(construction)


class InputControls(unittest.TestCase):
    def test_missing_candidate_creation_and_kind_change(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'candidate.ts'
            before = construction.input_snapshot([str(path)])
            path.write_text('export {}')
            after = construction.input_snapshot([str(path)])
            self.assertEqual(before[0]['kind'], 'missing')
            self.assertEqual(after[0]['kind'], 'file')
            self.assertNotEqual(before, after)
            path.unlink()
            path.mkdir()
            self.assertEqual(construction.input_snapshot([str(path)])[0]['kind'], 'directory')

    def test_manifest_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'package.json'
            path.write_text('{}')
            before = construction.input_snapshot([str(path)])
            path.write_text('{"types":"entry.d.ts"}')
            self.assertNotEqual(before, construction.input_snapshot([str(path)]))

    def test_ancestor_symlink_retarget_with_identical_file_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ('one', 'two'):
                (root / name).mkdir()
                (root / name / 'dep.ts').write_text('export {}')
            link = root / 'linked'
            link.symlink_to('one', target_is_directory=True)
            paths = [str(link / 'dep.ts')]
            before = construction.input_snapshot(paths)
            link.unlink()
            link.symlink_to('two', target_is_directory=True)
            after = construction.input_snapshot(paths)
            self.assertEqual(before[0]['sha256'], after[0]['sha256'])
            self.assertNotEqual(before, after)

    def test_file_appended_after_diagnostics_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'extra.ts'
            path.write_text('export {}')
            with self.assertRaisesRegex(AssertionError, 'appended after diagnostics'):
                construction.loaded_paths('error TS2307: missing\n' + str(path) + '\n')


if __name__ == '__main__':
    unittest.main()
