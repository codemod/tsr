//! The probe coordinator's supported subset of native diagnostic merging.
//!
//! `ast/diagnostic.go:CompareDiagnostics` orders by filename, range, code and
//! message arguments. `compiler/program.go:SortAndDeduplicateDiagnostics` then
//! compacts equal entries. TSR's Diagnostic has no chains or related information
//! yet; those fields require extending this seam when they are ported.

use std::cmp::Ordering;

use tsr_diagnostics::Diagnostic;

pub(super) fn normalize<'a>(
    diagnostics: &mut Vec<(u32, Diagnostic)>,
    file_name: impl Fn(u32) -> &'a str,
) {
    let compare = |left: &(u32, Diagnostic), right: &(u32, Diagnostic)| {
        file_name(left.0)
            .cmp(file_name(right.0))
            .then_with(|| left.1.span.start.cmp(&right.1.span.start))
            .then_with(|| left.1.span.end.cmp(&right.1.span.end))
            .then_with(|| left.1.message.code().cmp(&right.1.message.code()))
            .then_with(|| left.1.args.cmp(&right.1.args))
    };
    diagnostics.sort_by(compare);
    diagnostics.dedup_by(|left, right| compare(left, right) == Ordering::Equal);
}

#[cfg(test)]
mod tests {
    use super::normalize;
    use tsr_core::Span;
    use tsr_diagnostics::{Diagnostic, messages};

    #[test]
    fn duplicate_identity_is_filename_range_code_and_arguments() {
        let diagnostic =
            Diagnostic::with_args(&messages::CANNOT_FIND_NAME_0, Span::new(1, 2), ["x".into()]);
        let mut entries = vec![(2, diagnostic.clone()), (1, diagnostic.clone()), (0, diagnostic)];
        // Different node identities can still name the same diagnostic file.
        normalize(&mut entries, |id| if id == 2 { "b.ts" } else { "a.ts" });
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, 1);
        assert_eq!(entries[1].0, 2);
    }

    #[test]
    fn end_code_and_unrendered_arguments_are_not_collapsed() {
        let diagnostic =
            Diagnostic::with_args(&messages::CANNOT_FIND_NAME_0, Span::new(1, 2), ["x".into()]);
        let mut longer = diagnostic.clone();
        longer.span.end = 3;
        let mut later = diagnostic.clone();
        later.span.start = 2;
        let mut other_code = diagnostic.clone();
        other_code.message = &messages::UNUSED_TS_EXPECT_ERROR_DIRECTIVE;
        let mut extra_argument = diagnostic.clone();
        extra_argument.args.push("not rendered by this template".into());
        let mut other_argument = diagnostic.clone();
        other_argument.args[0] = "y".into();
        assert_eq!(extra_argument.text(), diagnostic.text());
        let mut entries = vec![
            (0, longer),
            (0, later),
            (0, extra_argument),
            (0, other_argument),
            (0, other_code),
            (0, diagnostic),
        ];
        normalize(&mut entries, |_| "a.ts");
        assert_eq!(entries.len(), 6);
        assert!(entries.windows(2).all(|pair| (pair[0].1.span.start, pair[0].1.span.end)
            <= (pair[1].1.span.start, pair[1].1.span.end)));
    }
}
