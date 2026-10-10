//! `serializeTypeName` (`nodecopy.go:436-449`) for a reused annotation whose
//! module-member name is not in scope at the print site: native spells it
//! through `symbolToTypeNode` and keeps the written type arguments.
//! Outcomes checked against pinned tsgo 5b1047d (`--declaration`).
//! `docs/parity/notes/r6-specifiers2.md` §2.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/r6_specifiers2", "probe.ts", source);
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
fn reused_return_names_an_unimported_alias_through_its_module() {
    let lines = lines(
        r#"// @target: es2015
// @module: commonjs
// @filename: node_modules/sc/index.d.ts
export interface DefaultTheme {}
export type SC<TTag extends string, TTheme = DefaultTheme, TStyle = {}> = string & { tag: TTag; theme: TTheme; s: TStyle };
export interface SI {
    div: (a: TemplateStringsArray) => SC<"div">;
}
declare const styled: SI;
export default styled;
// @filename: index.ts
import styled from "sc";
export const d = styled.div;
"#,
    );
    let wanted = r#"d : (a: TemplateStringsArray) => import("sc").SC<"div">"#;
    assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
}
