//! Which top-level declarations reach the `.d.ts`.
//!
//! # Why this pass exists at all
//!
//! A declaration that is never emitted cannot produce a declaration-emit error.
//! The corpus states this about as plainly as a test can:
//! `compiler/isolatedDeclarationErrorsReturnTypes` contains three structurally
//! identical classes — `ExportedClass`, `IndirectlyExportedClass`, `InternalClass`
//! — each with the same unannotated members. The first errors because it is
//! exported; the second errors because an exported `const` *annotation* names it;
//! the third is silent. Without this pass the analysis would report every member of
//! `InternalClass` and be wrong by construction on a third of that file.
//!
//! # The approximation, stated up front
//!
//! Upstream decides this through `EmitResolver.IsDeclarationVisible`, which is
//! checker-backed. This pass approximates it with **reachability from the exports
//! through type positions**, which is the same shape oxc uses
//! (`oxc_isolated_declarations/src/scope.rs`) and is cited as inspiration under
//! [ADR-0004], not ported.
//!
//! Two consequences, both accepted and both worth knowing when reading a failure:
//!
//! - **Name resolution is by top-level name only.** There is no scope chain: a
//!   reference to `T` marks *the* top-level `T`, if one exists. Shadowing inside a
//!   nested scope is invisible to this pass. Since the pass is only ever consulted
//!   for top-level declarations, a shadowed inner `T` cannot be what a `.d.ts`
//!   would have emitted anyway.
//! - **Reference collection descends into everything except function bodies.**
//!   Strictly, only type positions can pull a declaration into the `.d.ts`, so
//!   collecting from initialisers too over-approximates: a declaration may be
//!   marked visible that upstream would have dropped. The error direction is
//!   deliberate — an over-approximation reports diagnostics upstream does not
//!   (visible as a conformance failure), whereas an under-approximation stays
//!   silent (invisible). Failing loudly is the correct bias for a component with a
//!   16-case oracle.
//!
//! [ADR-0004]: ../../../docs/adr/0004-oxc-inspiration-not-dependency.md

use rustc_hash::{FxHashMap, FxHashSet};
use tsr_ast::{
    EntityName, Expression, ModifierLike, NamedExportBindings, NodeId, SourceFile, Statement,
    SyntaxKind, Visit,
};

/// The top-level declarations that reach the `.d.ts`, keyed by statement node id.
#[derive(Debug, Default)]
pub struct Visible {
    ids: FxHashSet<NodeId>,
    /// The declared names reachability actually passed through. A statement can
    /// be visible for one of its names only — `var x = 10, m2: T` with
    /// `export = m2` reaches `m2` and not `x` — and declarator-level filtering
    /// needs that distinction, which `ids` alone cannot carry.
    names: FxHashSet<String>,
}

impl Visible {
    /// Whether this statement is emitted into the `.d.ts`.
    #[must_use]
    pub fn contains(&self, id: Option<NodeId>) -> bool {
        id.is_some_and(|id| self.ids.contains(&id))
    }

