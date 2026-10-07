//! The TypeScript AST.
//!
//! Node shapes, syntax kinds, and unions are **generated** from
//! `vendor/typescript-go/_scripts/ast.json` — the same schema-validated
//! definition upstream feeds to its own Go generator. Conformance is therefore a
//! property of the build: regenerate, and any upstream change appears as a diff.
//! See `tests/kind_conformance.rs` for the assertions that keep it honest.
//!
//! # Shape of the AST
//!
//! Per PLAN.md §3.2, *the tree is a tree*. Node structs hold only syntax children,
//! as direct arena references. Everything cyclic or phase-specific — parent,
//! symbol, scope, type, flow node — lives in id-keyed side tables ([`tsr_core`]),
//! which is what upstream also does via its 23 `LinkStore`s and what oxc does via
//! `AstNodes`.
//!
//! Concretely, fields upstream marks `goOnly` (`Symbol`, `Locals`, `FlowNode`,
//! `NextContainer`, `facts`) are absent from these structs by design.

mod flags;
mod generated;
mod node_map;
pub mod parent;
pub mod predicates;
pub mod publication;
pub use generated::visit::{for_each_child_id, push_children};
pub use node_map::NodeMap;
pub use parent::assign_parents;
pub use predicates::Tree;

pub use flags::{ModifierFlags, NodeFlags, TokenFlags};
pub use generated::visit::Visit;
pub use generated::{alias::*, kind::SyntaxKind, nodes::*, visit};
use tsr_core::{Span, define_index};

define_index! {
    /// Identifies a node within the [`NodeTable`] it was assigned from.
    ///
    /// Assigned by the parser and used to key every side table. **The scope is
    /// the table, not the file** — `NodeTable::push` hands out `self.len()`, so
    /// parsing several files into one table numbers them all uniquely, which is
    /// what `tsr_parser::parse_into` does for the files of a program
    /// ([ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)).
    /// Parsing each file into its own table, which is what
    /// `tsr_parser::parse_with_options` does, makes the scope the file.
    ///
    /// This doc comment previously read "scoped per file, not per program", and
    /// that was the invariant the checker was built against: it is why handing
    /// it a symbol from another file reads the wrong declarations. The property
    /// was never enforced by anything — it was a description of how the parser
    /// happened to be called.
    pub struct NodeId;
}

/// Nodes that carry a [`NodeId`] slot.
///
/// Implemented for every generated node, so the parser can register any of them
/// through one generic path instead of repeating the assignment per node type.
pub trait HasNodeId {
    /// Record the id assigned by the parser.
    ///
    /// Takes `&mut`: the arena returns `&mut` for a freshly allocated value, so
    /// registration happens before any shared reference exists and no interior
    /// mutability is needed. Keeping it that way is what makes the AST `Sync`.
    fn set_node_id(&mut self, id: NodeId);

    /// The id, or `None` if the node has not been registered.
    fn node_id(&self) -> Option<NodeId>;
}

/// A token node.
///
/// Upstream models tokens as a generic `Token[TKind]` with 33 named
/// instantiations (`AsteriskToken`, `QuestionToken`, …). Since the instantiations
/// differ only in which kinds they admit, they collapse here into one type
/// carrying its kind — the constraint is documented on each field that uses it.
/// Deliberately not `Copy`, unlike every other single-field node: the
/// `node_id` is what makes a token findable in the side tables, and a `Copy`
/// token would silently duplicate an id that identifies one position.
#[derive(Debug, Clone)]
pub struct Token<'a> {
    /// Which token this is.
    pub kind: SyntaxKind,
    /// Key into the side tables holding this node's kind, span, and parent.
    pub node_id: Option<NodeId>,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl Token<'_> {
    /// Create a token of the given kind.
    #[must_use]
    pub const fn new(kind: SyntaxKind) -> Self {
        Self { kind, node_id: None, _marker: std::marker::PhantomData }
    }
}

impl HasNodeId for Token<'_> {
    fn set_node_id(&mut self, id: NodeId) {
        self.node_id = Some(id);
    }

    fn node_id(&self) -> Option<NodeId> {
        self.node_id
    }
}

