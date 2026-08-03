//! Scanner suites.
//!
//! These are the first suites that measure a real compiler stage rather than the
//! harness. Both are deliberately weak claims — the scanner cannot be held to
//! `.errors.txt` yet, because that baseline mixes syntactic and semantic
//! diagnostics with no marker distinguishing them, and most of its contents are
//! the checker's. What *can* be checked today:
//!
//! - the scanner terminates and consumes every byte of every corpus file;
//! - files upstream reports no diagnostics for produce no *scan* diagnostics.
//!
//! The second is a genuine conformance signal: a scan error on a file TypeScript
//! accepts is unambiguously our bug.

use tsr_ast::SyntaxKind;
use tsr_scanner::Scanner;

use crate::{
    corpus::CaseEntry,
    suite::{Outcome, Suite},
};

/// Scan every unit of every case; require termination and full coverage.
///
/// A scanner that fails to advance on some input hangs the parser forever, and a
/// scanner that stops early silently truncates the file. Neither shows up as a
/// wrong token — only as an absent one — so both are checked directly.
pub struct ScannerTermination;

impl Suite for ScannerTermination {
    fn name(&self) -> &'static str {
        "scanner_termination"
    }

    fn describes(&self) -> &'static str {
        "every unit scans to end-of-file, consuming all input, without stalling"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        let parsed = match case.load() {
            Ok(parsed) => parsed,
            Err(err) => return Outcome::Failed { reason: format!("{err:#}") },
        };

        for file in &parsed.files {
            let mut scanner = Scanner::new(&file.content);
            let mut last_end = 0u32;
            let mut steps = 0usize;
            // Every token consumes at least one byte except the final EOF, so the
            // token count cannot exceed the byte count. Exceeding it means the
            // scanner is not advancing.
            let limit = file.content.len() + 2;

            loop {
                let token = scanner.scan();
                steps += 1;
                if token.kind == SyntaxKind::EndOfFile {
                    if (token.span.end as usize) < file.content.len() {
                        return Outcome::Failed {
                            reason: format!(
                                "{}: stopped at byte {} of {}",
                                file.name,
                                token.span.end,
                                file.content.len()
                            ),
                        };
                    }
                    break;
                }
                if token.span.end <= last_end && token.span.start >= last_end {
                    return Outcome::Failed {
                        reason: format!("{}: no progress at byte {}", file.name, last_end),
                    };
                }
                last_end = token.span.end;
                if steps > limit {
                    return Outcome::Failed {
                        reason: format!("{}: exceeded {limit} tokens; not advancing", file.name),
                    };
                }
            }
        }
        Outcome::Passed
    }
}

/// Files TypeScript accepts must produce no scan diagnostics.
///
/// Restricted to cases with no `.errors.txt` — for those, upstream's expected
/// diagnostic output is exactly nothing, so any scan error is our bug. Cases that
/// *do* expect errors are skipped: the scanner cannot tell which of those errors
/// are its responsibility, and counting them either way would be noise.
pub struct ScannerCleanFiles;

