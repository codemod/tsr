//! Parsing a TypeScript compiler test case.
//!
//! Ported from typescript-go's `ParseTestFilesAndSymlinksWithOptions`
//! (`internal/testrunner/test_case_parser.go`).
//!
//! A case file is TypeScript source with `//`-comment directives interleaved:
//!
//! ```text
//! // @target: es2015
//! // @strict: true
//! // @filename: a.ts
//! export const x = 1;
//! // @filename: b.ts
//! import { x } from "./a";
//! ```
//!
//! `@filename` splits the case into units; every other directive is a global
//! option. Getting this wrong misattributes source to the wrong file, which would
//! make every downstream comparison meaningless — so this module mirrors
//! upstream's rules exactly rather than approximating them.

use std::collections::BTreeMap;

/// One file within a test case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFile {
    /// The unit's name, from `@filename` or defaulted to the case's own basename.
    pub name: String,
    /// The unit's source text.
    pub content: String,
}

/// A parsed test case.
#[derive(Debug, Clone)]
pub struct TestCase {
    /// Suite-relative name without extension, e.g. `compiler/2dArrays`.
    pub name: String,
    /// The units the case declares, in order. Always at least one.
    pub files: Vec<TestFile>,
    /// Global compiler options, names lowercased, values trimmed.
    pub options: BTreeMap<String, String>,
    /// Symlinks declared via `@symlink`, mapping link path to the unit it targets.
    pub symlinks: BTreeMap<String, String>,
    /// `@currentDirectory`, when the case sets one.
    pub current_directory: Option<String>,
    /// Whether the case has an `.errors.txt` baseline.
    ///
    /// Upstream's `hadErrorBaseline` (`type_symbol_baseline.go:270`), stamped
    /// by the loader because only the corpus knows the baselines. It is the
    /// first condition of `writeTypeOrSymbol`'s guard chain: in a case that
    /// produced diagnostics, EVERY `any`-flagged type routes to the node
    /// builder and prints `any` rather than the intrinsic `error`.
    ///
    /// `false` for a `TestCase::parse` built without a corpus, which is what
    /// the unit tests use — those assert on shapes, not on this flag.
    pub had_error_baseline: bool,
    /// A structural problem upstream would `panic` on.
    ///
    /// Upstream panics when non-comment content appears before the first
    /// `@filename`. A harness must not die on one malformed case out of 12,444, so
    /// the condition is recorded and surfaced as a suite failure instead.
    pub error: Option<String>,
}

impl TestCase {
    /// Parse a case from its source text.
    ///
    /// `name` is the suite-relative name (`compiler/2dArrays`); `default_unit_name`
    /// is the file name given to content appearing before any `@filename`, which
    /// upstream takes from the case's own path.
    #[must_use]
    pub fn parse(name: &str, default_unit_name: &str, source: &str) -> Self {
        // Test files are checked in with a UTF-8 BOM often enough that ignoring it
        // silently breaks the very first directive.
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);

        let mut options = BTreeMap::new();
        let mut symlinks = BTreeMap::new();
        let mut current_directory = None;
        let mut files: Vec<TestFile> = Vec::new();
        let mut error = None;

        let mut current_name: Option<String> = None;
        let mut current_content = String::new();