/// Per-node data assigned during parsing and binding.
///
/// A struct-of-arrays side table rather than fields on each node: the checker
/// reads parents constantly without touching flags, and keeping the columns apart
/// means those reads do not drag flag bytes through cache. Upstream reads
/// `.Parent` 2,092 times across `checker`, `ls`, and `binder`.
#[derive(Debug, Default)]
pub struct NodeTable {
    parent: Vec<Option<NodeId>>,
    kind: Vec<SyntaxKind>,
    span: Vec<Span>,
    flags: Vec<NodeFlags>,
    type_argument_lists: Vec<(NodeId, Span)>,
}

impl NodeTable {
    /// Append a completed file's private rows at a canonical program base.
    /// Spans remain file-relative; only parent identities are relocated.
    pub fn append_relocated(&mut self, local: &Self) -> std::ops::Range<u32> {
        let base = u32::try_from(self.len()).expect("node count exceeds u32");
        let end = base
            .checked_add(u32::try_from(local.len()).expect("node count exceeds u32"))
            .expect("node count exceeds u32");
        self.kind.extend_from_slice(&local.kind);
        self.span.extend_from_slice(&local.span);
        self.flags.extend_from_slice(&local.flags);
        self.type_argument_lists.extend(local.type_argument_lists.iter().map(|(host, span)| {
            (NodeId::new(base + host.as_u32()), *span)
        }));
        self.parent
            .extend(local.parent.iter().map(|id| id.map(|id| NodeId::new(base + id.as_u32()))));
        base..end
    }
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty table sized for `nodes` entries.
    ///
    /// Four parallel vectors means four capacity checks and four growth
    /// reallocations per doubling; reserving once removes all of them. The caller
    /// estimates from source length — being wrong costs a little memory, while
    /// being right removes ~4% of parse time on a large file.
    #[must_use]
    pub fn with_capacity(nodes: usize) -> Self {
        Self {
            parent: Vec::with_capacity(nodes),
            kind: Vec::with_capacity(nodes),
            span: Vec::with_capacity(nodes),
            flags: Vec::with_capacity(nodes),
            type_argument_lists: Vec::new(),
        }
    }

    /// Reserve room for `additional` more nodes.
    ///
    /// The append-to-a-shared-table counterpart of [`NodeTable::with_capacity`],
    /// and it exists for the same measured reason: four parallel vectors mean
    /// four growth reallocations per doubling. A table that several files parse
    /// into cannot be sized once at construction, so each file reserves its own
    /// estimate as it arrives.
    pub fn reserve(&mut self, additional: usize) {
        self.parent.reserve(additional);
        self.kind.reserve(additional);
        self.span.reserve(additional);
        self.flags.reserve(additional);
    }