    /// Whether reachability passed through this declared name.
    #[must_use]
    pub fn reaches_name(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// How many top-level declarations are emitted.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether nothing is emitted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// Compute the visible set for a file.
///
/// `commonjs` is [`is_commonjs_module`]'s answer: a `CommonJS` file is a module
/// whose exports are its `module.exports`/`exports.x` assignments.
#[must_use]
pub fn visible_declarations<'a>(file: &'a SourceFile<'a>, commonjs: bool) -> Visible {
    // A *script* — no imports, no exports — emits every top-level declaration,
    // because there is no export list to be reachable from. `isolatedDeclarationErrors`
    // is exactly this shape: not one `export` in the file, and upstream still
    // reports four errors in it.
    if commonjs {
        return visible_members(file.statements, true);
    }
    if !is_module(file.statements) {
        let ids = file.statements.iter().filter_map(Statement::node_id).collect();
        let names = file
            .statements
            .iter()
            .flat_map(|statement| declared_names(statement))
            .map(ToString::to_string)
            .collect();
        return Visible { ids, names };
    }

    visible_module_members(file.statements)
}

/// Compute declarations reachable from the exports of a module-like statement list.
///
/// Unlike a source file, a namespace body is always a module scope even when it
/// contains no import/export syntax at the outer file level. Declaration emit
/// uses this after transforming the body so dependencies erased from private
/// members no longer keep private aliases alive.
#[must_use]
pub fn visible_module_members<'a>(statements: &'a [Statement<'a>]) -> Visible {
    visible_members(statements, false)
}

/// Whether a JavaScript file is a `CommonJS` module: upstream's binder sets
/// `CommonJSModuleIndicator` (`internal/binder/binder.go:927`) on the first
/// `require(…)` call, `module.exports = …`, `exports.x = …`/`module.exports.x
/// = …` or `Object.defineProperty(exports, …)` it binds — anywhere in the file
/// — unless the file is already an ES module.
#[must_use]
pub fn is_commonjs_module(file: &SourceFile<'_>, javascript: bool) -> bool {
    struct Finder(bool);
    impl<'a> Visit<'a> for Finder {
        fn visit_node(&mut self, node: tsr_ast::Node<'a>) {
            if self.0 {
                return;
            }
            match node {
                tsr_ast::Node::CallExpression(call) => {
                    let require = matches!(call.expression, Some(Expression::Identifier(callee)) if callee.text == "require")
                        && call.arguments.len() == 1;
                    if require || is_define_property_on_exports(call) {
                        self.0 = true;
                        return;
                    }
                }
                tsr_ast::Node::BinaryExpression(binary)
                    if binary
                        .operator_token
                        .is_some_and(|token| token.kind == SyntaxKind::EqualsToken)
                        && binary
                            .left
                            .as_ref()
                            .is_some_and(|left| commonjs_export_target(left).is_some()) =>
                {
                    self.0 = true;
                    return;
                }
                _ => {}
            }
            tsr_ast::visit::walk_node(self, node);
        }
    }
    if !javascript || is_module(file.statements) {
        return false;
    }
    let mut finder = Finder(false);
    for statement in file.statements {
        finder.visit_node(tsr_ast::Node::from(*statement));
    }
    finder.0
}

/// `Object.defineProperty(exports | module.exports, "name", descriptor)`
/// (`ast.IsBindableObjectDefinePropertyCall` with an exports target).
#[must_use]
pub fn is_define_property_on_exports(call: &tsr_ast::CallExpression<'_>) -> bool {
    matches!(
        call.expression,
        Some(Expression::PropertyAccessExpression(access))
            if matches!(access.expression, Some(Expression::Identifier(object)) if object.text == "Object")
                && matches!(access.name, Some(tsr_ast::MemberName::Identifier(name)) if name.text == "defineProperty")
    ) && call.arguments.len() == 3
        && call.arguments.first().is_some_and(is_exports_object)
        && matches!(
            call.arguments.get(1),
            Some(Expression::StringLiteral(_) | Expression::NumericLiteral(_))
        )
}

/// What a `CommonJS` assignment target exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommonJsTarget<'a> {
    /// `module.exports = …` (`JSDeclarationKindModuleExports`).
    ModuleExports,
    /// `exports.name = …` or `module.exports.name = …`
    /// (`JSDeclarationKindExportsProperty`).
    Property(&'a str),
}

/// `ast.GetAssignmentDeclarationKind`'s two `CommonJS` arms
/// (`internal/ast/utilities.go:1541`), for an assignment's left side.
#[must_use]
pub fn commonjs_export_target<'a>(left: &Expression<'a>) -> Option<CommonJsTarget<'a>> {
    if is_module_exports(left) {
        return Some(CommonJsTarget::ModuleExports);
    }
    let (object, name) = match left {
        Expression::PropertyAccessExpression(access) => match access.name {
            Some(tsr_ast::MemberName::Identifier(name)) => (access.expression.as_ref()?, name.text),
            _ => return None,
        },
        Expression::ElementAccessExpression(access) => match access.argument_expression {
            Some(Expression::StringLiteral(name)) => (access.expression.as_ref()?, name.text),
            _ => return None,
        },
        _ => return None,
    };
    is_exports_object(object).then_some(CommonJsTarget::Property(name))
}

/// `exports` or `module.exports`.
fn is_exports_object(expression: &Expression<'_>) -> bool {
    matches!(expression, Expression::Identifier(identifier) if identifier.text == "exports")
        || is_module_exports(expression)
}

/// `ast.IsModuleExportsAccessExpression`.
fn is_module_exports(expression: &Expression<'_>) -> bool {
    let (object, name) = match expression {
        Expression::PropertyAccessExpression(access) => match access.name {
            Some(tsr_ast::MemberName::Identifier(name)) => (access.expression.as_ref(), name.text),
            _ => return false,
        },
        Expression::ElementAccessExpression(access) => match access.argument_expression {
            Some(Expression::StringLiteral(name)) => (access.expression.as_ref(), name.text),
            _ => return false,
        },
        _ => return false,
    };
    name == "exports"
        && matches!(object, Some(Expression::Identifier(module)) if module.text == "module")
}

