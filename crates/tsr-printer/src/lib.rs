//! Prints the AST back to TypeScript source.
//!
//! Upstream counterpart: `internal/printer/printer.go` (6,280 lines). Unlike
//! [`tsr_dts`], this has a real counterpart and no checker entanglement, so it is
//! a faithful port under [ADR-0001]'s default rather than a rewrite.
//!
//! # What "correct" means here, and why it is not "byte-identical"
//!
//! The gate is a **round trip**: parse → print → reparse → compare trees. The
//! printer is correct when the tree survives, not when the text is reproduced.
//! That is a deliberately different target from Phase 5's emit baselines, and it
//! buys two things — the whole 12,444-case corpus becomes the denominator with no
//! baselines to acquire, and the suite can only sit at 100%, like
//! `scanner_termination`.
//!
//! It also means formatting is free. Quote style, spacing, and line breaks are
//! ours to choose because none of them is in the tree. Phase 5 will need
//! upstream's exact choices; this does not.
//!
//! # Two things that are not free
//!
//! **Literal text is decoded, not raw.** The scanner resolves escapes, so a
//! `StringLiteral`'s `text` is `a"b`, not `"a\"b"`. Printing it back therefore
//! means re-escaping and re-quoting — upstream has the same problem and solves it
//! in `getLiteralText`. Get this wrong and the round trip fails on any string
//! containing a quote or a backslash, which is thousands of corpus cases.
//!
//! **Adjacent tokens can merge.** Printing `a` then `+` then `+b` yields `a++b`,
//! which reparses as a different tree. Rather than hand-placing a space at every
//! call site — where one omission is a silent corruption — [`Printer::write`]
//! inserts a separator whenever the last character written and the first
//! character about to be written could scan as one token. Spacing is not in the
//! tree, so being conservative costs nothing and being wrong costs a case.
//!
//! # Unsupported nodes fail loudly
//!
//! A kind the printer does not handle is **recorded**, never approximated. The
//! conformance suite surfaces the counts per kind, so the unimplemented surface is
//! always a measured worklist rather than an assumption. Emitting a plausible
//! guess for an unhandled node would produce a tree mismatch far from its cause.
//!
//! [ADR-0001]: ../../../docs/adr/0001-idiomatic-rewrite.md

use tsr_ast::{
    ClassElement, Expression, ModifierLike, Node, ObjectLiteralElementLike, SourceFile, SyntaxKind,
    Token, TypeNode,
};

mod expressions;
mod jsx;
mod statements;
mod types;

/// The result of printing a file.
#[derive(Debug, Clone)]
pub struct Printed {
    /// The emitted source text.
    pub text: String,
    /// Node kinds the printer does not implement, in the order first met.
    ///
    /// Empty means the file printed completely. A non-empty list means the text
    /// is *incomplete* and must not be treated as equivalent to the input.
    pub unsupported: Vec<SyntaxKind>,
}

impl Printed {
    /// Whether every node was printed.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.unsupported.is_empty()
    }
}

/// Print a source file.
#[must_use]
pub fn print(file: &SourceFile<'_>, nodes: &tsr_ast::NodeTable) -> Printed {
    let mut printer = Printer::new(nodes);
    printer.source_file(file);
    Printed { text: printer.out, unsupported: printer.unsupported }
}

/// Emits text, tracking indentation and token adjacency.
pub(crate) struct Printer<'t> {
    out: String,
    indent: usize,
    unsupported: Vec<SyntaxKind>,
    /// `const`/`let` live in `NodeFlags`, not in the tree, so printing a variable
    /// statement needs the side table the parser filled in.
    nodes: &'t tsr_ast::NodeTable,
}

impl<'t> Printer<'t> {
    fn new(nodes: &'t tsr_ast::NodeTable) -> Self {
        Self { out: String::new(), indent: 0, unsupported: Vec::new(), nodes }
    }

    fn source_file(&mut self, file: &SourceFile<'_>) {
        for statement in file.statements {
            self.statement(statement);
        }
    }

    // ----- the writer --------------------------------------------------------

    /// Write raw text, inserting a separator first if the tokens would merge.
    pub(crate) fn write(&mut self, text: &str) {
        let Some(next) = text.chars().next() else { return };
        if let Some(last) = self.out.chars().last()
            && would_merge(last, next)
        {
            self.out.push(' ');
        }
        self.out.push_str(text);
    }

