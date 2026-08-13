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
    fn a_baseline_this_reader_does_not_recognise_yields_nothing() {
        assert!(parse("total nonsense\nwith no sections\n").is_empty());
    }
}
