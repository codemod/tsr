//! Diagnostics as `tsc` prints them.
//!
//! Ported from `internal/diagnosticwriter/diagnosticwriter.go` at the pinned
//! commit — specifically `WriteFormatDiagnostic` (`:467`),
//! `WriteFlattenedDiagnosticMessage` (`:263`), `WriteErrorSummaryText` (`:330`),
//! `getErrorSummary` (`:377`), `writeTabularErrorsDisplay` (`:412`) and
//! `prettyPathForFileError` (`:443`).
//!
//! # What is here, and what is deliberately not
//!
//! The **plain** output — the one `tsc --pretty false` produces, and the one
//! every `.errors.txt` baseline and every editor problem-matcher parses:
//!
//! ```text
//! src/a.ts(3,7): error TS2304: Cannot find name 'x'.
//! ```
//!
//! The **pretty** output is not ported: no colours, no gutter, no source frame
//! with squiggles under the offending range. That is
//! `FormatDiagnosticWithColorAndContext` and `writeCodeSnippet`
//! (`diagnosticwriter.go:134-252`), and it is a real feature the driver will
//! want, tracked as a named gap in `STATUS.md` §4 rather than approximated here.
//!
//! Approximating it is the specific thing this module refuses to do. A frame
//! that is *nearly* upstream's — right idea, different padding, different
//! ellipsis rule, colours chosen by eye — is worse than no frame at all,
//! because it looks finished. Nobody re-derives a format that already renders;
//! they layer around it, and then it cannot be replaced without breaking
//! whatever grew on top. The plain path below is byte-exact or it is a bug, and
//! that is a property worth having while the compiler underneath is still
//! wrong about types.
//!
//! # Message chains and related information are not represented
//!
//! Upstream's `Diagnostic` carries a `MessageChain()` — the nested "Type 'A' is
//! not assignable to type 'B'. / Property 'x' is missing…" cascade — and a
//! `RelatedInformation()` list. [`Diagnostic`] has neither field, so
//! [`write_flattened_diagnostic_message`] is a single message today. The
//! recursion upstream performs is written out anyway, guarded by a `chain()`
//! that returns nothing, so that adding the field is a change in one place
//! rather than a rediscovery of the indentation rule (two spaces per level).

use std::fmt::Write as _;

use tsr_path::{ComparePathsOptions, convert_to_relative_path, is_rooted_disk_path};

use crate::{Category, Diagnostic};

/// Grey, then reset — the only escape sequences the plain path emits.
///
/// They appear in exactly one place upstream, `prettyPathForFileError`
/// (`diagnosticwriter.go:443`), which writes them **unconditionally**: the error
/// summary's `a.ts:3` is coloured even when nothing else is. Reproduced rather
/// than cleaned up, because the summary line is compared against upstream's and
/// dropping them would make it differ.
const FOREGROUND_COLOR_ESCAPE_GREY: &str = "\u{1b}[90m";
/// See [`FOREGROUND_COLOR_ESCAPE_GREY`].
const RESET_ESCAPE_SEQUENCE: &str = "\u{1b}[0m";

/// How output is rendered (`diagnosticwriter.FormattingOptions`).
#[derive(Debug, Clone)]
pub struct FormattingOptions {
    /// What ends a line. `\n` everywhere except a Windows console.
    ///
    /// A field rather than a constant because it is one upstream, and because
    /// the summary block's blank lines are built from it — hard-coding `\n`
    /// would make two of the three line endings in a `--pretty false` run
    /// disagree on Windows.
    pub newline: String,
    /// What an absolute path is made relative to before printing.
    pub compare_paths: ComparePathsOptions,
}

impl FormattingOptions {
    /// The defaults for a non-watch command-line run.
    #[must_use]
    pub fn new(current_directory: String, use_case_sensitive_file_names: bool) -> Self {
        Self {
            newline: "\n".to_string(),
            compare_paths: ComparePathsOptions { use_case_sensitive_file_names, current_directory },
        }
    }
}