    /// Write text that is known to need no separator, such as a closing brace.
    pub(crate) fn write_raw(&mut self, text: &str) {
        self.out.push_str(text);
    }

    pub(crate) fn newline(&mut self) {
        self.out.push('\n');
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    pub(crate) fn indented(&mut self, body: impl FnOnce(&mut Self)) {
        self.indent += 1;
        body(self);
        self.indent -= 1;
    }

    /// The real kind of a node, from the side table.
    ///
    /// Several nodes carry a `kind` *field* that is a punctuation token rather
    /// than the node's kind — `BindingPattern.kind` is the opening bracket, not
    /// `ObjectBindingPattern`. Reading the side table instead avoids a whole class
    /// of silently-wrong dispatch.
    pub(crate) fn kind_of(&self, id: Option<tsr_ast::NodeId>) -> SyntaxKind {
        id.map_or(SyntaxKind::Unknown, |id| self.nodes.kind(id))
    }

    /// How much text has been written, for assertions.
    pub(crate) fn len(&self) -> usize {
        self.out.len()
    }

    /// Record a node kind this printer does not implement.
    pub(crate) fn unsupported(&mut self, kind: SyntaxKind) {
        if !self.unsupported.contains(&kind) {
            self.unsupported.push(kind);
        }
    }

    // ----- shared helpers ----------------------------------------------------

    pub(crate) fn token(&mut self, token: &Token<'_>) {
        match token_text(token.kind) {
            Some(text) => self.write(text),
            None => self.unsupported(token.kind),
        }
    }

    pub(crate) fn modifiers(&mut self, modifiers: &[ModifierLike<'_>]) {
        for modifier in modifiers {
            match modifier {
                ModifierLike::Token(token) => {
                    self.token(token);
                    self.write(" ");
                }
                ModifierLike::Decorator(decorator) => {
                    self.write("@");
                    if let Some(expression) = &decorator.expression {
                        self.any_expression(Node::from(*expression));
                    }
                    self.newline();
                }
            }
        }
    }

    /// Print a node that is known to be an expression in a wider union.
    ///
    /// The generated unions (`ConciseBody`, `ForInitializer`,
    /// `LeftHandSideExpression`) overlap `Expression` without being convertible to
    /// it directly, so they round-trip through `Node`. A node that is genuinely not
    /// an expression is recorded rather than skipped.
    pub(crate) fn any_expression(&mut self, node: Node<'_>) {
        match Expression::try_from(node) {
            Ok(expression) => self.expression(&expression),
            Err(other) => {
                let kind = self.kind_of(other.node_id());
                self.unsupported(kind);
            }
        }
    }

    /// `<A, B>` for a type-parameter list, or nothing when empty.
    pub(crate) fn type_parameters(
        &mut self,
        parameters: &[&tsr_ast::TypeParameterDeclaration<'_>],
    ) {
        if parameters.is_empty() {
            return;
        }
        self.write("<");
        for (index, parameter) in parameters.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.modifiers(parameter.modifiers);
            if let Some(name) = parameter.name {
                self.write(name.text);
            }
            if let Some(constraint) = parameter.constraint {
                self.write(" extends ");
                self.type_node(&constraint);
            }
            if let Some(default) = parameter.default_type {
                self.write(" = ");
                self.type_node(&default);
            }
        }
        self.write(">");
    }

    /// `<A, B>` for a type-argument list, or nothing when empty.
    pub(crate) fn type_arguments(&mut self, arguments: &[TypeNode<'_>]) {
        if arguments.is_empty() {
            return;
        }
        self.write("<");
        for (index, argument) in arguments.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.type_node(argument);
        }
        self.write(">");
    }

    pub(crate) fn parameters(&mut self, parameters: &[&tsr_ast::ParameterDeclaration<'_>]) {
        self.write("(");
        for (index, parameter) in parameters.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.modifiers(parameter.modifiers);
            if parameter.dot_dot_dot_token.is_some() {
                self.write("...");
            }
            if let Some(name) = &parameter.name {
                self.binding_name(name);
            }
            if parameter.question_token.is_some() {
                self.write("?");
            }
            if let Some(node) = &parameter.r#type {
                self.write(": ");
                self.type_node(node);
            }
            if let Some(initializer) = &parameter.initializer {
                self.write(" = ");
                self.expression(initializer);
            }
        }
        self.write(")");
    }

    pub(crate) fn binding_name(&mut self, name: &tsr_ast::BindingName<'_>) {
        match name {
            tsr_ast::BindingName::Identifier(identifier) => self.write(identifier.text),
            tsr_ast::BindingName::BindingPattern(pattern) => self.binding_pattern(pattern),
        }
    }

    pub(crate) fn binding_pattern(&mut self, pattern: &tsr_ast::BindingPattern<'_>) {
        let object = self.kind_of(pattern.node_id) == SyntaxKind::ObjectBindingPattern;
        self.write(if object { "{ " } else { "[" });
        for (index, element) in pattern.elements.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            // An array pattern hole is an element with no name.
            if element.dot_dot_dot_token.is_some() {
                self.write("...");
            }
            if let Some(property) = &element.property_name {
                self.property_name(property);
                self.write(": ");
            }
            if let Some(name) = &element.name {
                self.binding_name(name);
            }
            if let Some(initializer) = &element.initializer {
                self.write(" = ");
                self.expression(initializer);
            }
        }
        self.write(if object { " }" } else { "]" });
    }

