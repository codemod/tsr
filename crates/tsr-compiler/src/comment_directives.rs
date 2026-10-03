//! `@ts-ignore` and `@ts-expect-error` — the **program-level** diagnostic
//! filter.
//!
//! `getDiagnosticsWithPrecedingDirectives` (`internal/compiler/program.go:1386`)
//! runs after the binder and the checker have both had their say and before
//! anything is compared to a baseline. It is not a checker rule: it filters
//! diagnostics of *every* code, from every producer, and it is therefore the
//! only place in this port where one build pays for every rule at once.
//!
//! # Two halves, and the second is not optional
//!
//! 1. A diagnostic whose line is preceded — across blank and comment lines only —
//!    by a line carrying a directive is **dropped**, and the directive is marked
//!    used.
//! 2. Every `@ts-expect-error` still unused afterwards becomes **TS2578**
//!    `Unused '@ts-expect-error' directive.` at the directive's own position
//!    (`program.go:1377`).
//!
//! Porting only the first half would trade one wrong diagnostic for one missing
//! one on every case that writes a directive it does not need, and
//! `conformance/ts-expect-error` is exactly such a case.
//!
//! # Where the directives come from
//!
//! Upstream's scanner records them as it scans (`scanner.go:1003`), which this
//! port's does not. They are recovered here by walking the token stream and
//! asking [`tsr_scanner::leading_comment_ranges`] for the trivia in front of
//! each token — the same comments the scanner saw, reached from the outside.
//! **Scanning the raw text for `//` instead would find directives inside string
//! literals and template bodies**, and the corpus contains files whose whole
//! point is that.

use tsr_core::Span;
use tsr_diagnostics::{Diagnostic, messages};
use tsr_scanner::{Scanner, leading_comment_ranges};

/// One `@ts-ignore` or `@ts-expect-error`, and whether anything used it.
#[derive(Debug, Clone, Copy)]
pub struct CommentDirective {
    /// Where the *comment* starts — `NewTextRange(start, end)` upstream, and
    /// the position TS2578 is reported at.
    pub span: Span,
    /// 0-based line the comment starts on.
    pub line: u32,
    /// `@ts-expect-error` rather than `@ts-ignore`. Only this kind can be
    /// unused-and-therefore-an-error.
    pub expects_error: bool,
}

/// Every directive in one file, in source order.
///
/// `ast.CommentDirectiveKind` (`scanner.go:1003`): a comment whose body, after
/// optional whitespace, begins with `@ts-expect-error` or `@ts-ignore`. The
/// leading `//` or `/*` is stripped first, and a `/** … */` doc comment counts —
/// upstream's scanner does not discriminate on the comment's flavour.
#[must_use]
pub fn directives_in(source: &str) -> Vec<CommentDirective> {
    let mut out = Vec::new();
    let mut scanner = Scanner::new(source);
    let mut seen_to = 0usize;
    loop {
        let token = scanner.scan();
        let full_start = scanner.full_start() as usize;
        if full_start >= seen_to {
            for comment in leading_comment_ranges(source, seen_to) {
                if (comment.start as usize) < seen_to {
                    continue;
                }
                if let Some(directive) =
                    directive_of(source, comment.start as usize, comment.end as usize)
                {
                    out.push(directive);
                }
            }
            seen_to = full_start.max(seen_to);
        }
        if token.kind == tsr_ast::SyntaxKind::EndOfFile {
            break;
        }
    }
    out.sort_unstable_by_key(|directive| directive.span.start);
    out.dedup_by_key(|directive| directive.span.start);
    out
}

/// Is the comment at `[pos, end)` a directive?
///
/// `processCommentDirective` (`scanner.go:972`), including the detail that
/// decides where the diagnostic goes: for a **block** comment the recorded
/// position is `lastLineStart` — the start of the comment's *last* line, not the
/// `/*` (`scanner.go:674`). So
///
/// ```text
/// /*
///  @ts-expect-error */
/// var x: number = 'nope';
/// ```
///
/// records the directive on the second line, which is what makes the backward
/// scan reach it from the third; and `ts-expect-error.ts(11,1)` puts TS2578 at
/// **column 1** for a one-line block comment whose `@` is at column 4. Reading
/// the `/*` as the position instead loses the first fact and the second.
fn directive_of(source: &str, comment_start: usize, end: usize) -> Option<CommentDirective> {
    let text = source.get(comment_start..end)?;
    let multiline = text.starts_with("/*");
    let start = if multiline {
        // `lastLineStart`: the byte after the comment's final line break, or the
        // comment's own start when it has none.
        text.rfind('\n').map_or(comment_start, |index| comment_start + index + 1)
    } else {
        comment_start
    };
    let body = source.get(start..end)?;
    let body = if multiline {
        // "Skip whitespace, then combinations of / and *" (`scanner.go:975`).
        body.trim_start_matches([' ', '\t']).trim_start_matches(['/', '*'])
    } else {
        // "Skip opening //, then another / if present" (`scanner.go:986`).
        body.strip_prefix("//")?.trim_start_matches('/')
    };
    let body = body.trim_start_matches([' ', '\t']).strip_prefix('@')?;
    let expects_error = if body.starts_with("ts-expect-error") {
        true
    } else if body.starts_with("ts-ignore") {
        false
    } else {
        return None;
    };
    Some(CommentDirective {
        span: Span::new(
            u32::try_from(start).unwrap_or(u32::MAX),
            u32::try_from(end).unwrap_or(u32::MAX),
        ),
        line: line_of(source, start),
        expects_error,
    })
}

