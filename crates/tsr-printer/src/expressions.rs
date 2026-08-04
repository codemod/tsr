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

use crate::{Printer, big_int_text, escape_template, quote_string};

impl Printer<'_> {
    pub(crate) fn expression(&mut self, expression: &Expression<'_>) {
        match expression {
            Expression::Identifier(node) => self.write(node.text),
            Expression::PrivateIdentifier(node) => self.write(node.text),
            Expression::NumericLiteral(node) => self.write(node.text),
            Expression::BigIntLiteral(node) => {
                let text = big_int_text(node.text);
                self.write(&text);
            }
            Expression::StringLiteral(node) => {
                let text = quote_string(node.text);
                self.write(&text);
            }
            Expression::RegularExpressionLiteral(node) => self.write(node.text),
            Expression::NoSubstitutionTemplateLiteral(node) => {
                let text = format!("`{}`", escape_template(node.text));
                self.write(&text);
            }
            Expression::KeywordExpression(node) => match crate::token_text(node.kind) {
                Some(text) => self.write(text),
                None => self.unsupported(node.kind),
            },
            Expression::TemplateExpression(node) => {
                if let Some(head) = node.head {
                    // `raw_text` is the whole token, delimiters included — it is
                    // already `` `abc${ ``, so wrapping it again produces nonsense.
                    self.write(head.raw_text);
                }
                for span in node.template_spans {
                    if let Some(expression) = &span.expression {
                        self.expression(expression);
                    }
                    self.template_chunk(span.literal.as_ref());
                }
            }
            Expression::TaggedTemplateExpression(node) => {
                if let Some(tag) = &node.tag {
                    self.expression(tag);
                }
                self.type_arguments(node.type_arguments);
                if let Some(template) = &node.template {
                    self.any_expression(tsr_ast::Node::from(*template));
                }
            }
            Expression::ParenthesizedExpression(node) => {
                self.write("(");
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
                self.write(")");
            }
            Expression::ArrayLiteralExpression(node) => {
                self.write("[");
                for (index, element) in node.elements.iter().enumerate() {
                    if index > 0 {
                        self.write(", ");
                    }
                    self.expression(element);
                }
                self.write("]");
            }
            Expression::ObjectLiteralExpression(node) => self.object_members(node.properties),
            Expression::PropertyAccessExpression(node) => {
                if let Some(target) = &node.expression {
                    self.expression(target);
                }
                if let Some(token) = node.question_dot_token {
                    self.token(token);
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
            Expression::ElementAccessExpression(node) => {
                if let Some(target) = &node.expression {
                    self.expression(target);
                }
                if let Some(token) = node.question_dot_token {
                    self.token(token);
                }
                self.write("[");
                if let Some(argument) = &node.argument_expression {
                    self.expression(argument);
                }
                self.write("]");
            }
            Expression::CallExpression(node) => {
                if let Some(target) = &node.expression {
                    self.expression(target);
                }
                if let Some(token) = node.question_dot_token {
                    self.token(token);
                }
                self.type_arguments(node.type_arguments);
                self.arguments(node.arguments);
            }
            Expression::NewExpression(node) => {
                self.write("new ");
                if let Some(target) = &node.expression {
                    self.expression(target);
                }
                self.type_arguments(node.type_arguments);
                // `new C` and `new C()` are different trees: the argument list is
                // optional and its absence is recorded, so it must not be invented.
                // `new C` and `new C()` parse to the same tree — both carry an
                // empty argument list — so always emitting `()` loses nothing.
                self.arguments(node.arguments);
            }
            Expression::BinaryExpression(node) => {
                if let Some(left) = &node.left {
                    self.expression(left);
                }
                self.write(" ");
                if let Some(token) = node.operator_token {
                    self.token(token);
                }
                self.write(" ");
                if let Some(right) = &node.right {
                    self.expression(right);
                }
            }
            Expression::PrefixUnaryExpression(node) => {
                self.token(node.operator);
                if let Some(operand) = &node.operand {
                    self.expression(operand);
                }
            }
            Expression::PostfixUnaryExpression(node) => {
                if let Some(operand) = &node.operand {
                    self.expression(operand);
                }
                self.token(node.operator);
            }
            Expression::ConditionalExpression(node) => {
                if let Some(condition) = &node.condition {
                    self.expression(condition);
                }
                self.write(" ? ");
                if let Some(when_true) = &node.when_true {
                    self.expression(when_true);
                }
                self.write(" : ");
                if let Some(when_false) = &node.when_false {
                    self.expression(when_false);
                }
            }
            Expression::ArrowFunction(node) => {
                self.modifiers(node.modifiers);
                self.type_parameters(node.type_parameters);
                self.parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.write(" => ");
                match &node.body {
                    Some(tsr_ast::ConciseBody::Block(block)) => self.block(block),
                    Some(other) => self.any_expression(tsr_ast::Node::from(*other)),
                    None => {}
                }
            }
            Expression::FunctionExpression(node) => {
                self.modifiers(node.modifiers);
                self.write("function");
                if node.asterisk_token.is_some() {
                    self.write("*");
                }
                if let Some(name) = node.name {
                    self.write(" ");
                    self.write(name.text);
                }
                self.type_parameters(node.type_parameters);
                self.parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                if let Some(tsr_ast::FunctionBody::Block(block)) = &node.body {
                    self.write(" ");
                    self.block(block);
                }
            }
            Expression::ClassExpression(node) => {
                self.modifiers(node.modifiers);
                self.write("class");
                if let Some(name) = node.name {
                    self.write(" ");
                    self.write(name.text);
                }
                self.type_parameters(node.type_parameters);
                self.class_heritage(node.heritage_clauses);
                self.write(" ");
                self.class_body(node.members);
            }
            Expression::SpreadElement(node) => {
                self.write("...");
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
            }
            Expression::AwaitExpression(node) => {
                self.write("await ");
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
            }
            Expression::YieldExpression(node) => {
                self.write("yield");
                if node.asterisk_token.is_some() {
                    self.write("*");
                }
                if let Some(inner) = &node.expression {
                    self.write(" ");
                    self.expression(inner);
                }
            }
            Expression::TypeOfExpression(node) => {
                self.write("typeof ");
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
            }
            Expression::VoidExpression(node) => {
                self.write("void ");
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
            }
            Expression::DeleteExpression(node) => {
                self.write("delete ");
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
            }
            Expression::NonNullExpression(node) => {
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
                self.write("!");
            }
            Expression::AsExpression(node) => {
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
                self.write(" as ");
                if let Some(r#type) = &node.r#type {
                    self.type_node(r#type);
                }
            }
            Expression::SatisfiesExpression(node) => {
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
                self.write(" satisfies ");
                if let Some(r#type) = &node.r#type {
                    self.type_node(r#type);
                }
            }
            Expression::TypeAssertion(node) => {
                self.write("<");
                if let Some(r#type) = &node.r#type {
                    self.type_node(r#type);
                }
                self.write(">");
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
            }
            Expression::ExpressionWithTypeArguments(node) => {
                if let Some(inner) = &node.expression {
                    self.expression(inner);
                }
                self.type_arguments(node.type_arguments);
            }
            Expression::MetaProperty(node) => {
                self.token(node.keyword_token);
                self.write(".");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
            }
            Expression::OmittedExpression(_) => {}
            other if self.jsx_expression(other) => {}
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

    fn arguments(&mut self, arguments: &[Expression<'_>]) {
        self.write("(");
        for (index, argument) in arguments.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.expression(argument);
        }
        self.write(")");
    }

    fn class_heritage(&mut self, clauses: &[&tsr_ast::HeritageClause<'_>]) {
        for clause in clauses {
            self.write(" ");
            self.token(clause.token);
            self.write(" ");
            for (index, base) in clause.types.iter().enumerate() {
                if index > 0 {
                    self.write(", ");
                }
                if let Some(expression) = &base.expression {
                    self.expression(expression);
                }
                self.type_arguments(base.type_arguments);
            }
        }
    }
}