    /// Number of nodes recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.kind.len()
    }

    /// Whether any nodes are recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kind.is_empty()
    }

    /// Record a node, returning its id.
    ///
    /// # Panics
    /// Panics if the file contains more than `u32::MAX - 1` nodes.
    pub fn push(&mut self, kind: SyntaxKind, span: Span, flags: NodeFlags) -> NodeId {
        let id = NodeId::new(u32::try_from(self.len()).expect("node count exceeds u32"));
        self.parent.push(None);
        self.kind.push(kind);
        self.span.push(span);
        self.flags.push(flags);
        id
    }

    /// The node's kind.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn kind(&self, id: NodeId) -> SyntaxKind {
        self.kind[id.as_u32() as usize]
    }

    /// The node's span.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn span(&self, id: NodeId) -> Span {
        self.span[id.as_u32() as usize]
    }

    /// Native type argument list extent; absence represents a nil list.
    #[must_use]
    pub fn type_argument_list_span(&self, host: NodeId) -> Option<Span> {
        self.type_argument_lists.binary_search_by_key(&host, |(id, _)| *id)
            .ok().map(|index| self.type_argument_lists[index].1)
    }

    /// Set completed native list metadata on its concrete syntax owner.
    pub fn set_type_argument_list_span(&mut self, host: NodeId, span: Option<Span>) {
        assert!((host.as_u32() as usize) < self.len());
        match (self.type_argument_lists.binary_search_by_key(&host, |(id, _)| *id), span) {
            (Ok(index), Some(span)) => self.type_argument_lists[index].1 = span,
            (Ok(index), None) => { self.type_argument_lists.remove(index); }
            (Err(index), Some(span)) => self.type_argument_lists.insert(index, (host, span)),
            (Err(_), None) => {}
        }
    }

    /// The node's flags.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn flags(&self, id: NodeId) -> NodeFlags {
        self.flags[id.as_u32() as usize]
    }

    /// The node's parent, or `None` for the source file root.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.parent[id.as_u32() as usize]
    }

    /// Set the node's parent. Called by the binder.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    pub fn set_parent(&mut self, id: NodeId, parent: NodeId) {
        self.parent[id.as_u32() as usize] = Some(parent);
    }

    /// Add flags to a node — the caller that motivated it stamps
    /// [`NodeFlags::JAVASCRIPT_FILE`] on a source-file root after parsing,
    /// which upstream's parser does from its `ScriptKind` and this parser
    /// cannot: it never sees the file name (ADR-0016). Additive only, like
    /// upstream's `node.Flags |= …` sites; there is deliberately no clearing
    /// counterpart.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    pub fn add_flags(&mut self, id: NodeId, flags: NodeFlags) {
        self.flags[id.as_u32() as usize] |= flags;
    }

    /// Discard rows from `len` onward.
    ///
    /// Used by speculative parsing: a rejected attempt registers nodes that never
    /// enter the tree. Sound only because ids are handed out sequentially and a
    /// discarded node's id is not yet referenced anywhere.
    pub fn truncate(&mut self, len: usize) {
        self.parent.truncate(len);
        self.kind.truncate(len);
        self.span.truncate(len);
        self.flags.truncate(len);
        let lists = self.type_argument_lists.partition_point(|(host, _)| (host.as_u32() as usize) < len);
        self.type_argument_lists.truncate(lists);
    }

    /// Walk from `id` up to the root.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(self.parent(id), move |&n| self.parent(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_discriminants_are_contiguous_from_zero() {
        assert_eq!(SyntaxKind::Unknown as u16, 0);
        assert_eq!(SyntaxKind::ALL.len(), SyntaxKind::COUNT as usize);
        for (i, kind) in SyntaxKind::ALL.iter().enumerate() {
            assert_eq!(*kind as u16 as usize, i, "kind {kind} has the wrong discriminant");
        }
    }

    #[test]
    fn from_u16_round_trips_and_rejects_out_of_range() {
        for kind in SyntaxKind::ALL {
            assert_eq!(SyntaxKind::from_u16(kind as u16), Some(kind));
        }
        assert_eq!(SyntaxKind::from_u16(SyntaxKind::COUNT), None);
        assert_eq!(SyntaxKind::from_u16(u16::MAX), None);
    }

    #[test]
    fn node_table_tracks_parents_and_ancestors() {
        let mut t = NodeTable::new();
        let root = t.push(SyntaxKind::SourceFile, Span::new(0, 10), NodeFlags::empty());
        let child = t.push(SyntaxKind::Block, Span::new(1, 9), NodeFlags::empty());
        let grandchild = t.push(SyntaxKind::EmptyStatement, Span::new(2, 3), NodeFlags::empty());
        t.set_parent(child, root);
        t.set_parent(grandchild, child);

        assert_eq!(t.kind(child), SyntaxKind::Block);
        assert_eq!(t.parent(root), None);
        assert_eq!(t.ancestors(grandchild).collect::<Vec<_>>(), vec![child, root]);
    }
}

/// `ModuleInstanceState` (`ast/utilities.go:2313`), minus the `Unknown` state,
/// which exists only for `getModuleInstanceStateCached`'s cycle guard (spelled
/// `None` in the `visited` map here).
///
/// The declaration order is upstream's numeric order and the derived `Ord`
/// relies on it: `getModuleInstanceStateForAliasTarget` keeps the *greater*
/// state with `>`, and upstream numbers `ConstEnumOnly` above `Instantiated`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModuleInstanceState {
    /// The declaration emits nothing — it contains only types.
    NonInstantiated,
    /// It emits JavaScript.
    Instantiated,
    /// It emits only `const enum`s, so whether it is code depends on
    /// `preserveConstEnums`.
    ConstEnumOnly,
}

