//! `///`-directives and `@`-pragmas from a file's comment preamble.
//!
//! Ported from `internal/parser/parser.go` (`getCommentPragmas`,
//! `extractPragmas`, `processPragmasIntoFields`) at the pinned commit.
//!
//! # Why the parser and not the checker
//!
//! ```text
//! /// <reference path="./a.ts" />
//! /// <reference types="node" />
//! /// <reference lib="dom" />
//! ```
//!
//! These are **program structure**: each one adds a file to the compilation, and
//! the file loader resolves them exactly as it resolves an `import`. They are
//! syntactically comments, so nothing but the parser is in a position to see
//! them — by the time anything else runs, the trivia is gone.
//!
//! # Why the result is not on `SourceFile`
//!
//! Upstream hangs `ReferencedFiles`, `TypeReferenceDirectives`,
//! `LibReferenceDirectives`, and `CheckJsDirective` off the `SourceFile` node.
//! Here `SourceFile` is generated from `ast.json` and must not be hand-edited,
//! and more importantly this is exactly the kind of per-file, non-tree data that
//! [ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md) puts beside the
//! tree rather than in it. So it comes back as [`FileReferences`] on
//! [`crate::ParsedSourceFile`], next to the JSDoc table, which has the same
//! shape and the same reason.
//!
//! # The parsing is deliberately not a scanner
//!
//! Upstream matches these with hand-written index arithmetic rather than
//! re-entering the tokenizer, because a directive is not a token sequence — it is
//! a *string* pattern inside a comment, and `<reference` must not tokenize. This
//! follows it literally, including the quirks: `pos += 10` to skip `reference`
//! without re-measuring it, and the multi-line rule that only the first
//! `@`-token on a line is considered.

use tsr_core::Span;
use tsr_scanner::{CommentKind, TriviaComment, leading_comment_ranges};

/// Which module format a `resolution-mode` attribute asked for.
///
/// Mirrors `core.ResolutionMode`, which is `ModuleKind` narrowed to three
/// values. Spelled out here so `tsr-parser` need not depend on `tsr-core`'s
/// options module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResolutionMode {
    /// Not specified.
    #[default]
    None,
    /// `resolution-mode="require"`.
    CommonJS,
    /// `resolution-mode="import"`.
    ESNext,
}

/// One `/// <reference … />` directive (`ast.FileReference`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReference {
    /// The span of the quoted value, so a diagnostic can point at it.
    pub span: Span,
    /// The referenced path, package, or lib name.
    pub file_name: String,
    /// What `resolution-mode` asked for; `types` references only.
    pub resolution_mode: ResolutionMode,
    /// Whether `preserve="true"` was set.
    pub preserve: bool,
}

/// A `// @ts-check` or `// @ts-nocheck` directive (`ast.CheckJsDirective`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckJsDirective {
    /// Whether checking was switched on rather than off.
    pub enabled: bool,
    /// The comment it came from.
    pub span: Span,
}

/// What a file's preamble declared (`SourceFile`'s pragma-derived fields).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileReferences {
    /// `/// <reference path="…" />` — another source file to include.
    pub referenced_files: Vec<FileReference>,
    /// `/// <reference types="…" />` — an `@types` package to include.
    pub type_reference_directives: Vec<FileReference>,
    /// `/// <reference lib="…" />` — a built-in library to include.
    pub lib_reference_directives: Vec<FileReference>,
    /// The last `@ts-check`/`@ts-nocheck` in the file, which wins.
    pub check_js_directive: Option<CheckJsDirective>,
    /// The **namespace** of an `@jsx` pragma's factory: `dom` for
    /// `@jsx dom.createElement`, `h` for `@jsx h`.
    ///
    /// `getJsxNamespace` (`checker/jsx.go`) takes the factory's *first*
    /// identifier and `getJsxNamespaceAt` (`:1306`) then looks for a `JSX`
    /// namespace among that symbol's exports — so this name replaces the global
    /// `JSX` for the whole file. See `checker-notes-diag2.md` §211.
    pub jsx_factory_namespace: Option<String>,
    /// Spans of `<reference />` directives naming none of `path`, `types`, or
    /// `lib`, which the parser reports as invalid syntax.
    pub invalid_reference_directives: Vec<Span>,
    /// Spans of `resolution-mode` values that were neither `require` nor
    /// `import`.
    pub invalid_resolution_modes: Vec<Span>,
}

