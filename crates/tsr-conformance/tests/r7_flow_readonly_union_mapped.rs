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

#[test]
fn bare_parameter_setters_keep_their_declaration_and_receiver_domains_separate() {
    // Expected diagnostics are pinned tsgo 5b1047d on this source. Bare W
    // differs from the getter R; each ordered receiver/base map must apply
    // before writes are checked, without changing reads or compound writes.
    let source = r#"interface Access<R, W> { get value(): R; set value(v: W); }
declare const numbers: Access<string, number>;
declare const strings: Access<number, string>;
numbers.value = 42;
numbers.value = true;
numbers["value"] = 42;
strings.value = "ok";
strings.value = 42;
const readString: string = numbers.value;
const readNumber: number = strings.value;
const wrongRead: number = numbers.value;
numbers.value += "suffix";
strings.value += 1;
function generic<T>(a: Access<number, T>, value: T) {
    a.value = value;
    a.value = 42;
}
class Base<R, W> { get value(): R { return null as any; } set value(v: W) {} }
class Flip<X, Y> extends Base<Y, X> {}
class Derived<X, Y> extends Flip<X, Y> {}
declare const derived: Derived<number, boolean>;
derived.value = 42;
derived.value = "bad";
const derivedRead: boolean = derived.value;
interface Child<T> extends Access<string, T> {}
declare const child: Child<boolean>;
child.value = false;
child.value = 42;
interface Other<T> { get value(): number; set value(v: T); }
function distinct(u: Access<string, boolean> | Other<string>) {
    u.value = true;
    u.value = "ok";
    u.value = 42;
}
function inheritedUnion(u: Child<boolean> | Other<string>) {
    u.value = true;
    u.value = "ok";
    u.value = 42;
}
interface Getter<T> { get value(): T; }
declare const readonly: Getter<string>;
readonly.value = "bad";
function sameRoot(s: Access<string, boolean> | Access<number, string>) {
    s.value = true;
    s.value = "ok";
    s.value = 42;
}
function merged(m: Access<string, boolean> | Access<string, number>) {
    m.value = true;
    m.value = 42;
    m.value = "bad";
}
function intersection(i: Access<string, number> & { tag: boolean }) {
    i.value = 42;
    i.value = true;
}
"#;
    let mut case = TestCase::parse("probe/bare-parameter-setters", "accessors.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "true".into());
    let reported: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    assert_eq!(
        reported,
        [
            (5, 1, 2322),
            (8, 1, 2322),
            (11, 7, 2322),
            (12, 1, 2322),
            (13, 1, 2322),
            (16, 5, 2322),
            (23, 1, 2322),
            (28, 1, 2322),
            (33, 5, 2322),
            (38, 5, 2322),
            (42, 10, 2540),
            (46, 5, 2322),
            (49, 5, 2322),
            (51, 5, 2322),
            (55, 5, 2322),
        ]
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    let prefixes = [
        ("numbers.value : ", vec!["number", "number", "string", "string", "string"]),
        ("strings.value : ", vec!["string", "string", "number", "number"]),
        ("a.value : ", vec!["T", "T"]),
        ("derived.value : ", vec!["number", "number", "boolean"]),
        ("child.value : ", vec!["boolean", "boolean"]),
        ("u.value : ", vec!["string | boolean"; 6]),
        ("s.value : ", vec!["string | boolean"; 3]),
        ("m.value : ", vec!["number"; 3]),
        ("i.value : ", vec!["number"; 2]),
    ];
    for (prefix, expected) in &prefixes {
        let actual: Vec<_> = lines.iter().filter_map(|line| line.strip_prefix(prefix)).collect();
        assert_eq!(actual, *expected, "{prefix}");
    }
    for query in lines.windows(3) {
        if let Some(expected) =
            prefixes.iter().find_map(|(prefix, _)| query[0].strip_prefix(prefix))
        {
            assert_eq!(query[2], format!("value : {expected}"));
        }
    }
}
