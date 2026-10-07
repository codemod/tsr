//! Native 5b1047d keyword-body publication and generic arity controls.
//! Explicit boolean unions retain their own alias; written slots use the
//! existing reader without any renderer admission change.

use tsr_conformance::{
    TestCase, diagnostics_suite, errors_baseline, types_baseline, types_producer,
};

#[test]
fn keyword_alias_declarations_and_existing_written_slots_match_all_native_assertions() {
    let case = TestCase::parse(
        "probe/genericKeywordAliasOwner",
        "genericKeywordAliasOwner.ts",
        include_str!("fixtures/generic_keyword_aliases/Keywords.ts"),
    );
    let expected =
        types_baseline::parse(include_str!("fixtures/generic_keyword_aliases/Keywords.types"));
    let actual = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    assert_eq!(actual.len(), expected.len());
    let mut count = 0;
    for (expected, actual) in expected.iter().zip(actual) {
        let wanted = expected.assertions.iter().map(|row| row.text.as_str()).collect::<Vec<_>>();
        let got = actual.iter().map(types_producer::Assertion::line).collect::<Vec<_>>();
        count += wanted.len();
        assert_eq!(got, wanted, "all native positional assertions");
    }
    assert_eq!(count, 24);
    assert!(diagnostics_suite::reported_for(&case).is_empty());
}

#[test]
fn keyword_alias_publication_preserves_the_complete_native_arity_occurrence_bag() {
    let case = TestCase::parse(
        "probe/genericAliasArityBoundary",
        "genericAliasArityBoundary.ts",
        include_str!("fixtures/generic_keyword_aliases/genericAliasArityBoundary.ts"),
    );
    let mut expected = errors_baseline::parse(include_str!(
        "fixtures/generic_keyword_aliases/genericAliasArityBoundary.errors.txt"
    ));
    let mut actual = diagnostics_suite::reported_for(&case);
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(expected.len(), 3);
    assert_eq!(actual, expected, "two TS2314 occurrences and one TS2707 occurrence");
}