impl FileReferences {
    /// Whether the preamble declared nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.referenced_files.is_empty()
            && self.type_reference_directives.is_empty()
            && self.lib_reference_directives.is_empty()
            && self.check_js_directive.is_none()
            && self.invalid_reference_directives.is_empty()
            && self.invalid_resolution_modes.is_empty()
    }
}

/// One recognised pragma, before it is sorted into fields (`ast.Pragma`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pragma {
    name: String,
    comment: TriviaComment,
    /// `(name, value, value span)`, in source order.
    args: Vec<(String, String, Span)>,
}

impl Pragma {
    fn arg(&self, name: &str) -> Option<&(String, String, Span)> {
        self.args.iter().find(|(key, _, _)| key == name)
    }
}

/// Read a file's preamble (`parser.getCommentPragmas` +
/// `parser.processPragmasIntoFields`).
#[must_use]
pub fn parse_file_references(source: &str) -> FileReferences {
    let mut result = FileReferences::default();
    for comment in leading_comment_ranges(source, 0) {
        for pragma in extract_pragmas(comment, comment.text(source)) {
            process_pragma(&pragma, &mut result);
        }
    }
    result
}

/// `parser.processPragmasIntoFields`, for one pragma.
fn process_pragma(pragma: &Pragma, result: &mut FileReferences) {
    match pragma.name.as_str() {
        "reference" => {
            let no_default_lib = pragma.arg("no-default-lib");
            // Upstream ignores `no-default-lib` here — it is read elsewhere —
            // but it still *consumes* the directive, so a `no-default-lib`
            // reference is not reported as invalid.
            if no_default_lib.is_some_and(|(_, value, _)| value == "true") {
                return;
            }
            let preserve = pragma.arg("preserve").is_some_and(|(_, value, _)| value == "true");

            if let Some((_, file_name, span)) = pragma.arg("types") {
                let resolution_mode = match pragma.arg("resolution-mode") {
                    None => ResolutionMode::None,
                    Some((_, value, span)) => match value.as_str() {
                        "import" => ResolutionMode::ESNext,
                        "require" => ResolutionMode::CommonJS,
                        _ => {
                            result.invalid_resolution_modes.push(*span);
                            ResolutionMode::None
                        }
                    },
                };
                result.type_reference_directives.push(FileReference {
                    span: *span,
                    file_name: file_name.clone(),
                    resolution_mode,
                    preserve,
                });
            } else if let Some((_, file_name, span)) = pragma.arg("lib") {
                result.lib_reference_directives.push(FileReference {
                    span: *span,
                    file_name: file_name.clone(),
                    resolution_mode: ResolutionMode::None,
                    preserve,
                });
            } else if let Some((_, file_name, span)) = pragma.arg("path") {
                result.referenced_files.push(FileReference {
                    span: *span,
                    file_name: file_name.clone(),
                    resolution_mode: ResolutionMode::None,
                    preserve,
                });
            } else {
                result
                    .invalid_reference_directives
                    .push(Span::new(pragma.comment.start, pragma.comment.end));
            }
        }
        "ts-check" | "ts-nocheck" => {
            // The *last* of either wins, which is why this compares positions
            // rather than taking the first.
            let span = Span::new(pragma.comment.start, pragma.comment.end);
            let replace =
                result.check_js_directive.is_none_or(|existing| span.start > existing.span.start);
            if replace {
                result.check_js_directive =
                    Some(CheckJsDirective { enabled: pragma.name == "ts-check", span });
            }
        }
        // `@jsx <factory>` names the JSX namespace for the file; only the
        // factory's first identifier matters, because `getJsxNamespace` splits
        // on the first `.`. `jsxfrag`, `jsximportsource` and `jsxruntime` are
        // still recognised only so they are not mistaken for anything else.
        "jsx" => {
            if let Some((_, value, _)) = pragma.args.first() {
                let namespace = value.split('.').next().unwrap_or(value).trim();
                if !namespace.is_empty() {
                    result.jsx_factory_namespace = Some(namespace.to_string());
                }
            }
        }
        _ => {}
    }
}

