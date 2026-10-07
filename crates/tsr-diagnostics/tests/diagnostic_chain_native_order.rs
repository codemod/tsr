//! Native chain ordering, equality, rendering, and independent related locations.

use std::{cmp::Ordering, sync::Arc};
use tsr_core::Span;
use tsr_diagnostics::format::{
    write_flattened_diagnostic_message, write_format_diagnostic_with_color_and_context,
};
use tsr_diagnostics::{
    Diagnostic, DiagnosticFile, FormattingOptions, LocatedDiagnostic, compare_diagnostics,
    equal_diagnostics, equal_diagnostics_no_related_info, messages,
};

fn head() -> Diagnostic {
    Diagnostic::with_args(
        &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1,
        Span::new(0, 1),
        ["A".into(), "B".into()],
    )
}
fn child(arg: &str) -> Diagnostic {
    Diagnostic::with_args(&messages::_0_EXPECTED, Span::new(0, 1), [arg.into()])
}

#[test]
fn tree_shape_precedes_content_and_longer_explanations_sort_first() {
    let mut a = head();
    a.add_message_chain(Some(child("z")));
    let mut b = head();
    b.add_message_chain(Some(child("a")));
    b.add_message_chain(Some(child("b")));
    assert_eq!(compare_diagnostics(&a, &b), Ordering::Greater);
    let mut nested = head();
    let mut c = child("z");
    c.add_message_chain(Some(child("z")));
    nested.add_message_chain(Some(c));
    assert_eq!(compare_diagnostics(&nested, &a), Ordering::Less);
    assert_eq!(compare_diagnostics(&a, &nested), Ordering::Greater);
}

#[test]
fn comparison_ignores_child_codes_but_equality_does_not() {
    let mut a = head();
    a.add_message_chain(Some(child("x")));
    let mut b = head();
    b.add_message_chain(Some(Diagnostic::with_args(
        &messages::CANNOT_FIND_NAME_0,
        Span::new(9, 10),
        ["x".into()],
    )));
    assert_eq!(compare_diagnostics(&a, &b), Ordering::Equal);
    assert!(!equal_diagnostics(&a, &b));
    b.set_message_chain(vec![Diagnostic::with_args(
        &messages::_0_EXPECTED,
        Span::new(9, 10),
        ["x".into()],
    )]);
    assert!(equal_diagnostics(&a, &b));
}

#[test]
fn wrapping_inherits_location_and_related_info_without_conflating_siblings() {
    let file = Arc::new(DiagnosticFile::new("/a.ts", "x"));
    let mut c = child("x");
    c.set_file(file);
    c.add_related_information(Some(child("related")));
    let mut parent = Diagnostic::new_chain(
        Some(c),
        &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1,
        ["A".into(), "B".into()],
    );
    assert_eq!(parent.span, Span::new(0, 1));
    assert_eq!(parent.file().unwrap().file_name(), "/a.ts");
    parent.add_related_information(Some(child("parent only")));
    assert_eq!(
        parent.related_information().iter().map(Diagnostic::text).collect::<Vec<_>>(),
        ["'related' expected.", "'parent only' expected."]
    );
    assert_eq!(
        parent.message_chain()[0]
            .related_information()
            .iter()
            .map(Diagnostic::text)
            .collect::<Vec<_>>(),
        ["'related' expected."]
    );
    let mut clone = parent.clone();
    clone.set_related_information(Arc::new(Vec::new()));
    assert!(equal_diagnostics_no_related_info(&parent, &clone));
    assert!(!equal_diagnostics(&parent, &clone));
    assert_eq!(compare_diagnostics(&parent, &clone), Ordering::Less);
}

#[test]
fn flattening_preserves_sibling_order_nesting_and_requested_newline() {
    let mut parent = head();
    let mut c = child("first");
    c.add_message_chain(Some(child("nested")));
    parent.add_message_chain(Some(c)).add_message_chain(Some(child("second")));
    let mut output = String::new();
    write_flattened_diagnostic_message(&mut output, &parent, "\r\n");
    assert_eq!(
        output,
        "Type 'A' is not assignable to type 'B'.\r\n  'first' expected.\r\n    'nested' expected.\r\n  'second' expected."
    );
}

#[test]
fn related_information_pretty_output_uses_its_own_source_location() {
    let file = Arc::new(DiagnosticFile::new("/b.ts", "x\ny"));
    let mut related =
        Diagnostic::with_args(&messages::_0_EXPECTED, Span::new(2, 3), ["related".into()]);
    related.set_file(file);
    let mut parent = head();
    parent.add_related_information(Some(related));
    let options = FormattingOptions::new("/".into(), true);
    let mut output = String::new();
    write_format_diagnostic_with_color_and_context(
        &mut output,
        &LocatedDiagnostic::global(&parent),
        &options,
    );
    assert!(output.contains("\n  \u{1b}[96mb.ts\u{1b}[0m:\u{1b}[93m2\u{1b}[0m:\u{1b}[93m1\u{1b}[0m - 'related' expected."), "{output:?}");
    assert!(output.contains("    \u{1b}[7m2\u{1b}[0m y"), "{output:?}");
    let plain =
        tsr_diagnostics::format_diagnostics(&[LocatedDiagnostic::global(&parent)], &options);
    assert_eq!(plain, "error TS2322: Type 'A' is not assignable to type 'B'.\n");
}
