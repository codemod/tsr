//! The transform: a source tree in, a declaration tree out.
//!
//! Ported from typescript-go's `DeclarationTransformer`
//! (`internal/transformers/declarations/transform.go`), around the
//! [`EmitResolver`] seam. Every item names the upstream function it ports.
//!
//! # The three-way split of upstream's file
//!
//! `transform.go` is 2,986 lines, and reading it as one thing is what makes this
//! subsystem look unportable. It is three:
//!
//! 1. **Dispatch and elision** — `visit`, `visitDeclarationStatements`,
//!    `transformTopLevelDeclaration`, `visitDeclarationSubtree`, and the per-node
//!    `transformX` functions. Pure syntax. Ported here, faithfully.
//! 2. **Modifier arithmetic** — `ensureModifiers` and friends. Also pure syntax.
//!    Ported in [`crate::modifiers`].
//! 3. **The resolver calls** — `ensureType`, `ensureNoInitializer`,
//!    `isDeclarationAndNotVisible`, `updateParamList`'s private check. These are
//!    the checker, and they are the whole of
//!    [ADR-0021](../../../docs/adr/0021-isolated-declarations-is-not-a-port.md)'s
//!    finding. They go through [`EmitResolver`], which
//!    [`crate::SyntacticResolver`] answers without a checker.
//!
//! (1) and (2) are two thirds of the file and none of the difficulty.
//!
//! # What is deliberately not ported yet, and why each is safe to omit
//!
//! Each of these is *absent*, not stubbed to something plausible — an emitter that
//! silently produces the wrong text is worse than one that produces none.
//!
//! | Upstream | Why it is out |
//! |---|---|
//! | `transformExpandoAssignment` and the expando block (`:2719`–`:2963`) | Needs `IsExpandoFunctionDeclaration`, which is `TS9023`; the analysis reports it, so the case is not in the target |
//! | `transformCommonJSExport`, `visitCJSExportAssignments` (`:1326`, `:2672`) | CommonJS `module.exports =` emit. Needs the `Program` to know the module kind |
//! | `visitThisPropertyAssignments`, `collectThisPropertyAssignments` (`:2072`, `:2163`) | JS-file only, and JSDoc-driven |
//! | The `JSDoc*` transform arms (`:2576`–`:2632`) | JS-file only |
//! | `CreateLateBoundIndexSignatures` in `buildClassMembers` (`:1918`) | Purely a checker product |
//! | `getReferencedFiles` path rewriting (`:464`) | Needs the output path, which needs the `Program` |
//!
//! [`EmitResolver`]: crate::EmitResolver

use std::collections::HashSet;

use tsr_ast::{
    ClassElement, Expression, ModifierFlags, ModifierLike, NodeFlags, ParameterDeclaration,
    SourceFile, Statement, SyntaxKind, TypeElement, TypeNode,
};
use tsr_core::Span;

use crate::{
    Freshness,
    factory::Factory,
    modifiers,
    resolver::{EmitResolver, LiteralConstHost, has_modifier, is_private_member},
};

/// Ported from `DeclarationTransformer` (`transform.go:63`).
///
/// Upstream's struct has 27 fields; the ones absent here are the visitors it
/// builds in its constructor (this port dispatches directly), the diagnostic
/// context stack (`getSymbolAccessibilityDiagnostic`, which exists to name a
/// *checker* error), and the expando and `CommonJS` maps listed as out of scope in
/// the module docs.
// Four `bool`s, which clippy reads as a state machine wanting to be written. It
// is not one: they are upstream's four independent flags (`transform.go:63`),
// they are set and restored around different scopes, and collapsing them into an
// enum would make the port unrecognisable against the file it tracks.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Transformer<'a, 't, R> {
    pub(crate) factory: Factory<'a, 't>,
    resolver: R,
    /// `transform.go:63`'s `needsDeclare`. False inside a namespace body, where
    /// the enclosing `declare` already covers everything.
    needs_declare: bool,
    /// `needsScopeFixMarker`: something was emitted that is not exported, so the
    /// file would otherwise stop being a module.
    needs_scope_fix_marker: bool,
    /// `resultHasScopeMarker`: an `export {}` or `export =` is already present.
    result_has_scope_marker: bool,
    /// `resultHasExternalModuleIndicator`: something in the output still makes the
    /// file a module.
    result_has_external_module_indicator: bool,
    /// Source and synthesized binding names, for `_default` collision avoidance.
    used_names: HashSet<String>,
    /// Whether declarations are nested under an ambient module/namespace.
    ambient_context: bool,
    /// Where the resolver was asked for a type and had none.
    ///
    /// No upstream counterpart: upstream's resolver always answers. This is what
    /// makes "the emitter guessed" a measurable event rather than a silent `any`
    /// in the output.
    pub(crate) inference_required: Vec<Span>,
}

