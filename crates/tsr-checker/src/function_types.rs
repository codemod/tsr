//! Function type nodes: `(x: number) => string` in annotation position.
//!
//! Ported from `Checker.getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode`
//! (`checker.go`), for the **function-type half** — the half
//! [`crate::declared`] left to `errorType` when it ported the type-literal half.
//!
//! # A function type is an object type carrying one call signature
//!
//! Not a distinct kind of type. Upstream's binder is explicit about this:
//! `bindFunctionOrConstructorType` (`binder.go:985`) creates a
//! `SymbolFlagsSignature` symbol for the node and then an anonymous
//! `__type` symbol whose sole member *is* that signature, so that
//! `(x: number) => string` and `{ (x: number): string }` are indistinguishable
//! to everything downstream. The type built here is therefore the same
//! [`crate::types::TypeData::Anonymous`] a function symbol's type already is.
//!
//! # That symbol is what makes calls resolve, and it was not free
//!
//! When this module was first written the binder did not create one: it
//! mentioned `FunctionTypeNode` only in `container.rs`, where it grants
//! `IS_CONTAINER` and locals — which is **not** the same thing as a symbol, and
//! the difference cost a measurement to establish rather than a reading of the
//! flags. `symbol_of` on the `FunctionType` node of
//! `declare const f: (x: number) => string;` answered `None`.
//!
//! Porting `bindFunctionOrConstructorType` (`bd tsr-y4u`) closed that, and doing
//! so made `resolve_call_signature` (`crate::calls`) start resolving calls
//! through a function-typed callee **with no code written in `calls.rs`** —
//! `f(1)` types as `string`. That is not a happy accident: it is what upstream's
//! binder comment says the two-symbol construction is *for*. `calls.rs`'s own
//! module doc names the previous behaviour as a known gap; it is now closed, and
//! `tests/function_types.rs` pins it with a test that was written asserting
//! `error` and watched to flip.
//!
//! # The printed form comes from the signature, not from the source text
//!
//! `<T>(x?: A, ...r: B[]) => C` is rendered by
//! [`Checker::signature_to_string`], which is
//! `signatureToSignatureDeclarationHelper` with `kind == ast.KindFunctionType`
//! (`nodebuilderimpl.go:1792`). Reproducing the annotation's source slice
//! instead would agree with the baseline for the easy cases and diverge on every
//! one where upstream's node builder normalises — spacing, an inferred
//! optionality marker, a parameter whose annotation is itself an alias.
//!
//! # Every gap is inherited, and none is invented here
//!
//! This module adds no gap of its own. A function type is `errorType` exactly
//! when [`Checker::get_signature_from_declaration`] refuses the declaration —
//! a destructuring parameter, a parameter or return annotation that is itself a
//! gap, a type parameter carrying a modifier — and that list, with the reasoning
//! for each entry, lives on that function. `(x: Unported) => void` is not
//! `(x: any) => void`.

use tsr_ast::FunctionTypeNode;

use crate::{checker::Checker, flags::TypeFlags, types::TypeId};

impl<'a> Checker<'a, '_> {
    /// The type a function type node denotes.
    ///
    /// Ported from `Checker.getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode`
    /// (`checker.go`), function-type half.
    ///
    /// # No symbol is invented when the binder has not made one
    ///
    /// A function type node with no binder symbol is a **gap**, not an anonymous
    /// type with a synthesised identity. That alternative was considered while
    /// `bindFunctionOrConstructorType` was still unported, and rejected: a symbol
    /// created by the checker is in no symbol table and owns no declarations, so
    /// `get_signatures_of_symbol` would read declarations never bound to it, and
    /// the callee of `((x: number) => string)(1)` would resolve against whatever
    /// the invented symbol collided with. It would also have put the fix in the
    /// layer that cannot see the problem.
    ///
    /// # `ConstructorTypeNode` is not handled here
    ///
    /// `new (x: number) => C` reaches the same upstream function and prints with
    /// a leading `new `, which [`Checker::signature_to_string`] does not emit —
    /// it renders the `KindFunctionType` form only. Answering a constructor type
    /// through this path would print it as though the `new` were absent, which
    /// is a wrong line rather than a missing one. `bd tsr-4sc.8`.
    pub(crate) fn get_type_from_function_type_node(
        &mut self,
        node: &'a FunctionTypeNode<'a>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let Some(id) = node.node_id else { return error };
        let Some(signature) = self.get_signature_from_declaration(id) else {
            return error;
        };
        let text = self.signature_to_string(&signature);
        // The symbol is `bindFunctionOrConstructorType`'s `__type` symbol, whose
        // members table holds the `__call` signature symbol. A function type node
        // that somehow has none is a gap rather than a type with a synthetic
        // identity — see the note above.
        let Some(symbol) = self.binder.symbol_of(id) else { return error };
        self.store.new_anonymous(TypeFlags::OBJECT, text, symbol)
    }
}
