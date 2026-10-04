//! Pinned tsgo 5b1047d original parameter source-view control. All assertions
//! and diagnostic occurrences are compared; computed parameter/return/call
//! slots stay distinct from the reused indexed annotation in function displays.

use tsr_conformance::{TestCase, diagnostics_suite, types_baseline, types_producer};

#[test]
fn completed_original_parameter_views_match_all_native_assertions_without_diagnostics() {
    let case = TestCase::parse(
        "probe/lazyReturnParameterEntry",
        "lazyReturnParameterEntry.ts",
        include_str!("fixtures/structural_parameter_source_views/Entry.ts"),
    );
    let expected = types_baseline::parse(include_str!(
        "fixtures/structural_parameter_source_views/Entry.types"
    ));
    let actual = types_producer::assertions_for_case(&case, &expected, false);
    assert_eq!(actual.len(), expected.len(), "complete file population");
    let mut count = 0;
    for (wanted, got) in expected.iter().zip(actual) {
        let wanted =
            wanted.assertions.iter().map(|assertion| assertion.text.as_str()).collect::<Vec<_>>();
        let got = got.iter().map(types_producer::Assertion::line).collect::<Vec<_>>();
        count += wanted.len();
        assert_eq!(got, wanted, "all native positional assertions");
    }
    assert_eq!(count, 50);
    assert!(diagnostics_suite::reported_for(&case).is_empty(), "native has no diagnostics");
}

#[test]
fn admitted_and_declined_reference_views_keep_the_complete_native_empty_error_bag() {
    let case = TestCase::parse(
        "probe/structuralParameterSourceViews",
        "structuralParameterSourceViews.ts",
        include_str!("fixtures/structural_parameter_source_views/Sites.ts"),
    );
    // Native TestLocal produces no errors baseline for this entire four-file
    // control. The immutable unit test separately pins each admission/refusal;
    // qualification displays outside this certificate remain unsupported.
    assert!(diagnostics_suite::reported_for(&case).is_empty());
}
