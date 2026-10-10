//! `getNameOfSymbolAsWritten` (`nodebuilderimpl.go:973`) names a merged
//! `default` symbol by its first named declaration
//! (`docs/parity/notes/r7-printer.md` §4). Expected lines from pinned native
//! tsgo 5b1047d's baseline for `exportDefaultClassAndValue`.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn a_default_class_merged_after_export_default_value_is_written_by_the_value_name() {
    let source = r"// @module: commonjs
// @target: es2015
const foo = 1
export default foo
export default class Foo {}
";
    let case = TestCase::parse("probe/default_symbol_written_name", "a.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<String> = types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in ["foo : 1", "Foo : foo"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