    pub(crate) fn property_name(&mut self, name: &tsr_ast::PropertyName<'_>) {
        match name {
            tsr_ast::PropertyName::Identifier(identifier) => self.write(identifier.text),
            // `PrivateIdentifier.text` already carries its `#`.
            tsr_ast::PropertyName::PrivateIdentifier(identifier) => self.write(identifier.text),
            tsr_ast::PropertyName::StringLiteral(literal) => {
                let quoted = quote_string(literal.text);
                self.write(&quoted);
            }
            tsr_ast::PropertyName::NumericLiteral(literal) => self.write(literal.text),
            tsr_ast::PropertyName::BigIntLiteral(literal) => {
                let text = big_int_text(literal.text);
                self.write(&text);
            }
            tsr_ast::PropertyName::NoSubstitutionTemplateLiteral(literal) => {
                let text = format!("`{}`", escape_template(literal.text));
                self.write(&text);
            }
            tsr_ast::PropertyName::ComputedPropertyName(computed) => {
                self.write("[");
                if let Some(expression) = &computed.expression {
                    self.expression(expression);
                }
                self.write("]");
            }
        }
    }

    pub(crate) fn entity_name(&mut self, name: &tsr_ast::EntityName<'_>) {
        match name {
            tsr_ast::EntityName::Identifier(identifier) => self.write(identifier.text),
            tsr_ast::EntityName::QualifiedName(qualified) => {
                if let Some(left) = &qualified.left {
                    self.entity_name(left);
                }
                self.write_raw(".");
                if let Some(right) = qualified.right {
                    self.write_raw(right.text);
                }
            }
        }
    }

    /// A `{ … }` body of class or object members.
    pub(crate) fn class_body(&mut self, members: &[ClassElement<'_>]) {
        self.write("{");
        self.indented(|printer| {
            for member in members {
                printer.newline();
                printer.class_element(member);
            }
        });
        self.newline();
        self.write_raw("}");
    }

    pub(crate) fn object_members(&mut self, members: &[ObjectLiteralElementLike<'_>]) {
        self.write("{");
        self.indented(|printer| {
            for (index, member) in members.iter().enumerate() {
                if index > 0 {
                    printer.write_raw(",");
                }
                printer.newline();
                printer.object_member(member);
            }
        });
        self.newline();
        self.write_raw("}");
    }
}

/// Characters that can begin or continue a multi-character punctuator.
const PUNCTUATION: &str = "+-*/<>=&|!?%^~";

/// Whether two adjacent characters would scan as one token.
///
/// Conservative on purpose: a false positive costs one space in output nobody
/// compares, and a false negative silently changes the tree.
fn would_merge(last: char, next: char) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    if word(last) && word(next) {
        return true;
    }
    // `/` then `/` or `*` opens a comment and swallows the rest of the line.
    if last == '/' && (next == '/' || next == '*') {
        return true;
    }
    // A digit followed by `.` continues the numeric literal.
    if last.is_ascii_digit() && next == '.' {
        return true;
    }
    PUNCTUATION.contains(last) && PUNCTUATION.contains(next)
}

/// Re-quote a decoded string value.
pub(crate) fn quote_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // U+2028/U+2029 are line terminators in JS but not in JSON, and an
            // unescaped one ends the literal.
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Escape a decoded template chunk for emission between backticks.
pub(crate) fn escape_template(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '`' => out.push_str("\\`"),
            '\\' => out.push_str("\\\\"),
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
            c => out.push(c),
        }
    }
    out
}

