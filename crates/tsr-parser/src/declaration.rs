//! Class, interface, type alias, and enum declarations.

use tsr_ast::*;
use tsr_diagnostics::messages;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// `class C<T> extends B implements I { … }`.
    pub(crate) fn parse_class_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        self.expect(SyntaxKind::ClassKeyword);
        // `export default class {}` has no name.
        let name =
            if self.at(SyntaxKind::Identifier) { Some(self.parse_identifier()) } else { None };
        let type_parameters = self.parse_type_parameters();
        let heritage = self.parse_heritage_clauses();

        self.expect(SyntaxKind::OpenBraceToken);
        let mut members = Vec::new();
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            if let Some(member) = self.parse_class_member() {
                members.push(member);
            } else {
                self.error_at_current(&messages::PROPERTY_OR_SIGNATURE_EXPECTED);
                self.next_token();
                continue;
            }
            if self.pos() == before {
                self.next_token();
            }
        }
        self.expect(SyntaxKind::CloseBraceToken);

        let modifiers = self.arena.alloc_slice(modifiers);
        let type_parameters = self.arena.alloc_slice(&type_parameters);
        let heritage = self.arena.alloc_slice(&heritage);
        let members = self.arena.alloc_slice(&members);
        let node = self.finish_node(
            ClassDeclaration::new(modifiers, name, type_parameters, heritage, members),
            SyntaxKind::ClassDeclaration,
            start,
        );
        Statement::ClassDeclaration(node)
    }

    /// `extends B` and `implements I, J`.
    fn parse_heritage_clauses(&mut self) -> Vec<&'a HeritageClause<'a>> {
        let mut clauses = Vec::new();
        while matches!(self.token.kind, SyntaxKind::ExtendsKeyword | SyntaxKind::ImplementsKeyword)
        {
            let start = self.pos();
            let token = self.take_token();
            let mut types = Vec::new();
            loop {
                let type_start = self.pos();
                let expression = self.parse_left_hand_side_for_heritage();
                let type_arguments = self.parse_type_arguments_opt();
                let type_arguments = self.arena.alloc_slice(&type_arguments);
                types.push(self.finish_node(
                    ExpressionWithTypeArguments::new(Some(expression), type_arguments),
                    SyntaxKind::ExpressionWithTypeArguments,
                    type_start,
                ));
                if !self.eat(SyntaxKind::CommaToken) {
                    break;
                }
            }
            let types = self.arena.alloc_slice(&types);
            clauses.push(self.finish_node(
                HeritageClause::new(token, types),
                SyntaxKind::HeritageClause,
                start,
            ));
        }
        clauses
    }

    /// A dotted name in a heritage position: `extends a.b.C`.
    ///
    /// Deliberately not the full expression grammar — `extends` takes a
    /// left-hand-side expression, and parsing more would swallow the `{`.
    fn parse_left_hand_side_for_heritage(&mut self) -> Expression<'a> {
        let start = self.pos();
        let mut expression = Expression::Identifier(self.parse_identifier());
        while self.at(SyntaxKind::DotToken) {
            self.next_token();
            let name = self.parse_identifier();
            let node = self.finish_node(
                PropertyAccessExpression::new(
                    Some(expression),
                    None,
                    Some(MemberName::Identifier(name)),
                ),
                SyntaxKind::PropertyAccessExpression,
                start,
            );
            expression = Expression::PropertyAccessExpression(node);
        }
        expression
    }

    /// One member of a class body.
    fn parse_class_member(&mut self) -> Option<ClassElement<'a>> {
        let start = self.pos();

        if self.at(SyntaxKind::SemicolonToken) {
            self.next_token();
            let node = self.finish_node(
                SemicolonClassElement::new(),
                SyntaxKind::SemicolonClassElement,
                start,
            );
            return Some(ClassElement::SemicolonClassElement(node));
        }

        let modifiers = self.parse_modifiers();
        let asterisk =
            if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };

        // `constructor(...)` is a constructor; `constructor` alone is a property
        // named "constructor".
        if self.at(SyntaxKind::ConstructorKeyword) && self.next_is_open_paren() {
            self.next_token();
            let parameters = self.parse_parameter_list();
            let body = self.parse_method_body();
            let modifiers = self.arena.alloc_slice(&modifiers);
            let parameters = self.arena.alloc_slice(&parameters);
            let node = self.finish_node(
                ConstructorDeclaration::new(modifiers, &[], parameters, None, None, body, None),
                SyntaxKind::Constructor,
                start,
            );
            return Some(ClassElement::ConstructorDeclaration(node));
        }

        // `get x() {}` is an accessor; `get` alone is a property named "get", so
        // the following token decides.
        if matches!(self.token.kind, SyntaxKind::GetKeyword | SyntaxKind::SetKeyword)
            && self.next_starts_accessor_name()
        {
            let is_getter = self.at(SyntaxKind::GetKeyword);
            self.next_token();
            let name = self.parse_property_name();
            let parameters = self.parse_parameter_list();
            let return_type = self.parse_type_annotation();
            let body = self.parse_method_body();
            let modifiers = self.arena.alloc_slice(&modifiers);
            let parameters = self.arena.alloc_slice(&parameters);
            return Some(if is_getter {
                ClassElement::GetAccessorDeclaration(self.finish_node(
                    GetAccessorDeclaration::new(
                        modifiers,
                        name,
                        &[],
                        parameters,
                        return_type,
                        None,
                        body,
                        None,
                        None,
                    ),
                    SyntaxKind::GetAccessor,
                    start,
                ))
            } else {
                ClassElement::SetAccessorDeclaration(self.finish_node(
                    SetAccessorDeclaration::new(
                        modifiers,
                        name,
                        &[],
                        parameters,
                        return_type,
                        None,
                        body,
                        None,
                        None,
                    ),
                    SyntaxKind::SetAccessor,
                    start,
                ))
            });
        }

        if !self.at_property_name_start() {
            return None;
        }

        let name = self.parse_property_name();
        let question =
            if self.at(SyntaxKind::QuestionToken) || self.at(SyntaxKind::ExclamationToken) {
                Some(self.take_token())
            } else {
                None
            };

        let modifiers_slice = self.arena.alloc_slice(&modifiers);

        // A `(` or `<` here makes it a method rather than a property.
        if self.at(SyntaxKind::OpenParenToken) || self.at(SyntaxKind::LessThanToken) {
            let type_parameters = self.parse_type_parameters();
            let parameters = self.parse_parameter_list();
            let return_type = self.parse_type_annotation();
            let body = self.parse_method_body();
            let type_parameters = self.arena.alloc_slice(&type_parameters);
            let parameters = self.arena.alloc_slice(&parameters);
            let node = self.finish_node(
                MethodDeclaration::new(
                    modifiers_slice,
                    asterisk,
                    name,
                    question,
                    type_parameters,
                    parameters,
                    return_type,
                    None,
                    body,
                ),
                SyntaxKind::MethodDeclaration,
                start,
            );
            return Some(ClassElement::MethodDeclaration(node));
        }

        let type_node = self.parse_type_annotation();
        let initializer = if self.eat(SyntaxKind::EqualsToken) {
            Some(self.parse_assignment_expression())
        } else {
            None
        };
        self.parse_semicolon();
        let node = self.finish_node(
            PropertyDeclaration::new(modifiers_slice, name, question, type_node, initializer),
            SyntaxKind::PropertyDeclaration,
            start,
        );
        Some(ClassElement::PropertyDeclaration(node))
    }

    /// A method body, or `None` for an overload signature.
    fn parse_method_body(&mut self) -> Option<FunctionBody<'a>> {
        if self.at(SyntaxKind::OpenBraceToken) {
            Some(FunctionBody::Block(self.parse_block()))
        } else {
            self.parse_semicolon();
            None
        }
    }

    /// Whether the token after `get`/`set` starts a property name.
    fn next_starts_accessor_name(&mut self) -> bool {
        let mut matched = false;
        self.try_parse(|p| {
            p.next_token();
            matched = p.at_property_name_start();
            None::<()>
        });
        matched
    }

    fn next_is_open_paren(&mut self) -> bool {
        let mut matched = false;
        self.try_parse(|p| {
            p.next_token();
            matched = p.at(SyntaxKind::OpenParenToken);
            None::<()>
        });
        matched
    }

    /// Whether the cursor can begin a property name.
    fn at_property_name_start(&self) -> bool {
        matches!(
            self.token.kind,
            SyntaxKind::Identifier
                | SyntaxKind::StringLiteral
                | SyntaxKind::NumericLiteral
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::PrivateIdentifier
        ) || self.token.kind.is_keyword()
    }

    /// `interface I<T> extends J { … }`.
    pub(crate) fn parse_interface_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        self.expect(SyntaxKind::InterfaceKeyword);
        let name = self.parse_identifier();
        let type_parameters = self.parse_type_parameters();
        let heritage = self.parse_heritage_clauses();

        self.expect(SyntaxKind::OpenBraceToken);
        let mut members = Vec::new();
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            if let Some(member) = self.parse_type_member() {
                members.push(member);
            } else {
                self.error_at_current(&messages::PROPERTY_OR_SIGNATURE_EXPECTED);
                self.next_token();
                continue;
            }
            if !self.eat(SyntaxKind::SemicolonToken)
                && !self.eat(SyntaxKind::CommaToken)
                && self.pos() == before
            {
                self.next_token();
            }
        }
        self.expect(SyntaxKind::CloseBraceToken);

        let modifiers = self.arena.alloc_slice(modifiers);
        let type_parameters = self.arena.alloc_slice(&type_parameters);
        let heritage = self.arena.alloc_slice(&heritage);
        let members = self.arena.alloc_slice(&members);
        let node = self.finish_node(
            InterfaceDeclaration::new(modifiers, Some(name), type_parameters, heritage, members),
            SyntaxKind::InterfaceDeclaration,
            start,
        );
        Statement::InterfaceDeclaration(node)
    }

    /// `type T<U> = …;`.
    pub(crate) fn parse_type_alias_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        self.expect(SyntaxKind::TypeKeyword);
        let name = self.parse_identifier();
        let type_parameters = self.parse_type_parameters();
        self.expect(SyntaxKind::EqualsToken);
        let type_node = self.parse_type();
        self.parse_semicolon();

        let modifiers = self.arena.alloc_slice(modifiers);
        let type_parameters = self.arena.alloc_slice(&type_parameters);
        let node = self.finish_node(
            TypeAliasDeclaration::new(modifiers, Some(name), type_parameters, Some(type_node)),
            SyntaxKind::TypeAliasDeclaration,
            start,
        );
        Statement::TypeAliasDeclaration(node)
    }

    /// `enum E { A, B = 1 }`.
    pub(crate) fn parse_enum_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        self.expect(SyntaxKind::EnumKeyword);
        let name = self.parse_identifier();
        self.expect(SyntaxKind::OpenBraceToken);

        let mut members = Vec::new();
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            let member_start = self.pos();
            let member_name = self.parse_property_name();
            let initializer = if self.eat(SyntaxKind::EqualsToken) {
                Some(self.parse_assignment_expression())
            } else {
                None
            };
            members.push(self.finish_node(
                EnumMember::new(member_name, initializer, &[], None),
                SyntaxKind::EnumMember,
                member_start,
            ));
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
            if self.pos() == before {
                break;
            }
        }
        self.expect(SyntaxKind::CloseBraceToken);

        let modifiers = self.arena.alloc_slice(modifiers);
        let members = self.arena.alloc_slice(&members);
        let node = self.finish_node(
            EnumDeclaration::new(modifiers, Some(name), members),
            SyntaxKind::EnumDeclaration,
            start,
        );
        Statement::EnumDeclaration(node)
    }

    /// Type arguments in a heritage clause, which may be absent.
    fn parse_type_arguments_opt(&mut self) -> Vec<TypeNode<'a>> {
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
        if !self.at(SyntaxKind::GreaterThanToken) {
            self.rescan_greater_than();
        }
        self.expect(SyntaxKind::GreaterThanToken);
        arguments
    }
}