/// The file a diagnostic is positioned in (`diagnosticwriter.FileLike`).
///
/// Owns its line map, computed once on construction. Upstream gets this for free
/// — `ast.SourceFile` memoises `ECMALineMap()` — and the equivalent here is to
/// build one of these per file and share it across every diagnostic in it.
/// Constructing one per diagnostic is correct but rescans the file each time.
#[derive(Debug, Clone)]
pub struct DiagnosticFile {
    file_name: String,
    text: String,
    line_starts: Vec<u32>,
}

impl DiagnosticFile {
    /// Index a file's text so positions in it can be printed.
    #[must_use]
    pub fn new(file_name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let line_starts = tsr_core::ecma_line_starts(&text);
        Self { file_name: file_name.into(), text, line_starts }
    }

    /// The name as the compiler resolved it, before any relativising.
    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Zero-based line and UTF-16 character for a byte offset.
    #[must_use]
    pub fn line_and_character(&self, position: u32) -> (u32, u32) {
        tsr_core::line_and_character(&self.text, &self.line_starts, position)
    }

    /// Zero-based line for a byte offset (`GetECMALineOfPosition`).
    #[must_use]
    pub fn line_of_position(&self, position: u32) -> u32 {
        tsr_core::compute_line_of_position(&self.line_starts, position)
    }
}

/// A diagnostic together with the file it belongs to.
///
/// Upstream's `Diagnostic` interface has a `File()` method; this port's
/// [`Diagnostic`] is a message, a span and its arguments, with no back-edge to a
/// file — the same tree-plus-side-tables discipline the AST follows
/// (ADR-0003). Pairing them at the boundary is what that costs.
///
/// `file` is `None` for a **global** diagnostic: a bad compiler option, a
/// missing input file. Those print with no location prefix and are counted
/// separately by the summary.
#[derive(Debug, Clone, Copy)]
pub struct LocatedDiagnostic<'a> {
    /// Where it is, if it is anywhere.
    pub file: Option<&'a DiagnosticFile>,
    /// What it says.
    pub diagnostic: &'a Diagnostic,
}

impl<'a> LocatedDiagnostic<'a> {
    /// A diagnostic positioned in a file.
    #[must_use]
    pub fn in_file(file: &'a DiagnosticFile, diagnostic: &'a Diagnostic) -> Self {
        Self { file: Some(file), diagnostic }
    }

    /// A diagnostic with no location.
    #[must_use]
    pub fn global(diagnostic: &'a Diagnostic) -> Self {
        Self { file: None, diagnostic }
    }
}

/// Write one diagnostic in the plain format (`WriteFormatDiagnostic`).
///
/// The exact shape, and every part of it is load-bearing for something that
/// parses the output:
///
/// ```text
/// relative/name.ts(line,character): error TS2304: Cannot find name 'x'.
/// ```
///
/// Line and character are **one-based here and zero-based everywhere inside the
/// compiler**; the `+ 1` on each is the only place the convention changes.
pub fn write_format_diagnostic(
    output: &mut String,
    located: &LocatedDiagnostic<'_>,
    options: &FormattingOptions,
) {
    if let Some(file) = located.file {
        let (line, character) = file.line_and_character(located.diagnostic.span.start);
        let relative = convert_to_relative_path(file.file_name(), &options.compare_paths);
        let _ = write!(output, "{relative}({},{}): ", line + 1, character + 1);
    }

    let _ = write!(
        output,
        "{} {}: ",
        located.diagnostic.message.category().name(),
        located.diagnostic.code()
    );
    write_flattened_diagnostic_message(output, located.diagnostic, &options.newline);
    output.push_str(&options.newline);
}

