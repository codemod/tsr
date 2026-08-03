//! The walk: create symbols, put them in the right table.
//!
//! Ported from `internal/binder/binder.go`. The structure follows upstream's —
//! `bindContainer` saves the current scope, recurses, and restores — with the
//! difference that scope state is held in locals here rather than in fields on a
//! long-lived binder, since the recursion is a plain function call.
//!
//! **What this does not do yet:** the control-flow graph. `ContainerFlags`
//! records `IS_CONTROL_FLOW_CONTAINER` faithfully, and nothing consumes it. Flow
//! nodes are what narrowing needs, they are roughly half of upstream's binder, and
//! they are filed separately — building symbols first means the checker has
//! something to resolve against while the flow graph is written.

use tsr_ast::{Node, NodeId, NodeTable, SourceFile, push_children};
use tsr_core::Idx as _;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    BindResult,
    container::{ContainerFlags, container_flags},
    symbol::{SymbolFlags, SymbolId, SymbolStore, SymbolTable},
};

/// Where a declaration's symbol belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destination {
    /// The enclosing container's `locals` — ordinary lexical scope.
    Locals,
    /// The container symbol's `members` — class, interface, enum, type literal.
    Members,
    /// The container symbol's `exports` — module or namespace.
    ///
    /// Not yet constructed: routing an `export`ed declaration here needs module
    /// versus script detection, which needs module resolution. Kept because the
    /// table it targets already exists on `Symbol` and the destination is where
    /// that decision will land.
    #[expect(dead_code, reason = "awaiting export handling; see lib.rs")]
    Exports,
}

pub(crate) struct Binder<'a, 'n> {
    nodes: &'n NodeTable,
    symbols: SymbolStore<'a>,
    /// `node -> the symbol it declares`. Dense, because node ids are dense.
    node_symbols: Vec<Option<SymbolId>>,
    /// `container node -> its locals`. Sparse: most nodes are not containers.
    locals: rustc_hash::FxHashMap<NodeId, SymbolTable<'a>>,
    diagnostics: Vec<Diagnostic>,
}

/// The scope a declaration lands in.
#[derive(Clone, Copy)]
struct Scope {
    /// Nearest node with a `locals` table.
    container: NodeId,
    /// Nearest block, for `let`/`const` and class declarations.
    block: NodeId,
    /// Symbol of the nearest container that owns members or exports.
    owner: Option<SymbolId>,
}

