//! Expressions.
//!
//! # Parentheses are nodes, so precedence is not this printer's problem
//!
//! TypeScript keeps `ParenthesizedExpression` in the tree rather than recovering
//! grouping from precedence, so `(a + b) * c` and `a + b * c` are already
//! different trees on the way in. Printing each node's children in order
//! therefore reproduces the grouping for free, and a precedence table — the part
//! of upstream's printer that decides where to *add* parentheses — is not needed
//! for a round trip. Phase 5 will need it, because upstream's emit removes
//! redundant parentheses and this does not.

use tsr_ast::Expression;

use crate::{ListFormat, Printer, big_int_text, escape_template, quote_string};

impl Printer<'_> {
    /// `a + b + c …`, walking the left spine iteratively.
    ///
    /// Ported from `Printer.emitBinaryExpression` (`internal/printer/printer.go`),
    /// which recurses. It can: Go grows a goroutine's stack on demand. Here the
    /// stack is fixed, and `a + a + a …` is **left-leaning and unbounded** —
    /// `parser.rs` climbs precedence in a loop, so its own `descend()` guard never
    /// fires and the tree is as deep as the chain. The corpus contains a
    /// 4,958-operand chain (`compiler/binderBinaryExpressionStress`), and
    /// recursing once per operand is what overflowed CI's debug run.
    ///
    /// strada solves this the same way and names it in that test's header comment
    /// ("we have to skip the trampoline"). ADR-0029 rejects an explicit work stack
    /// as a *policy* while retaining it for exactly this: one known-pathological
    /// path, where the transformation is a pure reassociation of the emit order
    /// and the output is byte-identical.
    ///
    /// Only the left spine is flattened. `a = b = c` is right-leaning and recurses
    /// through `right`, which the depth guard covers instead; no corpus file gets
    /// near it.
    fn emit_binary_expression<'e>(&mut self, node: &'e tsr_ast::BinaryExpression<'e>) {
        // Collect the spine, shallowest first.
        let mut spine = vec![node];
        while let Some(Expression::BinaryExpression(left)) =
            spine.last().expect("spine is never empty").left
        {
            spine.push(left);
        }

        // The deepest node's left is not itself a binary expression, or it would
        // be on the spine. Everything below it is ordinary depth.
        if let Some(left) = &spine.last().expect("spine is never empty").left {
            self.emit_expression(left);
        }

        // Then each operator and right operand, innermost outwards — the order a
        // recursive emit would have produced on the way back up.
        for binary in spine.iter().rev() {
            self.write(" ");
            if let Some(token) = binary.operator_token {
                self.emit_token_node(token);
            }
            self.write(" ");
            if let Some(right) = &binary.right {
                self.emit_expression(right);
            }
        }
    }

    /// Emit an expression, growing the stack if the tree is deeply nested.
    ///
    /// The parser caps its own recursion but legitimately produces trees deeper
    /// than that cap, so this walk cannot assume a shallow input. See
    /// [`tsr_core::stack::ensure_sufficient`] and ADR-0030 — on wasm32 this is a
    /// plain call and deep input traps.
    pub(crate) fn emit_expression(&mut self, expression: &Expression<'_>) {
        tsr_core::stack::ensure_sufficient(|| self.emit_expression_inner(expression));
    }

    fn emit_expression_inner(&mut self, expression: &Expression<'_>) {
        match expression {
            // Ported from `Printer.emitIdentifierReference` (`internal/printer/printer.go`).
            Expression::Identifier(node) => self.write(node.text),
            // Ported from `Printer.emitPrivateIdentifier` (`internal/printer/printer.go`).
            Expression::PrivateIdentifier(node) => self.write(node.text),
            // Ported from `Printer.emitNumericLiteral` (`internal/printer/printer.go`).
            Expression::NumericLiteral(node) => self.write_numeric_literal(node.text),
            // Ported from `Printer.emitBigIntLiteral` (`internal/printer/printer.go`).
            Expression::BigIntLiteral(node) => {
                let text = big_int_text(node.text);
                self.write_numeric_literal(&text);
            }
            // Ported from `Printer.emitStringLiteral` (`internal/printer/printer.go`).
            Expression::StringLiteral(node) => {
                let text = quote_string(node.text);
                self.write(&text);
            }
            // Ported from `Printer.emitRegularExpressionLiteral` (`internal/printer/printer.go`).
            Expression::RegularExpressionLiteral(node) => self.write(node.text),
            // Ported from `Printer.emitNoSubstitutionTemplateLiteral` (`internal/printer/printer.go`).
            Expression::NoSubstitutionTemplateLiteral(node) => {
                let text = format!("`{}`", escape_template(node.text));
                self.write(&text);
            }
            Expression::KeywordExpression(node) => match crate::token_text(node.kind) {
                Some(text) => self.write(text),
                None => self.unsupported(node.kind),
            },
            // Ported from `Printer.emitTemplateExpression` (`internal/printer/printer.go`).
            Expression::TemplateExpression(node) => {
                if let Some(head) = node.head {
                    // `raw_text` is the whole token, delimiters included — it is
                    // already `` `abc${ ``, so wrapping it again produces nonsense.
                    self.write(head.raw_text);
                }
                for span in node.template_spans {
                    if let Some(expression) = &span.expression {
                        self.emit_expression(expression);
                    }
                    self.template_chunk(span.literal.as_ref());
                }
            }
            // Ported from `Printer.emitTaggedTemplateExpression` (`internal/printer/printer.go`).
            Expression::TaggedTemplateExpression(node) => {
                if let Some(tag) = &node.tag {
                    self.emit_expression(tag);
                }
                self.emit_type_arguments(node.type_arguments);
                if let Some(template) = &node.template {
                    self.any_expression(tsr_ast::Node::from(*template));
                }
            }
            // Ported from `Printer.emitParenthesizedExpression` (`internal/printer/printer.go`).
            Expression::ParenthesizedExpression(node) => {
                self.write("(");
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
                self.write(")");
            }
            // Ported from `Printer.emitArrayLiteralExpression` (`internal/printer/printer.go`).
            Expression::ArrayLiteralExpression(node) => {
                let ends_in_elision =
                    matches!(node.elements.last(), Some(Expression::OmittedExpression(_)));
                self.emit_list_with_trailing_delimiter(
                    node.elements,
                    ListFormat::ARRAY_LITERAL_EXPRESSION_ELEMENTS,
                    ends_in_elision,
                    |printer, element| printer.emit_expression(element),
                );
            }
            // Ported from `Printer.emitObjectLiteralExpression` (`internal/printer/printer.go`).
            Expression::ObjectLiteralExpression(node) => self.object_members(node.properties),
            // Ported from `Printer.emitPropertyAccessExpression` (`internal/printer/printer.go`).
            Expression::PropertyAccessExpression(node) => {
                if let Some(target) = &node.expression {
                    self.emit_expression(target);
                }
                if let Some(token) = node.question_dot_token {
                    self.emit_token_node(token);
                } else {
                    self.write(".");
                }
                match &node.name {
                    Some(tsr_ast::MemberName::Identifier(name)) => self.write(name.text),
                    Some(tsr_ast::MemberName::PrivateIdentifier(name)) => {
                        self.write(name.text);
                    }
                    None => {}
                }
            }
            // Ported from `Printer.emitElementAccessExpression` (`internal/printer/printer.go`).
            Expression::ElementAccessExpression(node) => {
                if let Some(target) = &node.expression {
                    self.emit_expression(target);
                }
                if let Some(token) = node.question_dot_token {
                    self.emit_token_node(token);
                }
                self.write("[");
                if let Some(argument) = &node.argument_expression {
                    self.emit_expression(argument);
                }
                self.write("]");
            }
            // Ported from `Printer.emitCallExpression` (`internal/printer/printer.go`).
            Expression::CallExpression(node) => {
                if let Some(target) = &node.expression {
                    self.emit_expression(target);
                }
                if let Some(token) = node.question_dot_token {
                    self.emit_token_node(token);
                }
                self.emit_type_arguments(node.type_arguments);
                self.arguments(node.arguments);
            }
            // Ported from `Printer.emitNewExpression` (`internal/printer/printer.go`).
            Expression::NewExpression(node) => {
                self.write("new ");
                if let Some(target) = &node.expression {
                    self.emit_expression(target);
                }
                self.emit_type_arguments(node.type_arguments);
                // `new C` and `new C()` are different trees: the argument list is
                // optional and its absence is recorded, so it must not be invented.
                // `new C` and `new C()` parse to the same tree — both carry an
                // empty argument list — so always emitting `()` loses nothing.
                let callee_already_carries_recovered_call = node.arguments.is_empty()
                    && matches!(
                        node.expression,
                        Some(Expression::TypeAssertion(assertion))
                            if matches!(assertion.expression, Some(Expression::CallExpression(_)))
                    );
                if !callee_already_carries_recovered_call {
                    self.arguments(node.arguments);
                }
            }
            // Ported from `Printer.emitBinaryExpression` (`internal/printer/printer.go`).
            Expression::BinaryExpression(node) => self.emit_binary_expression(node),
            // Ported from `Printer.emitPrefixUnaryExpression` (`internal/printer/printer.go`).
            Expression::PrefixUnaryExpression(node) => {
                self.emit_token_node(node.operator);
                if let Some(operand) = &node.operand {
                    self.emit_expression(operand);
                }
            }
            // Ported from `Printer.emitPostfixUnaryExpression` (`internal/printer/printer.go`).
            Expression::PostfixUnaryExpression(node) => {
                if let Some(operand) = &node.operand {
                    self.emit_expression(operand);
                }
                self.emit_token_node(node.operator);
            }
            // Ported from `Printer.emitConditionalExpression` (`internal/printer/printer.go`).
            Expression::ConditionalExpression(node) => {
                if let Some(condition) = &node.condition {
                    self.emit_expression(condition);
                }
                self.write(" ? ");
                if let Some(when_true) = &node.when_true {
                    self.emit_expression(when_true);
                }
                self.write(" : ");
                if let Some(when_false) = &node.when_false {
                    self.emit_expression(when_false);
                }
            }
            // Ported from `Printer.emitArrowFunction` (`internal/printer/printer.go`).
            Expression::ArrowFunction(node) => {
                self.emit_modifier_list(node.modifiers);
                self.emit_type_parameters(node.type_parameters);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.write(" => ");
                match &node.body {
                    Some(tsr_ast::ConciseBody::Block(block)) => self.emit_block(block),
                    Some(other) => self.any_expression(tsr_ast::Node::from(*other)),
                    None => {}
                }
            }
            // Ported from `Printer.emitFunctionExpression` (`internal/printer/printer.go`).
            Expression::FunctionExpression(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("function");
                if node.asterisk_token.is_some() {
                    self.write("*");
                }
                if let Some(name) = node.name {
                    self.write(" ");
                    self.write(name.text);
                }
                self.emit_type_parameters(node.type_parameters);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                if let Some(tsr_ast::FunctionBody::Block(block)) = &node.body {
                    self.write(" ");
                    self.emit_block(block);
                }
            }
            // Ported from `Printer.emitClassExpression` (`internal/printer/printer.go`).
            Expression::ClassExpression(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("class");
                if let Some(name) = node.name {
                    self.write(" ");
                    self.write(name.text);
                }
                self.emit_type_parameters(node.type_parameters);
                self.class_heritage(node.heritage_clauses);
                self.write(" ");
                self.class_body(node.members);
            }
            // Ported from `Printer.emitSpreadElement` (`internal/printer/printer.go`).
            Expression::SpreadElement(node) => {
                self.write("...");
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
            }
            // Ported from `Printer.emitAwaitExpression` (`internal/printer/printer.go`).
            Expression::AwaitExpression(node) => {
                self.write("await ");
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
            }
            // Ported from `Printer.emitYieldExpression` (`internal/printer/printer.go`).
            Expression::YieldExpression(node) => {
                self.write("yield");
                if node.asterisk_token.is_some() {
                    self.write("*");
                }
                if let Some(inner) = &node.expression {
                    self.write(" ");
                    self.emit_expression(inner);
                }
            }
            // Ported from `Printer.emitTypeOfExpression` (`internal/printer/printer.go`).
            Expression::TypeOfExpression(node) => {
                self.write("typeof ");
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
            }
            // Ported from `Printer.emitVoidExpression` (`internal/printer/printer.go`).
            Expression::VoidExpression(node) => {
                self.write("void ");
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
            }
            // Ported from `Printer.emitDeleteExpression` (`internal/printer/printer.go`).
            Expression::DeleteExpression(node) => {
                self.write("delete ");
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
            }
            // Ported from `Printer.emitNonNullExpression` (`internal/printer/printer.go`).
            Expression::NonNullExpression(node) => {
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
                self.write("!");
            }
            // Ported from `Printer.emitAsExpression` (`internal/printer/printer.go`).
            Expression::AsExpression(node) => {
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
                self.write(" as ");
                if let Some(r#type) = &node.r#type {
                    self.emit_type_node(r#type);
                }
            }
            // Ported from `Printer.emitSatisfiesExpression` (`internal/printer/printer.go`).
            Expression::SatisfiesExpression(node) => {
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
                self.write(" satisfies ");
                if let Some(r#type) = &node.r#type {
                    self.emit_type_node(r#type);
                }
            }
            // Ported from `Printer.emitTypeAssertionExpression` (`internal/printer/printer.go`).
            Expression::TypeAssertion(node) => {
                self.write("<");
                if let Some(r#type) = &node.r#type {
                    self.emit_type_node(r#type);
                }
                self.write(">");
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
            }
            // Ported from `Printer.emitExpressionWithTypeArguments` (`internal/printer/printer.go`).
            Expression::ExpressionWithTypeArguments(node) => {
                if let Some(inner) = &node.expression {
                    self.emit_expression(inner);
                }
                self.emit_type_arguments(node.type_arguments);
            }
            // Ported from `Printer.emitMetaProperty` (`internal/printer/printer.go`).
            Expression::MetaProperty(node) => {
                self.emit_token_node(node.keyword_token);
                self.write(".");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
            }
            // Ported from `Printer.emitOmittedExpression` (`internal/printer/printer.go`).
            Expression::OmittedExpression(_) => {}
            other if self.emit_jsx_expression(other) => {}
            other => {
                let kind = self.kind_of(other.node_id());
                self.unsupported(kind);
            }
        }
    }

    /// The chunk that closes one template span, up to the next `${` or the end.
    pub(crate) fn template_chunk(&mut self, literal: Option<&tsr_ast::TemplateMiddleOrTail<'_>>) {
        // `raw_text` is the source text of the chunk, so it needs no re-escaping —
        // unlike a string literal, whose `text` is decoded.
        // As with the head, `raw_text` already carries `}` and its terminator.
        let raw = match literal {
            Some(tsr_ast::TemplateMiddleOrTail::TemplateMiddle(middle)) => middle.raw_text,
            Some(tsr_ast::TemplateMiddleOrTail::TemplateTail(tail)) => tail.raw_text,
            None => return,
        };
        self.write(raw);
    }

    /// Ported from the `LFCallExpressionArguments` emit site in
    /// `internal/printer/printer.go`.
    fn arguments(&mut self, arguments: &[Expression<'_>]) {
        self.emit_list(arguments, ListFormat::CALL_EXPRESSION_ARGUMENTS, |printer, argument| {
            printer.emit_expression(argument);
        });
    }

    fn class_heritage(&mut self, clauses: &[&tsr_ast::HeritageClause<'_>]) {
        for clause in clauses {
            self.write(" ");
            self.emit_token_node(clause.token);
            self.write(" ");
            for (index, base) in clause.types.iter().enumerate() {
                if index > 0 {
                    self.write(", ");
                }
                if let Some(expression) = &base.expression {
                    self.emit_expression(expression);
                }
                self.emit_type_arguments(base.type_arguments);
            }
        }
    }
}