        for line in split_lines(source) {
            let Some(directive) = Directive::parse(line) else {
                // Newline is a *separator*, not a terminator, so a unit's content
                // has no trailing newline. A side effect upstream relies on: while
                // the buffer is still empty, leading blank lines add nothing and
                // are dropped.
                if !current_content.is_empty() {
                    current_content.push('\n');
                }
                current_content.push_str(line);
                continue;
            };

            match directive.name.as_str() {
                "filename" => {
                    if let Some(previous) = current_name.take() {
                        // A `@filename` after a previous one closes that unit.
                        files.push(TestFile {
                            name: previous,
                            content: std::mem::take(&mut current_content),
                        });
                    } else {
                        // The *first* `@filename`. Anything before it must be
                        // trivia; upstream panics otherwise. Either way the buffer
                        // is discarded rather than becoming an implicit unit.
                        if error.is_none() && !is_only_trivia(&current_content) {
                            error = Some(
                                "non-comment content appears before the first '@filename'"
                                    .to_string(),
                            );
                        }
                        current_content.clear();
                    }
                    current_name = Some(directive.value);
                }
                "currentdirectory" => current_directory = Some(directive.value),
                "symlink" => {
                    if let Some(target) = current_name.as_deref() {
                        for link in directive.value.split(',') {
                            let link = link.trim();
                            if !link.is_empty() {
                                symlinks.insert(link.to_string(), target.to_string());
                            }
                        }
                    }
                }
                // `// @link: A -> B` creates a symlink at **B** pointing to
                // **A** — upstream's `linkRegex` captures the two sides and
                // stores `symlinks[right] = left`
                // (`internal/testrunner/test_case_parser.go:44`, `:290`),
                // which `harnessutil.go:201` mounts as
                // `testfs[src] = Symlink(target)`. Unlike `@symlink` it names
                // both ends, so it needs no current unit.
                "link" => {
                    if let Some((target, link)) = directive.value.split_once("->") {
                        let (target, link) = (target.trim(), link.trim());
                        if !target.is_empty() && !link.is_empty() {
                            symlinks.insert(link.to_string(), target.to_string());
                        }
                    }
                }
                _ => {
                    options.insert(directive.name, directive.value);
                }
            }
        }

        // EOF always pushes the final unit, unconditionally — a case consisting of
        // nothing but directives still yields one (empty) file. Dropping those
        // would silently shrink the denominator.
        files.push(TestFile {
            name: current_name.unwrap_or_else(|| default_unit_name.to_string()),
            content: current_content,
        });

        Self {
            name: name.to_string(),
            files,
            options,
            symlinks,
            current_directory,
            error,
            had_error_baseline: false,
        }
    }

    /// Total source bytes across all units.
    #[must_use]
    pub fn source_len(&self) -> usize {
        self.files.iter().map(|f| f.content.len()).sum()
    }
}

/// A `// @name: value` directive.
struct Directive {
    name: String,
    value: String,
}

impl Directive {
    /// Match upstream's `optionRegex`: `(?m)^\/{2}\s*@(\w+)\s*:\s*([^\r\n]*)`.
    ///
    /// Hand-rolled rather than pulling in a regex dependency: the pattern is fixed,
    /// this runs over ~12k files, and the rules are worth stating explicitly.
    ///
    /// Note the anchor — `//` must begin the line, so an indented `// @filename:`
    /// inside a code block is *not* a directive.
    fn parse(line: &str) -> Option<Self> {
        let rest = line.strip_prefix("//")?.trim_start();
        let rest = rest.strip_prefix('@')?;

        // `\w+` is ASCII alphanumeric plus underscore.
        let name_end = rest.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')?;
        if name_end == 0 {
            return None;
        }
        let (name, rest) = rest.split_at(name_end);

        let value = rest.trim_start().strip_prefix(':')?;

        Some(Self { name: name.to_ascii_lowercase(), value: value.trim().to_string() })
    }
}

/// Split on `\r?\n`, matching upstream's `lineDelimiter`.
fn split_lines(source: &str) -> impl Iterator<Item = &str> {
    source.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line))
}

