//! Native generic-target explanations, distinct constraints, and inherited spans.

use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;
use tsr_diagnostics::{Diagnostic, format::write_flattened_diagnostic_message};

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker.diagnostics().iter().map(|(_, diagnostic)| diagnostic.clone()).collect()
}

#[test]
fn type_parameter_failure_preserves_native_constraint_explanation_and_span() {
    let source = "function unconstrained<T>(x: number): T { return x; }\n\
                  function constrained<T extends number>(x: number): T { return x; }\n\
                  function literalConstraint<T extends 1>(x: 1): T { return x; }\n\
                  function unrelated<T extends string>(x: number): T { return x; }\n";
    let diagnostics = diagnostics(source);
    let actual = diagnostics
        .iter()
        .map(|diagnostic| {
            let mut text = String::new();
            write_flattened_diagnostic_message(&mut text, diagnostic, "\n");
            (diagnostic.message.code(), text)
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, [
        (2322, "Type 'number' is not assignable to type 'T'.\n  'T' could be instantiated with an arbitrary type which could be unrelated to 'number'.".into()),
        (2322, "Type 'number' is not assignable to type 'T'.\n  'number' is assignable to the constraint of type 'T', but 'T' could be instantiated with a different subtype of constraint 'number'.".into()),
        (2322, "Type '1' is not assignable to type 'T'.\n  '1' is assignable to the constraint of type 'T', but 'T' could be instantiated with a different subtype of constraint '1'.".into()),
        (2322, "Type 'number' is not assignable to type 'T'.\n  'T' could be instantiated with an arbitrary type which could be unrelated to 'number'.".into()),
    ]);
    assert_eq!(
        diagnostics.iter().map(|d| d.message_chain()[0].message.code()).collect::<Vec<_>>(),
        [5082, 5075, 5075, 5082]
    );
    for diagnostic in &diagnostics {
        assert_eq!(diagnostic.message_chain()[0].span, diagnostic.span);
    }
}

#[test]
fn primitive_failure_remains_head_only_and_valid_generic_return_stays_clean() {
    let diagnostics = diagnostics(
        "function f(x: string): number { return x; } function g<T>(x: T): T { return x; }",
    );
    assert_eq!(
        diagnostics.iter().map(Diagnostic::text).collect::<Vec<_>>(),
        ["Type 'string' is not assignable to type 'number'."]
    );
    assert!(diagnostics[0].message_chain().is_empty());
}
