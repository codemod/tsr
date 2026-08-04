//! Prints the AST back to TypeScript source.
//!
//! # A port, and where it deviates
//!
//! Ported from typescript-go's printer (`internal/printer/printer.go`,
//! `textwriter.go`), under [ADR-0001](../../../docs/adr/0001-idiomatic-rewrite.md)'s
//! "port, don't reinvent" default. The structure follows upstream's:
//!
//! - [`writer::TextWriter`] ports `textWriter`, deferred indentation included.
//! - [`list_format::ListFormat`] ports the `LF*` table one-for-one. Every child
//!   list is emitted through [`Printer::emit_list`], as upstream does, so layout
//!   lives in the format rather than at the call site.
//! - Dispatch is a `match` per category rather than 284 methods, which is the
//!   idiomatic form of upstream's central `switch node.Kind`. Each arm names the
//!   `emitX` it ports, so a `grep` for an upstream function lands on our code —
//!   which is what [conventions.md](../../../docs/conventions.md) requires the
//!   anchors for.
//!
//! Four deliberate deviations, each with a reason rather than an omission:
//!
//! 1. **A separator guard.** [`Printer::write`] inserts a space when the previous
//!    character and the next would scan as one token; upstream places every space
//!    by hand. Ported without it, this printer emitted `1.toString()` for
//!    `1 .toString()`. Whitespace is not in the tree, so the guard cannot cost
//!    correctness.
//! 2. **No comments, no source maps.** Neither is needed to preserve a tree; both
//!    are a third of upstream's file.
//! 3. **`PRESERVE_LINES` and `PREFER_NEW_LINE` degrade to single-line.** They
//!    consult original node positions to keep the author's layout, which this
//!    printer does not reproduce.
//! 4. **No precedence table.** Upstream re-derives where parentheses are needed;
//!    `ParenthesizedExpression` is in the tree, so printing children in order
//!    reproduces the grouping for free. Phase 5 needs the table.
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

pub(crate) use list_format::ListFormat;
use tsr_ast::{
    ClassElement, Expression, ModifierLike, Node, ObjectLiteralElementLike, SourceFile, SyntaxKind,
    Token, TypeNode,
};

mod expressions;
mod jsx;
mod list_format;
mod statements;
mod types;
mod writer;

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
    printer.emit_source_file(file);
    Printed { text: printer.writer.into_string(), unsupported: printer.unsupported }
}

/// Ported from typescript-go's `Printer` (`internal/printer/printer.go`).
///
/// Upstream's `Printer` also carries an `EmitContext`, a name generator, comment
/// and source-map state, and `EmitResolver`. None is needed to preserve a tree;
/// each is listed in the crate docs as deliberately absent.
pub(crate) struct Printer<'t> {
    writer: writer::TextWriter,
    unsupported: Vec<SyntaxKind>,
    /// Whether the last text written was a numeric literal. See
    /// [`Printer::write_numeric_literal`].
    last_was_numeric: bool,
    /// `const`/`let` live in `NodeFlags`, not in the tree, so printing a variable
    /// statement needs the side table the parser filled in.
    nodes: &'t tsr_ast::NodeTable,
}

impl<'t> Printer<'t> {
    fn new(nodes: &'t tsr_ast::NodeTable) -> Self {
        Self {
            writer: writer::TextWriter::new(),
            unsupported: Vec::new(),
            last_was_numeric: false,
            nodes,
        }
    }

    /// Ported from `Printer.emitSourceFile` (`internal/printer/printer.go`).
    ///
    /// Upstream also emits a shebang, prologue directives, triple-slash
    /// directives and helpers before the statements. All four are emit concerns
    /// rather than tree-preserving ones.
    fn emit_source_file(&mut self, file: &SourceFile<'_>) {
        self.emit_list(file.statements, ListFormat::SOURCE_FILE_STATEMENTS, |printer, node| {
            printer.emit_statement(node);
        });
    }

    // ----- the writer, ported from `Printer.write*` --------------------------

