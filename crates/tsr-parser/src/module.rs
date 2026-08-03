//! Import, export, and namespace declarations.

use tsr_ast::*;
use tsr_diagnostics::messages;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Whether `import` begins a declaration rather than `import(…)` or
    /// `import.meta`, both of which are expressions.
    pub(crate) fn import_starts_declaration(&mut self) -> bool {
        self.peek_kind(|kind| !matches!(kind, SyntaxKind::OpenParenToken | SyntaxKind::DotToken))
    }

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

        // `import type …` — but `import type from "m"` imports a binding named
        // `type`, so the token after decides.
        let is_type_only = self.at(SyntaxKind::TypeKeyword) && self.type_is_modifier_here();
        let phase = if is_type_only { Some(self.take_token()) } else { None };

        // `import x = require("m")` and `import x = A.B`.
        if self.at_binding_identifier() && self.next_is_equals() {
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
    fn parse_named_import_bindings(&mut self) -> Option<NamedImportBindings<'a>> {
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
        let mut elements = Vec::new();
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            let element_start = self.pos();
            let type_only = self.at(SyntaxKind::TypeKeyword) && self.type_is_modifier_here();
            if type_only {
                self.next_token();
            }
            let first = self.parse_module_export_name();
            // `{ a as b }` renames; `{ a }` does not.
            let (property_name, name) = if self.eat(SyntaxKind::AsKeyword) {
                (Some(first), self.parse_identifier())
            } else {
                let name = match first {
                    ModuleExportName::Identifier(id) => id,
                    // A string export name must be renamed to be importable.
                    ModuleExportName::StringLiteral(_) => {
                        self.error_at_current(&messages::IDENTIFIER_EXPECTED);
                        self.missing_identifier()
                    }
                };
                (None, name)
            };
            elements.push(self.finish_node(
                ImportSpecifier::new(type_only, property_name, Some(name)),
                SyntaxKind::ImportSpecifier,
                element_start,
            ));
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
            if self.pos() == before {
                break;
            }
        }
        self.expect(SyntaxKind::CloseBraceToken);
        let elements = self.arena.alloc_slice(&elements);
        let node = self.finish_node(NamedImports::new(elements), SyntaxKind::NamedImports, start);
        Some(NamedImportBindings::NamedImports(node))
    }

    /// `require("m")` or a dotted entity name.
    fn parse_module_reference(&mut self) -> ModuleReference<'a> {
        let start = self.pos();
        if self.at(SyntaxKind::RequireKeyword) {
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
        ModuleReference::from(self.parse_entity_name())
    }

    /// `export …` in all its forms.
    pub(crate) fn parse_export(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        let modifiers_slice = self.arena.alloc_slice(modifiers);

        // `export as namespace N;` declares a UMD global. It has no dedicated
        // node here, so it is recorded as an export assignment of the name.
        if self.at(SyntaxKind::AsKeyword) {
            self.next_token();
            self.expect(SyntaxKind::NamespaceKeyword);
            let name = self.parse_identifier();
            self.parse_semicolon();
            let node = self.finish_node(
                ExportAssignment::new(
                    modifiers_slice,
                    false,
                    None,
                    Some(Expression::Identifier(name)),
                ),
                SyntaxKind::ExportAssignment,
                start,
            );
            return Statement::ExportAssignment(node);
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

        // `export default …` — a declaration if one follows, otherwise an
        // expression.
        if self.at(SyntaxKind::DefaultKeyword) {
            self.next_token();
            // `export default @dec class {}` — decorators sit between.
            if self.at(SyntaxKind::AtToken) {
                let decorators = self.parse_modifiers();
                return self.parse_declaration_after_modifiers(start, &decorators);
            }
            if matches!(
                self.token.kind,
                SyntaxKind::ClassKeyword
                    | SyntaxKind::FunctionKeyword
                    | SyntaxKind::AbstractKeyword
                    | SyntaxKind::InterfaceKeyword
                    | SyntaxKind::EnumKeyword
                    | SyntaxKind::AsyncKeyword
            ) {
                return self.parse_declaration_after_modifiers(start, modifiers);
            }
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
            let mut elements = Vec::new();
            while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
                let before = self.pos();
                let element_start = self.pos();
                let type_only = self.at(SyntaxKind::TypeKeyword) && self.type_is_modifier_here();
                if type_only {
                    self.next_token();
                }
                let first = self.parse_module_export_name();
                let (property_name, name) = if self.eat(SyntaxKind::AsKeyword) {
                    (Some(first), self.parse_module_export_name())
                } else {
                    (None, first)
                };
                elements.push(self.finish_node(
                    ExportSpecifier::new(type_only, property_name, Some(name)),
                    SyntaxKind::ExportSpecifier,
                    element_start,
                ));
                if !self.eat(SyntaxKind::CommaToken) {
                    break;
                }
                if self.pos() == before {
                    break;
                }
            }
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
        let mut all = modifiers.to_vec();
        all.extend(self.parse_modifiers());
        self.parse_declaration_after_modifiers(start, &all)
    }

    /// `namespace N { … }` and `module "m" { … }`.
    pub(crate) fn parse_module_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        // `declare global { … }` has no separate name: `global` is both.
        if self.at(SyntaxKind::GlobalKeyword) {
            let keyword_start = self.pos();
            let keyword = self.take_token();
            let name = ModuleName::Identifier(self.finish_node(
                Identifier::new("global"),
                SyntaxKind::Identifier,
                keyword_start,
            ));
            let block_start = self.pos();
            self.expect(SyntaxKind::OpenBraceToken);
            let statements = self.parse_statement_list(SyntaxKind::CloseBraceToken);
            self.expect(SyntaxKind::CloseBraceToken);
            let statements = self.arena.alloc_slice(&statements);
            let body = ModuleBody::ModuleBlock(self.finish_node(
                ModuleBlock::new(statements),
                SyntaxKind::ModuleBlock,
                block_start,
            ));
            let modifiers = self.arena.alloc_slice(modifiers);
            return Statement::ModuleDeclaration(self.finish_node(
                ModuleDeclaration::new(modifiers, keyword, Some(name), Some(body), None),
                SyntaxKind::ModuleDeclaration,
                start,
            ));
        }

        let keyword = self.take_token();
        let name = if self.at(SyntaxKind::StringLiteral) {
            let literal_start = self.pos();
            let text = self.token_value();
            let flags = self.token.ast_flags();
            self.next_token();
            ModuleName::StringLiteral(self.finish_node(
                StringLiteral::new(text, flags),
                SyntaxKind::StringLiteral,
                literal_start,
            ))
        } else {
            // A dotted name declares nested namespaces: `namespace A.B {}`.
            let mut name = ModuleName::Identifier(self.parse_identifier());
            while self.eat(SyntaxKind::DotToken) {
                name = ModuleName::Identifier(self.parse_identifier());
            }
            name
        };

        let body = if self.at(SyntaxKind::OpenBraceToken) {
            let block_start = self.pos();
            self.expect(SyntaxKind::OpenBraceToken);
            let statements = self.parse_statement_list(SyntaxKind::CloseBraceToken);
            self.expect(SyntaxKind::CloseBraceToken);
            let statements = self.arena.alloc_slice(&statements);
            Some(ModuleBody::ModuleBlock(self.finish_node(
                ModuleBlock::new(statements),
                SyntaxKind::ModuleBlock,
                block_start,
            )))
        } else {
            self.parse_semicolon();
            None
        };

        let modifiers = self.arena.alloc_slice(modifiers);
        let node = self.finish_node(
            ModuleDeclaration::new(modifiers, keyword, Some(name), body, None),
            SyntaxKind::ModuleDeclaration,
            start,
        );
        Statement::ModuleDeclaration(node)
    }

    /// `with { type: "json" }` — import attributes, if present.
    ///
    /// Also accepts the older `assert` spelling, which TypeScript still parses.
    fn parse_import_attributes(&mut self) -> Option<&'a ImportAttributes<'a>> {
        if !self.at(SyntaxKind::WithKeyword) && !self.at(SyntaxKind::AssertKeyword) {
            return None;
        }
        let start = self.pos();
        let token = self.take_token();
        self.expect(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            let element_start = self.pos();
            // An attribute key is an identifier or a string, not the full
            // property-name grammar — no computed keys here.
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
                ImportAttributeName::Identifier(self.parse_identifier())
            };
            self.expect(SyntaxKind::ColonToken);
            let value = self.parse_assignment_expression();
            elements.push(self.finish_node(
                ImportAttribute::new(Some(name), Some(value)),
                SyntaxKind::ImportAttribute,
                element_start,
            ));
            if !self.eat(SyntaxKind::CommaToken) || self.pos() == before {
                break;
            }
        }
        self.expect(SyntaxKind::CloseBraceToken);

        let elements = self.arena.alloc_slice(&elements);
        Some(self.finish_node(
            ImportAttributes::new(token, elements, false),
            SyntaxKind::ImportAttributes,
            start,
        ))
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
        ModuleExportName::Identifier(self.parse_identifier())
    }

    /// The `"module"` in `from "module"`.
    fn parse_module_specifier(&mut self) -> Expression<'a> {
        let start = self.pos();
        if !self.at(SyntaxKind::StringLiteral) {
            self.error_at_current(&messages::STRING_LITERAL_EXPECTED);
            return Expression::Identifier(self.missing_identifier());
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
        self.peek_kind(|kind| {
            !matches!(
                kind,
                SyntaxKind::FromKeyword
                    | SyntaxKind::CommaToken
                    | SyntaxKind::EqualsToken
                    | SyntaxKind::CloseBraceToken
                    | SyntaxKind::AsKeyword
            )
        })
    }

    /// Whether the cursor is on a name usable as a binding.
    ///
    /// Contextual keywords qualify; reserved words do not.
    fn at_binding_identifier(&self) -> bool {
        self.at(SyntaxKind::Identifier) || crate::statement::is_contextual_keyword(self.token.kind)
    }

    /// Whether `type` here modifies an export clause rather than naming an alias.
    fn next_starts_export_clause(&mut self) -> bool {
        self.peek_kind(|kind| {
            matches!(kind, SyntaxKind::OpenBraceToken | SyntaxKind::AsteriskToken)
        })
    }

    fn next_is_equals(&mut self) -> bool {
        self.peek_kind(|kind| kind == SyntaxKind::EqualsToken)
    }
}
