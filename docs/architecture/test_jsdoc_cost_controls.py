"""Negative controls for the archived attribution reader, not speed assertions."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import sys
import os

sys.dont_write_bytecode = True

SPEC = importlib.util.spec_from_file_location("jsdoc_cost", Path(__file__).with_name("jsdoc-cost-controls.py"))
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class ProbeReaderTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "trace.tsv"
        self.child = {"pid": 73, "started_at_unix_ns": 1000, "exit_code": 0, "timed_out": False,
                      "stdout": "Parsed files: 1\n"}
        self.header = "tsr-jsdoc-cost-v2\t73\t2000\n"
        self.row = "2f612e7473\t12\tTypeScript\t100\t200\t30\t80\t1\t1\t12\t1\t24\n"

    def read(self, text):
        self.path.write_text(text)
        return probe.read_probe(self.path, self.child)

    def test_valid_enclosing_intervals(self):
        result = self.read(self.header + self.row)
        self.assertEqual(result["totals"]["jsdoc_ns"], 30)
        self.assertEqual(result["categories"]["ts"]["jsdoc_arena_requested_bytes"], 80)

    def test_replayed_process_rejected(self):
        with self.assertRaisesRegex(ValueError, "PID/version"):
            self.read(self.header.replace("73", "74") + self.row)

    def test_partial_record_rejected(self):
        with self.assertRaisesRegex(ValueError, "partial"):
            self.read((self.header + self.row).rstrip("\n"))

    def test_same_pid_replay_rejected(self):
        with self.assertRaisesRegex(ValueError, "stale same-PID"):
            self.read(self.header.replace("2000", "999") + self.row)

    def test_incomplete_scope_rejected(self):
        with self.assertRaisesRegex(ValueError, "every compiler file parse"):
            self.read(self.header)

    def test_time_outside_parse_rejected(self):
        with self.assertRaisesRegex(ValueError, "time exceeds"):
            self.read(self.header + self.row.replace("\t30\t80", "\t101\t80"))

    def test_bytes_outside_parse_rejected(self):
        with self.assertRaisesRegex(ValueError, "bytes exceed"):
            self.read(self.header + self.row.replace("\t30\t80", "\t30\t201"))

    def test_failed_child_rejected(self):
        self.child["exit_code"] = -9
        with self.assertRaisesRegex(ValueError, "incomplete trace child"):
            self.read(self.header + self.row)

    def test_noncanonical_filename_rejected(self):
        with self.assertRaisesRegex(ValueError, "noncanonical"):
            self.read(self.header + self.row.replace("2f612e7473", "2F612e7473"))

    def test_fifo_rejected_without_blocking(self):
        os.mkfifo(self.path)
        with self.assertRaisesRegex(OSError, "not a regular"):
            probe.read_probe(self.path, self.child)

    def test_generator_identity(self):
        first = probe.fixtures(Path(self.directory.name) / "first")
        second = probe.fixtures(Path(self.directory.name) / "second")
        self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