/// `parser.extractPragmas`.
fn extract_pragmas(comment: TriviaComment, text: &str) -> Vec<Pragma> {
    match comment.kind {
        CommentKind::SingleLine => {
            extract_single_line_pragma(comment, text).map(|pragma| vec![pragma]).unwrap_or_default()
        }
        CommentKind::MultiLine => extract_multi_line_pragmas(comment, text),
    }
}

fn extract_single_line_pragma(comment: TriviaComment, text: &str) -> Option<Pragma> {
    let mut pos = 2;
    let triple_slash = text[pos..].starts_with('/');
    if triple_slash {
        pos += 1;
    }
    pos = skip_blanks(text, pos);

    if triple_slash && text[pos..].starts_with('<') {
        let tag_name = extract_name(text, pos + 1);
        if tag_name != "reference" {
            return None;
        }
        // Upstream advances by a hard-coded 10 — `<` plus `reference` — rather
        // than by the measured tag length. Faithful, and harmless because the
        // name was just checked to be exactly that.
        pos += 10;

        let mut args = Vec::new();
        loop {
            pos = skip_blanks(text, pos);
            if text[pos..].starts_with("/>") {
                break;
            }
            let arg_name = extract_name(text, pos);
            if arg_name.is_empty() {
                break;
            }
            pos = skip_blanks(text, pos + arg_name.len());
            if !text[pos..].starts_with('=') {
                break;
            }
            pos = skip_blanks(text, pos + 1);
            let Some(value) = extract_quoted_string(text, pos) else { break };
            // The span covers the value, not its quotes.
            let value_start = comment.start + u32::try_from(pos).unwrap_or(0) + 1;
            let value_end = value_start + u32::try_from(value.len()).unwrap_or(0);
            args.push((arg_name, value.to_string(), Span::new(value_start, value_end)));
            pos += value.len() + 2;
        }
        return Some(Pragma { name: "reference".to_string(), comment, args });
    }

    if text[pos..].starts_with('@') {
        let name = extract_name(text, pos + 1);
        if name != "ts-check" && name != "ts-nocheck" {
            return None;
        }
        return Some(Pragma { name, comment, args: Vec::new() });
    }
    None
}

fn extract_multi_line_pragmas(comment: TriviaComment, text: &str) -> Vec<Pragma> {
    let text = text.strip_suffix("*/").unwrap_or(text);
    let mut pragmas = Vec::new();
    let mut pos = 2;
    while pos <= text.len() {
        let Some(at) = text[pos..].find('@').map(|index| index + pos) else { break };
        // Mirrors TypeScript's `/@(\S+)(\s+(?:\S.*)?)?$/gm`: the `@` must be
        // followed immediately by a non-blank name, and the rest of the line is
        // that pragma's argument. Only the *first* `@`-token on a line counts,
        // so an email address earlier on the line hides a later `@jsx`.
        let name_start = at + 1;
        let name_end = skip_non_blanks(text, name_start);
        if name_end == name_start {
            pos = at + 1;
            continue;
        }
        let line_end = line_end_pos(text, at);
        let name = text[name_start..name_end].to_ascii_lowercase();
        if matches!(name.as_str(), "jsx" | "jsxfrag" | "jsximportsource" | "jsxruntime") {
            let start = skip_blanks(text, name_end);
            let arg_end = skip_non_blanks(text, start);
            if arg_end != start {
                let span = Span::new(
                    comment.start + u32::try_from(start).unwrap_or(0),
                    comment.start + u32::try_from(arg_end).unwrap_or(0),
                );
                pragmas.push(Pragma {
                    name,
                    comment,
                    args: vec![("factory".to_string(), text[start..arg_end].to_string(), span)],
                });
            }
        }
        pos = line_end;
        if pos == at {
            break;
        }
    }
    pragmas
}

