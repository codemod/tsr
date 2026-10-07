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
fn runtime_payload_overrides_catalogue_and_wrapping_initializes_a_new_parent() {
    let plain = head();
    let mut changed = plain.clone();
    changed
        .set_category(tsr_diagnostics::Category::Suggestion)
        .set_reports_unnecessary(true)
        .set_reports_deprecated(true);
    assert_eq!(changed.category(), tsr_diagnostics::Category::Suggestion);
    assert!(changed.clone().reports_unnecessary());
    assert!(changed.clone().reports_deprecated());
    assert_eq!(compare_diagnostics(&plain, &changed), Ordering::Equal);
    assert!(equal_diagnostics(&plain, &changed));
    let options = FormattingOptions::new("/".into(), true);
    assert_eq!(
        tsr_diagnostics::format_diagnostics(&[LocatedDiagnostic::global(&changed)], &options),
        "suggestion TS2322: Type 'A' is not assignable to type 'B'.\n"
    );
    let parent = Diagnostic::new_chain(
        Some(changed),
        &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1,
        ["A".into(), "B".into()],
    );
    assert_eq!(parent.category(), tsr_diagnostics::Category::Error);
    assert!(!parent.reports_unnecessary());
    assert!(!parent.reports_deprecated());
    assert!(parent.message_chain()[0].reports_unnecessary());
    let mut pretty = String::new();
    write_format_diagnostic_with_color_and_context(
        &mut pretty,
        &LocatedDiagnostic::global(&parent.message_chain()[0]),
        &options,
    );
    assert!(pretty.contains("suggestion"), "{pretty:?}");
}

#[test]
fn no_emit_policy_is_per_diagnostic_and_clone_retains_but_parent_does_not_inherit() {
    let plain = head();
    let mut marked = plain.clone();
    marked.set_skipped_on_no_emit();
    assert!(!plain.skipped_on_no_emit());
    assert!(marked.skipped_on_no_emit());
    assert!(marked.clone().skipped_on_no_emit());
    assert_eq!(compare_diagnostics(&plain, &marked), Ordering::Equal);
    assert!(equal_diagnostics(&plain, &marked));
    let parent = Diagnostic::new_chain(
        Some(marked),
        &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1,
        ["A".into(), "B".into()],
    );
    assert!(!parent.skipped_on_no_emit());
    assert!(parent.message_chain()[0].skipped_on_no_emit());
}

#[test]
fn pinned_go_string_order_is_utf8_not_utf16() {
    // Go strings.Compare/slices.Compare(string) order bytes: U+E000 precedes
    // U+10000. UTF-16 would invert this pair (D800 precedes E000).
    let left = Diagnostic::with_args(&messages::_0_EXPECTED, Span::new(0, 1), ["\u{e000}".into()]);
    let right =
        Diagnostic::with_args(&messages::_0_EXPECTED, Span::new(0, 1), ["\u{10000}".into()]);
    assert_eq!(compare_diagnostics(&left, &right), Ordering::Less);
    let mut nested_left = head();
    nested_left.add_message_chain(Some(left));
    let mut nested_right = head();
    nested_right.add_message_chain(Some(right));
    assert_eq!(compare_diagnostics(&nested_left, &nested_right), Ordering::Less);
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
fn duplicate_heads_merge_sorted_unique_related_info_but_distinct_chains_survive() {
    let mut first = head();
    first.add_message_chain(Some(child("explanation")));
    first.add_related_information(Some(child("z")));
    first.add_related_information(Some(child("a")));
    let mut second = head();
    second.add_message_chain(Some(child("explanation")));
    second.add_related_information(Some(child("a")));
    second.add_related_information(Some(child("b")));
    let mut distinct = head();
    distinct.add_message_chain(Some(child("different")));
    let mut sorted = vec![second.clone(), distinct, first, second];
    sorted.sort_unstable_by(compare_diagnostics);
    let result = tsr_diagnostics::compact_and_merge_related_infos(sorted);
    assert_eq!(
        result.iter().map(|d| d.message_chain()[0].text()).collect::<Vec<_>>(),
        ["'different' expected.", "'explanation' expected."]
    );
    assert_eq!(
        result[1].related_information().iter().map(Diagnostic::text).collect::<Vec<_>>(),
        ["'a' expected.", "'b' expected.", "'z' expected."]
    );
}

#[test]
fn external_primary_paths_keep_file_identity_and_merge_only_equal_trees() {
    let mut first = head();
    first.add_related_information(Some(child("z")));
    let mut duplicate = head();
    duplicate.add_related_information(Some(child("a")));
    let mut different_tree = head();
    different_tree.add_message_chain(Some(child("explanation")));
    let rows = tsr_diagnostics::sort_and_deduplicate_located_diagnostics(
        vec![
            ("b.ts", head()),
            ("a.ts", first),
            ("", head()),
            ("a.ts", duplicate),
            ("a.ts", different_tree),
        ],
        |(path, diagnostic)| (*path, diagnostic),
        |(_, diagnostic)| diagnostic,
    );
    assert_eq!(
        rows.iter().map(|(path, _)| *path).collect::<Vec<_>>(),
        ["", "a.ts", "a.ts", "b.ts"]
    );
    assert_eq!(rows[1].1.message_chain()[0].text(), "'explanation' expected.");
    assert_eq!(
        rows[2].1.related_information().iter().map(Diagnostic::text).collect::<Vec<_>>(),
        ["'a' expected.", "'z' expected."]
    );
    assert!(rows.iter().all(|(_, diagnostic)| diagnostic.file().is_none()));
}

#[test]
fn comparator_equivalent_children_with_different_codes_are_not_compacted() {
    let mut first = head();
    first.add_message_chain(Some(child("x")));
    let mut second = head();
    second.add_message_chain(Some(Diagnostic::with_args(
        &messages::CANNOT_FIND_NAME_0,
        Span::new(0, 1),
        ["x".into()],
    )));
    let result = tsr_diagnostics::sort_and_deduplicate_diagnostics(vec![first, second]);
    let mut codes = result.iter().map(|d| d.message_chain()[0].message.code()).collect::<Vec<_>>();
    codes.sort_unstable();
    assert_eq!(codes, [1005, 2304]);
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
