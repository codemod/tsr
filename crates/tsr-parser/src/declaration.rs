//! Class, interface, type alias, and enum declarations.

use tsr_ast::*;
use tsr_diagnostics::messages;

use crate::list::ParsingContext;
use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// `class C<T> extends B implements I { … }`.
    /// Whether the `implements` at the cursor opens a heritage clause rather than
    /// naming the class.
    ///
    /// Upstream's `isImplementsClause` (`internal/parser/parser.go:1806`), used by
    /// `parseNameOfClassDeclarationOrExpression` (`:1791`) with the comment that
    /// says why it has to exist: `implements` is a *future reserved* word, so it is
    /// a legal binding identifier outside strict mode, and `class implements … `
    /// is genuinely ambiguous between
    ///
    /// - a class expression with no name, where `implements` starts the heritage
    ///   clause, and
    /// - a class named `implements`.
    ///
    /// Upstream resolves it by looking one token past `implements`: an identifier
    /// or keyword there means a heritage clause. Without this,
    /// `const C = class implements number {}` parsed as a class *named*
    /// `implements` — and two of them in one file were a duplicate identifier,
    /// which is how it was found (6 of the TS2300 over-reports in
    /// `compiler/classImplementsPrimitive` and `conformance/classExtendingPrimitive`).
    fn is_implements_clause(&mut self) -> bool {
        self.at(SyntaxKind::ImplementsKeyword)
            && self.look_ahead(|parser| {
                parser.next_token();
                // Upstream's `tokenIsIdentifierOrKeyword`: every keyword kind sorts
                // after `Identifier` (`internal/parser/utilities.go:20`).
                parser.token.kind >= SyntaxKind::Identifier
            })
    }

    pub(crate) fn parse_class_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        self.expect(SyntaxKind::ClassKeyword);
        // `export default class {}` has no name; `class require {}` has one that
        // happens to be a contextual keyword.
        let name = if (self.at(SyntaxKind::Identifier)
            || crate::statement::is_contextual_keyword(self.token.kind))
            && !self.is_implements_clause()
        {
            Some(self.parse_identifier())
        } else {
            None
        };
        let type_parameters = self.parse_type_parameters();
        let heritage = self.parse_heritage_clauses();

        // `parseClassDeclarationOrExpression`: no members without the `{`.
        let members = if self.expect(SyntaxKind::OpenBraceToken) {
            let members = self.parse_list(ParsingContext::ClassMembers, Self::parse_class_element);
            self.expect(SyntaxKind::CloseBraceToken);
            members
        } else {
            Vec::new()
        };

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
                // `isListElement(PCHeritageClauseElement)` (`parser.go:858`),
                // tested BEFORE each element including the first, exactly as
                // `parseDelimitedList` tests it. Every token that fails this
                // test in the corpus's error-recovery cases also satisfies
                // `isListTerminator(PCHeritageClauseElement)` — `{`, `extends`,
                // `implements` (`:923`) — so breaking here and upstream's
                // three-way decision cannot disagree on them. §194.
                if !self.is_heritage_clause_element() {
                    break;
                }
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

    /// Whether a heritage clause element can start at the cursor.
    ///
    /// `isListElement`'s `PCHeritageClauseElement` arm (`parser.go:858-870`),
    /// with `inErrorRecovery` false — the value `parseDelimitedList` passes.
    ///
    /// Two subtleties, both upstream's and both load-bearing in the corpus:
    ///
    /// - A `{` is an element only when what follows makes it an object literal
    ///   rather than the class body. `class C extends A, {` must stop at the
    ///   `{`, or the class body is consumed as a base expression.
    /// - `extends`/`implements` is not an element even though it is an
    ///   identifier-shaped token, so `class C extends implements A {}` gives an
    ///   `extends` clause with **no** types and a separate `implements` clause,
    ///   which is why upstream records one assertion for it and this port
    ///   recorded three.
    fn is_heritage_clause_element(&mut self) -> bool {
        if self.at(SyntaxKind::OpenBraceToken) {
            return self.is_valid_heritage_clause_object_literal();
        }
        self.is_start_of_left_hand_side_expression()
            && !self.is_heritage_clause_extends_or_implements_keyword()
    }

    /// `isValidHeritageClauseObjectLiteral` (`parser.go:6278`).
    ///
    /// `extends {}` is the base expression only when the `{}` is followed by
    /// something that continues the header — `{`, `,`, `extends`, `implements`.
    /// A non-empty `{` is always an element; only the empty one is ambiguous
    /// with the class body.
    fn is_valid_heritage_clause_object_literal(&mut self) -> bool {
        self.look_ahead(|parser| {
            parser.next_token();
            if !parser.at(SyntaxKind::CloseBraceToken) {
                return true;
            }
            parser.next_token();
            matches!(
                parser.token.kind,
                SyntaxKind::CommaToken
                    | SyntaxKind::OpenBraceToken
                    | SyntaxKind::ExtendsKeyword
                    | SyntaxKind::ImplementsKeyword
            )
        })
    }

    /// `isHeritageClauseExtendsOrImplementsKeyword` (`parser.go:6301`).
    ///
    /// An `extends` or `implements` that is followed by something an expression
    /// could start with — which is what tells `class C extends implements A`'s
    /// `implements` from a class genuinely extending a variable *named*
    /// `implements`.
    fn is_heritage_clause_extends_or_implements_keyword(&mut self) -> bool {
        matches!(self.token.kind, SyntaxKind::ExtendsKeyword | SyntaxKind::ImplementsKeyword)
            && self.look_ahead(|parser| {
                parser.next_token();
                parser.is_start_of_expression()
            })
    }

    /// The expression after `extends` or `implements`.
    ///
    /// `extends` takes a *left-hand-side* expression, not a full one — mixin
    /// factories like `extends Configurable(Base)` and `extends class {}` are
    /// legal, but parsing further would swallow the class body's `{`.
    fn parse_left_hand_side_for_heritage(&mut self) -> Expression<'a> {
        let start = self.pos();
        let mut expression = match self.token.kind {
            // `class A extends class {} {}` — an anonymous class expression.
            SyntaxKind::ClassKeyword => self.parse_class_expression(),
            // `class D extends (await p) {}` — any parenthesised expression.
            SyntaxKind::OpenParenToken => {
                let paren_start = self.pos();
                self.next_token();
                let inner = self.parse_expression();
                self.expect(SyntaxKind::CloseParenToken);
                Expression::ParenthesizedExpression(self.finish_node(
                    ParenthesizedExpression::new(Some(inner)),
                    SyntaxKind::ParenthesizedExpression,
                    paren_start,
                ))
            }
            // The heritage operand is a LeftHandSideExpression, so `extends
            // null` parses — the fallback reads an identifier NAME, and the
            // checker owns any complaint (`classExtendingNull`).
            // **A reserved word that is not a primary cannot open a heritage
            // expression.** `null`, `this`, `super`, `true` and `false` are
            // primaries upstream and keep parsing; `void`, `typeof`, `delete`
            // and the rest reach
            // `parseIdentifierWithDiagnostic(Expression_expected)`
            // (`parser.go:5591`) — the same fallback §574 found for `++`.
            // Contextual keywords are ordinary identifiers and are unaffected.
            // `docs/architecture/checker-notes-diag2.md` §591.
            kind if kind >= SyntaxKind::FIRST_RESERVED_WORD
                && kind <= SyntaxKind::LAST_RESERVED_WORD
                && !matches!(
                    kind,
                    SyntaxKind::NullKeyword
                        | SyntaxKind::ThisKeyword
                        | SyntaxKind::SuperKeyword
                        | SyntaxKind::TrueKeyword
                        | SyntaxKind::FalseKeyword
                        | SyntaxKind::ImportKeyword
                        | SyntaxKind::NewKeyword
                ) =>
            {
                self.error_at_current(&messages::EXPRESSION_EXPECTED);
                Expression::Identifier(self.parse_identifier_name())
            }
            _ => Expression::Identifier(self.parse_identifier_name()),
        };
        loop {
            match self.token.kind {
                SyntaxKind::DotToken => {
                    self.next_token();
                    let name = self.parse_identifier_name();
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
                SyntaxKind::OpenParenToken => {
                    let arguments = self.parse_arguments();
                    let arguments = self.arena.alloc_slice(&arguments);
                    let node = self.finish_node(
                        CallExpression::new(Some(expression), None, &[], arguments),
                        SyntaxKind::CallExpression,
                        start,
                    );
                    expression = Expression::CallExpression(node);
                }
                // `extends Class<A>("A")(…)` — type arguments only continue the
                // chain when a call follows; otherwise they belong to the clause.
                SyntaxKind::LessThanToken => {
                    let Some(type_arguments) = self.try_parse(|p| {
                        let arguments = p.parse_type_arguments_for_call()?;
                        p.at(SyntaxKind::OpenParenToken).then_some(arguments)
                    }) else {
                        break;
                    };
                    let arguments = self.parse_arguments();
                    let arguments = self.arena.alloc_slice(&arguments);
                    let type_arguments = self.arena.alloc_slice(&type_arguments);
                    let node = self.finish_node(
                        CallExpression::new(Some(expression), None, type_arguments, arguments),
                        SyntaxKind::CallExpression,
                        start,
                    );
                    expression = Expression::CallExpression(node);
                }
                _ => break,
            }
        }
        expression
    }

    /// One member of a class body — typescript-go's `Parser.parseClassElement`
    /// (`parser.go`). Only called where `isListElement(PCClassMembers)` holds.
    fn parse_class_element(&mut self) -> ClassElement<'a> {
        let docs = self.parse_leading_jsdoc();
        let member = self.parse_class_element_worker();
        self.attach_jsdoc(member.into(), docs);
        member
    }

    fn parse_class_element_worker(&mut self) -> ClassElement<'a> {
        let start = self.pos();

        if self.at(SyntaxKind::SemicolonToken) {
            self.next_token();
            let node = self.finish_node(
                SemicolonClassElement::new(),
                SyntaxKind::SemicolonClassElement,
                start,
            );
            return ClassElement::SemicolonClassElement(node);
        }

        // A class member is one of the two positions where `const` is a
        // modifier rather than a declaration keyword; see `parse_modifiers_ex`.
        let modifiers = self.parse_modifiers_ex(true, true);

        // `static { … }` is a static initialization block; upstream told
        // `parseModifiersEx` to stop at `static {`, so illegal modifiers before
        // it (`async static { }`) still make a static block. §211.
        if self.at(SyntaxKind::StaticKeyword) && self.next_is_open_brace() {
            self.next_token();
            // `parseClassStaticBlockBody` turns the await context ON
            // unconditionally: `static { await x }` is legal.
            let body = self.with_await_context(true, Self::parse_block);
            let modifiers = self.arena.alloc_slice(&modifiers);
            return ClassElement::ClassStaticBlockDeclaration(self.finish_node(
                ClassStaticBlockDeclaration::new(modifiers, Some(body)),
                SyntaxKind::ClassStaticBlockDeclaration,
                start,
            ));
        }

        if let Some(is_getter) = self.parse_get_or_set_contextual_modifier() {
            return match self.parse_accessor_declaration(start, &modifiers, is_getter, false) {
                AccessorDeclaration::GetAccessorDeclaration(node) => {
                    ClassElement::GetAccessorDeclaration(node)
                }
                AccessorDeclaration::SetAccessorDeclaration(node) => {
                    ClassElement::SetAccessorDeclaration(node)
                }
            };
        }

        // `tryParseConstructorDeclaration`: the `constructor` keyword commits
        // unconditionally, and a string literal spelling `"constructor"`
        // commits when `(` follows. Type parameters and a return type are
        // grammar errors the checker reports, not the parser.
        if self.at(SyntaxKind::ConstructorKeyword)
            || (self.at(SyntaxKind::StringLiteral)
                && self.token_value() == "constructor"
                && self.next_is_open_paren())
        {
            self.next_token();
            let type_parameters = self.parse_type_parameters();
            // A constructor cannot be `async`, so its signature flags carry no
            // `ParseFlagsAwait` and its context is OFF however it is nested.
            let (parameters, return_type, body) = self.with_await_context(false, |parser| {
                let parameters = parser.parse_parameter_list();
                let return_type = parser.parse_return_type_annotation();
                let body =
                    parser.parse_function_block_or_semicolon(false, Some(&messages::OR_EXPECTED));
                (parameters, return_type, body)
            });
            let modifiers = self.arena.alloc_slice(&modifiers);
            let type_parameters = self.arena.alloc_slice(&type_parameters);
            let parameters = self.arena.alloc_slice(&parameters);
            let node = self.finish_node(
                ConstructorDeclaration::new(
                    modifiers,
                    type_parameters,
                    parameters,
                    return_type,
                    None,
                    body,
                    None,
                ),
                SyntaxKind::Constructor,
                start,
            );
            return ClassElement::ConstructorDeclaration(node);
        }

        // `[key: string]: T` — an index signature on a class.
        if self.at(SyntaxKind::OpenBracketToken) && self.bracket_holds_index_signature() {
            return ClassElement::IndexSignatureDeclaration(
                self.parse_index_signature_declaration(start, &modifiers),
            );
        }

        // Checked *after* indexers, because `[` can start an index signature
        // or a computed property name.
        if crate::list::token_is_identifier_or_keyword(self.token.kind)
            || matches!(
                self.token.kind,
                SyntaxKind::StringLiteral
                    | SyntaxKind::NumericLiteral
                    | SyntaxKind::BigIntLiteral
                    | SyntaxKind::AsteriskToken
                    | SyntaxKind::OpenBracketToken
            )
        {
            return self.parse_property_or_method_declaration(start, &modifiers);
        }

        // `isListElement` admitted the token, so modifiers were parsed:
        // treat this as a property declaration with a missing name.
        let at = self.node_end();
        self.error_at(&messages::DECLARATION_EXPECTED, tsr_core::Span::at(at));
        let name = PropertyName::Identifier(self.missing_identifier());
        let modifiers = self.arena.alloc_slice(&modifiers);
        self.parse_property_declaration(start, modifiers, name, None)
    }

    /// `parseContextualModifier(KindGetKeyword)` then `(KindSetKeyword)`
    /// (`parser.go`): consumes `get`/`set` when what follows can name an
    /// accessor, answering whether it was a getter.
    pub(crate) fn parse_get_or_set_contextual_modifier(&mut self) -> Option<bool> {
        if !matches!(self.token.kind, SyntaxKind::GetKeyword | SyntaxKind::SetKeyword) {
            return None;
        }
        let is_getter = self.at(SyntaxKind::GetKeyword);
        self.try_parse(|p| {
            p.next_token();
            // `canFollowGetOrSetKeyword`.
            (p.at(SyntaxKind::OpenBracketToken) || p.is_literal_property_name())
                .then_some(is_getter)
        })
    }

    /// typescript-go's `Parser.parseAccessorDeclaration` (`parser.go`), after
    /// the `get`/`set` keyword. `is_type` is `ParseFlagsType`: an accessor in
    /// an interface or type literal.
    pub(crate) fn parse_accessor_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
        is_getter: bool,
        is_type: bool,
    ) -> AccessorDeclaration<'a> {
        let name = self.parse_property_name();
        let type_parameters = self.parse_type_parameters();
        // An accessor cannot be `async`: its context is OFF.
        let (parameters, return_type, body) = self.with_await_context(false, |parser| {
            let parameters = parser.parse_parameter_list();
            let return_type = parser.parse_return_type_annotation();
            let body = parser.parse_function_block_or_semicolon(is_type, None);
            (parameters, return_type, body)
        });
        let modifiers = self.arena.alloc_slice(modifiers);
        let type_parameters = self.arena.alloc_slice(&type_parameters);
        let parameters = self.arena.alloc_slice(&parameters);
        if is_getter {
            AccessorDeclaration::GetAccessorDeclaration(self.finish_node(
                GetAccessorDeclaration::new(
                    modifiers,
                    name,
                    type_parameters,
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
            AccessorDeclaration::SetAccessorDeclaration(self.finish_node(
                SetAccessorDeclaration::new(
                    modifiers,
                    name,
                    type_parameters,
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
        }
    }

    /// typescript-go's `Parser.parseIndexSignatureDeclaration` (`parser.go`).
    pub(crate) fn parse_index_signature_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> &'a IndexSignatureDeclaration<'a> {
        // `parseBracketedList(PCParameters, parseParameter, [, ])`. §209/§210:
        // a list of zero or more ordinary parameters, each able to carry
        // modifiers and an initializer.
        let parsed = if self.expect(SyntaxKind::OpenBracketToken) {
            let (parsed, _) =
                self.parse_delimited_list(ParsingContext::Parameters, Self::parse_parameter);
            self.expect(SyntaxKind::CloseBracketToken);
            parsed
        } else {
            Vec::new()
        };
        let value_type = self.parse_type_annotation();
        self.parse_type_member_semicolon();
        let modifiers = self.arena.alloc_slice(modifiers);
        let parameters = self.arena.alloc_slice(&parsed);
        self.finish_node(
            IndexSignatureDeclaration::new(modifiers, parameters, value_type, None, &[]),
            SyntaxKind::IndexSignature,
            start,
        )
    }

    /// typescript-go's `Parser.parsePropertyOrMethodDeclaration` (`parser.go`).
    fn parse_property_or_method_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> ClassElement<'a> {
        let asterisk =
            if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };
        let name = self.parse_property_name();
        // Note: this is not legal as per the grammar. But we allow it in the
        // parser and report an error in the grammar checker.
        let question =
            if self.at(SyntaxKind::QuestionToken) { Some(self.take_token()) } else { None };
        let modifiers_slice = self.arena.alloc_slice(modifiers);
        if asterisk.is_some()
            || self.at(SyntaxKind::OpenParenToken)
            || self.at(SyntaxKind::LessThanToken)
        {
            // `parseMethodDeclaration`.
            let type_parameters = self.parse_type_parameters();
            // A method's own await context, from its own `async` — §193.
            let is_async = Self::is_async(modifiers);
            let (parameters, return_type, body) = self.with_await_context(is_async, |parser| {
                let parameters = parser.parse_parameter_list();
                let return_type = parser.parse_return_type_annotation();
                let body =
                    parser.parse_function_block_or_semicolon(false, Some(&messages::OR_EXPECTED));
                (parameters, return_type, body)
            });
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
            return ClassElement::MethodDeclaration(node);
        }
        self.parse_property_declaration(start, modifiers_slice, name, question)
    }

    /// typescript-go's `Parser.parsePropertyDeclaration` (`parser.go`).
    fn parse_property_declaration(
        &mut self,
        start: u32,
        modifiers: &'a [ModifierLike<'a>],
        name: PropertyName<'a>,
        question: Option<&'a tsr_ast::Token<'a>>,
    ) -> ClassElement<'a> {
        let postfix = question.or_else(|| {
            (self.at(SyntaxKind::ExclamationToken) && !self.token.has_preceding_line_break())
                .then(|| self.take_token())
        });
        let type_node = self.parse_type_annotation();
        let initializer = if self.eat(SyntaxKind::EqualsToken) {
            Some(self.parse_assignment_expression())
        } else {
            None
        };
        self.parse_semicolon_after_property_name(name, type_node.is_some(), initializer.is_some());
        let node = self.finish_node(
            PropertyDeclaration::new(modifiers, name, postfix, type_node, initializer),
            SyntaxKind::PropertyDeclaration,
            start,
        );
        ClassElement::PropertyDeclaration(node)
    }

    /// typescript-go's `Parser.parseSemicolonAfterPropertyName` (`parser.go`).
    fn parse_semicolon_after_property_name(
        &mut self,
        name: PropertyName<'a>,
        has_type: bool,
        has_initializer: bool,
    ) {
        if self.at(SyntaxKind::AtToken) && !self.token.has_preceding_line_break() {
            self.error_at_current(
                &messages::DECORATORS_MUST_PRECEDE_THE_NAME_AND_ALL_KEYWORDS_OF_PROPERTY_DECLARATIONS,
            );
            return;
        }
        if self.at(SyntaxKind::OpenParenToken) {
            self.error_at_current(&messages::CANNOT_START_A_FUNCTION_CALL_IN_A_TYPE_ANNOTATION);
            self.next_token();
            return;
        }
        if has_type && !self.can_parse_semicolon() {
            if has_initializer {
                self.error_at_current_with(&messages::_0_EXPECTED, &[";"]);
            } else {
                self.error_at_current(&messages::EXPECTED_FOR_PROPERTY_INITIALIZER);
            }
            return;
        }
        if self.try_parse_semicolon() {
            return;
        }
        if has_initializer {
            self.error_at_current_with(&messages::_0_EXPECTED, &[";"]);
            return;
        }
        let (text, span) = match name {
            PropertyName::Identifier(identifier) => (
                identifier.text,
                identifier.node_id.map_or(self.token.span, |id| self.nodes.span(id)),
            ),
            _ => ("", self.token.span),
        };
        self.parse_error_for_missing_semicolon_after_name(text, span);
    }

    /// typescript-go's `Parser.parseFunctionBlockOrSemicolon` (`parser.go`):
    /// a body, or `None` for an overload signature. `is_type` is
    /// `ParseFlagsType`; `missing_open_brace` replaces `'{' expected`.
    ///
    /// Without its `{`, upstream's `parseBlock` builds an empty block that
    /// covers no text, and `ast.NodeIsMissing` makes every consumer treat the
    /// function as bodiless (`f(), f()` is two overloads returning `any`).
    /// This port has no zero-width node, so that body is `None`.
    pub(crate) fn parse_function_block_or_semicolon(
        &mut self,
        is_type: bool,
        missing_open_brace: Option<&'static tsr_diagnostics::Message>,
    ) -> Option<FunctionBody<'a>> {
        if !self.at(SyntaxKind::OpenBraceToken) {
            if is_type {
                self.parse_type_member_semicolon();
                return None;
            }
            if self.can_parse_semicolon() {
                self.parse_semicolon();
                return None;
            }
            match missing_open_brace {
                Some(message) => self.error_at_current(message),
                None => {
                    self.expect(SyntaxKind::OpenBraceToken);
                }
            }
            return None;
        }
        Some(FunctionBody::Block(self.parse_block()))
    }

    /// typescript-go's `Parser.parseTypeMemberSemicolon` (`parser.go`): type
    /// members are separated by commas or (possibly ASI) semicolons.
    pub(crate) fn parse_type_member_semicolon(&mut self) {
        if self.eat(SyntaxKind::CommaToken) {
            return;
        }
        self.parse_semicolon();
    }

    pub(crate) fn next_is_open_brace(&mut self) -> bool {
        self.peek_kind(|kind| kind == SyntaxKind::OpenBraceToken)
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

        let members = self.parse_object_type_members();

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
        let members = if self.expect(SyntaxKind::OpenBraceToken) {
            // Enum members are in neither a yield nor an await context.
            let (members, _) = self.with_await_context(false, |parser| {
                parser.parse_delimited_list(ParsingContext::EnumMembers, Self::parse_enum_member)
            });
            self.expect(SyntaxKind::CloseBraceToken);
            members
        } else {
            Vec::new()
        };

        let modifiers = self.arena.alloc_slice(modifiers);
        let members = self.arena.alloc_slice(&members);
        let node = self.finish_node(
            EnumDeclaration::new(modifiers, Some(name), members),
            SyntaxKind::EnumDeclaration,
            start,
        );
        Statement::EnumDeclaration(node)
    }

    /// typescript-go's `Parser.parseEnumMember` (`parser.go`).
    fn parse_enum_member(&mut self) -> &'a EnumMember<'a> {
        let member_start = self.pos();
        let member_name = self.parse_property_name();
        let initializer = if self.eat(SyntaxKind::EqualsToken) {
            Some(self.parse_assignment_expression())
        } else {
            None
        };
        self.finish_node(
            EnumMember::new(member_name, initializer, &[], None),
            SyntaxKind::EnumMember,
            member_start,
        )
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