impl Suite for ScannerCleanFiles {
    fn name(&self) -> &'static str {
        "scanner_clean_files"
    }

    fn describes(&self) -> &'static str {
        "a file TypeScript reports no errors for produces no scan diagnostics"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if !case.has_any_baseline() {
            return Outcome::Skipped {
                reason: "upstream recorded no output for this case, so there is nothing to judge \
                         against"
                    .into(),
            };
        }
        if case.has_known_divergence() {
            return Outcome::Skipped {
                reason: "upstream records a known divergence from TypeScript (.diff baseline)"
                    .into(),
            };
        }
        if case.has_varied_errors() {
            return Outcome::Skipped {
                reason: "configuration-varied baselines; needs per-configuration runs".into(),
            };
        }
        match case.expected_errors() {
            Err(err) => return Outcome::Failed { reason: format!("{err:#}") },
            Ok(Some(_)) => {
                return Outcome::Skipped {
                    reason: "expects diagnostics; the scanner cannot tell which are its own".into(),
                };
            }
            Ok(None) => {}
        }

        let parsed = match case.load() {
            Ok(parsed) => parsed,
            Err(err) => return Outcome::Failed { reason: format!("{err:#}") },
        };

        for file in &parsed.files {
            // JSON and non-TS units are present in some cases (package.json via
            // `@filename`); scanning them as TypeScript is not meaningful.
            if !is_typescript_unit(&file.name) {
                continue;
            }
            // Lossy UTF-8 decoding leaves replacement characters in the handful of
            // UTF-16-encoded corpus files. Encoding detection belongs to the file
            // loader, not the scanner, so those are not this suite's business.
            if file.content.contains('\u{FFFD}') {
                continue;
            }
            // JSX scanning exists (`Scanner::scan_jsx_token` and friends) but only
            // the parser knows when to enter it — which element, which attribute,
            // which child. The heuristic driver below cannot, so `.tsx` is covered
            // by `parser_typescript` instead of pretended at here.
            if tsr_parser::ScriptKind::from_file_name(&file.name).allows_jsx() {
                continue;
            }
            let mut scanner = Scanner::new(&file.content);
            scan_like_a_parser(&mut scanner);
            let diagnostics = scanner.diagnostics();
            if let Some(first) = diagnostics.first() {
                return Outcome::Failed {
                    reason: format!(
                        "{}: {} scan diagnostic(s), first is TS{} at {:?}: {}",
                        file.name,
                        diagnostics.len(),
                        first.message.code(),
                        first.span,
                        first.text()
                    ),
                };
            }
        }
        Outcome::Passed
    }
}

/// Drive the scanner with the minimum context a parser would supply.
///
/// Two constructs are not decidable lexically, so a bare `while scan()` loop
/// mis-scans them and blames the scanner for it:
///
/// - **Template continuations.** After a `TemplateHead`, the `}` closing a
///   substitution must be re-scanned as template text. Nesting depth is fully
///   decidable, so this is tracked exactly.
/// - **Regular expressions.** Whether `/` starts a regex or is division depends on
///   grammatical position — `a / b` versus `a(/b/)` — and genuinely cannot be
///   resolved without a parser. The heuristic here is to attempt a re-scan and
///   keep it when the literal closes on the same line. It is wrong for some
///   division expressions, which is acceptable for a "produces no diagnostics"
///   check but would not be for a token-stream comparison.
///
/// This is harness scaffolding, not scanner behaviour; the parser replaces it.
fn scan_like_a_parser(scanner: &mut Scanner) {
    // One entry per open template substitution; the value is the brace depth
    // within it, so `${ { } }` closes the right brace.
    let mut template_braces: Vec<u32> = Vec::new();

    loop {
        let token = scanner.scan();
        match token.kind {
            SyntaxKind::EndOfFile => break,
            SyntaxKind::TemplateHead | SyntaxKind::TemplateMiddle => template_braces.push(0),
            SyntaxKind::OpenBraceToken => {
                if let Some(depth) = template_braces.last_mut() {
                    *depth += 1;
                }
            }
            SyntaxKind::CloseBraceToken => match template_braces.last_mut() {
                Some(0) => {
                    template_braces.pop();
                    // Re-scan as the continuation; it yields Middle or Tail, and a
                    // Middle re-opens a substitution.
                    if scanner.rescan_template_continuation().kind == SyntaxKind::TemplateMiddle {
                        template_braces.push(0);
                    }
                }
                Some(depth) => *depth -= 1,
                None => {}
            },
            SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken => {
                // Probe: try the slash as a regex, and rewind if it does not close.
                // `restore` discards the speculative diagnostics, so a division
                // expression is not blamed for an unterminated literal.
                let saved = scanner.save();
                let before = scanner.diagnostics().len();
                scanner.rescan_as_regular_expression();
                if scanner.diagnostics().len() > before {
                    scanner.restore(saved);
                }
            }
            _ => {}
        }
    }
}

/// Whether a unit should be scanned as TypeScript.
pub(crate) fn is_typescript_unit(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}
