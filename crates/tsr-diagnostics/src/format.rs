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
//! …and the **pretty** output, which is what `tsc` prints to a terminal:
//! the location in colour, the category and code, then a framed source snippet
//! with tildes under the offending range.
//!
//! > **This module previously refused to port the pretty path**, on the grounds
//! > that a frame which is *nearly* upstream's is worse than none. That refusal
//! > was retired rather than overruled: `STATUS-cli.md` §2 measured what the CLI
//! > baselines actually assert, and they assert the pretty form *by default* —
//! > `showConfig/Default-initialized-TSConfig.js` expects
//! > `<ESC>[91merror<ESC>[0m<ESC>[90m TS5081: <ESC>[0m…`. The refusal was
//! > against approximating; with an oracle that compares bytes, approximation is
//! > no longer possible, which is exactly the condition the refusal named.
//!
//! `writeCodeSnippet` (`diagnosticwriter.go:169`) is transliterated rather than
//! rewritten. Its gutter arithmetic, its five-line elision rule and its
//! tab-to-single-space substitution are all load-bearing for byte equality and
//! none of them is guessable from looking at the output.
//!
//! Message chains preserve insertion order and two-space nesting. Related
//! information is rendered only in pretty output, at its own source location,
//! as in `internal/diagnosticwriter/diagnosticwriter.go`.

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
/// Errors (`foregroundColorEscapeRed`).
const FOREGROUND_COLOR_ESCAPE_RED: &str = "\u{1b}[91m";
/// Warnings (`foregroundColorEscapeYellow`); also the line and column of a
/// location.
const FOREGROUND_COLOR_ESCAPE_YELLOW: &str = "\u{1b}[93m";
/// Informational messages (`foregroundColorEscapeBlue`); also a file name.
const FOREGROUND_COLOR_ESCAPE_BLUE: &str = "\u{1b}[94m";
/// File names in a location (`foregroundColorEscapeCyan`).
const FOREGROUND_COLOR_ESCAPE_CYAN: &str = "\u{1b}[96m";
/// The inverse-video run the gutter is drawn in (`gutterStyleSequence`).
const GUTTER_STYLE_SEQUENCE: &str = "\u{1b}[7m";
/// What separates the gutter from the source line.
const GUTTER_SEPARATOR: &str = " ";
/// What stands in for the lines an over-long span elides.
const ELLIPSIS: &str = "...";

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

    /// Native ordering for diagnostics whose primary files live in the Program.
    #[must_use]
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        crate::compare::compare_at_paths(
            self.diagnostic,
            self.file.map_or("", DiagnosticFile::file_name),
            other.diagnostic,
            other.file.map_or("", DiagnosticFile::file_name),
        )
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

    let _ =
        write!(output, "{} {}: ", located.diagnostic.category().name(), located.diagnostic.code());
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
/// Children retain insertion order; each nesting level adds two spaces.
pub fn write_flattened_diagnostic_message(
    output: &mut String,
    diagnostic: &Diagnostic,
    newline: &str,
) {
    output.push_str(&diagnostic.text());
    for child in diagnostic.message_chain() {
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
    for child in diagnostic.message_chain() {
        flatten_diagnostic_message_chain(output, child, newline, level + 1);
    }
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
        if located.diagnostic.category() != Category::Error {
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

// ---------------------------------------------------------------------------
// The pretty path (`FormatDiagnosticWithColorAndContext`, `writeCodeSnippet`,
// `WriteLocation`, `getCategoryFormat`). See the module docs for why this
// exists now when it deliberately did not before.
// ---------------------------------------------------------------------------

/// The colour a category is printed in (`getCategoryFormat`).
const fn category_format(category: Category) -> &'static str {
    match category {
        Category::Error => FOREGROUND_COLOR_ESCAPE_RED,
        Category::Warning => FOREGROUND_COLOR_ESCAPE_YELLOW,
        Category::Suggestion => FOREGROUND_COLOR_ESCAPE_GREY,
        Category::Message => FOREGROUND_COLOR_ESCAPE_BLUE,
    }
}

/// Write `text` wrapped in `style`, then reset (`writeWithStyleAndReset`).
fn write_with_style_and_reset(output: &mut String, text: &str, style: &str) {
    output.push_str(style);
    output.push_str(text);
    output.push_str(RESET_ESCAPE_SEQUENCE);
}

/// `file:line:col`, in colour (`WriteLocation`).
///
/// Note the separator: **colons**, where the plain path uses `(line,col)`.
/// The two formats disagree deliberately, and an editor configured for one will
/// not parse the other.
fn write_location(
    output: &mut String,
    file: &DiagnosticFile,
    position: u32,
    options: &FormattingOptions,
) {
    let (line, character) = file.line_and_character(position);
    let relative = convert_to_relative_path(file.file_name(), &options.compare_paths);
    write_with_style_and_reset(output, &relative, FOREGROUND_COLOR_ESCAPE_CYAN);
    output.push(':');
    write_with_style_and_reset(output, &(line + 1).to_string(), FOREGROUND_COLOR_ESCAPE_YELLOW);
    output.push(':');
    write_with_style_and_reset(
        output,
        &(character + 1).to_string(),
        FOREGROUND_COLOR_ESCAPE_YELLOW,
    );
}

/// Write one diagnostic in the pretty format
/// (`FormatDiagnosticWithColorAndContext`).
pub fn write_format_diagnostic_with_color_and_context(
    output: &mut String,
    located: &LocatedDiagnostic<'_>,
    options: &FormattingOptions,
) {
    if let Some(file) = located.file {
        write_location(output, file, located.diagnostic.span.start, options);
        output.push_str(" - ");
    }

    let category = located.diagnostic.category();
    write_with_style_and_reset(output, category.name(), category_format(category));
    let _ = write!(
        output,
        "{FOREGROUND_COLOR_ESCAPE_GREY} {}: {RESET_ESCAPE_SEQUENCE}",
        located.diagnostic.code()
    );
    write_flattened_diagnostic_message(output, located.diagnostic, &options.newline);

    if let Some(file) = located.file {
        // Upstream also excludes `File_appears_to_be_binary`, whose "snippet"
        // would be the binary itself. That message is not reachable here — the
        // driver never reports it — so the guard is the `Some(file)` alone.
        output.push_str(&options.newline);
        write_code_snippet(
            output,
            file,
            located.diagnostic.span.start,
            located.diagnostic.span.end.saturating_sub(located.diagnostic.span.start),
            category_format(category),
            "",
            options,
        );
        output.push_str(&options.newline);
    }

    for related in located.diagnostic.related_information() {
        if let Some(file) = related.file() {
            output.push_str(&options.newline);
            output.push_str("  ");
            write_location(output, file, related.span.start, options);
            output.push_str(" - ");
            write_flattened_diagnostic_message(output, related, &options.newline);
            write_code_snippet(
                output,
                file,
                related.span.start,
                related.span.end.saturating_sub(related.span.start),
                FOREGROUND_COLOR_ESCAPE_CYAN,
                "    ",
                options,
            );
        }
        output.push_str(&options.newline);
    }
}

/// Write every diagnostic in the pretty format, blank-line separated.
///
/// The separator is upstream's `FormatDiagnosticsWithColorAndContext`
/// (`:122`) and is one of the few places the two paths differ structurally:
/// the plain path writes nothing between diagnostics.
pub fn write_format_diagnostics_with_color_and_context(
    output: &mut String,
    diagnostics: &[LocatedDiagnostic<'_>],
    options: &FormattingOptions,
) {
    for (index, located) in diagnostics.iter().enumerate() {
        if index > 0 {
            output.push_str(&options.newline);
        }
        write_format_diagnostic_with_color_and_context(output, located, options);
    }
}

/// The framed source excerpt with tildes under the span (`writeCodeSnippet`).
///
/// Transliterated. Four details are load-bearing and none is guessable:
///
/// - **A zero-length span squiggles one character**, the one after its start.
///   Every "expected" diagnostic has a zero-length span.
/// - **A span over five lines shows the first two and last two**, with an
///   ellipsis row between, and the gutter widens to fit `...` when it does.
/// - **Tabs become one space each**, not a tab stop, so the tildes line up.
/// - **Trailing whitespace is trimmed** from each line before it is measured,
///   which is why the squiggle can be shorter than the untrimmed line.
#[allow(clippy::too_many_arguments)]
fn write_code_snippet(
    output: &mut String,
    file: &DiagnosticFile,
    start: u32,
    length: u32,
    squiggle_color: &str,
    indent: &str,
    options: &FormattingOptions,
) {
    let (first_line, first_line_char) = file.line_and_character(start);
    let (last_line, mut last_line_char) = file.line_and_character(start + length);
    if length == 0 {
        last_line_char += 1;
    }

    let text_length = u32::try_from(file.text.len()).unwrap_or(u32::MAX);
    let last_line_of_file = file.line_of_position(text_length);

    let has_more_than_five_lines = last_line.saturating_sub(first_line) >= 4;
    let mut gutter_width = (last_line + 1).to_string().len();
    if has_more_than_five_lines {
        gutter_width = gutter_width.max(ELLIPSIS.len());
    }

    let mut line = first_line;
    while line <= last_line {
        output.push_str(&options.newline);

        if has_more_than_five_lines && first_line + 1 < line && line < last_line - 1 {
            output.push_str(indent);
            output.push_str(GUTTER_STYLE_SEQUENCE);
            let _ = write!(output, "{ELLIPSIS:>gutter_width$}");
            output.push_str(RESET_ESCAPE_SEQUENCE);
            output.push_str(GUTTER_SEPARATOR);
            output.push_str(&options.newline);
            line = last_line - 1;
        }

        let line_start = file.line_starts.get(line as usize).copied().unwrap_or(0) as usize;
        let line_end = if line < last_line_of_file {
            file.line_starts.get(line as usize + 1).copied().unwrap_or(text_length) as usize
        } else {
            file.text.len()
        };
        let raw = file.text.get(line_start..line_end).unwrap_or_default();
        let line_content = raw.trim_end().replace('\t', " ");

        output.push_str(indent);
        output.push_str(GUTTER_STYLE_SEQUENCE);
        let _ = write!(output, "{:>gutter_width$}", line + 1);
        output.push_str(RESET_ESCAPE_SEQUENCE);
        output.push_str(GUTTER_SEPARATOR);
        output.push_str(&line_content);
        output.push_str(&options.newline);

        output.push_str(indent);
        output.push_str(GUTTER_STYLE_SEQUENCE);
        let _ = write!(output, "{:>gutter_width$}", "");
        output.push_str(RESET_ESCAPE_SEQUENCE);
        output.push_str(GUTTER_SEPARATOR);
        output.push_str(squiggle_color);

        let content_width = tsr_core::utf16_len(&line_content);
        if line == first_line {
            let last_char_for_line = if line == last_line { last_line_char } else { content_width };
            for _ in 0..first_line_char {
                output.push(' ');
            }
            for _ in 0..last_char_for_line.saturating_sub(first_line_char) {
                output.push('~');
            }
        } else if line == last_line {
            for _ in 0..last_line_char {
                output.push('~');
            }
        } else {
            for _ in 0..content_width {
                output.push('~');
            }
        }

        output.push_str(RESET_ESCAPE_SEQUENCE);
        line += 1;
    }
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
    #[test]
    fn a_pretty_diagnostic_has_a_location_a_category_and_a_frame() {
        let file = DiagnosticFile::new("/home/project/a.ts", "let a = 1;\nconsole.log(x);\n");
        let d = Diagnostic::with_args(cannot_find_name(), Span::new(23, 24), ["x".to_string()]);
        let mut output = String::new();
        write_format_diagnostic_with_color_and_context(
            &mut output,
            &LocatedDiagnostic::in_file(&file, &d),
            &options(),
        );
        assert_eq!(
            output,
            concat!(
                "\u{1b}[96ma.ts\u{1b}[0m:\u{1b}[93m2\u{1b}[0m:\u{1b}[93m13\u{1b}[0m - ",
                "\u{1b}[91merror\u{1b}[0m\u{1b}[90m TS2304: \u{1b}[0m",
                // Two newlines: one closing the message, one opening the
                // snippet's first line. Upstream writes both.
                "Cannot find name 'x'.\n\n",
                "\u{1b}[7m2\u{1b}[0m console.log(x);\n",
                "\u{1b}[7m \u{1b}[0m \u{1b}[91m            ~\u{1b}[0m\n"
            )
        );
    }

    #[test]
    fn a_global_pretty_diagnostic_has_no_location_and_no_frame() {
        // The `--showConfig` with no config baseline is exactly this shape.
        let d = diagnostic(cannot_find_name(), 0, &["x"]);
        let mut output = String::new();
        write_format_diagnostic_with_color_and_context(
            &mut output,
            &LocatedDiagnostic::global(&d),
            &options(),
        );
        assert_eq!(
            output,
            "\u{1b}[91merror\u{1b}[0m\u{1b}[90m TS2304: \u{1b}[0mCannot find name 'x'."
        );
    }

    #[test]
    fn a_zero_length_span_squiggles_one_character() {
        // Every "expected" diagnostic has one, so this is not an edge case.
        let file = DiagnosticFile::new("/home/project/a.ts", "let a =\n");
        let d = Diagnostic::new(cannot_find_name(), Span::new(7, 7));
        let mut output = String::new();
        write_format_diagnostic_with_color_and_context(
            &mut output,
            &LocatedDiagnostic::in_file(&file, &d),
            &options(),
        );
        assert!(output.contains("       ~\u{1b}[0m"), "{output:?}");
    }

    #[test]
    fn a_tab_becomes_one_space_so_the_squiggle_lines_up() {
        let file = DiagnosticFile::new("/home/project/a.ts", "\t\tlet a = 1;\n");
        let d = Diagnostic::new(cannot_find_name(), Span::new(6, 7));
        let mut output = String::new();
        write_format_diagnostic_with_color_and_context(
            &mut output,
            &LocatedDiagnostic::in_file(&file, &d),
            &options(),
        );
        // The source line renders with two spaces, not two tabs.
        assert!(output.contains("\u{1b}[0m   let a = 1;"), "{output:?}");
        assert!(!output.contains('\t'), "{output:?}");
    }

    #[test]
    fn a_span_over_five_lines_elides_the_middle() {
        let mut text = String::new();
        for n in 1..=8 {
            let _ = writeln!(text, "line{n};");
        }
        let file = DiagnosticFile::new("/home/project/a.ts", text);
        let d = Diagnostic::new(cannot_find_name(), Span::new(0, 47));
        let mut output = String::new();
        write_format_diagnostic_with_color_and_context(
            &mut output,
            &LocatedDiagnostic::in_file(&file, &d),
            &options(),
        );
        assert!(output.contains("..."), "{output:?}");
        // First two and last two lines survive; the middle does not.
        assert!(output.contains("line1;"), "{output:?}");
        assert!(output.contains("line2;"), "{output:?}");
        assert!(!output.contains("line4;"), "{output:?}");
    }

    #[test]
    fn several_pretty_diagnostics_are_blank_line_separated() {
        // Unlike the plain path, which writes nothing between them.
        let file = DiagnosticFile::new("/home/project/a.ts", "x;\ny;\n");
        let first = diagnostic(cannot_find_name(), 0, &["x"]);
        let second = diagnostic(cannot_find_name(), 3, &["y"]);
        let mut output = String::new();
        write_format_diagnostics_with_color_and_context(
            &mut output,
            &[
                LocatedDiagnostic::in_file(&file, &first),
                LocatedDiagnostic::in_file(&file, &second),
            ],
            &options(),
        );
        assert!(output.contains("\u{1b}[0m\n\n\u{1b}[96m"), "{output:?}");
    }
}