/// A bigint literal always ends in `n`; the decoded text may not.
pub(crate) fn big_int_text(value: &str) -> String {
    if value.ends_with('n') { value.to_string() } else { format!("{value}n") }
}

/// The source text of a fixed-text token.
///
/// Returns `None` for a token whose text is not fixed, which the caller records
/// as unsupported rather than guessing at.
pub(crate) fn token_text(kind: SyntaxKind) -> Option<&'static str> {
    Some(match kind {
        SyntaxKind::OpenBraceToken => "{",
        SyntaxKind::CloseBraceToken => "}",
        SyntaxKind::OpenParenToken => "(",
        SyntaxKind::CloseParenToken => ")",
        SyntaxKind::OpenBracketToken => "[",
        SyntaxKind::CloseBracketToken => "]",
        SyntaxKind::DotToken => ".",
        SyntaxKind::DotDotDotToken => "...",
        SyntaxKind::SemicolonToken => ";",
        SyntaxKind::CommaToken => ",",
        SyntaxKind::QuestionDotToken => "?.",
        SyntaxKind::LessThanToken => "<",
        SyntaxKind::GreaterThanToken => ">",
        SyntaxKind::LessThanEqualsToken => "<=",
        SyntaxKind::GreaterThanEqualsToken => ">=",
        SyntaxKind::EqualsEqualsToken => "==",
        SyntaxKind::ExclamationEqualsToken => "!=",
        SyntaxKind::EqualsEqualsEqualsToken => "===",
        SyntaxKind::ExclamationEqualsEqualsToken => "!==",
        SyntaxKind::EqualsGreaterThanToken => "=>",
        SyntaxKind::PlusToken => "+",
        SyntaxKind::MinusToken => "-",
        SyntaxKind::AsteriskToken => "*",
        SyntaxKind::AsteriskAsteriskToken => "**",
        SyntaxKind::SlashToken => "/",
        SyntaxKind::PercentToken => "%",
        SyntaxKind::PlusPlusToken => "++",
        SyntaxKind::MinusMinusToken => "--",
        SyntaxKind::LessThanLessThanToken => "<<",
        SyntaxKind::GreaterThanGreaterThanToken => ">>",
        SyntaxKind::GreaterThanGreaterThanGreaterThanToken => ">>>",
        SyntaxKind::AmpersandToken => "&",
        SyntaxKind::BarToken => "|",
        SyntaxKind::CaretToken => "^",
        SyntaxKind::ExclamationToken => "!",
        SyntaxKind::TildeToken => "~",
        SyntaxKind::AmpersandAmpersandToken => "&&",
        SyntaxKind::BarBarToken => "||",
        SyntaxKind::QuestionQuestionToken => "??",
        SyntaxKind::QuestionToken => "?",
        SyntaxKind::ColonToken => ":",
        SyntaxKind::AtToken => "@",
        SyntaxKind::BacktickToken => "`",
        SyntaxKind::EqualsToken => "=",
        SyntaxKind::PlusEqualsToken => "+=",
        SyntaxKind::MinusEqualsToken => "-=",
        SyntaxKind::AsteriskEqualsToken => "*=",
        SyntaxKind::AsteriskAsteriskEqualsToken => "**=",
        SyntaxKind::SlashEqualsToken => "/=",
        SyntaxKind::PercentEqualsToken => "%=",
        SyntaxKind::LessThanLessThanEqualsToken => "<<=",
        SyntaxKind::GreaterThanGreaterThanEqualsToken => ">>=",
        SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken => ">>>=",
        SyntaxKind::AmpersandEqualsToken => "&=",
        SyntaxKind::BarEqualsToken => "|=",
        SyntaxKind::BarBarEqualsToken => "||=",
        SyntaxKind::AmpersandAmpersandEqualsToken => "&&=",
        SyntaxKind::QuestionQuestionEqualsToken => "??=",
        SyntaxKind::CaretEqualsToken => "^=",
        SyntaxKind::InKeyword => "in",
        SyntaxKind::InstanceOfKeyword => "instanceof",
        other => return keyword_text(other),
    })
}