impl<'a, 't, R: EmitResolver<'a>> Transformer<'a, 't, R> {
    pub(crate) fn new(factory: Factory<'a, 't>, resolver: R) -> Self {
        Self {
            factory,
            resolver,
            needs_declare: true,
            needs_scope_fix_marker: false,
            result_has_scope_marker: false,
            result_has_external_module_indicator: false,
            used_names: HashSet::new(),
            ambient_context: false,
            inference_required: Vec::new(),
        }
    }

    /// Ported from `transformSourceFile` (`transform.go:341`) together with
    /// `visitSourceFile` (`:280`).
    ///
    /// Upstream's `collectFileReferences` (`:310`) is folded into the caller here:
    /// the triple-slash directives are already parsed into
    /// `ParsedSourceFile::file_references` by `tsr_parser::pragma`, so there is
    /// nothing for a collection pass to walk. `appendCjsExports` (`:327`) is a
    /// no-op without `CommonJS` collection and is not reproduced as an empty
    /// concatenation.
    ///
    /// `transformAndReplaceLatePaintedStatements` (`:386`) *is* structurally
    /// present, as the `late` queue below — but the queue is only ever filled by
    /// `handleSymbolAccessibilityError`, which is checker-driven. With a syntactic
    /// resolver it stays empty, and the loop runs zero times. That is stated here
    /// rather than deleted, because the shape is what Phase 4 will fill.
    pub(crate) fn transform_source_file(&mut self, file: &SourceFile<'a>) -> &'a SourceFile<'a> {
        let is_module = is_external_module(file.statements);
        reserve_statement_names(file.statements, &mut self.used_names);

        let mut statements: Vec<Statement<'a>> = Vec::with_capacity(file.statements.len());
        for statement in file.statements {
            for result in self.visit_statement(statement, true) {
                if is_external_module_indicator(&result) {
                    self.result_has_external_module_indicator = true;
                }
                if is_scope_marker(&result) {
                    self.result_has_scope_marker = true;
                }
                if needs_scope_marker(&result) {
                    self.needs_scope_fix_marker = true;
                }
                statements.push(result);
            }
        }

        // `transformSourceFile`'s scope-marker block. A `.d.ts` that dropped every
        // export would stop being a module and its declarations would leak into
        // the global scope, so an empty `export {}` is appended to hold the file's
        // moduleness.
        if is_module
            && (!self.result_has_external_module_indicator
                || (self.needs_scope_fix_marker && !self.result_has_scope_marker))
        {
            let marker = self.create_empty_exports();
            statements.push(marker);
        }

        let statements = self.factory.slice(&statements);
        self.factory.update_source_file(file, statements)
    }

    /// Ported from `createEmptyExports` (`transform.go:382`).
    fn create_empty_exports(&mut self) -> Statement<'a> {
        let span = Span::new(0, 0);
        let named = self.factory.alloc(
            tsr_ast::NamedExports::new(&[]),
            SyntaxKind::NamedExports,
            span,
            NodeFlags::empty(),
        );
        Statement::ExportDeclaration(self.factory.alloc(
            tsr_ast::ExportDeclaration::new(
                &[],
                false,
                Some(tsr_ast::NamedExportBindings::NamedExports(named)),
                None,
                None,
            ),
            SyntaxKind::ExportDeclaration,
            span,
            NodeFlags::empty(),
        ))
    }

    /// Ported from `visit` (`transform.go:226`) and `visitDeclarationStatements`
    /// (`:1144`), which upstream splits only because its visitor framework needs a
    /// single-node signature.
    ///
    /// Returns zero, one or more statements: upstream signals "elide" with `nil`
    /// and "more than one" with a `SyntaxList`, both of which a `Vec` says
    /// directly.
    fn visit_statement(
        &mut self,
        statement: &Statement<'a>,
        parent_is_file: bool,
    ) -> Vec<Statement<'a>> {
        match statement {
            // `transformImportDeclaration` (`transform.go:2471`) and
            // `transformImportEqualsDeclaration` (`:2448`). Both keep the statement
            // only when something in the output still refers to it — upstream asks
            // `IsReferencedAliasDeclaration`/`IsDeclarationVisible`, and the
            // syntactic stand-in is the same reachability set every other
            // declaration goes through. An import nobody references is dropped, and
            // the file gains an `export {}` marker instead if that was all it had.
            Statement::ImportDeclaration(import) => {
                // A side-effect import (`import "./polyfill";`) binds no name, so
                // reachability has nothing to say about it and it must never be
                // elided: dropping it changes what the *importer* of this `.d.ts`
                // loads. Upstream keeps it for the same reason —
                // `transformImportDeclaration` returns the declaration unchanged
                // when `decl.ImportClause == nil` (`transform.go:2474`).
                let binds_nothing = matches!(
                    statement,
                    Statement::ImportDeclaration(import) if import.import_clause.is_none()
                );
                if binds_nothing {
                    return vec![*statement];
                }
                if parent_is_file && !self.resolver.is_declaration_visible(statement) {
                    return Vec::new();
                }
                vec![self.transform_import_declaration(import)]
            }
            Statement::ImportEqualsDeclaration(_) => {
                if parent_is_file && !self.resolver.is_declaration_visible(statement) {
                    return Vec::new();
                }
                vec![*statement]
            }
            Statement::ExportDeclaration(_) => vec![*statement],
            Statement::ExportAssignment(node) => self.transform_export_assignment(node),
            Statement::FunctionDeclaration(_)
            | Statement::ModuleDeclaration(_)
            | Statement::InterfaceDeclaration(_)
            | Statement::ClassDeclaration(_)
            | Statement::TypeAliasDeclaration(_)
            | Statement::EnumDeclaration(_)
            | Statement::VariableStatement(_) => {
                self.transform_top_level_declaration(statement, parent_is_file)
            }

            // Statements we elide. Upstream lists these by kind; the effect is
            // that nothing with a runtime body survives.
            _ => Vec::new(),
        }
    }

    /// A deferred import's runtime phase has no meaning in a declaration file.
    /// Its bindings remain ordinary type-visible imports, while `import type`
    /// keeps its phase modifier.
    fn transform_import_declaration(
        &mut self,
        node: &'a tsr_ast::ImportDeclaration<'a>,
    ) -> Statement<'a> {
        let Some(clause) = node.import_clause else {
            return Statement::ImportDeclaration(node);
        };
        if !clause.phase_modifier.is_some_and(|modifier| modifier.kind == SyntaxKind::DeferKeyword)
        {
            return Statement::ImportDeclaration(node);
        }
        let span = self.span_of(clause.node_id);
        let clause = self.factory.alloc(
            tsr_ast::ImportClause::new(None, clause.name, clause.named_bindings),
            SyntaxKind::ImportClause,
            span,
            NodeFlags::empty(),
        );
        Statement::ImportDeclaration(self.factory.alloc(
            tsr_ast::ImportDeclaration::new(
                node.modifiers,
                Some(clause),
                node.module_specifier,
                node.attributes,
            ),
            SyntaxKind::ImportDeclaration,
            self.span_of(node.node_id),
            NodeFlags::empty(),
        ))
    }

    /// Turn `export default <expression>` into the declaration form TypeScript
    /// emits: a collision-free `_default` variable followed by a default export
    /// of that identifier. A named identifier export already is valid declaration
    /// syntax and needs no synthetic binding.
    fn transform_export_assignment(
        &mut self,
        node: &'a tsr_ast::ExportAssignment<'a>,
    ) -> Vec<Statement<'a>> {
        if matches!(node.expression, Some(Expression::Identifier(_))) {
            return vec![Statement::ExportAssignment(node)];
        }

        let span = self.span_of(node.node_id);
        let name = self.fresh_default_export_name(span);
        let r#type =
            self.ensure_type(None, node.expression.as_ref(), Freshness::Widening, node.node_id);
        let declaration = self.factory.alloc(
            tsr_ast::VariableDeclaration::new(
                Some(tsr_ast::BindingName::Identifier(name)),
                None,
                r#type,
                None,
            ),
            SyntaxKind::VariableDeclaration,
            span,
            NodeFlags::empty(),
        );
        let declarations = self.factory.slice(&[declaration]);
        let list = self.factory.alloc(
            tsr_ast::VariableDeclarationList::new(declarations),
            SyntaxKind::VariableDeclarationList,
            span,
            NodeFlags::CONST,
        );
        let declare = self.factory.modifier(SyntaxKind::DeclareKeyword, span);
        let modifiers = self.factory.slice(&[declare]);
        let variable = Statement::VariableStatement(self.factory.alloc(
            tsr_ast::VariableStatement::new(modifiers, Some(list)),
            SyntaxKind::VariableStatement,
            span,
            NodeFlags::empty(),
        ));
        let export = Statement::ExportAssignment(self.factory.alloc(
            tsr_ast::ExportAssignment::new(
                node.modifiers,
                node.is_export_equals,
                node.r#type,
                Some(Expression::Identifier(name)),
            ),
            SyntaxKind::ExportAssignment,
            span,
            NodeFlags::empty(),
        ));
        vec![variable, export]
    }

    fn fresh_default_export_name(&mut self, span: Span) -> &'a tsr_ast::Identifier<'a> {
        let mut suffix = 0usize;
        loop {
            let candidate =
                if suffix == 0 { "_default".to_string() } else { format!("_default_{suffix}") };
            if self.used_names.insert(candidate.clone()) {
                return self.factory.identifier(&candidate, span);
            }
            suffix += 1;
        }
    }

    /// Ported from `transformTopLevelDeclaration` (`transform.go:1703`).
    fn transform_top_level_declaration(
        &mut self,
        statement: &Statement<'a>,
        parent_is_file: bool,
    ) -> Vec<Statement<'a>> {
        // `isDeclarationAndNotVisible` (`util.go`). A declaration nothing exported
        // reaches is not in the `.d.ts` at all.
        if !self.resolver.is_declaration_visible(statement) {
            return Vec::new();
        }
        match statement {
            Statement::TypeAliasDeclaration(node) => {
                // `transformTypeAliasDeclaration` (`:1783`) clears `needsDeclare`
                // for the duration: a type alias's body can contain declarations,
                // and none of them takes `declare`.
                let saved = std::mem::replace(&mut self.needs_declare, false);
                let modifiers =
                    self.ensure_modifiers(node.modifiers, node.node_id, parent_is_file, false);
                self.needs_declare = saved;
                vec![Statement::TypeAliasDeclaration(self.factory.alloc(
                    tsr_ast::TypeAliasDeclaration::new(
                        modifiers,
                        node.name,
                        node.type_parameters,
                        node.r#type,
                    ),
                    SyntaxKind::TypeAliasDeclaration,
                    self.span_of(node.node_id),
                    NodeFlags::empty(),
                ))]
            }
            Statement::InterfaceDeclaration(node) => {
                // `transformInterfaceDeclaration` (`:1794`). `isAlwaysType`, so no
                // `declare`.
                let modifiers =
                    self.ensure_modifiers(node.modifiers, node.node_id, parent_is_file, true);
                let members = self.visit_type_members(node.members);
                vec![Statement::InterfaceDeclaration(self.factory.alloc(
                    tsr_ast::InterfaceDeclaration::new(
                        modifiers,
                        node.name,
                        node.type_parameters,
                        node.heritage_clauses,
                        members,
                    ),
                    SyntaxKind::InterfaceDeclaration,
                    self.span_of(node.node_id),
                    NodeFlags::empty(),
                ))]
            }
            Statement::FunctionDeclaration(node) => {
                // `transformFunctionDeclaration` (`:1805`).
                let modifiers =
                    self.ensure_modifiers(node.modifiers, node.node_id, parent_is_file, false);
                let parameters = self.update_param_list(node.parameters, false);
                let return_type =
                    self.ensure_return_type(node.r#type, node.body.as_ref(), node.node_id);
                vec![Statement::FunctionDeclaration(self.factory.alloc(
                    tsr_ast::FunctionDeclaration::new(
                        modifiers,
                        None,
                        node.name,
                        node.type_parameters,
                        parameters,
                        return_type,
                        None,
                        None,
                    ),
                    SyntaxKind::FunctionDeclaration,
                    self.span_of(node.node_id),
                    NodeFlags::empty(),
                ))]
            }
            Statement::ClassDeclaration(node) => self.transform_class_declaration(node),
            Statement::EnumDeclaration(node) => {
                // `transformEnumDeclaration` (`:2262`): every member's initializer
                // is rewritten to its constant value, and dropped when there is
                // none. See [`crate::enum_value`] for why keeping the source form
                // is not an option.
                let modifiers =
                    self.ensure_modifiers(node.modifiers, node.node_id, parent_is_file, false);
                let name = node.name.map_or("", |name| name.text);
                let values = self.resolver.get_enum_member_values(node.members, name);
                let mut members = Vec::with_capacity(node.members.len());
                for (member, value) in node.members.iter().zip(values) {
                    let span = self.span_of(member.node_id);
                    let is_ambient = self.ambient_context
                        || has_modifier(node.modifiers, SyntaxKind::DeclareKeyword);
                    let initializer = if is_ambient && member.initializer.is_none() {
                        None
                    } else {
                        value.map(|value| self.enum_initializer(&value, span))
                    };
                    members.push(self.factory.alloc(
                        tsr_ast::EnumMember::new(member.name, initializer, &[], None),
                        SyntaxKind::EnumMember,
                        span,
                        NodeFlags::empty(),
                    ));
                }
                let members = self.factory.slice(&members);
                vec![Statement::EnumDeclaration(self.factory.alloc(
                    tsr_ast::EnumDeclaration::new(modifiers, node.name, members),
                    SyntaxKind::EnumDeclaration,
                    self.span_of(node.node_id),
                    NodeFlags::empty(),
                ))]
            }
            Statement::VariableStatement(node) => self.transform_variable_statement(node),
            Statement::ModuleDeclaration(node) => self.transform_module_declaration(node),
            // `transformTopLevelDeclaration`'s default arm panics upstream,
            // because its dispatch has already filtered the kinds. Here the caller
            // is the same dispatch, so this is equally unreachable — and eliding
            // rather than panicking keeps a corpus run from dying on one case.
            _ => Vec::new(),
        }
    }

    /// Ported from `transformVariableStatement` (`transform.go:2207`).
    fn transform_variable_statement(
        &mut self,
        node: &tsr_ast::VariableStatement<'a>,
    ) -> Vec<Statement<'a>> {
        let Some(list) = node.declaration_list else { return Vec::new() };
        let flags = self.factory.flags_of(list.node_id);
        let is_const = flags.intersects(NodeFlags::CONSTANT);

        let mut declarations = Vec::with_capacity(list.declarations.len());
        for declaration in list.declarations {
            declarations.push(self.transform_variable_declaration(declaration, is_const));
        }
        if declarations.is_empty() {
            return Vec::new();
        }
        let declarations = self.factory.slice(&declarations);

        let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, true, false);
        // Upstream rewrites a `using`/`await using` list to `const`, because
        // neither keyword is legal in a `.d.ts`. `NodeFlags` are what the printer
        // reads for the keyword, so the rewrite is a flag change on a new list
        // node rather than a token swap.
        let list_flags = if flags.contains(NodeFlags::USING) {
            (flags & !NodeFlags::USING) | NodeFlags::CONST
        } else {
            flags
        };
        let list = self.factory.alloc(
            tsr_ast::VariableDeclarationList::new(declarations),
            SyntaxKind::VariableDeclarationList,
            self.span_of(list.node_id),
            list_flags,
        );
        vec![Statement::VariableStatement(self.factory.alloc(
            tsr_ast::VariableStatement::new(modifiers, Some(list)),
            SyntaxKind::VariableStatement,
            self.span_of(node.node_id),
            NodeFlags::empty(),
        ))]
    }

    /// Ported from `transformVariableDeclaration` (`transform.go:835`).
    fn transform_variable_declaration(
        &mut self,
        declaration: &'a tsr_ast::VariableDeclaration<'a>,
        is_const: bool,
    ) -> &'a tsr_ast::VariableDeclaration<'a> {
        let host = LiteralConstHost::Variable(declaration, is_const);
        let initializer = self.ensure_no_initializer(host);
        let r#type = if initializer.is_some() {
            // `ensureType`'s first branch: a literal const emits its value, not a
            // type, and emitting both would be a syntax error.
            None
        } else {
            // **Widening, even for a `const` declaration.** A `const` variable is
            // not a const *assertion*: `const o = { a: 1 }` has type
            // `{ a: number }`, and `const b = [1, 2]` has type `number[]` — only
            // `as const` produces `readonly` members and literal element types.
            // `compiler/isolatedDeclarationsLiterals` pairs the two spellings of
            // the same object literal for exactly this contrast, and its baseline
            // reads `readonly one: 1` for the asserted one and `one: number` for
            // the plain `const`.
            //
            // Where a `const` *does* keep its literal type — `const one = 1` — the
            // value is emitted instead of a type, through `ensure_no_initializer`
            // above. Freshness is entered only by `as const`, in
            // [`crate::type_builder`].
            let _ = is_const;
            self.ensure_type(
                declaration.r#type,
                declaration.initializer.as_ref(),
                Freshness::Widening,
                declaration.node_id,
            )
        };
        self.factory.alloc(
            tsr_ast::VariableDeclaration::new(declaration.name, None, r#type, initializer),
            SyntaxKind::VariableDeclaration,
            self.span_of(declaration.node_id),
            NodeFlags::empty(),
        )
    }

    /// Ported from `transformModuleDeclaration` (`transform.go:1822`).
    ///
    /// The three-way choice at the end of upstream's function is the interesting
    /// part and is reproduced: a namespace body whose statements are *all*
    /// exported has its `export` modifiers stripped, because everything in an
    /// ambient namespace is exported already; a body with a mix gains an
    /// `export {}` marker so the unexported ones stay unexported.
    fn transform_module_declaration(
        &mut self,
        node: &tsr_ast::ModuleDeclaration<'a>,
    ) -> Vec<Statement<'a>> {
        let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, true, false);
        let saved_declare = std::mem::replace(&mut self.needs_declare, false);
        let enters_ambient = self.ambient_context
            || self.factory.flags_of(node.node_id).contains(NodeFlags::AMBIENT)
            || has_modifier(node.modifiers, SyntaxKind::DeclareKeyword);
        let saved_ambient = std::mem::replace(&mut self.ambient_context, enters_ambient);
        let saved_needs_fix = std::mem::replace(&mut self.needs_scope_fix_marker, false);
        let saved_has_marker = std::mem::replace(&mut self.result_has_scope_marker, false);

        let body = match node.body {
            Some(tsr_ast::ModuleBody::ModuleBlock(block)) => {
                let mut statements = Vec::with_capacity(block.statements.len());
                for statement in block.statements {
                    for result in self.visit_statement(statement, false) {
                        if is_scope_marker(&result) {
                            self.result_has_scope_marker = true;
                        }
                        if needs_scope_marker(&result) {
                            self.needs_scope_fix_marker = true;
                        }
                        statements.push(result);
                    }
                }
                // "If it was `declare`'d everything is implicitly exported
                // already, ignore late printed privates" (`transform.go:1846`).
                // Without this the transform appends an `export {}` inside every
                // `declare namespace` in the corpus — which parses, and which
                // upstream never writes.
                //
                // Upstream tests `input.Flags & NodeFlagsAmbient`, a flag its
                // parser sets inside an ambient context. **This parser does not set
                // it** — `declare namespace M {}` comes back with empty
                // `NodeFlags` — so the flag test alone was inert and moved the
                // corpus by exactly zero. The `declare` modifier is the same fact
                // in the form this tree actually carries it; the flag is still
                // tested so this reads correctly once the parser gap
                // (`bd tsr-qc3`) is closed.
                if enters_ambient {
                    self.needs_scope_fix_marker = false;
                }
                // `!ast.IsGlobalScopeAugmentation(input)`: `declare global { … }`
                // is not a scope of its own, so it neither needs a marker nor has
                // its exports stripped.
                let is_global = node.keyword.kind == SyntaxKind::GlobalKeyword;
                if !is_global && !self.result_has_scope_marker {
                    if self.needs_scope_fix_marker {
                        let marker = self.create_empty_exports();
                        statements.push(marker);
                    } else {
                        statements = statements
                            .iter()
                            .map(|statement| self.strip_export_modifiers(statement))
                            .collect();
                    }
                }
                let statements = self.factory.slice(&statements);
                Some(tsr_ast::ModuleBody::ModuleBlock(self.factory.alloc(
                    tsr_ast::ModuleBlock::new(statements),
                    SyntaxKind::ModuleBlock,
                    self.span_of(block.node_id),
                    NodeFlags::empty(),
                )))
            }
            // `namespace A.B {}` nests two `ModuleDeclaration`s; the inner one is
            // transformed in place and keeps its own header, which is what the
            // printer expects (`docs/architecture/printer.md`).
            Some(tsr_ast::ModuleBody::ModuleDeclaration(inner)) => {
                let inner = self.transform_module_declaration(inner);
                match inner.into_iter().next() {
                    Some(Statement::ModuleDeclaration(inner)) => {
                        Some(tsr_ast::ModuleBody::ModuleDeclaration(inner))
                    }
                    _ => None,
                }
            }
            None => None,
        };

        self.needs_declare = saved_declare;
        self.ambient_context = saved_ambient;
        self.needs_scope_fix_marker = saved_needs_fix;
        self.result_has_scope_marker = saved_has_marker;

        vec![Statement::ModuleDeclaration(self.factory.alloc(
            tsr_ast::ModuleDeclaration::new(modifiers, node.keyword, node.name, body, None),
            SyntaxKind::ModuleDeclaration,
            self.span_of(node.node_id),
            NodeFlags::empty(),
        ))]
    }

    /// Ported from `stripExportModifiers` (`transform.go:1895`).
    fn strip_export_modifiers(&mut self, statement: &Statement<'a>) -> Statement<'a> {
        // `export import` stays as written — imports are *not* implicitly exported
        // in an ambient namespace — and so does anything `default`, which must
        // keep its `export` to parse.
        if matches!(statement, Statement::ImportEqualsDeclaration(_)) {
            return *statement;
        }
        let Some(existing) = statement_modifiers(statement) else { return *statement };
        let old = modifiers::modifier_flags(existing);
        if old.contains(ModifierFlags::DEFAULT) || !old.contains(ModifierFlags::EXPORT) {
            return *statement;
        }
        let new_flags = old & (modifiers::ALL ^ ModifierFlags::EXPORT);
        let span = self.span_of(statement.node_id());
        let created = modifiers::create_modifiers_from_flags(&mut self.factory, new_flags, span);
        let replacement = self.factory.slice(&created);
        replace_modifiers(&mut self.factory, statement, replacement)
    }

    /// Ported from `transformClassDeclaration` (`transform.go:1978`) and
    /// `buildClassMembers` (`:1918`).
    ///
    /// Upstream's `extends`-clause rewrite — hoisting a non-entity-name base into
    /// a synthesized `declare const X_base: …` — is absent, because it needs
    /// `CreateTypeOfExpression`. That case is `TS9021`, which the analysis
    /// reports, so it is not in the target.
    fn transform_class_declaration(
        &mut self,
        node: &tsr_ast::ClassDeclaration<'a>,
    ) -> Vec<Statement<'a>> {
        let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, true, false);
        let (base_variable, heritage_clauses) = self.rewrite_class_base(node);
        let mut members: Vec<ClassElement<'a>> = Vec::with_capacity(node.members.len());

        // `buildClassMembers`'s parameter-property pass: a `private x` in the
        // constructor's parameter list is a property of the class, and the `.d.ts`
        // has to restate it as one.
        if let Some(ClassElement::ConstructorDeclaration(constructor)) = node
            .members
            .iter()
            .find(|member| matches!(member, ClassElement::ConstructorDeclaration(_)))
        {
            for parameter in constructor.parameters {
                if modifiers::modifier_flags(parameter.modifiers)
                    .intersects(PARAMETER_PROPERTY_MODIFIER)
                {
                    if let Some(property) = self.parameter_property(parameter) {
                        members.push(property);
                    }
                }
            }
        }

        // `buildClassMembers`'s `#private` marker: a class with any private name
        // is nominally typed, and the `.d.ts` keeps that by carrying one.
        if node.members.iter().any(|member| {
            matches!(class_member_name(member), Some(tsr_ast::PropertyName::PrivateIdentifier(_)))
        }) {
            let span = self.span_of(node.node_id);
            let name = self.factory.alloc(
                tsr_ast::PrivateIdentifier::new("#private"),
                SyntaxKind::PrivateIdentifier,
                span,
                NodeFlags::empty(),
            );
            members.push(ClassElement::PropertyDeclaration(self.factory.alloc(
                tsr_ast::PropertyDeclaration::new(
                    &[],
                    tsr_ast::PropertyName::PrivateIdentifier(name),
                    None,
                    None,
                    None,
                ),
                SyntaxKind::PropertyDeclaration,
                span,
                NodeFlags::empty(),
            )));
        }

        let has_constructor_overloads = node.members.iter().any(|member| {
            matches!(member, ClassElement::ConstructorDeclaration(constructor) if constructor.body.is_none())
        });
        let mut private_method_markers = Vec::new();
        for member in node.members {
            if matches!(member, ClassElement::ConstructorDeclaration(constructor) if constructor.body.is_some())
                && has_constructor_overloads
            {
                continue;
            }
            if let ClassElement::MethodDeclaration(method) = member
                && is_private_member(member)
            {
                let is_static = has_modifier(method.modifiers, SyntaxKind::StaticKeyword);
                if private_method_markers.iter().any(|(seen_static, seen_name)| {
                    *seen_static == is_static && property_names_equal(seen_name, &method.name)
                }) {
                    continue;
                }
                private_method_markers.push((is_static, method.name));
            }
            if let Some(member) = self.visit_class_element(member) {
                members.push(member);
            }
        }

        let members = self.factory.slice(&members);
        let class = Statement::ClassDeclaration(self.factory.alloc(
            tsr_ast::ClassDeclaration::new(
                modifiers,
                node.name,
                node.type_parameters,
                heritage_clauses,
                members,
            ),
            SyntaxKind::ClassDeclaration,
            self.span_of(node.node_id),
            NodeFlags::empty(),
        ));
        match base_variable {
            Some(base) => vec![base, class],
            None => vec![class],
        }
    }

    /// Hoist a non-name `extends` expression to the private declaration shape
    /// upstream emits. Its exact type is checker-built and the existing TS9021
    /// diagnostic records that gap; `any` keeps this approximate output valid
    /// while preserving declaration order, naming, and heritage structure.
    fn rewrite_class_base(
        &mut self,
        node: &tsr_ast::ClassDeclaration<'a>,
    ) -> (Option<Statement<'a>>, &'a [&'a tsr_ast::HeritageClause<'a>]) {
        let Some(class_name) = node.name else { return (None, node.heritage_clauses) };
        let Some((clause_index, base)) =
            node.heritage_clauses.iter().enumerate().find_map(|(index, clause)| {
                if clause.token.kind != SyntaxKind::ExtendsKeyword {
                    return None;
                }
                let base = *clause.types.first()?;
                let expression = base.expression.as_ref()?;
                (!is_entity_name_expression(expression)).then_some((index, base))
            })
        else {
            return (None, node.heritage_clauses);
        };

        let span = self.span_of(base.node_id);
        let base_name = self.fresh_class_base_name(class_name.text, span);
        let any_type = self.factory.keyword_type(SyntaxKind::AnyKeyword, span);
        let declaration = self.factory.alloc(
            tsr_ast::VariableDeclaration::new(
                Some(tsr_ast::BindingName::Identifier(base_name)),
                None,
                Some(any_type),
                None,
            ),
            SyntaxKind::VariableDeclaration,
            span,
            NodeFlags::empty(),
        );
        let declarations = self.factory.slice(&[declaration]);
        let list = self.factory.alloc(
            tsr_ast::VariableDeclarationList::new(declarations),
            SyntaxKind::VariableDeclarationList,
            span,
            NodeFlags::CONST,
        );
        let declare = self.factory.modifier(SyntaxKind::DeclareKeyword, span);
        let modifiers = self.factory.slice(&[declare]);
        let variable = Statement::VariableStatement(self.factory.alloc(
            tsr_ast::VariableStatement::new(modifiers, Some(list)),
            SyntaxKind::VariableStatement,
            span,
            NodeFlags::empty(),
        ));

        let replacement_base = self.factory.alloc(
            tsr_ast::ExpressionWithTypeArguments::new(
                Some(Expression::Identifier(base_name)),
                base.type_arguments,
            ),
            SyntaxKind::ExpressionWithTypeArguments,
            span,
            NodeFlags::empty(),
        );
        let mut clauses = node.heritage_clauses.to_vec();
        let original = clauses[clause_index];
        let types = self.factory.slice(&[replacement_base]);
        clauses[clause_index] = self.factory.alloc(
            tsr_ast::HeritageClause::new(original.token, types),
            SyntaxKind::HeritageClause,
            self.span_of(original.node_id),
            NodeFlags::empty(),
        );
        (Some(variable), self.factory.slice(&clauses))
    }

    fn fresh_class_base_name(
        &mut self,
        class_name: &str,
        span: Span,
    ) -> &'a tsr_ast::Identifier<'a> {
        let stem = format!("{class_name}_base");
        let mut suffix = 0usize;
        loop {
            let candidate = if suffix == 0 { stem.clone() } else { format!("{stem}_{suffix}") };
            if self.used_names.insert(candidate.clone()) {
                return self.factory.identifier(&candidate, span);
            }
            suffix += 1;
        }
    }

    /// The property a parameter property declares, from `buildClassMembers`.
    fn parameter_property(
        &mut self,
        parameter: &'a ParameterDeclaration<'a>,
    ) -> Option<ClassElement<'a>> {
        let Some(tsr_ast::BindingName::Identifier(name)) = parameter.name else {
            // A destructured parameter property is an error upstream too; it emits
            // something approximate through `walkBindingPattern`, which is not
            // reproduced.
            return None;
        };
        let span = self.span_of(parameter.node_id);
        let modifiers = self.ensure_modifiers(parameter.modifiers, parameter.node_id, false, false);
        // `ensureType(param, /*ignorePrivate*/ false)` (`transform.go:1933`). The
        // *property* a private parameter property declares emits no type, while
        // the constructor parameter it came from keeps one — `ensureParameter`
        // passes `ignorePrivate: true` for exactly that reason. Emitting the type
        // in both places leaks a private member's shape, and parses.
        let r#type = if has_modifier(parameter.modifiers, SyntaxKind::PrivateKeyword) {
            None
        } else {
            self.ensure_type(parameter.r#type, None, Freshness::Widening, parameter.node_id)
        };
        Some(ClassElement::PropertyDeclaration(self.factory.alloc(
            tsr_ast::PropertyDeclaration::new(
                modifiers,
                tsr_ast::PropertyName::Identifier(name),
                parameter.question_token,
                r#type,
                None,
            ),
            SyntaxKind::PropertyDeclaration,
            span,
            NodeFlags::empty(),
        )))
    }

    /// Ported from the class-member arms of `visitDeclarationSubtree`
    /// (`transform.go:573`) and the `transformX` functions they call.
    fn visit_class_element(&mut self, member: &ClassElement<'a>) -> Option<ClassElement<'a>> {
        // `visitDeclarationSubtree`: a `SemicolonClassElement` and a static block
        // both emit nothing.
        if matches!(
            member,
            ClassElement::SemicolonClassElement(_) | ClassElement::ClassStaticBlockDeclaration(_)
        ) {
            return None;
        }
        // A `#private` member is covered by the single `#private` marker the class
        // already carries; emitting it by name would leak it.
        if matches!(class_member_name(member), Some(tsr_ast::PropertyName::PrivateIdentifier(_))) {
            return None;
        }
        let private = is_private_member(member);

        match member {
            // `transformPropertyDeclaration` (`:979`).
            ClassElement::PropertyDeclaration(node) => {
                let span = self.span_of(node.node_id);
                let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, false, false);
                // A definite-assignment `!` is not legal in a `.d.ts`; a `?` is.
                let postfix =
                    node.postfix_token.filter(|token| token.kind != SyntaxKind::ExclamationToken);
                let host = LiteralConstHost::Property(node);
                let initializer = self.ensure_no_initializer(host);
                let r#type = if initializer.is_some() || private {
                    None
                } else {
                    self.ensure_type(
                        node.r#type,
                        node.initializer.as_ref(),
                        Freshness::Widening,
                        node.node_id,
                    )
                };
                Some(ClassElement::PropertyDeclaration(self.factory.alloc(
                    tsr_ast::PropertyDeclaration::new(
                        modifiers,
                        node.name,
                        postfix,
                        r#type,
                        initializer,
                    ),
                    SyntaxKind::PropertyDeclaration,
                    span,
                    NodeFlags::empty(),
                )))
            }
            // `transformMethodDeclaration` (`:1123`), including
            // `omitPrivateMethodType` (`:1090`): a private method emits as a
            // property with no type, because its signature is not part of the
            // class's public shape.
            ClassElement::MethodDeclaration(node) => {
                let span = self.span_of(node.node_id);
                let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, false, false);
                if private {
                    return Some(ClassElement::PropertyDeclaration(self.factory.alloc(
                        tsr_ast::PropertyDeclaration::new(modifiers, node.name, None, None, None),
                        SyntaxKind::PropertyDeclaration,
                        span,
                        NodeFlags::empty(),
                    )));
                }
                let parameters = self.update_param_list(node.parameters, false);
                let return_type =
                    self.ensure_return_type(node.r#type, node.body.as_ref(), node.node_id);
                Some(ClassElement::MethodDeclaration(self.factory.alloc(
                    tsr_ast::MethodDeclaration::new(
                        modifiers,
                        None,
                        node.name,
                        node.postfix_token,
                        node.type_parameters,
                        parameters,
                        return_type,
                        None,
                        None,
                    ),
                    SyntaxKind::MethodDeclaration,
                    span,
                    NodeFlags::empty(),
                )))
            }
            // `transformConstructorDeclaration` (`:1068`). No type parameters and
            // no return type: a constructor may not be annotated.
            ClassElement::ConstructorDeclaration(node) => {
                let span = self.span_of(node.node_id);
                let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, false, false);
                let parameters = self.update_param_list(node.parameters, private);
                Some(ClassElement::ConstructorDeclaration(self.factory.alloc(
                    tsr_ast::ConstructorDeclaration::new(
                        modifiers,
                        &[],
                        parameters,
                        None,
                        None,
                        None,
                        None,
                    ),
                    SyntaxKind::Constructor,
                    span,
                    NodeFlags::empty(),
                )))
            }
            // `transformGetAccesorDeclaration` (`:1015`).
            ClassElement::GetAccessorDeclaration(node) => {
                let span = self.span_of(node.node_id);
                let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, false, false);
                let parameters = self.update_param_list(node.parameters, private);
                let return_type = if private {
                    None
                } else {
                    self.ensure_return_type(node.r#type, node.body.as_ref(), node.node_id)
                };
                Some(ClassElement::GetAccessorDeclaration(self.factory.alloc(
                    tsr_ast::GetAccessorDeclaration::new(
                        modifiers,
                        node.name,
                        &[],
                        parameters,
                        return_type,
                        None,
                        None,
                        None,
                        None,
                    ),
                    SyntaxKind::GetAccessor,
                    span,
                    NodeFlags::empty(),
                )))
            }
            // `transformSetAccessorDeclaration` (`:998`) with
            // `updateAccessorParamList` (`:1031`), which synthesizes `value: any`
            // when the setter has no parameter to reuse.
            ClassElement::SetAccessorDeclaration(node) => {
                let span = self.span_of(node.node_id);
                let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, false, false);
                let parameters = self.update_accessor_param_list(node.parameters, private, span);
                Some(ClassElement::SetAccessorDeclaration(self.factory.alloc(
                    tsr_ast::SetAccessorDeclaration::new(
                        modifiers,
                        node.name,
                        &[],
                        parameters,
                        None,
                        None,
                        None,
                        None,
                        None,
                    ),
                    SyntaxKind::SetAccessor,
                    span,
                    NodeFlags::empty(),
                )))
            }
            // `transformIndexSignatureDeclaration` (`:941`), including its fallback
            // to `any` for a missing type.
            ClassElement::IndexSignatureDeclaration(node) => {
                let span = self.span_of(node.node_id);
                let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, false, false);
                let parameters = self.update_param_list(node.parameters, false);
                let r#type = node
                    .r#type
                    .unwrap_or_else(|| self.factory.keyword_type(SyntaxKind::AnyKeyword, span));
                Some(ClassElement::IndexSignatureDeclaration(self.factory.alloc(
                    tsr_ast::IndexSignatureDeclaration::new(
                        modifiers,
                        parameters,
                        Some(r#type),
                        None,
                        &[],
                    ),
                    SyntaxKind::IndexSignature,
                    span,
                    NodeFlags::empty(),
                )))
            }
            ClassElement::SemicolonClassElement(_)
            | ClassElement::ClassStaticBlockDeclaration(_) => None,
        }
    }

    /// Interface and type-literal members.
    ///
    /// Ported from the `TypeElement` arms of `visitDeclarationSubtree`. An
    /// interface member is already declaration-shaped, so the only work is the
    /// signature transforms — chiefly dropping an initializer that a
    /// `PropertySignature` should never have carried.
    fn visit_type_members(&mut self, members: &'a [TypeElement<'a>]) -> &'a [TypeElement<'a>] {
        let mut result = Vec::with_capacity(members.len());
        for member in members {
            match member {
                TypeElement::CallSignatureDeclaration(node) => {
                    let span = self.span_of(node.node_id);
                    let parameters = self.update_param_list(node.parameters, false);
                    result.push(TypeElement::CallSignatureDeclaration(self.factory.alloc(
                        tsr_ast::CallSignatureDeclaration::new(
                            node.type_parameters,
                            parameters,
                            node.r#type,
                            node.full_signature,
                        ),
                        SyntaxKind::CallSignature,
                        span,
                        NodeFlags::empty(),
                    )));
                }
                TypeElement::ConstructSignatureDeclaration(node) => {
                    let span = self.span_of(node.node_id);
                    let parameters = self.update_param_list(node.parameters, false);
                    result.push(TypeElement::ConstructSignatureDeclaration(self.factory.alloc(
                        tsr_ast::ConstructSignatureDeclaration::new(
                            node.type_parameters,
                            parameters,
                            node.r#type,
                            node.full_signature,
                        ),
                        SyntaxKind::ConstructSignature,
                        span,
                        NodeFlags::empty(),
                    )));
                }
                // `transformPropertySignatureDeclaration` (`:963`).
                TypeElement::PropertySignatureDeclaration(node) => {
                    let span = self.span_of(node.node_id);
                    result.push(TypeElement::PropertySignatureDeclaration(self.factory.alloc(
                        tsr_ast::PropertySignatureDeclaration::new(
                            node.modifiers,
                            node.name,
                            node.postfix_token,
                            node.r#type,
                            None,
                        ),
                        SyntaxKind::PropertySignature,
                        span,
                        NodeFlags::empty(),
                    )));
                }
                TypeElement::MethodSignatureDeclaration(node) => {
                    let span = self.span_of(node.node_id);
                    let modifiers =
                        self.ensure_modifiers(node.modifiers, node.node_id, false, true);
                    let parameters = self.update_param_list(node.parameters, false);
                    result.push(TypeElement::MethodSignatureDeclaration(self.factory.alloc(
                        tsr_ast::MethodSignatureDeclaration::new(
                            modifiers,
                            node.name,
                            node.postfix_token,
                            node.type_parameters,
                            parameters,
                            node.r#type,
                            node.full_signature,
                        ),
                        SyntaxKind::MethodSignature,
                        span,
                        NodeFlags::empty(),
                    )));
                }
                TypeElement::IndexSignatureDeclaration(node) => {
                    let span = self.span_of(node.node_id);
                    let parameters = self.update_param_list(node.parameters, false);
                    result.push(TypeElement::IndexSignatureDeclaration(self.factory.alloc(
                        tsr_ast::IndexSignatureDeclaration::new(
                            node.modifiers,
                            parameters,
                            node.r#type,
                            node.full_signature,
                            node.type_parameters,
                        ),
                        SyntaxKind::IndexSignature,
                        span,
                        NodeFlags::empty(),
                    )));
                }
                TypeElement::NotEmittedTypeElement(_) => {}
                other => result.push(*other),
            }
        }
        self.factory.slice(&result)
    }

    /// Ported from `updateParamList` (`transform.go:2384`) and `ensureParameter`
    /// (`:2395`).
    fn update_param_list(
        &mut self,
        parameters: &'a [&'a ParameterDeclaration<'a>],
        is_private: bool,
    ) -> &'a [&'a ParameterDeclaration<'a>] {
        // A private member's parameters are not part of the public shape, so
        // upstream emits an empty list rather than the real one.
        if is_private || parameters.is_empty() {
            return &[];
        }
        let mut result = Vec::with_capacity(parameters.len());
        for (index, parameter) in parameters.iter().enumerate() {
            let ensured = self.ensure_parameter(parameter);
            let has_later_required = parameters[index + 1..]
                .iter()
                .any(|later| !self.resolver.is_optional_parameter(later));
            if parameter.initializer.is_some() && has_later_required {
                result.push(self.require_initialized_parameter(ensured));
            } else {
                result.push(ensured);
            }
        }
        self.factory.slice(&result)
    }

    fn require_initialized_parameter(
        &mut self,
        parameter: &'a ParameterDeclaration<'a>,
    ) -> &'a ParameterDeclaration<'a> {
        let span = self.span_of(parameter.node_id);
        let existing = parameter
            .r#type
            .unwrap_or_else(|| self.factory.keyword_type(SyntaxKind::AnyKeyword, span));
        let undefined = self.factory.keyword_type(SyntaxKind::UndefinedKeyword, span);
        let types = self.factory.slice(&[existing, undefined]);
        let union = self.factory.alloc(
            tsr_ast::UnionTypeNode::new(types),
            SyntaxKind::UnionType,
            span,
            NodeFlags::empty(),
        );
        self.factory.alloc(
            ParameterDeclaration::new(
                parameter.modifiers,
                parameter.dot_dot_dot_token,
                parameter.name,
                None,
                Some(TypeNode::UnionTypeNode(union)),
                None,
            ),
            SyntaxKind::Parameter,
            span,
            NodeFlags::empty(),
        )
    }

    /// Ported from `updateAccessorParamList` (`transform.go:1031`).
    fn update_accessor_param_list(
        &mut self,
        parameters: &'a [&'a ParameterDeclaration<'a>],
        is_private: bool,
        span: Span,
    ) -> &'a [&'a ParameterDeclaration<'a>] {
        if !is_private && let Some(parameter) = parameters.first() {
            let parameter = self.ensure_parameter(parameter);
            return self.factory.slice(&[parameter]);
        }
        // "When synthesizing a missing value parameter, emit `value: any` for
        // non-private accessors to match TypeScript's declaration emit behavior."
        let r#type = if is_private {
            None
        } else {
            Some(self.factory.keyword_type(SyntaxKind::AnyKeyword, span))
        };
        let name = self.factory.identifier("value", span);
        let parameter = self.factory.alloc(
            ParameterDeclaration::new(
                &[],
                None,
                Some(tsr_ast::BindingName::Identifier(name)),
                None,
                r#type,
                None,
            ),
            SyntaxKind::Parameter,
            span,
            NodeFlags::empty(),
        );
        self.factory.slice(&[parameter])
    }

    /// Ported from `ensureParameter` (`transform.go:2395`).
    ///
    /// A parameter with an initializer becomes optional, because the initializer
    /// itself cannot be emitted — which is the same fact stated two ways, and the
    /// reason dropping the initializer without adding the `?` would change the
    /// signature.
    fn ensure_parameter(
        &mut self,
        parameter: &'a ParameterDeclaration<'a>,
    ) -> &'a ParameterDeclaration<'a> {
        let span = self.span_of(parameter.node_id);
        let question = if self.resolver.is_optional_parameter(parameter) {
            Some(
                parameter
                    .question_token
                    .unwrap_or_else(|| self.factory.token(SyntaxKind::QuestionToken, span)),
            )
        } else {
            None
        };
        // `ensureType(p, /*ignorePrivate*/ true)`: a parameter property's type is
        // visible even when the property is private.
        let r#type =
            self.ensure_type(parameter.r#type, None, Freshness::Widening, parameter.node_id);
        self.factory.alloc(
            ParameterDeclaration::new(
                &[],
                parameter.dot_dot_dot_token,
                parameter.name,
                question,
                r#type,
                None,
            ),
            SyntaxKind::Parameter,
            span,
            NodeFlags::empty(),
        )
    }

    /// Ported from `ensureType` (`transform.go:1629`), for a declaration whose
    /// type may come from its initializer.
    ///
    /// Upstream's structure is preserved exactly, because the order of its tests
    /// is what decides the output: an existing annotation wins over inference, and
    /// a failed inference becomes `any` rather than nothing.
    fn ensure_type(
        &mut self,
        annotation: Option<TypeNode<'a>>,
        initializer: Option<&Expression<'a>>,
        freshness: Freshness,
        node_id: Option<tsr_ast::NodeId>,
    ) -> Option<TypeNode<'a>> {
        if annotation.is_some() {
            return annotation;
        }
        if let Some(built) =
            self.resolver.create_type_of_declaration(&mut self.factory, initializer, freshness)
        {
            return Some(built);
        }
        // `if typeNode == nil { return NewKeywordTypeNode(KindAnyKeyword) }`.
        let span = self.span_of(node_id);
        self.inference_required.push(span);
        Some(self.factory.keyword_type(SyntaxKind::AnyKeyword, span))
    }

    /// `ensureType`'s function-like branch, which goes to
    /// `CreateReturnTypeOfSignatureDeclaration` rather than
    /// `CreateTypeOfDeclaration`.
    fn ensure_return_type(
        &mut self,
        annotation: Option<TypeNode<'a>>,
        body: Option<&tsr_ast::FunctionBody<'a>>,
        node_id: Option<tsr_ast::NodeId>,
    ) -> Option<TypeNode<'a>> {
        if annotation.is_some() {
            return annotation;
        }
        if matches!(body, Some(tsr_ast::FunctionBody::Block(block)) if block.statements.is_empty())
        {
            let span = self.span_of(node_id);
            return Some(self.factory.keyword_type(SyntaxKind::VoidKeyword, span));
        }
        if let Some(built) =
            self.resolver.create_return_type_of_signature_declaration(&mut self.factory)
        {
            return Some(built);
        }
        let span = self.span_of(node_id);
        self.inference_required.push(span);
        Some(self.factory.keyword_type(SyntaxKind::AnyKeyword, span))
    }

    /// Ported from `ensureNoInitializer` (`transform.go:2421`).
    ///
    /// Returns the value a literal `const` keeps, and `None` for everything else —
    /// which is the overwhelming majority, since an initializer in a `.d.ts` is
    /// the exception rather than the rule.
    fn ensure_no_initializer(&mut self, host: LiteralConstHost<'_, 'a>) -> Option<Expression<'a>> {
        if !self.resolver.is_literal_const_declaration(host) {
            return None;
        }
        self.resolver.create_literal_const_value(&mut self.factory, host)
    }

    /// The initializer a folded enum value emits, from `transformEnumDeclaration`.
    ///
    /// Upstream's arms are reproduced exactly, including the two that are not
    /// literals at all: an infinite value emits the *identifier* `Infinity`, and a
    /// `NaN` emits the identifier `NaN`, because neither has a literal spelling.
    fn enum_initializer(
        &mut self,
        value: &crate::enum_value::EnumValue,
        span: Span,
    ) -> Expression<'a> {
        use crate::enum_value::EnumValue;
        match value {
            EnumValue::String(text) => {
                let text = self.factory.alloc_str(text);
                Expression::StringLiteral(self.factory.alloc(
                    tsr_ast::StringLiteral::new(text, tsr_ast::TokenFlags::empty()),
                    SyntaxKind::StringLiteral,
                    span,
                    NodeFlags::empty(),
                ))
            }
            EnumValue::Number(number) if number.is_nan() => {
                Expression::Identifier(self.factory.identifier("NaN", span))
            }
            EnumValue::Number(number) if number.is_infinite() => {
                let name = self.factory.identifier("Infinity", span);
                let identifier = Expression::Identifier(name);
                if *number > 0.0 { identifier } else { self.negate(identifier, span) }
            }
            EnumValue::Number(number) => {
                let magnitude = self.numeric_literal(number.abs(), span);
                if *number < 0.0 { self.negate(magnitude, span) } else { magnitude }
            }
        }
    }

    fn numeric_literal(&mut self, value: f64, span: Span) -> Expression<'a> {
        let text = crate::type_builder::format_number(value);
        let text = self.factory.alloc_str(&text);
        Expression::NumericLiteral(self.factory.alloc(
            tsr_ast::NumericLiteral::new(text, tsr_ast::TokenFlags::empty()),
            SyntaxKind::NumericLiteral,
            span,
            NodeFlags::empty(),
        ))
    }

    fn negate(&mut self, operand: Expression<'a>, span: Span) -> Expression<'a> {
        let operator = self.factory.token(SyntaxKind::MinusToken, span);
        Expression::PrefixUnaryExpression(self.factory.alloc(
            tsr_ast::PrefixUnaryExpression::new(operator, Some(operand)),
            SyntaxKind::PrefixUnaryExpression,
            span,
            NodeFlags::empty(),
        ))
    }

    fn ensure_modifiers(
        &mut self,
        existing: &'a [ModifierLike<'a>],
        node_id: Option<tsr_ast::NodeId>,
        parent_is_file: bool,
        is_always_type: bool,
    ) -> &'a [ModifierLike<'a>] {
        let span = self.span_of(node_id);
        modifiers::ensure_modifiers(
            &mut self.factory,
            existing,
            span,
            self.needs_declare,
            parent_is_file,
            is_always_type,
        )
    }

    fn span_of(&self, node_id: Option<tsr_ast::NodeId>) -> Span {
        self.factory.span_of(node_id)
    }
}

