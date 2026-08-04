//! Parsing a `.json` file, and `tsconfig.json` in particular.
//!
//! Ported from `internal/parser/parser.go` (`parseJSONText`, `validateJsonValue`)
//! at the pinned commit.
//!
//! # Why the *parser* reads tsconfig.json
//!
//! It would be less work to hand `tsconfig.json` to a JSON library. Upstream does
//! not, and neither does TypeScript, for three reasons that all still apply:
//!
//! 1. **`tsconfig.json` is not JSON.** It permits comments and trailing commas —
//!    the corpus contains configs with both — so a conforming JSON parser
//!    rejects real configs. Here comments are trivia and a trailing comma is
//!    ordinary object-literal syntax, so both work without a special case.
//! 2. **Every option error points at a span.** `Compiler option 'target' requires
//!    a value of type string` is reported *at the value*, which means the value
//!    has to be a node.
//! 3. **The language service edits it.** Completions inside a `tsconfig.json`
//!    are a real feature, and they need the same tree.
//!
//! So a JSON file is an ordinary [`SourceFile`] whose single statement is an
//! expression statement wrapping the top-level value. [`tsr_module::json`] is a
//! *different* reader for a different job: `package.json` is machine-written,
//! strictly JSON, and read for values rather than for spans.
//!
//! # Error recovery
//!
//! A JSON file with several top-level values is not valid, and upstream still
//! produces a tree for it — the values are collected into a synthetic array so
//! the language service has something to walk. That is reproduced here; a
//! recovery shape that differs from upstream's would show up as a different
//! diagnostic set on malformed configs.

use tsr_ast::{
    ArrayLiteralExpression, Expression, ExpressionStatement, KeywordExpression, SourceFile,
    Statement, SyntaxKind,
};
use tsr_diagnostics::messages;

use crate::{Parser, parser::ScriptKind};

impl<'a> Parser<'a> {
    /// Parse the file as JSON (`Parser.parseJSONText`).
    ///
    /// # Panics
    ///
    /// If the parser was not built with [`ScriptKind::Json`]. The dialect
    /// decides how `<` and `{` scan, so parsing JSON with a TypeScript scanner
    /// would silently produce a different tree.
    pub fn parse_json_text(&mut self) -> &'a SourceFile<'a> {
        assert_eq!(self.script_kind, ScriptKind::Json, "parse_json_text needs ScriptKind::Json");
        let start = self.pos();

        let statements = if self.at(SyntaxKind::EndOfFile) {
            Vec::new()
        } else {
            vec![self.parse_json_statement(start)]
        };

        let eof = self.alloc_token(SyntaxKind::EndOfFile, self.token.span);
        self.finish_node(
            SourceFile::new(self.arena.alloc_slice(&statements), eof),
            SyntaxKind::SourceFile,
            start,
        )
    }

    /// The one statement a JSON file has: its top-level value.
    fn parse_json_statement(&mut self, start: u32) -> Statement<'a> {
        let mut values: Vec<Expression<'a>> = Vec::new();
        while !self.at(SyntaxKind::EndOfFile) {
            let value = self.parse_json_value();
            // The first value is the document; a second one is an error, and
            // reported once rather than per extra value.
            if values.is_empty() && !self.at(SyntaxKind::EndOfFile) {
                self.error_at_current(&messages::UNEXPECTED_TOKEN);
            }
            values.push(value);
        }

        // Exactly one value is the normal case; several are collected into a
        // synthetic array so the tree stays walkable.
        let expression = if let [single] = values[..] {
            single
        } else {
            let elements = self.arena.alloc_slice(&values);
            Expression::ArrayLiteralExpression(self.finish_node(
                ArrayLiteralExpression::new(elements, false),
                SyntaxKind::ArrayLiteralExpression,
                start,
            ))
        };