/// Whether accumulated text is only whitespace and comments.
///
/// Approximates upstream's `scanner.SkipTrivia` check. Line comments and blank
/// lines are the overwhelmingly common case; block comments are handled crudely
/// but conservatively — anything unrecognised counts as real content, so the
/// approximation errs toward reporting a problem rather than hiding one.
fn is_only_trivia(text: &str) -> bool {
    let mut in_block = false;
    for line in text.lines() {
        let mut rest = line.trim();
        loop {
            if in_block {
                match rest.find("*/") {
                    Some(end) => {
                        in_block = false;
                        rest = rest[end + 2..].trim_start();
                    }
                    None => break,
                }
            } else if rest.is_empty() || rest.starts_with("//") {
                break;
            } else if rest.starts_with("/*") {
                in_block = true;
                rest = &rest[2..];
            } else {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_implicit_file() {
        let case = TestCase::parse("compiler/x", "x.ts", "const a = 1;\n");
        assert_eq!(case.files.len(), 1);
        assert_eq!(case.files[0].name, "x.ts");
        // Newline is a separator, but a source ending in "\n" yields a trailing
        // empty segment, which contributes one — so the trailing newline survives.
        assert_eq!(case.files[0].content, "const a = 1;\n");
        assert!(case.options.is_empty());
    }

    #[test]
    fn options_are_lowercased_and_trimmed() {
        let case = TestCase::parse("c/x", "x.ts", "// @Target:   ES2015  \n//@strict:true\n");
        assert_eq!(case.options.get("target").map(String::as_str), Some("ES2015"));
        assert_eq!(case.options.get("strict").map(String::as_str), Some("true"));
    }

    #[test]
    fn filename_splits_units() {
        let src = "// @filename: a.ts\nexport const x = 1;\n// @filename: b.ts\nimport 'a';\n";
        let case = TestCase::parse("c/x", "x.ts", src);
        assert_eq!(case.files.len(), 2);
        assert_eq!(case.files[0].name, "a.ts");
        // A unit closed by the *next* directive has no trailing newline; the
        // final unit does, because the source ends with one. This asymmetry is
        // upstream's, and is preserved deliberately.
        assert_eq!(case.files[0].content, "export const x = 1;");
        assert_eq!(case.files[1].name, "b.ts");
        assert_eq!(case.files[1].content, "import 'a';\n");
    }

    #[test]
    fn non_comment_content_before_the_first_filename_is_an_error() {
        // Upstream panics here. We record it instead so one malformed case cannot
        // take down a 12,444-case run.
        let src = "const before = 1;\n// @filename: a.ts\nconst after = 2;\n";
        let case = TestCase::parse("c/x", "x.ts", src);
        assert!(case.error.is_some(), "expected a recorded structural error");
        // The stray content is discarded, not turned into an implicit unit.
        assert_eq!(case.files.len(), 1);
        assert_eq!(case.files[0].name, "a.ts");
        assert_eq!(case.files[0].content, "const after = 2;\n");
    }

    #[test]
    fn comments_before_the_first_filename_are_trivia_not_an_error() {
        let src = "// a note\n/* block */\n// @filename: a.ts\nconst x = 1;\n";
        let case = TestCase::parse("c/x", "x.ts", src);
        assert!(case.error.is_none(), "comments are trivia: {:?}", case.error);
        assert_eq!(case.files.len(), 1);
        assert_eq!(case.files[0].name, "a.ts");
    }

    #[test]
    fn leading_blank_lines_are_dropped_within_a_unit() {
        let src = "// @filename: a.ts\n\n\nconst x = 1;\n";
        let case = TestCase::parse("c/x", "x.ts", src);
        assert_eq!(case.files[0].content, "const x = 1;\n");
    }

    #[test]
    fn leading_bom_does_not_hide_the_first_directive() {
        let case = TestCase::parse("c/x", "x.ts", "\u{feff}// @target: es5\nconst a = 1;\n");
        assert_eq!(case.options.get("target").map(String::as_str), Some("es5"));
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let src = "// @filename: a.ts\r\nconst x = 1;\r\n";
        let case = TestCase::parse("c/x", "x.ts", src);
        assert_eq!(case.files[0].name, "a.ts");
        assert_eq!(case.files[0].content, "const x = 1;\n");
    }

    #[test]
    fn indented_directive_is_not_a_directive() {
        // Upstream's regex is anchored at line start, so this is ordinary source.
        let case = TestCase::parse("c/x", "x.ts", "  // @filename: a.ts\nconst x = 1;\n");
        assert_eq!(case.files.len(), 1);
        assert_eq!(case.files[0].name, "x.ts");
    }

    #[test]
    fn a_case_of_only_directives_still_yields_one_file() {
        // Several corpus cases are nothing but options; dropping them would shrink
        // the denominator without anyone noticing.
        let case = TestCase::parse("c/x", "x.ts", "// @target: es5\n");
        assert_eq!(case.files.len(), 1);
        assert!(case.files[0].content.trim().is_empty());
    }

    #[test]
    fn current_directory_and_symlink_are_not_compiler_options() {
        let src = "// @currentDirectory: /src\n// @filename: a.ts\n// @symlink: /link.ts\nx;\n";
        let case = TestCase::parse("c/x", "x.ts", src);
        assert_eq!(case.current_directory.as_deref(), Some("/src"));
        assert_eq!(case.symlinks.get("/link.ts").map(String::as_str), Some("a.ts"));
        assert!(!case.options.contains_key("currentdirectory"));
        assert!(!case.options.contains_key("symlink"));
    }
}
