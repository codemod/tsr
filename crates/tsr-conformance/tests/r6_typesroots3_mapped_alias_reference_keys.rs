//! A homomorphic mapping over an alias whose body is another generic alias's
//! reference (`Omit<T, K>` = `Pick<T, Exclude<keyof T, K>>`), or over an
//! intersection holding one, enumerates the modifiers type's properties
//! (`resolveMappedTypeMembers` → `forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType`,
//! checker.go:22727). `docs/parity/notes/r6-typesroots3.md` §2. Every
//! expectation was read from the pinned `tsgo` (5b1047d) first.
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
fn a_mapping_over_an_alias_reference_and_its_intersection_enumerates_keys() {
    let source = r#"// @strict: true
// @target: es2020
type M = { a: 1; b: 2; c: 3 };
declare const om: { [K in keyof Omit<M, "a">]: K };
export const omB = om.b;
declare const inter: { [K in keyof (Omit<M, "c"> & Partial<Pick<M, "c">>)]: K };
export const interA = inter.a;
export const interC = inter.c;
type GS<K extends string> = Record<K, number>;
export async function f<K extends string>(p: Promise<GS<K>>, k: K) { const s = await p; return s[k]; }
"#;
    assert_lines(
        "probe/mapped-alias-reference-keys",
        source,
        &[
            r#"om : { b: "b"; c: "c"; }"#,
            r#"omB : "b""#,
            r#"interA : "a""#,
            r#"interC : "c" | undefined"#,
            "f : <K extends string>(p: Promise<GS<K>>, k: K) => Promise<GS<K>[K]>",
        ],
    );
}
