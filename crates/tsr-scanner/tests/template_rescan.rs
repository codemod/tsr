//! `ReScanTemplateToken` recomputes a template's value from scratch
//! (`docs/parity/notes/r6-printer.md` §4).

use tsr_ast::SyntaxKind::NoSubstitutionTemplateLiteral;
use tsr_scanner::Scanner;

fn rescanned(source: &str, is_tagged: bool) -> (String, String) {
    let mut scanner = Scanner::new(source);
    assert_eq!(scanner.scan().kind, NoSubstitutionTemplateLiteral);
    let initial = scanner.token_value().to_string();
    assert_eq!(scanner.rescan_template(is_tagged).kind, NoSubstitutionTemplateLiteral);
    (initial, scanner.token_value().to_string())
}

#[test]
fn an_untagged_rescan_cooks_a_legacy_octal_escape() {
    // The initial scan does not report, so the escape keeps its raw text;
    // the untagged rescan reports, so it cooks (`scanEscapeSequence`,
    // `scanner.go:1700`). The rescan does not start from the first value.
    assert_eq!(rescanned(r"`\55`", false), (r"\55".to_string(), "-".to_string()));
    assert_eq!(rescanned(r"`x\055y`", false), (r"x\055y".to_string(), "x-y".to_string()));
    assert_eq!(rescanned(r"`\5`", false).1, "\u{5}");
}

#[test]
fn a_tagged_rescan_keeps_the_raw_escape() {
    assert_eq!(rescanned(r"`\55`", true).1, r"\55");
}
