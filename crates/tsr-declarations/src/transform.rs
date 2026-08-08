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
//! | `transformCommonJSExport`, `visitCJSExportAssignments` (`:1326`, `:2672`) | `CommonJS` `module.exports =` emit. Needs the `Program` to know the module kind |
//! | `visitThisPropertyAssignments`, `collectThisPropertyAssignments` (`:2072`, `:2163`) | JS-file only, and JSDoc-driven |
//! | The `JSDoc*` transform arms (`:2576`–`:2632`) | JS-file only |
//! | `CreateLateBoundIndexSignatures` in `buildClassMembers` (`:1918`) | Purely a checker product |
//! | `getReferencedFiles` path rewriting (`:464`) | Needs the output path, which needs the `Program` |
//!
//! Function expandos whose host and property name are both syntactically known
//! are handled below. Checker-only expando classification and assignments hidden
//! in nested control flow remain outside this pass.
//!
//! [`EmitResolver`]: crate::EmitResolver

use std::collections::{HashMap, HashSet};

use tsr_ast::{
    ClassElement, Expression, ModifierFlags, ModifierLike, NodeFlags, ParameterDeclaration,
    SourceFile, Statement, SyntaxKind, TypeElement, TypeNode,
};
use tsr_core::Span;

use crate::{
    DeclarationEmitOptions, Freshness,
    factory::Factory,
    modifiers,
    resolver::{EmitResolver, LiteralConstHost, has_modifier, is_private_member},
};

#[derive(Clone, Copy)]
struct ExpandoMember<'a> {
    /// The declaration-space name when the computed key can be represented as
    /// an identifier. A late-bound assignment still creates the function's
    /// namespace when this is `None`, but contributes no namespace member.
    name: Option<&'a str>,
    initializer: Option<Expression<'a>>,
    node_id: Option<tsr_ast::NodeId>,
}

/// Ported from `DeclarationTransformer` (`transform.go:63`).
///
/// Upstream's struct has 27 fields; the ones absent here are the visitors it
/// builds in its constructor (this port dispatches directly), the diagnostic
/// context stack (`getSymbolAccessibilityDiagnostic`, which exists to name a
/// *checker* error), and the `CommonJS` maps listed as out of scope in the module
/// docs.
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
    /// Syntactically named property assignments attached to function-valued
    /// variables, grouped by their host binding.
    expando_members: HashMap<String, Vec<ExpandoMember<'a>>>,
    /// Non-generic top-level aliases whose written target can be followed
    /// without checker inference.
    type_aliases: HashMap<String, TypeNode<'a>>,
    /// Syntax-only compiler options and the source trivia they inspect.
    options: DeclarationEmitOptions<'a>,
    /// Whether the file being transformed is a JavaScript file, where JSDoc
    /// `@protected`/`@private` tags act as accessibility modifiers.
    javascript_file: bool,
    /// `@param {T} name` types from the enclosing signature's JSDoc, active
    /// while that signature's parameters are ensured. JavaScript files only.
    jsdoc_param_types: HashMap<String, TypeNode<'a>>,
    /// Nested `host.p.q = value` assignments on empty-object consts, keyed by
    /// the host binding. JavaScript files only
    /// (`typeFromPropertyAssignment39`).
    object_expandos: HashMap<String, ObjectExpando<'a>>,
    /// Names bound by `import * as N`; destructuring one emits `typeof N`.
    namespace_imports: HashSet<String>,
    /// Where the resolver was asked for a type and had none.
    ///
    /// No upstream counterpart: upstream's resolver always answers. This is what
    /// makes "the emitter guessed" a measurable event rather than a silent `any`
    /// in the output.
    pub(crate) inference_required: Vec<Span>,
}