    /// Ported from `Printer.write`.
    ///
    /// **Deviation, deliberate:** upstream writes the text unconditionally and
    /// relies on every call site having placed its own spaces through
    /// `writeSpace()`. This inserts a separator first when the previous character
    /// and the next would scan as one token. Ported without the guard, this
    /// printer emitted `1.toString()` for `1 .toString()`. Whitespace is not in
    /// the tree, so the guard cannot cost correctness — only tidiness.
    pub(crate) fn write(&mut self, text: &str) {
        let Some(next) = text.chars().next() else { return };
        if let Some(last) = self.writer.last_char()
            && would_merge(last, next, self.last_was_numeric)
        {
            self.writer.write(" ");
        }
        self.last_was_numeric = false;
        self.writer.write(text);
    }

    /// `write`, for a numeric or bigint literal.
    ///
    /// The separator guard is character-based, and one of its rules — a digit
    /// followed by `.` continues the literal — is a fact about *tokens*, not
    /// characters. `c1.foo` ends its first token in a digit too, and printed as
    /// `c1 .foo`, which parses to the same tree and so survived the round trip. The
    /// flag is what tells the two apart, and it is set only here.
    pub(crate) fn write_numeric_literal(&mut self, text: &str) {
        self.write(text);
        self.last_was_numeric = true;
    }

    /// Ported from `Printer.writeKeyword`.
    pub(crate) fn write_keyword(&mut self, text: &str) {
        self.write(text);
    }

    /// Ported from `Printer.writePunctuation`.
    pub(crate) fn write_punctuation(&mut self, text: &str) {
        self.write(text);
    }

    /// Ported from `Printer.writeOperator`.
    pub(crate) fn write_operator(&mut self, text: &str) {
        self.write(text);
    }

    /// Ported from `Printer.writeLiteral`.
    ///
    /// Unused so far: literals currently go through `write`. Kept because the
    /// write-kind split is upstream's, and a syntax-highlighting writer (Phase 7)
    /// needs each kind to be distinguishable at the call site.
    #[allow(dead_code)]
    pub(crate) fn write_literal(&mut self, text: &str) {
        self.write(text);
    }

    /// Ported from `Printer.writeSpace`.
    pub(crate) fn write_space(&mut self) {
        self.writer.write(" ");
    }

    /// Ported from `Printer.writeLine`.
    pub(crate) fn write_line(&mut self) {
        self.writer.write_line();
    }

    /// Ported from `Printer.increaseIndent`.
    pub(crate) fn increase_indent(&mut self) {
        self.writer.increase_indent();
    }

    /// Ported from `Printer.decreaseIndent`.
    pub(crate) fn decrease_indent(&mut self) {
        self.writer.decrease_indent();
    }

    /// Write text with **no** separator check. JSX only.
    ///
    /// Inside JSX the bypass is required rather than convenient: `JsxText` is a
    /// node, so an inserted space would change the tree.
    pub(crate) fn write_raw(&mut self, text: &str) {
        self.writer.raw_write(text);
    }

