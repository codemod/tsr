//! TS1009 `Trailing comma not allowed.` on `import(x,)`.
//!
//! `checkGrammarImportCallExpression` (`grammarchecks.go:2162`) asks
//! `checkGrammarForDisallowedTrailingComma` (`:671`) of the arguments when the
//! module kind is none of `node16`..`nodenext`, `esnext` and `preserve`
//! (`:2182-2184`). The report sits on the comma (`list.End() - 1`). The
//! arms before it each return first:
//!
//! - `verbatimModuleSyntax` with `commonjs` (`:2163`);
//! - `es2015` (`:2171`), which `check.rs` reports;
//! - type arguments (`:2176`).
//!
//! `import.defer(…)` never reaches the arm: its own test (`:2167`) returns
//! for every module kind but `esnext` and `preserve`, and those two skip the
//! arm.
//!
//! This AST keeps no span for an argument list, so the comma is found where
//! the parser found it. It is the first token after the last argument, past
//! whitespace and comments.
//!
//! `docs/parity/notes/r6-smallcodes4.md` §2.3 records the hook and its
//! measurement.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_core::ModuleKind;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// `checkGrammarImportCallExpression`'s trailing-comma arm
    /// (`grammarchecks.go:2182-2184`), for an `import(…)` call.
    pub(crate) fn check_import_call_trailing_comma(&mut self, node: NodeId) {
        // `grammarErrorAtPos` reports only in a file with no parse errors.
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::CallExpression(call)) = self.node_map.get(node) else { return };
        if !matches!(call.expression, Some(Expression::KeywordExpression(keyword))
            if keyword.kind == SyntaxKind::ImportKeyword)
        {
            return;
        }
        let module = self.module_kind;
        if (self.verbatim_module_syntax && module == ModuleKind::CommonJS)
            || module == ModuleKind::ES2015
            || !call.type_arguments.is_empty()
            || matches!(
                module,
                ModuleKind::Node16
                    | ModuleKind::Node18
                    | ModuleKind::Node20
                    | ModuleKind::NodeNext
                    | ModuleKind::ESNext
                    | ModuleKind::Preserve
            )
        {
            return;
        }
        let Some(last) = call.arguments.last().and_then(Expression::node_id) else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let Some(text) = self.module_host.and_then(|host| host.source_text(file, self.nodes))
        else {
            return;
        };
        let Some(comma) = comma_after(text, self.nodes.span(last).end as usize) else { return };
        let comma = u32::try_from(comma).expect("source offsets fit in u32");
        self.report(
            file,
            Diagnostic::new(
                &messages::TRAILING_COMMA_NOT_ALLOWED,
                tsr_core::Span::new(comma, comma + 1),
            ),
        );
    }
}

/// The offset of a `,` that is the first token at or after `from`, past
/// whitespace and `//` and `/* */` comments; `None` when the first token is
/// anything else.
fn comma_after(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut at = from;
    while at < bytes.len() {
        match bytes[at] {
            b',' => return Some(at),
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at = text[at..].find(['\n', '\r']).map_or(bytes.len(), |end| at + end);
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at = text[at + 2..].find("*/").map(|end| at + 2 + end + 2)?;
            }
            byte if byte.is_ascii_whitespace() => at += 1,
            _ => {
                // Non-ASCII whitespace (`\u{a0}`, `\u{2028}`, …) is trivia too.
                let character = text[at..].chars().next()?;
                if !character.is_whitespace() {
                    return None;
                }
                at += character.len_utf8();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::comma_after;

    #[test]
    fn the_comma_is_found_past_trivia() {
        assert_eq!(comma_after("x ,)", 1), Some(2));
        assert_eq!(comma_after("x /* , */ // c\n ,)", 1), Some(16));
        assert_eq!(comma_after("x )", 1), None);
        assert_eq!(comma_after("x /* , */)", 1), None);
    }
}
