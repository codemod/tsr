//! Completed generic construct-only types are not callable.
use tsr_checker::{Checker, check::FileContext};

#[test]
fn generic_constructor_input_does_not_hide_noncallable_diagnostic() {
    let source = "interface Constructor<T> { new (value: T): Date } function use<T>(constructor: Constructor<T>, value: T) { const invalid = constructor(value); const valid = new constructor(value); return valid; }";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "noncallable.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(
        parsed.source_file.node_id.unwrap(),
        FileContext { ambient: false, has_parse_errors: false },
    );
    let diagnostics: Vec<_> = checker
        .diagnostics()
        .iter()
        .filter(|(_, diagnostic)| diagnostic.code() == "TS2348")
        .map(|(_, diagnostic)| diagnostic.text())
        .collect();
    assert_eq!(
        diagnostics,
        ["Value of type 'Constructor<T>' is not callable. Did you mean to include 'new'?"]
    );
}
