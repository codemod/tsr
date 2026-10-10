//! A union property's `CheckFlagsReadonly` reads each constituent's mapped
//! member, not the modifiers type's symbol (createUnionOrIntersectionProperty,
//! pinned checker.go:21452; resolveMappedTypeMembers, :20930-20937).
//!
//! Expected TS2540 positions come from pinned tsgo
//! 5b1047d10d32e7d5b446be4de56b126ff42f82bb on the same source.

use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn union_of_mapped_constituents_reads_each_mapped_modifier() {
    let source = "type Mutable<T> = { -readonly [K in keyof T]: T[K] };
interface A { readonly flags: number; kind: \"a\" }
interface B { readonly flags: number; kind: \"b\" }
declare const m: Mutable<A> | Mutable<B>;
m.flags = 1;
declare const r: Readonly<A> | A;
r.flags = 1;
r.kind = \"a\";
declare const ab: A | B;
ab.flags = 1;
declare const mr: Mutable<B> | Readonly<A>;
mr.flags = 1;
mr.kind = \"a\";";
    let mut case = TestCase::parse("probe/readonly-union-mapped", "probe.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "true".into());
    let mut reported: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    reported.sort_unstable();
    assert_eq!(reported, [(7, 3, 2540), (8, 3, 2540), (10, 4, 2540), (12, 4, 2540), (13, 4, 2540)]);
}
