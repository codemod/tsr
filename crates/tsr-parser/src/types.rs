//! Type annotation parsing.
//!
//! The type grammar is a separate language sharing TypeScript's token stream.
//! Precedence, loosest first: conditional, union, intersection, postfix
//! (`T[]`, `T[K]`), then primary.

use tsr_ast::*;
use tsr_diagnostics::messages;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse `: T`, if present.
    pub(crate) fn parse_type_annotation(&mut self) -> Option<TypeNode<'a>> {
        if self.eat(SyntaxKind::ColonToken) { Some(self.parse_type()) } else { None }
    }

    /// Parse a type.
    pub(crate) fn parse_type(&mut self) -> TypeNode<'a> {
        let Some(type_node) = self.descend(Parser::parse_conditional_type) else {
            // Depth limit reached; a missing node beats a stack overflow.
            self.error_at_current(&messages::TYPE_EXPECTED);
            return self.missing_type();
        };
        type_node
    }

    /// `T extends U ? X : Y`, the loosest-binding type form.
    fn parse_conditional_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        let check = self.parse_union_type();

        // `extends` here is only a conditional type at the top level of a type;
        // inside a type parameter list it constrains, and that caller does not
        // route through here.
        if !self.at(SyntaxKind::ExtendsKeyword) {
            return check;
        }
        self.next_token();
        let extends = self.parse_union_type();
        self.expect(SyntaxKind::QuestionToken);
        let true_type = self.parse_type();
        self.expect(SyntaxKind::ColonToken);
        let false_type = self.parse_type();

        let node = self.finish_node(
            ConditionalTypeNode::new(Some(check), Some(extends), Some(true_type), Some(false_type)),
            SyntaxKind::ConditionalType,
            start,
        );
        TypeNode::ConditionalTypeNode(node)
    }

    /// A synthesised type standing in for one the source omitted.
    fn missing_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        let node = self.finish_node(
            KeywordTypeNode::new(SyntaxKind::AnyKeyword),
            SyntaxKind::AnyKeyword,
            start,
        );
        TypeNode::KeywordTypeNode(node)
    }

    fn parse_union_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        // A leading `|` is allowed: `type T = | A | B`.
        self.eat(SyntaxKind::BarToken);
        let first = self.parse_intersection_type();
        if !self.at(SyntaxKind::BarToken) {
            return first;
        }
        let mut types = vec![first];
        while self.eat(SyntaxKind::BarToken) {
            types.push(self.parse_intersection_type());
        }
        let types = self.arena.alloc_slice(&types);
        let node = self.finish_node(UnionTypeNode::new(types), SyntaxKind::UnionType, start);
        TypeNode::UnionTypeNode(node)
    }

    fn parse_intersection_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        self.eat(SyntaxKind::AmpersandToken);
        let first = self.parse_postfix_type();
        if !self.at(SyntaxKind::AmpersandToken) {
            return first;
        }
        let mut types = vec![first];
        while self.eat(SyntaxKind::AmpersandToken) {
            types.push(self.parse_postfix_type());
        }
        let types = self.arena.alloc_slice(&types);
        let node =
            self.finish_node(IntersectionTypeNode::new(types), SyntaxKind::IntersectionType, start);
        TypeNode::IntersectionTypeNode(node)
    }

    /// `T[]` and `T[K]`, which share a prefix.
    fn parse_postfix_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        let mut type_node = self.parse_primary_type();

        // A line break ends the type: `let x: T\n[1]` is not an array type.
        while self.at(SyntaxKind::OpenBracketToken) && !self.token.has_preceding_line_break() {
            self.next_token();
            if self.eat(SyntaxKind::CloseBracketToken) {
                let node = self.finish_node(
                    ArrayTypeNode::new(Some(type_node)),
                    SyntaxKind::ArrayType,
                    start,
                );
                type_node = TypeNode::ArrayTypeNode(node);
            } else {
                let index = self.parse_type();
                self.expect(SyntaxKind::CloseBracketToken);
                let node = self.finish_node(
                    IndexedAccessTypeNode::new(Some(type_node), Some(index)),
                    SyntaxKind::IndexedAccessType,
                    start,
                );
                type_node = TypeNode::IndexedAccessTypeNode(node);
            }
        }
        type_node
    }

    #[allow(clippy::too_many_lines)]
    fn parse_primary_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        match self.token.kind {
            // Keyword types: `string`, `number`, `any`, `void`, …
            kind if kind.is_keyword_type() => {
                self.next_token();
                let node = self.finish_node(KeywordTypeNode::new(kind), kind, start);
                TypeNode::KeywordTypeNode(node)
            }
            // `(` opens either a parenthesised type or a function type's parameter
            // list, and the two diverge only at the `=>`. Speculate, then fall back.
            SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken => {
                if let Some(function_type) = self.try_parse_function_type() {
                    return function_type;
                }
                if self.at(SyntaxKind::LessThanToken) {
                    self.error_at_current(&messages::TYPE_EXPECTED);
                    return self.missing_type();
                }
                self.next_token();
                let inner = self.parse_type();
                self.expect(SyntaxKind::CloseParenToken);
                let node = self.finish_node(
                    ParenthesizedTypeNode::new(Some(inner)),
                    SyntaxKind::ParenthesizedType,
                    start,
                );
                TypeNode::ParenthesizedTypeNode(node)
            }
            SyntaxKind::NewKeyword => {
                self.next_token();
                let type_parameters = self.parse_type_parameters();
                let parameters = self.parse_parameter_list();
                self.expect(SyntaxKind::EqualsGreaterThanToken);
                let return_type = self.parse_type();
                let type_parameters = self.arena.alloc_slice(&type_parameters);
                let parameters = self.arena.alloc_slice(&parameters);
                let node = self.finish_node(
                    ConstructorTypeNode::new(
                        &[],
                        type_parameters,
                        parameters,
                        Some(return_type),
                        None,
                    ),
                    SyntaxKind::ConstructorType,
                    start,
                );
                TypeNode::ConstructorTypeNode(node)
            }
            SyntaxKind::OpenBracketToken => self.parse_tuple_type(),
            SyntaxKind::OpenBraceToken => self.parse_type_literal(),
            SyntaxKind::TypeOfKeyword => {
                self.next_token();
                let name = self.parse_entity_name();
                let node = self.finish_node(
                    TypeQueryNode::new(Some(name), &[]),
                    SyntaxKind::TypeQuery,
                    start,
                );
                TypeNode::TypeQueryNode(node)
            }
            SyntaxKind::InferKeyword => {
                self.next_token();
                let parameter_start = self.pos();
                let name = self.parse_identifier();
                let parameter = self.finish_node(
                    TypeParameterDeclaration::new(&[], Some(name), None, None, None),
                    SyntaxKind::TypeParameter,
                    parameter_start,
                );
                let node = self.finish_node(
                    InferTypeNode::new(Some(parameter)),
                    SyntaxKind::InferType,
                    start,
                );
                TypeNode::InferTypeNode(node)
            }
            SyntaxKind::KeyOfKeyword | SyntaxKind::ReadonlyKeyword | SyntaxKind::UniqueKeyword => {
                let operator = self.take_token();
                let operand = self.parse_postfix_type();
                let node = self.finish_node(
                    TypeOperatorNode::new(operator, Some(operand)),
                    SyntaxKind::TypeOperator,
                    start,
                );
                TypeNode::TypeOperatorNode(node)
            }
            SyntaxKind::StringLiteral
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::NullKeyword => self.parse_literal_type(),
            SyntaxKind::MinusToken => {
                // Negative numeric literal types: `-1`.
                self.next_token();
                self.parse_literal_type_from(start)
            }
            SyntaxKind::Identifier => {
                let name = self.parse_entity_name();
                let type_arguments = self.parse_type_arguments();
                let type_arguments = self.arena.alloc_slice(&type_arguments);
                let node = self.finish_node(
                    TypeReferenceNode::new(Some(name), type_arguments),
                    SyntaxKind::TypeReference,
                    start,
                );
                TypeNode::TypeReferenceNode(node)
            }
            _ => {
                self.error_at_current(&messages::TYPE_EXPECTED);
                self.missing_type()
            }
        }
    }

    /// `(a: T) => R` and `<T>(a: T) => R`, when the lookahead confirms one.
    fn try_parse_function_type(&mut self) -> Option<TypeNode<'a>> {
        let start = self.pos();
        let parsed = self.try_parse(|p| {
            let type_parameters = p.parse_type_parameters();
            if !p.at(SyntaxKind::OpenParenToken) {
                return None;
            }
            let parameters = p.parse_parameter_list();
            if !p.at(SyntaxKind::EqualsGreaterThanToken) {
                return None;
            }
            Some((type_parameters, parameters))
        })?;

        let (type_parameters, parameters) = parsed;
        self.expect(SyntaxKind::EqualsGreaterThanToken);
        let return_type = self.parse_type();
        let type_parameters = self.arena.alloc_slice(&type_parameters);
        let parameters = self.arena.alloc_slice(&parameters);
        let node = self.finish_node(
            FunctionTypeNode::new(type_parameters, parameters, Some(return_type), &[], None),
            SyntaxKind::FunctionType,
            start,
        );
        Some(TypeNode::FunctionTypeNode(node))
    }

    fn parse_literal_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        self.parse_literal_type_from(start)
    }

    fn parse_literal_type_from(&mut self, start: u32) -> TypeNode<'a> {
        let literal_start = self.pos();
        let kind = self.token.kind;
        let text = self.token_value();
        let flags = self.token.ast_flags();
        self.next_token();

        // `LiteralTypeNode` holds a `Node`, not an `Expression`, because the
        // literal may be a keyword (`null`, `true`) rather than a value expression.
        let literal: Node<'a> = match kind {
            SyntaxKind::StringLiteral => Node::StringLiteral(self.finish_node(
                StringLiteral::new(text, flags),
                SyntaxKind::StringLiteral,
                literal_start,
            )),
            SyntaxKind::NumericLiteral => Node::NumericLiteral(self.finish_node(
                NumericLiteral::new(text, flags),
                SyntaxKind::NumericLiteral,
                literal_start,
            )),
            SyntaxKind::BigIntLiteral => Node::BigIntLiteral(self.finish_node(
                BigIntLiteral::new(text, flags),
                SyntaxKind::BigIntLiteral,
                literal_start,
            )),
            other => Node::KeywordExpression(self.finish_node(
                KeywordExpression::new(other),
                other,
                literal_start,
            )),
        };

        let node =
            self.finish_node(LiteralTypeNode::new(Some(literal)), SyntaxKind::LiteralType, start);
        TypeNode::LiteralTypeNode(node)
    }

    fn parse_tuple_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBracketToken);
        let mut elements = Vec::new();
        while !self.at(SyntaxKind::CloseBracketToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            elements.push(self.parse_type());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
            if self.pos() == before {
                break;
            }
        }
        self.expect(SyntaxKind::CloseBracketToken);
        let elements = self.arena.alloc_slice(&elements);
        let node = self.finish_node(TupleTypeNode::new(elements), SyntaxKind::TupleType, start);
        TypeNode::TupleTypeNode(node)
    }

    /// `{ a: string; b(): void }`.
    fn parse_type_literal(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);
        let mut members = Vec::new();
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            if let Some(member) = self.parse_type_member() {
                members.push(member);
            } else {
                self.error_at_current(&messages::PROPERTY_OR_SIGNATURE_EXPECTED);
                self.next_token();
            }
            // Members are separated by `;` or `,`, either optional before `}`.
            // Members are separated by `;` or `,`, both optional before `}`.
            if !self.eat(SyntaxKind::SemicolonToken)
                && !self.eat(SyntaxKind::CommaToken)
                && self.pos() == before
            {
                break;
            }
        }
        self.expect(SyntaxKind::CloseBraceToken);
        let members = self.arena.alloc_slice(&members);
        let node = self.finish_node(TypeLiteralNode::new(members), SyntaxKind::TypeLiteral, start);
        TypeNode::TypeLiteralNode(node)
    }

    /// One member of a type literal or interface body.
    pub(crate) fn parse_type_member(&mut self) -> Option<TypeElement<'a>> {
        let start = self.pos();
        if !matches!(
            self.token.kind,
            SyntaxKind::Identifier
                | SyntaxKind::StringLiteral
                | SyntaxKind::NumericLiteral
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::ReadonlyKeyword
        ) && !self.token.kind.is_keyword()
        {
            return None;
        }

        let modifiers = self.parse_modifiers();
        let name = self.parse_property_name();
        let question =
            if self.at(SyntaxKind::QuestionToken) { Some(self.take_token()) } else { None };

        let modifiers = self.arena.alloc_slice(&modifiers);

        // A `(` here makes it a method signature rather than a property.
        if self.at(SyntaxKind::OpenParenToken) {
            let parameters = self.parse_parameter_list();
            let return_type = self.parse_type_annotation();
            let parameters = self.arena.alloc_slice(&parameters);
            let node = self.finish_node(
                MethodSignatureDeclaration::new(
                    modifiers,
                    name,
                    question,
                    &[],
                    parameters,
                    return_type,
                    None,
                ),
                SyntaxKind::MethodSignature,
                start,
            );
            return Some(TypeElement::MethodSignatureDeclaration(node));
        }

        let type_node = self.parse_type_annotation();
        let node = self.finish_node(
            PropertySignatureDeclaration::new(modifiers, name, question, type_node, None),
            SyntaxKind::PropertySignature,
            start,
        );
        Some(TypeElement::PropertySignatureDeclaration(node))
    }

    /// A dotted name: `A`, `A.B`, `A.B.C`.
    pub(crate) fn parse_entity_name(&mut self) -> EntityName<'a> {
        let start = self.pos();
        let mut name = EntityName::Identifier(self.parse_identifier());
        while self.at(SyntaxKind::DotToken) {
            self.next_token();
            let right = self.parse_identifier();
            let node = self.finish_node(
                QualifiedName::new(Some(name), Some(right)),
                SyntaxKind::QualifiedName,
                start,
            );
            name = EntityName::QualifiedName(node);
        }
        name
    }

    /// `<A, B>` after a type reference, if present.
    fn parse_type_arguments(&mut self) -> Vec<TypeNode<'a>> {
        if !self.at(SyntaxKind::LessThanToken) {
            return Vec::new();
        }
        self.next_token();
        let mut arguments = Vec::new();
        loop {
            arguments.push(self.parse_type());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
        }
        // `List<List<T>>` lexes the close as `>>`; split it.
        if !self.at(SyntaxKind::GreaterThanToken) {
            self.rescan_greater_than();
        }
        self.expect(SyntaxKind::GreaterThanToken);
        arguments
    }

    /// `<T, U extends V>` on a declaration, if present.
    pub(crate) fn parse_type_parameters(&mut self) -> Vec<&'a TypeParameterDeclaration<'a>> {
        if !self.at(SyntaxKind::LessThanToken) {
            return Vec::new();
        }
        self.next_token();
        let mut parameters = Vec::new();
        while !self.at(SyntaxKind::GreaterThanToken) && !self.at(SyntaxKind::EndOfFile) {
            let start = self.pos();
            let name = self.parse_identifier();
            let constraint =
                if self.eat(SyntaxKind::ExtendsKeyword) { Some(self.parse_type()) } else { None };
            let default =
                if self.eat(SyntaxKind::EqualsToken) { Some(self.parse_type()) } else { None };
            parameters.push(self.finish_node(
                TypeParameterDeclaration::new(&[], Some(name), constraint, None, default),
                SyntaxKind::TypeParameter,
                start,
            ));
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
        }
        if !self.at(SyntaxKind::GreaterThanToken) {
            self.rescan_greater_than();
        }
        self.expect(SyntaxKind::GreaterThanToken);
        parameters
    }
}