fn visible_members<'a>(statements: &'a [Statement<'a>], commonjs: bool) -> Visible {
    let by_name = index_by_name(statements);

    let mut visible: FxHashSet<NodeId> = FxHashSet::default();
    let mut names: FxHashSet<String> = FxHashSet::default();
    let mut queue: Vec<&Statement<'a>> = Vec::new();

    for statement in statements {
        // In an external module, `declare global` and string-named ambient
        // modules are augmentations. They contribute declarations by side effect
        // even though they carry no `export` modifier, and their bodies can make
        // imports visible.
        let is_augmentation = matches!(
            statement,
            Statement::ModuleDeclaration(module)
                if module.keyword.kind == SyntaxKind::GlobalKeyword
                    || matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
        );
        if exports_something(statement) || is_augmentation {
            if let Some(id) = statement.node_id() {
                visible.insert(id);
            }
            names.extend(declared_names(statement).into_iter().map(ToString::to_string));
            queue.push(statement);
        }
        // `export { a, b }` names declarations elsewhere in the file. The
        // specifier's *property name* is the local one when both are written
        // (`export { local as public }`).
        if let Statement::ExportDeclaration(export) = statement
            && export.module_specifier.is_none()
            && let Some(NamedExportBindings::NamedExports(named)) = &export.export_clause
        {
            for specifier in named.elements {
                let local = specifier.property_name.as_ref().or(specifier.name.as_ref());
                if let Some(name) = local.and_then(module_export_name)
                    && let Some(targets) = by_name.get(name)
                {
                    names.insert(name.to_string());
                    for target in targets {
                        if let Some(id) = target.node_id()
                            && visible.insert(id)
                        {
                            queue.push(target);
                        }
                    }
                }
            }
        }
    }

    // A `CommonJS` file's exports are its top-level `module.exports = …` and
    // `exports.x = …` assignments; each emits its right side's shape (an
    // `export =` of an identifier, or a typed `_exports`/member), so whatever
    // that names is reached.
    if commonjs {
        for statement in statements {
            let Statement::ExpressionStatement(expression) = statement else { continue };
            let Some(Expression::BinaryExpression(binary)) = &expression.expression else {
                continue;
            };
            if binary.operator_token.is_none_or(|token| token.kind != SyntaxKind::EqualsToken)
                || binary.left.as_ref().is_none_or(|left| commonjs_export_target(left).is_none())
            {
                continue;
            }
            let Some(right) = &binary.right else { continue };
            let mut collector = ReferenceCollector::default();
            collector.record_default_export_expression(right);
            let restated: Vec<&str> = collector
                .value_names
                .iter()
                .copied()
                .filter(|name| {
                    by_name.get(name).is_some_and(|targets| {
                        targets.iter().all(|target| restates_as_typeof(target))
                    })
                })
                .collect();
            for name in collector.names.into_iter().chain(restated) {
                let Some(targets) = by_name.get(name) else { continue };
                names.insert(name.to_string());
                for target in targets {
                    if let Some(id) = target.node_id()
                        && visible.insert(id)
                    {
                        queue.push(target);
                    }
                }
            }
        }
    }

    // Fixpoint: a visible declaration drags in whatever it names.
    while let Some(statement) = queue.pop() {
        let mut collector = ReferenceCollector::default();
        collector.visit_node(tsr_ast::Node::from(*statement));
        let restated_values = collector.value_names.into_iter().filter(|name| {
            by_name.get(name).is_some_and(|targets| {
                targets.iter().all(|target| restates_as_typeof(target))
                    && targets
                        .iter()
                        .any(|target| !matches!(target, Statement::ModuleDeclaration(_)))
            })
        });
        for name in collector.names.into_iter().chain(restated_values) {
            if let Some(targets) = by_name.get(name) {
                names.insert(name.to_string());
                for target in targets {
                    if let Some(id) = target.node_id()
                        && visible.insert(id)
                    {
                        queue.push(target);
                    }
                }
            }
        }
        for name in collector.type_names {
            let Some(targets) = by_name.get(name) else { continue };
            // The binder keeps exported and local declarations in different
            // tables, so one name can be two symbols; a type reference resolves
            // to the one with a type meaning and makes *all* of that symbol's
            // declarations visible (`hasVisibleDeclarations`,
            // `internal/checker/emitresolver.go:384`) — an interface's merged
            // local namespace comes along
            // (`declarationEmitNamespaceMergedWithInterfaceNestedFunction`),
            // an exported interface's same-named local `const` does not.
            let symbol_reached = |exported: bool| {
                targets
                    .iter()
                    .any(|target| exports_something(target) == exported && has_type_meaning(target))
            };
            let (exported, local) = (symbol_reached(true), symbol_reached(false));
            // A name with no type-meaning declaration in this file keeps the old
            // name-only reach: it may be an augmentation or a global this pass
            // cannot see, and over-reaching is the deliberate bias.
            let reached: Vec<_> = targets
                .iter()
                .filter(|target| {
                    (!exported && !local)
                        || if exports_something(target) { exported } else { local }
                })
                .collect();
            names.insert(name.to_string());
            for target in reached {
                if let Some(id) = target.node_id()
                    && visible.insert(id)
                {
                    queue.push(target);
                }
            }
        }
    }

    Visible { ids: visible, names }
}

/// Index top-level declarations by the name they introduce.
///
/// Every declaration is retained for a name because TypeScript declarations can
/// merge (`interface` + `interface`, `namespace` + `function`, and others).
/// Replacing an earlier entry with a later one made reachability depend on source
/// order and emitted only half of a merged symbol.
fn index_by_name<'a, 'b>(
    statements: &'b [Statement<'a>],
) -> FxHashMap<&'a str, Vec<&'b Statement<'a>>> {
    let mut map = FxHashMap::default();
    for statement in statements {
        for name in declared_names(statement) {
            map.entry(name).or_insert_with(Vec::new).push(statement);
        }
    }
    map
}

/// Every identifier a binding name introduces, patterns included.
fn collect_bound_names<'a>(name: Option<&tsr_ast::BindingName<'a>>, names: &mut Vec<&'a str>) {
    match name {
        Some(tsr_ast::BindingName::Identifier(identifier)) => names.push(identifier.text),
        Some(tsr_ast::BindingName::BindingPattern(pattern)) => {
            for element in pattern.elements {
                collect_bound_names(element.name.as_ref(), names);
            }
        }
        None => {}
    }
}

