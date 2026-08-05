//! The checker's state: what it remembers, and the handles it hands out.
//!
//! Ported from `internal/checker/checker.go`. This module holds the [`Checker`]
//! struct itself and the few helpers that belong to no single concern; the
//! functions that compute types live in one module per concern and add `impl`
//! blocks to this same type:
//!
//! | module | upstream | answers |
//! |---|---|---|
//! | [`crate::expressions`] | `checkExpression` | the type of an expression |
//! | [`crate::binary`] | `checkBinaryLikeExpression` | `a + b`, `a === b`, `a = b` |
//! | [`crate::members`] | `checkPropertyAccessExpression`, `getPropertyOfType` | `a.b` |
//! | [`crate::symbols`] | `getTypeOfSymbol` | the type a *value* symbol has |
//! | [`crate::declared`] | `getTypeFromTypeNode`, `getDeclaredTypeOfSymbol` | what a type node and a *type* symbol denote |
//! | [`crate::literals`] | `getWidenedLiteralType` and its pair | fresh versus regular |
//!
//! The split is by upstream concern rather than by size, so that a reader who
//! knows `checker.go` can find the arm they want; upstream keeps all of this in
//! one 60,269-line file and that is not a shape worth reproducing. The fields
//! below are `pub(crate)` because every one of those modules writes to a memo.

