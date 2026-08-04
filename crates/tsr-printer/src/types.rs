//! Type nodes and type-literal members.

use tsr_ast::{TypeElement, TypeNode};

use crate::Printer;

impl Printer<'_> {
    pub(crate) fn type_node(&mut self, node: &TypeNode<'_>) {
        match node {
            TypeNode::KeywordTypeNode(keyword) => match crate::token_text(keyword.kind) {
                Some(text) => self.write(text),
                None => self.unsupported(keyword.kind),
            },
            TypeNode::TypeReferenceNode(reference) => {
                match &reference.type_name {
                    Some(name) => self.entity_name(name),
                    // A `TypeReferenceNode` with no name is how the parser spells
                    // `as const`; `const` is a keyword and never an identifier.
                    None => self.write("const"),
                }
                self.type_arguments(reference.type_arguments);
            }
            TypeNode::ArrayTypeNode(array) => {
                if let Some(element) = &array.element_type {
                    self.type_node(element);
                }
                self.write_raw("[]");
            }
            TypeNode::TupleTypeNode(tuple) => {
                self.write("[");
                for (index, element) in tuple.elements.iter().enumerate() {
                    if index > 0 {
                        self.write_raw(", ");
                    }
                    self.type_node(element);
                }
                self.write_raw("]");
            }
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
                self.write_raw(": ");
                if let Some(r#type) = &member.r#type {
                    self.type_node(r#type);
                }
            }
            TypeNode::OptionalTypeNode(optional) => {
                if let Some(r#type) = &optional.r#type {
                    self.type_node(r#type);
                }
                self.write_raw("?");
            }
            TypeNode::RestTypeNode(rest) => {
                self.write("...");
                if let Some(r#type) = &rest.r#type {
                    self.type_node(r#type);
                }
            }
            TypeNode::UnionTypeNode(union) => {
                for (index, member) in union.types.iter().enumerate() {
                    if index > 0 {
                        self.write(" | ");
                    }
                    self.type_node(member);
                }
            }
            TypeNode::IntersectionTypeNode(intersection) => {
                for (index, member) in intersection.types.iter().enumerate() {
                    if index > 0 {
                        self.write(" & ");
                    }
                    self.type_node(member);
                }
            }
            TypeNode::ParenthesizedTypeNode(parenthesized) => {
                self.write("(");
                if let Some(r#type) = &parenthesized.r#type {
                    self.type_node(r#type);
                }
                self.write_raw(")");
            }
            TypeNode::LiteralTypeNode(literal) => {
                // A literal type's payload is a bare `Node`: it may be a literal, a
                // prefixed `-1`, or the `null` keyword.
                if let Some(inner) = literal.literal {
                    self.any_expression(inner);
                }
            }
            TypeNode::TypeOperatorNode(operator) => {
                self.token(operator.operator);
                self.write(" ");
                if let Some(r#type) = &operator.r#type {
                    self.type_node(r#type);
                }
            }
            TypeNode::IndexedAccessTypeNode(indexed) => {
                if let Some(object) = &indexed.object_type {
                    self.type_node(object);
                }
                self.write_raw("[");
                if let Some(index) = &indexed.index_type {
                    self.type_node(index);
                }
                self.write_raw("]");
            }
            TypeNode::TypeQueryNode(query) => {
                self.write("typeof ");
                if let Some(name) = &query.expr_name {
                    self.entity_name(name);
                }
                self.type_arguments(query.type_arguments);
            }
            TypeNode::ThisTypeNode(_) => self.write("this"),
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
                    self.type_node(r#type);
                }
            }
            TypeNode::InferTypeNode(infer) => {
                self.write("infer ");
                if let Some(parameter) = infer.type_parameter {
                    if let Some(name) = parameter.name {
                        self.write(name.text);
                    }
                    if let Some(constraint) = parameter.constraint {
                        self.write(" extends ");
                        self.type_node(&constraint);
                    }
                }
            }
            TypeNode::ConditionalTypeNode(conditional) => {
                if let Some(check) = &conditional.check_type {
                    self.type_node(check);
                }
                self.write(" extends ");
                if let Some(extends) = &conditional.extends_type {
                    self.type_node(extends);
                }
                self.write(" ? ");
                if let Some(true_type) = &conditional.true_type {
                    self.type_node(true_type);
                }
                self.write(" : ");
                if let Some(false_type) = &conditional.false_type {
                    self.type_node(false_type);
                }
            }
            TypeNode::FunctionTypeNode(function) => {
                self.type_parameters(function.type_parameters);
                self.parameters(function.parameters);
                self.write(" => ");
                if let Some(r#type) = &function.r#type {
                    self.type_node(r#type);
                }
            }
            TypeNode::ConstructorTypeNode(constructor) => {
                self.modifiers(constructor.modifiers);
                self.write("new ");
                self.type_parameters(constructor.type_parameters);
                self.parameters(constructor.parameters);
                self.write(" => ");
                if let Some(r#type) = &constructor.r#type {
                    self.type_node(r#type);
                }
            }
            TypeNode::TypeLiteralNode(literal) => self.type_members(literal.members),
            TypeNode::MappedTypeNode(mapped) => {
                self.write("{");
                if let Some(token) = mapped.readonly_token {
                    self.write(" ");
                    self.token(token);
                }
                self.write(" [");
                if let Some(parameter) = mapped.type_parameter {
                    if let Some(name) = parameter.name {
                        self.write(name.text);
                    }
                    if let Some(constraint) = parameter.constraint {
                        self.write(" in ");
                        self.type_node(&constraint);
                    }
                }
                if let Some(name) = &mapped.name_type {
                    self.write(" as ");
                    self.type_node(name);
                }
                self.write_raw("]");
                if let Some(token) = mapped.question_token {
                    self.token(token);
                }
                if let Some(r#type) = &mapped.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.write_raw("; }");
            }
            TypeNode::TemplateLiteralTypeNode(template) => {
                if let Some(head) = template.head {
                    self.write(head.raw_text);
                }
                for span in template.template_spans {
                    if let Some(r#type) = &span.r#type {
                        self.type_node(r#type);
                    }
                    self.template_chunk(span.literal.as_ref());
                }
            }
            TypeNode::ImportTypeNode(import) => {
                if import.is_type_of {
                    self.write("typeof ");
                }
                self.write("import(");
                if let Some(argument) = &import.argument {
                    self.type_node(argument);
                }
                self.write_raw(")");
                if let Some(qualifier) = &import.qualifier {
                    self.write_raw(".");
                    self.entity_name(qualifier);
                }
                self.type_arguments(import.type_arguments);
            }
            other => {
                let kind = self.kind_of(other.node_id());
                self.unsupported(kind);
            }
        }
    }

    /// The `{ … }` body of an interface or type literal.
    pub(crate) fn type_members(&mut self, members: &[TypeElement<'_>]) {
        self.write("{");
        self.indented(|printer| {
            for member in members {
                printer.newline();
                printer.type_element(member);
            }
        });
        self.newline();
        self.write_raw("}");
    }

    fn type_element(&mut self, member: &TypeElement<'_>) {
        match member {
            TypeElement::PropertySignatureDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.property_name(&node.name);
                if let Some(token) = node.postfix_token {
                    self.token(token);
                }
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.write_raw(";");
            }
            TypeElement::MethodSignatureDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.property_name(&node.name);
                if let Some(token) = node.postfix_token {
                    self.token(token);
                }
                self.type_parameters(node.type_parameters);
                self.parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.write_raw(";");
            }
            TypeElement::CallSignatureDeclaration(node) => {
                self.type_parameters(node.type_parameters);
                self.parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.write_raw(";");
            }
            TypeElement::ConstructSignatureDeclaration(node) => {
                self.write("new ");
                self.type_parameters(node.type_parameters);
                self.parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.write_raw(";");
            }
            TypeElement::IndexSignatureDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("[");
                for parameter in node.parameters {
                    if let Some(name) = &parameter.name {
                        self.binding_name(name);
                    }
                    if let Some(r#type) = &parameter.r#type {
                        self.write(": ");
                        self.type_node(r#type);
                    }
                }
                self.write_raw("]");
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.write_raw(";");
            }
            TypeElement::GetAccessorDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("get ");
                self.property_name(&node.name);
                self.parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.write_raw(";");
            }
            TypeElement::SetAccessorDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("set ");
                self.property_name(&node.name);
                self.parameters(node.parameters);
                self.write_raw(";");
            }
            // A transform artefact that never appears in a parsed tree.
            TypeElement::NotEmittedTypeElement(_) => {}
        }
    }
}
