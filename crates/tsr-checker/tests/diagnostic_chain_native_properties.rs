//! Completed property relation failures retain native ordering and compression.
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
fn property_assignment_argument_and_nested_path_keep_first_failure() {
    let ds = diagnostics(
        "declare let source: { x: string; y: number }; let target: { x: number; y: string } = source; declare function accept(x: { x: number; y: string }): void; accept(source); declare let nested: { x: { y: string } }; let nestedTarget: { x: { y: number } } = nested;",
    );
    let actual = ds
        .iter()
        .map(|d| {
            let mut text = String::new();
            write_flattened_diagnostic_message(&mut text, d, "\n");
            (d.message.code(), text)
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, [
        (2322, "Type '{ x: string; y: number; }' is not assignable to type '{ x: number; y: string; }'.\n  Types of property 'x' are incompatible.\n    Type 'string' is not assignable to type 'number'.".into()),
        (2345, "Argument of type '{ x: string; y: number; }' is not assignable to parameter of type '{ x: number; y: string; }'.\n  Types of property 'x' are incompatible.\n    Type 'string' is not assignable to type 'number'.".into()),
        (2322, "Type '{ x: { y: string; }; }' is not assignable to type '{ x: { y: number; }; }'.\n  The types of 'x.y' are incompatible between these types.\n    Type 'string' is not assignable to type 'number'.".into()),
    ]);
    for d in &ds {
        assert_eq!(d.message_chain()[0].span, d.span);
        assert_eq!(d.message_chain()[0].message_chain()[0].span, d.span);
    }
}
#[test]
fn quoted_property_paths_and_signature_members_preserve_native_trees() {
    let ds = diagnostics(
        "declare let a: { \"x-y\": { z: string } }; let b: { \"x-y\": { z: number } } = a; declare let c: { x: { \"y-z\": string } }; let d: { x: { \"y-z\": number } } = c; declare let s: { x: (a: any, b: any) => {} }; let t: { x: (a: any) => {} } = s;",
    );
    let actual = ds
        .iter()
        .map(|d| {
            let mut text = String::new();
            write_flattened_diagnostic_message(&mut text, &d.message_chain()[0], "\n");
            text
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        [
            "The types of '[\"x-y\"].z' are incompatible between these types.\n  Type 'string' is not assignable to type 'number'.",
            "The types of 'x[\"y-z\"]' are incompatible between these types.\n  Type 'string' is not assignable to type 'number'.",
            "Types of property 'x' are incompatible.\n  Type '(a: any, b: any) => {}' is not assignable to type '(a: any) => {}'.\n    Target signature provides too few arguments. Expected 2 or more, but got 1.",
        ]
    );
}

#[test]
fn parameter_failure_wraps_the_actual_contravariant_object_chain() {
    let ds = diagnostics(
        "declare let s: (source: { x: number }) => void; let t: (target: { x: string }) => void = s; declare function accept(callback: (target: { x: string }) => void): void; accept(s);",
    );
    assert_eq!(ds.iter().map(|d| d.message.code()).collect::<Vec<_>>(), [2322, 2345]);
    for d in &ds {
        let mut text = String::new();
        write_flattened_diagnostic_message(&mut text, &d.message_chain()[0], "\n");
        assert_eq!(
            text,
            "Types of parameters 'source' and 'target' are incompatible.\n  Type '{ x: string; }' is not assignable to type '{ x: number; }'.\n    Types of property 'x' are incompatible.\n      Type 'string' is not assignable to type 'number'."
        );
        assert_eq!(d.message_chain()[0].message_chain()[0].span, d.span);
    }
}

#[test]
fn return_object_chain_and_property_return_marker_preserve_native_compression() {
    let ds = diagnostics(
        "declare let r: () => { x: string }; let q: () => { x: number } = r; declare let p: { f: () => { x: string } }; let o: { f: () => { x: number } } = p;",
    );
    let texts = ds
        .iter()
        .map(|d| {
            let mut text = String::new();
            write_flattened_diagnostic_message(&mut text, &d.message_chain()[0], "\n");
            text
        })
        .collect::<Vec<_>>();
    assert_eq!(
        texts,
        [
            "Type '{ x: string; }' is not assignable to type '{ x: number; }'.\n  Types of property 'x' are incompatible.\n    Type 'string' is not assignable to type 'number'.",
            "The types returned by 'f().x' are incompatible between these types.\n  Type 'string' is not assignable to type 'number'.",
        ]
    );
}

#[test]
fn outer_property_and_constructor_return_paths_reduce_in_native_order() {
    let ds = diagnostics(
        "declare let s: { x: { f: () => string } }; let t: { x: { f: () => number } } = s; declare let c: { make: new () => { value: string } }; let d: { make: new () => { value: number } } = c;",
    );
    let texts = ds
        .iter()
        .map(|d| {
            let mut text = String::new();
            write_flattened_diagnostic_message(&mut text, &d.message_chain()[0], "\n");
            text
        })
        .collect::<Vec<_>>();
    assert_eq!(
        texts,
        [
            "The types of 'x.f()' are incompatible between these types.\n  Type 'string' is not assignable to type 'number'.",
            "The types returned by '(new make()).value' are incompatible between these types.\n  Type 'string' is not assignable to type 'number'.",
        ]
    );
}

#[test]
fn nested_callback_parameters_retain_both_native_parameter_wrappers() {
    let ds = diagnostics(
        "declare let source: (callback: (value: {x: number}) => void) => void; let target: (callback: (value: {x: string}) => void) => void = source;",
    );
    assert_eq!(ds.iter().map(|d| d.message.code()).collect::<Vec<_>>(), [2322]);
    let mut text = String::new();
    write_flattened_diagnostic_message(&mut text, &ds[0].message_chain()[0], "\n");
    assert_eq!(
        text,
        "Types of parameters 'callback' and 'callback' are incompatible.\n  Types of parameters 'value' and 'value' are incompatible.\n    Type '{ x: number; }' is not assignable to type '{ x: string; }'.\n      Types of property 'x' are incompatible.\n        Type 'number' is not assignable to type 'string'."
    );
}

#[test]
fn callback_arity_failure_is_wrapped_by_its_parameter_context() {
    let ds = diagnostics(
        "declare let source: (cb: (a: any) => void) => void; let target: (cb: (a: any, b: any) => void) => void = source;",
    );
    assert_eq!(ds.iter().map(|d| d.message.code()).collect::<Vec<_>>(), [2322]);
    let mut text = String::new();
    write_flattened_diagnostic_message(&mut text, &ds[0].message_chain()[0], "\n");
    assert_eq!(
        text,
        "Types of parameters 'cb' and 'cb' are incompatible.\n  Target signature provides too few arguments. Expected 2 or more, but got 1."
    );
}

#[test]
fn signature_valued_return_arity_retains_return_relation_and_property_marker() {
    let ds = diagnostics(
        "declare let source: () => (x: any, y: any) => {}; let target: () => (x: any) => {} = source; declare let s: {make: () => (x: any, y: any) => {}}; let t: {make: () => (x: any) => {}} = s;",
    );
    let texts = ds
        .iter()
        .map(|d| {
            let mut text = String::new();
            write_flattened_diagnostic_message(&mut text, &d.message_chain()[0], "\n");
            text
        })
        .collect::<Vec<_>>();
    assert_eq!(
        texts,
        [
            "Type '(x: any, y: any) => {}' is not assignable to type '(x: any) => {}'.\n  Target signature provides too few arguments. Expected 2 or more, but got 1.",
            "The types returned by 'make()' are incompatible between these types.\n  Type '(x: any, y: any) => {}' is not assignable to type '(x: any) => {}'.\n    Target signature provides too few arguments. Expected 2 or more, but got 1.",
        ]
    );
}

#[test]
fn source_type_parameter_primitive_constraint_failure_retains_inner_relation() {
    let ds = diagnostics(
        "function failed<U extends string>(value: U): number { return value; } function argument<U extends string>(value: U) { accept(value); } declare function accept(value: number): void;",
    );
    assert_eq!(ds.iter().map(|d| d.message.code()).collect::<Vec<_>>(), [2322, 2345]);
    for d in ds {
        assert_eq!(
            d.message_chain().iter().map(Diagnostic::text).collect::<Vec<_>>(),
            ["Type 'string' is not assignable to type 'number'."]
        );
        assert_eq!(d.message_chain()[0].span, d.span);
    }
}

#[test]
fn immediate_source_constraints_are_not_flattened_to_terminal_type() {
    let ds = diagnostics(
        "function f<T extends string, U extends T>(value: U): number { return value; } function g<T extends string, U extends T>(value: U) { accept(value); } declare function accept(value: number): void;",
    );
    assert_eq!(ds.iter().map(|d| d.message.code()).collect::<Vec<_>>(), [2322, 2345]);
    for d in ds {
        let mut text = String::new();
        write_flattened_diagnostic_message(&mut text, &d.message_chain()[0], "\n");
        assert_eq!(
            text,
            "Type 'T' is not assignable to type 'number'.\n  Type 'string' is not assignable to type 'number'."
        );
    }
}

#[test]
fn signature_this_failure_retains_native_receiver_chain() {
    let ds = diagnostics(
        "declare let source: (this: { x: number }) => void; let target: (this: { x: string }) => void = source;",
    );
    assert_eq!(ds.iter().map(|d| d.message.code()).collect::<Vec<_>>(), [2322]);
    let mut text = String::new();
    write_flattened_diagnostic_message(&mut text, &ds[0].message_chain()[0], "\n");
    assert_eq!(
        text,
        "The 'this' types of each signature are incompatible.\n  Type '{ x: string; }' is not assignable to type '{ x: number; }'.\n    Types of property 'x' are incompatible.\n      Type 'string' is not assignable to type 'number'."
    );
}

#[test]
fn signature_valued_this_arity_retains_receiver_wrapper_and_inner_relation() {
    let ds = diagnostics(
        "declare let source: (this: (a: any) => void) => void; let target: (this: (a: any, b: any) => void) => void = source;",
    );
    assert_eq!(ds.iter().map(|d| d.message.code()).collect::<Vec<_>>(), [2322]);
    let mut text = String::new();
    write_flattened_diagnostic_message(&mut text, &ds[0].message_chain()[0], "\n");
    assert_eq!(
        text,
        "The 'this' types of each signature are incompatible.\n  Type '(a: any, b: any) => void' is not assignable to type '(a: any) => void'.\n    Target signature provides too few arguments. Expected 2 or more, but got 1."
    );
}

#[test]
fn spread_entries_do_not_disable_explicit_object_member_elaboration() {
    let source = "let target: {x: number} = {...{}, x: \"wrong\"};";
    let ds = diagnostics(source);
    assert_eq!(ds.iter().map(|d| d.message.code()).collect::<Vec<_>>(), [2322]);
    assert_eq!(ds[0].text(), "Type 'string' is not assignable to type 'number'.");
    assert!(ds[0].message_chain().is_empty());
    assert_eq!(ds[0].span.start as usize, source.find("\"wrong\"").unwrap());
}

#[test]
fn compatible_properties_and_overload_alternative_do_not_publish_failed_chains() {
    let ds = diagnostics(
        "declare let source: { x: number }; let target: { x: number } = source; declare function f(x: { a: string }): void; declare function f(x: { a: number }): void; f({a: 1});",
    );
    assert!(ds.is_empty(), "{ds:?}");
}
