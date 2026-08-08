//! Reading a `.js` emit baseline and its embedded sections.
//!
//! # Why declaration output has no baseline of its own
//!
//! Upstream writes **no standalone `.d.ts` baselines**. Declaration output is
//! embedded in the `.js` baseline as a named section, alongside the echoed input
//! and the emitted JavaScript:
//!
//! ```text
//! //// [tests/cases/compiler/example.ts] ////
//!
//! //// [a.ts]
//! export default 1 + 1;
//!
//! //// [a.js]
//! export default 1 + 1;
//!
//! //// [a.d.ts]
//! declare const _default: number;
//! ```
//!
//! So anything that wants to judge `.d.ts` text has to split this file first. In
//! the submodule's `compiler/` and `conformance/` suites there are 11,714 `.js`
//! baselines, of which **1,894 carry at least one declaration section**.
//!
//! # Three things the format does that a naive splitter gets wrong
//!
//! **The header is a section-looking line that is not a section.** The first line
//! names the *case*, not a file, and is distinguished only by a trailing `////`.
//! Checked across every baseline in both suites: no other line has that shape, so
//! the trailing marker is a sound discriminator rather than a guess.
//!
//! **Section names repeat.** A case with a `.js` *input* — anything using
//! `@allowJs` — echoes `file.js` as source and then emits `file.js` again. Both
//! sections are real and they are not the same text, so this reader keeps an
//! ordered list and never a map. Turning it into a map would silently drop one of
//! every such pair, always the input, in exactly the cases where JavaScript input
//! is the thing under test.
//!
//! **`[DtsFileErrors]` is not a file.** 22 baselines carry a section by that name
//! whose body is an `.errors.txt`-shaped block: the diagnostics the *generated*
//! `.d.ts` itself produces when typechecked. It is a property of the case, not a
//! file in it, so it is exposed separately from the file sections.
//!
//! This module is the `.js`/`.d.ts` half of `bd tsr-bb4.2` only. `.types` and
//! `.symbols` are Phase 4's oracle and stay there.

/// One `//// [name]` section of a `.js` baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The name in brackets, verbatim — `a.ts`, `a.d.ts`, `DtsFileErrors`.
    pub name: String,
    /// The section body, with the trailing blank lines that separate sections
    /// removed but internal blank lines kept.
    pub content: String,
}

impl Section {
    /// Whether this section is declaration output.
    ///
    /// `.d.mts` and `.d.cts` count: the corpus has 247 and 248 of them against
    /// 3,215 plain `.d.ts`, and a suffix check for `.d.ts` alone would quietly
    /// treat every ESM/CJS declaration case as having no declaration output.
    #[must_use]
    pub fn is_declaration(&self) -> bool {
        self.name.ends_with(".d.ts")
            || self.name.ends_with(".d.mts")
            || self.name.ends_with(".d.cts")
    }
}

/// A parsed `.js` emit baseline.
#[derive(Debug, Clone, Default)]
pub struct JsBaseline {
    /// The case path from the header line, e.g. `tests/cases/compiler/example.ts`.
    pub case: Option<String>,
    /// Every file section, in order, duplicates preserved.
    pub sections: Vec<Section>,
    /// The body of `[DtsFileErrors]`, when the case has one.
    pub declaration_errors: Option<String>,
}

impl JsBaseline {
    /// Split a `.js` baseline into its sections.
    ///
    /// Never fails: a baseline that does not match the shape yields whatever was
    /// recognised. Callers ask what they need — usually
    /// [`has_declaration_output`](Self::has_declaration_output) — and an
    /// unrecognised file answers "no", which is the honest answer for a splitter
    /// that found nothing.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut baseline = Self::default();
        let mut current: Option<(String, Vec<&str>)> = None;

        for line in text.lines() {
            if let Some(name) = header_name(line) {
                baseline.case = Some(name.to_string());
                continue;
            }
            if let Some(name) = section_name(line) {
                baseline.push(current.take());
                current = Some((name.to_string(), Vec::new()));
                continue;
            }
            // `!!!! File x missing from original emit, but present in noCheck
            // emit` is the baseline runner's own annotation, written *between*
            // sections; it is metadata about which run produced the following
            // section, never emitted text. For a checker-free emitter the
            // noCheck emit is the right oracle anyway (`noEmitOnError`).
            if line.starts_with("!!!!") {
                continue;
            }
            if let Some((_, body)) = current.as_mut() {
                body.push(line);
            }
        }
        baseline.push(current);
        baseline
    }

    fn push(&mut self, section: Option<(String, Vec<&str>)>) {
        let Some((name, body)) = section else { return };
        // Sections are separated by blank lines that belong to neither side.
        let mut content = body.join("\n");
        content.truncate(content.trim_end_matches('\n').len());

        if name == DTS_FILE_ERRORS {
            self.declaration_errors = Some(content);
        } else {
            self.sections.push(Section { name, content });
        }
    }

    /// Whether the case emitted any declaration file.
    #[must_use]
    pub fn has_declaration_output(&self) -> bool {
        self.sections.iter().any(Section::is_declaration)
    }

    /// The declaration sections, in order.
    pub fn declarations(&self) -> impl Iterator<Item = &Section> {
        self.sections.iter().filter(|section| section.is_declaration())
    }
}