    /// How much text has been written, for assertions.
    pub(crate) fn len(&self) -> usize {
        self.writer.text_pos()
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

    /// Record a node kind this printer does not implement.
    ///
    /// No upstream counterpart: upstream panics on an unexpected kind, because it
    /// is complete. Recording instead is what lets the conformance suite report the
    /// unimplemented surface as a measured worklist.
    pub(crate) fn unsupported(&mut self, kind: SyntaxKind) {
        if !self.unsupported.contains(&kind) {
            self.unsupported.push(kind);
        }
    }

    // ----- lists, ported from `Printer.emitList` -----------------------------

    /// Ported from `Printer.emitList` (`internal/printer/printer.go`).
    ///
    /// Every child list goes through here, exactly as upstream: the layout lives
    /// in the [`ListFormat`] rather than at the call site, which is what makes
    /// "how is an enum body laid out?" a question with one answer.
    pub(crate) fn emit_list<T>(
        &mut self,
        children: &[T],
        format: ListFormat,
        mut emit: impl FnMut(&mut Self, &T),
    ) {
        if children.is_empty() && format.contains(ListFormat::OPTIONAL_IF_EMPTY) {
            return;
        }
        if let Some(open) = format.opening_bracket() {
            self.write_punctuation(open);
        }
        if children.is_empty() {
            // Ported from `emitListRange`'s empty branch (`printer.go:4745`). An
            // empty *multi-line* list is not `{}` — it is a brace, a line break and
            // a brace, which is how upstream writes `interface I {\n}` for
            // `interface I { }`. An empty list with `SPACE_BETWEEN_BRACES` gets a
            // single space unless `NO_SPACE_IF_EMPTY` says otherwise.
            //
            // An earlier version wrote a space for *every* empty bracketed list,
            // which produced `f( )` and `{ }`, and a version after that wrote
            // nothing at all, which produced `interface I {}` where upstream writes
            // two lines. All three parse identically; only a byte comparison can
            // tell them apart, which is why the round trip carried the bug for
            // 11,726 cases.
            if format.is_multi_line() {
                self.write_line();
            } else if format.contains(ListFormat::SPACE_BETWEEN_BRACES)
                && !format.contains(ListFormat::NO_SPACE_IF_EMPTY)
            {
                self.write_space();
            }
        } else {
            self.emit_list_items(children, format, &mut emit);
        }
        if let Some(close) = format.closing_bracket() {
            self.write_punctuation(close);
        }
        if format.contains(ListFormat::SPACE_AFTER_LIST) && !children.is_empty() {
            self.write_space();
        }
    }

    /// Ported from `Printer.emitListItems` (`internal/printer/printer.go`).
    ///
    /// Upstream's version also drives comment emission and consults the original
    /// node positions through `getLeadingLineTerminatorCount` and
    /// `getSeparatingLineTerminatorCount` to preserve the author's line breaks.
    /// Neither survives here — comments are not emitted and layout is not
    /// preserved — so those degrade to "a line break iff the format is
    /// multi-line". The delimiter, indent, bracket and trailing-comma logic is
    /// upstream's.
    fn emit_list_items<T>(
        &mut self,
        children: &[T],
        format: ListFormat,
        emit: &mut impl FnMut(&mut Self, &T),
    ) {
        if children.is_empty() {
            return;
        }
        if format.is_multi_line() {
            self.write_line();
        } else if format.contains(ListFormat::SPACE_BETWEEN_BRACES) {
            self.write_space();
        }
        if format.contains(ListFormat::INDENTED) {
            self.increase_indent();
        }

        for (index, child) in children.iter().enumerate() {
            if index > 0 {
                self.write_delimiter(format);
                if format.is_multi_line() {
                    self.write_line();
                } else if format.contains(ListFormat::SPACE_BETWEEN_SIBLINGS) {
                    self.write_space();
                }
            }
            emit(self, child);
        }

        if format.contains(ListFormat::INDENTED) {
            self.decrease_indent();
        }
        if format.is_multi_line() && !format.contains(ListFormat::NO_TRAILING_NEW_LINE) {
            self.write_line();
        } else if format.contains(ListFormat::SPACE_BETWEEN_BRACES) {
            self.write_space();
        }
    }

    /// Ported from `Printer.writeDelimiter` (`internal/printer/printer.go`).
    fn write_delimiter(&mut self, format: ListFormat) {
        match format.intersection(ListFormat::DELIMITERS_MASK) {
            ListFormat::COMMA_DELIMITED => self.write_punctuation(","),
            ListFormat::BAR_DELIMITED => {
                self.write_space();
                self.write_punctuation("|");
            }
            ListFormat::AMPERSAND_DELIMITED => {
                self.write_space();
                self.write_punctuation("&");
            }
            ListFormat::ASTERISK_DELIMITED => {
                self.write_space();
                self.write_punctuation("*");
                self.write_space();
            }
            _ => {}
        }
    }

    // ----- shared helpers ----------------------------------------------------

    /// Ported from `Printer.emitTokenNode` (`internal/printer/printer.go`).
    pub(crate) fn emit_token_node(&mut self, token: &Token<'_>) {
        match token_text(token.kind) {
            Some(text) => self.write(text),
            None => self.unsupported(token.kind),
        }
    }

    /// Ported from `Printer.emitModifierList` (`internal/printer/printer.go`),
    /// which emits `LFModifiers` — space-separated, with a trailing space.
    pub(crate) fn emit_modifier_list(&mut self, modifiers: &[ModifierLike<'_>]) {
        self.emit_list(modifiers, ListFormat::MODIFIERS, |printer, modifier| match modifier {
            ModifierLike::Token(token) => printer.emit_token_node(token),
            ModifierLike::Decorator(decorator) => printer.emit_decorator(decorator),
        });
    }

    /// Ported from `Printer.emitDecorator` (`internal/printer/printer.go`).
    fn emit_decorator(&mut self, decorator: &tsr_ast::Decorator<'_>) {
        self.write_punctuation("@");
        if let Some(expression) = &decorator.expression {
            self.any_expression(Node::from(*expression));
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
            Ok(expression) => self.emit_expression(&expression),
            Err(other) => {
                let kind = self.kind_of(other.node_id());
                self.unsupported(kind);
            }
        }
    }

    /// Ported from `Printer.emitTypeParameters` (`internal/printer/printer.go`),
    /// which emits `LFTypeParameters` — angle-bracketed and optional when empty.
    pub(crate) fn emit_type_parameters(
        &mut self,
        parameters: &[&tsr_ast::TypeParameterDeclaration<'_>],
    ) {
        self.emit_list(parameters, ListFormat::TYPE_PARAMETERS, |printer, parameter| {
            printer.emit_type_parameter(parameter);
        });
    }

    /// Ported from `Printer.emitTypeParameter` (`internal/printer/printer.go`).
    fn emit_type_parameter(&mut self, parameter: &tsr_ast::TypeParameterDeclaration<'_>) {
        self.emit_modifier_list(parameter.modifiers);
        if let Some(name) = parameter.name {
            self.write(name.text);
        }
        if let Some(constraint) = parameter.constraint {
            self.write_space();
            self.write_keyword("extends");
            self.write_space();
            self.emit_type_node(&constraint);
        }
        if let Some(default) = parameter.default_type {
            self.write_space();
            self.write_operator("=");
            self.write_space();
            self.emit_type_node(&default);
        }
    }

    /// Ported from `Printer.emitTypeArguments` (`internal/printer/printer.go`),
    /// which emits `LFTypeArguments`.
    pub(crate) fn emit_type_arguments(&mut self, arguments: &[TypeNode<'_>]) {
        self.emit_list(arguments, ListFormat::TYPE_ARGUMENTS, |printer, argument| {
            printer.emit_type_node(argument);
        });
    }

    /// Ported from `Printer.emitParameters` (`internal/printer/printer.go`),
    /// which emits `LFParameters`. Parenthesised, so unlike a type-parameter list
    /// the brackets are written even when the list is empty.
    pub(crate) fn emit_parameters(&mut self, parameters: &[&tsr_ast::ParameterDeclaration<'_>]) {
        self.emit_list(parameters, ListFormat::PARAMETERS, |printer, parameter| {
            printer.emit_parameter(parameter);
        });
    }

    /// Ported from `Printer.emitParameter` (`internal/printer/printer.go`).
    fn emit_parameter(&mut self, parameter: &tsr_ast::ParameterDeclaration<'_>) {
        self.emit_modifier_list(parameter.modifiers);
        if parameter.dot_dot_dot_token.is_some() {
            self.write_punctuation("...");
        }
        if let Some(name) = &parameter.name {
            self.emit_binding_name(name);
        }
        if parameter.question_token.is_some() {
            self.write_punctuation("?");
        }
        if let Some(node) = &parameter.r#type {
            self.write_punctuation(":");
            self.write_space();
            self.emit_type_node(node);
        }
        if let Some(initializer) = &parameter.initializer {
            self.write_space();
            self.write_operator("=");
            self.write_space();
            self.emit_expression(initializer);
        }
    }

    pub(crate) fn emit_binding_name(&mut self, name: &tsr_ast::BindingName<'_>) {
        match name {
            tsr_ast::BindingName::Identifier(identifier) => self.write(identifier.text),
            tsr_ast::BindingName::BindingPattern(pattern) => self.emit_binding_pattern(pattern),
        }
    }

    /// Ported from `Printer.emitObjectBindingPattern`/`emitArrayBindingPattern`
    /// (`internal/printer/printer.go`), which differ only in their list format.
    pub(crate) fn emit_binding_pattern(&mut self, pattern: &tsr_ast::BindingPattern<'_>) {
        // The `kind` field is the opening bracket token, not the pattern's kind.
        let object = self.kind_of(pattern.node_id) == SyntaxKind::ObjectBindingPattern;
        let (format, open, close) = if object {
            (ListFormat::OBJECT_BINDING_PATTERN_ELEMENTS, "{", "}")
        } else {
            (ListFormat::ARRAY_BINDING_PATTERN_ELEMENTS, "[", "]")
        };
        self.write_punctuation(open);
        self.emit_list(pattern.elements, format, |printer, element| {
            printer.emit_binding_element(element);
        });
        self.write_punctuation(close);
    }

    /// Ported from `Printer.emitBindingElement` (`internal/printer/printer.go`).
    fn emit_binding_element(&mut self, element: &tsr_ast::BindingElement<'_>) {
        if element.dot_dot_dot_token.is_some() {
            self.write_punctuation("...");
        }
        if let Some(property) = &element.property_name {
            self.emit_property_name(property);
            self.write_punctuation(":");
            self.write_space();
        }
        if let Some(name) = &element.name {
            self.emit_binding_name(name);
        }
        if let Some(initializer) = &element.initializer {
            self.write_space();
            self.write_operator("=");
            self.write_space();
            self.emit_expression(initializer);
        }
    }

    pub(crate) fn emit_property_name(&mut self, name: &tsr_ast::PropertyName<'_>) {
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
                    self.emit_expression(expression);
                }
                self.write("]");
            }
        }
    }

    pub(crate) fn emit_entity_name(&mut self, name: &tsr_ast::EntityName<'_>) {
        match name {
            tsr_ast::EntityName::Identifier(identifier) => self.write(identifier.text),
            tsr_ast::EntityName::QualifiedName(qualified) => {
                if let Some(left) = &qualified.left {
                    self.emit_entity_name(left);
                }
                self.write(".");
                if let Some(right) = qualified.right {
                    self.write(right.text);
                }
            }
        }
    }

    /// Ported from the `LFClassMembers` emit sites in
    /// `internal/printer/printer.go` — `Printer.emitClassDeclaration` (`:3764`) and
    /// `Printer.emitClassExpression` (`:2956`), which brace the member list and
    /// emit it through `emitClassElement`.
    ///
    /// Upstream has no single `emitClassBody`: both call sites inline the braces.
    /// This anchor named one anyway until `cargo xtask anchors` asked upstream.
    pub(crate) fn class_body(&mut self, members: &[ClassElement<'_>]) {
        self.write_punctuation("{");
        self.emit_list(members, ListFormat::CLASS_MEMBERS, |printer, member| {
            printer.emit_class_element(member);
        });
        self.write_punctuation("}");
    }

    /// Ported from `Printer.emitObjectLiteralExpression`
    /// (`internal/printer/printer.go`), which emits `LFObjectLiteralExpressionProperties`.
    /// That format carries its own braces, so unlike a class body this does not
    /// write them.
    pub(crate) fn object_members(&mut self, members: &[ObjectLiteralElementLike<'_>]) {
        self.emit_list(
            members,
            ListFormat::OBJECT_LITERAL_EXPRESSION_PROPERTIES,
            |printer, member| printer.emit_object_member(member),
        );
    }
}

/// Characters that can begin or continue a multi-character punctuator.
const PUNCTUATION: &str = "+-*/<>=&|!?%^~";

/// Whether two adjacent characters would scan as one token.
///
/// Conservative on purpose: a false positive costs one space in output nobody
/// compares, and a false negative silently changes the tree.
fn would_merge(last: char, next: char, last_was_numeric: bool) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    if word(last) && word(next) {
        return true;
    }
    // `/` then `/` or `*` opens a comment and swallows the rest of the line.
    if last == '/' && (next == '/' || next == '*') {
        return true;
    }
    // A digit followed by `.` continues the numeric literal — but only if the
    // digit ended a *literal*. `c1.foo` also ends in a digit.
    if last_was_numeric && last.is_ascii_digit() && next == '.' {
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
        assert!(would_merge('n', 'x', false), "`return` + `x` must not become `returnx`");
        assert!(!would_merge(')', 'x', false));
    }

    #[test]
    fn adjacent_punctuation_that_could_scan_as_one_token_is_separated() {
        assert!(would_merge('+', '+', false), "`a` `+` `+b` must not become `a++b`");
        assert!(would_merge('<', '=', false));
        assert!(would_merge('/', '/', false), "a comment would swallow the rest of the line");
    }

    #[test]
    fn the_digit_then_dot_rule_needs_a_literal_not_just_a_digit() {
        // `1` `.toString()` must not become `1.toString()`, which does not parse…
        assert!(would_merge('1', '.', true));
        // …but `c1` `.foo` is two tokens already, and `c1 .foo` is not what any
        // baseline writes. The guard is character-based; this one rule is not.
        assert!(!would_merge('1', '.', false));
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