/// The names a top-level statement introduces into the file's scope.
fn declared_names<'a>(statement: &Statement<'a>) -> Vec<&'a str> {
    match statement {
        Statement::ClassDeclaration(d) => d.name.map(|n| n.text).into_iter().collect(),
        Statement::FunctionDeclaration(d) => d.name.map(|n| n.text).into_iter().collect(),
        Statement::InterfaceDeclaration(d) => d.name.map(|n| n.text).into_iter().collect(),
        Statement::TypeAliasDeclaration(d) => d.name.map(|n| n.text).into_iter().collect(),
        Statement::EnumDeclaration(d) => d.name.map(|n| n.text).into_iter().collect(),
        Statement::ImportEqualsDeclaration(d) => d.name.map(|n| n.text).into_iter().collect(),
        Statement::ModuleDeclaration(d) => match &d.name {
            Some(tsr_ast::ModuleName::Identifier(n)) => vec![n.text],
            _ => Vec::new(),
        },
        Statement::VariableStatement(d) => {
            let mut names = Vec::new();
            if let Some(list) = d.declaration_list {
                for declaration in list.declarations {
                    // Binding patterns introduce every bound identifier:
                    // `const { Foo } = A` declares `Foo`, and a class
                    // extending it must reach the statement
                    // (`declarationEmitExpressionInExtends6`).
                    collect_bound_names(declaration.name.as_ref(), &mut names);
                }
            }
            names
        }
        Statement::ImportDeclaration(d) => {
            let mut names = Vec::new();
            if let Some(clause) = d.import_clause {
                if let Some(n) = clause.name {
                    names.push(n.text);
                }
                match &clause.named_bindings {
                    Some(tsr_ast::NamedImportBindings::NamespaceImport(ns)) => {
                        if let Some(n) = ns.name {
                            names.push(n.text);
                        }
                    }
                    Some(tsr_ast::NamedImportBindings::NamedImports(imports)) => {
                        for specifier in imports.elements {
                            if let Some(n) = specifier.name {
                                names.push(n.text);
                            }
                        }
                    }
                    None => {}
                }
            }
            names
        }
        _ => Vec::new(),
    }
}

/// Whether the file is a module, which is what makes visibility a question.
fn is_module(statements: &[Statement<'_>]) -> bool {
    statements.iter().any(|statement| {
        matches!(
            statement,
            Statement::ImportDeclaration(_)
                | Statement::ExportDeclaration(_)
                | Statement::ExportAssignment(_)
        ) || exports_something(statement)
    })
}

/// Whether the statement carries an `export` modifier or is `export default`.
fn exports_something(statement: &Statement<'_>) -> bool {
    if matches!(statement, Statement::ExportAssignment(_)) {
        return true;
    }
    modifiers_of(statement).is_some_and(|modifiers| {
        modifiers.iter().any(|modifier| {
            matches!(modifier, ModifierLike::Token(token) if token.kind == SyntaxKind::ExportKeyword)
        })
    })
}

/// The modifier list of a top-level statement, when it has one.
fn modifiers_of<'a, 'b>(statement: &'b Statement<'a>) -> Option<&'b [ModifierLike<'a>]> {
    Some(match statement {
        Statement::ClassDeclaration(d) => d.modifiers,
        Statement::FunctionDeclaration(d) => d.modifiers,
        Statement::InterfaceDeclaration(d) => d.modifiers,
        Statement::TypeAliasDeclaration(d) => d.modifiers,
        Statement::EnumDeclaration(d) => d.modifiers,
        Statement::ModuleDeclaration(d) => d.modifiers,
        Statement::VariableStatement(d) => d.modifiers,
        Statement::ImportEqualsDeclaration(d) => d.modifiers,
        Statement::ExportDeclaration(d) => d.modifiers,
        Statement::ExportAssignment(d) => d.modifiers,
        _ => return None,
    })
}

/// The identifier text of a `ModuleExportName`, when it is one.
fn module_export_name<'a>(name: &tsr_ast::ModuleExportName<'a>) -> Option<&'a str> {
    match name {
        tsr_ast::ModuleExportName::Identifier(identifier) => Some(identifier.text),
        tsr_ast::ModuleExportName::StringLiteral(_) => None,
    }
}

