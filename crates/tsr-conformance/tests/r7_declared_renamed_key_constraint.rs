//! r7-declared §49: a mapped constraint written as a renamed import of a
//! literal-union alias enumerates its keys, checked against pinned native
//! tsgo 5b1047d (declarationEmitAliasFromIndirectFile).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r#"// @strict: true
// @module: commonjs
// @filename: locale.d.ts
export type CustomLocale = { w: string };
export type key = "ar" | "bg";
// @filename: instance.d.ts
import { CustomLocale, key as LocaleKey } from "./locale";
export interface F {
    l10ns: { [k in LocaleKey]?: CustomLocale } & { d: 1 };
}
declare const fp: F;
export const z = fp.l10ns;
"#;

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/r7_declared_renamed", "r7_declared_renamed.ts", source);
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

#[test]
fn a_renamed_literal_union_constraint_enumerates_its_keys() {
    let wanted =
        "z : { ar?: CustomLocale | undefined; bg?: CustomLocale | undefined; } & { d: 1; }";
    let lines = assertions(SOURCE);
    assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
}
