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
    EntityName, ModifierLike, NamedExportBindings, NodeId, SourceFile, Statement, SyntaxKind, Visit,
};

/// The top-level declarations that reach the `.d.ts`, keyed by statement node id.
#[derive(Debug, Default)]
pub struct Visible {
    ids: FxHashSet<NodeId>,
}

impl Visible {
    /// Whether this statement is emitted into the `.d.ts`.
    #[must_use]
    pub fn contains(&self, id: Option<NodeId>) -> bool {
        id.is_some_and(|id| self.ids.contains(&id))
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
#[must_use]
pub fn visible_declarations<'a>(file: &'a SourceFile<'a>) -> Visible {
    let by_name = index_by_name(file.statements);

    // A *script* — no imports, no exports — emits every top-level declaration,
    // because there is no export list to be reachable from. `isolatedDeclarationErrors`
    // is exactly this shape: not one `export` in the file, and upstream still
    // reports four errors in it.
    if !is_module(file.statements) {
        let ids = file.statements.iter().filter_map(Statement::node_id).collect();
        return Visible { ids };
    }

    let mut visible: FxHashSet<NodeId> = FxHashSet::default();
    let mut queue: Vec<&Statement<'a>> = Vec::new();

    for statement in file.statements {
        if exports_something(statement) {
            if let Some(id) = statement.node_id() {
                visible.insert(id);
            }
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
                    && let Some(target) = by_name.get(name)
                    && let Some(id) = target.node_id()
                    && visible.insert(id)
                {
                    queue.push(target);
                }
            }
        }
    }

    // Fixpoint: a visible declaration drags in whatever it names.
    while let Some(statement) = queue.pop() {
        let mut collector = ReferenceCollector::default();
        collector.visit_node(tsr_ast::Node::from(*statement));
        for name in collector.names {
            if let Some(target) = by_name.get(name)
                && let Some(id) = target.node_id()
                && visible.insert(id)
            {
                queue.push(target);
            }
        }
    }

    Visible { ids: visible }
}

/// Index top-level declarations by the name they introduce.
///
/// Later declarations win on collision, which is wrong for merged declarations
/// (`interface` + `interface`, `namespace` + `function`) and right for nothing.
/// It is tolerable only because the map is used to *reach* declarations, and a
/// merged pair is nearly always reached together through some other edge. Merged
/// declarations are the known gap here.
fn index_by_name<'a, 'b>(statements: &'b [Statement<'a>]) -> FxHashMap<&'a str, &'b Statement<'a>> {
    let mut map = FxHashMap::default();
    for statement in statements {
        for name in declared_names(statement) {
            map.insert(name, statement);
        }
    }
    map
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
                    if let Some(tsr_ast::BindingName::Identifier(n)) = &declaration.name {
                        names.push(n.text);
                    }
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

/// Collects every name a declaration mentions outside a function body.
///
/// Function bodies are skipped because nothing inside one is emitted: a `.d.ts`
/// keeps signatures and drops implementations. Everything else is collected, which
/// over-approximates — see the module docs for why that direction is the safe one.
#[derive(Default)]
struct ReferenceCollector<'a> {
    names: Vec<&'a str>,
}

impl<'a> ReferenceCollector<'a> {
    fn record_entity_name(&mut self, name: &EntityName<'a>) {
        match name {
            EntityName::Identifier(identifier) => self.names.push(identifier.text),
            EntityName::QualifiedName(qualified) => {
                if let Some(left) = &qualified.left {
                    self.record_entity_name(left);
                }
            }
        }
    }
}

impl<'a> Visit<'a> for ReferenceCollector<'a> {
    fn visit_identifier(&mut self, node: &'a tsr_ast::Identifier<'a>) {
        self.names.push(node.text);
    }

    fn visit_block(&mut self, _node: &'a tsr_ast::Block<'a>) {
        // A function body emits nothing, so it can make nothing visible.
    }

    fn visit_type_reference_node(&mut self, node: &'a tsr_ast::TypeReferenceNode<'a>) {
        if let Some(name) = &node.type_name {
            self.record_entity_name(name);
        }
        for argument in node.type_arguments {
            self.visit_node(tsr_ast::Node::from(*argument));
        }
    }
}
