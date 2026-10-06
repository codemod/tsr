//! Import, export, and namespace declarations.

use tsr_ast::*;
use tsr_diagnostics::messages;

use crate::list::ParsingContext;
use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// `import …` in all its forms.
    pub(crate) fn parse_import_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        self.expect(SyntaxKind::ImportKeyword);
        let modifiers = self.arena.alloc_slice(modifiers);

        // `import "module";` — a side-effect import with no bindings.
        if self.at(SyntaxKind::StringLiteral) {
            let specifier = self.parse_module_specifier();
            let attributes = self.parse_import_attributes();
            self.parse_semicolon();
            let node = self.finish_node(
                ImportDeclaration::new(modifiers, None, Some(specifier), attributes),
                SyntaxKind::ImportDeclaration,
                start,
            );
            return Statement::ImportDeclaration(node);
        }

        // `import type …` / `import defer …` — but both words may also name the
        // default binding, so the following tokens decide. In particular,
        // `import type from from "m"` is a type-only import whose binding is
        // named `from`, while `import type from "m"` imports a binding named
        // `type`.
        let has_phase = (self.at(SyntaxKind::TypeKeyword) && self.type_is_modifier_here())
            || (self.at(SyntaxKind::DeferKeyword) && self.defer_is_modifier_here());
        let phase = if has_phase { Some(self.take_token()) } else { None };
        let is_type_only = phase.is_some_and(|phase| phase.kind == SyntaxKind::TypeKeyword);

        // `import x = require("m")` and `import x = A.B`. Upstream decides on
        // the token *after* the identifier: anything but `,` or `from`
        // (`tokenAfterImportedIdentifierDefinitelyProducesImportDeclaration`)
        // makes an import-equals declaration, which then expects its `=`
        // (`parseImportDeclarationOrImportEqualsDeclaration`, `parser.go:2263`;
        // not after `defer`). `import abstract class D {}` is TS1005 `'='
        // expected` there, with an entity-name reference, not an import clause
        // whose specifier is a class expression.
        let is_defer = phase.is_some_and(|phase| phase.kind == SyntaxKind::DeferKeyword);
        if self.at_binding_identifier()
            && !is_defer
            && self
                .peek_kind(|kind| !matches!(kind, SyntaxKind::CommaToken | SyntaxKind::FromKeyword))
        {
            let name = self.parse_identifier();
            self.expect(SyntaxKind::EqualsToken);
            let reference = self.parse_module_reference();
            self.parse_semicolon();
            let node = self.finish_node(
                ImportEqualsDeclaration::new(modifiers, is_type_only, Some(name), Some(reference)),
                SyntaxKind::ImportEqualsDeclaration,
                start,
            );
            return Statement::ImportEqualsDeclaration(node);
        }

        let clause_start = self.pos();
        // Contextual keywords are legal binding names: `import type from "m"`
        // imports something called `type`.
        let default_name =
            if self.at_binding_identifier() { Some(self.parse_identifier()) } else { None };
        // A default import may be followed by named or namespace bindings.
        let named_bindings = if default_name.is_none() || self.eat(SyntaxKind::CommaToken) {
            self.parse_named_import_bindings()
        } else {
            None
        };
        let clause = self.finish_node(
            ImportClause::new(phase, default_name, named_bindings),
            SyntaxKind::ImportClause,
            clause_start,
        );

        self.expect(SyntaxKind::FromKeyword);
        let specifier = self.parse_module_specifier();
        let attributes = self.parse_import_attributes();
        self.parse_semicolon();

        let node = self.finish_node(
            ImportDeclaration::new(modifiers, Some(clause), Some(specifier), attributes),
            SyntaxKind::ImportDeclaration,
            start,
        );
        Statement::ImportDeclaration(node)
    }

    /// `* as ns` or `{ a, b as c }`.
    pub(crate) fn parse_named_import_bindings(&mut self) -> Option<NamedImportBindings<'a>> {
        let start = self.pos();
        if self.at(SyntaxKind::AsteriskToken) {
            self.next_token();
            self.expect(SyntaxKind::AsKeyword);
            let name = self.parse_identifier();
            let node = self.finish_node(
                NamespaceImport::new(Some(name)),
                SyntaxKind::NamespaceImport,
                start,
            );
            return Some(NamedImportBindings::NamespaceImport(node));
        }
        if !self.at(SyntaxKind::OpenBraceToken) {
            return None;
        }

        self.next_token();
        // `parseNamedImportsOrExports`: `parseBracketedList(
        // PCImportOrExportSpecifiers, parseImportSpecifier, {, })`.
        let (elements, _) = self.parse_delimited_list(
            ParsingContext::ImportOrExportSpecifiers,
            Self::parse_import_specifier,
        );
        self.expect(SyntaxKind::CloseBraceToken);
        let elements = self.arena.alloc_slice(&elements);
        let node = self.finish_node(NamedImports::new(elements), SyntaxKind::NamedImports, start);
        Some(NamedImportBindings::NamedImports(node))
    }

    /// typescript-go's `Parser.parseImportSpecifier` (`parser.go`).
    fn parse_import_specifier(&mut self) -> &'a ImportSpecifier<'a> {
        let start = self.pos();
        let (type_only, property_name, name) = self.parse_import_or_export_specifier(true);
        let name = match name {
            ModuleExportName::Identifier(id) => id,
            // A string export name must be renamed to be importable.
            ModuleExportName::StringLiteral(literal) => {
                let span = literal.node_id.map_or(self.token.span, |id| self.nodes.span(id));
                self.error_at(&messages::IDENTIFIER_EXPECTED, span);
                self.missing_identifier()
            }
        };
        self.finish_node(
            ImportSpecifier::new(type_only, property_name, Some(name)),
            SyntaxKind::ImportSpecifier,
            start,
        )
    }

    /// typescript-go's `Parser.parseExportSpecifier` (`parser.go`).
    fn parse_export_specifier(&mut self) -> &'a ExportSpecifier<'a> {
        let start = self.pos();
        let (type_only, property_name, name) = self.parse_import_or_export_specifier(false);
        self.finish_node(
            ExportSpecifier::new(type_only, property_name, Some(name)),
            SyntaxKind::ExportSpecifier,
            start,
        )
    }

    /// typescript-go's `Parser.parseImportOrExportSpecifier` (`parser.go`):
    /// `(isTypeOnly, propertyName, name)`. A keyword that is no identifier is
    /// consumed as a name in an import specifier and reported (TS1003), and
    /// a leading `type` is disambiguated by the `as` tokens that follow.
    fn parse_import_or_export_specifier(
        &mut self,
        is_import: bool,
    ) -> (bool, Option<ModuleExportName<'a>>, ModuleExportName<'a>) {
        let mut can_parse_as_keyword = true;
        let mut type_only = false;
        let mut property_name = None;
        let (mut name, mut name_ok) = self.parse_module_export_name_checked(is_import);
        if matches!(name, ModuleExportName::Identifier(id) if id.text == "type") {
            if self.at(SyntaxKind::AsKeyword) {
                // { type as ...? }
                let first_as = self.parse_identifier_name();
                if self.at(SyntaxKind::AsKeyword) {
                    // { type as as ...? }
                    let second_as = self.parse_identifier_name();
                    if self.can_parse_module_export_name() {
                        // { type as as something }
                        type_only = true;
                        property_name = Some(ModuleExportName::Identifier(first_as));
                        (name, name_ok) = self.parse_module_export_name_checked(is_import);
                    } else {
                        // { type as as }
                        property_name = Some(name);
                        name = ModuleExportName::Identifier(second_as);
                    }
                    can_parse_as_keyword = false;
                } else if self.can_parse_module_export_name() {
                    // { type as something }
                    property_name = Some(name);
                    can_parse_as_keyword = false;
                    (name, name_ok) = self.parse_module_export_name_checked(is_import);
                } else {
                    // { type as }
                    type_only = true;
                    name = ModuleExportName::Identifier(first_as);
                }
            } else if self.can_parse_module_export_name() {
                // { type something ...? }
                type_only = true;
                (name, name_ok) = self.parse_module_export_name_checked(is_import);
            }
        }
        if can_parse_as_keyword && self.at(SyntaxKind::AsKeyword) {
            property_name = Some(name);
            self.expect(SyntaxKind::AsKeyword);
            (name, name_ok) = self.parse_module_export_name_checked(is_import);
        }
        if !name_ok {
            let span = name.node_id().map_or(self.token.span, |id| self.nodes.span(id));
            self.error_at(&messages::IDENTIFIER_EXPECTED, span);
        }
        (type_only, property_name, name)
    }

    /// typescript-go's `Parser.canParseModuleExportName` (`parser.go`).
    fn can_parse_module_export_name(&self) -> bool {
        crate::list::token_is_identifier_or_keyword(self.token.kind)
            || self.at(SyntaxKind::StringLiteral)
    }

    /// typescript-go's `Parser.parseModuleExportName(disallowKeywords)`
    /// (`parser.go`): the name, and whether it is acceptable.
    fn parse_module_export_name_checked(
        &mut self,
        disallow_keywords: bool,
    ) -> (ModuleExportName<'a>, bool) {
        let name_ok = !disallow_keywords
            || self.at(SyntaxKind::StringLiteral)
            || !self.token.kind.is_keyword()
            || self.is_identifier();
        (self.parse_module_export_name(), name_ok)
    }

    /// `require("m")` or a dotted entity name.
    fn parse_module_reference(&mut self) -> ModuleReference<'a> {
        let start = self.pos();
        // `parseModuleReference` (`parser.go:2303`): `require` names an
        // external module only when `(` follows.
        if self.at(SyntaxKind::RequireKeyword)
            && self.peek_kind(|kind| kind == SyntaxKind::OpenParenToken)
        {
            self.next_token();
            self.expect(SyntaxKind::OpenParenToken);
            let specifier = self.parse_module_specifier();
            self.expect(SyntaxKind::CloseParenToken);
            let node = self.finish_node(
                ExternalModuleReference::new(Some(specifier)),
                SyntaxKind::ExternalModuleReference,
                start,
            );
            return ModuleReference::ExternalModuleReference(node);
        }
        // `parseEntityName(allowReservedWords: false)`: a reserved word is not
        // a name here (`import x = class` reports at `class` and leaves the
        // class to the next statement), unlike a type reference's entity name.
        let mut name = tsr_ast::EntityName::Identifier(self.parse_identifier());
        while self.eat(SyntaxKind::DotToken) {
            if self.at(SyntaxKind::LessThanToken) {
                break;
            }
            let right = self.parse_right_side_of_dot_identifier();
            let node = self.finish_node(
                tsr_ast::QualifiedName::new(Some(name), Some(right)),
                SyntaxKind::QualifiedName,
                start,
            );
            name = tsr_ast::EntityName::QualifiedName(node);
        }
        ModuleReference::from(name)
    }

    /// `export …` in all its forms.
    /// Everything after a leading `export`.
    ///
    /// `export_token` is that keyword. It becomes a modifier on a *declaration*
    /// — `export const x = 1` is a `VariableStatement` modified by `export` —
    /// but not on an `ExportDeclaration` or `ExportAssignment`, where the keyword
    /// is part of the node's own syntax rather than a modifier of it. That is
    /// upstream's split too.
    ///
    /// **The modifiers are the caller's**, not an empty slice: `declare export =
    /// value` is the recovery shape `statement.rs`'s comment already names, and
    /// discarding them here lost TS1120 — *an export assignment cannot have
    /// modifiers* — which has nothing to report without them. For a plain
    /// `export = x` the slice is empty and the node is unchanged. §511.
    pub(crate) fn parse_export(
        &mut self,
        start: u32,
        export_token: &'a Token<'a>,
        modifiers_slice: &'a [ModifierLike<'a>],
    ) -> Statement<'a> {
        // `export as namespace N;` declares a UMD global — the name the module
        // takes when it is loaded as a script rather than imported. It is not an
        // export assignment: `export default N` and `export as namespace N` mean
        // different things, and recording both as `ExportAssignment` made the
        // binder file the UMD name under `default`.
        if self.at(SyntaxKind::AsKeyword) {
            self.next_token();
            self.expect(SyntaxKind::NamespaceKeyword);
            let name = self.parse_identifier();
            self.parse_semicolon();
            let node = self.finish_node(
                NamespaceExportDeclaration::new(modifiers_slice, Some(name)),
                SyntaxKind::NamespaceExportDeclaration,
                start,
            );
            return Statement::NamespaceExportDeclaration(node);
        }

        // `export = expr;`
        if self.at(SyntaxKind::EqualsToken) {
            self.next_token();
            let expression = self.parse_assignment_expression();
            self.parse_semicolon();
            let node = self.finish_node(
                ExportAssignment::new(modifiers_slice, true, None, Some(expression)),
                SyntaxKind::ExportAssignment,
                start,
            );
            return Statement::ExportAssignment(node);
        }

        // `export default <expression>` — `parseExportAssignment`
        // (`parser.go`). `export default class`/`function`/`interface` never
        // reach here: there `export` and `default` are modifiers
        // (`nextTokenCanFollowDefaultKeyword`) and the declaration is parsed
        // by `parse_declaration_after_modifiers`.
        if self.at(SyntaxKind::DefaultKeyword) {
            self.next_token();
            let expression = self.parse_assignment_expression();
            self.parse_semicolon();
            let node = self.finish_node(
                ExportAssignment::new(modifiers_slice, false, None, Some(expression)),
                SyntaxKind::ExportAssignment,
                start,
            );
            return Statement::ExportAssignment(node);
        }

        // `export type { A }` and `export type * from "m"` are type-only
        // re-exports; `export type A = …` is a type alias declaration, and
        // consuming `type` here would leave it looking like an expression.
        let is_type_only = self.at(SyntaxKind::TypeKeyword) && self.next_starts_export_clause();
        if is_type_only {
            self.next_token();
        }

        // `export * from "m"` and `export * as ns from "m"`.
        if self.at(SyntaxKind::AsteriskToken) {
            let clause_start = self.pos();
            self.next_token();
            let clause = if self.eat(SyntaxKind::AsKeyword) {
                let name = self.parse_module_export_name();
                let node = self.finish_node(
                    NamespaceExport::new(Some(name)),
                    SyntaxKind::NamespaceExport,
                    clause_start,
                );
                Some(NamedExportBindings::NamespaceExport(node))
            } else {
                None
            };
            self.expect(SyntaxKind::FromKeyword);
            let specifier = self.parse_module_specifier();
            let attributes = self.parse_import_attributes();
            self.parse_semicolon();
            let node = self.finish_node(
                ExportDeclaration::new(
                    modifiers_slice,
                    is_type_only,
                    clause,
                    Some(specifier),
                    attributes,
                ),
                SyntaxKind::ExportDeclaration,
                start,
            );
            return Statement::ExportDeclaration(node);
        }

        // `export { a, b as c } [from "m"]`.
        if self.at(SyntaxKind::OpenBraceToken) {
            let clause_start = self.pos();
            self.next_token();
            let (elements, _) = self.parse_delimited_list(
                ParsingContext::ImportOrExportSpecifiers,
                Self::parse_export_specifier,
            );
            self.expect(SyntaxKind::CloseBraceToken);
            let elements = self.arena.alloc_slice(&elements);
            let named = self.finish_node(
                NamedExports::new(elements),
                SyntaxKind::NamedExports,
                clause_start,
            );
            let specifier = if self.eat(SyntaxKind::FromKeyword) {
                Some(self.parse_module_specifier())
            } else {
                None
            };
            let attributes = self.parse_import_attributes();
            self.parse_semicolon();
            let node = self.finish_node(
                ExportDeclaration::new(
                    modifiers_slice,
                    is_type_only,
                    Some(NamedExportBindings::NamedExports(named)),
                    specifier,
                    attributes,
                ),
                SyntaxKind::ExportDeclaration,
                start,
            );
            return Statement::ExportDeclaration(node);
        }

        // Anything else is a modifier on a declaration: `export const x = 1`.
        let mut all = vec![ModifierLike::Token(export_token)];
        all.extend(self.parse_modifiers());
        let all = self.arena.alloc_slice(&all);
        self.parse_declaration_after_modifiers(start, all)
    }

    /// `namespace N { … }` and `module "m" { … }`.
    pub(crate) fn parse_module_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        // `declare global { … }` has no separate name: `global` is both —
        // `parseAmbientExternalModuleDeclaration`'s global arm (`parser.go`).
        if self.at(SyntaxKind::GlobalKeyword) {
            let keyword_start = self.pos();
            let keyword = self.take_token();
            let name = ModuleName::Identifier(self.finish_node(
                Identifier::new("global"),
                SyntaxKind::Identifier,
                keyword_start,
            ));
            let body = self.parse_module_body();
            let modifiers = self.arena.alloc_slice(modifiers);
            return Statement::ModuleDeclaration(self.finish_node(
                ModuleDeclaration::new(modifiers, keyword, Some(name), body, None),
                SyntaxKind::ModuleDeclaration,
                start,
            ));
        }

        let keyword = self.take_token();
        if self.at(SyntaxKind::StringLiteral) {
            let literal_start = self.pos();
            let text = self.token_value();
            let flags = self.token.ast_flags();
            self.next_token();
            let name = ModuleName::StringLiteral(self.finish_node(
                StringLiteral::new(text, flags),
                SyntaxKind::StringLiteral,
                literal_start,
            ));
            let body = self.parse_module_body();
            let modifiers = self.arena.alloc_slice(modifiers);
            return Statement::ModuleDeclaration(self.finish_node(
                ModuleDeclaration::new(modifiers, keyword, Some(name), body, None),
                SyntaxKind::ModuleDeclaration,
                start,
            ));
        }

        let kind = keyword.kind;
        Statement::ModuleDeclaration(
            self.parse_dotted_module_declaration(start, modifiers, kind, keyword),
        )
    }

    /// One segment of a possibly-dotted namespace name, and everything under it.
    ///
    /// `namespace A.B { … }` means `namespace A { export namespace B { … } }`, so
    /// upstream desugars a dotted name into one `ModuleDeclaration` per segment
    /// rather than keeping the dots in a single node. Doing anything else loses
    /// the nesting the binder needs: without it, neither `A` nor `A.B` gets a
    /// symbol in the right table.
    ///
    /// Each segment after the first carries a **synthesised** `export` modifier —
    /// upstream's `implicitExport` — because the inner namespace has to be
    /// reachable through the outer one. Both it and the segment's `namespace`
    /// keyword are zero-width: the source spells them once, and the node shape
    /// wants one per level.
    fn parse_dotted_module_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
        keyword_kind: SyntaxKind,
        keyword: &'a tsr_ast::Token<'a>,
    ) -> &'a ModuleDeclaration<'a> {
        let name = ModuleName::Identifier(self.parse_identifier());
        let body = if self.eat(SyntaxKind::DotToken) {
            let nested_start = self.pos();
            let empty = tsr_core::Span::new(nested_start, nested_start);
            let nested_keyword = self.alloc_token(keyword_kind, empty);
            let export = self.alloc_token(SyntaxKind::ExportKeyword, empty);
            let inner = self.parse_dotted_module_declaration(
                nested_start,
                &[ModifierLike::Token(export)],
                keyword_kind,
                nested_keyword,
            );
            Some(ModuleBody::ModuleDeclaration(inner))
        } else {
            self.parse_module_body()
        };
        let modifiers = self.arena.alloc_slice(modifiers);
        self.finish_node(
            ModuleDeclaration::new(modifiers, keyword, Some(name), body, None),
            SyntaxKind::ModuleDeclaration,
            start,
        )
    }

    /// `{ … }` after a namespace name, or nothing for a bare declaration.
    fn parse_module_body(&mut self) -> Option<ModuleBody<'a>> {
        if !self.at(SyntaxKind::OpenBraceToken) {
            self.parse_semicolon();
            return None;
        }
        let block_start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);
        let statements = self.parse_statement_list(crate::list::ParsingContext::BlockStatements);
        self.expect(SyntaxKind::CloseBraceToken);
        let statements = self.arena.alloc_slice(&statements);
        Some(ModuleBody::ModuleBlock(self.finish_node(
            ModuleBlock::new(statements),
            SyntaxKind::ModuleBlock,
            block_start,
        )))
    }

    /// `with { type: "json" }` — import attributes, if present.
    ///
    /// Also accepts the older `assert` spelling, which TypeScript still parses.
    pub(crate) fn parse_import_attributes(&mut self) -> Option<&'a ImportAttributes<'a>> {
        if !self.at(SyntaxKind::WithKeyword) && !self.at(SyntaxKind::AssertKeyword) {
            return None;
        }
        let start = self.pos();
        let token = self.take_token();
        Some(self.parse_import_attributes_body(start, token))
    }

    /// `, { with: { "resolution-mode": "import" } }` in an import type.
    ///
    /// Ported from the import-type branch at `internal/parser/parser.go:3047`
    /// and `parseImportAttributes` at `parser.go:3085` in the pinned
    /// typescript-go source.
    pub(crate) fn parse_import_type_attributes(&mut self) -> Option<&'a ImportAttributes<'a>> {
        if !self.eat(SyntaxKind::CommaToken) {
            return None;
        }
        let start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);
        if !self.at(SyntaxKind::WithKeyword) && !self.at(SyntaxKind::AssertKeyword) {
            return None;
        }
        let token = self.take_token();
        self.expect(SyntaxKind::ColonToken);
        let attributes = self.parse_import_attributes_body(start, token);
        self.eat(SyntaxKind::CommaToken);
        self.expect(SyntaxKind::CloseBraceToken);
        Some(attributes)
    }

    fn parse_import_attributes_body(
        &mut self,
        start: u32,
        token: &'a Token<'a>,
    ) -> &'a ImportAttributes<'a> {
        self.expect(SyntaxKind::OpenBraceToken);

        // `parseDelimitedList(PCImportAttributes, parseImportAttribute)`.
        let (elements, _) = self
            .parse_delimited_list(ParsingContext::ImportAttributes, Self::parse_import_attribute);
        self.expect(SyntaxKind::CloseBraceToken);

        let elements = self.arena.alloc_slice(&elements);
        self.finish_node(
            ImportAttributes::new(token, elements, false),
            SyntaxKind::ImportAttributes,
            start,
        )
    }

    /// `type: "json"` — typescript-go's `Parser.parseImportAttribute`
    /// (`parser.go`). A key is an identifier or a string, not the full
    /// property-name grammar.
    fn parse_import_attribute(&mut self) -> &'a ImportAttribute<'a> {
        let element_start = self.pos();
        let name = if self.at(SyntaxKind::StringLiteral) {
            let literal_start = self.pos();
            let text = self.token_value();
            let flags = self.token.ast_flags();
            self.next_token();
            ImportAttributeName::StringLiteral(self.finish_node(
                StringLiteral::new(text, flags),
                SyntaxKind::StringLiteral,
                literal_start,
            ))
        } else {
            ImportAttributeName::Identifier(self.parse_identifier_name())
        };
        self.expect(SyntaxKind::ColonToken);
        let value = self.parse_assignment_expression();
        self.finish_node(
            ImportAttribute::new(Some(name), Some(value)),
            SyntaxKind::ImportAttribute,
            element_start,
        )
    }

    /// An import/export name, which may be a string: `export { a as "b" }`.
    fn parse_module_export_name(&mut self) -> ModuleExportName<'a> {
        if self.at(SyntaxKind::StringLiteral) {
            let start = self.pos();
            let text = self.token_value();
            let flags = self.token.ast_flags();
            self.next_token();
            return ModuleExportName::StringLiteral(self.finish_node(
                StringLiteral::new(text, flags),
                SyntaxKind::StringLiteral,
                start,
            ));
        }
        ModuleExportName::Identifier(self.parse_identifier_name())
    }

    /// The `"module"` in `from "module"`.
    pub(crate) fn parse_module_specifier(&mut self) -> Expression<'a> {
        let start = self.pos();
        if !self.at(SyntaxKind::StringLiteral) {
            // `parseModuleSpecifier` (`parser.go`) **parses an arbitrary
            // expression** here — its own comment says *"we allow arbitrary
            // expressions here, even though the grammar only allows string
            // literals; we check to ensure that it is only a string literal
            // later in the grammar check pass"*. So `import foo = require(x)`
            // consumes `x`, finds the `)`, and yields exactly one diagnostic.
            //
            // This port reported at the same position — which is right — and
            // then returned a **missing** identifier without consuming, so the
            // `)` was reported missing too. `importNonStringLiteral` is one
            // TS1141 upstream and was TS1141 plus a TS1005 here.
            //
            // The report stays in the parser rather than moving to a grammar
            // pass: it lands at upstream's own position, and moving it would
            // trade a bounded fix for an unported check. §217.
            //
            // §279: but ONLY when an expression can actually start here.
            // `import` on its own line before another import statement made
            // this arm call `parse_expression` on the SECOND `import`, which
            // swallowed that whole declaration into the first one's specifier
            // (`importCallExpressionIncorrect1/2`). Upstream's expression
            // parse mints a missing identifier — one TS1109 at the token,
            // NOTHING consumed — and the next statement parses intact, which
            // is exactly what its baseline records.
            if !self.is_start_of_expression() {
                self.error_at_current(&messages::EXPRESSION_EXPECTED);
                return Expression::Identifier(self.missing_identifier());
            }
            self.error_at_current(&messages::STRING_LITERAL_EXPECTED);
            return self.parse_expression();
        }
        let text = self.token_value();
        let flags = self.token.ast_flags();
        self.next_token();
        Expression::StringLiteral(self.finish_node(
            StringLiteral::new(text, flags),
            SyntaxKind::StringLiteral,
            start,
        ))
    }

    /// Whether `type` here is the type-only modifier rather than a name.
    ///
    /// `import type { A } from "m"` is type-only; `import type from "m"` imports
    /// a binding called `type`. The distinguishing token is what follows.
    fn type_is_modifier_here(&mut self) -> bool {
        self.look_ahead(|parser| {
            parser.next_token();
            if parser.at(SyntaxKind::FromKeyword) {
                parser.next_token();
                return matches!(
                    parser.token.kind,
                    SyntaxKind::FromKeyword | SyntaxKind::EqualsToken
                );
            }
            !matches!(
                parser.token.kind,
                SyntaxKind::CommaToken
                    | SyntaxKind::EqualsToken
                    | SyntaxKind::CloseBraceToken
                    | SyntaxKind::AsKeyword
            )
        })
    }

    /// Whether `defer` is an import phase rather than the default binding name.
    fn defer_is_modifier_here(&mut self) -> bool {
        self.look_ahead(|parser| {
            parser.next_token();
            if parser.at(SyntaxKind::FromKeyword) {
                parser.next_token();
                return !parser.at(SyntaxKind::StringLiteral);
            }
            !matches!(parser.token.kind, SyntaxKind::CommaToken | SyntaxKind::EqualsToken)
        })
    }

    /// Whether the cursor is on a name usable as a binding.
    ///
    /// Contextual keywords qualify; reserved words do not.
    pub(crate) fn at_binding_identifier(&self) -> bool {
        self.at(SyntaxKind::Identifier) || crate::statement::is_contextual_keyword(self.token.kind)
    }

    /// Whether `type` here modifies an export clause rather than naming an alias.
    fn next_starts_export_clause(&mut self) -> bool {
        self.peek_kind(|kind| {
            matches!(kind, SyntaxKind::OpenBraceToken | SyntaxKind::AsteriskToken)
        })
    }
}
