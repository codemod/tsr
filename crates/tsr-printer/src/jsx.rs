//! JSX.
//!
//! # JSX text is significant, so nothing here may reformat
//!
//! Everywhere else the printer chooses its own whitespace, because whitespace is
//! not in the tree. Inside a JSX element it *is*: `JsxText` is a node carrying the
//! literal characters between tags, and inserting a newline or an indent adds
//! characters to it. So this module writes children back exactly as they were and
//! never calls [`Printer::newline`] or [`Printer::write`] — only
//! [`Printer::write_raw`], which does not insert separators either.
//!
//! The same reasoning rules out a space after `<`: `< div` is not a tag.

use tsr_ast::{Expression, JsxAttributeLike, JsxAttributeName, JsxAttributeValue, JsxChild};

use crate::{Printer, quote_string};

impl Printer<'_> {
    /// Whether this expression is JSX, and print it if so.
    pub(crate) fn jsx_expression(&mut self, expression: &Expression<'_>) -> bool {
        match expression {
            Expression::JsxElement(node) => {
                if let Some(opening) = node.opening_element {
                    self.write_raw("<");
                    self.jsx_tag_name(opening.tag_name.as_ref());
                    self.type_arguments(opening.type_arguments);
                    self.jsx_attributes(opening.attributes);
                    self.write_raw(">");
                }
                for child in node.children {
                    self.jsx_child(child);
                }
                if let Some(closing) = node.closing_element {
                    self.write_raw("</");
                    self.jsx_tag_name(closing.tag_name.as_ref());
                    self.write_raw(">");
                }
                true
            }
            Expression::JsxSelfClosingElement(node) => {
                self.write_raw("<");
                self.jsx_tag_name(node.tag_name.as_ref());
                self.type_arguments(node.type_arguments);
                self.jsx_attributes(node.attributes);
                self.write_raw("/>");
                true
            }
            Expression::JsxFragment(node) => {
                self.write_raw("<>");
                for child in node.children {
                    self.jsx_child(child);
                }
                self.write_raw("</>");
                true
            }
            Expression::JsxExpression(node) => {
                self.jsx_braced(node);
                true
            }
            Expression::JsxText(node) => {
                self.write_raw(node.text);
                true
            }
            _ => false,
        }
    }

    fn jsx_child(&mut self, child: &JsxChild<'_>) {
        match child {
            JsxChild::JsxText(text) => self.write_raw(text.text),
            JsxChild::JsxExpression(expression) => self.jsx_braced(expression),
            other => {
                // The element forms are all expressions; route them back through
                // the expression printer so nesting needs no second implementation.
                let node = tsr_ast::Node::from(*other);
                match Expression::try_from(node) {
                    Ok(expression) => {
                        self.jsx_expression(&expression);
                    }
                    Err(unhandled) => {
                        let kind = self.kind_of(unhandled.node_id());
                        self.unsupported(kind);
                    }
                }
            }
        }
    }

    fn jsx_braced(&mut self, expression: &tsr_ast::JsxExpression<'_>) {
        self.write_raw("{");
        if expression.dot_dot_dot_token.is_some() {
            self.write_raw("...");
        }
        if let Some(inner) = &expression.expression {
            self.expression(inner);
        }
        self.write_raw("}");
    }

    fn jsx_attributes(&mut self, attributes: Option<&tsr_ast::JsxAttributes<'_>>) {
        let Some(attributes) = attributes else { return };
        for property in attributes.properties {
            self.write_raw(" ");
            match property {
                JsxAttributeLike::JsxAttribute(attribute) => {
                    match &attribute.name {
                        Some(JsxAttributeName::Identifier(name)) => self.write_raw(name.text),
                        Some(JsxAttributeName::JsxNamespacedName(name)) => {
                            self.jsx_namespaced_name(name);
                        }
                        None => {}
                    }
                    match &attribute.initializer {
                        // A bare attribute (`<a disabled />`) has no initialiser,
                        // and inventing `={true}` would add nodes to the tree.
                        None => {}
                        Some(JsxAttributeValue::StringLiteral(literal)) => {
                            self.write_raw("=");
                            let quoted = quote_string(literal.text);
                            self.write_raw(&quoted);
                        }
                        Some(JsxAttributeValue::JsxExpression(expression)) => {
                            self.write_raw("=");
                            self.jsx_braced(expression);
                        }
                        Some(other) => {
                            self.write_raw("=");
                            let node = tsr_ast::Node::from(*other);
                            match Expression::try_from(node) {
                                Ok(expression) => {
                                    self.jsx_expression(&expression);
                                }
                                Err(unhandled) => {
                                    let kind = self.kind_of(unhandled.node_id());
                                    self.unsupported(kind);
                                }
                            }
                        }
                    }
                }
                JsxAttributeLike::JsxSpreadAttribute(spread) => {
                    self.write_raw("{...");
                    if let Some(inner) = &spread.expression {
                        self.expression(inner);
                    }
                    self.write_raw("}");
                }
            }
        }
    }

    fn jsx_tag_name(&mut self, name: Option<&tsr_ast::JsxTagNameExpression<'_>>) {
        match name {
            Some(tsr_ast::JsxTagNameExpression::Identifier(identifier)) => {
                self.write_raw(identifier.text);
            }
            Some(tsr_ast::JsxTagNameExpression::JsxNamespacedName(namespaced)) => {
                self.jsx_namespaced_name(namespaced);
            }
            Some(tsr_ast::JsxTagNameExpression::KeywordExpression(keyword)) => {
                // `<this.Foo />` — the tag is a keyword rather than an identifier.
                if let Some(text) = crate::token_text(keyword.kind) {
                    self.write_raw(text);
                } else {
                    self.unsupported(keyword.kind);
                }
            }
            Some(tsr_ast::JsxTagNameExpression::PropertyAccessExpression(access)) => {
                if let Some(target) = &access.expression {
                    // A dotted tag name is an ordinary property access, but it must
                    // not gain spaces: `<a . b />` is not a tag.
                    let before = self.len();
                    self.expression(target);
                    debug_assert!(self.len() > before, "a tag name always writes");
                }
                self.write_raw(".");
                if let Some(tsr_ast::MemberName::Identifier(member)) = &access.name {
                    self.write_raw(member.text);
                }
            }
            None => {}
        }
    }

    fn jsx_namespaced_name(&mut self, name: &tsr_ast::JsxNamespacedName<'_>) {
        if let Some(namespace) = name.namespace {
            self.write_raw(namespace.text);
        }
        self.write_raw(":");
        if let Some(inner) = name.name {
            self.write_raw(inner.text);
        }
    }
}
