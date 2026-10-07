//! Native getSignatureFromDeclaration/contextuallyCheckFunctionExpressionOrObjectLiteralMethod,
//! pinned to tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb. Keep duplicate
//! diagnostic bags visible.
use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn recursive_declarations_original_entry_does_not_add_either_circular_return_occurrence() {
    let fixture = "declarationsWithRecursiveInternalTypesProduceUniqueTypeParams.ts";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/typescript-go/_submodules/TypeScript/tests/cases/compiler");
    if !root.is_dir() {
        eprintln!("skipping original-entry native fixture: TypeScript submodule is absent");
        return;
    }
    let source = std::fs::read_to_string(root.join(fixture)).expect("pinned native fixture");
    let case = TestCase::parse("probe/original-callable-entry", fixture, &source);
    let mut actual: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    actual.sort_unstable();
    // Native has an empty bag: neither circular-return TS7024 (8,20)/(11,13)
    // nor the `unique symbol` computed-key TS2464 (13,82).
    assert_eq!(actual, []);
}
