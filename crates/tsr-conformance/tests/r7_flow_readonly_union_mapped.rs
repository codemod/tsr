//! A union property's `CheckFlagsReadonly` reads each constituent's mapped
//! member, not the modifiers type's symbol (createUnionOrIntersectionProperty,
//! pinned checker.go:21452; resolveMappedTypeMembers, :20930-20937).
//!
//! Expected TS2540 positions come from pinned tsgo
//! 5b1047d10d32e7d5b446be4de56b126ff42f82bb on the same source.

use tsr_conformance::{TestCase, diagnostics_suite, types_baseline::FileTypes, types_producer};

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

#[test]
fn union_accessor_writes_use_setters_without_changing_reads_or_receiver_maps() {
    // Native unions setter types, even when one getter/setter pair is not
    // divergent (divergentAccessorsTypes3/8). Write-only expression and name
    // queries must agree with assignment diagnostics; compound writes still
    // read the getter. Expected errors are pinned tsgo 5b1047d on this source.
    let source = r#"class Wide { get value(): string { return ""; } set value(v: string | number) {} }
class Narrow { get value(): string { return ""; } set value(v: string) {} }
function plain(u: Wide | Narrow) {
    u.value = 42;
    u.value = true;
    u["value"] = 42;
    u["value"] = true;
    const read: string = u.value;
    const wrongRead: number = u.value;
    u.value += 1;
    u.value -= 1;
}
class Inherited extends Wide {}
function inherited(u: Inherited | Narrow) { u.value = 42; u.value = true; }
class GetterOnly { get value(): string { return ""; } }
function readonly(r: Wide | GetterOnly) { r.value = 42; }
interface A<T> { get value(): T; set value(v: T | number); }
interface B<T> { get value(): T; set value(v: T | string); }
function mapped(m: A<boolean> | B<boolean>) {
    m.value = true;
    m.value = 42;
    m.value = "hello";
    m.value = {};
    const read: boolean = m.value;
}
class Generic<T> { get value(): T { return null as any; } set value(v: T) {} }
class Derived<T> extends Generic<T> {}
function generic(g: Derived<number>) { g.value = 42; g.value = true; }
"#;
    let mut case = TestCase::parse("probe/union-accessor-writes", "accessors.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "true".into());
    let reported: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    assert_eq!(
        reported,
        [
            (5, 5, 2322),
            (7, 5, 2322),
            (9, 11, 2322),
            (11, 5, 2362),
            (14, 59, 2322),
            (16, 45, 2540),
            (23, 5, 2322),
            (28, 54, 2322),
        ]
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for (prefix, expected) in [
        (
            "u.value : ",
            vec![
                "string | number",
                "string | number",
                "string",
                "string",
                "string",
                "string",
                "string | number",
                "string | number",
            ],
        ),
        (
            "m.value : ",
            vec![
                "string | number | boolean",
                "string | number | boolean",
                "string | number | boolean",
                "string | number | boolean",
                "boolean",
            ],
        ),
        ("g.value : ", vec!["number", "number"]),
    ] {
        let actual: Vec<_> = lines.iter().filter_map(|line| line.strip_prefix(prefix)).collect();
        assert_eq!(actual, expected, "{prefix}");
    }
    // In the public .types stream each access is followed by its receiver
    // and member name. The member-name query must report the same write/read
    // type, rather than falling back to the getter or declaration's raw T.
    for query in lines.windows(3) {
        if let Some(expected) = ["u.value : ", "m.value : ", "g.value : "]
            .iter()
            .find_map(|prefix| query[0].strip_prefix(prefix))
        {
            assert_eq!(query[2], format!("value : {expected}"));
        }
    }
}