impl<'a, 't, R: EmitResolver<'a>> Transformer<'a, 't, R> {
    pub(crate) fn new(
        factory: Factory<'a, 't>,
        resolver: R,
        options: DeclarationEmitOptions<'a>,
    ) -> Self {
        Self {
            factory,
            resolver,
            needs_declare: true,
            needs_scope_fix_marker: false,
            result_has_scope_marker: false,
            result_has_external_module_indicator: false,
            used_names: HashSet::new(),
            ambient_context: false,
            expando_members: HashMap::new(),
            type_aliases: HashMap::new(),
            options,
            javascript_file: false,
            jsdoc_param_types: HashMap::new(),
            object_expandos: HashMap::new(),
            namespace_imports: HashSet::new(),
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
        self.javascript_file =
            self.factory.flags_of(file.node_id).contains(NodeFlags::JAVASCRIPT_FILE);
        let is_module = is_external_module(file.statements) || self.options.force_module;
        reserve_statement_names(file.statements, &mut self.used_names);
        self.expando_members = collect_expando_members(file.statements, self.factory.nodes());
        self.collect_object_expandos(file.statements);
        self.namespace_imports = file
            .statements
            .iter()
            .filter_map(|statement| {
                let Statement::ImportDeclaration(import) = statement else { return None };
                let clause = import.import_clause?;
                let tsr_ast::NamedImportBindings::NamespaceImport(namespace) =
                    clause.named_bindings.as_ref()?
                else {
                    return None;
                };
                Some(namespace.name?.text.to_string())
            })
            .collect();
        self.type_aliases = file
            .statements
            .iter()
            .filter_map(|statement| {
                let Statement::TypeAliasDeclaration(alias) = statement else { return None };
                if !alias.type_parameters.is_empty() {
                    return None;
                }
                Some((alias.name?.text.to_string(), alias.r#type?))
            })
            .collect();

        // Each synthesized alias goes before the top-level statement its
        // declaring comment sits in (or above) — a `@typedef` inside a class
        // body hoists before the class, one between statements stays between
        // them.
        let mut aliases = self.synthesize_jsdoc_aliases(is_module).into_iter().peekable();
        let mut statements: Vec<Statement<'a>> = Vec::with_capacity(file.statements.len());
        for statement in file.statements {
            let statement_end = self.span_of(statement.node_id()).end as usize;
            while let Some((position, _)) = aliases.peek() {
                if *position < statement_end {
                    let (_, alias) = aliases.next().expect("peeked");
                    statements.push(alias);
                } else {
                    break;
                }
            }
            if self.should_strip_internal(statement.node_id()) {
                continue;
            }
            if is_function_overload_implementation(statement, file.statements) {
                continue;
            }
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
        statements.extend(aliases.map(|(_, alias)| alias));

        // Visibility is initially computed from source syntax so declarations
        // needed by an emitted type are available to the transformer. Recompute
        // it from the declaration syntax afterward: private class members have
        // lost their types by now and must no longer keep otherwise-private
        // declarations alive. This is the file-scope counterpart of the same
        // second-stage pass used for source namespace bodies below.
        if is_module {
            let visible = tsr_dts::visibility::visible_module_members(&statements);
            statements.retain(|statement| {
                visible.contains(statement.node_id())
                    || matches!(
                        statement,
                        Statement::ExportDeclaration(_) | Statement::ExportAssignment(_)
                    )
                    || matches!(
                        statement,
                        Statement::ImportDeclaration(import) if import.import_clause.is_none()
                    )
            });
            // A statement can be visible for one declarator only: `var x = 10,
            // m2: T` with `export = m2` keeps `m2` and drops `x`. Upstream
            // filters per binding name (`getBindingNameVisible`,
            // `transform.go:2216`).
            statements = statements
                .iter()
                .map(|statement| self.prune_invisible_declarators(statement, &visible))
                .collect();
            self.result_has_external_module_indicator =
                statements.iter().any(is_external_module_indicator);
            self.needs_scope_fix_marker = statements.iter().any(needs_scope_marker);
            self.result_has_scope_marker = statements.iter().any(is_scope_marker);
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
        if clause.phase_modifier.is_none_or(|modifier| modifier.kind != SyntaxKind::DeferKeyword) {
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
        // A literal keeps its value as the synthesized const's initializer —
        // `export default 0` emits `declare const _default = 0;` — and only a
        // non-literal widens into a type annotation.
        let literal = node.expression.as_ref().and_then(|expression| {
            self.ensure_no_initializer(LiteralConstHost::Expression(expression))
        });
        let r#type = if literal.is_some() {
            None
        } else {
            self.ensure_type(None, node.expression.as_ref(), Freshness::Widening, node.node_id)
        };
        let declaration = self.factory.alloc(
            tsr_ast::VariableDeclaration::new(
                Some(tsr_ast::BindingName::Identifier(name)),
                None,
                r#type,
                literal,
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
                let r#type = node.r#type.map(|r#type| self.transform_written_type(r#type));
                self.needs_declare = saved;
                vec![Statement::TypeAliasDeclaration(self.factory.alloc(
                    tsr_ast::TypeAliasDeclaration::new(
                        modifiers,
                        node.name,
                        node.type_parameters,
                        r#type,
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
                let expando_members =
                    node.name.and_then(|name| self.expando_members.get(name.text).cloned());
                let rewrites_default = expando_members.is_some()
                    && has_modifier(node.modifiers, SyntaxKind::DefaultKeyword);
                let modifiers = if rewrites_default {
                    let span = self.span_of(node.node_id);
                    let flags = (modifiers::modifier_flags(node.modifiers)
                        & !(ModifierFlags::EXPORT | ModifierFlags::DEFAULT))
                        | ModifierFlags::AMBIENT;
                    let created =
                        modifiers::create_modifiers_from_flags(&mut self.factory, flags, span);
                    self.factory.slice(&created)
                } else {
                    self.ensure_modifiers(node.modifiers, node.node_id, parent_is_file, false)
                };
                let parameters = self.update_param_list(node.parameters, false, node.node_id);
                let return_type =
                    self.ensure_return_type(node.r#type, node.body.as_ref(), node.node_id);
                let function = Statement::FunctionDeclaration(self.factory.alloc(
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
                ));
                let mut result = vec![function];
                if let Some(name) = node.name
                    && let Some(members) = expando_members
                {
                    result.push(self.create_expando_namespace(
                        name,
                        modifiers,
                        self.span_of(node.node_id),
                        &members,
                    ));
                    if rewrites_default {
                        result.push(Statement::ExportAssignment(self.factory.alloc(
                            tsr_ast::ExportAssignment::new(
                                &[],
                                false,
                                None,
                                Some(Expression::Identifier(name)),
                            ),
                            SyntaxKind::ExportAssignment,
                            self.span_of(node.node_id),
                            NodeFlags::empty(),
                        )));
                    }
                }
                result
            }
            Statement::ClassDeclaration(node) => {
                self.transform_class_declaration(node, parent_is_file)
            }
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
            Statement::VariableStatement(node) => {
                self.transform_variable_statement(node, parent_is_file)
            }
            Statement::ModuleDeclaration(node) => {
                self.transform_module_declaration(node, parent_is_file)
            }
            // `transformTopLevelDeclaration`'s default arm panics upstream,
            // because its dispatch has already filtered the kinds. Here the caller
            // is the same dispatch, so this is equally unreachable — and eliding
            // rather than panicking keeps a corpus run from dying on one case.
            _ => Vec::new(),
        }
    }

    /// Drop variable declarators visibility never passed through, keeping the
    /// statement itself when at least one declarator is reachable.
    fn prune_invisible_declarators(
        &mut self,
        statement: &Statement<'a>,
        visible: &tsr_dts::visibility::Visible,
    ) -> Statement<'a> {
        let Statement::VariableStatement(node) = statement else { return *statement };
        let Some(list) = node.declaration_list else { return *statement };
        let kept: Vec<_> = list
            .declarations
            .iter()
            .copied()
            .filter(|declaration| match &declaration.name {
                Some(tsr_ast::BindingName::Identifier(name)) => visible.reaches_name(name.text),
                // A binding pattern introduces several names; filtering it
                // partially would change its shape, so it stays whole.
                _ => true,
            })
            .collect();
        if kept.len() == list.declarations.len() || kept.is_empty() {
            return *statement;
        }
        let declarations = self.factory.slice(&kept);
        let list_flags = self.factory.flags_of(list.node_id);
        let new_list = self.factory.alloc(
            tsr_ast::VariableDeclarationList::new(declarations),
            SyntaxKind::VariableDeclarationList,
            self.span_of(list.node_id),
            list_flags,
        );
        Statement::VariableStatement(self.factory.alloc(
            tsr_ast::VariableStatement::new(node.modifiers, Some(new_list)),
            SyntaxKind::VariableStatement,
            self.span_of(node.node_id),
            NodeFlags::empty(),
        ))
    }

    /// Ported from `transformVariableStatement` (`transform.go:2207`).
    fn transform_variable_statement(
        &mut self,
        node: &tsr_ast::VariableStatement<'a>,
        parent_is_file: bool,
    ) -> Vec<Statement<'a>> {
        let Some(list) = node.declaration_list else { return Vec::new() };
        if let Some(promoted) = self.promote_expando_function(node, list, parent_is_file) {
            return promoted;
        }
        let flags = self.factory.flags_of(list.node_id);
        let is_const = flags.intersects(NodeFlags::CONSTANT);

        // In JavaScript, a single-declarator statement's leading `@type` JSDoc
        // annotates that declarator (`callbackOnConstructor`'s `ooscope2`).
        let jsdoc_annotation = if list.declarations.len() == 1 {
            let span = self.span_of(node.node_id);
            self.leading_jsdoc(node.node_id)
                .and_then(|comment| self.jsdoc_type(comment, "type", span))
        } else {
            None
        };

        let mut declarations = Vec::with_capacity(list.declarations.len());
        for declaration in list.declarations {
            if declaration.name.is_some_and(|name| !binding_name_has_bindings(name)) {
                continue;
            }
            if declaration.name.is_some_and(binding_name_contains_initializer) {
                let mut names = Vec::new();
                collect_binding_identifiers(declaration.name, &mut names);
                for name in names {
                    let span = self.span_of(name.node_id);
                    let r#type = self.factory.keyword_type(SyntaxKind::AnyKeyword, span);
                    declarations.push(self.factory.alloc(
                        tsr_ast::VariableDeclaration::new(
                            Some(tsr_ast::BindingName::Identifier(name)),
                            None,
                            Some(r#type),
                            None,
                        ),
                        SyntaxKind::VariableDeclaration,
                        span,
                        NodeFlags::empty(),
                    ));
                }
            } else {
                declarations.push(self.transform_variable_declaration(
                    declaration,
                    is_const,
                    jsdoc_annotation,
                ));
            }
        }
        if declarations.is_empty() {
            return Vec::new();
        }
        let declarations = self.factory.slice(&declarations);

        let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, parent_is_file, false);
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

    fn promote_expando_function(
        &mut self,
        statement: &tsr_ast::VariableStatement<'a>,
        list: &'a tsr_ast::VariableDeclarationList<'a>,
        parent_is_file: bool,
    ) -> Option<Vec<Statement<'a>>> {
        let [declaration] = list.declarations else { return None };
        let Some(tsr_ast::BindingName::Identifier(name)) = declaration.name else { return None };
        let initializer = declaration.initializer.as_ref()?;
        if !matches!(initializer, Expression::ArrowFunction(_) | Expression::FunctionExpression(_))
        {
            return None;
        }
        let members = self.expando_members.get(name.text)?.clone();
        if members.iter().any(|member| member.name.is_none()) {
            return None;
        }
        let span = self.span_of(declaration.node_id);
        let TypeNode::FunctionTypeNode(function_type) =
            self.ensure_type(None, Some(initializer), Freshness::Widening, declaration.node_id)?
        else {
            return None;
        };
        let modifiers =
            self.ensure_modifiers(statement.modifiers, statement.node_id, parent_is_file, false);
        let function = Statement::FunctionDeclaration(self.factory.alloc(
            tsr_ast::FunctionDeclaration::new(
                modifiers,
                None,
                Some(name),
                function_type.type_parameters,
                function_type.parameters,
                function_type.r#type,
                None,
                None,
            ),
            SyntaxKind::FunctionDeclaration,
            span,
            NodeFlags::empty(),
        ));

        let namespace = self.create_expando_namespace(name, modifiers, span, &members);
        Some(vec![function, namespace])
    }

    fn create_expando_namespace(
        &mut self,
        name: &'a tsr_ast::Identifier<'a>,
        modifiers: &'a [ModifierLike<'a>],
        span: Span,
        members: &[ExpandoMember<'a>],
    ) -> Statement<'a> {
        let mut namespace_statements = Vec::with_capacity(members.len());
        for member in members {
            let Some(name) = member.name else { continue };
            let member_span = self.span_of(member.node_id);
            let member_name = self.factory.identifier(name, member_span);
            let member_type = self.ensure_type(
                None,
                member.initializer.as_ref(),
                Freshness::Widening,
                member.node_id,
            );
            let declaration = self.factory.alloc(
                tsr_ast::VariableDeclaration::new(
                    Some(tsr_ast::BindingName::Identifier(member_name)),
                    None,
                    member_type,
                    None,
                ),
                SyntaxKind::VariableDeclaration,
                member_span,
                NodeFlags::empty(),
            );
            let declarations = self.factory.slice(&[declaration]);
            let list = self.factory.alloc(
                tsr_ast::VariableDeclarationList::new(declarations),
                SyntaxKind::VariableDeclarationList,
                member_span,
                NodeFlags::empty(),
            );
            namespace_statements.push(Statement::VariableStatement(self.factory.alloc(
                tsr_ast::VariableStatement::new(&[], Some(list)),
                SyntaxKind::VariableStatement,
                member_span,
                NodeFlags::empty(),
            )));
        }
        let namespace_statements = self.factory.slice(&namespace_statements);
        let block = self.factory.alloc(
            tsr_ast::ModuleBlock::new(namespace_statements),
            SyntaxKind::ModuleBlock,
            span,
            NodeFlags::empty(),
        );
        let namespace_keyword = self.factory.token(SyntaxKind::NamespaceKeyword, span);
        Statement::ModuleDeclaration(self.factory.alloc(
            tsr_ast::ModuleDeclaration::new(
                modifiers,
                namespace_keyword,
                Some(tsr_ast::ModuleName::Identifier(name)),
                Some(tsr_ast::ModuleBody::ModuleBlock(block)),
                None,
            ),
            SyntaxKind::ModuleDeclaration,
            span,
            NodeFlags::empty(),
        ))
    }

    /// Ported from `transformVariableDeclaration` (`transform.go:835`).
    fn transform_variable_declaration(
        &mut self,
        declaration: &'a tsr_ast::VariableDeclaration<'a>,
        is_const: bool,
        jsdoc_annotation: Option<TypeNode<'a>>,
    ) -> &'a tsr_ast::VariableDeclaration<'a> {
        let host = LiteralConstHost::Variable(declaration, is_const);
        // A JSDoc `@type` is a written annotation: it wins over the
        // literal-const initializer form just as a written one would.
        let initializer =
            if jsdoc_annotation.is_some() { None } else { self.ensure_no_initializer(host) };
        let annotation = declaration
            .r#type
            .map(|r#type| self.transform_written_type(r#type))
            .or(jsdoc_annotation)
            .or_else(|| {
                // An empty-object const with collected property assignments
                // takes the expando tree as its type.
                let Some(tsr_ast::BindingName::Identifier(name)) = &declaration.name else {
                    return None;
                };
                let empty_object = matches!(
                    declaration.initializer.as_ref(),
                    Some(Expression::ObjectLiteralExpression(literal))
                        if literal.properties.is_empty()
                );
                if !empty_object {
                    return None;
                }
                let expando = self.object_expandos.remove(name.text)?;
                let span = self.span_of(declaration.node_id);
                Some(self.object_expando_type(expando, span))
            })
            .or_else(|| {
                // A binding pattern destructuring a namespace import keeps its
                // shape, typed by `typeof` that entity: `const { Foo } = A`
                // emits `declare const { Foo }: typeof A;`
                // (`declarationEmitExpressionInExtends6`). Other entities need
                // the checker's member types and stay in the inference bucket.
                let Some(tsr_ast::BindingName::BindingPattern(_)) = &declaration.name else {
                    return None;
                };
                let initializer = declaration.initializer.as_ref()?;
                let root = property_path(initializer)?.0;
                if !self.namespace_imports.contains(&root) {
                    return None;
                }
                let entity = self.entity_of_expression(initializer)?;
                let span = self.span_of(declaration.node_id);
                Some(TypeNode::TypeQueryNode(self.factory.alloc(
                    tsr_ast::TypeQueryNode::new(Some(entity), &[]),
                    SyntaxKind::TypeQuery,
                    span,
                    NodeFlags::empty(),
                )))
            });
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
                annotation,
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

    fn transform_written_type(&mut self, r#type: TypeNode<'a>) -> TypeNode<'a> {
        match r#type {
            TypeNode::TypeLiteralNode(literal) => {
                let members = self.visit_type_members(literal.members);
                TypeNode::TypeLiteralNode(self.factory.alloc(
                    tsr_ast::TypeLiteralNode::new(members),
                    SyntaxKind::TypeLiteral,
                    self.span_of(literal.node_id),
                    NodeFlags::empty(),
                ))
            }
            TypeNode::MappedTypeNode(mapped) => {
                let span = self.span_of(mapped.node_id);
                let name_type = mapped.name_type.map(|name| self.transform_written_type(name));
                let value_type = match mapped.r#type {
                    Some(value) => self.transform_written_type(value),
                    None => self.factory.keyword_type(SyntaxKind::AnyKeyword, span),
                };
                TypeNode::MappedTypeNode(self.factory.alloc(
                    tsr_ast::MappedTypeNode::new(
                        mapped.readonly_token,
                        mapped.type_parameter,
                        name_type,
                        mapped.question_token,
                        Some(value_type),
                        mapped.members,
                    ),
                    SyntaxKind::MappedType,
                    span,
                    NodeFlags::empty(),
                ))
            }
            TypeNode::ParenthesizedTypeNode(parenthesized) => {
                let inner = parenthesized.r#type.map(|inner| self.transform_written_type(inner));
                TypeNode::ParenthesizedTypeNode(self.factory.alloc(
                    tsr_ast::ParenthesizedTypeNode::new(inner),
                    SyntaxKind::ParenthesizedType,
                    self.span_of(parenthesized.node_id),
                    NodeFlags::empty(),
                ))
            }
            TypeNode::ConditionalTypeNode(conditional) => {
                let check = conditional.check_type.map(|node| self.transform_written_type(node));
                let extends =
                    conditional.extends_type.map(|node| self.transform_written_type(node));
                let true_type = conditional.true_type.map(|node| self.transform_written_type(node));
                let false_type =
                    conditional.false_type.map(|node| self.transform_written_type(node));
                TypeNode::ConditionalTypeNode(self.factory.alloc(
                    tsr_ast::ConditionalTypeNode::new(check, extends, true_type, false_type),
                    SyntaxKind::ConditionalType,
                    self.span_of(conditional.node_id),
                    NodeFlags::empty(),
                ))
            }
            _ => r#type,
        }
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
        parent_is_file: bool,
    ) -> Vec<Statement<'a>> {
        let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, parent_is_file, false);
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
                    if is_function_overload_implementation(statement, block.statements) {
                        continue;
                    }
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
                // A source namespace is a module scope: only exported members and
                // the declarations their emitted syntax references survive. Run
                // this after transformation so a type erased from a private class
                // member cannot keep a private import alias alive.
                if !enters_ambient {
                    let visible = tsr_dts::visibility::visible_module_members(&statements);
                    statements.retain(|statement| {
                        visible.contains(statement.node_id())
                            || matches!(
                                statement,
                                Statement::ExportDeclaration(_) | Statement::ExportAssignment(_)
                            )
                    });
                    self.needs_scope_fix_marker = statements.iter().any(needs_scope_marker);
                    self.result_has_scope_marker = statements.iter().any(is_scope_marker);
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
                let inner = self.transform_module_declaration(inner, false);
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
        parent_is_file: bool,
    ) -> Vec<Statement<'a>> {
        let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, parent_is_file, false);
        let (base_variable, heritage_clauses) = self.rewrite_class_base(node);
        let heritage_clauses = self.with_jsdoc_implements(node, heritage_clauses);
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
                    && !self.should_strip_internal(parameter.node_id)
                {
                    members.extend(self.parameter_properties(parameter));
                }
            }
        }

        // In JavaScript, `this.X = …` in the constructor body declares a class
        // property; upstream binds these as members and emits them before the
        // constructor.
        if self.javascript_file
            && let Some(ClassElement::ConstructorDeclaration(constructor)) = node
                .members
                .iter()
                .find(|member| matches!(member, ClassElement::ConstructorDeclaration(_)))
        {
            members.extend(self.this_assignment_properties(constructor));
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
            if self.should_strip_internal(member.node_id()) {
                continue;
            }
            if matches!(member, ClassElement::ConstructorDeclaration(constructor) if constructor.body.is_some())
                && has_constructor_overloads
            {
                continue;
            }
            if let ClassElement::MethodDeclaration(method) = member {
                let is_static = has_modifier(method.modifiers, SyntaxKind::StaticKeyword);
                let has_overload = node.members.iter().any(|candidate| {
                    let ClassElement::MethodDeclaration(candidate) = candidate else {
                        return false;
                    };
                    candidate.body.is_none()
                        && has_modifier(candidate.modifiers, SyntaxKind::StaticKeyword) == is_static
                        && property_names_equal(&candidate.name, &method.name)
                });
                if method.body.is_some() && has_overload {
                    continue;
                }
                if is_private_member(member) {
                    if private_method_markers.iter().any(|(seen_static, seen_name)| {
                        *seen_static == is_static && property_names_equal(seen_name, &method.name)
                    }) {
                        continue;
                    }
                    private_method_markers.push((is_static, method.name));
                }
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

    /// The properties a parameter property declares, from `buildClassMembers`.
    fn parameter_properties(
        &mut self,
        parameter: &'a ParameterDeclaration<'a>,
    ) -> Vec<ClassElement<'a>> {
        let Some(name) = parameter.name else { return Vec::new() };
        let span = self.span_of(parameter.node_id);
        let modifiers = self.ensure_modifiers(parameter.modifiers, parameter.node_id, false, false);
        // `ensureType(param, /*ignorePrivate*/ false)` (`transform.go:1933`). The
        // *property* a private parameter property declares emits no type, while
        // the constructor parameter it came from keeps one — `ensureParameter`
        // passes `ignorePrivate: true` for exactly that reason. Emitting the type
        // in both places leaks a private member's shape, and parses.
        let private = has_modifier(parameter.modifiers, SyntaxKind::PrivateKeyword);
        let r#type = if private {
            None
        } else {
            self.ensure_type(parameter.r#type, None, Freshness::Widening, parameter.node_id).map(
                |r#type| {
                    if parameter.question_token.is_some() {
                        self.include_undefined_type(r#type, span)
                    } else {
                        r#type
                    }
                },
            )
        };
        let mut properties = Vec::new();
        self.collect_parameter_properties(
            name,
            r#type,
            private,
            modifiers,
            parameter.question_token,
            &mut properties,
        );
        properties
    }

    fn collect_parameter_properties(
        &mut self,
        name: tsr_ast::BindingName<'a>,
        r#type: Option<TypeNode<'a>>,
        private: bool,
        modifiers: &'a [ModifierLike<'a>],
        question_token: Option<&'a tsr_ast::Token<'a>>,
        properties: &mut Vec<ClassElement<'a>>,
    ) {
        let tsr_ast::BindingName::BindingPattern(pattern) = name else {
            let tsr_ast::BindingName::Identifier(name) = name else { return };
            if r#type.is_none() && !private {
                return;
            }
            let span = self.span_of(name.node_id);
            properties.push(ClassElement::PropertyDeclaration(self.factory.alloc(
                tsr_ast::PropertyDeclaration::new(
                    modifiers,
                    tsr_ast::PropertyName::Identifier(name),
                    question_token,
                    r#type,
                    None,
                ),
                SyntaxKind::PropertyDeclaration,
                span,
                NodeFlags::empty(),
            )));
            return;
        };

        let pattern_kind =
            pattern.node_id.map_or(pattern.kind.kind, |id| self.factory.nodes().kind(id));
        for (index, element) in pattern.elements.iter().enumerate() {
            let Some(element_name) = element.name else { continue };
            let element_type = if private {
                None
            } else {
                self.destructured_element_type(r#type, pattern_kind, index, element)
            };
            self.collect_parameter_properties(
                element_name,
                element_type,
                private,
                modifiers,
                None,
                properties,
            );
        }
    }

    fn destructured_element_type(
        &self,
        parent: Option<TypeNode<'a>>,
        pattern_kind: SyntaxKind,
        index: usize,
        element: &'a tsr_ast::BindingElement<'a>,
    ) -> Option<TypeNode<'a>> {
        let parent = self.resolve_syntactic_type_alias(parent?);
        if matches!(parent, TypeNode::KeywordTypeNode(keyword) if keyword.kind == SyntaxKind::AnyKeyword)
        {
            return Some(parent);
        }
        if pattern_kind == SyntaxKind::ArrayBindingPattern {
            return match parent {
                TypeNode::ArrayTypeNode(array) => array.element_type,
                TypeNode::TupleTypeNode(tuple) => tuple.elements.get(index).copied(),
                _ => None,
            };
        }

        let TypeNode::TypeLiteralNode(literal) = parent else { return None };
        let key = element.property_name.or(match element.name {
            Some(tsr_ast::BindingName::Identifier(identifier)) => {
                Some(tsr_ast::PropertyName::Identifier(identifier))
            }
            _ => None,
        })?;
        literal.members.iter().find_map(|member| {
            let TypeElement::PropertySignatureDeclaration(property) = member else { return None };
            if property_names_equal(&property.name, &key) { property.r#type } else { None }
        })
    }

    fn resolve_syntactic_type_alias(&self, mut r#type: TypeNode<'a>) -> TypeNode<'a> {
        for _ in 0..16 {
            let TypeNode::TypeReferenceNode(reference) = r#type else { break };
            if !reference.type_arguments.is_empty() {
                break;
            }
            let Some(tsr_ast::EntityName::Identifier(name)) = reference.type_name else { break };
            let Some(alias) = self.type_aliases.get(name.text).copied() else { break };
            if alias.node_id() == r#type.node_id() {
                break;
            }
            r#type = alias;
        }
        r#type
    }

    /// The accessibility a JS member's JSDoc declares, when its modifiers
    /// don't already spell one. Upstream reads `@public`/`@protected`/`@private`
    /// tags as modifiers in JavaScript files only
    /// (`lateBoundAssignmentCandidateJS3` emits `protected prop: string;`).
    fn jsdoc_accessibility(
        &self,
        node_id: Option<tsr_ast::NodeId>,
        modifiers: &[ModifierLike<'a>],
    ) -> Option<SyntaxKind> {
        if !self.javascript_file {
            return None;
        }
        let written = modifiers::modifier_flags(modifiers);
        if written
            .intersects(ModifierFlags::PUBLIC | ModifierFlags::PROTECTED | ModifierFlags::PRIVATE)
        {
            return None;
        }
        let source = self.options.source_text?;
        let start = self.span_of(node_id).start as usize;
        let comment = nearest_leading_comment(&source[..start.min(source.len())])?;
        if !comment.starts_with("/**") {
            return None;
        }
        if comment.contains("@private") {
            Some(SyntaxKind::PrivateKeyword)
        } else if comment.contains("@protected") {
            Some(SyntaxKind::ProtectedKeyword)
        } else {
            None
        }
    }

    /// Parse a JSDoc type the syntax-only emitter can rebuild: a keyword or a
    /// bare type name, braced or not. Anything richer needs JSDoc type-node
    /// parsing this port does not yet do, and answers `None` (emitting `any`).
    fn simple_jsdoc_type(&mut self, text: &str, span: Span) -> Option<TypeNode<'a>> {
        let text = text.trim();
        let text =
            text.strip_prefix('{').and_then(|inner| inner.strip_suffix('}')).unwrap_or(text).trim();
        let keyword = match text {
            "object" => Some(SyntaxKind::ObjectKeyword),
            "string" => Some(SyntaxKind::StringKeyword),
            "number" => Some(SyntaxKind::NumberKeyword),
            "boolean" => Some(SyntaxKind::BooleanKeyword),
            "any" | "*" => Some(SyntaxKind::AnyKeyword),
            "unknown" => Some(SyntaxKind::UnknownKeyword),
            "undefined" => Some(SyntaxKind::UndefinedKeyword),
            "symbol" => Some(SyntaxKind::SymbolKeyword),
            "bigint" => Some(SyntaxKind::BigIntKeyword),
            "never" => Some(SyntaxKind::NeverKeyword),
            "void" => Some(SyntaxKind::VoidKeyword),
            _ => None,
        };
        if let Some(kind) = keyword {
            return Some(self.factory.keyword_type(kind, span));
        }
        let mut chars = text.chars();
        let head_is_name = chars.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$');
        if !text.is_empty()
            && head_is_name
            && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            let text = self.factory.alloc_str(text);
            let name = self.factory.identifier(text, span);
            return Some(TypeNode::TypeReferenceNode(self.factory.alloc(
                tsr_ast::TypeReferenceNode::new(Some(tsr_ast::EntityName::Identifier(name)), &[]),
                SyntaxKind::TypeReference,
                span,
                NodeFlags::empty(),
            )));
        }
        None
    }

    /// A JSDoc type the simple builder cannot spell, parsed for real.
    ///
    /// The braced text is parsed into the transform's own arena and node
    /// table ([`Factory::parse_grafted_type`]), padded to its original file
    /// offset so every span lands inside the source. Multi-line type texts
    /// carry `*` line prefixes that are not type syntax, so they answer
    /// `None`.
    fn rich_jsdoc_type(&mut self, comment: &'a str, tag: &str) -> Option<TypeNode<'a>> {
        let range = jsdoc_braced_range(comment, tag)?;
        self.graft_jsdoc_range(comment, range)
    }

    /// Parse `comment[start..end]` as a type at its original file offset.
    fn graft_jsdoc_range(
        &mut self,
        comment: &'a str,
        (start, end): (usize, usize),
    ) -> Option<TypeNode<'a>> {
        let source = self.options.source_text?;
        let text = &comment[start..end];
        if text.trim().is_empty() {
            return None;
        }
        let rewritten = jsdoc_type_text_to_ts(text);
        let text: &str = rewritten.as_deref().unwrap_or(text);
        // A multi-line braced text is real type syntax only when its
        // continuation lines carry no `*` decoration; stripping decorations
        // would break the offset the graft depends on.
        if text.contains(['\n', '\r'])
            && text.lines().skip(1).any(|line| line.trim_start().starts_with('*'))
        {
            return None;
        }
        let comment_offset = (comment.as_ptr() as usize).checked_sub(source.as_ptr() as usize)?;
        if comment_offset + comment.len() > source.len() {
            return None;
        }
        let padded = format!("{}{}", " ".repeat(comment_offset + start), text);
        let padded = self.factory.alloc_str(&padded);
        self.factory.parse_grafted_type(padded)
    }

    /// The type a JSDoc tag declares: the simple builder first, then a real
    /// parse of the braced text.
    fn jsdoc_type(&mut self, comment: &'a str, tag: &str, span: Span) -> Option<TypeNode<'a>> {
        if let Some(text) = jsdoc_tag_text(comment, tag)
            && let Some(node) = self.simple_jsdoc_type(&text, span)
        {
            return Some(node);
        }
        self.rich_jsdoc_type(comment, tag)
    }

    /// An entity name mirroring an identifier or property-access chain.
    fn entity_of_expression(
        &mut self,
        expression: &Expression<'a>,
    ) -> Option<tsr_ast::EntityName<'a>> {
        match expression {
            Expression::Identifier(identifier) => Some(tsr_ast::EntityName::Identifier(identifier)),
            Expression::PropertyAccessExpression(access) => {
                let left = self.entity_of_expression(access.expression.as_ref()?)?;
                let Some(tsr_ast::MemberName::Identifier(right)) = &access.name else {
                    return None;
                };
                let span = self.span_of(access.node_id);
                Some(tsr_ast::EntityName::QualifiedName(self.factory.alloc(
                    tsr_ast::QualifiedName::new(Some(left), Some(right)),
                    SyntaxKind::QualifiedName,
                    span,
                    NodeFlags::empty(),
                )))
            }
            _ => None,
        }
    }

    /// Collect nested property assignments on empty-object consts.
    ///
    /// `const foo = {}; foo["baz"] = {}; foo["baz"]["blah"] = 3;` binds `foo`
    /// to `{ baz: { blah: number; }; }` in JavaScript; upstream builds this in
    /// the binder, and the syntactic stand-in is a path tree over top-level
    /// assignment statements.
    fn collect_object_expandos(&mut self, statements: &'a [Statement<'a>]) {
        if !self.javascript_file {
            return;
        }
        for statement in statements {
            let Statement::ExpressionStatement(statement) = statement else { continue };
            let Some(Expression::BinaryExpression(assignment)) = &statement.expression else {
                continue;
            };
            if assignment.operator_token.is_none_or(|token| token.kind != SyntaxKind::EqualsToken) {
                continue;
            }
            let Some(left) = &assignment.left else { continue };
            let Some((base, path)) = property_path(left) else { continue };
            if path.is_empty() {
                continue;
            }
            let mut node = self.object_expandos.entry(base).or_default();
            for segment in path {
                let position =
                    node.children.iter().position(|(name, _)| *name == segment).unwrap_or_else(
                        || {
                            node.children.push((segment.clone(), ObjectExpando::default()));
                            node.children.len() - 1
                        },
                    );
                node = &mut node.children[position].1;
            }
            if node.value.is_none() {
                node.value = assignment.right;
            }
        }
    }

    /// The nested type literal an object expando tree spells.
    fn object_expando_type(&mut self, expando: ObjectExpando<'a>, span: Span) -> TypeNode<'a> {
        let mut members = Vec::new();
        for (name, child) in expando.children {
            let r#type = if child.children.is_empty() {
                self.ensure_type(None, child.value.as_ref(), Freshness::Widening, None)
            } else {
                Some(self.object_expando_type(child, span))
            };
            let text = self.factory.alloc_str(&name);
            let name = tsr_ast::PropertyName::Identifier(self.factory.identifier(text, span));
            members.push(TypeElement::PropertySignatureDeclaration(self.factory.alloc(
                tsr_ast::PropertySignatureDeclaration::new(&[], name, None, r#type, None),
                SyntaxKind::PropertySignature,
                span,
                NodeFlags::empty(),
            )));
        }
        let members = self.factory.slice(&members);
        TypeNode::TypeLiteralNode(self.factory.alloc(
            tsr_ast::TypeLiteralNode::new(members),
            SyntaxKind::TypeLiteral,
            span,
            NodeFlags::empty(),
        ))
    }

    /// Type aliases a JS file declares through `@typedef` and `@callback`.
    ///
    /// Upstream hoists them to the top of the `.d.ts`, exported when the file
    /// is a module, without replaying the declaring comment. Dotted names
    /// (`@typedef {number} Dotted.Name`) declare namespace members this port
    /// does not yet synthesize and are skipped.
    fn synthesize_jsdoc_aliases(&mut self, is_module: bool) -> Vec<(usize, Statement<'a>)> {
        if !self.javascript_file {
            return Vec::new();
        }
        let Some(source) = self.options.source_text else { return Vec::new() };
        let mut result = Vec::new();
        let mut cursor = 0usize;
        while let Some(found) = source[cursor..].find("/**") {
            let start = cursor + found;
            let Some(close) = source[start..].find("*/") else { break };
            let end = start + close + 2;
            cursor = end;
            let comment = &source[start..end];
            // Comment ownership (`recursiveTypeReferences2` pins all four
            // arms): an alias replays its consecutive typedef-comment run only
            // when the run is followed by a blank line or end of input —
            // otherwise the comments are the following code's leading trivia
            // (a member's JSDoc, a `@type` comment) and replay there or not at
            // all. Owning aliases take a zero-width span at the run's end so
            // ordinary replay walks the whole run; the rest take a zero span.
            let run_end = typedef_run_end(source, end);
            let is_callback = jsdoc_braced_range(comment, "typedef").is_none()
                && jsdoc_tag_text(comment, "callback").is_some();
            let span = if owns_comment_run(source, run_end, is_callback) {
                let position = u32::try_from(run_end).unwrap_or(0);
                Span::new(position, position)
            } else {
                Span::new(0, 0)
            };
            if let Some(range) = jsdoc_braced_range(comment, "typedef") {
                let after = &comment[range.1 + 1..];
                let name_start = range.1 + 1 + (after.len() - after.trim_start().len());
                // `@typedef {T} A.B` declares `namespace A { export type B }`
                // (`reparser.go`'s `wrapInJSDocNamespace`); the declaring
                // comment stays on its host statement.
                if let Some(segments) = dotted_name_at(comment, name_start)
                    && segments.len() > 1
                {
                    let Some(r#type) = self.graft_jsdoc_range(comment, range) else { continue };
                    let namespace =
                        self.jsdoc_namespace_alias(&segments, r#type, comment, is_module);
                    result.push((start, namespace));
                    continue;
                }
                let Some(name) = identifier_at(comment, name_start) else { continue };
                // `@typedef {Object}` plus `@property` tags is JSDoc's object
                // literal spelling, and the tags are the members.
                let braced = comment[range.0..range.1].trim();
                let r#type = if braced.eq_ignore_ascii_case("object") {
                    self.jsdoc_property_object_type(comment)
                        .or_else(|| self.graft_jsdoc_range(comment, range))
                } else {
                    self.graft_jsdoc_range(comment, range)
                };
                let Some(r#type) = r#type else { continue };
                let alias = self.jsdoc_alias(&name, r#type, comment, is_module, span);
                result.push((start, alias));
            } else if let Some(name) = jsdoc_tag_text(comment, "callback") {
                if identifier_at(&name, 0).as_deref() != Some(name.as_str()) {
                    continue;
                }
                let Some(r#type) = self.jsdoc_callback_type(comment) else { continue };
                let alias = self.jsdoc_alias(&name, r#type, comment, is_module, span);
                result.push((start, alias));
            }
        }
        result
    }

    fn jsdoc_alias(
        &mut self,
        name: &str,
        r#type: TypeNode<'a>,
        comment: &'a str,
        is_module: bool,
        span: Span,
    ) -> Statement<'a> {
        let type_parameters = self.jsdoc_template_parameters(comment, span);
        let text = self.factory.alloc_str(name);
        let name = self.factory.identifier(text, span);
        let modifiers: &'a [ModifierLike<'a>] = if is_module {
            let token = self.factory.modifier(SyntaxKind::ExportKeyword, span);
            self.factory.slice(&[token])
        } else {
            &[]
        };
        Statement::TypeAliasDeclaration(self.factory.alloc(
            tsr_ast::TypeAliasDeclaration::new(
                modifiers,
                Some(name),
                type_parameters,
                Some(r#type),
            ),
            SyntaxKind::TypeAliasDeclaration,
            span,
            NodeFlags::empty(),
        ))
    }

    /// The namespace-wrapped alias a dotted `@typedef` name declares.
    fn jsdoc_namespace_alias(
        &mut self,
        segments: &[String],
        r#type: TypeNode<'a>,
        comment: &'a str,
        is_module: bool,
    ) -> Statement<'a> {
        let span = Span::new(0, 0);
        let type_parameters = self.jsdoc_template_parameters(comment, span);
        let export = self.factory.modifier(SyntaxKind::ExportKeyword, span);
        let alias_modifiers = self.factory.slice(&[export]);
        let (last, outer) = segments.split_last().expect("dotted name has segments");
        let text = self.factory.alloc_str(last);
        let name = self.factory.identifier(text, span);
        let mut statement = Statement::TypeAliasDeclaration(self.factory.alloc(
            tsr_ast::TypeAliasDeclaration::new(
                alias_modifiers,
                Some(name),
                type_parameters,
                Some(r#type),
            ),
            SyntaxKind::TypeAliasDeclaration,
            span,
            NodeFlags::empty(),
        ));
        for (index, segment) in outer.iter().enumerate().rev() {
            let statements = self.factory.slice(&[statement]);
            let block = self.factory.alloc(
                tsr_ast::ModuleBlock::new(statements),
                SyntaxKind::ModuleBlock,
                span,
                NodeFlags::empty(),
            );
            let modifiers: &'a [ModifierLike<'a>] = if index == 0 {
                let declare = self.factory.modifier(SyntaxKind::DeclareKeyword, span);
                if is_module {
                    let export = self.factory.modifier(SyntaxKind::ExportKeyword, span);
                    self.factory.slice(&[export, declare])
                } else {
                    self.factory.slice(&[declare])
                }
            } else {
                &[]
            };
            let keyword = self.factory.token(SyntaxKind::NamespaceKeyword, span);
            let text = self.factory.alloc_str(segment);
            let name = self.factory.identifier(text, span);
            statement = Statement::ModuleDeclaration(self.factory.alloc(
                tsr_ast::ModuleDeclaration::new(
                    modifiers,
                    keyword,
                    Some(tsr_ast::ModuleName::Identifier(name)),
                    Some(tsr_ast::ModuleBody::ModuleBlock(block)),
                    None,
                ),
                SyntaxKind::ModuleDeclaration,
                span,
                NodeFlags::empty(),
            ));
        }
        statement
    }

    /// `@template T` and `@template {C} K` tags as alias type parameters.
    fn jsdoc_template_parameters(
        &mut self,
        comment: &'a str,
        span: Span,
    ) -> &'a [&'a tsr_ast::TypeParameterDeclaration<'a>] {
        let mut parameters: Vec<&'a tsr_ast::TypeParameterDeclaration<'a>> = Vec::new();
        let mut cursor = 0usize;
        while let Some(found) = comment[cursor..].find("@template") {
            let index = cursor + found;
            let after = index + "@template".len();
            cursor = after;
            if !comment[after..].chars().next().is_none_or(char::is_whitespace) {
                continue;
            }
            let rest = &comment[after..];
            let mut position = after + (rest.len() - rest.trim_start().len());
            let mut constraint = None;
            if comment[position..].starts_with('{')
                && let Some(close) = matched_brace(&comment[position + 1..])
            {
                constraint = self.graft_jsdoc_range(comment, (position + 1, position + 1 + close));
                position = position + 1 + close + 1;
                let rest = &comment[position..];
                position += rest.len() - rest.trim_start().len();
            }
            let Some(name) = identifier_at(comment, position) else { continue };
            let text = self.factory.alloc_str(&name);
            let identifier = self.factory.identifier(text, span);
            parameters.push(self.factory.alloc(
                tsr_ast::TypeParameterDeclaration::new(
                    &[],
                    Some(identifier),
                    constraint,
                    None,
                    None,
                ),
                SyntaxKind::TypeParameter,
                span,
                NodeFlags::empty(),
            ));
        }
        self.factory.slice(&parameters)
    }

    /// The object type an `@typedef {Object}` block declares through its
    /// `@property`/`@prop` tags, built as text and grafted.
    fn jsdoc_property_object_type(&mut self, comment: &'a str) -> Option<TypeNode<'a>> {
        let source = self.options.source_text?;
        let mut properties = jsdoc_tagged_types(comment, "@property");
        properties.extend(jsdoc_tagged_types(comment, "@prop "));
        if properties.is_empty() {
            return None;
        }
        let mut text = String::from("{ ");
        for (range, name, optional) in properties {
            text.push_str(&name);
            if optional {
                text.push('?');
            }
            text.push_str(": ");
            text.push_str(&comment[range.0..range.1]);
            text.push_str("; ");
        }
        text.push('}');
        if text.contains(['\n', '\r']) {
            return None;
        }
        let offset = (comment.as_ptr() as usize).checked_sub(source.as_ptr() as usize)?;
        let padded = format!("{}{}", " ".repeat(offset), text);
        let padded = self.factory.alloc_str(&padded);
        self.factory.parse_grafted_type(padded)
    }

    /// The function type a `@callback` block declares, built from its `@param`
    /// tags and `@returns`, then parsed like any other grafted type.
    fn jsdoc_callback_type(&mut self, comment: &'a str) -> Option<TypeNode<'a>> {
        let source = self.options.source_text?;
        let mut text = String::from("(");
        for (index, (range, name, optional)) in jsdoc_param_tags(comment).into_iter().enumerate() {
            if index > 0 {
                text.push_str(", ");
            }
            // `@param {...T} args` is JSDoc's rest-parameter spelling; the
            // dots move onto the parameter and the element type stays as
            // written (`callbackTagVariadicType` keeps `...args: string`).
            let mut r#type = &comment[range.0..range.1];
            if let Some(rest) = r#type.strip_prefix("...") {
                text.push_str("...");
                r#type = rest;
            }
            text.push_str(&name);
            if optional {
                text.push('?');
            }
            text.push_str(": ");
            text.push_str(r#type);
        }
        text.push_str(") => ");
        match jsdoc_braced_range(comment, "returns")
            .or_else(|| jsdoc_braced_range(comment, "return"))
        {
            Some(range) => text.push_str(&comment[range.0..range.1]),
            None => text.push_str("void"),
        }
        if text.contains(['\n', '\r']) {
            return None;
        }
        let offset = (comment.as_ptr() as usize).checked_sub(source.as_ptr() as usize)?;
        let padded = format!("{}{}", " ".repeat(offset), text);
        let padded = self.factory.alloc_str(&padded);
        self.factory.parse_grafted_type(padded)
    }

    /// The nearest leading JSDoc before `node_id`, when the file is JavaScript.
    fn leading_jsdoc(&self, node_id: Option<tsr_ast::NodeId>) -> Option<&'a str> {
        if !self.javascript_file {
            return None;
        }
        let source = self.options.source_text?;
        let start = self.span_of(node_id).start as usize;
        let comment = nearest_leading_comment(&source[..start.min(source.len())])?;
        comment.starts_with("/**").then_some(comment)
    }

    /// Collect `@param {T} name` types from a signature's leading JSDoc and
    /// make them the active parameter-type context, returning the map to
    /// restore.
    fn begin_jsdoc_params(
        &mut self,
        node_id: Option<tsr_ast::NodeId>,
    ) -> HashMap<String, TypeNode<'a>> {
        let mut map = HashMap::new();
        if let Some(comment) = self.leading_jsdoc(node_id) {
            let span = self.span_of(node_id);
            for (range, name, _) in jsdoc_param_tags(comment) {
                let node = self
                    .simple_jsdoc_type(&comment[range.0..range.1], span)
                    .or_else(|| self.graft_jsdoc_range(comment, range));
                if let Some(node) = node {
                    map.insert(name, node);
                }
            }
        }
        std::mem::replace(&mut self.jsdoc_param_types, map)
    }

    /// Append heritage a JS class declares through `@implements` JSDoc.
    ///
    /// `/** @implements A */ class B {}` emits `class B implements A`; the tag
    /// takes braced and bare names alike.
    fn with_jsdoc_implements(
        &mut self,
        node: &tsr_ast::ClassDeclaration<'a>,
        heritage_clauses: &'a [&'a tsr_ast::HeritageClause<'a>],
    ) -> &'a [&'a tsr_ast::HeritageClause<'a>] {
        let Some(comment) = self.leading_jsdoc(node.node_id) else { return heritage_clauses };
        let Some(name) = jsdoc_tag_text(comment, "implements") else { return heritage_clauses };
        let span = self.span_of(node.node_id);
        let name = name.trim_start_matches('{').trim_end_matches('}').trim();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
            return heritage_clauses;
        }
        let text = self.factory.alloc_str(name);
        let identifier = self.factory.identifier(text, span);
        let with_arguments = self.factory.alloc(
            tsr_ast::ExpressionWithTypeArguments::new(
                Some(Expression::Identifier(identifier)),
                &[],
            ),
            SyntaxKind::ExpressionWithTypeArguments,
            span,
            NodeFlags::empty(),
        );
        let types = self.factory.slice(&[with_arguments]);
        let token = self.factory.token(SyntaxKind::ImplementsKeyword, span);
        let clause = self.factory.alloc(
            tsr_ast::HeritageClause::new(token, types),
            SyntaxKind::HeritageClause,
            span,
            NodeFlags::empty(),
        );
        let mut all: Vec<&tsr_ast::HeritageClause<'a>> = heritage_clauses.to_vec();
        all.push(clause);
        self.factory.slice(&all)
    }

    /// Class properties a JS constructor declares by assigning to `this`.
    ///
    /// Upstream binds these as members (`this.foo = bar` in the constructor is
    /// a property declaration in JavaScript); the syntactic stand-in collects
    /// first-occurrence `this.X = …` / `this["X"] = …` statements, typed by
    /// their leading `@type` JSDoc. The property carries the assignment's span
    /// so declaration printing replays that JSDoc.
    fn this_assignment_properties(
        &mut self,
        constructor: &tsr_ast::ConstructorDeclaration<'a>,
    ) -> Vec<ClassElement<'a>> {
        let Some(tsr_ast::FunctionBody::Block(body)) = constructor.body else { return Vec::new() };
        let mut seen: HashSet<String> = HashSet::new();
        let mut properties = Vec::new();
        for statement in body.statements {
            let Statement::ExpressionStatement(statement) = statement else { continue };
            let Some(Expression::BinaryExpression(assignment)) = &statement.expression else {
                continue;
            };
            if assignment.operator_token.is_none_or(|token| token.kind != SyntaxKind::EqualsToken) {
                continue;
            }
            let span = self.span_of(statement.node_id);
            let name = match &assignment.left {
                Some(Expression::PropertyAccessExpression(access)) => {
                    let Some(Expression::KeywordExpression(keyword)) = &access.expression else {
                        continue;
                    };
                    if keyword.kind != SyntaxKind::ThisKeyword {
                        continue;
                    }
                    let Some(tsr_ast::MemberName::Identifier(name)) = &access.name else {
                        continue;
                    };
                    tsr_ast::PropertyName::Identifier(self.factory.identifier(name.text, span))
                }
                Some(Expression::ElementAccessExpression(access)) => {
                    let Some(Expression::KeywordExpression(keyword)) = &access.expression else {
                        continue;
                    };
                    if keyword.kind != SyntaxKind::ThisKeyword {
                        continue;
                    }
                    let Some(Expression::StringLiteral(literal)) = &access.argument_expression
                    else {
                        continue;
                    };
                    tsr_ast::PropertyName::StringLiteral(self.factory.alloc(
                        tsr_ast::StringLiteral::new(literal.text, literal.token_flags),
                        SyntaxKind::StringLiteral,
                        span,
                        NodeFlags::empty(),
                    ))
                }
                _ => continue,
            };
            let key = match &name {
                tsr_ast::PropertyName::Identifier(id) => id.text.to_string(),
                tsr_ast::PropertyName::StringLiteral(literal) => literal.text.to_string(),
                _ => continue,
            };
            if !seen.insert(key) {
                continue;
            }
            // A `@type` JSDoc is the written type; otherwise the assigned
            // expression widens like a property initializer would.
            let r#type = self
                .leading_jsdoc(statement.node_id)
                .and_then(|comment| self.jsdoc_type(comment, "type", span))
                .or_else(|| {
                    self.ensure_type(
                        None,
                        assignment.right.as_ref(),
                        Freshness::Widening,
                        statement.node_id,
                    )
                });
            properties.push(ClassElement::PropertyDeclaration(self.factory.alloc(
                tsr_ast::PropertyDeclaration::new(&[], name, None, r#type, None),
                SyntaxKind::PropertyDeclaration,
                span,
                NodeFlags::empty(),
            )));
        }
        properties
    }

    /// Prepend a JSDoc-declared accessibility modifier, if any.
    fn with_jsdoc_accessibility(
        &mut self,
        modifiers: &'a [ModifierLike<'a>],
        accessibility: Option<SyntaxKind>,
        span: Span,
    ) -> &'a [ModifierLike<'a>] {
        let Some(kind) = accessibility else { return modifiers };
        let mut all = vec![self.factory.modifier(kind, span)];
        all.extend_from_slice(modifiers);
        self.factory.slice(&all)
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
        if class_member_name(member).is_some_and(|name| !property_name_is_nameable(name)) {
            return None;
        }
        let private = is_private_member(member);

        match member {
            // `transformPropertyDeclaration` (`:979`).
            ClassElement::PropertyDeclaration(node) => {
                let span = self.span_of(node.node_id);
                let accessibility = self.jsdoc_accessibility(node.node_id, node.modifiers);
                let private = private || accessibility == Some(SyntaxKind::PrivateKeyword);
                let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, false, false);
                let modifiers = self.with_jsdoc_accessibility(modifiers, accessibility, span);
                // A definite-assignment `!` is not legal in a `.d.ts`; a `?` is.
                let postfix =
                    node.postfix_token.filter(|token| token.kind != SyntaxKind::ExclamationToken);
                let host = LiteralConstHost::Property(node);
                let initializer = self.ensure_no_initializer(host);
                // In JavaScript, a member's leading `@type` JSDoc is its
                // written annotation (`typedefOnSemicolonClassElement`).
                let annotation = node.r#type.or_else(|| {
                    let comment = self.leading_jsdoc(node.node_id)?;
                    self.jsdoc_type(comment, "type", span)
                });
                let r#type = if initializer.is_some() || private {
                    None
                } else {
                    self.ensure_type(
                        annotation,
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
                let accessibility = self.jsdoc_accessibility(node.node_id, node.modifiers);
                let private = private || accessibility == Some(SyntaxKind::PrivateKeyword);
                let modifiers = self.ensure_modifiers(node.modifiers, node.node_id, false, false);
                let modifiers = self.with_jsdoc_accessibility(modifiers, accessibility, span);
                if private {
                    return Some(ClassElement::PropertyDeclaration(self.factory.alloc(
                        tsr_ast::PropertyDeclaration::new(modifiers, node.name, None, None, None),
                        SyntaxKind::PropertyDeclaration,
                        span,
                        NodeFlags::empty(),
                    )));
                }
                let parameters = self.update_param_list(node.parameters, false, node.node_id);
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
                let parameters = self.update_param_list(node.parameters, private, node.node_id);
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
                let parameters = self.update_param_list(node.parameters, private, node.node_id);
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
                let parameters = self.update_param_list(node.parameters, false, node.node_id);
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
            if self.should_strip_internal(member.node_id()) {
                continue;
            }
            if type_element_name(member).is_some_and(|name| !property_name_is_nameable(name)) {
                continue;
            }
            match member {
                TypeElement::CallSignatureDeclaration(node) => {
                    let span = self.span_of(node.node_id);
                    let parameters = self.update_param_list(node.parameters, false, node.node_id);
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
                    let parameters = self.update_param_list(node.parameters, false, node.node_id);
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
                    let r#type = self.ensure_type(
                        node.r#type,
                        node.initializer.as_ref(),
                        Freshness::Widening,
                        node.node_id,
                    );
                    result.push(TypeElement::PropertySignatureDeclaration(self.factory.alloc(
                        tsr_ast::PropertySignatureDeclaration::new(
                            node.modifiers,
                            node.name,
                            node.postfix_token,
                            r#type,
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
                    let parameters = self.update_param_list(node.parameters, false, node.node_id);
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
                    let parameters = self.update_param_list(node.parameters, false, node.node_id);
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

    /// Whether `stripInternal` removes this declaration using source-visible
    /// comment trivia only. Upstream attaches the closest leading comment to the
    /// node; an intervening non-internal comment therefore prevents an older
    /// `@internal` comment from being borrowed by the following declaration.
    fn should_strip_internal(&self, node_id: Option<tsr_ast::NodeId>) -> bool {
        if !self.options.strip_internal {
            return false;
        }
        let Some(source) = self.options.source_text else { return false };
        let start = self.span_of(node_id).start as usize;
        nearest_leading_comment(&source[..start.min(source.len())])
            .is_some_and(|comment| comment.contains("@internal"))
    }

    /// Ported from `updateParamList` (`transform.go:2384`) and `ensureParameter`
    /// (`:2395`).
    fn update_param_list(
        &mut self,
        parameters: &'a [&'a ParameterDeclaration<'a>],
        is_private: bool,
        host: Option<tsr_ast::NodeId>,
    ) -> &'a [&'a ParameterDeclaration<'a>] {
        // A private member's parameters are not part of the public shape, so
        // upstream emits an empty list rather than the real one.
        if is_private || parameters.is_empty() {
            return &[];
        }
        // In JavaScript, the signature's leading `@param {T} name` JSDoc is
        // the parameters' written type context.
        let saved = self.begin_jsdoc_params(host);
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
        self.jsdoc_param_types = saved;
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

    fn include_undefined_type(&mut self, r#type: TypeNode<'a>, span: Span) -> TypeNode<'a> {
        let already_includes_undefined = match r#type {
            TypeNode::KeywordTypeNode(keyword) => keyword.kind == SyntaxKind::UndefinedKeyword,
            TypeNode::UnionTypeNode(union) => union.types.iter().any(|member| {
                matches!(member, TypeNode::KeywordTypeNode(keyword) if keyword.kind == SyntaxKind::UndefinedKeyword)
            }),
            _ => false,
        };
        if already_includes_undefined {
            return r#type;
        }
        let undefined = self.factory.keyword_type(SyntaxKind::UndefinedKeyword, span);
        let types = self.factory.slice(&[r#type, undefined]);
        TypeNode::UnionTypeNode(self.factory.alloc(
            tsr_ast::UnionTypeNode::new(types),
            SyntaxKind::UnionType,
            span,
            NodeFlags::empty(),
        ))
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
        let name = parameter.name.map(|name| self.strip_binding_initializers(name));
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
        let written = parameter.r#type.or_else(|| {
            // In JavaScript the enclosing signature's `@param {T} name` is the
            // written type.
            let Some(tsr_ast::BindingName::Identifier(name)) = &parameter.name else {
                return None;
            };
            self.jsdoc_param_types.get(name.text).copied()
        });
        let r#type =
            self.ensure_type(written, None, Freshness::Widening, parameter.node_id).map(|r#type| {
                if parameter.question_token.is_some()
                    && modifiers::modifier_flags(parameter.modifiers)
                        .intersects(PARAMETER_PROPERTY_MODIFIER)
                    && !has_modifier(parameter.modifiers, SyntaxKind::PrivateKeyword)
                {
                    self.include_undefined_type(r#type, span)
                } else {
                    r#type
                }
            });
        self.factory.alloc(
            ParameterDeclaration::new(
                &[],
                parameter.dot_dot_dot_token,
                name,
                question,
                r#type,
                None,
            ),
            SyntaxKind::Parameter,
            span,
            NodeFlags::empty(),
        )
    }

    fn strip_binding_initializers(
        &mut self,
        name: tsr_ast::BindingName<'a>,
    ) -> tsr_ast::BindingName<'a> {
        let tsr_ast::BindingName::BindingPattern(pattern) = name else { return name };
        if !binding_name_contains_initializer(name) {
            return name;
        }
        let mut elements = Vec::with_capacity(pattern.elements.len());
        for element in pattern.elements {
            let nested = element.name.map(|name| self.strip_binding_initializers(name));
            elements.push(self.factory.alloc(
                tsr_ast::BindingElement::new(
                    element.dot_dot_dot_token,
                    element.property_name,
                    nested,
                    None,
                ),
                SyntaxKind::BindingElement,
                self.span_of(element.node_id),
                NodeFlags::empty(),
            ));
        }
        let elements = self.factory.slice(&elements);
        let kind = pattern.node_id.map_or(pattern.kind.kind, |id| self.factory.nodes().kind(id));
        let flags = self.factory.flags_of(pattern.node_id) & NodeFlags::HAS_TRAILING_COMMA;
        tsr_ast::BindingName::BindingPattern(self.factory.alloc(
            tsr_ast::BindingPattern::new(pattern.kind, elements),
            kind,
            self.span_of(pattern.node_id),
            flags,
        ))
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

fn collect_expando_members<'a>(
    statements: &'a [Statement<'a>],
    nodes: &tsr_ast::NodeTable,
) -> HashMap<String, Vec<ExpandoMember<'a>>> {
    let mut string_constants = HashMap::new();
    for statement in statements {
        let Statement::VariableStatement(statement) = statement else { continue };
        let Some(list) = statement.declaration_list else { continue };
        if !list.node_id.is_some_and(|id| nodes.flags(id).intersects(NodeFlags::CONSTANT)) {
            continue;
        }
        for declaration in list.declarations {
            let Some(tsr_ast::BindingName::Identifier(name)) = declaration.name else { continue };
            let Some(Expression::StringLiteral(value)) = declaration.initializer else { continue };
            string_constants.insert(name.text, value.text);
        }
    }

    let mut members: HashMap<String, Vec<ExpandoMember<'a>>> = HashMap::new();
    for statement in statements {
        let Statement::ExpressionStatement(statement) = statement else { continue };
        let Some(Expression::BinaryExpression(binary)) = statement.expression else { continue };
        if binary.operator_token.is_none_or(|token| token.kind != SyntaxKind::EqualsToken) {
            continue;
        }
        let Some(left) = binary.left else { continue };
        let Some((host, name)) = expando_assignment_name(&left, &string_constants) else {
            continue;
        };
        members.entry(host.to_string()).or_default().push(ExpandoMember {
            name,
            initializer: binary.right,
            node_id: binary.node_id,
        });
    }
    members
}

fn expando_assignment_name<'a>(
    left: &Expression<'a>,
    string_constants: &HashMap<&'a str, &'a str>,
) -> Option<(&'a str, Option<&'a str>)> {
    match left {
        Expression::PropertyAccessExpression(access) => {
            let Some(Expression::Identifier(host)) = access.expression else { return None };
            let Some(tsr_ast::MemberName::Identifier(name)) = access.name else { return None };
            Some((host.text, Some(name.text)))
        }
        Expression::ElementAccessExpression(access) => {
            let Some(Expression::Identifier(host)) = access.expression else { return None };
            let name = match access.argument_expression {
                Some(Expression::StringLiteral(name)) => Some(name.text),
                Some(Expression::Identifier(name)) => string_constants.get(name.text).copied(),
                _ => None,
            };
            Some((host.text, name.filter(|name| is_identifier_text(name))))
        }
        _ => None,
    }
}

fn is_identifier_text(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else { return false };
    (first.is_ascii_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

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

fn binding_name_has_bindings(name: tsr_ast::BindingName<'_>) -> bool {
    match name {
        tsr_ast::BindingName::Identifier(_) => true,
        tsr_ast::BindingName::BindingPattern(pattern) => pattern
            .elements
            .iter()
            .any(|element| element.name.is_some_and(binding_name_has_bindings)),
    }
}

fn binding_name_contains_initializer(name: tsr_ast::BindingName<'_>) -> bool {
    match name {
        tsr_ast::BindingName::Identifier(_) => false,
        tsr_ast::BindingName::BindingPattern(pattern) => pattern.elements.iter().any(|element| {
            element.initializer.is_some()
                || element.name.is_some_and(binding_name_contains_initializer)
        }),
    }
}

fn collect_binding_identifiers<'a>(
    name: Option<tsr_ast::BindingName<'a>>,
    output: &mut Vec<&'a tsr_ast::Identifier<'a>>,
) {
    match name {
        Some(tsr_ast::BindingName::Identifier(identifier)) => output.push(identifier),
        Some(tsr_ast::BindingName::BindingPattern(pattern)) => {
            for element in pattern.elements {
                collect_binding_identifiers(element.name, output);
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

/// The closest comment immediately preceding a node, excluding whitespace.
///
/// Returning one comment is intentional: an intervening non-internal comment
/// prevents an older `@internal` comment from being attached to the declaration.
/// JSDoc-only type spellings rewritten to TypeScript, per upstream's JSDoc
/// node mapping (`jsDeclarationsReusesExistingNodesMappingJSDocTypes`):
/// `?` → `any | null`, `T?` → `T | null`, `T=` → `T | undefined`, `T!` → `T`,
/// `function(…)` → `Function`, and `X.<…>` generics lose the dot with
/// `Object.<K, V>` becoming `Record<K, V>`. `None` means the text is already
/// TypeScript.
fn jsdoc_type_text_to_ts(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed == "?" {
        return Some("any | null".to_string());
    }
    if trimmed == "function" || trimmed.starts_with("function(") {
        return Some("Function".to_string());
    }
    if let Some(rest) = trimmed.strip_prefix('?') {
        return Some(format!("{} | null", rest.trim_start()));
    }
    let ends_in_name = |rest: &str| {
        rest.chars().last().is_some_and(|c| c.is_alphanumeric() || matches!(c, '>' | ']' | ')'))
    };
    if let Some(rest) = trimmed.strip_suffix('?')
        && ends_in_name(rest.trim_end())
    {
        return Some(format!("{} | null", rest.trim_end()));
    }
    if let Some(rest) = trimmed.strip_suffix('=')
        && ends_in_name(rest.trim_end())
    {
        return Some(format!("{} | undefined", rest.trim_end()));
    }
    if let Some(rest) = trimmed.strip_suffix('!')
        && ends_in_name(rest.trim_end())
    {
        return Some(rest.trim_end().to_string());
    }
    if trimmed.contains(".<") {
        return Some(trimmed.replace("Object.<", "Record<").replace(".<", "<"));
    }
    None
}

/// The end of the consecutive typedef/callback comment run continuing at
/// `from` (the end of a comment): each further comment must be separated by
/// whitespace without a blank line and itself declare a typedef or callback.
fn typedef_run_end(source: &str, mut from: usize) -> usize {
    loop {
        let rest = &source[from..];
        let trimmed = rest.trim_start();
        let gap_len = rest.len() - trimmed.len();
        if rest[..gap_len].matches('\n').count() > 1 || !trimmed.starts_with("/**") {
            return from;
        }
        let Some(close) = trimmed.find("*/") else { return from };
        let comment = &trimmed[..close + 2];
        if jsdoc_braced_range(comment, "typedef").is_none()
            && jsdoc_tag_text(comment, "callback").is_none()
        {
            return from;
        }
        from += gap_len + close + 2;
    }
}

/// Whether the typedef-comment run ending at `from` owns its comments.
///
/// Owned — the synthesized aliases replay the run — exactly when a blank line
/// (or end of input) separates the run from what follows, and what follows is
/// not an ordinary comment: a following non-typedef comment claims the whole
/// leading trivia for the code below it (`recursiveTypeReferences2`'s
/// `XMLObject` block stays with `const p`, while its three single-line
/// typedefs hoist).
fn owns_comment_run(source: &str, from: usize, is_callback: bool) -> bool {
    let rest = &source[from..];
    let trimmed = rest.trim_start();
    if trimmed.is_empty() {
        return true;
    }
    if rest[..rest.len() - trimmed.len()].matches('\n').count() < 2 {
        return false;
    }
    // A following non-typedef comment claims the trivia for the code below it
    // — but only away from `@typedef` runs; a `@callback` keeps its comment
    // (`callbackTagVariadicType` versus `recursiveTypeReferences2`, both
    // baseline-pinned).
    if is_callback {
        return true;
    }
    if let Some(after_open) = trimmed.strip_prefix("/**") {
        let Some(close) = after_open.find("*/") else { return false };
        let comment = &trimmed[..close + 2 + 3];
        return jsdoc_braced_range(comment, "typedef").is_some()
            || jsdoc_tag_text(comment, "callback").is_some();
    }
    !trimmed.starts_with("//") && !trimmed.starts_with("/*")
}

/// Nested property assignments rooted at one object-literal binding.
#[derive(Default)]
struct ObjectExpando<'a> {
    children: Vec<(String, ObjectExpando<'a>)>,
    value: Option<Expression<'a>>,
}

/// `foo.a["b"].c` as its base binding plus named path segments.
fn property_path(expression: &Expression<'_>) -> Option<(String, Vec<String>)> {
    match expression {
        Expression::Identifier(name) => Some((name.text.to_string(), Vec::new())),
        Expression::PropertyAccessExpression(access) => {
            let (base, mut path) = property_path(access.expression.as_ref()?)?;
            let tsr_ast::MemberName::Identifier(name) = access.name.as_ref()? else { return None };
            path.push(name.text.to_string());
            Some((base, path))
        }
        Expression::ElementAccessExpression(access) => {
            let (base, mut path) = property_path(access.expression.as_ref()?)?;
            let Expression::StringLiteral(name) = access.argument_expression.as_ref()? else {
                return None;
            };
            path.push(name.text.to_string());
            Some((base, path))
        }
        _ => None,
    }
}

/// The dotted identifier path starting at `position`, when one does.
fn dotted_name_at(text: &str, position: usize) -> Option<Vec<String>> {
    let mut segments = Vec::new();
    let mut cursor = position;
    loop {
        let rest = text.get(cursor..)?;
        let head = rest.chars().next()?;
        if !(head.is_alphabetic() || head == '_' || head == '$') {
            return None;
        }
        let name: String =
            rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect();
        cursor += name.len();
        segments.push(name);
        if text.get(cursor..).is_some_and(|rest| rest.starts_with('.')) {
            cursor += 1;
        } else {
            return Some(segments);
        }
    }
}

/// The identifier starting at `position`, when one does and nothing dotted
/// follows it.
fn identifier_at(text: &str, position: usize) -> Option<String> {
    let rest = text.get(position..)?;
    let mut chars = rest.chars();
    let head = chars.next()?;
    if !(head.is_alphabetic() || head == '_' || head == '$') {
        return None;
    }
    let name: String =
        rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect();
    if rest[name.len()..].starts_with('.') {
        return None;
    }
    Some(name)
}

/// The end of a brace-matched region starting after an opening `{`.
fn matched_brace(text: &str) -> Option<usize> {
    let mut depth = 1usize;
    for (index, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// The brace-matched `{…}` range following `@<tag>`, as offsets into `comment`.
fn jsdoc_braced_range(comment: &str, tag: &str) -> Option<(usize, usize)> {
    let marker = format!("@{tag}");
    let mut cursor = 0usize;
    loop {
        let index = comment[cursor..].find(&marker)? + cursor;
        let after = index + marker.len();
        cursor = after;
        if !comment[after..].chars().next().is_none_or(char::is_whitespace) {
            continue;
        }
        let rest = &comment[after..];
        let skipped = rest.len() - rest.trim_start().len();
        let open = after + skipped;
        if !comment[open..].starts_with('{') {
            continue;
        }
        let close = matched_brace(&comment[open + 1..])?;
        return Some((open + 1, open + 1 + close));
    }
}

/// The text following `@<tag>` in a JSDoc comment — braced (`{T}`) or the
/// bare first token. The boundary check keeps `@type` from matching
/// `@typedef`.
fn jsdoc_tag_text(comment: &str, tag: &str) -> Option<String> {
    let marker = format!("@{tag}");
    let mut search = comment;
    loop {
        let index = search.find(&marker)?;
        let after = &search[index + marker.len()..];
        if after.chars().next().is_none_or(char::is_whitespace) {
            let rest = after.trim_start();
            if let Some(inner) = rest.strip_prefix('{') {
                let end = inner.find('}')?;
                return Some(inner[..end].trim().to_string());
            }
            // `@implements A*/` closes the comment right after the name.
            let token = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('/')
                .trim_end_matches('*')
                .to_string();
            return (!token.is_empty()).then_some(token);
        }
        search = after;
    }
}

/// Every `@param {T} name` tag in a JSDoc comment, as the type's brace-matched
/// range into `comment` plus the parameter name. A bracketed name (`[name]`,
/// `[name=default]`) is JSDoc's optional syntax; the brackets and default are
/// not part of the name.
fn jsdoc_param_tags(comment: &str) -> Vec<((usize, usize), String, bool)> {
    jsdoc_tagged_types(comment, "@param")
}

/// Every `@<tag> {T} name` occurrence, shared by `@param` and `@property`.
fn jsdoc_tagged_types(comment: &str, marker: &str) -> Vec<((usize, usize), String, bool)> {
    let mut result = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = comment[cursor..].find(marker) {
        let index = cursor + found;
        let after = index + marker.len();
        cursor = after;
        if !comment[after..].chars().next().is_none_or(char::is_whitespace) {
            continue;
        }
        let rest = &comment[after..];
        let skipped = rest.len() - rest.trim_start().len();
        let open = after + skipped;
        if !comment[open..].starts_with('{') {
            continue;
        }
        let Some(close) = matched_brace(&comment[open + 1..]) else { continue };
        let range = (open + 1, open + 1 + close);
        let name_part = comment[range.1 + 1..].trim_start();
        let raw = name_part.split_whitespace().next().unwrap_or("");
        let optional = raw.starts_with('[');
        let raw = raw.strip_prefix('[').unwrap_or(raw);
        let name: String =
            raw.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect();
        if !name.is_empty() && range.0 < range.1 {
            result.push((range, name, optional));
        }
    }
    result
}

fn nearest_leading_comment(prefix: &str) -> Option<&str> {
    let trimmed = prefix.trim_end_matches(char::is_whitespace);
    if trimmed.ends_with("*/") {
        let start = trimmed.rfind("/*")?;
        return Some(&trimmed[start..]);
    }

    let line_start = trimmed.rfind(['\n', '\r']).map_or(0, |index| index + 1);
    let line = trimmed[line_start..].trim_start();
    line.starts_with("//").then_some(line)
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

fn type_element_name<'b, 'a>(member: &'b TypeElement<'a>) -> Option<&'b tsr_ast::PropertyName<'a>> {
    match member {
        TypeElement::PropertySignatureDeclaration(node) => Some(&node.name),
        TypeElement::MethodSignatureDeclaration(node) => Some(&node.name),
        TypeElement::GetAccessorDeclaration(node) => Some(&node.name),
        TypeElement::SetAccessorDeclaration(node) => Some(&node.name),
        _ => None,
    }
}

fn property_name_is_nameable(name: &tsr_ast::PropertyName<'_>) -> bool {
    let tsr_ast::PropertyName::ComputedPropertyName(computed) = name else { return true };
    matches!(
        computed.expression,
        Some(
            Expression::Identifier(_)
                | Expression::PropertyAccessExpression(_)
                | Expression::StringLiteral(_)
                | Expression::NumericLiteral(_)
                | Expression::NoSubstitutionTemplateLiteral(_)
        )
    )
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

fn is_function_overload_implementation(
    statement: &Statement<'_>,
    siblings: &[Statement<'_>],
) -> bool {
    let Statement::FunctionDeclaration(function) = statement else { return false };
    let Some(name) = function.name else { return false };
    function.body.is_some()
        && siblings.iter().any(|candidate| {
            matches!(
                candidate,
                Statement::FunctionDeclaration(candidate)
                    if candidate.body.is_none()
                        && candidate.name.is_some_and(|candidate| candidate.text == name.text)
            )
        })
}
