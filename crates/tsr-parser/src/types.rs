//! Type annotation parsing.
//!
//! The type grammar is a separate language sharing TypeScript's token stream.
//! Precedence, loosest first: conditional, union, intersection, postfix
//! (`T[]`, `T[K]`), then primary.

use tsr_ast::*;
use tsr_diagnostics::messages;

use crate::list::ParsingContext;
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

    /// typescript-go's `Parser.parseTypeOrTypePredicate` (`parser.go`): an
    /// identifier followed by `is` on the same line is the predicate prefix
    /// (`parseTypePredicatePrefix`); `this is T` and `asserts x` are
    /// `parseNonArrayType` arms, reached through [`Parser::parse_type`].
    pub(crate) fn parse_type_or_type_predicate(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        if self.is_identifier() && self.next_is_is_keyword() {
            let parameter = TypePredicateParameterName::Identifier(self.parse_identifier());
            self.next_token();
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

    /// typescript-go's `Parser.parseKeywordTypeNode` (`parser.go`).
    fn parse_keyword_type_node(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        let kind = self.token.kind;
        self.next_token();
        TypeNode::KeywordTypeNode(self.finish_node(KeywordTypeNode::new(kind), kind, start))
    }

    /// The body of `parseTypeAliasDeclaration` (`parser.go:2102`): a lone
    /// `intrinsic` not followed by `.` is the intrinsic keyword type.
    pub(crate) fn parse_type_alias_body(&mut self) -> TypeNode<'a> {
        if self.at(SyntaxKind::IntrinsicKeyword) && !self.next_is_dot() {
            return self.parse_keyword_type_node();
        }
        self.parse_type()
    }

    /// Whether a `.` follows, making a keyword a namespace qualifier.
    fn next_is_dot(&mut self) -> bool {
        self.peek_kind(|kind| kind == SyntaxKind::DotToken)
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
    pub(crate) fn parse_type_operator_or_higher(&mut self) -> TypeNode<'a> {
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
        loop {
            if self.token.has_preceding_line_break() {
                break;
            }
            // `parsePostfixTypeOrHigher`'s JSDoc arms: `T!` and `T?`.
            if let Some(jsdoc) = self.parse_jsdoc_postfix_type(start, type_node) {
                type_node = jsdoc;
                continue;
            }
            if !self.at(SyntaxKind::OpenBracketToken) {
                break;
            }
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
            // typescript-go's `parseNonArrayType` gives `void` its own arm with
            // no dot lookahead: `void.x` is the keyword type, then `.` errors.
            // `intrinsic` has no arm at all: it is a type reference here, and
            // a keyword only as a whole type alias body
            // ([`Parser::parse_type_alias_body`]).
            kind if kind.is_keyword_type()
                && kind != SyntaxKind::IntrinsicKeyword
                && (kind == SyntaxKind::VoidKeyword || !self.next_is_dot()) =>
            {
                self.parse_keyword_type_node()
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
                    // `parseModifier`: the node ends at the keyword's end.
                    let token = self.take_token();
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
            // `parseNonArrayType`'s `this` arm: `this is T` is a predicate in
            // any type position (`parseThisTypePredicate`); the checker
            // rejects it where a predicate is not allowed (TS1228).
            SyntaxKind::ThisKeyword => {
                self.next_token();
                let node = self.finish_node(ThisTypeNode::new(), SyntaxKind::ThisType, start);
                if self.at(SyntaxKind::IsKeyword) && !self.token.has_preceding_line_break() {
                    self.next_token();
                    let type_node = self.parse_type();
                    return TypeNode::TypePredicateNode(self.finish_node(
                        TypePredicateNode::new(
                            None,
                            Some(TypePredicateParameterName::ThisTypeNode(node)),
                            Some(type_node),
                        ),
                        SyntaxKind::TypePredicate,
                        start,
                    ));
                }
                TypeNode::ThisTypeNode(node)
            }
            // `parseNonArrayType`'s `asserts` arm, `parseAssertsTypePredicate`;
            // otherwise `asserts` is an ordinary type reference.
            SyntaxKind::AssertsKeyword
                if self.look_ahead(Self::next_token_is_identifier_or_keyword_on_same_line) =>
            {
                let asserts = self.take_token();
                let parameter = self.parse_type_predicate_parameter();
                let type_node =
                    if self.eat(SyntaxKind::IsKeyword) { Some(self.parse_type()) } else { None };
                TypeNode::TypePredicateNode(self.finish_node(
                    TypePredicateNode::new(Some(asserts), Some(parameter), type_node),
                    SyntaxKind::TypePredicate,
                    start,
                ))
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
            // `parseNonArrayType`'s JSDoc arms: `*`, `?T`, `!T`.
            SyntaxKind::AsteriskToken
            | SyntaxKind::AsteriskEqualsToken
            | SyntaxKind::QuestionToken
            | SyntaxKind::QuestionQuestionToken
            | SyntaxKind::ExclamationToken => self.parse_jsdoc_prefix_type(),
            // `parseNonArrayType`'s default is `parseTypeReference`, whose entity
            // name admits reserved words (`@param {function} f`). Only
            // `function` — the reserved word `isStartOfType` names — is taken
            // so far: the others also reach here from `parse_type_parameters`'
            // missing list recovery (`type T<in in>`), where upstream never
            // asks for a type. docs/parity/notes/js.md.
            SyntaxKind::FunctionKeyword => {
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

    /// typescript-go's `Parser.nextIsUnambiguouslyStartOfFunctionType`
    /// (`parser.go`).
    fn next_is_unambiguously_start_of_function_type(&mut self) -> bool {
        self.next_token();
        // `( )` and `( ...`
        if self.at(SyntaxKind::CloseParenToken) || self.at(SyntaxKind::DotDotDotToken) {
            return true;
        }
        if self.skip_parameter_start() {
            // `( xxx :`, `( xxx ,`, `( xxx ?`, `( xxx =`
            if matches!(
                self.token.kind,
                SyntaxKind::ColonToken
                    | SyntaxKind::CommaToken
                    | SyntaxKind::QuestionToken
                    | SyntaxKind::EqualsToken
            ) {
                return true;
            }
            // `( xxx ) =>`
            if self.at(SyntaxKind::CloseParenToken) {
                self.next_token();
                return self.at(SyntaxKind::EqualsGreaterThanToken);
            }
        }
        false
    }

    /// typescript-go's `Parser.skipParameterStart` (`parser.go`).
    fn skip_parameter_start(&mut self) -> bool {
        if self.token.kind.is_modifier() {
            self.parse_modifiers();
        }
        self.eat(SyntaxKind::DotDotDotToken);
        if self.is_identifier() || self.at(SyntaxKind::ThisKeyword) {
            self.next_token();
            return true;
        }
        if self.at(SyntaxKind::OpenBracketToken) || self.at(SyntaxKind::OpenBraceToken) {
            // Only a binding pattern that parses without errors.
            let errors = self.diagnostics.len();
            self.parse_binding_name();
            return errors == self.diagnostics.len();
        }
        false
    }

    /// `(a: T) => R` and `<T>(a: T) => R`, when the lookahead confirms one.
    fn try_parse_function_type(&mut self) -> Option<TypeNode<'a>> {
        // `isStartOfFunctionTypeOrConstructorType`: a `(` opens a function
        // type only when what follows is unambiguously a parameter list
        // (`nextIsUnambiguouslyStartOfFunctionType`); otherwise it is a
        // parenthesized type, whose recovering parameter list must not be
        // tried.
        if self.at(SyntaxKind::OpenParenToken)
            && !self.look_ahead(Self::next_is_unambiguously_start_of_function_type)
        {
            return None;
        }
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

        self.parse_tuple_element_type_rest(start)
    }

    /// `parseTupleElementType` after its `...` arm.
    fn parse_tuple_element_type_rest(&mut self, start: u32) -> TypeNode<'a> {
        let inner = self.parse_type();
        // `parseTupleElementType` (`parser.go:3645`): a postfix `T?` the type
        // grammar read as a JSDoc nullable is the tuple's optional element.
        if let TypeNode::JSDocNullableType(nullable) = inner
            && let Some(element) = nullable.r#type
            && element.node_id().map(|id| self.nodes.span(id).start) == Some(start)
        {
            return TypeNode::OptionalTypeNode(self.finish_node(
                OptionalTypeNode::new(Some(element)),
                SyntaxKind::OptionalType,
                start,
            ));
        }
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
        // `parseBracketedList(PCTupleElementTypes, …, [, ])` (`parser.go:3613`).
        let elements = if self.expect(SyntaxKind::OpenBracketToken) {
            let (elements, _) = self
                .parse_delimited_list(ParsingContext::TupleElementTypes, Self::parse_tuple_element);
            self.expect(SyntaxKind::CloseBracketToken);
            elements
        } else {
            Vec::new()
        };
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

    /// Whether a type can start at the cursor — typescript-go's
    /// `Parser.isStartOfType` (`parser.go`). `in_start_of_parameter` refuses
    /// the forms that begin a parameter's own syntax.
    pub(crate) fn is_start_of_type(&mut self, in_start_of_parameter: bool) -> bool {
        match self.token.kind {
            SyntaxKind::AnyKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::StringKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::BigIntKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::ReadonlyKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::UniqueKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::UndefinedKeyword
            | SyntaxKind::NullKeyword
            | SyntaxKind::ThisKeyword
            | SyntaxKind::TypeOfKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::LessThanToken
            | SyntaxKind::BarToken
            | SyntaxKind::AmpersandToken
            | SyntaxKind::NewKeyword
            | SyntaxKind::StringLiteral
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::AsteriskToken
            | SyntaxKind::QuestionToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::DotDotDotToken
            | SyntaxKind::InferKeyword
            | SyntaxKind::ImportKeyword
            | SyntaxKind::AssertsKeyword
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead => true,
            SyntaxKind::FunctionKeyword => !in_start_of_parameter,
            SyntaxKind::MinusToken => {
                !in_start_of_parameter
                    && self.peek_kind(|kind| {
                        matches!(kind, SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral)
                    })
            }
            // Only `(` followed by `)`, `...`, an identifier, a modifier, or
            // something that starts a type: not `(1)`.
            SyntaxKind::OpenParenToken => {
                !in_start_of_parameter
                    && self.look_ahead(|p| {
                        p.next_token();
                        p.at(SyntaxKind::CloseParenToken)
                            || p.is_start_of_parameter()
                            || p.is_start_of_type(false)
                    })
            }
            _ => self.is_identifier(),
        }
    }

    /// typescript-go's `Parser.isStartOfParameter(isJSDocParameter: false)`
    /// (`parser.go`).
    pub(crate) fn is_start_of_parameter(&mut self) -> bool {
        self.at(SyntaxKind::DotDotDotToken)
            || self.is_binding_identifier_or_private_identifier_or_pattern()
            || self.token.kind.is_modifier()
            || self.at(SyntaxKind::AtToken)
            || self.is_start_of_type(true)
    }

    /// `{ a: string; b(): void }` — typescript-go's `Parser.parseTypeLiteral`
    /// (`parser.go`).
    fn parse_type_literal(&mut self) -> TypeNode<'a> {
        let start = self.pos();
        let members = self.parse_object_type_members();
        let members = self.arena.alloc_slice(&members);
        let node = self.finish_node(TypeLiteralNode::new(members), SyntaxKind::TypeLiteral, start);
        TypeNode::TypeLiteralNode(node)
    }

    /// typescript-go's `Parser.parseObjectTypeMembers` (`parser.go`): no
    /// members without the `{`.
    pub(crate) fn parse_object_type_members(&mut self) -> Vec<TypeElement<'a>> {
        if !self.expect(SyntaxKind::OpenBraceToken) {
            return Vec::new();
        }
        let members = self.parse_list(ParsingContext::TypeMembers, Self::parse_type_member);
        self.expect(SyntaxKind::CloseBraceToken);
        members
    }

    /// One member of a type literal or interface body — typescript-go's
    /// `Parser.parseTypeMember` (`parser.go`). Only called where
    /// `isListElement(PCTypeMembers)` holds.
    pub(crate) fn parse_type_member(&mut self) -> TypeElement<'a> {
        let docs = self.parse_leading_jsdoc();
        let member = self.parse_type_member_worker();
        self.attach_jsdoc(member.into(), docs);
        member
    }

    fn parse_type_member_worker(&mut self) -> TypeElement<'a> {
        let start = self.pos();

        // `(): T` and `<T>(): U` — a call signature has no name.
        if self.at(SyntaxKind::OpenParenToken) || self.at(SyntaxKind::LessThanToken) {
            return self.parse_signature_member(start, false);
        }
        // `new (): T` — but `new` can also name a property, so the next token
        // decides.
        if self.at(SyntaxKind::NewKeyword) && self.next_starts_signature() {
            return self.parse_signature_member(start, true);
        }

        let modifiers = self.parse_modifiers();

        if let Some(is_getter) = self.parse_get_or_set_contextual_modifier() {
            return match self.parse_accessor_declaration(start, &modifiers, is_getter, true) {
                AccessorDeclaration::GetAccessorDeclaration(node) => {
                    TypeElement::GetAccessorDeclaration(node)
                }
                AccessorDeclaration::SetAccessorDeclaration(node) => {
                    TypeElement::SetAccessorDeclaration(node)
                }
            };
        }

        // `[key: string]: T` is an index signature; `[Symbol.iterator]()` is a
        // computed property name.
        if self.at(SyntaxKind::OpenBracketToken) && self.bracket_holds_index_signature() {
            return TypeElement::IndexSignatureDeclaration(
                self.parse_index_signature_declaration(start, &modifiers),
            );
        }

        // `parsePropertyOrMethodSignature`.
        let name = self.parse_property_name();
        let question =
            if self.at(SyntaxKind::QuestionToken) { Some(self.take_token()) } else { None };
        let modifiers = self.arena.alloc_slice(&modifiers);
        if self.at(SyntaxKind::OpenParenToken) || self.at(SyntaxKind::LessThanToken) {
            // Method signatures don't exist in expression contexts, so they
            // have neither [Yield] nor [Await].
            let type_parameters = self.parse_type_parameters();
            let parameters = self.with_await_context(false, Self::parse_parameter_list);
            let return_type = self.parse_return_type_in_type();
            self.parse_type_member_semicolon();
            let type_parameters = self.arena.alloc_slice(&type_parameters);
            let parameters = self.arena.alloc_slice(&parameters);
            return TypeElement::MethodSignatureDeclaration(self.finish_node(
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
            ));
        }
        let type_node = self.parse_type_annotation();
        // Type literal properties cannot have initializers, but one is parsed
        // so the checker can report it.
        let initializer = if self.eat(SyntaxKind::EqualsToken) {
            Some(self.parse_assignment_expression())
        } else {
            None
        };
        self.parse_type_member_semicolon();
        TypeElement::PropertySignatureDeclaration(self.finish_node(
            PropertySignatureDeclaration::new(modifiers, name, question, type_node, initializer),
            SyntaxKind::PropertySignature,
            start,
        ))
    }

    /// typescript-go's `Parser.parseSignatureMember` (`parser.go`): a call
    /// signature, or with `is_construct` a construct signature after `new`.
    fn parse_signature_member(&mut self, start: u32, is_construct: bool) -> TypeElement<'a> {
        if is_construct {
            self.expect(SyntaxKind::NewKeyword);
        }
        let type_parameters = self.parse_type_parameters();
        let parameters = self.with_await_context(false, Self::parse_parameter_list);
        let return_type = self.parse_return_type_in_type();
        self.parse_type_member_semicolon();
        let type_parameters = self.arena.alloc_slice(&type_parameters);
        let parameters = self.arena.alloc_slice(&parameters);
        if is_construct {
            TypeElement::ConstructSignatureDeclaration(self.finish_node(
                ConstructSignatureDeclaration::new(type_parameters, parameters, return_type, None),
                SyntaxKind::ConstructSignature,
                start,
            ))
        } else {
            TypeElement::CallSignatureDeclaration(self.finish_node(
                CallSignatureDeclaration::new(type_parameters, parameters, return_type, None),
                SyntaxKind::CallSignature,
                start,
            ))
        }
    }

    /// typescript-go's `Parser.parseReturnType(KindColonToken, isType: true)`
    /// (`parser.go`): `=>` where `:` belongs is reported and the type parsed
    /// anyway — easy to get backward in a type context.
    fn parse_return_type_in_type(&mut self) -> Option<TypeNode<'a>> {
        if self.at(SyntaxKind::EqualsGreaterThanToken) {
            self.error_at_current_with(&messages::_0_EXPECTED, &[":"]);
            self.next_token();
            return Some(self.with_conditional_types_allowed(Parser::parse_type_or_type_predicate));
        }
        self.parse_return_type_annotation()
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
            // `Object.<K, V>`: the entity is part of a JSDoc-style generic
            // (`parseEntityName`, `parser.go:2910`).
            if self.at(SyntaxKind::LessThanToken) {
                break;
            }
            // `parseRightSideOfDot`'s first arm: `A.` followed by a
            // line break and `identifierOrKeyword identifierOrKeyword` is a
            // missing name right after the dot, not a qualifier.
            let right = if self.right_side_of_dot_is_missing() {
                self.report_missing_right_side_of_dot();
                self.missing_identifier()
            } else {
                self.parse_identifier_name()
            };
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
        let mut missing_slots: Vec<u32> = Vec::new();
        loop {
            // §421: an ELIDED slot — `Foo<a,,b>()` — is a missing type with a
            // deferred "Type expected", not a disambiguation failure:
            // upstream's parseDelimitedList reports and the list still
            // succeeds (`callExpressionWithMissingTypeArgument1`). The
            // diagnostic is emitted only once the `>` confirms the list, so
            // the complaint gate below keeps rejecting real less-than chains.
            if self.at(SyntaxKind::CommaToken) {
                missing_slots.push(self.pos());
                let missing = self.missing_identifier();
                let reference = self.finish_node(
                    tsr_ast::TypeReferenceNode::new(
                        Some(tsr_ast::EntityName::Identifier(missing)),
                        &[],
                    ),
                    SyntaxKind::TypeReference,
                    self.pos(),
                );
                arguments.push(TypeNode::TypeReferenceNode(reference));
            } else {
                arguments.push(self.parse_type());
            }
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
        for slot in missing_slots {
            self.error_at(&messages::TYPE_EXPECTED, tsr_core::Span::at(slot));
        }
        self.next_token();
        Some(arguments)
    }

    /// `<A, B>` after a type reference, if present.
    fn parse_type_arguments_of_type_reference(&mut self) -> Vec<TypeNode<'a>> {
        if self.token.has_preceding_line_break() { Vec::new() } else { self.parse_type_arguments() }
    }

    /// `<A, B>` after a type reference, if present.
    pub(crate) fn parse_type_arguments(&mut self) -> Vec<TypeNode<'a>> {
        if self.at(SyntaxKind::LessThanLessThanToken) {
            self.rescan_less_than();
        }
        if !self.at(SyntaxKind::LessThanToken) {
            return Vec::new();
        }
        self.next_token();
        // `parseBracketedList(PCTypeArguments, parseType, <, >)` (`parser.go:3014`).
        let (arguments, _) =
            self.parse_delimited_list(ParsingContext::TypeArguments, Self::parse_type);
        // `List<List<T>>` lexes the close as `>>`; split it.
        if !self.at(SyntaxKind::GreaterThanToken) {
            self.rescan_greater_than();
        }
        self.expect(SyntaxKind::GreaterThanToken);
        arguments
    }

    /// `<T, U extends V>` on a declaration, if present — typescript-go's
    /// `Parser.parseTypeParameters` (`parser.go:3221`): a bracketed
    /// `parseDelimitedList(PCTypeParameters, parseTypeParameter)`, so a stray
    /// token inside the brackets recovers through the list machinery
    /// (`crate::list`) rather than ending the list at the first non-comma.
    pub(crate) fn parse_type_parameters(&mut self) -> Vec<&'a TypeParameterDeclaration<'a>> {
        if self.at(SyntaxKind::LessThanLessThanToken) {
            self.rescan_less_than();
        }
        if !self.at(SyntaxKind::LessThanToken) {
            return Vec::new();
        }
        self.next_token();
        let (parameters, _) =
            self.parse_delimited_list(ParsingContext::TypeParameters, Self::parse_type_parameter);
        if !self.at(SyntaxKind::GreaterThanToken) {
            self.rescan_greater_than();
        }
        self.expect(SyntaxKind::GreaterThanToken);
        parameters
    }

    /// One type parameter — typescript-go's `Parser.parseTypeParameter`
    /// (`parser.go:3228`). Only called where `isListElement(PCTypeParameters)`
    /// holds.
    fn parse_type_parameter(&mut self) -> &'a TypeParameterDeclaration<'a> {
        let start = self.pos();
        let modifiers = self.parse_type_parameter_modifiers();
        let modifiers = self.arena.alloc_slice(&modifiers);
        let name = self.parse_identifier();
        let constraint =
            if self.eat(SyntaxKind::ExtendsKeyword) { Some(self.parse_type()) } else { None };
        let default =
            if self.eat(SyntaxKind::EqualsToken) { Some(self.parse_type()) } else { None };
        self.finish_node(
            TypeParameterDeclaration::new(modifiers, Some(name), constraint, None, default),
            SyntaxKind::TypeParameter,
            start,
        )
    }
}