/// `ast.ModifierFlagsParameterPropertyModifier`.
const PARAMETER_PROPERTY_MODIFIER: ModifierFlags = ModifierFlags::PUBLIC
    .union(ModifierFlags::PRIVATE)
    .union(ModifierFlags::PROTECTED)
    .union(ModifierFlags::READONLY)
    .union(ModifierFlags::OVERRIDE);

fn reserve_statement_names(statements: &[Statement<'_>], used: &mut HashSet<String>) {
    for statement in statements {
        let name = match statement {
            Statement::ClassDeclaration(node) => node.name,
            Statement::FunctionDeclaration(node) => node.name,
            Statement::InterfaceDeclaration(node) => node.name,
            Statement::TypeAliasDeclaration(node) => node.name,
            Statement::EnumDeclaration(node) => node.name,
            Statement::ImportEqualsDeclaration(node) => node.name,
            _ => None,
        };
        if let Some(name) = name {
            used.insert(name.text.to_string());
        }
        if let Statement::ModuleDeclaration(node) = statement
            && let Some(tsr_ast::ModuleName::Identifier(name)) = node.name
        {
            used.insert(name.text.to_string());
        }
        if let Statement::VariableStatement(node) = statement
            && let Some(list) = node.declaration_list
        {
            for declaration in list.declarations {
                reserve_binding_name(declaration.name, used);
            }
        }
    }
}

fn reserve_binding_name(name: Option<tsr_ast::BindingName<'_>>, used: &mut HashSet<String>) {
    match name {
        Some(tsr_ast::BindingName::Identifier(identifier)) => {
            used.insert(identifier.text.to_string());
        }
        Some(tsr_ast::BindingName::BindingPattern(pattern)) => {
            for element in pattern.elements {
                reserve_binding_name(element.name, used);
            }
        }
        None => {}
    }
}

/// Ported from `ast.IsExternalModuleIndicator` (`internal/ast/utilities.go:1677`).
fn is_external_module_indicator(statement: &Statement<'_>) -> bool {
    matches!(
        statement,
        Statement::ImportDeclaration(_)
            | Statement::ImportEqualsDeclaration(_)
            | Statement::ExportDeclaration(_)
            | Statement::ExportAssignment(_)
    ) || statement_modifiers(statement)
        .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::ExportKeyword))
}