/// Write every diagnostic, in order (`WriteFormatDiagnostics`).
///
/// No separator between them: each already ends in a newline. Note that the
/// *pretty* path does insert a blank line between diagnostics, which is one more
/// reason the two are not interchangeable.
pub fn write_format_diagnostics(
    output: &mut String,
    diagnostics: &[LocatedDiagnostic<'_>],
    options: &FormattingOptions,
) {
    for located in diagnostics {
        write_format_diagnostic(output, located, options);
    }
}

/// The rendered diagnostics as a string, for a caller that does not have one.
#[must_use]
pub fn format_diagnostics(
    diagnostics: &[LocatedDiagnostic<'_>],
    options: &FormattingOptions,
) -> String {
    let mut output = String::new();
    write_format_diagnostics(&mut output, diagnostics, options);
    output
}

/// The message text, with any chain flattened beneath it
/// (`WriteFlattenedDiagnosticMessage`).
///
/// See the module docs: [`Diagnostic`] has no chain today, so the loop below
/// never runs. It is written out because the indentation rule — two spaces per
/// nesting level, each level on its own line — is upstream's and is not
/// re-derivable from the output alone once a chain exists.
pub fn write_flattened_diagnostic_message(
    output: &mut String,
    diagnostic: &Diagnostic,
    newline: &str,
) {
    output.push_str(&diagnostic.text());
    for child in chain(diagnostic) {
        flatten_diagnostic_message_chain(output, child, newline, 1);
    }
}

/// One level of a message chain (`flattenDiagnosticMessageChain`).
fn flatten_diagnostic_message_chain(
    output: &mut String,
    diagnostic: &Diagnostic,
    newline: &str,
    level: usize,
) {
    output.push_str(newline);
    for _ in 0..level {
        output.push_str("  ");
    }
    output.push_str(&diagnostic.text());
    for child in chain(diagnostic) {
        flatten_diagnostic_message_chain(output, child, newline, level + 1);
    }
}

/// A diagnostic's nested explanations (`Diagnostic.MessageChain()`).
///
/// Always empty: the field does not exist yet. Isolated in one function so that
/// adding it is a one-line change here rather than a search for every place the
/// chain should have been walked.
const fn chain(_diagnostic: &Diagnostic) -> &'static [Diagnostic] {
    &[]
}

/// What the summary counted (`diagnosticwriter.ErrorSummary`).
#[derive(Debug, Default)]
struct ErrorSummary<'a> {
    total_error_count: usize,
    global_errors: Vec<&'a LocatedDiagnostic<'a>>,
    /// File name to that file's errors, in the order they were reported.
    ///
    /// Upstream keys this on the `FileLike` pointer and notes in a `!!!` comment
    /// that it wants an ordered map and sorts for consistency instead. Keying on
    /// the name reaches the same place: the sort upstream performs is
    /// `strings.Compare` on exactly this name.
    errors_by_file: Vec<(&'a str, Vec<&'a LocatedDiagnostic<'a>>)>,
}

/// Count and group the errors (`getErrorSummary`).
///
/// **Only `Category::Error` is counted.** A warning or a suggestion prints
/// through [`write_format_diagnostics`] and contributes nothing to the summary
/// or to the exit code, which is what makes `tsc` exit 0 with warnings on screen.
fn get_error_summary<'a>(diagnostics: &'a [LocatedDiagnostic<'a>]) -> ErrorSummary<'a> {
    let mut summary = ErrorSummary::default();

    for located in diagnostics {
        if located.diagnostic.message.category() != Category::Error {
            continue;
        }
        summary.total_error_count += 1;

        match located.file {
            None => summary.global_errors.push(located),
            Some(file) => {
                let name = file.file_name();
                if let Some(entry) =
                    summary.errors_by_file.iter_mut().find(|(existing, _)| *existing == name)
                {
                    entry.1.push(located);
                } else {
                    summary.errors_by_file.push((name, vec![located]));
                }
            }
        }
    }

    summary.errors_by_file.sort_by_key(|(name, _)| *name);
    summary
}

/// Write the `Found N errors` block (`WriteErrorSummaryText`).
///
/// Nothing at all when there are no errors — not "Found 0 errors" — which is why
/// a clean run prints an empty string rather than a reassurance.
///
/// The shape is a blank line, the message, a blank line, and then, **only when
/// more than one file has errors**, the tabular breakdown followed by one more
/// newline.
pub fn write_error_summary_text(
    output: &mut String,
    diagnostics: &[LocatedDiagnostic<'_>],
    options: &FormattingOptions,
) {
    let summary = get_error_summary(diagnostics);
    if summary.total_error_count == 0 {
        return;
    }

    let first = summary.errors_by_file.first();
    let first_file_name = first
        .map(|(name, errors)| pretty_path_for_file_error(name, errors, options))
        .unwrap_or_default();
    let erroring_files = summary.errors_by_file.len();

    let message = if summary.total_error_count == 1 {
        // A single error, special-cased upstream. The `first_file_name.is_empty()`
        // leg catches an error in a file whose only error has no printable path.
        if !summary.global_errors.is_empty() || first_file_name.is_empty() {
            crate::messages::FOUND_1_ERROR.format(&[])
        } else {
            crate::messages::FOUND_1_ERROR_IN_0.format(&[&first_file_name])
        }
    } else {
        let total = summary.total_error_count.to_string();
        match erroring_files {
            0 => crate::messages::FOUND_0_ERRORS.format(&[&total]),
            1 => crate::messages::FOUND_0_ERRORS_IN_THE_SAME_FILE_STARTING_AT_COLON_1
                .format(&[&total, &first_file_name]),
            _ => crate::messages::FOUND_0_ERRORS_IN_1_FILES
                .format(&[&total, &erroring_files.to_string()]),
        }
    };

    output.push_str(&options.newline);
    output.push_str(&message);
    output.push_str(&options.newline);
    output.push_str(&options.newline);

    if erroring_files > 1 {
        write_tabular_errors_display(output, &summary, options);
        output.push_str(&options.newline);
    }
}

/// The per-file error counts (`writeTabularErrorsDisplay`).
///
/// Two columns, right-aligned counts. The padding arithmetic is upstream's and
/// is stranger than it looks: the left column is as wide as the longer of
/// `"Errors"` and the largest count, and the header gets *extra* leading spaces
/// only when the counts are wider than the word.
fn write_tabular_errors_display(
    output: &mut String,
    summary: &ErrorSummary<'_>,
    options: &FormattingOptions,
) {
    let max_errors =
        summary.errors_by_file.iter().map(|(_, errors)| errors.len()).max().unwrap_or(0);

    let header_row = crate::messages::ERRORS_FILES.format(&[]);
    // Upstream splits the header on a space and measures the first word, with a
    // `!!!` comment admitting this was never localised. `"Errors  Files"` has two
    // spaces between the words, so this is 6.
    let left_column_heading_length =
        header_row.split(' ').next().map_or(0, |first| first.chars().count());
    let length_of_biggest_error_count = max_errors.to_string().len();
    let left_padding_goal = left_column_heading_length.max(length_of_biggest_error_count);
    let header_padding = length_of_biggest_error_count.saturating_sub(left_column_heading_length);

    for _ in 0..header_padding {
        output.push(' ');
    }
    output.push_str(&header_row);
    output.push_str(&options.newline);

    for (name, errors) in &summary.errors_by_file {
        // `%*d  ` — right-aligned in `left_padding_goal`, then two spaces.
        let _ = write!(output, "{:>width$}  ", errors.len(), width = left_padding_goal);
        output.push_str(&pretty_path_for_file_error(name, errors, options));
        output.push_str(&options.newline);
    }
}

/// A file and the line of its first error (`prettyPathForFileError`).
///
/// `a.ts:3`, with the `:3` in grey. The relativising is conditional here in a way
/// it is not in [`write_format_diagnostic`]: upstream requires **both** the file
/// name and the current directory to be absolute before converting, so a
/// relative current directory leaves the absolute name alone rather than
/// producing a path relative to nothing.
fn pretty_path_for_file_error(
    file_name: &str,
    errors: &[&LocatedDiagnostic<'_>],
    options: &FormattingOptions,
) -> String {
    let Some(first) = errors.first() else { return String::new() };
    let Some(file) = first.file else { return String::new() };

    let line = file.line_of_position(first.diagnostic.span.start);
    let mut name = file_name.to_string();
    if is_rooted_disk_path(file_name)
        && is_rooted_disk_path(&options.compare_paths.current_directory)
    {
        name = convert_to_relative_path(file_name, &options.compare_paths);
    }

    format!("{name}{FOREGROUND_COLOR_ESCAPE_GREY}:{}{RESET_ESCAPE_SEQUENCE}", line + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsr_core::Span;

    fn options() -> FormattingOptions {
        FormattingOptions::new("/home/project".to_string(), true)
    }

    fn diagnostic(message: &'static crate::Message, start: u32, args: &[&str]) -> Diagnostic {
        Diagnostic::with_args(
            message,
            Span::new(start, start + 1),
            args.iter().map(|arg| (*arg).to_string()),
        )
    }

    /// TS2304, the diagnostic every user has seen.
    fn cannot_find_name() -> &'static crate::Message {
        crate::by_code(2304).expect("TS2304 exists")
    }

    #[test]
    fn a_located_diagnostic_prints_the_tsc_line() {
        let file = DiagnosticFile::new("/home/project/src/a.ts", "let a = 1;\nconsole.log(x);\n");
        // `x` is at offset 23: line 1, character 12 zero-based.
        let d = diagnostic(cannot_find_name(), 23, &["x"]);
        let located = LocatedDiagnostic::in_file(&file, &d);

        assert_eq!(
            format_diagnostics(&[located], &options()),
            "src/a.ts(2,13): error TS2304: Cannot find name 'x'.\n"
        );
    }

    #[test]
    fn the_first_position_is_one_one() {
        // The off-by-one every reimplementation gets wrong in one direction or
        // the other.
        let file = DiagnosticFile::new("/home/project/a.ts", "x;");
        let d = diagnostic(cannot_find_name(), 0, &["x"]);
        assert_eq!(
            format_diagnostics(&[LocatedDiagnostic::in_file(&file, &d)], &options()),
            "a.ts(1,1): error TS2304: Cannot find name 'x'.\n"
        );
    }

    #[test]
    fn a_global_diagnostic_has_no_location_prefix() {
        let d = diagnostic(cannot_find_name(), 0, &["x"]);
        assert_eq!(
            format_diagnostics(&[LocatedDiagnostic::global(&d)], &options()),
            "error TS2304: Cannot find name 'x'.\n"
        );
    }

    #[test]
    fn a_path_outside_the_current_directory_stays_navigable() {
        // `../` rather than an absolute path, which is what makes an editor's
        // problem matcher able to open it.
        let file = DiagnosticFile::new("/home/other/a.ts", "x;");
        let d = diagnostic(cannot_find_name(), 0, &["x"]);
        assert_eq!(
            format_diagnostics(&[LocatedDiagnostic::in_file(&file, &d)], &options()),
            "../other/a.ts(1,1): error TS2304: Cannot find name 'x'.\n"
        );
    }

    #[test]
    fn an_astral_character_shifts_the_column_by_two() {
        // A UTF-16 column, not a `char` column: `tsc` would say 16.
        let file = DiagnosticFile::new("/home/project/a.ts", "const a = '😀'; x;");
        let position = u32::try_from(file.text.find("x;").unwrap()).unwrap();
        let d = diagnostic(cannot_find_name(), position, &["x"]);
        let (_, character) = file.line_and_character(position);
        assert_eq!(character + 1, 17);
        assert!(
            format_diagnostics(&[LocatedDiagnostic::in_file(&file, &d)], &options())
                .starts_with("a.ts(1,17): ")
        );
    }

    #[test]
    fn no_errors_means_no_summary_at_all() {
        let mut output = String::new();
        write_error_summary_text(&mut output, &[], &options());
        assert_eq!(output, "");
    }

    #[test]
    fn one_error_names_its_file() {
        let file = DiagnosticFile::new("/home/project/a.ts", "x;");
        let d = diagnostic(cannot_find_name(), 0, &["x"]);
        let mut output = String::new();
        write_error_summary_text(&mut output, &[LocatedDiagnostic::in_file(&file, &d)], &options());
        assert_eq!(output, "\nFound 1 error in a.ts\u{1b}[90m:1\u{1b}[0m\n\n");
    }

    #[test]
    fn several_errors_in_one_file_report_the_starting_line() {
        let file = DiagnosticFile::new("/home/project/a.ts", "x;\ny;\n");
        let first = diagnostic(cannot_find_name(), 0, &["x"]);
        let second = diagnostic(cannot_find_name(), 3, &["y"]);
        let mut output = String::new();
        write_error_summary_text(
            &mut output,
            &[
                LocatedDiagnostic::in_file(&file, &first),
                LocatedDiagnostic::in_file(&file, &second),
            ],
            &options(),
        );
        // No tabular block: that needs more than one *file*.
        assert_eq!(
            output,
            "\nFound 2 errors in the same file, starting at: a.ts\u{1b}[90m:1\u{1b}[0m\n\n"
        );
    }

    #[test]
    fn several_files_get_the_tabular_breakdown() {
        let a = DiagnosticFile::new("/home/project/a.ts", "x;\n");
        let b = DiagnosticFile::new("/home/project/b.ts", "y;\nz;\n");
        let d1 = diagnostic(cannot_find_name(), 0, &["x"]);
        let d2 = diagnostic(cannot_find_name(), 0, &["y"]);
        let d3 = diagnostic(cannot_find_name(), 3, &["z"]);

        let mut output = String::new();
        write_error_summary_text(
            &mut output,
            &[
                LocatedDiagnostic::in_file(&b, &d2),
                LocatedDiagnostic::in_file(&a, &d1),
                LocatedDiagnostic::in_file(&b, &d3),
            ],
            &options(),
        );

        // Files sorted by name, counts right-aligned in a six-wide column
        // (the width of "Errors"), two spaces, then the path.
        assert_eq!(
            output,
            "\nFound 3 errors in 2 files.\n\n\
             Errors  Files\n\
             \x20    1  a.ts\u{1b}[90m:1\u{1b}[0m\n\
             \x20    2  b.ts\u{1b}[90m:1\u{1b}[0m\n\n"
        );
    }

    #[test]
    fn a_global_error_suppresses_the_file_name_for_a_single_error() {
        let d = diagnostic(cannot_find_name(), 0, &["x"]);
        let mut output = String::new();
        write_error_summary_text(&mut output, &[LocatedDiagnostic::global(&d)], &options());
        // `Found 1 error.` carries a period; `Found 1 error in {0}` does not.
        assert_eq!(output, "\nFound 1 error.\n\n");
    }

    #[test]
    fn only_errors_are_counted() {
        // A message-category diagnostic prints but does not reach the summary,
        // which is what keeps a clean-but-chatty run reporting nothing.
        let file = DiagnosticFile::new("/home/project/a.ts", "x;");
        let d = diagnostic(&crate::messages::FOUND_1_ERROR, 0, &[]);
        let mut output = String::new();
        write_error_summary_text(&mut output, &[LocatedDiagnostic::in_file(&file, &d)], &options());
        assert_eq!(output, "");
    }

    #[test]
    fn the_newline_is_configurable_throughout() {
        let file = DiagnosticFile::new("/home/project/a.ts", "x;");
        let d = diagnostic(cannot_find_name(), 0, &["x"]);
        let mut options = options();
        options.newline = "\r\n".to_string();

        let located = LocatedDiagnostic::in_file(&file, &d);
        assert!(format_diagnostics(&[located], &options).ends_with("\r\n"));

        let mut output = String::new();
        write_error_summary_text(&mut output, &[located], &options);
        assert_eq!(output, "\r\nFound 1 error in a.ts\u{1b}[90m:1\u{1b}[0m\r\n\r\n");
    }
}
