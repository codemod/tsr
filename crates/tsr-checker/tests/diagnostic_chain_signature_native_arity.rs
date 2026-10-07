//! Signature arity failures originate in the relation worker, not a call-site pass.
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
    checker.diagnostics().iter().map(|(_, d)| d.clone()).collect()
}

#[test]
fn failed_signature_assignment_and_argument_preserve_native_explanation() {
    let ds = diagnostics(
        "let target: (x: any) => {}; declare let source: (x: any, y: any) => {}; target = source; declare function accept(callback: (x: any) => {}): void; accept(source);",
    );
    let texts = ds
        .iter()
        .map(|d| {
            let mut text = String::new();
            write_flattened_diagnostic_message(&mut text, d, "\n");
            (d.message.code(), text)
        })
        .collect::<Vec<_>>();
    assert_eq!(texts, [
        (2322,"Type '(x: any, y: any) => {}' is not assignable to type '(x: any) => {}'.\n  Target signature provides too few arguments. Expected 2 or more, but got 1.".into()),
        (2345,"Argument of type '(x: any, y: any) => {}' is not assignable to parameter of type '(x: any) => {}'.\n  Target signature provides too few arguments. Expected 2 or more, but got 1.".into()),
    ]);
    for d in ds {
        assert_eq!(d.message_chain()[0].span, d.span);
    }
}

#[test]
fn call_failure_precedes_construct_failure_and_owns_the_explanation() {
    let ds = diagnostics(
        "declare let source: { (a: any, b: any): {}; new(a: any, b: any, c: any): {}; }; let target: { (a: any): {}; new(a: any): {}; } = source;",
    );
    assert_eq!(ds.iter().map(|d| d.message.code()).collect::<Vec<_>>(), [2322]);
    assert_eq!(
        ds[0].message_chain().iter().map(Diagnostic::text).collect::<Vec<_>>(),
        ["Target signature provides too few arguments. Expected 2 or more, but got 1."]
    );
}

#[test]
fn first_failed_target_overload_owns_the_arity_explanation() {
    let ds = diagnostics(
        "declare let source: (a: any, b: any, c: any) => {}; let target: { (a: any): {}; (a: any, b: any): {}; } = source;",
    );
    assert_eq!(ds.iter().map(|d| d.message.code()).collect::<Vec<_>>(), [2322]);
    assert_eq!(
        ds[0].message_chain().iter().map(Diagnostic::text).collect::<Vec<_>>(),
        ["Target signature provides too few arguments. Expected 3 or more, but got 1."]
    );
}

#[test]
fn optional_parameter_rest_target_and_compatible_overload_do_not_leak_failure() {
    let ds = diagnostics(
        "declare let optional: (x: any, y?: any) => {}; let target: (x: any) => {} = optional; declare let restTarget: (...args: any[]) => {}; declare let source: (x: any, y: any) => {}; restTarget = source; declare function overloaded(x: any, y: any): {}; declare function overloaded(x: any): {}; target = overloaded;",
    );
    assert_eq!(ds.iter().map(Diagnostic::text).collect::<Vec<_>>(), Vec::<String>::new());
}
