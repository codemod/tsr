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

    /// Parse a return type, which may be a type predicate.
    ///
    /// `: x is T` and `: asserts x is T` are legal only in return position, which
    /// is why this is separate from [`Parser::parse_type_annotation`].
    pub(crate) fn parse_return_type_annotation(&mut self) -> Option<TypeNode<'a>> {
        if !self.eat(SyntaxKind::ColonToken) {
            return None;
        }
        Some(self.with_conditional_types_allowed(Parser::parse_type_or_type_predicate))
    }

    /// A type, or a type predicate if one is in position.
    pub(crate) fn parse_type_or_type_predicate(&mut self) -> TypeNode<'a> {
        let start = self.pos();

        // `asserts x` and `asserts x is T`. `asserts` is contextual: `asserts` on
        // its own is an ordinary type reference.
        if self.at(SyntaxKind::AssertsKeyword) && self.next_starts_predicate_subject() {
            let asserts = self.take_token();
            let parameter = self.parse_type_predicate_parameter();
            let type_node =
                if self.eat(SyntaxKind::IsKeyword) { Some(self.parse_type()) } else { None };
            return TypeNode::TypePredicateNode(self.finish_node(
                TypePredicateNode::new(Some(asserts), Some(parameter), type_node),
                SyntaxKind::TypePredicate,
                start,
            ));
        }

        // `x is T`.
        if (self.at(SyntaxKind::Identifier)
            || self.at(SyntaxKind::ThisKeyword)
            || crate::statement::is_contextual_keyword(self.token.kind))
            && self.next_is_is_keyword()
        {
            let parameter = self.parse_type_predicate_parameter();
            self.expect(SyntaxKind::IsKeyword);
            let type_node = self.parse_type();
            return TypeNode::TypePredicateNode(self.finish_node(
                TypePredicateNode::new(None, Some(parameter), Some(type_node)),
                SyntaxKind::TypePredicate,
                start,
            ));
        }

        self.parse_type()
    }

    /// The subject of a type predicate: an identifier or `this`.
    fn parse_type_predicate_parameter(&mut self) -> TypePredicateParameterName<'a> {
        if self.at(SyntaxKind::ThisKeyword) {
            let start = self.pos();
            self.next_token();
            return TypePredicateParameterName::ThisTypeNode(self.finish_node(
                ThisTypeNode::new(),
                SyntaxKind::ThisType,
                start,
            ));
        }
        TypePredicateParameterName::Identifier(self.parse_identifier())
    }

    /// Whether `in`/`out`/`const` here is the parameter's *name* rather than a
    /// modifier — `<const>` declares a parameter called `const`.
    fn next_is_type_parameter_terminator(&mut self) -> bool {
        self.peek_kind(|kind| {
            matches!(
                kind,
                SyntaxKind::GreaterThanToken
                    | SyntaxKind::CommaToken
                    | SyntaxKind::EqualsToken
                    | SyntaxKind::ExtendsKeyword
            )
        })
    }

    /// Whether a `.` follows, making a keyword a namespace qualifier.
    fn next_is_dot(&mut self) -> bool {
        self.peek_kind(|kind| kind == SyntaxKind::DotToken)
    }

    fn next_starts_predicate_subject(&mut self) -> bool {
        self.peek_kind(|kind| {
            kind == SyntaxKind::Identifier
                || kind == SyntaxKind::ThisKeyword
                || crate::statement::is_contextual_keyword(kind)
        })
    }

    fn next_is_is_keyword(&mut self) -> bool {
        // `p.token == ast.KindIsKeyword && !p.hasPrecedingLineBreak()`
        // (`parser.go:3408`). **The line-break half is not decoration.** A
        // return type on its own line followed by a member named `is` is the
        // shape `conformance/typePredicateASI` records:
        //
        // ```text
        // interface I {
        //     foo(callback: (a: any, b: any) => void): I
        //     is(): boolean;
        // }
        // ```
        //
        // Without the guard, `I` and the *next member's* name parse as one
        // predicate: the corpus line reads `(callback: (a: any, b: any) =>
        // void) => I is any` where the baseline says `=> I`, and the `is()`
        // member disappears from the interface. Found by reading the residual
        // wrong lines of the type-predicate build rather than by a parser test
        // — `docs/architecture/checker-notes-typepred.md` §3.
        let mut matched = false;
        self.try_parse(|p| {
            p.next_token();
            matched = p.token.kind == SyntaxKind::IsKeyword && !p.token.has_preceding_line_break();
            None::<()>
        });
        matched
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
        if self.disallow_conditional_types > 0
            || !self.at(SyntaxKind::ExtendsKeyword)
            || self.token.has_preceding_line_break()
        {
            return check;
        }
        self.next_token();
        let extends = self.with_conditional_types_disallowed(Parser::parse_type);
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
        // A leading `|` is allowed: `type T = | A | B` — and it forces the
        // union node even with one constituent (`parser.go:2649`,
        // `p.token == operator || hasLeadingOperator`); the degenerate node
        // is what keeps `getAliasSymbolForTypeNode` from naming the
        // constituent (checker-notes-narrow.md §89.1).
        let has_leading = self.eat(SyntaxKind::BarToken);
        let first = self.parse_intersection_type();
        if !has_leading && !self.at(SyntaxKind::BarToken) {
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
        // A leading `&` forces the node like the leading `|` above.
        let has_leading = self.eat(SyntaxKind::AmpersandToken);
        let first = self.parse_type_operator_or_higher();
        if !has_leading && !self.at(SyntaxKind::AmpersandToken) {
            return first;
        }
        let mut types = vec![first];
        while self.eat(SyntaxKind::AmpersandToken) {
            types.push(self.parse_type_operator_or_higher());
        }
        let types = self.arena.alloc_slice(&types);
        let node =
            self.finish_node(IntersectionTypeNode::new(types), SyntaxKind::IntersectionType, start);
        TypeNode::IntersectionTypeNode(node)
    }

    /// Parse the operator-precedence type layer.
    ///
    /// Ported from TypeScript's `parseTypeOperatorOrHigher` in
    /// `src/compiler/parser.ts`: a conditional-type restriction applies to an
    /// immediately nested `infer`, but ordinary nested type references restore
    /// conditional types inside their own type arguments.
    fn parse_type_operator_or_higher(&mut self) -> TypeNode<'a> {
        if matches!(
            self.token.kind,
            SyntaxKind::InferKeyword
                | SyntaxKind::KeyOfKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::UniqueKeyword
        ) {
            self.parse_postfix_type()
        } else {
            self.with_conditional_types_allowed(Parser::parse_postfix_type)
        }
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
        // Bound before matching: a guard below needs `&mut self` for lookahead.
        let token_kind = self.token.kind;
        match token_kind {
            // Keyword types: `string`, `number`, `any`, `void`, … unless a `.`
            // follows, in which case the keyword names a namespace:
            // `var x: string.X` refers to a namespace called `string`.
            kind if kind.is_keyword_type() && !self.next_is_dot() => {
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
                // Parentheses create a fresh conditional-type grammar context:
                // `T extends (infer U extends number ? 1 : 0) ? ...` parses the
                // inner `extends` as a conditional even though the outer
                // extends-side otherwise disallows conditional types.
                let inner = self.with_conditional_types_allowed(Parser::parse_type);
                self.expect(SyntaxKind::CloseParenToken);
                let node = self.finish_node(
                    ParenthesizedTypeNode::new(Some(inner)),
                    SyntaxKind::ParenthesizedType,
                    start,
                );
                TypeNode::ParenthesizedTypeNode(node)
            }
            SyntaxKind::AbstractKeyword | SyntaxKind::NewKeyword => {
                // `abstract new (…) => T` — a constructor type that cannot be
                // instantiated directly.
                let modifiers = if self.at(SyntaxKind::AbstractKeyword) {
                    let modifier_start = self.pos();
                    self.next_token();
                    let token = self.alloc_token(
                        SyntaxKind::AbstractKeyword,
                        tsr_core::Span::new(modifier_start, self.pos()),
                    );
                    self.arena.alloc_slice(&[ModifierLike::Token(token)])
                } else {
                    &[][..]
                };
                self.expect(SyntaxKind::NewKeyword);
                let type_parameters = self.parse_type_parameters();
                let parameters = self.parse_parameter_list();
                self.expect(SyntaxKind::EqualsGreaterThanToken);
                let return_type =
                    self.with_conditional_types_allowed(Parser::parse_type_or_type_predicate);
                let type_parameters = self.arena.alloc_slice(&type_parameters);
                let parameters = self.arena.alloc_slice(&parameters);
                let node = self.finish_node(
                    ConstructorTypeNode::new(
                        modifiers,
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
            SyntaxKind::OpenBraceToken => {
                if self.brace_holds_mapped_type() {
                    self.parse_mapped_type()
                } else {
                    self.parse_type_literal()
                }
            }
            // `import("m").T` and `typeof import("m")`.
            SyntaxKind::ImportKeyword => {
                self.next_token();
                self.expect(SyntaxKind::OpenParenToken);
                let argument = self.parse_type();
                let attributes = self.parse_import_type_attributes();
                self.expect(SyntaxKind::CloseParenToken);
                let qualifier = if self.eat(SyntaxKind::DotToken) {
                    Some(self.parse_entity_name())
                } else {
                    None
                };
                let type_arguments = self.parse_type_arguments_of_type_reference();
                let type_arguments = self.arena.alloc_slice(&type_arguments);
                let node = self.finish_node(
                    ImportTypeNode::new(
                        false,
                        Some(argument),
                        attributes,
                        qualifier,
                        type_arguments,
                    ),
                    SyntaxKind::ImportType,
                    start,
                );
                TypeNode::ImportTypeNode(node)
            }
            SyntaxKind::ThisKeyword => {
                self.next_token();
                let node = self.finish_node(ThisTypeNode::new(), SyntaxKind::ThisType, start);
                TypeNode::ThisTypeNode(node)
            }
            SyntaxKind::TypeOfKeyword => {
                self.next_token();
                // `typeof import("m").A` — a type query over an import type.
                if self.at(SyntaxKind::ImportKeyword) {
                    let TypeNode::ImportTypeNode(import) = self.parse_primary_type() else {
                        return self.missing_type();
                    };
                    let node = self.finish_node(
                        ImportTypeNode::new(
                            true,
                            import.argument,
                            import.attributes,
                            import.qualifier,
                            import.type_arguments,
                        ),
                        SyntaxKind::ImportType,
                        start,
                    );
                    return TypeNode::ImportTypeNode(node);
                }
                let name = self.parse_entity_name();
                // `typeof foo<T>` — an instantiation expression in type position.
                let type_arguments = self.parse_type_arguments_of_type_reference();
                let type_arguments = self.arena.alloc_slice(&type_arguments);
                let node = self.finish_node(
                    TypeQueryNode::new(Some(name), type_arguments),
                    SyntaxKind::TypeQuery,
                    start,
                );
                TypeNode::TypeQueryNode(node)
            }
            SyntaxKind::InferKeyword => {
                self.next_token();
                let parameter_start = self.pos();
                let name = self.parse_identifier();
                // `infer R extends T` constrains the inference. The constraint
                // binds tighter than the enclosing conditional's `extends`, so it
                // is parsed here rather than left to `parse_conditional_type`.
                let constraint = if self.at(SyntaxKind::ExtendsKeyword) {
                    let already_disallowed = self.disallow_conditional_types > 0;
                    self.try_parse(|p| {
                        p.next_token();
                        let constraint = p.with_conditional_types_disallowed(Parser::parse_type);
                        (already_disallowed || !p.at(SyntaxKind::QuestionToken))
                            .then_some(constraint)
                    })
                } else {
                    None
                };
                let parameter = self.finish_node(
                    TypeParameterDeclaration::new(&[], Some(name), constraint, None, None),
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
            SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                self.parse_template_literal_type()
            }
            SyntaxKind::StringLiteral
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::NullKeyword => self.parse_literal_type(),
            SyntaxKind::MinusToken => self.parse_negative_literal_type(),
            // `x as const` — a const assertion, spelled as a type.
            SyntaxKind::ConstKeyword => {
                self.next_token();
                let node = self.finish_node(
                    TypeReferenceNode::new(None, &[]),
                    SyntaxKind::TypeReference,
                    start,
                );
                TypeNode::TypeReferenceNode(node)
            }
            SyntaxKind::Identifier => {
                let name = self.parse_entity_name();
                let type_arguments = self.parse_type_arguments_of_type_reference();
                let type_arguments = self.arena.alloc_slice(&type_arguments);
                let node = self.finish_node(
                    TypeReferenceNode::new(Some(name), type_arguments),
                    SyntaxKind::TypeReference,
                    start,
                );
                TypeNode::TypeReferenceNode(node)
            }
            // A contextual keyword can name a type: `require.I`, `type`, `module`.
            kind if crate::statement::is_contextual_keyword(kind) => {
                let name = self.parse_entity_name();
                let type_arguments = self.parse_type_arguments_of_type_reference();
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

    fn with_conditional_types_disallowed<T>(&mut self, parse: impl FnOnce(&mut Self) -> T) -> T {
        self.disallow_conditional_types += 1;
        let result = parse(self);
        self.disallow_conditional_types -= 1;
        result
    }

    fn with_conditional_types_allowed<T>(&mut self, parse: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.disallow_conditional_types;
        self.disallow_conditional_types = 0;
        let result = parse(self);
        self.disallow_conditional_types = saved;
        result
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
        // `(x: T) => x is U` is a predicate, same as a function's return type.
        let return_type = self.with_conditional_types_allowed(Parser::parse_type_or_type_predicate);
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

    /// A negative numeric literal type keeps its prefix expression as the
    /// `LiteralTypeNode` payload. Consuming `-` before building the ordinary
    /// numeric literal silently changed `type T = -1` into `type T = 1`.
    fn parse_negative_literal_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        let operator = self.take_token();
        let literal_start = self.pos();
        let operand = match self.token.kind {
            kind @ (SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral) => {
                let text = self.token_value();
                let flags = self.token.ast_flags();
                self.next_token();
                match kind {
                    SyntaxKind::NumericLiteral => Expression::NumericLiteral(self.finish_node(
                        NumericLiteral::new(text, flags),
                        SyntaxKind::NumericLiteral,
                        literal_start,
                    )),
                    SyntaxKind::BigIntLiteral => Expression::BigIntLiteral(self.finish_node(
                        BigIntLiteral::new(text, flags),
                        SyntaxKind::BigIntLiteral,
                        literal_start,
                    )),
                    _ => unreachable!(),
                }
            }
            // Error recovery: keep a malformed operand in the tree rather than
            // panicking on compiler tests such as `type T = -foo`.
            _ => self.parse_unary_expression(),
        };
        let prefix = self.finish_node(
            PrefixUnaryExpression::new(operator, Some(operand)),
            SyntaxKind::PrefixUnaryExpression,
            start,
        );
        let node = self.finish_node(
            LiteralTypeNode::new(Some(Node::PrefixUnaryExpression(prefix))),
            SyntaxKind::LiteralType,
            start,
        );
        TypeNode::LiteralTypeNode(node)
    }

    fn parse_literal_type_from(&mut self, start: u32) -> TypeNode<'a> {
        let literal_start = self.pos();
        let kind = self.token.kind;
        let text = self.token_value();
        let raw = self.token_text();
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
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                Node::NoSubstitutionTemplateLiteral(self.finish_node(
                    NoSubstitutionTemplateLiteral::new(text, flags, flags, raw),
                    SyntaxKind::NoSubstitutionTemplateLiteral,
                    literal_start,
                ))
            }
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

    /// `` `a${T}b` `` as a type.
    fn parse_template_literal_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        if self.at(SyntaxKind::NoSubstitutionTemplateLiteral) {
            // No substitutions: it is just a string literal type.
            return self.parse_literal_type();
        }

        let head_start = self.pos();
        let raw = self.token_text();
        let text = self.token_value();
        let flags = self.token.ast_flags();
        self.next_token();
        let head = self.finish_node(
            TemplateHead::new(text, raw, flags, flags),
            SyntaxKind::TemplateHead,
            head_start,
        );

        let mut spans = Vec::new();
        loop {
            let span_start = self.pos();
            let type_node = self.parse_type();
            if !self.at(SyntaxKind::CloseBraceToken) {
                self.error_at_current_with(&messages::_0_EXPECTED, &["}"]);
                break;
            }
            self.rescan_template_continuation();
            let literal_start = self.pos();
            let is_tail = self.at(SyntaxKind::TemplateTail);
            let raw = self.token_text();
            let text = self.token_value();
            let flags = self.token.ast_flags();
            self.next_token();
            let literal = if is_tail {
                TemplateMiddleOrTail::TemplateTail(self.finish_node(
                    TemplateTail::new(text, raw, flags, flags),
                    SyntaxKind::TemplateTail,
                    literal_start,
                ))
            } else {
                TemplateMiddleOrTail::TemplateMiddle(self.finish_node(
                    TemplateMiddle::new(text, raw, flags, flags),
                    SyntaxKind::TemplateMiddle,
                    literal_start,
                ))
            };
            spans.push(self.finish_node(
                TemplateLiteralTypeSpan::new(Some(type_node), Some(literal)),
                SyntaxKind::TemplateLiteralTypeSpan,
                span_start,
            ));
            if is_tail {
                break;
            }
        }

        let spans = self.arena.alloc_slice(&spans);
        let node = self.finish_node(
            TemplateLiteralTypeNode::new(Some(head), spans),
            SyntaxKind::TemplateLiteralType,
            start,
        );
        TypeNode::TemplateLiteralTypeNode(node)
    }

    /// One tuple element, which may be rest, optional, or named.
    fn parse_tuple_element(&mut self) -> TypeNode<'a> {
        let start = self.pos();

        // `...T`
        if self.at(SyntaxKind::DotDotDotToken) {
            self.next_token();
            let inner = self.parse_tuple_element();
            return TypeNode::RestTypeNode(self.finish_node(
                RestTypeNode::new(Some(inner)),
                SyntaxKind::RestType,
                start,
            ));
        }

        // `name: T` and `name?: T` are named members, not annotations.
        if (self.at(SyntaxKind::Identifier)
            || crate::statement::is_contextual_keyword(self.token.kind))
            && self.next_starts_named_tuple_member()
        {
            let name = self.parse_identifier();
            let question =
                if self.at(SyntaxKind::QuestionToken) { Some(self.take_token()) } else { None };
            self.expect(SyntaxKind::ColonToken);
            let inner = self.parse_type();
            return TypeNode::NamedTupleMember(self.finish_node(
                NamedTupleMember::new(None, Some(name), question, Some(inner)),
                SyntaxKind::NamedTupleMember,
                start,
            ));
        }

        let inner = self.parse_type();
        // `T?`
        if self.at(SyntaxKind::QuestionToken) {
            self.next_token();
            return TypeNode::OptionalTypeNode(self.finish_node(
                OptionalTypeNode::new(Some(inner)),
                SyntaxKind::OptionalType,
                start,
            ));
        }
        inner
    }

    /// Whether `name` here labels a tuple member rather than being its type.
    ///
    /// `[a: number]` and `[a?: number]` are named; `[any?]` is an optional
    /// element whose type happens to be a keyword. Only a `:` — after the
    /// optional `?` — settles it.
    fn next_starts_named_tuple_member(&mut self) -> bool {
        let mut matched = false;
        self.try_parse(|p| {
            p.next_token();
            if p.at(SyntaxKind::QuestionToken) {
                p.next_token();
            }
            matched = p.at(SyntaxKind::ColonToken);
            None::<()>
        });
        matched
    }

    fn parse_tuple_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBracketToken);
        let mut elements = Vec::new();
        while !self.at(SyntaxKind::CloseBracketToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            elements.push(self.parse_tuple_element());
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

    /// Whether `{` opens a mapped type rather than a type literal.
    ///
    /// A mapped type is `{ [K in T]: U }`; the distinguishing shape is
    /// `[ identifier in` (possibly after `readonly`/`+`/`-`).
    fn brace_holds_mapped_type(&mut self) -> bool {
        let mut matched = false;
        self.try_parse(|p| {
            p.next_token();
            while matches!(
                p.token.kind,
                SyntaxKind::ReadonlyKeyword | SyntaxKind::PlusToken | SyntaxKind::MinusToken
            ) {
                p.next_token();
            }
            if !p.at(SyntaxKind::OpenBracketToken) {
                return None::<()>;
            }
            p.next_token();
            if !p.at(SyntaxKind::Identifier)
                && !crate::statement::is_contextual_keyword(p.token.kind)
            {
                return None;
            }
            p.next_token();
            matched = p.at(SyntaxKind::InKeyword);
            None
        });
        matched
    }

    /// `{ readonly [K in T as U]?: V }`.
    fn parse_mapped_type(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);

        // `+readonly` / `-readonly` store the sign token. The following keyword
        // is implied by that token kind when the printer reconstructs the syntax,
        // matching upstream's `MappedTypeNode.ReadonlyToken` representation.
        let readonly = if matches!(self.token.kind, SyntaxKind::PlusToken | SyntaxKind::MinusToken)
        {
            let sign = self.take_token();
            self.expect(SyntaxKind::ReadonlyKeyword);
            Some(sign)
        } else if self.at(SyntaxKind::ReadonlyKeyword) {
            Some(self.take_token())
        } else {
            None
        };

        self.expect(SyntaxKind::OpenBracketToken);
        let parameter_start = self.pos();
        let name = self.parse_identifier();
        self.expect(SyntaxKind::InKeyword);
        let constraint = self.with_conditional_types_allowed(Parser::parse_type);
        let parameter = self.finish_node(
            TypeParameterDeclaration::new(&[], Some(name), Some(constraint), None, None),
            SyntaxKind::TypeParameter,
            parameter_start,
        );
        // `as U` renames the key.
        let name_type = if self.eat(SyntaxKind::AsKeyword) {
            Some(self.with_conditional_types_allowed(Parser::parse_type))
        } else {
            None
        };
        self.expect(SyntaxKind::CloseBracketToken);

        let question = if matches!(self.token.kind, SyntaxKind::PlusToken | SyntaxKind::MinusToken)
        {
            let sign = self.take_token();
            self.expect(SyntaxKind::QuestionToken);
            Some(sign)
        } else if self.at(SyntaxKind::QuestionToken) {
            Some(self.take_token())
        } else {
            None
        };

        let value = self.parse_type_annotation();
        self.eat(SyntaxKind::SemicolonToken);
        self.expect(SyntaxKind::CloseBraceToken);

        let node = self.finish_node(
            MappedTypeNode::new(readonly, Some(parameter), name_type, question, value, &[]),
            SyntaxKind::MappedType,
            start,
        );
        TypeNode::MappedTypeNode(node)
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
    ///
    /// Five shapes share this position, distinguished by their first tokens:
    /// call signatures `(): T`, construct signatures `new (): T`, index
    /// signatures `[k: string]: T`, accessors `get x(): T`, and named
    /// property/method signatures.
    pub(crate) fn parse_type_member(&mut self) -> Option<TypeElement<'a>> {
        let docs = self.parse_leading_jsdoc();
        let member = self.parse_type_member_worker();
        if let Some(member) = member {
            self.attach_jsdoc(member.into(), docs);
        }
        member
    }

    fn parse_type_member_worker(&mut self) -> Option<TypeElement<'a>> {
        let start = self.pos();

        // `(): T` and `<T>(): U` — a call signature has no name.
        if self.at(SyntaxKind::OpenParenToken) || self.at(SyntaxKind::LessThanToken) {
            let type_parameters = self.parse_type_parameters();
            let parameters = self.parse_parameter_list();
            let return_type = self.parse_return_type_annotation();
            let type_parameters = self.arena.alloc_slice(&type_parameters);
            let parameters = self.arena.alloc_slice(&parameters);
            return Some(TypeElement::CallSignatureDeclaration(self.finish_node(
                CallSignatureDeclaration::new(type_parameters, parameters, return_type, None),
                SyntaxKind::CallSignature,
                start,
            )));
        }

        // `new (): T` — but `new` can also name a property, so the next token
        // decides.
        if self.at(SyntaxKind::NewKeyword) && self.next_starts_signature() {
            self.next_token();
            let type_parameters = self.parse_type_parameters();
            let parameters = self.parse_parameter_list();
            let return_type = self.parse_return_type_annotation();
            let type_parameters = self.arena.alloc_slice(&type_parameters);
            let parameters = self.arena.alloc_slice(&parameters);
            return Some(TypeElement::ConstructSignatureDeclaration(self.finish_node(
                ConstructSignatureDeclaration::new(type_parameters, parameters, return_type, None),
                SyntaxKind::ConstructSignature,
                start,
            )));
        }

        let modifiers = self.parse_modifiers();

        // `[key: string]: T` is an index signature; `[Symbol.iterator]()` is a
        // computed property name. Only the former has `identifier :` inside.
        if self.at(SyntaxKind::OpenBracketToken) && self.bracket_holds_index_signature() {
            // `parseIndexSignatureDeclaration` (`parser.go:3562`) is
            // `parseBracketedList(PCParameters, parseParameter, [, ])` — a
            // *list*, of zero or more parameters, each parsed by the ordinary
            // parameter parser and so each able to carry modifiers.
            //
            // This port read exactly one bare `identifier: type`, which was
            // consistent with the old one-shape lookahead and became wrong the
            // moment §209 admitted upstream's eight recovery shapes: `[]` has
            // no parameter to read and `[public x: string]` has a modifier
            // before it. Both derailed the whole member — the first
            // manufactured a missing identifier, the second parsed `public` as
            // the parameter name and then failed to find `]`. §209.
            self.expect(SyntaxKind::OpenBracketToken);
            let mut parsed = Vec::new();
            while !self.at(SyntaxKind::CloseBracketToken) && !self.at(SyntaxKind::EndOfFile) {
                let before = self.pos();
                parsed.push(self.parse_parameter());
                if !self.eat(SyntaxKind::CommaToken) {
                    break;
                }
                if self.pos() == before {
                    break;
                }
            }
            self.expect(SyntaxKind::CloseBracketToken);
            let value_type = self.parse_type_annotation();
            let modifiers = self.arena.alloc_slice(&modifiers);
            let parameters = self.arena.alloc_slice(&parsed);
            return Some(TypeElement::IndexSignatureDeclaration(self.finish_node(
                IndexSignatureDeclaration::new(modifiers, parameters, value_type, None, &[]),
                SyntaxKind::IndexSignature,
                start,
            )));
        }

        // `get x(): T` / `set x(v: T)`.
        if matches!(self.token.kind, SyntaxKind::GetKeyword | SyntaxKind::SetKeyword)
            && self.next_starts_property_name()
        {
            let is_getter = self.at(SyntaxKind::GetKeyword);
            self.next_token();
            let name = self.parse_property_name();
            let parameters = self.parse_parameter_list();
            let return_type = self.parse_return_type_annotation();
            let modifiers = self.arena.alloc_slice(&modifiers);
            let parameters = self.arena.alloc_slice(&parameters);
            return Some(if is_getter {
                TypeElement::GetAccessorDeclaration(self.finish_node(
                    GetAccessorDeclaration::new(
                        modifiers,
                        name,
                        &[],
                        parameters,
                        return_type,
                        None,
                        None,
                        None,
                        None,
                    ),
                    SyntaxKind::GetAccessor,
                    start,
                ))
            } else {
                TypeElement::SetAccessorDeclaration(self.finish_node(
                    SetAccessorDeclaration::new(
                        modifiers,
                        name,
                        &[],
                        parameters,
                        return_type,
                        None,
                        None,
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
            if self.at(SyntaxKind::QuestionToken) { Some(self.take_token()) } else { None };
        let modifiers = self.arena.alloc_slice(&modifiers);

        // `m(): T` and `m<U>(): T` are method signatures.
        if self.at(SyntaxKind::OpenParenToken) || self.at(SyntaxKind::LessThanToken) {
            let type_parameters = self.parse_type_parameters();
            let parameters = self.parse_parameter_list();
            let return_type = self.parse_return_type_annotation();
            let type_parameters = self.arena.alloc_slice(&type_parameters);
            let parameters = self.arena.alloc_slice(&parameters);
            return Some(TypeElement::MethodSignatureDeclaration(self.finish_node(
                MethodSignatureDeclaration::new(
                    modifiers,
                    name,
                    question,
                    type_parameters,
                    parameters,
                    return_type,
                    None,
                ),
                SyntaxKind::MethodSignature,
                start,
            )));
        }

        let type_node = self.parse_type_annotation();
        Some(TypeElement::PropertySignatureDeclaration(self.finish_node(
            PropertySignatureDeclaration::new(modifiers, name, question, type_node, None),
            SyntaxKind::PropertySignature,
            start,
        )))
    }

    /// Whether `new` or `get`/`set` here introduces a signature rather than a name.
    fn next_starts_signature(&mut self) -> bool {
        self.peek_kind(|kind| {
            matches!(kind, SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken)
        })
    }

    pub(crate) fn next_starts_property_name(&mut self) -> bool {
        self.peek_kind(|kind| {
            matches!(
                kind,
                SyntaxKind::Identifier
                    | SyntaxKind::StringLiteral
                    | SyntaxKind::NumericLiteral
                    | SyntaxKind::OpenBracketToken
            ) || kind.is_keyword()
        })
    }

    /// Whether `[` opens an index signature rather than a computed name.
    ///
    /// `[k: string]: T` has `identifier :` inside; `[Symbol.iterator]()` does not.
    pub(crate) fn bracket_holds_index_signature(&mut self) -> bool {
        self.look_ahead(Self::next_is_unambiguously_index_signature)
    }

    /// `nextIsUnambiguouslyIndexSignature` (`parser.go:3513`), transcribed
    /// whole, comment list and all.
    ///
    /// The only *well-formed* sequence is `[id:`, and this port tested exactly
    /// that. Upstream deliberately admits eight more for **error recovery**,
    /// and its own comment lists them:
    ///
    /// ```text
    ///   [...        [id,        [id?,       [id?:
    ///   [id?]       [public id  [private id [protected id
    ///   []
    /// ```
    ///
    /// The last one is the whole of `compiler/indexWithoutParamType`:
    /// `var y: { []; }` is an index signature with **no parameters** upstream,
    /// so it is a type node and the `.types` walker never looks inside it —
    /// one assertion, `>y : {}`. Read as a computed property name instead, the
    /// `[]` becomes an *expression* and the walker emits two more lines. The
    /// types were never wrong; the walk was. §209.
    fn next_is_unambiguously_index_signature(&mut self) -> bool {
        self.next_token();
        if self.at(SyntaxKind::DotDotDotToken) || self.at(SyntaxKind::CloseBracketToken) {
            return true;
        }
        if self.token.kind.is_modifier() {
            self.next_token();
            if self.is_binding_identifier() {
                return true;
            }
        } else if !self.is_binding_identifier() {
            return false;
        } else {
            // Skip the identifier.
            self.next_token();
        }
        // A colon signifies a well-formed indexer. A comma is a *badly* formed
        // one, and is admitted precisely because a comma expression is illegal
        // in a computed property name — so it cannot be the other reading.
        if self.at(SyntaxKind::ColonToken) || self.at(SyntaxKind::CommaToken) {
            return true;
        }
        // A question mark could be an optional-property indexer or the start of
        // a conditional expression in a computed name; only what follows tells
        // them apart.
        if !self.at(SyntaxKind::QuestionToken) {
            return false;
        }
        self.next_token();
        self.at(SyntaxKind::ColonToken)
            || self.at(SyntaxKind::CommaToken)
            || self.at(SyntaxKind::CloseBracketToken)
    }

    /// A dotted name: `A`, `A.B`, `A.B.C`.
    pub(crate) fn parse_entity_name(&mut self) -> EntityName<'a> {
        let start = self.pos();
        // `parseEntityNameOfTypeReference` passes `allowReservedWords: true`
        // (`parser.go:2897`): `typeof this` and a reserved-word head both
        // parse, and the checker owns any complaint.
        let mut name = EntityName::Identifier(self.parse_identifier_name());
        while self.at(SyntaxKind::DotToken) {
            self.next_token();
            let right = self.parse_identifier_name();
            let node = self.finish_node(
                QualifiedName::new(Some(name), Some(right)),
                SyntaxKind::QualifiedName,
                start,
            );
            name = EntityName::QualifiedName(node);
        }
        name
    }

    /// Type arguments for a call: `f<T>(x)`.
    ///
    /// Returns `None` rather than recovering, because the caller uses failure to
    /// decide that `<` was a comparison after all.
    pub(crate) fn parse_type_arguments_for_call(&mut self) -> Option<Vec<TypeNode<'a>>> {
        // `Foo<<T>() => void>` opens with a single `<<` shift token.
        if self.at(SyntaxKind::LessThanLessThanToken) {
            self.rescan_less_than();
        }
        if !self.eat(SyntaxKind::LessThanToken) {
            return None;
        }
        let before = self.diagnostics.len();
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
        // Any complaint means this was not a type-argument list.
        if !self.at(SyntaxKind::GreaterThanToken) || self.diagnostics.len() != before {
            return None;
        }
        self.next_token();
        Some(arguments)
    }

    /// `<A, B>` after a type reference, if present.
    fn parse_type_arguments_of_type_reference(&mut self) -> Vec<TypeNode<'a>> {
        if self.token.has_preceding_line_break() { Vec::new() } else { self.parse_type_arguments() }
    }

    /// `<A, B>` after a type reference, if present.
    fn parse_type_arguments(&mut self) -> Vec<TypeNode<'a>> {
        if self.at(SyntaxKind::LessThanLessThanToken) {
            self.rescan_less_than();
        }
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
        if self.at(SyntaxKind::LessThanLessThanToken) {
            self.rescan_less_than();
        }
        if !self.at(SyntaxKind::LessThanToken) {
            return Vec::new();
        }
        self.next_token();
        let mut parameters = Vec::new();
        while !self.at(SyntaxKind::GreaterThanToken) && !self.at(SyntaxKind::EndOfFile) {
            let start = self.pos();
            // Variance annotations (`in`, `out`) and `const` type parameters.
            let mut modifiers = Vec::new();
            while matches!(
                self.token.kind,
                SyntaxKind::InKeyword | SyntaxKind::OutKeyword | SyntaxKind::ConstKeyword
            ) && !self.next_is_type_parameter_terminator()
            {
                let modifier_start = self.pos();
                let kind = self.token.kind;
                self.next_token();
                let token = self.alloc_token(kind, tsr_core::Span::new(modifier_start, self.pos()));
                modifiers.push(ModifierLike::Token(token));
            }
            let modifiers = self.arena.alloc_slice(&modifiers);
            let name = self.parse_identifier();
            let constraint =
                if self.eat(SyntaxKind::ExtendsKeyword) { Some(self.parse_type()) } else { None };
            let default =
                if self.eat(SyntaxKind::EqualsToken) { Some(self.parse_type()) } else { None };
            parameters.push(self.finish_node(
                TypeParameterDeclaration::new(modifiers, Some(name), constraint, None, default),
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
