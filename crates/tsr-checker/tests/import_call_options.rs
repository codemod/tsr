//! Native import options diagnostics apply even for a valid specifier.
use tsr_checker::{Checker, check::FileContext};

#[test]
fn obsolete_assert_property_reports_after_valid_string_specifier() {
    let source = "declare const specifier: string; const old = import(specifier, { assert: { type: 'json' } });";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "options.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(
        parsed.source_file.node_id.unwrap(),
        FileContext { ambient: false, has_parse_errors: false },
    );
    let actual: Vec<_> = checker
        .diagnostics()
        .iter()
        .filter(|(_, diagnostic)| diagnostic.code() == "TS2880")
        .map(|(_, diagnostic)| diagnostic.text())
        .collect();
    assert_eq!(
        actual,
        [
            "Import assertions have been replaced by import attributes. Use 'with' instead of 'assert'."
        ]
    );
}
