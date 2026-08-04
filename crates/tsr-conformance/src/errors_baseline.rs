//! Reading the positioned diagnostics out of an `.errors.txt` baseline.
//!
//! # The shape, and the double-count trap
//!
//! An `.errors.txt` baseline states every diagnostic **twice**:
//!
//! ```text
//! a.ts(1,16): error TS9037: Default exports can't be inferred with --isolatedDeclarations.
//!
//! ==== a.ts (1 errors) ====
//!     export default 1 + 1;
//!                    ~~~~~
//! !!! error TS9037: Default exports can't be inferred with --isolatedDeclarations.
//! !!! related TS9036 a.ts:1:1: Move the expression in default export to a variable…
//! ```
//!
//! once in the header block, with a position, and once as a `!!!` line under an
//! echo of the source, without one. Counting `error TS9…` across a baseline
//! therefore returns exactly twice the number of diagnostics, and every count
//! comes out suspiciously even. Only the header block is parsed here.
//!
//! `!!! related` lines are *not* diagnostics. They are the `TS9027`–`TS9036`
//! "add a type annotation to…" hints attached to a parent, they never appear in
//! the header block, and they are outside this oracle.
//!
//! Positions in the header are **1-based line and column**, and the column counts
//! UTF-16 code units, like every other position TypeScript emits.

/// One positioned diagnostic from a baseline header.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BaselineDiagnostic {
    /// The unit the diagnostic is in, as the baseline names it.
    pub file: String,
    /// 1-based line.
    pub line: u32,
    /// 1-based column, in UTF-16 code units.
    pub column: u32,
    /// The `TSxxxx` number.
    pub code: u32,
}

impl std::fmt::Display for BaselineDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({},{}): TS{}", self.file, self.line, self.column, self.code)
    }
}

/// Parse the header block of an `.errors.txt` baseline.
///
/// Stops at the first `==== ` source echo, so a `~~~~`-annotated line that happens
/// to look like a header cannot be misread as one.
#[must_use]
pub fn parse(text: &str) -> Vec<BaselineDiagnostic> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with("==== ") {
            break;
        }
        if let Some(diagnostic) = parse_header_line(line) {
            out.push(diagnostic);
        }
    }
    out
}

/// `file.ts(12,34): error TS9007: message` → the position and the code.
///
/// Global diagnostics have no `(line,column)` and are deliberately not matched:
/// this oracle compares positions, and a diagnostic without one cannot be located
/// well enough to judge.
fn parse_header_line(line: &str) -> Option<BaselineDiagnostic> {
    let (location, rest) = line.split_once("): ")?;
    let (file, position) = location.rsplit_once('(')?;
    let (line_number, column) = position.split_once(',')?;

    let rest = rest.strip_prefix("error TS").or_else(|| rest.strip_prefix("warning TS"))?;
    let code = rest.split(':').next()?;

    Some(BaselineDiagnostic {
        file: file.to_string(),
        line: line_number.trim().parse().ok()?,
        column: column.trim().parse().ok()?,
        code: code.trim().parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_line_yields_its_position_and_code() {
        let parsed = parse("a.ts(1,16): error TS9037: Default exports can't be inferred.\n");
        assert_eq!(
            parsed,
            [BaselineDiagnostic { file: "a.ts".into(), line: 1, column: 16, code: 9037 }]
        );
    }

    #[test]
    fn the_source_echo_is_not_parsed_twice() {
        // The `!!!` restatement carries no position, and counting both would
        // double every tally in the suite — the trap this module exists to avoid.
        let text = "a.ts(1,16): error TS9037: Nope.\n\n\
                    ==== a.ts (1 errors) ====\n    \
                    export default 1 + 1;\n                   ~~~~~\n\
                    !!! error TS9037: Nope.\n";
        assert_eq!(parse(text).len(), 1);
    }

    #[test]
    fn related_hints_are_not_diagnostics() {
        let text = "a.ts(1,16): error TS9037: Nope.\n\
                    ==== a.ts ====\n\
                    !!! related TS9036 a.ts:1:1: Move the expression.\n";
        assert_eq!(parse(text).len(), 1);
    }

    #[test]
    fn a_file_name_containing_parentheses_uses_the_last_one() {
        // Configuration-varied baselines are named `case(target=es5).errors.txt`,
        // and their header lines carry the same shape.
        let parsed = parse("a(target=es5).ts(2,3): error TS9010: Nope.\n");
        assert_eq!(parsed[0].file, "a(target=es5).ts");
        assert_eq!((parsed[0].line, parsed[0].column), (2, 3));
    }

    #[test]
    fn a_diagnostic_with_no_position_is_not_matched() {
        assert!(parse("error TS5042: Option 'project' cannot be mixed.\n").is_empty());
    }
}
