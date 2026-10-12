//! getPropertyTypeForIndexType's noImplicitAny arm (pinned checker.go:27147):
//! TS7052 for a `get`/`set` member that takes the key, one TS7053 per union
//! key (`AccessFlagsSuppressNoImplicitAnyError`) naming an enum constituent
//! `[E.p]`; a `null` destructuring parent; and a class whose computed member
//! name can never be late-bound (`[this.w]`) still certifies its misses.
//! Expected positions and texts are pinned tsgo
//! 5b1047d10d32e7d5b446be4de56b126ff42f82bb on the same source.

use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn index_suggestions_and_misses_match_native() {
    let source = r#"declare const c: { get: (key: string) => number };
c["k"];
const o = { a: 0 };
declare const k: "a" | "b" | "c";
o[k];
enum E { p, q }
declare const ek: E;
o[ek];
const { z } = null;
class D {
    [this.w] = 1;
    m() { this.w; }
}
"#;
    let mut case = TestCase::parse("probe/index-suggestions", "probe.ts", source);
    case.options.insert("target".into(), "es2022".into());
    case.options.insert("strict".into(), "true".into());
    let mut reported: Vec<_> = diagnostics_suite::rendered_for(&case)
        .into_iter()
        .map(|((_, line, column, code), message)| (line, column, code, message))
        .collect();
    reported.sort();
    let expected: Vec<(u32, u32, u32, String)> = vec![
        (2, 1, 7052, "Element implicitly has an 'any' type because type '{ get: (key: string) => number; }' has no index signature. Did you mean to call 'c.get'?".into()),
        (5, 1, 7053, "Element implicitly has an 'any' type because expression of type '\"a\" | \"b\" | \"c\"' can't be used to index type '{ a: number; }'.".into()),
        (8, 1, 7053, "Element implicitly has an 'any' type because expression of type 'E' can't be used to index type '{ a: number; }'.".into()),
        (9, 9, 2339, "Property 'z' does not exist on type 'null'.".into()),
        (11, 5, 1166, "A computed property name in a class property declaration must have a simple literal type or a 'unique symbol' type.".into()),
        (11, 6, 2465, "'this' cannot be referenced in a computed property name.".into()),
        (11, 11, 2339, "Property 'w' does not exist on type 'D'.".into()),
        (12, 16, 2339, "Property 'w' does not exist on type 'D'.".into()),
    ];
    assert_eq!(reported, expected);
}
