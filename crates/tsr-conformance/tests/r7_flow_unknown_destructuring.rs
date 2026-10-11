//! TS2339 for a destructured name of an `unknown` parent
//! (getBindingElementTypeFromParentType, pinned checker.go:17707;
//! getPropertyTypeForIndexType, :27001). Expected positions are pinned tsgo
//! 5b1047d10d32e7d5b446be4de56b126ff42f82bb on the same source: the nested
//! `y` is silent (its parent is the error type), and the contextually typed
//! parameter has no error (native types it `{ cancelable: any; }`).

use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn unknown_parent_misses_every_destructured_name() {
    let source = "try {} catch ({ x }: unknown) { x }
try {} catch ({ a: { y } }: unknown) { y }
function wrap<Args extends unknown[]>(_: (...args: Args) => void) {}
wrap(({ cancelable } = {}) => {});";
    let mut case = TestCase::parse("probe/unknown-destructuring", "probe.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "true".into());
    let mut reported: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    reported.sort_unstable();
    assert_eq!(reported, [(1, 17, 2339), (2, 17, 2339)]);
}
