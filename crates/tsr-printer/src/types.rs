//! Type nodes and type-literal members.

use tsr_ast::{SyntaxKind, TypeElement, TypeNode};

use crate::{ListFormat, Printer};

impl Printer<'_> {
    pub(crate) fn emit_type_node(&mut self, node: &TypeNode<'_>) {
        match node {
            // Ported from `Printer.emitKeywordTypeNode` (`internal/printer/printer.go`).
            TypeNode::KeywordTypeNode(keyword) => match crate::token_text(keyword.kind) {
                Some(text) => self.write(text),
                None => self.unsupported(keyword.kind),
            },
            // Ported from `Printer.emitTypeReference` (`internal/printer/printer.go`).
            TypeNode::TypeReferenceNode(reference) => {
                match &reference.type_name {
                    Some(name) => self.emit_entity_name(name),
                    // A `TypeReferenceNode` with no name is how the parser spells
                    // `as const`; `const` is a keyword and never an identifier.
                    None => self.write("const"),
                }
                self.emit_type_arguments(reference.type_arguments);
            }
            // Ported from `Printer.emitArrayType` (`internal/printer/printer.go`).
            TypeNode::ArrayTypeNode(array) => {
                if let Some(element) = &array.element_type {
                    self.emit_type_node(element);
                }
                self.write("[]");
            }
            // Ported from `Printer.emitTupleType` (`internal/printer/printer.go`).
            TypeNode::TupleTypeNode(tuple) => {
                self.write_punctuation("[");
                self.emit_list(
                    tuple.elements,
                    ListFormat::SINGLE_LINE_TUPLE_TYPE_ELEMENTS,
                    |printer, element| printer.emit_type_node(element),
                );
                self.write_punctuation("]");
            }
            // Ported from `Printer.emitNamedTupleMember` (`internal/printer/printer.go`).
            TypeNode::NamedTupleMember(member) => {
                if member.dot_dot_dot_token.is_some() {
                    self.write("...");
                }
                if let Some(name) = member.name {
                    self.write(name.text);
                }
                if member.question_token.is_some() {
                    self.write("?");
                }
                self.write(": ");
                if let Some(r#type) = &member.r#type {
                    self.emit_type_node(r#type);
                }
            }
            // Ported from `Printer.emitOptionalType` (`internal/printer/printer.go`).
            TypeNode::OptionalTypeNode(optional) => {
                if let Some(r#type) = &optional.r#type {
                    self.emit_type_node(r#type);
                }
                self.write("?");
            }
            // Ported from `Printer.emitRestType` (`internal/printer/printer.go`).
            TypeNode::RestTypeNode(rest) => {
                self.write("...");
                if let Some(r#type) = &rest.r#type {
                    self.emit_type_node(r#type);
                }
            }
            // Ported from `Printer.emitUnionType` (`internal/printer/printer.go`).
            TypeNode::UnionTypeNode(union) => {
                self.emit_list(
                    union.types,
                    ListFormat::UNION_TYPE_CONSTITUENTS,
                    |printer, member| {
                        printer.emit_type_node(member);
                    },
                );
            }
            // Ported from `Printer.emitIntersectionType` (`internal/printer/printer.go`).
            TypeNode::IntersectionTypeNode(intersection) => {
                self.emit_list(
                    intersection.types,
                    ListFormat::INTERSECTION_TYPE_CONSTITUENTS,
                    |printer, member| {
                        printer.emit_type_node(member);
                    },
                );
            }
            // Ported from `Printer.emitParenthesizedType` (`internal/printer/printer.go`).
            TypeNode::ParenthesizedTypeNode(parenthesized) => {
                self.write("(");
                if let Some(r#type) = &parenthesized.r#type {
                    self.emit_type_node(r#type);
                }
                self.write(")");
            }
            // Ported from `Printer.emitLiteralType` (`internal/printer/printer.go`).
            TypeNode::LiteralTypeNode(literal) => {
                // A literal type's payload is a bare `Node`: it may be a literal, a
                // prefixed `-1`, or the `null` keyword.
                if let Some(inner) = literal.literal {
                    self.any_expression(inner);
                }
            }
            // Ported from `Printer.emitTypeOperator` (`internal/printer/printer.go`).
            TypeNode::TypeOperatorNode(operator) => {
                self.emit_token_node(operator.operator);
                self.write(" ");
                if let Some(r#type) = &operator.r#type {
                    self.emit_type_node(r#type);
                }
            }
            // Ported from `Printer.emitIndexedAccessType` (`internal/printer/printer.go`).
            TypeNode::IndexedAccessTypeNode(indexed) => {
                if let Some(object) = &indexed.object_type {
                    self.emit_type_node(object);
                }
                self.write("[");
                if let Some(index) = &indexed.index_type {
                    self.emit_type_node(index);
                }
                self.write("]");
            }
            // Ported from `Printer.emitTypeQuery` (`internal/printer/printer.go`).
            TypeNode::TypeQueryNode(query) => {
                self.write("typeof ");
                if let Some(name) = &query.expr_name {
                    self.emit_entity_name(name);
                }
                self.emit_type_arguments(query.type_arguments);
            }
            // Ported from `Printer.emitThisType` (`internal/printer/printer.go`).
            TypeNode::ThisTypeNode(_) => self.write("this"),
            // Ported from `Printer.emitTypePredicate` (`internal/printer/printer.go`).
            TypeNode::TypePredicateNode(predicate) => {
                if predicate.asserts_modifier.is_some() {
                    self.write("asserts ");
                }
                match &predicate.parameter_name {
                    Some(tsr_ast::TypePredicateParameterName::Identifier(name)) => {
                        self.write(name.text);
                    }
                    Some(tsr_ast::TypePredicateParameterName::ThisTypeNode(_)) => {
                        self.write("this");
                    }
                    None => {}
                }
                if let Some(r#type) = &predicate.r#type {
                    self.write(" is ");
                    self.emit_type_node(r#type);
                }
            }
            // Ported from `Printer.emitInferType` (`internal/printer/printer.go`).
            TypeNode::InferTypeNode(infer) => {
                self.write("infer ");
                if let Some(parameter) = infer.type_parameter {
                    if let Some(name) = parameter.name {
                        self.write(name.text);
                    }
                    if let Some(constraint) = parameter.constraint {
                        self.write(" extends ");
                        self.emit_type_node(&constraint);
                    }
                }
            }
            // Ported from `Printer.emitConditionalType` (`internal/printer/printer.go`).
            TypeNode::ConditionalTypeNode(conditional) => {
                if let Some(check) = &conditional.check_type {
                    self.emit_type_node(check);
                }
                self.write(" extends ");
                if let Some(extends) = &conditional.extends_type {
                    self.emit_type_node(extends);
                }
                self.write(" ? ");
                if let Some(true_type) = &conditional.true_type {
                    self.emit_type_node(true_type);
                }
                self.write(" : ");
                if let Some(false_type) = &conditional.false_type {
                    self.emit_type_node(false_type);
                }
            }
            // Ported from `Printer.emitFunctionType` (`internal/printer/printer.go`).
            TypeNode::FunctionTypeNode(function) => {
                self.emit_type_parameters(function.type_parameters);
                self.emit_parameters(function.parameters);
                self.write(" => ");
                if let Some(r#type) = &function.r#type {
                    let synthesized = function.node_id.is_some_and(|id| {
                        self.nodes.flags(id).contains(tsr_ast::NodeFlags::SYNTHESIZED)
                    });
                    if synthesized {
                        self.single_line_type_depth += 1;
                    }
                    self.emit_type_node(r#type);
                    if synthesized {
                        self.single_line_type_depth -= 1;
                    }
                }
            }
            // Ported from `Printer.emitConstructorType` (`internal/printer/printer.go`).
            TypeNode::ConstructorTypeNode(constructor) => {
                self.emit_modifier_list(constructor.modifiers);
                self.write("new ");
                self.emit_type_parameters(constructor.type_parameters);
                self.emit_parameters(constructor.parameters);
                self.write(" => ");
                if let Some(r#type) = &constructor.r#type {
                    self.emit_type_node(r#type);
                }
            }
            // Ported from `Printer.emitTypeLiteral` (`internal/printer/printer.go`).
            // Ported from `Printer.emitTypeLiteral` (`printer.go:1964`), which is
            // *not* `emitInterfaceDeclaration`'s list. The difference is
            // `LFOptionalIfEmpty`: an empty type literal is `{}` on one line, while
            // an empty interface body is a brace, a newline and a brace. Sharing
            // one helper between them printed `type A = {` / `}` where every
            // baseline writes `type A = {};`.
            TypeNode::TypeLiteralNode(literal) => {
                self.write_punctuation("{");
                self.emit_list(
                    literal.members,
                    ListFormat::MULTI_LINE_TYPE_LITERAL_MEMBERS
                        .union(ListFormat::NO_SPACE_IF_EMPTY),
                    |printer, member| printer.emit_type_element(member),
                );
                self.write_punctuation("}");
            }
            // Ported from `Printer.emitMappedType` (`internal/printer/printer.go`).
            TypeNode::MappedTypeNode(mapped) => {
                self.write("{");
                // Upstream's default is multiline; only an explicit SingleLine
                // emit flag uses the compact form. This tree has no emit-context
                // override yet, so source and declaration mapped types take the
                // default path rather than silently behaving as SingleLine.
                let single_line = self.single_line_type_depth > 0;
                if single_line {
                    self.write(" ");
                } else {
                    self.write_line();
                    self.increase_indent();
                }
                if let Some(token) = mapped.readonly_token {
                    self.emit_token_node(token);
                    if token.kind != SyntaxKind::ReadonlyKeyword {
                        self.write("readonly");
                    }
                    self.write(" ");
                }
                self.write("[");
                if let Some(parameter) = mapped.type_parameter {
                    if let Some(name) = parameter.name {
                        self.write(name.text);
                    }
                    if let Some(constraint) = parameter.constraint {
                        self.write(" in ");
                        self.emit_type_node(&constraint);
                    }
                }
                if let Some(name) = &mapped.name_type {
                    self.write(" as ");
                    self.emit_type_node(name);
                }
                self.write("]");
                if let Some(token) = mapped.question_token {
                    self.emit_token_node(token);
                    if token.kind != SyntaxKind::QuestionToken {
                        self.write("?");
                    }
                }
                if let Some(r#type) = &mapped.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.write(";");
                if single_line {
                    self.write(" ");
                } else {
                    self.write_line();
                    self.decrease_indent();
                }
                self.write("}");
            }
            // Ported from `Printer.emitTemplateType` (`internal/printer/printer.go`).
            TypeNode::TemplateLiteralTypeNode(template) => {
                if let Some(head) = template.head {
                    self.write(head.raw_text);
                }
                for span in template.template_spans {
                    if let Some(r#type) = &span.r#type {
                        self.emit_type_node(r#type);
                    }
                    self.template_chunk(span.literal.as_ref());
                }
            }
            // Ported from `Printer.emitImportTypeNode` (`internal/printer/printer.go`).
            TypeNode::ImportTypeNode(import) => {
                if import.is_type_of {
                    self.write("typeof ");
                }
                self.write("import(");
                if let Some(argument) = &import.argument {
                    self.emit_type_node(argument);
                }
                if let Some(attributes) = import.attributes {
                    self.write(", { ");
                    self.emit_token_node(attributes.token);
                    self.write(": ");
                    self.import_attributes_body(attributes);
                    self.write(" }");
                }
                self.write(")");
                if let Some(qualifier) = &import.qualifier {
                    self.write(".");
                    self.emit_entity_name(qualifier);
                }
                self.emit_type_arguments(import.type_arguments);
            }
            other => {
                let kind = self.kind_of(other.node_id());
                self.unsupported(kind);
            }
        }
    }

    /// The `{ … }` body of an interface or type literal.
    /// Ported from the `LFInterfaceMembers` / `LFMultiLineTypeLiteralMembers`
    /// emit sites in `internal/printer/printer.go`.
    pub(crate) fn type_members(&mut self, members: &[TypeElement<'_>]) {
        self.write_punctuation("{");
        self.emit_list(members, ListFormat::INTERFACE_MEMBERS, |printer, member| {
            printer.emit_type_element(member);
        });
        self.write_punctuation("}");
    }

    fn emit_type_element(&mut self, member: &TypeElement<'_>) {
        self.emit_leading_jsdoc(member.node_id());
        match member {
            // Ported from `Printer.emitPropertySignature` (`internal/printer/printer.go`).
            TypeElement::PropertySignatureDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.emit_property_name(&node.name);
                if let Some(token) = node.postfix_token {
                    self.emit_token_node(token);
                }
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.write(";");
            }
            // Ported from `Printer.emitMethodSignature` (`internal/printer/printer.go`).
            TypeElement::MethodSignatureDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.emit_property_name(&node.name);
                if let Some(token) = node.postfix_token {
                    self.emit_token_node(token);
                }
                self.emit_type_parameters(node.type_parameters);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.write(";");
            }
            // Ported from `Printer.emitCallSignature` (`internal/printer/printer.go`).
            TypeElement::CallSignatureDeclaration(node) => {
                self.emit_type_parameters(node.type_parameters);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.write(";");
            }
            // Ported from `Printer.emitConstructSignature` (`internal/printer/printer.go`).
            TypeElement::ConstructSignatureDeclaration(node) => {
                self.write("new ");
                self.emit_type_parameters(node.type_parameters);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.write(";");
            }
            // Ported from `Printer.emitIndexSignature` (`internal/printer/printer.go`).
            TypeElement::IndexSignatureDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("[");
                for parameter in node.parameters {
                    if let Some(name) = &parameter.name {
                        self.emit_binding_name(name);
                    }
                    if let Some(r#type) = &parameter.r#type {
                        self.write(": ");
                        self.emit_type_node(r#type);
                    }
                }
                self.write("]");
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.write(";");
            }
            TypeElement::GetAccessorDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("get ");
                self.emit_property_name(&node.name);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.write(";");
            }
            TypeElement::SetAccessorDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("set ");
                self.emit_property_name(&node.name);
                self.emit_parameters(node.parameters);
                self.write(";");
            }
            // A transform artefact that never appears in a parsed tree.
            TypeElement::NotEmittedTypeElement(_) => {}
        }
    }
}