/// The name `[DtsFileErrors]` uses, which is a case property rather than a file.
const DTS_FILE_ERRORS: &str = "DtsFileErrors";

/// `//// [case.ts] ////` → the case name.
///
/// The trailing `////` is what separates the header from a section, and nothing
/// else in the corpus carries it.
fn header_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("//// [")?;
    let inner = rest.strip_suffix("] ////")?;
    Some(inner)
}

/// `//// [a.d.ts]` → the section name.
fn section_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("//// [")?;
    let inner = rest.strip_suffix(']')?;
    // A header is not a section; `strip_suffix(']')` would not match it anyway,
    // but being explicit keeps the two readable side by side.
    (!line.ends_with("] ////")).then_some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "//// [tests/cases/compiler/example.ts] ////\n\
        \n\
        //// [a.ts]\n\
        export default 1 + 1;\n\
        \n\
        \n\
        //// [a.js]\n\
        export default 1 + 1;\n\
        //// [a.d.ts]\n\
        declare const _default: number;\n\
        export default _default;\n";

    #[test]
    fn the_header_names_the_case_and_is_not_a_section() {
        let parsed = JsBaseline::parse(SAMPLE);
        assert_eq!(parsed.case.as_deref(), Some("tests/cases/compiler/example.ts"));
        assert_eq!(
            parsed.sections.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["a.ts", "a.js", "a.d.ts"]
        );
    }

    #[test]
    fn a_section_keeps_its_body_without_the_separating_blank_lines() {
        let parsed = JsBaseline::parse(SAMPLE);
        assert_eq!(parsed.sections[0].content, "export default 1 + 1;");
        assert_eq!(
            parsed.sections[2].content,
            "declare const _default: number;\nexport default _default;"
        );
    }

    #[test]
    fn declaration_sections_are_found_by_suffix() {
        let parsed = JsBaseline::parse(SAMPLE);
        assert!(parsed.has_declaration_output());
        assert_eq!(parsed.declarations().count(), 1);
    }

    #[test]
    fn esm_and_cjs_declarations_count_too() {
        // 495 baselines emit only `.d.mts`/`.d.cts`; a `.d.ts` suffix check alone
        // would report every one of them as having no declaration output.
        for name in [".d.mts", ".d.cts"] {
            let text = format!("//// [c.ts] ////\n//// [a{name}]\ndeclare const x: 1;\n");
            assert!(JsBaseline::parse(&text).has_declaration_output(), "{name}");
        }
    }

    #[test]
    fn a_repeated_section_name_keeps_both() {
        // `@allowJs` echoes a `.js` input and then emits `.js` output under the
        // same name. A map would drop the input, in exactly the cases where
        // JavaScript input is what is being tested.
        let text = "//// [c.ts] ////\n\
            //// [f.js]\n\
            export var y;\n\
            //// [f.js]\n\
            export var y;\n";
        let parsed = JsBaseline::parse(text);
        assert_eq!(parsed.sections.len(), 2);
        assert_eq!(parsed.sections[0].name, parsed.sections[1].name);
    }

    #[test]
    fn dts_file_errors_is_kept_apart_from_the_files() {
        let text = "//// [c.ts] ////\n\
            //// [a.d.ts]\n\
            declare let x: $;\n\
            //// [DtsFileErrors]\n\
            /app.d.ts(2,16): error TS2749: nope.\n";
        let parsed = JsBaseline::parse(text);
        assert_eq!(parsed.sections.len(), 1, "the error block is not a file");
        assert!(parsed.declaration_errors.is_some());
    }

    #[test]
    fn the_section_reader_rejects_the_header_on_its_own() {
        // `parse` tests `header_name` first, so the guard inside `section_name` is
        // unreachable through it and no corpus-level mutation can catch a
        // regression in it. Exercised directly instead, the same way the analysis
        // covers rules its oracle cannot see.
        assert_eq!(section_name("//// [a.d.ts]"), Some("a.d.ts"));
        assert_eq!(section_name("//// [case.ts] ////"), None);
        assert_eq!(header_name("//// [case.ts] ////"), Some("case.ts"));
        assert_eq!(header_name("//// [a.d.ts]"), None);
    }

    #[test]
    fn an_unrecognised_file_reports_no_declarations_rather_than_failing() {
        let parsed = JsBaseline::parse("not a baseline at all\n");
        assert!(parsed.case.is_none());
        assert!(!parsed.has_declaration_output());
    }
}
