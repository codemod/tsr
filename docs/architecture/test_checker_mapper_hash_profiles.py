"""Qualify worker-thread sample attribution without treating waits as checker cost."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    'hash_profiles', Path(__file__).with_name('current-project-cost-controls.py'))
profile = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(profile)

SAMPLE = '''Call graph:
    10 Thread_1 DispatchQueue_1: com.apple.main-thread (serial)
      10 tsr_execute::checker_pool::check_program_files (in tsr)
        10 __psynch_cvwait (in libsystem_kernel.dylib)
    8 Thread_2 Name: checker-0
      8 tsr_checker::inference::instantiate_type (in tsr)
        5 rustc_hash::FxHasher::write (in tsr)
        3 malloc (in libsystem_malloc.dylib)
    6 Thread_3 Name: checker-1
      6 tsr_checker::declared::get_instantiated_type_reference (in tsr)
        6 malloc (in libsystem_malloc.dylib)
Total number
'''


class ThreadSampleTests(unittest.TestCase):
    def test_main_wait_and_both_workers_are_separate(self):
        result = profile.parse_thread_samples(SAMPLE)
        self.assertEqual([row['total_samples'] for row in result['threads']], [10, 8, 6])
        self.assertEqual(result['total_thread_samples'], 24)
        self.assertEqual(result['threads'][0]['phases'], {'other': 10})
        self.assertEqual(result['threads'][1]['phases'], {'checker': 8})
        self.assertEqual(result['threads'][2]['phases'], {'checker': 6})
        self.assertEqual(result['threads'][1]['allocator_nearest_owner'],
                         [['tsr_checker::inference::instantiate_type', 3]])
        self.assertEqual(result['threads'][2]['allocator_nearest_owner'],
                         [['tsr_checker::declared::get_instantiated_type_reference', 6]])

    def test_default_parser_keeps_existing_main_thread_scope(self):
        result = profile.parse_sample(SAMPLE)
        self.assertEqual(result['total_samples'], 10)
        self.assertEqual(result['ignored_thread_roots'], 2)
        self.assertEqual(result['allocator_nearest_owner'], [])

    def test_duplicate_thread_or_child_overcount_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            profile.parse_thread_samples(SAMPLE.replace('Thread_3 Name: checker-1', 'Thread_2 Name: checker-0'))
        with self.assertRaisesRegex(ValueError, 'exceed'):
            profile.parse_thread_samples(SAMPLE.replace('        5 rustc_hash', '        9 rustc_hash'))

    def test_unowned_worker_frame_is_not_assigned_to_a_checker(self):
        result = profile.parse_thread_samples(SAMPLE.replace(
            'tsr_checker::inference::instantiate_type', 'foreign_worker'))
        worker = result['threads'][1]
        self.assertEqual(worker['phases'], {'other': 8})
        self.assertEqual(worker['nearest_tsr_owner'], [['<no TSR frame>', 8]])


if __name__ == '__main__':
    unittest.main()
