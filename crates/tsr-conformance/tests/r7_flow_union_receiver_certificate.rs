//! A narrowed union receiver's TS2339 is certified when its flow walk met no
//! step that may have answered wider than native (r7-flow §49): `instanceof`
//! narrowing an `any` to `C1 | C2` reports, and a type-predicate narrowing
//! this port cannot decide (a memberless mapped predicate) stays silent,
//! where native narrows to `ArrayLike<any>` and reports nothing. Expected
//! positions are pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb on the
//! same source.

use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn narrowed_union_receiver_reports_only_when_certified() {
    let source = r"interface C1 { foo: string; bar1: number }
interface C2 { foo: string; bar2: number }
interface CConstructor { new (): C1; new (x: number): C2 }
declare var C: CConstructor;
declare var obj: any;
if (obj instanceof C) {
    obj.bar1;
}
declare function hasOwnProperty<P extends PropertyKey>(target: {}, property: P): target is { readonly [K in P]: unknown };
declare const arrayLikeOrIterable: ArrayLike<any> | Iterable<any>;
if (hasOwnProperty(arrayLikeOrIterable, 'length')) {
    arrayLikeOrIterable.length;
}
";
    let mut case = TestCase::parse("probe/union-receiver-certificate", "probe.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "true".into());
    let reported: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    assert_eq!(reported, [(7, 9, 2339)]);
}