/// The text of a keyword token.
fn keyword_text(kind: SyntaxKind) -> Option<&'static str> {
    let name = kind.name();
    let text = name.strip_suffix("Keyword")?;
    // `SyntaxKind::name` is PascalCase; every keyword is its lowercase spelling
    // except the ones that are not identifiers at all, which are handled above.
    Some(match text {
        "Abstract" => "abstract",
        "Accessor" => "accessor",
        "Any" => "any",
        "As" => "as",
        "Assert" => "assert",
        "Asserts" => "asserts",
        "Async" => "async",
        "Await" => "await",
        "BigInt" => "bigint",
        "Boolean" => "boolean",
        "Break" => "break",
        "Case" => "case",
        "Catch" => "catch",
        "Class" => "class",
        "Const" => "const",
        "Constructor" => "constructor",
        "Continue" => "continue",
        "Debugger" => "debugger",
        "Declare" => "declare",
        "Default" => "default",
        "Defer" => "defer",
        "Delete" => "delete",
        "Do" => "do",
        "Else" => "else",
        "Enum" => "enum",
        "Export" => "export",
        "Extends" => "extends",
        "False" => "false",
        "Finally" => "finally",
        "For" => "for",
        "From" => "from",
        "Function" => "function",
        "Get" => "get",
        "Global" => "global",
        "If" => "if",
        "Implements" => "implements",
        "Import" => "import",
        "Infer" => "infer",
        "Interface" => "interface",
        "Intrinsic" => "intrinsic",
        "Is" => "is",
        "KeyOf" => "keyof",
        "Let" => "let",
        "Module" => "module",
        "Namespace" => "namespace",
        "Never" => "never",
        "New" => "new",
        "Null" => "null",
        "Number" => "number",
        "Object" => "object",
        "Of" => "of",
        "Out" => "out",
        "Override" => "override",
        "Package" => "package",
        "Private" => "private",
        "Protected" => "protected",
        "Public" => "public",
        "Readonly" => "readonly",
        "Require" => "require",
        "Return" => "return",
        "Satisfies" => "satisfies",
        "Set" => "set",
        "Static" => "static",
        "String" => "string",
        "Super" => "super",
        "Switch" => "switch",
        "Symbol" => "symbol",
        "This" => "this",
        "Throw" => "throw",
        "True" => "true",
        "Try" => "try",
        "Type" => "type",
        "TypeOf" => "typeof",
        "Undefined" => "undefined",
        "Unique" => "unique",
        "Unknown" => "unknown",
        "Using" => "using",
        "Var" => "var",
        "Void" => "void",
        "While" => "while",
        "With" => "with",
        "Yield" => "yield",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_words_are_separated() {
        assert!(would_merge('n', 'x'), "`return` + `x` must not become `returnx`");
        assert!(!would_merge(')', 'x'));
    }

    #[test]
    fn adjacent_punctuation_that_could_scan_as_one_token_is_separated() {
        assert!(would_merge('+', '+'), "`a` `+` `+b` must not become `a++b`");
        assert!(would_merge('<', '='));
        assert!(would_merge('/', '/'), "a comment would swallow the rest of the line");
    }

    #[test]
    fn a_string_is_requoted_from_its_decoded_value() {
        // The scanner hands back the decoded value, so the printer owns escaping.
        assert_eq!(quote_string(r#"a"b"#), r#""a\"b""#);
        assert_eq!(quote_string("a\\b"), r#""a\\b""#);
        assert_eq!(quote_string("a\nb"), r#""a\nb""#);
        // U+2028 is a line terminator in JavaScript, so it cannot be left bare
        // inside a string literal even though it is printable.
        assert_eq!(quote_string("\u{2028}"), r#""\u2028""#);
    }

    #[test]
    fn a_template_chunk_escapes_its_own_terminators() {
        assert_eq!(escape_template("a`b"), "a\\`b");
        assert_eq!(escape_template("a${b"), "a\\${b");
        assert_eq!(escape_template("a$b"), "a$b", "a lone dollar is not a terminator");
    }

    #[test]
    fn a_bigint_keeps_its_suffix() {
        assert_eq!(big_int_text("10"), "10n");
        assert_eq!(big_int_text("10n"), "10n");
    }

    #[test]
    fn keywords_come_from_the_generated_kind_names() {
        assert_eq!(token_text(SyntaxKind::ConstKeyword), Some("const"));
        assert_eq!(token_text(SyntaxKind::TypeOfKeyword), Some("typeof"));
        assert_eq!(token_text(SyntaxKind::EqualsGreaterThanToken), Some("=>"));
    }
}