/// Ported from `isScopeMarker` (`internal/transformers/declarations/util.go`).
fn is_scope_marker(statement: &Statement<'_>) -> bool {
    matches!(statement, Statement::ExportAssignment(_) | Statement::ExportDeclaration(_))
}

/// Ported from `needsScopeMarker` (`internal/transformers/declarations/util.go`).
fn needs_scope_marker(statement: &Statement<'_>) -> bool {
    !is_external_module_indicator(statement) && !is_ambient_module(statement)
}

/// Ported from `ast.IsAmbientModule` (`internal/ast/utilities.go:1652`).
fn is_ambient_module(statement: &Statement<'_>) -> bool {
    let Statement::ModuleDeclaration(module) = statement else { return false };
    matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
        || module.keyword.kind == SyntaxKind::GlobalKeyword
}

/// Whether the file is a module at all, which decides whether the scope marker is
/// even a question. Ported from `ast.IsExternalOrCommonJSModule`.
fn is_external_module(statements: &[Statement<'_>]) -> bool {
    statements.iter().any(is_external_module_indicator)
}

/// The modifier list of a statement, when it can have one.
fn statement_modifiers<'a>(statement: &Statement<'a>) -> Option<&'a [ModifierLike<'a>]> {
    Some(match statement {
        Statement::ClassDeclaration(node) => node.modifiers,
        Statement::FunctionDeclaration(node) => node.modifiers,
        Statement::InterfaceDeclaration(node) => node.modifiers,
        Statement::TypeAliasDeclaration(node) => node.modifiers,
        Statement::EnumDeclaration(node) => node.modifiers,
        Statement::ModuleDeclaration(node) => node.modifiers,
        Statement::VariableStatement(node) => node.modifiers,
        Statement::ImportEqualsDeclaration(node) => node.modifiers,
        Statement::ImportDeclaration(node) => node.modifiers,
        Statement::ExportDeclaration(node) => node.modifiers,
        Statement::ExportAssignment(node) => node.modifiers,
        _ => return None,
    })
}

