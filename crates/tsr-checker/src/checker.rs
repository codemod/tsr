//! The checker.
//!
//! Ported from `internal/checker/checker.go`. This is the first vertical slice:
//! literal and keyword expressions, and the freshness rule that decides whether
//! `const x = "a"` is `"a"` or `string`. See the crate docs for exactly what
//! exists.

use rustc_hash::FxHashMap;
use tsr_ast::{Expression, Node, NodeId, NodeTable, SyntaxKind};
use tsr_binder::BindResult;

use crate::{
    flags::TypeFlags,
    intrinsics::Intrinsics,
    printing,
    types::{TypeData, TypeId, TypeStore},
};

/// Computes types.
///
/// **Every method takes `&mut self` and returns [`TypeId`].** No method hands out
/// a reference into checker state, which is what makes lazy memoisation ordinary
/// safe Rust here — see
/// [ADR-0013](../../../docs/adr/0013-checker-memoisation.md). Read a type's
/// contents with [`Checker::type_of`].
pub struct Checker<'a, 'n> {
    store: TypeStore,
    intrinsics: Intrinsics,
    nodes: &'n NodeTable,
    #[allow(
        dead_code,
        reason = "the first slice types expressions that need no symbol; \
                                 declaration types are the next one (bd tsr-4sc.2)"
    )]
    binder: &'a BindResult<'a>,
    /// `expression node -> its type`, the memo upstream keeps in `nodeLinks`.
    node_types: FxHashMap<NodeId, TypeId>,
    /// How many times the *worker* has run, as opposed to the memo answering.
    ///
    /// Instrumentation, not state: it exists because the memo is otherwise
    /// unobservable. Interning already makes a repeated literal produce no new
    /// type, so a test that counts types cannot tell a working memo from a
    /// missing one — a no-op test, and this counter is what makes the memo test
    /// actually bite.
    computations: usize,
    /// Fresh literal type -> its widened (regular) form.
    ///
    /// Upstream keeps this as `regularType` on the literal type itself
    /// (`types.go`, `LiteralType.regularType`). A side table here, for the same
    /// reason the binder uses one: the stored type is immutable once created.
    regular_types: FxHashMap<TypeId, TypeId>,
}

impl<'a, 'n> Checker<'a, 'n> {
    /// Create a checker over one bound file.
    #[must_use]
    pub fn new(binder: &'a BindResult<'a>, nodes: &'n NodeTable) -> Self {
        let mut store = TypeStore::new();
        let intrinsics = Intrinsics::create(&mut store);
        Self {
            store,
            intrinsics,
            nodes,
            binder,
            computations: 0,
            node_types: FxHashMap::default(),
            regular_types: FxHashMap::default(),
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

    /// The type of an expression.
    ///
    /// Ported from `Checker.checkExpression` (`internal/checker/checker.go`),
    /// restricted to the forms this slice covers. Anything else yields
    /// `errorType` — **not** `anyType`: `errorType` is upstream's marker for "could
    /// not be computed", and using `any` would claim a real answer.
    pub fn check_expression(&mut self, expression: Expression<'_>) -> TypeId {
        let node = Node::from(expression);
        if let Some(id) = node.node_id()
            && let Some(&cached) = self.node_types.get(&id)
        {
            return cached;
        }

        self.computations += 1;
        let computed = self.check_expression_worker(expression);

        if let Some(id) = node.node_id() {
            self.node_types.insert(id, computed);
        }
        computed
    }

    fn check_expression_worker(&mut self, expression: Expression<'_>) -> TypeId {
        match expression {
            Expression::StringLiteral(node) => self
                .store
                .intern(TypeFlags::STRING_LITERAL, TypeData::StringLiteral(node.text.to_string())),
            Expression::NoSubstitutionTemplateLiteral(node) => self
                .store
                .intern(TypeFlags::STRING_LITERAL, TypeData::StringLiteral(node.text.to_string())),
            Expression::NumericLiteral(node) => self.store.intern(
                TypeFlags::NUMBER_LITERAL,
                TypeData::NumberLiteral(printing::normalise_number(node.text)),
            ),
            Expression::BigIntLiteral(node) => self.store.intern(
                TypeFlags::BIG_INT_LITERAL,
                // A bigint literal's text carries its trailing `n`; the type's
                // payload is the digits, and `type_to_string` puts the `n` back.
                TypeData::BigIntLiteral(node.text.trim_end_matches('n').to_string()),
            ),
            Expression::KeywordExpression(node) => match node.kind {
                SyntaxKind::TrueKeyword => self.intrinsics.true_type,
                SyntaxKind::FalseKeyword => self.intrinsics.false_type,
                SyntaxKind::NullKeyword => self.intrinsics.null,
                // `undefined` is an identifier rather than a keyword in the
                // grammar, so it does not arrive here.
                _ => self.intrinsics.error,
            },
            Expression::ParenthesizedExpression(node) => {
                node.expression.map_or(self.intrinsics.error, |inner| self.check_expression(inner))
            }
            _ => self.intrinsics.error,
        }
    }

    /// The widened form of a literal type.
    ///
    /// Ported from `Checker.getRegularTypeOfLiteralType`
    /// (`internal/checker/checker.go`). This is the rule behind the most
    /// frequently surprising line in any `.types` baseline: the *expression*
    /// `"a"` always has type `"a"`, but the *variable* in `let x = "a"` has type
    /// `string`, because a `let` widens the literal while a `const` keeps it.
    ///
    /// Only the widening direction is ported. Upstream also tracks *freshness*
    /// separately, so that a literal written directly and one arriving through a
    /// `const` are distinguishable; that needs the `freshType`/`regularType` pair
    /// and is part of the next slice (`bd tsr-4sc.2`).
    pub fn widen_literal(&mut self, id: TypeId) -> TypeId {
        if let Some(&cached) = self.regular_types.get(&id) {
            return cached;
        }
        let flags = self.store.get(id).flags;
        let widened = if flags.contains(TypeFlags::STRING_LITERAL) {
            self.intrinsics.string
        } else if flags.contains(TypeFlags::NUMBER_LITERAL) {
            self.intrinsics.number
        } else if flags.contains(TypeFlags::BIG_INT_LITERAL) {
            self.intrinsics.bigint
        } else if flags.contains(TypeFlags::BOOLEAN_LITERAL) {
            self.intrinsics.boolean
        } else {
            id
        };
        self.regular_types.insert(id, widened);
        widened
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
