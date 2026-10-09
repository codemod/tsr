//! Reading upstream's `.types` baselines — the checker's oracle.
//!
//! For **every expression** in a file, upstream records the type it computed:
//!
//! ```text
//! === example.ts ===
//! const foo = function (this: any) { }
//! >foo : (this: any) => void
//! >function (this: any) { } : (this: any) => void
//! ```
//!
//! Same shape as [`crate::symbols_baseline`], with a type where the symbol goes.
//! The pairing is deliberate upstream: `.symbols` tests *resolution* and `.types`
//! tests *inference*, at the same positions, so a failure in one localises against
//! the other.
//!
//! # The size of the target
//!
//! Measured at the pin, across the `compiler/` and `conformance/` suites:
//!
//! | | |
//! |---|---|
//! | `.types` baselines | **12,155** |
//! | positioned type assertions in them | **594,122** |
//! | of the baselines, configuration-varied | 2,032 |
//!
//! That is the number `bd tsr-4sc` — the checker — is measured against, and it is
//! why this reader exists before a line of checker code does. `binder_symbols`
//! reports 97.98% on *symbol tables*; nothing in this repository currently
//! measures a type, so there is no signal at all on the 60,269 lines of
//! `internal/checker` until this suite has something to run.
//!
//! # Why the assertion text is kept verbatim
//!
//! A line is `>{expression} : {type}`, and **the expression can contain `" : "`**
//! — a conditional does:
//!
//! ```text
//! >Math.random() > 0.5 ? "abc" : "def" : "abc" | "def"
//! ```
//!
//! Splitting at the first `" : "` gives the expression as
//! `Math.random() > 0.5 ? "abc"`. 773 assertion lines in `compiler/` alone carry
//! two or more, and there is no unambiguous split, because upstream writes the
//! separator without escaping either side.
//!
//! (A type annotation is *not* an instance of this: `this: any` has no space
//! before the colon, so `" : "` does not match it. The first draft of this module
//! used that as its example and its test failed, which is the only reason the real
//! shape got looked up.)
//!
//! So the comparison this suite will eventually do is **whole-line**: we render
//! `>{expression} : {type}` ourselves and compare strings. That sidesteps the
//! ambiguity entirely, and it is also the stricter check. [`TypeAssertion::split`]
//! exists only to make a failure readable and is documented as best-effort.

/// One `>expression : type` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAssertion {
    /// The line's content after `>`, verbatim. This is what gets compared.
    pub text: String,
}

impl TypeAssertion {
    /// A best-effort split into expression and type, **for diagnostics only**.
    ///
    /// Splits at the first `" : "`, which is wrong whenever the expression itself
    /// contains one — a conditional does. Never use this to decide whether an
    /// assertion matches; use [`TypeAssertion::text`].
    #[must_use]
    pub fn split(&self) -> Option<(&str, &str)> {
        self.text.split_once(" : ")
    }
}

/// The type assertions for one file of a case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTypes {
    /// The file, as the `=== name ===` header gives it.
    pub file: String,
    /// Every `>expression : type` line, in source order.
    pub assertions: Vec<TypeAssertion>,
}

/// Split a `.types` baseline into its per-file sections.
///
/// Never fails: a baseline that does not match the shape yields whatever was
/// recognised, which for an unrecognised file is nothing — the honest answer for
/// a splitter that found none.
#[must_use]
pub fn parse(text: &str) -> Vec<FileTypes> {
    let mut files: Vec<FileTypes> = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("=== ").and_then(|rest| rest.strip_suffix(" ===")) {
            files.push(FileTypes { file: name.to_string(), assertions: Vec::new() });
            continue;
        }
        // The header line is `//// [path] ////` and is not a section.
        //
        // §294: an assertion is `>{text} : {type}` — the separator is part of
        // the writer's format and every genuine assertion carries it. A
        // SOURCE-ECHO line that happens to start with `>` does not:
        // `>> // after` in `parserGreaterThanTokenAmbiguity5`'s echo was
        // counted as a phantom baseline assertion for four cases, un-matchable
        // by construction. Requiring the separator is what upstream's own
        // reader does structurally by emitting and consuming the same format.
        if let Some(rest) = line.strip_prefix('>')
            && rest.contains(" : ")
            && let Some(current) = files.last_mut()
        {
            current.assertions.push(TypeAssertion { text: rest.to_string() });
        }
    }
    files
}

