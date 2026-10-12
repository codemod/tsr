//! checkIndexedAccessIndexType (pinned checker.go:8219): TS2542 for a
//! generic key written through a `readonly` mapped type, TS2536 for a
//! deferred access whose index is not assignable to the object's keys, and
//! TS4105 for a literal key naming a private member of a type parameter's
//! constraint. Expected positions are pinned tsgo
//! 5b1047d10d32e7d5b446be4de56b126ff42f82bb on the same source, less one:
//! native also reports `C`'s true-branch `P["ctx"]` at (13,39), which this
//! port declines (no substitution types, r7-flow §48). The mapped template
//! over an array-constrained variable (`B1`) is silent in both.

use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn index_type_reports_match_native() {
    let source = r#"function f<T>(x: T, y: Readonly<T>, k: keyof T) {
    x[k] = y[k];
    y[k] = x[k];
}
function g<T, U>(x: T | U, k: keyof T | keyof U) {
    x[k];
}
type AB = { a: 'a'; b: 'a' };
type T3 = { [key in AB[keyof AB]]: true };
type T4<K extends 'a' | 'b'> = T3[K];
type Obj = { [s: string]: number };
type B1<U extends string[]> = { [K in keyof U]: Obj[K] };
type C<P> = P["ctx"] extends object ? P["ctx"] : object;
class A { private a = 1; }
type X<T extends A> = T["a"];
"#;
    let mut case = TestCase::parse("probe/index-type-reports", "probe.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "true".into());
    let mut reported: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    reported.sort_unstable();
    assert_eq!(
        reported,
        [(3, 5, 2542), (6, 5, 2536), (10, 32, 2536), (13, 13, 2536), (15, 23, 4105)]
    );
}
