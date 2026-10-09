//! The parse diagnostics `processPragmasIntoFields` (`parser.go:6581`)
//! reports while it sorts a file's `/// <reference … />` pragmas into fields:
//!
//! - TS1084 `Invalid 'reference' directive syntax.` for a `reference` pragma
//!   naming none of `types`, `lib` and `path` (`parseErrorAtRange(pragma.TextRange, …)`,
//!   `parser.go:6623`);
//! - TS1453 ``resolution-mode` should be either `require` or `import`.`` for
//!   a `types` reference whose `resolution-mode` is anything else
//!   (`parseResolutionMode`, `parser.go:6641`, at the value's range).
//!
//! [`crate::pragma::parse_file_references`] already finds both shapes
//! (`invalid_reference_directives`, `invalid_resolution_modes`); nothing
//! reported them. Upstream runs the pass at the end of `parseSourceFile`, so
//! its reports join the parse diagnostics after every other parse error, and
//! `parseErrorAt`'s same-position guard (`parser.go:327`) drops a report at
//! the start of the last one.
//!
//! `docs/parity/notes/r6-smallcodes4.md` §2.2 records the hooks and their
//! measurement.

use tsr_diagnostics::{Diagnostic, messages};

use crate::pragma::FileReferences;

/// Append `references`' TS1084 and TS1453 reports to a file's parse
/// diagnostics, in `processPragmasIntoFields`' order: pragma by pragma, in
/// source order.
pub(crate) fn append_pragma_diagnostics(
    references: &FileReferences,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut reports: Vec<Diagnostic> = references
        .invalid_reference_directives
        .iter()
        .map(|&span| Diagnostic::new(&messages::INVALID_REFERENCE_DIRECTIVE_SYNTAX, span))
        .chain(references.invalid_resolution_modes.iter().map(|&span| {
            Diagnostic::new(&messages::RESOLUTION_MODE_SHOULD_BE_EITHER_REQUIRE_OR_IMPORT, span)
        }))
        .collect();
    reports.sort_by_key(|d| d.span.start);
    for report in reports {
        if diagnostics.last().is_some_and(|last| last.span.start == report.span.start) {
            continue;
        }
        diagnostics.push(report);
    }
}