/// [`parse`], told each section's source so that an echoed **code line** that
/// starts with `>` is not read as an assertion.
///
/// `iterateBaseline` (`type_symbol_baseline.go:208`) interleaves the file's own
/// lines, `codeLines`, with the `>text : type` rows, every code line exactly
/// once and in order. A code line may itself start with `>` and contain
/// `" : "`: `compiler/deferredConditionalTypes2` continues a type alias on a
/// line reading `>() => G extends B ? 1 : 2`, which [`parse`] counted as an
/// assertion no walker can produce (`docs/parity/notes/r5-align.md` §2.3).
/// §294's separator test cannot see it, because the line has the separator.
///
/// So each section walks its code lines in step: a baseline line equal to the
/// next unconsumed code line is that line's echo and is skipped. The writer's
/// other lines (assertions, and the blank it inserts before a non-blank,
/// non-bracket code line) are never equal to that next code line: an assertion
/// would have to repeat the source verbatim, and the inserted blank precedes
/// only a code line that is not blank. Code lines are split as
/// `codeLinesRegexp` does, with `\r\n` as one break, and pass through
/// `removeTestPathPrefixes`, which the writer applies to the whole section.
///
/// `source_of` answers a section's unit content by section name. A section it
/// cannot answer, or whose echo stops matching, keeps [`parse`]'s reading for
/// its remaining lines, so this never reads fewer lines than it can prove are
/// echoes.
#[must_use]
pub fn parse_with_sources<'s>(
    text: &str,
    mut source_of: impl FnMut(&str) -> Option<&'s str>,
) -> Vec<FileTypes> {
    let mut files: Vec<FileTypes> = Vec::new();
    let mut code: Vec<String> = Vec::new();
    let mut next = 0usize;
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("=== ").and_then(|rest| rest.strip_suffix(" ===")) {
            files.push(FileTypes { file: name.to_string(), assertions: Vec::new() });
            code = source_of(name).map(code_lines).unwrap_or_default();
            next = 0;
            continue;
        }
        if files.is_empty() {
            continue;
        }
        if let Some(expected) = code.get(next)
            && line.strip_prefix('\u{feff}').unwrap_or(line) == expected.as_str()
        {
            next += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix('>')
            && rest.contains(" : ")
            && let Some(current) = files.last_mut()
        {
            current.assertions.push(TypeAssertion { text: rest.to_string() });
        }
    }
    files
}

/// A unit's lines as the baseline echoes them: `codeLinesRegexp`'s breaks
/// (`\r\n`, `\n`, `\r`, U+2028, U+2029), each line through
/// `removeTestPathPrefixes`.
fn code_lines(source: &str) -> Vec<String> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut lines = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find(['\r', '\n', '\u{2028}', '\u{2029}']) {
        lines.push(crate::full_oracle::printed_path(&rest[..at], false));
        let width = if rest[at..].starts_with("\r\n") {
            2
        } else {
            rest[at..].chars().next().map_or(1, char::len_utf8)
        };
        rest = &rest[at + width..];
    }
    lines.push(crate::full_oracle::printed_path(rest, false));
    lines
}

/// How many type assertions a baseline carries, across every file.
#[must_use]
pub fn assertion_count(files: &[FileTypes]) -> usize {
    files.iter().map(|file| file.assertions.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_and_assertions_are_read_in_order() {
        let baseline = "\
//// [tests/cases/compiler/example.ts] ////

=== a.ts ===
const x = 1;
>x : 1
>1 : 1

=== b.ts ===
let y: string;
>y : string
";
        let files = parse(baseline);
        assert_eq!(files.len(), 2, "the `//// [..] ////` header is not a section");
        assert_eq!(files[0].file, "a.ts");
        assert_eq!(files[0].assertions.len(), 2);
        assert_eq!(files[1].assertions[0].text, "y : string");
        assert_eq!(assertion_count(&files), 3);
    }

    #[test]
    fn an_assertion_containing_the_separator_is_kept_whole() {
        // Taken verbatim from a corpus baseline. A conditional puts `" : "` inside
        // the *expression*, so the split is ambiguous — which is why comparison is
        // whole-line.
        let line = r#"Math.random() > 0.5 ? "abc" : "def" : "abc" | "def""#;
        let files = parse(&format!("=== a.ts ===\n>{line}\n"));
        let assertion = &files[0].assertions[0];
        assert_eq!(assertion.text, line);
        // The best-effort split gets this wrong, and the test says so by
        // demonstration rather than by comment.
        assert_eq!(
            assertion.split(),
            Some((r#"Math.random() > 0.5 ? "abc""#, r#""def" : "abc" | "def""#)),
            "the first ` : ` is the conditional's, not the separator"
        );
    }

    #[test]
    fn a_type_annotation_is_not_ambiguous() {
        // `this: any` has no space before the colon, so it does not look like the
        // separator. This is the case an earlier draft wrongly claimed was
        // ambiguous.
        let files = parse("=== a.ts ===\n>f : (this: any) => void\n");
        assert_eq!(files[0].assertions[0].split(), Some(("f", "(this: any) => void")));
    }

    #[test]
    fn an_echoed_code_line_that_looks_like_an_assertion_is_not_one() {
        // `compiler/deferredConditionalTypes2`: the alias continues on a line
        // that starts with `>` and carries the separator.
        let source = "type T<A> = (<G>() => G) extends <\r\n  G,\r\n>() => G extends A ? 1 : 2\r\n  ? true\r\n  : false;\r\n";
        let baseline = "\
=== a.ts ===
type T<A> = (<G>() => G) extends <
>T : T<A>

  G,
>() => G extends A ? 1 : 2
  ? true
>true : true

  : false;
>false : false

";
        assert_eq!(assertion_count(&parse(baseline)), 4, "the plain reader counts the echo");
        let files = parse_with_sources(baseline, |name| (name == "a.ts").then_some(source));
        let lines: Vec<&str> = files[0].assertions.iter().map(|a| a.text.as_str()).collect();
        assert_eq!(lines, ["T : T<A>", "true : true", "false : false"]);
        // No source for the section: the plain reading, unchanged.
        assert_eq!(assertion_count(&parse_with_sources(baseline, |_| None)), 4);
    }

    #[test]
    fn code_lines_break_like_the_writer_and_drop_test_path_prefixes() {
        assert_eq!(
            code_lines("a\r\nb\rc\u{2028}d\n/// <reference path=\"/.lib/x.d.ts\" />"),
            ["a", "b", "c", "d", "/// <reference path=\"x.d.ts\" />"]
        );
    }

    #[test]
    fn a_baseline_this_reader_does_not_recognise_yields_nothing() {
        assert!(parse("total nonsense\nwith no sections\n").is_empty());
    }
}