/// Ported from `ast.ReplaceModifiers` (`internal/ast/utilities.go`), for the
/// statement kinds `stripExportModifiers` can reach.
fn replace_modifiers<'a>(
    factory: &mut Factory<'a, '_>,
    statement: &Statement<'a>,
    modifiers: &'a [ModifierLike<'a>],
) -> Statement<'a> {
    let span = factory.span_of(statement.node_id());
    match statement {
        Statement::ClassDeclaration(node) => Statement::ClassDeclaration(factory.alloc(
            tsr_ast::ClassDeclaration::new(
                modifiers,
                node.name,
                node.type_parameters,
                node.heritage_clauses,
                node.members,
            ),
            SyntaxKind::ClassDeclaration,
            span,
            NodeFlags::empty(),
        )),
        Statement::FunctionDeclaration(node) => Statement::FunctionDeclaration(factory.alloc(
            tsr_ast::FunctionDeclaration::new(
                modifiers,
                node.asterisk_token,
                node.name,
                node.type_parameters,
                node.parameters,
                node.r#type,
                node.full_signature,
                node.body,
            ),
            SyntaxKind::FunctionDeclaration,
            span,
            NodeFlags::empty(),
        )),
        Statement::InterfaceDeclaration(node) => Statement::InterfaceDeclaration(factory.alloc(
            tsr_ast::InterfaceDeclaration::new(
                modifiers,
                node.name,
                node.type_parameters,
                node.heritage_clauses,
                node.members,
            ),
            SyntaxKind::InterfaceDeclaration,
            span,
            NodeFlags::empty(),
        )),
        Statement::TypeAliasDeclaration(node) => Statement::TypeAliasDeclaration(factory.alloc(
            tsr_ast::TypeAliasDeclaration::new(
                modifiers,
                node.name,
                node.type_parameters,
                node.r#type,
            ),
            SyntaxKind::TypeAliasDeclaration,
            span,
            NodeFlags::empty(),
        )),
        Statement::EnumDeclaration(node) => Statement::EnumDeclaration(factory.alloc(
            tsr_ast::EnumDeclaration::new(modifiers, node.name, node.members),
            SyntaxKind::EnumDeclaration,
            span,
            NodeFlags::empty(),
        )),
        Statement::ModuleDeclaration(node) => Statement::ModuleDeclaration(factory.alloc(
            tsr_ast::ModuleDeclaration::new(
                modifiers,
                node.keyword,
                node.name,
                node.body,
                node.asterisk_token,
            ),
            SyntaxKind::ModuleDeclaration,
            span,
            NodeFlags::empty(),
        )),
        Statement::VariableStatement(node) => Statement::VariableStatement(factory.alloc(
            tsr_ast::VariableStatement::new(modifiers, node.declaration_list),
            SyntaxKind::VariableStatement,
            span,
            NodeFlags::empty(),
        )),
        other => *other,
    }
}

