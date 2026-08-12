//! Class, interface, type alias, and enum declarations.

use tsr_ast::*;
use tsr_diagnostics::messages;

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

    /// One member of a class body.
    fn parse_class_member(&mut self) -> Option<ClassElement<'a>> {
        let docs = self.parse_leading_jsdoc();
        let member = self.parse_class_member_worker();
        if let Some(member) = member {
            self.attach_jsdoc(member.into(), docs);
        }
        member
    }

    fn parse_class_member_worker(&mut self) -> Option<ClassElement<'a>> {
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

        // `static { … }` is a static initialization block; `static` followed by
        // anything else is a modifier.
        if self.at(SyntaxKind::StaticKeyword) && self.next_is_open_brace() {
            self.next_token();
            // `parseClassStaticBlockBody` (`parser.go:2539`) turns the await
            // context ON unconditionally: `static { await x }` is legal.
            let body = self.with_await_context(true, Self::parse_block);
            return Some(ClassElement::ClassStaticBlockDeclaration(self.finish_node(
                ClassStaticBlockDeclaration::new(&[], Some(body)),
                SyntaxKind::ClassStaticBlockDeclaration,
                start,
            )));
        }

        // A class member is one of the two positions where `const` is a
        // modifier rather than a declaration keyword; see `parse_modifiers_ex`.
        let modifiers = self.parse_modifiers_ex(true);

        // `[key: string]: T` — an index signature on a class.
        if self.at(SyntaxKind::OpenBracketToken) && self.bracket_holds_index_signature() {
            // **§209's second consumer, and it is §209's own lesson.** That
            // section widened `bracket_holds_index_signature` to upstream's
            // nine shapes and rewrote the TYPE-LITERAL consumer to
            // `parseIndexSignatureDeclaration`'s bracketed parameter LIST
            // (`parser.go:3562`). This one — the CLASS member — was left
            // reading a single bare `id: type`, which is the same narrow
            // assumption the predicate used to license, at the other of its
            // two call sites.
            //
            // `class C { [a: number = 1]: number; }` is the witness:
            // `parse_parameter` reads the initializer and upstream records
            // `>1 : 1` for it, where the hand-rolled read stopped at the type
            // annotation and left `= 1` behind to be re-scanned into an
            // expression position. §210.
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
            self.parse_semicolon();
            let modifiers = self.arena.alloc_slice(&modifiers);
            let parameters = self.arena.alloc_slice(&parsed);
            return Some(ClassElement::IndexSignatureDeclaration(self.finish_node(
                IndexSignatureDeclaration::new(modifiers, parameters, value_type, None, &[]),
                SyntaxKind::IndexSignature,
                start,
            )));
        }

        // Ported from `Parser.tryParseConstructorDeclaration` (`parser.go:1917`):
        // the `constructor` keyword commits unconditionally, and a string literal
        // spelling `"constructor"` commits when `(` follows. The signature parses
        // type parameters and a return type — both grammar errors the checker
        // reports, not the parser.
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
                (parameters, return_type, parser.parse_method_body())
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
            // An accessor cannot be `async` either — same reasoning as the
            // constructor above.
            let (parameters, return_type, body) = self.with_await_context(false, |parser| {
                let parameters = parser.parse_parameter_list();
                let return_type = parser.parse_return_type_annotation();
                (parameters, return_type, parser.parse_method_body())
            });
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

        // Ported from `Parser.parsePropertyOrMethodDeclaration` (`parser.go:1938`):
        // the asterisk is parsed here, after the constructor and accessor arms,
        // and its presence alone commits to a method.
        let asterisk =
            if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };

        if asterisk.is_none() && !self.at_property_name_start() {
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

        // A `(` or `<` here makes it a method rather than a property, and an
        // asterisk already has (`parser.go:1944`).
        if asterisk.is_some()
            || self.at(SyntaxKind::OpenParenToken)
            || self.at(SyntaxKind::LessThanToken)
        {
            let type_parameters = self.parse_type_parameters();
            // A method's own await context, from its own `async` — §193.
            let is_async = Self::is_async(&modifiers);
            let (parameters, return_type, body) = self.with_await_context(is_async, |parser| {
                let parameters = parser.parse_parameter_list();
                let return_type = parser.parse_return_type_annotation();
                (parameters, return_type, parser.parse_method_body())
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

    fn next_is_open_brace(&mut self) -> bool {
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

    /// Whether the cursor can begin a property name.
    pub(crate) fn at_property_name_start(&self) -> bool {
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