fn skip_blanks(text: &str, mut pos: usize) -> usize {
    let bytes = text.as_bytes();
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
        pos += 1;
    }
    pos
}

fn skip_non_blanks(text: &str, mut pos: usize) -> usize {
    let bytes = text.as_bytes();
    while pos < bytes.len() && !matches!(bytes[pos], b' ' | b'\t' | b'\r' | b'\n') {
        pos += 1;
    }
    pos
}

fn line_end_pos(text: &str, pos: usize) -> usize {
    text[pos..]
        .char_indices()
        .find(|(_, ch)| matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        .map_or(text.len(), |(offset, _)| pos + offset)
}

/// An attribute or tag name: ASCII letters and `-`, lowercased
/// (`parser.extractName`).
fn extract_name(text: &str, pos: usize) -> String {
    let bytes = text.as_bytes();
    let mut end = pos;
    while end < bytes.len() && (bytes[end].is_ascii_alphabetic() || bytes[end] == b'-') {
        end += 1;
    }
    text[pos.min(text.len())..end].to_ascii_lowercase()
}

/// The contents of a `'…'` or `"…"` string (`parser.extractQuotedString`).
fn extract_quoted_string(text: &str, pos: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    let quote = *bytes.get(pos)?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let start = pos + 1;
    let end = start + text[start..].find(quote as char)?;
    Some(&text[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_reference_kinds_go_to_three_different_lists() {
        let references = parse_file_references(
            "/// <reference path=\"./a.ts\" />\n\
             /// <reference types=\"node\" />\n\
             /// <reference lib=\"dom\" />\n\
             const x = 1;\n",
        );
        assert_eq!(
            references.referenced_files.iter().map(|r| r.file_name.as_str()).collect::<Vec<_>>(),
            ["./a.ts"]
        );
        assert_eq!(
            references
                .type_reference_directives
                .iter()
                .map(|r| r.file_name.as_str())
                .collect::<Vec<_>>(),
            ["node"]
        );
        assert_eq!(
            references
                .lib_reference_directives
                .iter()
                .map(|r| r.file_name.as_str())
                .collect::<Vec<_>>(),
            ["dom"]
        );
    }

    #[test]
    fn the_span_covers_the_value_without_its_quotes() {
        // A diagnostic points here, so an off-by-one shows up as a squiggle on
        // the wrong character rather than as a wrong answer.
        let source = "/// <reference path=\"./a.ts\" />\n";
        let references = parse_file_references(source);
        let span = references.referenced_files[0].span;
        assert_eq!(&source[span.start as usize..span.end as usize], "./a.ts");
    }

    #[test]
    fn only_a_triple_slash_comment_carries_a_reference() {
        // Two slashes is an ordinary comment, whatever it contains.
        assert!(parse_file_references("// <reference path=\"./a.ts\" />\n").is_empty());
    }

    #[test]
    fn a_reference_naming_nothing_useful_is_recorded_as_invalid() {
        let references = parse_file_references("/// <reference foo=\"bar\" />\n");
        assert_eq!(references.invalid_reference_directives.len(), 1);
        assert!(references.referenced_files.is_empty());
    }

    #[test]
    fn no_default_lib_is_consumed_rather_than_reported() {
        // It names none of path/types/lib, so without the explicit arm it would
        // read as invalid syntax.
        let references = parse_file_references("/// <reference no-default-lib=\"true\" />\n");
        assert!(references.is_empty(), "{references:?}");
    }

    #[test]
    fn resolution_mode_is_read_and_validated() {
        let ok =
            parse_file_references("/// <reference types=\"node\" resolution-mode=\"import\" />\n");
        assert_eq!(ok.type_reference_directives[0].resolution_mode, ResolutionMode::ESNext);
        assert!(ok.invalid_resolution_modes.is_empty());

        let bad =
            parse_file_references("/// <reference types=\"node\" resolution-mode=\"esm\" />\n");
        assert_eq!(bad.type_reference_directives[0].resolution_mode, ResolutionMode::None);
        assert_eq!(bad.invalid_resolution_modes.len(), 1);
    }

    #[test]
    fn types_wins_over_lib_wins_over_path_when_several_are_present() {
        // Upstream's `switch` is ordered, not exclusive; a directive naming two
        // attributes takes the first arm that matches.
        let references =
            parse_file_references("/// <reference types=\"node\" path=\"./a.ts\" />\n");
        assert_eq!(references.type_reference_directives.len(), 1);
        assert!(references.referenced_files.is_empty());
    }

    #[test]
    fn preserve_is_carried_through() {
        let references =
            parse_file_references("/// <reference path=\"./a.ts\" preserve=\"true\" />\n");
        assert!(references.referenced_files[0].preserve);
        assert!(
            !parse_file_references("/// <reference path=\"./a.ts\" />\n").referenced_files[0]
                .preserve
        );
    }

    #[test]
    fn the_last_ts_check_directive_wins() {
        let references = parse_file_references("// @ts-nocheck\n// @ts-check\nconst x = 1;\n");
        assert!(references.check_js_directive.expect("a directive").enabled);

        let reversed = parse_file_references("// @ts-check\n// @ts-nocheck\nconst x = 1;\n");
        assert!(!reversed.check_js_directive.expect("a directive").enabled);
    }

    #[test]
    fn single_quotes_work_and_so_does_extra_whitespace() {
        let references = parse_file_references("///   <reference   path =  './a.ts'   />\nx;\n");
        assert_eq!(references.referenced_files[0].file_name, "./a.ts");
    }

    #[test]
    fn a_directive_after_real_code_is_not_a_directive() {
        // The preamble ends at the first token; this is what stops a reference
        // in the middle of a file from adding a file to the program.
        let references = parse_file_references("const x = 1;\n/// <reference path=\"./a.ts\" />\n");
        assert!(references.is_empty());
    }

    #[test]
    fn a_shebang_does_not_hide_the_preamble() {
        let references =
            parse_file_references("#!/usr/bin/env node\n/// <reference path=\"./a.ts\" />\n");
        assert_eq!(references.referenced_files.len(), 1);
    }

    #[test]
    fn jsx_pragmas_are_recognised_but_contribute_no_references() {
        // Recognised so they are not mistaken for something else; read by the
        // JSX transform rather than here.
        let references = parse_file_references("/* @jsx h */\nconst x = 1;\n");
        assert!(references.is_empty(), "no <reference /> directive is contributed");
        // …but the factory's namespace IS captured: `getJsxNamespaceAt`
        // resolves it and looks for a `JSX` namespace among its exports, so it
        // replaces the global `JSX` for the file. §211.
        assert_eq!(references.jsx_factory_namespace.as_deref(), Some("h"));
        let qualified = parse_file_references("/* @jsx dom.createElement */\nconst x = 1;\n");
        assert_eq!(
            qualified.jsx_factory_namespace.as_deref(),
            Some("dom"),
            "only the factory's first identifier is the namespace"
        );
    }

    #[test]
    fn only_the_first_at_token_on_a_line_counts() {
        // Upstream's regex quirk, ported deliberately: an email address earlier
        // on the line hides a later pragma.
        let references = parse_file_references("/* someone@example.com @ts-check */\nx;\n");
        assert!(references.check_js_directive.is_none());
    }

    #[test]
    fn an_unterminated_reference_does_not_run_away() {
        // Malformed input must terminate, not scan to end of file or panic.
        for source in [
            "/// <reference path=\"unclosed\n",
            "/// <reference path=\n",
            "/// <reference\n",
            "/// <\n",
            "///\n",
            "//\n",
        ] {
            let _ = parse_file_references(source);
        }
    }

    #[test]
    fn multi_byte_text_does_not_split_a_character() {
        // The scanner indexes by byte; a naive slice inside a multi-byte
        // character panics.
        let references = parse_file_references("// é\n/// <reference path=\"./é.ts\" />\nx;\n");
        assert_eq!(references.referenced_files[0].file_name, "./é.ts");
    }
}