        Statement::ExpressionStatement(self.finish_node(
            ExpressionStatement::new(Some(expression)),
            SyntaxKind::ExpressionStatement,
            start,
        ))
    }

    /// One JSON value.
    ///
    /// The dispatch is upstream's, including the two lookaheads. They exist for
    /// error recovery on a config whose braces are missing: `"a": 1` at top
    /// level is parsed as an *object literal* so the properties still land in
    /// the tree, and only a string that is not followed by `:` is a bare string
    /// value.
    fn parse_json_value(&mut self) -> Expression<'a> {
        // Bound before the match: the guards below run a lookahead, which needs
        // `&mut self`, and a match on `self.token.kind` would hold a borrow.
        let kind = self.token.kind;
        match kind {
            SyntaxKind::OpenBracketToken => self.parse_array_literal(),
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword | SyntaxKind::NullKeyword => {
                let start = self.pos();
                self.next_token();
                Expression::KeywordExpression(self.finish_node(
                    KeywordExpression::new(kind),
                    kind,
                    start,
                ))
            }
            // `-1` is a value; `-` followed by anything else is a malformed
            // property list.
            SyntaxKind::MinusToken
                if self.look_ahead(|parser| {
                    parser.next_token();
                    parser.at(SyntaxKind::NumericLiteral) && {
                        parser.next_token();
                        !parser.at(SyntaxKind::ColonToken)
                    }
                }) =>
            {
                self.parse_unary_expression()
            }
            SyntaxKind::NumericLiteral | SyntaxKind::StringLiteral
                if self.look_ahead(|parser| {
                    parser.next_token();
                    !parser.at(SyntaxKind::ColonToken)
                }) =>
            {
                self.parse_primary_expression()
            }
            _ => self.parse_object_literal(),
        }
    }
}

#[cfg(test)]
mod tests {
    use tsr_ast::{Node, push_children};
    use tsr_core::Arena;

    use crate::{ParseOptions, ScriptKind, parse_with_options};

    fn kinds(source: &str) -> Vec<String> {
        let arena = Arena::new();
        let parsed = parse_with_options(
            &arena,
            source,
            ParseOptions { script_kind: ScriptKind::Json, ..Default::default() },
        );
        // Pre-order, so the shape reads top-down.
        let mut kinds = Vec::new();
        let mut stack = vec![Node::from(parsed.source_file)];
        let mut children = Vec::new();
        while let Some(node) = stack.pop() {
            if let Some(id) = node.node_id() {
                kinds.push(parsed.nodes.kind(id).to_string());
            }
            children.clear();
            push_children(node, &mut children);
            stack.extend(children.iter().rev().copied());
        }
        kinds
    }

    fn diagnostics(source: &str) -> usize {
        let arena = Arena::new();
        parse_with_options(
            &arena,
            source,
            ParseOptions { script_kind: ScriptKind::Json, ..Default::default() },
        )
        .diagnostics
        .len()
    }

    #[test]
    fn an_object_is_one_expression_statement() {
        let kinds = kinds(r#"{ "a": 1 }"#);
        assert_eq!(&kinds[..3], ["SourceFile", "ExpressionStatement", "ObjectLiteralExpression"]);
        assert_eq!(diagnostics(r#"{ "a": 1 }"#), 0);
    }

    #[test]
    fn comments_and_trailing_commas_are_accepted() {
        // Both appear in the corpus's tsconfig units, and both make this file
        // invalid JSON. That is the whole reason the parser reads it.
        assert_eq!(diagnostics("{\n  // a note\n  \"a\": 1,\n}\n"), 0);
        assert_eq!(diagnostics("{\n  /* block */ \"a\": [1, 2,],\n}\n"), 0);
    }

    #[test]
    fn an_empty_file_has_no_statements() {
        assert_eq!(kinds(""), ["SourceFile", "EndOfFile"]);
        assert_eq!(diagnostics(""), 0);
    }

    #[test]
    fn each_json_value_shape_parses() {
        for (source, expected) in [
            ("[1, 2]", "ArrayLiteralExpression"),
            ("true", "TrueKeyword"),
            ("null", "NullKeyword"),
            ("-1", "PrefixUnaryExpression"),
            ("1", "NumericLiteral"),
            (r#""s""#, "StringLiteral"),
        ] {
            assert_eq!(kinds(source)[2], expected, "for {source}");
            assert_eq!(diagnostics(source), 0, "for {source}");
        }
    }

    #[test]
    fn a_property_list_without_braces_still_yields_an_object() {
        // Error recovery, and the reason the dispatch needs a lookahead: a
        // string followed by `:` is a property name, not a value.
        assert_eq!(kinds(r#""a": 1"#)[2], "ObjectLiteralExpression");
    }

    #[test]
    fn several_top_level_values_are_collected_into_an_array_and_reported_once() {
        assert_eq!(kinds("1 2 3")[2], "ArrayLiteralExpression");
        assert_eq!(diagnostics("1 2 3"), 1);
    }
}
