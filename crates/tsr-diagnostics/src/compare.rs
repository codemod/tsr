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

/// Sort, compact duplicates, and merge related information without discarding
/// distinct explanation trees.
///
/// Ported from typescript-go's `SortAndDeduplicateDiagnostics` and
/// `compactAndMergeRelatedInfos` (`internal/compiler/program.go`). Primary file
/// identities must be attached before calling this operation.
#[must_use]
pub fn sort_and_deduplicate_diagnostics(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_unstable_by(compare_diagnostics);
    compact_and_merge_related_infos(diagnostics)
}

/// Compact a sequence already sorted by [`compare_diagnostics`], merging
/// related information for adjacent native-equal diagnostic trees.
///
/// Mirrors native `compactAndMergeRelatedInfos`. Primary files must be attached;
/// comparator-equivalent but unequal chains must not be grouped together.
#[must_use]
pub fn compact_and_merge_related_infos(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut read = 0;
    let mut write = 0;
    while read < diagnostics.len() {
        let mut end = read + 1;
        while end < diagnostics.len()
            && equal_diagnostics_no_related_info(&diagnostics[read], &diagnostics[end])
        {
            end += 1;
        }
        if end > read + 1 {
            let count = diagnostics[read..end]
                .iter()
                .map(|diagnostic| diagnostic.related_information().len())
                .sum();
            if count != 0 {
                let mut related = Vec::with_capacity(count);
                for diagnostic in &diagnostics[read..end] {
                    related.extend_from_slice(diagnostic.related_information());
                }
                related.sort_unstable_by(compare_diagnostics);
                related.dedup_by(|left, right| equal_diagnostics(left, right));
                diagnostics[read].set_related_information(std::sync::Arc::new(related));
            }
        }
        if write != read {
            diagnostics.swap(write, read);
        }
        write += 1;
        read = end;
    }
    diagnostics.truncate(write);
    diagnostics
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