/// `GetModuleInstanceState` (`ast/utilities.go:2322`) for the module
/// declaration `node`.
///
/// Answers whether a module declaration contributes anything to the emitted
/// JavaScript. Three subsystems need it and none can own it: the **checker**
/// asks whether an unreachable namespace is unreachable *code*
/// (`isSourceElementUnreachable`, `checker.go:2461`), the **binder** asks
/// whether a namespace declares a value, which picks `ValueModule` over
/// `NamespaceModule` (`declareModuleSymbol`, `binder.go:813`), and the `.types`
/// writer asks it through `GetMeaningFromDeclaration`. It lives here, beside the
/// other `ast/utilities.go` ports, for the same reason it lives there upstream.
///
/// `parents` is `node`'s ancestor chain, outermost first and ending with its
/// parent. Upstream reads `node.Parent` once the explicit `ancestors` stack is
/// exhausted (`popAncestor`); the typed tree has no parent links, so callers
/// supply that chain and the walk pushes onto a copy of it.
/// `getModuleInstanceStateForAliasTarget` climbs it to find the statement an
/// `export { x }` names.
///
/// The `visited` map is `getModuleInstanceStateCached`'s: keyed by node id,
/// owned by this one call, `None` while a node is in progress (a cycle answers
/// `NonInstantiated`, as upstream's `Unknown` does) and the final state after.
/// See `docs/architecture/checker-notes-diag2.md` §89 and §95.
#[must_use]
pub fn module_instance_state<'a>(node: Node<'a>, parents: &[Node<'a>]) -> ModuleInstanceState {
    let mut walk = InstanceStateWalk { visited: std::collections::HashMap::new() };
    let mut ancestors = parents.to_vec();
    walk.module(node, &mut ancestors)
}

/// One `GetModuleInstanceState` call's `visited` map.
struct InstanceStateWalk {
    visited: std::collections::HashMap<NodeId, Option<ModuleInstanceState>>,
}