/// Collects the names a declaration mentions **in type positions**.
///
/// Only a type position can pull another declaration into the `.d.ts`. An
/// initialiser cannot: `export const instance: Indirect = new Indirect()` emits
/// `declare const instance: Indirect`, so the annotation is what makes `Indirect`
/// visible and the `new Indirect()` contributes nothing.
///
/// An earlier version collected from everything except function bodies, on the
/// theory that over-approximating was the safe direction. It was not merely
/// imprecise, it was wrong in a way the corpus catches three separate times:
///
/// - `declarationEmitIsolatedDeclarationErrorNotEmittedForNonEmittedFile` has
///   `const trpc = initTRPC.create()` reached only from the initialisers of the
///   exported consts. Upstream reports nothing for it; we reported `TS9010`.
/// - `isolatedDeclarationErrorsClassesExpressions` has `function id(...)` reached
///   only from `extends id(Base)` — an extends clause that upstream rejects
///   outright with `TS9021`, so nothing is dragged in.
/// - `computedPropertiesNarrowed` has `function ns()` reached only from
///   `[ns().v]: 1`.
///
/// The traversal therefore skips every expression-valued field rather than only
/// function bodies. Two expressions still count, and both are exact:
/// `export default a`, which emits a reference to `a`, and a heritage clause whose
/// expression is a plain entity name, which is the only kind that can be restated.
#[derive(Default)]
struct ReferenceCollector<'a> {
    names: Vec<&'a str>,
    /// Names an initializer uses as a *value* whose emitted type restates
    /// them: `x = C` emits `typeof C` and `new C()` emits `C` when `C` is a
    /// class, function or enum. Resolved against the declarations in the
    /// fixpoint, because a variable of the same name contributes its type, not
    /// its name.
    value_names: Vec<&'a str>,
    /// Bare identifiers written in a type reference (`x: Component`). They
    /// resolve in the *type* meaning, so a same-named variable or function is
    /// not what they name (`neverReturningFunctions1`: an exported interface
    /// `Component` beside a private `const Component`).
    type_names: Vec<&'a str>,
    bound_type_names: Vec<&'a str>,
    /// Identifiers that *declare* rather than reference: a declaration's own
    /// name and a non-computed member name. The generic walk visits them as
    /// identifiers, which made `export interface Component` reach a private
    /// `const Component` and a property signature `fooProps?:` reach a
    /// private `const fooProps` (`neverReturningFunctions1`,
    /// `nonPrimitiveAndEmptyObject`).
    declaring: FxHashSet<NodeId>,
}

impl<'a> ReferenceCollector<'a> {
    fn record_name(&mut self, name: &'a str) {
        if !self.bound_type_names.contains(&name) {
            self.names.push(name);
        }
    }

    fn record_entity_name(&mut self, name: &EntityName<'a>) {
        match name {
            EntityName::Identifier(identifier) => self.record_name(identifier.text),
            EntityName::QualifiedName(qualified) => {
                if let Some(left) = &qualified.left {
                    self.record_qualified_root(left);
                }
            }
        }
    }

    /// The leftmost identifier of a qualified name, recorded unconditionally.
    ///
    /// `E.Whatever` resolves `E` in *namespace* space, where type parameters
    /// never participate — so a signature's `<E>` must not shadow the
    /// namespace import the annotation actually names
    /// (`declarationEmitRetainedAnnotationRetainsImportInOutput`).
    fn record_qualified_root(&mut self, name: &EntityName<'a>) {
        match name {
            EntityName::Identifier(identifier) => self.names.push(identifier.text),
            EntityName::QualifiedName(qualified) => {
                if let Some(left) = &qualified.left {
                    self.record_qualified_root(left);
                }
            }
        }
    }

    /// The leftmost identifier of an entity-name-shaped expression, if it is one.
    ///
    /// `Base` and `a.b.Base` qualify; `id(Base)` does not.
    fn record_entity_expression(&mut self, expression: &Expression<'a>) {
        match expression {
            Expression::Identifier(identifier) => self.record_name(identifier.text),
            Expression::PropertyAccessExpression(access) => {
                if let Some(inner) = &access.expression {
                    self.record_entity_expression(inner);
                }
            }
            _ => {}
        }
    }

