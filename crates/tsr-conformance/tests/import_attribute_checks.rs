//! `checkImportAttributes`'s relation against `ImportAttributes` (TS2322) and
//! `checkExternalImportOrExportDeclaration`'s attribute-value loop (TS2858),
//! through the corpus pipeline with the bundled libs. Every expectation was
//! read off the pinned native tsgo (`5b1047d`) on the same source.
//! `docs/parity/notes/r6-triage.md` §4.

use tsr_conformance::{TestCase, diagnostics_suite};

fn rendered(source: &str) -> Vec<(u32, u32, u32, String)> {
    let case = TestCase::parse("probe/import-attribute-checks", "main.ts", source);
    let mut out: Vec<_> = diagnostics_suite::rendered_for(&case)
        .into_iter()
        .filter(|((file, ..), _)| file.ends_with("main.ts"))
        .map(|((_, line, column, code), text)| (line, column, code, text))
        .collect();
    out.sort();
    out
}

const PRELUDE: &str =
    "// @module: esnext\n// @filename: a.ts\nexport default {};\n// @filename: main.ts\n";

#[test]
fn non_string_values_report_the_relation_and_ts2858() {
    let source = format!(
        "{PRELUDE}import * as t1 from \"./a\" with {{ field: 0 }};\nimport * as t2 from \"./a\" with {{ type: \"json\", x: 1 }};\nimport * as t3 from \"./a\" with {{ type: \"json\" }};\nvoid t1; void t2; void t3;\n"
    );
    let got = rendered(&source);
    assert_eq!(
        got,
        [
            (
                1,
                27,
                2322,
                "Type '{ field: 0; }' is not assignable to type 'ImportAttributes'.".to_owned()
            ),
            (1, 41, 2858, "Import attribute values must be string literal expressions.".to_owned()),
            (
                2,
                27,
                2322,
                "Type '{ type: \"json\"; x: 1; }' is not assignable to type 'ImportAttributes'."
                    .to_owned()
            ),
            (2, 51, 2858, "Import attribute values must be string literal expressions.".to_owned()),
        ]
    );
}

#[test]
fn a_global_augmentation_constrains_attribute_values() {
    let source = format!(
        "{PRELUDE}declare global {{ interface ImportAttributes {{ type: \"json\" }} }}\nimport * as ns from \"./a\" with {{ type: \"not-json\" }};\nexport * from \"./a\" with {{ type: \"json\" }};\nvoid ns;\n"
    );
    let got = rendered(&source);
    assert_eq!(
        got,
        [(
            2,
            27,
            2322,
            "Type '{ type: \"not-json\"; }' is not assignable to type 'ImportAttributes'."
                .to_owned()
        )]
    );
}

#[test]
fn the_relation_stands_where_module_kind_rejects_attributes() {
    // `--module commonjs`: the TS2823 grammar arm returns after the relation.
    let source = "// @module: commonjs\n// @filename: a.ts\nexport default {};\n// @filename: main.ts\nimport * as t1 from \"./a\" with { field: 0 };\nvoid t1;\n";
    let codes: Vec<u32> = rendered(source).into_iter().map(|(_, _, code, _)| code).collect();
    assert_eq!(codes, [2322, 2823, 2858]);
}