/// The 0-based line a byte offset sits on.
fn line_of(source: &str, offset: usize) -> u32 {
    let count = source[..offset.min(source.len())].bytes().filter(|byte| *byte == b'\n').count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The filter itself, in the terms `program.go:1386` states it.
///
/// `lines` is each diagnostic's **0-based** line, parallel to `diagnostics`;
/// the caller has already computed it for the baseline comparison, and
/// recomputing it here would be a second answer to a question already answered.
///
/// Returns the diagnostics that survive, plus the TS2578 the unused
/// `@ts-expect-error` directives earn.
#[must_use]
pub fn filter<T: Clone>(
    source: &str,
    entries: &[(u32, T)],
    directives: &[CommentDirective],
) -> (Vec<T>, Vec<Diagnostic>) {
    if directives.is_empty() {
        return (entries.iter().map(|(_, item)| item.clone()).collect(), Vec::new());
    }
    let starts = line_starts(source);
    let mut used = vec![false; directives.len()];
    let mut kept = Vec::with_capacity(entries.len());

    for (line, item) in entries {
        let mut suppressed = false;
        // "for line := lineOf(diagnostic) - 1; line >= 0; line--"
        let mut cursor = i64::from(*line) - 1;
        while cursor >= 0 {
            let at = u32::try_from(cursor).unwrap_or(0);
            if let Some(index) = directives.iter().position(|d| d.line == at) {
                used[index] = true;
                suppressed = true;
                break;
            }
            // "Stop searching backwards when we encounter a line that isn't
            // blank or a comment." Without this a directive at the top of a file
            // would swallow every diagnostic in it.
            if !is_comment_or_blank_line(source, &starts, at) {
                break;
            }
            cursor -= 1;
        }
        if !suppressed {
            kept.push(item.clone());
        }
    }

    let unused = directives
        .iter()
        .zip(used)
        .filter(|(directive, used)| directive.expects_error && !used)
        .map(|(directive, _)| {
            Diagnostic::new(&messages::UNUSED_TS_EXPECT_ERROR_DIRECTIVE, directive.span)
        })
        .collect();
    (kept, unused)
}

/// Byte offset of the start of each line.
fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

/// `isCommentOrBlankLine` (`program.go`): whitespace to end of line, or
/// whitespace then `//` or `/*`.
fn is_comment_or_blank_line(source: &str, starts: &[usize], line: u32) -> bool {
    let Some(&start) = starts.get(line as usize) else { return false };
    let rest = &source[start..];
    let rest = rest.split('\n').next().unwrap_or(rest);
    let trimmed = rest.trim_start();
    // `//` **only**, and deliberately: `isCommentOrBlankLine` (`program.go:1445`)
    // does not recognise `/*`, so a block comment between a directive and a
    // diagnostic stops the backward scan. Accepting `/*` here made the scan run
    // past lines upstream stops at.
    trimmed.is_empty() || trimmed.starts_with("//")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directive_is_found_only_in_a_comment() {
        // The reason this walks tokens rather than the raw text: the string
        // literal below is not a directive, and a `text.find("@ts-ignore")`
        // implementation reports it as one.
        let source = "const a = \"// @ts-ignore\";\n// @ts-ignore\nlet b: string = 1;\n";
        let found = directives_in(source);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].line, 1);
        assert!(!found[0].expects_error);
    }

    #[test]
    fn the_search_stops_at_the_first_line_that_is_neither_blank_nor_a_comment() {
        // `program.go`'s backward scan: a directive two lines up with real code
        // in between must not suppress. Without the stop condition one directive
        // at the top of a file silences the whole file.
        let source = "// @ts-ignore\nlet a = 1;\nlet b = 2;\n";
        let directives = directives_in(source);
        let entries = [(2u32, "diagnostic on line 2")];
        let (kept, unused) = filter(source, &entries, &directives);
        assert_eq!(kept.len(), 1);
        assert!(unused.is_empty());
    }

    #[test]
    fn an_unused_expect_error_becomes_ts2578() {
        let source = "// @ts-expect-error\nlet a = 1;\n";
        let directives = directives_in(source);
        let entries: [(u32, ()); 0] = [];
        let (_, unused) = filter(source, &entries, &directives);
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].message.code(), 2578);
    }

    #[test]
    fn a_block_comment_directive_is_recorded_on_its_last_line() {
        // `scanner.go:674` passes `lastLineStart`, not the `/*`. Without it the
        // directive lands two lines above the diagnostic with a non-comment line
        // in between, and `conformance/ts-expect-error` keeps three wrong lines.
        let source = "/*\n @ts-expect-error */\nvar x: number = 'nope';\n";
        let directives = directives_in(source);
        assert_eq!(directives.len(), 1, "{directives:?}");
        assert_eq!(directives[0].line, 1);
        let entries = [(2u32, "the TS2322")];
        let (kept, unused) = filter(source, &entries, &directives);
        assert!(kept.is_empty());
        assert!(unused.is_empty());
    }

    #[test]
    fn a_used_expect_error_earns_no_diagnostic_of_its_own() {
        let source = "// @ts-expect-error\nlet a: string = 1;\n";
        let directives = directives_in(source);
        let entries = [(1u32, "the TS2322")];
        let (kept, unused) = filter(source, &entries, &directives);
        assert!(kept.is_empty());
        assert!(unused.is_empty());
    }
}
