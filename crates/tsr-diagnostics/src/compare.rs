//! Ported from typescript-go's `CompareDiagnostics` and `EqualDiagnostics`
//! (`internal/ast/diagnostic.go`). Comparison intentionally ignores child codes
//! and category; equality intentionally does not ignore child codes.

use std::cmp::Ordering;

use crate::Diagnostic;

/// Compare diagnostics using their attached file identity.
#[must_use]
pub fn compare_diagnostics(left: &Diagnostic, right: &Diagnostic) -> Ordering {
    compare_at_paths(left, path(left), right, path(right))
}

fn path(diagnostic: &Diagnostic) -> &str {
    diagnostic.file().map_or("", crate::DiagnosticFile::file_name)
}

pub(crate) fn compare_at_paths(
    left: &Diagnostic,
    left_path: &str,
    right: &Diagnostic,
    right_path: &str,
) -> Ordering {
    left_path
        .cmp(right_path)
        .then_with(|| left.span.start.cmp(&right.span.start))
        .then_with(|| left.span.end.cmp(&right.span.end))
        .then_with(|| left.message.code().cmp(&right.message.code()))
        .then_with(|| left.args.cmp(&right.args))
        .then_with(|| compare_chain_size(left.message_chain(), right.message_chain()))
        .then_with(|| compare_chain_content(left.message_chain(), right.message_chain()))
        .then_with(|| compare_related(left.related_information(), right.related_information()))
}

fn compare_chain_size(left: &[Diagnostic], right: &[Diagnostic]) -> Ordering {
    right.len().cmp(&left.len()).then_with(|| {
        left.iter()
            .zip(right)
            .map(|(left, right)| compare_chain_size(left.message_chain(), right.message_chain()))
            .find(|order| *order != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    })
}

// Called only after equal tree shape was established by compare_chain_size.
fn compare_chain_content(left: &[Diagnostic], right: &[Diagnostic]) -> Ordering {
    left.iter()
        .zip(right)
        .map(|(left, right)| {
            left.args
                .cmp(&right.args)
                .then_with(|| compare_chain_content(left.message_chain(), right.message_chain()))
        })
        .find(|order| *order != Ordering::Equal)
        .unwrap_or(Ordering::Equal)
}

fn compare_related(left: &[Diagnostic], right: &[Diagnostic]) -> Ordering {
    right.len().cmp(&left.len()).then_with(|| {
        left.iter()
            .zip(right)
            .map(|(left, right)| compare_diagnostics(left, right))
            .find(|order| *order != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    })
}

/// Native equality excluding related information; child locations are irrelevant.
#[must_use]
pub fn equal_diagnostics_no_related_info(left: &Diagnostic, right: &Diagnostic) -> bool {
    path(left) == path(right)
        && left.span == right.span
        && left.message.code() == right.message.code()
        && left.args == right.args
        && equal_chain(left.message_chain(), right.message_chain())
}

fn equal_chain(left: &[Diagnostic], right: &[Diagnostic]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.message.code() == right.message.code()
                && left.args == right.args
                && equal_chain(left.message_chain(), right.message_chain())
        })
}

/// Native equality including ordered related information.
#[must_use]
pub fn equal_diagnostics(left: &Diagnostic, right: &Diagnostic) -> bool {
    equal_diagnostics_no_related_info(left, right)
        && left.related_information().len() == right.related_information().len()
        && left
            .related_information()
            .iter()
            .zip(right.related_information())
            .all(|(left, right)| equal_diagnostics(left, right))
}