/// The name of a class member, when it has one.
fn class_member_name<'b, 'a>(
    member: &'b ClassElement<'a>,
) -> Option<&'b tsr_ast::PropertyName<'a>> {
    match member {
        ClassElement::PropertyDeclaration(node) => Some(&node.name),
        ClassElement::MethodDeclaration(node) => Some(&node.name),
        ClassElement::GetAccessorDeclaration(node) => Some(&node.name),
        ClassElement::SetAccessorDeclaration(node) => Some(&node.name),
        _ => None,
    }
}

fn property_names_equal(
    left: &tsr_ast::PropertyName<'_>,
    right: &tsr_ast::PropertyName<'_>,
) -> bool {
    use tsr_ast::PropertyName;

    match (left, right) {
        (PropertyName::BigIntLiteral(left), PropertyName::BigIntLiteral(right)) => {
            left.text == right.text
        }
        (PropertyName::Identifier(left), PropertyName::Identifier(right)) => {
            left.text == right.text
        }
        (
            PropertyName::NoSubstitutionTemplateLiteral(left),
            PropertyName::NoSubstitutionTemplateLiteral(right),
        ) => left.text == right.text,
        (PropertyName::NumericLiteral(left), PropertyName::NumericLiteral(right)) => {
            left.text == right.text
        }
        (PropertyName::PrivateIdentifier(left), PropertyName::PrivateIdentifier(right)) => {
            left.text == right.text
        }
        (PropertyName::StringLiteral(left), PropertyName::StringLiteral(right)) => {
            left.text == right.text
        }
        _ => false,
    }
}

fn is_entity_name_expression(expression: &Expression<'_>) -> bool {
    matches!(expression, Expression::Identifier(_) | Expression::PropertyAccessExpression(_))
}