impl<'a, 'n> Binder<'a, 'n> {
    pub(crate) fn new(nodes: &'n NodeTable) -> Self {
        Self {
            nodes,
            symbols: SymbolStore::new(),
            node_symbols: vec![None; nodes.len()],
            locals: rustc_hash::FxHashMap::default(),
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn bind_source_file(mut self, file: &'a SourceFile<'a>) -> BindResult<'a> {
        let root = Node::SourceFile(file);
        let root_id = root.node_id().expect("the source file is registered");
        self.locals.insert(root_id, SymbolTable::default());

        // A file's own symbol exists only for a module; a script's declarations
        // are globals. Distinguishing the two needs module resolution, which does
        // not exist, so this binds every file as a script and records the gap.
        let scope = Scope { container: root_id, block: root_id, owner: None };
        for statement in file.statements {
            self.bind(Node::from(*statement), scope);
        }

        BindResult {
            symbols: self.symbols,
            node_symbols: self.node_symbols,
            locals: self.locals,
            diagnostics: self.diagnostics,
        }
    }

    /// Bind `node` and everything under it.
    fn bind(&mut self, node: Node<'a>, scope: Scope) {
        let Some(id) = node.node_id() else {
            // Unregistered: descend without treating it as a declaration.
            self.bind_children(node, scope);
            return;
        };

        // Declare first, then descend: a class's own name is visible inside its
        // body, and its members go in the symbol this call creates.
        let declared = self.declare(node, id, scope);
        let inner = self.enter(node, id, scope, declared);
        self.bind_children(node, inner);
    }

    fn bind_children(&mut self, node: Node<'a>, scope: Scope) {
        // Reused across the recursion would be better, but the children have to
        // outlive the recursive calls that consume them, so each level gets its
        // own. Small and short-lived.
        let mut children = Vec::new();
        push_children(node, &mut children);
        for child in children {
            self.bind(child, scope);
        }
    }

    /// The scope `node`'s children see.
    fn enter(
        &mut self,
        node: Node<'a>,
        id: NodeId,
        scope: Scope,
        declared: Option<SymbolId>,
    ) -> Scope {
        let flags = container_flags(node, self.nodes);
        let mut inner = scope;

        if flags.contains(ContainerFlags::IS_CONTAINER) {
            inner.block = id;
            if flags.contains(ContainerFlags::HAS_LOCALS) {
                inner.container = id;
                self.locals.entry(id).or_default();
            }
            // A container that owns members or exports becomes the symbol its
            // children declare into. A function-like container does not: its
            // parameters and locals are lexical, not members.
            if !flags.contains(ContainerFlags::IS_FUNCTION_LIKE)
                && let Some(symbol) = declared
            {
                inner.owner = Some(symbol);
            }
        } else if flags.contains(ContainerFlags::IS_BLOCK_SCOPED_CONTAINER) {
            inner.block = id;
            if flags.contains(ContainerFlags::HAS_LOCALS) {
                self.locals.entry(id).or_default();
            }
        }
        inner
    }

    /// What kind of symbol `node` declares, and where it belongs.
    ///
    /// Wraps the kind-only [`classify`] with the one case that needs more than
    /// the node: a variable's scope depends on whether it was written `var`,
    /// `let`, or `const`, and all three produce the same node kind. Upstream keeps
    /// that in node flags and so do we, so it is reached through the side table
    /// rather than the tree.
    fn classify(&self, node: Node<'a>, id: NodeId) -> Option<(SymbolFlags, Destination)> {
        if matches!(node, Node::VariableDeclaration(_)) {
            let list_flags = self
                .nodes
                .parent(id)
                .map_or_else(tsr_ast::NodeFlags::empty, |list| self.nodes.flags(list));
            let block_scoped = list_flags.intersects(
                tsr_ast::NodeFlags::LET | tsr_ast::NodeFlags::CONST | tsr_ast::NodeFlags::USING,
            );
            return Some((
                if block_scoped {
                    SymbolFlags::BLOCK_SCOPED_VARIABLE
                } else {
                    SymbolFlags::FUNCTION_SCOPED_VARIABLE
                },
                Destination::Locals,
            ));
        }
        classify(node)
    }

    /// Create a symbol for `node` if it declares one.
    fn declare(&mut self, node: Node<'a>, id: NodeId, scope: Scope) -> Option<SymbolId> {
        let (flags, destination) = self.classify(node, id)?;
        let name = declaration_name(node)?;

        let table_owner = match destination {
            // `var` and functions go to the nearest *function* scope; `let`,
            // `const`, and classes go to the nearest block. This is the whole of
            // `var` hoisting.
            Destination::Locals => {
                if flags.intersects(SymbolFlags::BLOCK_SCOPED_VARIABLE | SymbolFlags::CLASS) {
                    scope.block
                } else {
                    scope.container
                }
            }
            Destination::Members | Destination::Exports => scope.container,
        };

        let symbol = self.declare_into(destination, table_owner, scope, name, flags, id);
        self.node_symbols[id.index()] = Some(symbol);
        Some(symbol)
    }

    /// Add `name` to the appropriate table, merging with an existing symbol where
    /// TypeScript allows it and reporting a duplicate where it does not.
    fn declare_into(
        &mut self,
        destination: Destination,
        table_owner: NodeId,
        scope: Scope,
        name: &'a str,
        flags: SymbolFlags,
        declaration: NodeId,
    ) -> SymbolId {
        let existing = match destination {
            Destination::Locals => {
                self.locals.get(&table_owner).and_then(|table| table.get(name).copied())
            }
            Destination::Members => {
                scope.owner.and_then(|owner| self.symbols.get(owner).members.get(name).copied())
            }
            Destination::Exports => {
                scope.owner.and_then(|owner| self.symbols.get(owner).exports.get(name).copied())
            }
        };

        let symbol = if let Some(existing) = existing {
            {
                let existing_flags = self.symbols.get(existing).flags;
                if flags.excludes().intersects(existing_flags) {
                    // A genuine redeclaration. Upstream reports on every
                    // declaration involved, not only the second.
                    self.diagnostics.push(Diagnostic::with_args(
                        &messages::DUPLICATE_IDENTIFIER_0,
                        self.nodes.span(declaration),
                        [name.to_string()],
                    ));
                    // Still merge, so the checker has one symbol to resolve
                    // against rather than a hole. Upstream does the same.
                }
                self.symbols.get_mut(existing).flags |= flags;
                existing
            }
        } else {
            {
                let created = self.symbols.create(name, flags);
                match destination {
                    Destination::Locals => {
                        self.locals.entry(table_owner).or_default().insert(name, created);
                    }
                    Destination::Members => {
                        if let Some(owner) = scope.owner {
                            self.symbols.get_mut(owner).members.insert(name, created);
                            self.symbols.get_mut(created).parent = Some(owner);
                        }
                    }
                    Destination::Exports => {
                        if let Some(owner) = scope.owner {
                            self.symbols.get_mut(owner).exports.insert(name, created);
                            self.symbols.get_mut(created).parent = Some(owner);
                        }
                    }
                }
                created
            }
        };

        let entry = self.symbols.get_mut(symbol);
        entry.declarations.push(declaration);
        if entry.value_declaration.is_none() && flags.intersects(SymbolFlags::VALUE) {
            entry.value_declaration = Some(declaration);
        }
        symbol
    }
}

/// What kind of symbol `node` declares, and where it belongs.
///
/// `None` for nodes that declare nothing, which is most of them.
fn classify(node: Node<'_>) -> Option<(SymbolFlags, Destination)> {
    use Destination as D;
    use SymbolFlags as S;
    Some(match node {
        Node::FunctionDeclaration(_) => (S::FUNCTION, D::Locals),
        Node::ClassDeclaration(_) | Node::ClassExpression(_) => (S::CLASS, D::Locals),
        Node::InterfaceDeclaration(_) => (S::INTERFACE, D::Locals),
        Node::TypeAliasDeclaration(_) => (S::TYPE_ALIAS, D::Locals),
        Node::EnumDeclaration(_) => (S::REGULAR_ENUM, D::Locals),
        Node::ModuleDeclaration(_) => (S::VALUE_MODULE, D::Locals),
        Node::TypeParameterDeclaration(_) => (S::TYPE_PARAMETER, D::Locals),
        Node::ParameterDeclaration(_) => (S::FUNCTION_SCOPED_VARIABLE, D::Locals),

        // Members.
        Node::PropertyDeclaration(_)
        | Node::PropertySignatureDeclaration(_)
        | Node::PropertyAssignment(_)
        | Node::ShorthandPropertyAssignment(_) => (S::PROPERTY, D::Members),
        Node::MethodDeclaration(_) | Node::MethodSignatureDeclaration(_) => (S::METHOD, D::Members),
        Node::GetAccessorDeclaration(_) => (S::GET_ACCESSOR, D::Members),
        Node::SetAccessorDeclaration(_) => (S::SET_ACCESSOR, D::Members),
        Node::EnumMember(_) => (S::ENUM_MEMBER, D::Members),

        // Imports and exports are aliases for symbols elsewhere. Resolving them
        // needs module resolution; the binder only records that they exist.
        Node::ImportClause(_)
        | Node::ImportSpecifier(_)
        | Node::NamespaceImport(_)
        | Node::ExportSpecifier(_)
        | Node::ImportEqualsDeclaration(_) => (S::ALIAS, D::Locals),

        _ => return None,
    })
}

/// The declared name, if the node has a simple one.
///
/// `None` for computed names (`[expr]`) and for destructuring patterns. Both
/// declare symbols in TypeScript — a binding pattern declares one per element —
/// and neither is handled yet; see the note in `lib.rs`.
fn declaration_name(node: Node<'_>) -> Option<&str> {
    fn from_property_name(name: tsr_ast::PropertyName<'_>) -> Option<&str> {
        match name {
            tsr_ast::PropertyName::Identifier(i) => Some(i.text),
            tsr_ast::PropertyName::StringLiteral(s) => Some(s.text),
            tsr_ast::PropertyName::NumericLiteral(n) => Some(n.text),
            tsr_ast::PropertyName::PrivateIdentifier(p) => Some(p.text),
            tsr_ast::PropertyName::ComputedPropertyName(_) => None,
            tsr_ast::PropertyName::BigIntLiteral(b) => Some(b.text),
            tsr_ast::PropertyName::NoSubstitutionTemplateLiteral(t) => Some(t.text),
        }
    }

    match node {
        Node::FunctionDeclaration(n) => n.name.map(|i| i.text),
        Node::ClassDeclaration(n) => n.name.map(|i| i.text),
        Node::ClassExpression(n) => n.name.map(|i| i.text),
        Node::InterfaceDeclaration(n) => n.name.map(|i| i.text),
        Node::TypeAliasDeclaration(n) => n.name.map(|i| i.text),
        Node::EnumDeclaration(n) => n.name.map(|i| i.text),
        Node::TypeParameterDeclaration(n) => n.name.map(|i| i.text),
        Node::ModuleDeclaration(n) => n.name.map(module_name),
        Node::ParameterDeclaration(n) => n.name.and_then(binding_name),
        Node::VariableDeclaration(n) => n.name.and_then(binding_name),
        Node::PropertyDeclaration(n) => from_property_name(n.name),
        Node::PropertySignatureDeclaration(n) => from_property_name(n.name),
        Node::MethodDeclaration(n) => from_property_name(n.name),
        Node::MethodSignatureDeclaration(n) => from_property_name(n.name),
        Node::GetAccessorDeclaration(n) => from_property_name(n.name),
        Node::SetAccessorDeclaration(n) => from_property_name(n.name),
        Node::EnumMember(n) => from_property_name(n.name),
        Node::PropertyAssignment(n) => from_property_name(n.name),
        Node::ShorthandPropertyAssignment(n) => from_property_name(n.name),
        Node::ImportClause(n) => n.name.map(|i| i.text),
        Node::ImportSpecifier(n) => n.name.map(|i| i.text),
        Node::NamespaceImport(n) => n.name.map(|i| i.text),
        Node::ExportSpecifier(n) => n.name.map(export_name),
        Node::ImportEqualsDeclaration(n) => n.name.map(|i| i.text),
        _ => None,
    }
}

fn module_name(name: tsr_ast::ModuleName<'_>) -> &str {
    match name {
        tsr_ast::ModuleName::Identifier(i) => i.text,
        tsr_ast::ModuleName::StringLiteral(s) => s.text,
    }
}

fn binding_name(name: tsr_ast::BindingName<'_>) -> Option<&str> {
    match name {
        tsr_ast::BindingName::Identifier(i) => Some(i.text),
        // Destructuring declares one symbol per element; not handled yet.
        tsr_ast::BindingName::BindingPattern(_) => None,
    }
}

/// The name of an `export { … }` specifier, which may be a string.
fn export_name(name: tsr_ast::ModuleExportName<'_>) -> &str {
    match name {
        tsr_ast::ModuleExportName::Identifier(i) => i.text,
        tsr_ast::ModuleExportName::StringLiteral(s) => s.text,
    }
}
