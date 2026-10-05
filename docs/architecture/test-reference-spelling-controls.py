"""Negative qualification controls for complete reference-display receipts."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("spelling", Path(__file__).with_name("reference-spelling-controls.py"))
spelling = importlib.util.module_from_spec(spec)
spec.loader.exec_module(spelling)


def native_receipt():
    orders = []
    for reverse in (False, True):
        names = ["probe_a", "probe_b"]
        if reverse:
            names.reverse()
        orders.append(dict(reverse=reverse, types=[dict(file="contract.ts", name=name, phase=phase,
                      type="number", query_instantiation_delta=0, print_instantiation_delta=0)
                      for phase in ("before-check", "after-check") for name in names],
                      loaded_inputs_sha256={"/contract.ts": "a" * 64}, diagnostics=[],
                      declarations={"/contract.d.ts": "export declare const probe_a: number;\n"},
                      emit_diagnostics=[], emit_skipped=False))
    return dict(schema=1, protocol="baseline", orders=orders)


class NativeControls(unittest.TestCase):
    def test_complete_native_receipt(self):
        rows = spelling.native_maps(native_receipt(), "baseline", ["probe_a", "probe_b"])
        self.assertEqual(len(rows), 2)

    def test_native_rejects_incomplete_or_changed_work(self):
        mutations = [
            lambda x: x.update(protocol="plain"),
            lambda x: x["orders"].pop(),
            lambda x: x["orders"][0]["types"].pop(),
            lambda x: x["orders"][0]["types"][0].update(name="probe_b"),
            lambda x: x["orders"][0]["types"][0].update(query_instantiation_delta=-1),
            lambda x: x["orders"][0]["types"][0].update(print_instantiation_delta=True),
            lambda x: x["orders"][0]["types"][0].update(type="string"),
            lambda x: x["orders"][1].update(loaded_inputs_sha256={}),
            lambda x: x["orders"][1].update(declarations={}),
            lambda x: x["orders"][1].update(emit_skipped=True),
            lambda x: x["orders"][1].update(diagnostics=[dict(code=2322)]),
            lambda x: x["orders"][1].update(declarations={"/contract.d.ts": "wrong"}),
        ]
        for mutate in mutations:
            with self.subTest(mutation=mutations.index(mutate)):
                value = native_receipt()
                mutate(value)
                with self.assertRaises(ValueError):
                    spelling.native_maps(value, "baseline", ["probe_a", "probe_b"])

    def test_rust_requires_inputs_and_complete_order_results(self):
        text = "input\tcontract.ts\t78\n" + "".join(f"{phase}\tcontract.ts\tprobe_a\tnumber\n"
            for phase in ("producer", "forward", "reverse"))
        self.assertEqual(spelling.rust_maps(text, ["probe_a"])[0], {"contract.ts": "x"})
        for changed in (text.replace("input\tcontract.ts\t78\n", ""),
                        text.replace("reverse\tcontract.ts\tprobe_a\tnumber\n", ""),
                        text.replace("reverse\tcontract.ts\tprobe_a\tnumber", "reverse\tcontract.ts\tprobe_a\tstring"),
                        text + "producer\tcontract.ts\tprobe_a\tnumber\n"):
            with self.assertRaises(ValueError):
                spelling.rust_maps(changed, ["probe_a"])

    def test_build_receipt_rejects_stale_binary_or_source(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            binary, helper, receipt = [root / name for name in ("binary", "helper", "receipt")]
            binary.write_bytes(b"binary")
            helper.write_bytes(b"source")
            value = dict(terminal=True, exit_code=0, binary_sha256=spelling.cost.file_hash(binary),
                         helper_sha256=spelling.cost.file_hash(helper))
            for mutation in ({}, dict(exit_code=1), dict(terminal=False), dict(binary_sha256="bad"), dict(helper_sha256="bad")):
                changed = copy.deepcopy(value)
                changed.update(mutation)
                receipt.write_text(json.dumps(changed))
                if mutation:
                    with self.assertRaises(ValueError):
                        spelling.build_identity(binary, receipt, helper)
                else:
                    self.assertEqual(spelling.build_identity(binary, receipt, helper), value)


if __name__ == "__main__":
    unittest.main()