    /// Names that become part of the synthesized `_default` declaration's type.
    /// Unlike an ordinary initializer, a default-export expression is itself the
    /// exported declaration, so `export default new A()` must retain `A`.
    fn record_default_export_expression(&mut self, expression: &Expression<'a>) {
        match expression {
            Expression::NewExpression(new_expression) => {
                if let Some(callee) = &new_expression.expression {
                    self.record_entity_expression(callee);
                }
                for argument in new_expression.type_arguments {
                    self.visit_node(tsr_ast::Node::from(*argument));
                }
            }
            Expression::AsExpression(as_expression) => {
                self.visit_type(as_expression.r#type);
            }
            Expression::SatisfiesExpression(satisfies) => {
                self.visit_type(satisfies.r#type);
            }
            Expression::ParenthesizedExpression(parenthesized) => {
                if let Some(inner) = &parenthesized.expression {
                    self.record_default_export_expression(inner);
                }
            }
            // `export default { fn }` emits the literal's shape, so the written
            // signatures and value references inside it are restated. Its
            // computed keys are deliberately not collected: an entity key that
            // names an enum member prints as the member's value upstream
            // (`declarationEmitComputedNameConstEnumAlias` emits `TEST: {}`
            // and drops the import), which this builder cannot yet do.
            Expression::ObjectLiteralExpression(_)
            | Expression::ArrowFunction(_)
            | Expression::FunctionExpression(_) => {
                self.collect_initializer_types(expression);
            }
            other => self.record_entity_expression(other),
        }
    }

    /// Record the names used by computed keys inside an emitted literal.
    ///
    /// Only the keys: `{ [u]: v }` emits `[u]` and so needs `u`, but `v` is a value
    /// and never appears in a `.d.ts`.
    fn collect_computed_keys(&mut self, expression: &Expression<'a>) {
        match expression {
            Expression::ObjectLiteralExpression(object) => {
                for property in object.properties {
                    if let Some(name) = object_member_name(property)
                        && let tsr_ast::PropertyName::ComputedPropertyName(computed) = name
                        && let Some(inner) = &computed.expression
                    {
                        self.record_entity_expression(inner);
                    }
                    if let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) =
                        property
                        && let Some(value) = &assignment.initializer
                    {
                        self.collect_computed_keys(value);
                    }
                }
            }
            Expression::ArrayLiteralExpression(array) => {
                for element in array.elements {
                    self.collect_computed_keys(element);
                }
            }
            Expression::AsExpression(as_expression) => {
                if let Some(inner) = &as_expression.expression {
                    self.collect_computed_keys(inner);
                }
            }
            Expression::ParenthesizedExpression(inner) => {
                if let Some(inner) = &inner.expression {
                    self.collect_computed_keys(inner);
                }
            }
            _ => {}
        }
    }

    /// Collect written types that the syntactic type builder copies out of an
    /// initializer.
    ///
    /// Initializers are normally values and cannot make an import visible. An
    /// arrow or function expression is the important exception: its written
    /// parameter and return annotations become the emitted variable's function
    /// type. Ignoring those annotations dropped imports from declarations such as
    /// `export const f = (value: Imported): void => {}` even though no inference
    /// is needed to know that `Imported` survives.
    fn collect_initializer_types(&mut self, expression: &Expression<'a>) {
        match expression {
            Expression::ArrowFunction(function) => {
                let bound_len = self.bound_type_names.len();
                self.bound_type_names.extend(
                    function
                        .type_parameters
                        .iter()
                        .filter_map(|parameter| parameter.name.map(|name| name.text)),
                );
                for parameter in function.type_parameters {
                    self.visit_type_parameter_declaration(parameter);
                }
                for parameter in function.parameters {
                    self.visit_parameter_declaration(parameter);
                }
                self.visit_type(function.r#type);
                self.bound_type_names.truncate(bound_len);
            }
            Expression::FunctionExpression(function) => {
                let bound_len = self.bound_type_names.len();
                self.bound_type_names.extend(
                    function
                        .type_parameters
                        .iter()
                        .filter_map(|parameter| parameter.name.map(|name| name.text)),
                );
                for parameter in function.type_parameters {
                    self.visit_type_parameter_declaration(parameter);
                }
                for parameter in function.parameters {
                    self.visit_parameter_declaration(parameter);
                }
                self.visit_type(function.r#type);
                self.bound_type_names.truncate(bound_len);
            }
            Expression::AsExpression(as_expression) => {
                self.visit_type(as_expression.r#type);
            }
            Expression::SatisfiesExpression(satisfies) => {
                self.visit_type(satisfies.r#type);
            }
            Expression::ParenthesizedExpression(parenthesized) => {
                if let Some(inner) = &parenthesized.expression {
                    self.collect_initializer_types(inner);
                }
            }
            Expression::Identifier(identifier) => {
                if !self.bound_type_names.contains(&identifier.text) {
                    self.value_names.push(identifier.text);
                }
            }
            Expression::NewExpression(new_expression) => {
                if new_expression.type_arguments.is_empty()
                    && let Some(Expression::Identifier(callee)) = &new_expression.expression
                    && !self.bound_type_names.contains(&callee.text)
                {
                    self.value_names.push(callee.text);
                }
            }
            // An object literal's methods and accessors keep their written
            // signatures in the emitted type literal, and its property values
            // contribute their own shapes: `{ m(): this is Foo {…} }` emits
            // `m(): this is Foo;` and needs `Foo`
            // (`declarationEmitThisPredicatesWithPrivateName02`).
            Expression::ObjectLiteralExpression(object) => {
                use tsr_ast::ObjectLiteralElementLike as Member;
                for property in object.properties {
                    match property {
                        Member::PropertyAssignment(assignment) => {
                            if let Some(value) = &assignment.initializer {
                                self.collect_initializer_types(value);
                            }
                        }
                        Member::MethodDeclaration(method) => {
                            let bound_len = self.bound_type_names.len();
                            self.bound_type_names.extend(
                                method
                                    .type_parameters
                                    .iter()
                                    .filter_map(|parameter| parameter.name.map(|name| name.text)),
                            );
                            for parameter in method.type_parameters {
                                self.visit_type_parameter_declaration(parameter);
                            }
                            for parameter in method.parameters {
                                self.visit_parameter_declaration(parameter);
                            }
                            self.visit_type(method.r#type);
                            self.bound_type_names.truncate(bound_len);
                        }
                        Member::GetAccessorDeclaration(accessor) => {
                            self.visit_type(accessor.r#type);
                        }
                        Member::SetAccessorDeclaration(accessor) => {
                            for parameter in accessor.parameters {
                                self.visit_parameter_declaration(parameter);
                            }
                        }
                        Member::ShorthandPropertyAssignment(shorthand) => {
                            if let tsr_ast::PropertyName::Identifier(name) = shorthand.name
                                && !self.bound_type_names.contains(&name.text)
                            {
                                self.value_names.push(name.text);
                            }
                        }
                        Member::SpreadAssignment(_) => {}
                    }
                }
            }
            Expression::ArrayLiteralExpression(array) => {
                for element in array.elements {
                    self.collect_initializer_types(element);
                }
            }
            _ => {}
        }
    }

    fn visit_type(&mut self, node: Option<tsr_ast::TypeNode<'a>>) {
        if let Some(node) = node {
            self.visit_node(tsr_ast::Node::from(node));
        }
    }
}

impl<'a> ReferenceCollector<'a> {
    fn declares(&mut self, name: Option<&tsr_ast::Identifier<'a>>) {
        if let Some(id) = name.and_then(|name| name.node_id) {
            self.declaring.insert(id);
        }
    }

    fn declares_member(&mut self, name: &tsr_ast::PropertyName<'a>) {
        if let tsr_ast::PropertyName::Identifier(identifier) = name {
            self.declares(Some(identifier));
        }
    }
}

impl<'a> Visit<'a> for ReferenceCollector<'a> {
    fn visit_identifier(&mut self, node: &'a tsr_ast::Identifier<'a>) {
        if node.node_id.is_some_and(|id| self.declaring.contains(&id)) {
            return;
        }
        self.record_name(node.text);
    }

    fn visit_interface_declaration(&mut self, node: &'a tsr_ast::InterfaceDeclaration<'a>) {
        self.declares(node.name);
        tsr_ast::visit::walk_interface_declaration(self, node);
    }

    fn visit_class_declaration(&mut self, node: &'a tsr_ast::ClassDeclaration<'a>) {
        self.declares(node.name);
        tsr_ast::visit::walk_class_declaration(self, node);
    }

    fn visit_function_declaration(&mut self, node: &'a tsr_ast::FunctionDeclaration<'a>) {
        self.declares(node.name);
        tsr_ast::visit::walk_function_declaration(self, node);
    }

    fn visit_type_alias_declaration(&mut self, node: &'a tsr_ast::TypeAliasDeclaration<'a>) {
        self.declares(node.name);
        tsr_ast::visit::walk_type_alias_declaration(self, node);
    }

    fn visit_enum_declaration(&mut self, node: &'a tsr_ast::EnumDeclaration<'a>) {
        self.declares(node.name);
        tsr_ast::visit::walk_enum_declaration(self, node);
    }

    fn visit_property_signature_declaration(
        &mut self,
        node: &'a tsr_ast::PropertySignatureDeclaration<'a>,
    ) {
        self.declares_member(&node.name);
        tsr_ast::visit::walk_property_signature_declaration(self, node);
    }

    fn visit_method_signature_declaration(
        &mut self,
        node: &'a tsr_ast::MethodSignatureDeclaration<'a>,
    ) {
        self.declares_member(&node.name);
        tsr_ast::visit::walk_method_signature_declaration(self, node);
    }

    fn visit_method_declaration(&mut self, node: &'a tsr_ast::MethodDeclaration<'a>) {
        self.declares_member(&node.name);
        tsr_ast::visit::walk_method_declaration(self, node);
    }

    fn visit_get_accessor_declaration(&mut self, node: &'a tsr_ast::GetAccessorDeclaration<'a>) {
        self.declares_member(&node.name);
        tsr_ast::visit::walk_get_accessor_declaration(self, node);
    }

    fn visit_set_accessor_declaration(&mut self, node: &'a tsr_ast::SetAccessorDeclaration<'a>) {
        self.declares_member(&node.name);
        tsr_ast::visit::walk_set_accessor_declaration(self, node);
    }

    fn visit_type_reference_node(&mut self, node: &'a tsr_ast::TypeReferenceNode<'a>) {
        match &node.type_name {
            Some(EntityName::Identifier(identifier)) => {
                if !self.bound_type_names.contains(&identifier.text) {
                    self.type_names.push(identifier.text);
                }
            }
            Some(name) => self.record_entity_name(name),
            None => {}
        }
        for argument in node.type_arguments {
            self.visit_node(tsr_ast::Node::from(*argument));
        }
    }

    fn visit_module_declaration(&mut self, node: &'a tsr_ast::ModuleDeclaration<'a>) {
        let bound_len = self.bound_type_names.len();
        match node.body {
            Some(tsr_ast::ModuleBody::ModuleBlock(block)) => {
                // Declarations in an augmentation or namespace bind names in that
                // module's scope. In `declare module "./m" { interface A {} }`,
                // the declaration name `A` must not retain a top-level `import
                // { A } from "./m"`; genuine external references from member
                // types remain unbound and still retain their imports.
                for statement in block.statements {
                    self.bound_type_names.extend(declared_names(statement));
                }
                for statement in block.statements {
                    self.visit_node(tsr_ast::Node::from(*statement));
                }
            }
            Some(tsr_ast::ModuleBody::ModuleDeclaration(inner)) => {
                self.visit_module_declaration(inner);
            }
            None => {}
        }
        self.bound_type_names.truncate(bound_len);
    }

    // A body emits nothing, so it can make nothing visible.
    fn visit_block(&mut self, _node: &'a tsr_ast::Block<'a>) {}

    // Every declaration form below is visited for its *type* only; its initialiser
    // is deliberately not traversed. See the type docs.
    fn visit_variable_declaration(&mut self, node: &'a tsr_ast::VariableDeclaration<'a>) {
        self.visit_type(node.r#type);
        // An *unannotated* declaration emits the shape of its initialiser, and a
        // computed key inside that shape is restated verbatim -- so the name it
        // uses has to be emitted too. `computedPropertiesNarrowed` needs this:
        // `let u = Symbol()` is not exported and upstream still reports TS9010 on
        // it, because `export let o4 = { [u]: 1 }` puts `u` in the output.
        if node.r#type.is_none()
            && let Some(initializer) = &node.initializer
        {
            self.collect_computed_keys(initializer);
            self.collect_initializer_types(initializer);
            // A binding pattern destructuring an entity emits
            // `typeof <entity>`, so the entity's root must stay reachable in
            // the source phase too (`declarationEmitExpressionInExtends6`).
            if matches!(node.name, Some(tsr_ast::BindingName::BindingPattern(_))) {
                self.record_entity_expression(initializer);
            }
        }
    }

    fn visit_property_declaration(&mut self, node: &'a tsr_ast::PropertyDeclaration<'a>) {
        // A computed name is restated in the `.d.ts` even when the member is
        // private and its type is dropped (`private [_data];`), so the entity
        // it names must be emitted too — upstream's `checkEntityNameVisibility`
        // on the computed expression
        // (`declarationEmitPrivateSymbolCausesVarDeclarationToBeEmitted`).
        if let tsr_ast::PropertyName::ComputedPropertyName(computed) = &node.name
            && let Some(expression) = &computed.expression
        {
            self.record_entity_expression(expression);
        }
        self.visit_type(node.r#type);
        if node.r#type.is_none()
            && let Some(initializer) = &node.initializer
        {
            self.collect_computed_keys(initializer);
            self.collect_initializer_types(initializer);
        }
    }

    fn visit_parameter_declaration(&mut self, node: &'a tsr_ast::ParameterDeclaration<'a>) {
        self.visit_type(node.r#type);
    }

    fn visit_type_parameter_declaration(
        &mut self,
        node: &'a tsr_ast::TypeParameterDeclaration<'a>,
    ) {
        // The declared name is a local binding, not a reference to a top-level
        // declaration or import with the same spelling. Its constraint and
        // default are type positions and can contain real references.
        self.visit_type(node.constraint);
        self.visit_type(node.default_type);
    }

    fn visit_property_assignment(&mut self, node: &'a tsr_ast::PropertyAssignment<'a>) {
        self.visit_type(node.r#type);
    }

    fn visit_enum_member(&mut self, _node: &'a tsr_ast::EnumMember<'a>) {}

    fn visit_binding_element(&mut self, node: &'a tsr_ast::BindingElement<'a>) {
        if let Some(name) = &node.name {
            self.visit_node(tsr_ast::Node::from(*name));
        }
    }

    fn visit_expression_statement(&mut self, _node: &'a tsr_ast::ExpressionStatement<'a>) {}

    /// A heritage clause contributes only when it names something restatable.
    fn visit_expression_with_type_arguments(
        &mut self,
        node: &'a tsr_ast::ExpressionWithTypeArguments<'a>,
    ) {
        if let Some(expression) = &node.expression {
            self.record_entity_expression(expression);
        }
        for argument in node.type_arguments {
            self.visit_node(tsr_ast::Node::from(*argument));
        }
    }

    /// `export default a` emits a reference to `a`, so `a` must be emitted too.
    fn visit_export_assignment(&mut self, node: &'a tsr_ast::ExportAssignment<'a>) {
        if let Some(expression) = &node.expression {
            self.record_default_export_expression(expression);
        }
    }
}

/// Whether a top-level statement declares something a bare type reference can
/// name: an interface, type alias, class or enum, or an import (whose meaning
/// this pass cannot see).
fn has_type_meaning(statement: &Statement<'_>) -> bool {
    matches!(
        statement,
        Statement::InterfaceDeclaration(_)
            | Statement::TypeAliasDeclaration(_)
            | Statement::ClassDeclaration(_)
            | Statement::EnumDeclaration(_)
            | Statement::ImportDeclaration(_)
            | Statement::ImportEqualsDeclaration(_)
    )
}

/// Whether a declaration's value is the symbol's own anonymous type, which an
/// emitted reference restates as `typeof Name`: a class, a function, a
/// non-const enum, or a namespace merged with one of them.
fn restates_as_typeof(statement: &Statement<'_>) -> bool {
    match statement {
        Statement::ClassDeclaration(_)
        | Statement::FunctionDeclaration(_)
        | Statement::ModuleDeclaration(_) => true,
        Statement::EnumDeclaration(enumeration) => !enumeration.modifiers.iter().any(|modifier| {
            matches!(modifier, ModifierLike::Token(token) if token.kind == SyntaxKind::ConstKeyword)
        }),
        _ => false,
    }
}

/// The property name of an object-literal member, when it has one.
fn object_member_name<'b, 'a>(
    property: &'b tsr_ast::ObjectLiteralElementLike<'a>,
) -> Option<&'b tsr_ast::PropertyName<'a>> {
    use tsr_ast::ObjectLiteralElementLike as Member;
    match property {
        Member::PropertyAssignment(node) => Some(&node.name),
        Member::MethodDeclaration(node) => Some(&node.name),
        Member::GetAccessorDeclaration(node) => Some(&node.name),
        Member::SetAccessorDeclaration(node) => Some(&node.name),
        Member::ShorthandPropertyAssignment(node) => Some(&node.name),
        Member::SpreadAssignment(_) => None,
    }
}
