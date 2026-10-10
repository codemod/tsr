//! A generic overload candidate infers from an object-literal argument
//! checked under ITS parameter's context (`chooseOverload` →
//! `inferTypeArguments` → `checkExpressionWithContextualType`, uncached), so a
//! member literal under a type parameter with a primitive constraint keeps its
//! literal (`isLiteralOfContextualType`). `docs/parity/notes/r6-typesroots3.md`
//! §3. Expectations from the pinned `tsgo` (5b1047d).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Every assertion line the corpus pipeline produces for `source`.
fn lines(name: &str, source: &str) -> Vec<String> {
    let case = TestCase::parse(name, "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

fn assert_lines(name: &str, source: &str, wanted: &[&str]) {
    let lines = lines(name, source);
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn a_generic_overload_rechecks_an_object_literal_under_its_context() {
    let source = r#"// @strict: true
// @target: es2020
export const o = Object.freeze({ a: 1, b: "string", c: true });
declare function f2(x: number): 1;
declare function f2<T extends { [idx: string]: U }, U extends string | number>(o: T): T;
export const o2 = f2({ a: 1, b: "s" });
"#;
    assert_lines(
        "probe/overload-object-literal-context",
        source,
        &[r#"o : Readonly<{ a: 1; b: "string"; c: true; }>"#, r#"o2 : { a: 1; b: "s"; }"#],
    );
}
