//! A class instance's TS2339 certificate when its base is generic, circular
//! or unresolved (`resolveBaseTypes`, pinned checker.go:19189). Expected
//! positions come from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.

use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn generic_circular_and_unresolved_bases_do_not_hide_a_miss() {
    let source = "class M<T> extends M<string> { }
(new M).blah;
class N0<T> { x = 1; }
class N extends N0<string> {}
(new N).blah;
(new N).x;
class U extends Missing {}
(new U).blah;";
    let mut case = TestCase::parse("probe/class-chain", "probe.ts", source);
    case.options.insert("target".into(), "es2015".into());
    let reported: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == 2339)
        .map(|diagnostic| (diagnostic.line, diagnostic.column))
        .collect();
    assert_eq!(reported, [(2, 9), (5, 9), (8, 9)]);
}
