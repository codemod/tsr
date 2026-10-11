//! `symbolToTypeNode`'s portability report (`nodebuilderimpl.go:681`)
//! follows an import type re-spelled from `getSymbolChain`: an alias written
//! `import('./inner').SomeType` in a package's index is printed through the
//! unexported `dist/inner` path in an inferred return type, which TS2883
//! reports (`docs/parity/notes/r7-printer.md` §28). The expected diagnostic
//! was read from pinned native tsgo 5b1047d's `declarationEmitUsingTypeAlias1`
//! baseline.
use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn an_inferred_type_respelled_through_an_unexported_path_reports_ts2883() {
    let case = TestCase::parse(
        "probe/unsafe_import_respelled_chain",
        "src/index.ts",
        r#"// @target: es2015
// @strict: true
// @declaration: true
// @module: nodenext
// @filename: node_modules/some-dep/dist/inner.d.ts
export declare type Other = { other: string };
export declare type SomeType = { arg: Other };
// @filename: node_modules/some-dep/dist/index.d.ts
export type OtherType = import('./inner').Other;
export type SomeType = import('./inner').SomeType;
// @filename: node_modules/some-dep/package.json
{
  "name": "some-dep",
  "exports": {
    ".": "./dist/index.js"
  }
}
// @filename: src/index.ts
import { SomeType } from "some-dep";

export const foo = (thing: SomeType) => {
  return thing;
};
"#,
    );
    let got: Vec<_> = diagnostics_suite::rendered_for(&case)
        .into_iter()
        .filter(|((file, ..), _)| file.ends_with("src/index.ts"))
        .map(|((_, line, column, code), text)| (line, column, code, text))
        .collect();
    assert_eq!(
        got,
        [(
            3,
            14,
            2883,
            "The inferred type of 'foo' cannot be named without a reference to 'SomeType' from \
             '../node_modules/some-dep/dist/inner'. This is likely not portable. A type \
             annotation is necessary."
                .to_owned()
        )]
    );
}
