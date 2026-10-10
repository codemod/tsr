//! `getExplicitTypeOfSymbol`'s for-of arm (pinned flow.go:2176): a for-of
//! variable whose iterated expression has an explicit dotted type supplies an
//! assertion call's effects signature.
//!
//! Expected TS2339 positions come from pinned tsgo
//! 5b1047d10d32e7d5b446be4de56b126ff42f82bb on the same source.

use tsr_conformance::{TestCase, diagnostics_suite};

fn diagnostics(source: &str) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/explicit-for-of", "probe.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "true".into());
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

#[test]
fn for_of_variables_over_explicit_iterables_narrow_by_asserts_this() {
    let source = "class Base { assertDerived(): asserts this is Derived {} }
class Derived extends Base { z = 1; }
declare const values: Base[];
for (const receiver of values) { receiver.assertDerived(); receiver.z; }
const inferred = [new Base()];
for (const receiver of inferred) { receiver.assertDerived(); receiver.z; }
declare const nested: Base[][];
for (const row of nested) for (const receiver of row) { receiver.assertDerived(); receiver.z; }
for (const self of self) { self.assertDerived(); }";
    let reported = diagnostics(source);
    // Only the inferred iterable stays un-narrowed: its variable has no
    // explicit type, so the call has no effects signature. Chained for-of
    // variables resolve through each other.
    assert_eq!(
        reported.iter().filter(|(_, _, code)| *code == 2339).collect::<Vec<_>>(),
        [&(6, 71, 2339)],
    );
    // `for (const self of self)` ends at `resolvingExplicitTypeOfSymbol`.
    assert!(reported.contains(&(9, 20, 2448)), "{reported:?}");
}