use rustc_hash::FxHashMap;
use tsr_ast::{NodeFlags, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::{BindResult, SymbolId};

use crate::{
    intrinsics::Intrinsics,
    printing,
    resolution::Resolutions,
    types::{TypeId, TypeStore},
};

/// Computes types.
///
/// **Every method takes `&mut self` and returns [`TypeId`].** No method hands out
/// a reference into checker state, which is what makes lazy memoisation ordinary
/// safe Rust here — see
/// [ADR-0013](../../../docs/adr/0013-checker-memoisation.md). Read a type's
/// contents with [`Checker::type_of`].
pub struct Checker<'a, 'n> {
    pub(crate) store: TypeStore,
    pub(crate) intrinsics: Intrinsics,
    pub(crate) nodes: &'n NodeTable,
    /// The way back from a `NodeId` to the typed node
    /// ([ADR-0033](../../../docs/adr/0033-the-parser-fills-the-node-map.md)).
    /// A symbol names its declaration by id; the annotation and initialiser live
    /// in the node.
    pub(crate) node_map: &'n NodeMap<'a>,
    pub(crate) binder: &'a BindResult<'a>,
    /// `expression node -> its type`, the memo upstream keeps in `nodeLinks`.
    pub(crate) node_types: FxHashMap<NodeId, TypeId>,
    /// How many times the *worker* has run, as opposed to the memo answering.
    ///
    /// Instrumentation, not state: it exists because the memo is otherwise
    /// unobservable. Interning already makes a repeated literal produce no new
    /// type, so a test that counts types cannot tell a working memo from a
    /// missing one — a no-op test, and this counter is what makes the memo test
    /// actually bite.
    pub(crate) computations: usize,
    /// Fresh literal type -> its widened (regular) form.
    ///
    /// Upstream keeps this as `regularType` on the literal type itself
    /// (`types.go`, `LiteralType.regularType`). A side table here, for the same
    /// reason the binder uses one: the stored type is immutable once created.
    pub(crate) regular_types: FxHashMap<TypeId, TypeId>,
    /// `symbol -> its type`, upstream's `valueSymbolLinks[symbol].resolvedType`.
    pub(crate) symbol_types: FxHashMap<SymbolId, TypeId>,
    /// `(generic symbol, type arguments) -> the instantiated reference`,
    /// upstream's `d.instantiations` keyed by `getTypeListKey`
    /// (`checker.go:17342`).
    ///
    /// Identity matters even though nothing yet looks inside one of these types:
    /// `C<number>` written twice must be one type, or the first relation check
    /// written will compare two handles that should have been equal.
    pub(crate) instantiations: FxHashMap<(SymbolId, Vec<TypeId>), TypeId>,
    /// A class symbol to its `this` type, upstream's `d.thisType`
    /// (`checker.go:17334`). One per class, so `this` has a stable identity
    /// inside one.
    pub(crate) this_types: FxHashMap<SymbolId, TypeId>,
    /// `symbol -> the type it *declares*`, upstream's
    /// `declaredTypeLinks[symbol].declaredType`.
    ///
    /// Separate from [`Checker::symbol_types`] because they are different
    /// questions about the same symbol: a class `C` declares the instance type
    /// `C` and *has* the type `typeof C`. Merging them would answer one with the
    /// other.
    pub(crate) declared_types: FxHashMap<SymbolId, TypeId>,
    /// In-progress resolutions, for circularity detection.
    pub(crate) resolutions: Resolutions<SymbolId>,
}

impl<'a, 'n> Checker<'a, 'n> {
    /// Create a checker over one bound file.
    #[must_use]
    pub fn new(
        binder: &'a BindResult<'a>,
        nodes: &'n NodeTable,
        node_map: &'n NodeMap<'a>,
    ) -> Self {
        let mut store = TypeStore::new();
        let intrinsics = Intrinsics::create(&mut store);
        Self {
            store,
            intrinsics,
            nodes,
            node_map,
            binder,
            computations: 0,
            node_types: FxHashMap::default(),
            regular_types: FxHashMap::default(),
            symbol_types: FxHashMap::default(),
            declared_types: FxHashMap::default(),
            this_types: FxHashMap::default(),
            instantiations: FxHashMap::default(),
            resolutions: Resolutions::new(),
        }
    }

    /// The well-known types.
    #[must_use]
    pub fn intrinsics(&self) -> &Intrinsics {
        &self.intrinsics
    }

    /// Read a type's contents.
    ///
    /// The only way in: [`TypeId`] is a handle and the store owns the type.
    #[must_use]
    pub fn type_of(&self, id: TypeId) -> &crate::types::Type {
        self.store.get(id)
    }

    /// Render a type as a `.types` baseline would print it.
    #[must_use]
    pub fn type_to_string(&self, id: TypeId) -> String {
        printing::type_to_string(self.store.get(id))
    }

    /// Whether a type is `errorType` itself, by identity.
    ///
    /// Not a flag test: `errorType` and `anyType` share `TypeFlagsAny` and are
    /// distinguished only by identity, which is the whole point of them being
    /// separate types (`checker.go:979`).
    pub(crate) fn is_error(&self, id: TypeId) -> bool {
        id == self.intrinsics.error
    }

    /// Ported from `ast.GetCombinedNodeFlags` / `getCombinedFlags`
    /// (`internal/ast/utilities.go:1180`).
    ///
    /// `const` is not a flag on the declaration: it is on the enclosing
    /// `VariableDeclarationList`, so answering "is this a const?" means walking
    /// up. This is the first place the port needs `NodeTable::parent`, and the
    /// reason [ADR-0033](../../../docs/adr/0033-the-parser-fills-the-node-map.md)
    /// insisted the lookup answer parent ids rather than only declarations.
    pub(crate) fn combined_node_flags(&self, declaration: NodeId) -> NodeFlags {
        let mut flags = self.nodes.flags(declaration);
        let mut node = declaration;
        if self.nodes.kind(node) == SyntaxKind::VariableDeclaration {
            let Some(parent) = self.nodes.parent(node) else { return flags };
            node = parent;
        }
        if self.nodes.kind(node) == SyntaxKind::VariableDeclarationList {
            flags |= self.nodes.flags(node);
            let Some(parent) = self.nodes.parent(node) else { return flags };
            node = parent;
        }
        if self.nodes.kind(node) == SyntaxKind::VariableStatement {
            flags |= self.nodes.flags(node);
        }
        flags
    }

    /// How many times an expression type was actually computed rather than
    /// served from the memo. See [`Checker::computations`]'s field docs.
    #[must_use]
    pub fn computations(&self) -> usize {
        self.computations
    }

    /// How many types exist. For tests that assert interning actually interns.
    #[must_use]
    pub fn type_count(&self) -> usize {
        self.store.len()
    }

    /// The node table this checker reads.
    #[must_use]
    pub fn nodes(&self) -> &NodeTable {
        self.nodes
    }
}