impl InstanceStateWalk {
    /// `getModuleInstanceState` (`ast/utilities.go:2326`).
    fn module<'a>(&mut self, node: Node<'a>, ancestors: &mut Vec<Node<'a>>) -> ModuleInstanceState {
        let Node::ModuleDeclaration(module) = node else {
            return ModuleInstanceState::Instantiated;
        };
        // `declare module "x";` with no body is instantiated upstream.
        let Some(body) = module.body else { return ModuleInstanceState::Instantiated };
        ancestors.push(node);
        let state = self.cached(Node::from(body), ancestors);
        ancestors.pop();
        state
    }

    /// `getModuleInstanceStateCached` (`ast/utilities.go:2335`).
    fn cached<'a>(&mut self, node: Node<'a>, ancestors: &mut Vec<Node<'a>>) -> ModuleInstanceState {
        let Some(id) = node.node_id() else { return self.worker(node, ancestors) };
        if let Some(cached) = self.visited.get(&id) {
            return cached.unwrap_or(ModuleInstanceState::NonInstantiated);
        }
        self.visited.insert(id, None);
        let result = self.worker(node, ancestors);
        self.visited.insert(id, Some(result));
        result
    }

    /// `getModuleInstanceStateWorker` (`ast/utilities.go:2352`).
    fn worker<'a>(&mut self, node: Node<'a>, ancestors: &mut Vec<Node<'a>>) -> ModuleInstanceState {
        match node {
            Node::InterfaceDeclaration(_) | Node::TypeAliasDeclaration(_) => {
                ModuleInstanceState::NonInstantiated
            }
            Node::EnumDeclaration(declaration)
                if has_syntactic_modifier(declaration.modifiers, SyntaxKind::ConstKeyword) =>
            {
                ModuleInstanceState::ConstEnumOnly
            }
            // A non-exported import declares nothing in the emitted module.
            Node::ImportDeclaration(declaration)
                if !has_syntactic_modifier(declaration.modifiers, SyntaxKind::ExportKeyword) =>
            {
                ModuleInstanceState::NonInstantiated
            }
            Node::ImportEqualsDeclaration(declaration)
                if !has_syntactic_modifier(declaration.modifiers, SyntaxKind::ExportKeyword) =>
            {
                ModuleInstanceState::NonInstantiated
            }
            Node::ExportDeclaration(declaration) if declaration.module_specifier.is_none() => {
                let Some(NamedExportBindings::NamedExports(clause)) = declaration.export_clause
                else {
                    return ModuleInstanceState::Instantiated;
                };
                let depth = ancestors.len();
                ancestors.push(node);
                ancestors.push(Node::from(declaration.export_clause.expect("matched above")));
                let mut state = ModuleInstanceState::NonInstantiated;
                for specifier in clause.elements {
                    let specifier_state = self.alias_target(specifier, ancestors);
                    if specifier_state > state {
                        state = specifier_state;
                    }
                    if state == ModuleInstanceState::Instantiated {
                        break;
                    }
                }
                ancestors.truncate(depth);
                state
            }
            Node::ModuleBlock(_) => {
                let mut children = Vec::new();
                push_children(node, &mut children);
                ancestors.push(node);
                let mut state = ModuleInstanceState::NonInstantiated;
                for child in children {
                    match self.cached(child, ancestors) {
                        ModuleInstanceState::Instantiated => {
                            state = ModuleInstanceState::Instantiated;
                            break;
                        }
                        ModuleInstanceState::ConstEnumOnly => {
                            state = ModuleInstanceState::ConstEnumOnly;
                        }
                        ModuleInstanceState::NonInstantiated => {}
                    }
                }
                ancestors.pop();
                state
            }
            Node::ModuleDeclaration(_) => self.module(node, ancestors),
            _ => ModuleInstanceState::Instantiated,
        }
    }

    /// `getModuleInstanceStateForAliasTarget` (`ast/utilities.go:2406`).
    ///
    /// `ancestors` ends with the specifier's parent (the `NamedExports`); the
    /// climb walks it from the top without mutating it, and each statement list
    /// it searches is evaluated with the chain cut back to that list's owner.
    fn alias_target<'a>(
        &mut self,
        specifier: &'a ExportSpecifier<'a>,
        ancestors: &[Node<'a>],
    ) -> ModuleInstanceState {
        let Some(ModuleExportName::Identifier(name)) = specifier.property_name.or(specifier.name)
        else {
            // Skip for invalid syntax like this: export { "x" }
            return ModuleInstanceState::Instantiated;
        };
        for index in (0..ancestors.len()).rev() {
            let statements = match ancestors[index] {
                Node::Block(block) => block.statements,
                Node::ModuleBlock(block) => block.statements,
                Node::SourceFile(file) => file.statements,
                _ => continue,
            };
            let mut statements_ancestors = ancestors[..=index].to_vec();
            let mut found = None;
            for statement in statements {
                let statement = Node::from(*statement);
                if !node_has_name(statement, name.text) {
                    continue;
                }
                let state = self.cached(statement, &mut statements_ancestors);
                if found.is_none_or(|found| state > found) {
                    found = Some(state);
                }
                if found == Some(ModuleInstanceState::Instantiated) {
                    return ModuleInstanceState::Instantiated;
                }
                if matches!(statement, Node::ImportEqualsDeclaration(_)) {
                    // Treat re-exports of import aliases as instantiated since
                    // they're ambiguous, consistent with `export import x = mod.x`.
                    found = Some(ModuleInstanceState::Instantiated);
                }
            }
            if let Some(found) = found {
                return found;
            }
        }
        // Couldn't locate, assume could refer to a value.
        ModuleInstanceState::Instantiated
    }
}

/// `NodeHasName` (`ast/utilities.go:2449`).
fn node_has_name(statement: Node<'_>, text: &str) -> bool {
    let identifier = match statement {
        Node::FunctionDeclaration(n) => n.name,
        Node::ClassDeclaration(n) => n.name,
        Node::InterfaceDeclaration(n) => n.name,
        Node::TypeAliasDeclaration(n) => n.name,
        Node::EnumDeclaration(n) => n.name,
        Node::ImportEqualsDeclaration(n) => n.name,
        Node::ModuleDeclaration(n) => match n.name {
            Some(ModuleName::Identifier(identifier)) => Some(identifier),
            _ => None,
        },
        Node::VariableStatement(n) => {
            return n.declaration_list.is_some_and(|list| {
                list.declarations.iter().any(|declaration| {
                    matches!(declaration.name, Some(BindingName::Identifier(identifier)) if identifier.text == text)
                })
            });
        }
        _ => None,
    };
    identifier.is_some_and(|identifier| identifier.text == text)
}

/// `HasSyntacticModifier` for a modifier list — the shared spelling of the test
/// both crates were writing inline.
#[must_use]
pub fn has_syntactic_modifier(modifiers: &[ModifierLike<'_>], keyword: SyntaxKind) -> bool {
    modifiers.iter().any(|modifier| match modifier {
        ModifierLike::Token(token) => token.kind == keyword,
        ModifierLike::Decorator(_) => false,
    })
}
