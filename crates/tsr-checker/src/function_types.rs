//! Function and constructor type nodes: `(x: number) => string` and
//! `new (x: number) => C` in annotation position.
//!
//! Ported from `Checker.getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode`
//! (`checker.go`), for the **function-type and constructor-type halves** — the
//! halves [`crate::declared`] left to `errorType` when it ported the
//! type-literal one. The constructor half arrived a cycle later, with
//! [`crate::signatures::SignatureKind`]; `docs/architecture/checker-notes-ctortype.md`
//! records why it could not arrive alone and what it was worth.
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
    /// # `ConstructorTypeNode` is its sibling, not this arm
    ///
    /// See [`Checker::get_type_from_constructor_type_node`]. The two are
    /// deliberately separate functions over one shared tail even though every
    /// line of the tail is identical: the *type node kinds* are distinct in the
    /// AST, `getTypeFromTypeNode`'s dispatch is by kind, and a single arm taking
    /// an enum of the two would put a match inside a function whose whole body
    /// is already dispatched on that match.
    pub(crate) fn get_type_from_function_type_node(
        &mut self,
        node: &'a FunctionTypeNode<'a>,
    ) -> TypeId {
        let Some(id) = node.node_id else { return self.intrinsics.error };
        self.signature_bearing_type_node(id)
    }

    /// The type a **constructor** type node denotes: `new (x: number) => C`.
    ///
    /// Ported from `Checker.getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode`
    /// (`checker.go`), constructor-type half — upstream's own function covers
    /// all three kinds and this port splits it by kind, as
    /// `getTypeFromTypeNode`'s dispatch does.
    ///
    /// # Nothing here distinguishes it from a function type, and that is the
    /// finding
    ///
    /// The `new ` is not written by this function. It is
    /// [`crate::signatures::SignatureKind`], set by `signature_kind_of` off the
    /// declaration exactly as `getSignatureFromDeclaration` sets
    /// `SignatureFlagsConstruct` (`checker.go:19902`), and read by
    /// `signature_to_string` exactly as the node builder reads it
    /// (`nodebuilderimpl.go:2712`). Putting the prefix here instead would have
    /// worked for this one caller and left every *other* renderer of a signature
    /// — the object-member form, `symbols.rs`, `inference.rs` — printing a
    /// construct signature as a call one.
    ///
    /// The binder symbol is the same construction too:
    /// `bindFunctionOrConstructorType` (`binder.go:985`) is named for both kinds
    /// and `crate::binder`'s port has always given a constructor type node its
    /// `__type` symbol (`binder.rs:3495`). That was checked rather than assumed
    /// — the function-type arm's own history is a case of `IS_CONTAINER` being
    /// mistaken for a symbol.
    pub(crate) fn get_type_from_constructor_type_node(
        &mut self,
        node: &'a tsr_ast::ConstructorTypeNode<'a>,
    ) -> TypeId {
        let Some(id) = node.node_id else { return self.intrinsics.error };
        self.signature_bearing_type_node(id)
    }

    /// The shared tail of the two arms above.
    fn signature_bearing_type_node(&mut self, id: tsr_ast::NodeId) -> TypeId {
        let error = self.intrinsics.error;
        let Some(signature) = self.get_signature_from_declaration(id) else {
            return error;
        };
        // §72 (`checker-notes-narrow.md`): `getAliasForTypeNode`'s three arms,
        // the SAME three the type-literal and union nodes take — a body under
        // a non-generic alias prints the alias's name (`type F2 = ({ a:
        // string }: O) => any` records `>F2 : F2`), a generic one gaps rather
        // than dropping its arguments, an unaliased node renders structurally.

        let mut alias_named = false;
        let text = match self.alias_symbol_for_type_node(id) {
            None => self.signature_to_string(&signature),
            Some(alias) if self.local_type_parameters_of(alias).is_empty() => {
                alias_named = true;
                self.binder.symbols().get(alias).name.to_string()
            }
            // §947.2: the alias currently being re-resolved renders its body
            // STRUCTURALLY here, which is §92's exemption for the union road
            // applied to this one. Keyed on `variadic_alias_in_progress` — the
            // alias §947.2's caller inserted before re-resolving — so an
            // unrelated function type reached during some other evaluation still
            // declines.
            Some(alias) if self.variadic_alias_in_progress.contains(&alias) => {
                self.signature_to_string(&signature)
            }
            Some(_) => return error,
        };
        // The symbol is `bindFunctionOrConstructorType`'s `__type` symbol, whose
        // members table holds the `__call` signature symbol. A node that somehow
        // has none is a gap rather than a type with a synthetic identity — see
        // the note above.
        let Some(symbol) = self.binder.symbol_of(id) else { return error };
        // §447: the `signature` flag records which NODE KIND the node builder
        // would emit (`TypeData::Anonymous::signature`'s own contract), and an
        // alias-NAMED bake emits a `TypeReferenceNode` — highest precedence,
        // never parenthesised — not a bare `FunctionTypeNode`. Passing `true`
        // for it printed `(F1) | (F2)` where `unionTypeCallSignatures4`
        // records `F1 | F2`. The flag's other consumer (`crate::flow`'s
        // typeof facts) is unaffected: it falls through to the
        // `signature_types` table this same function populates below.
        let built = self.store.new_anonymous(TypeFlags::OBJECT, text, symbol, !alias_named);
        // §89 (`checker-notes-narrow.md`): the site renderer's composite
        // re-render (§10.13) rebuilds a single-signature type from its
        // STRUCTURE — which is right for qualifier/rename sites and WRONG
        // for an alias-NAMED bake (`type H = (a: number) => void` printed
        // structurally at every site while the mint carried "H"; the open
        // trace this closes). The set tells it to keep the name.
        if alias_named {
            self.alias_named_signature_types.insert(built);
        }
        // The structure the text was rendered from, kept reachable from the id
        // so `instantiate_type` can rebuild this type with substituted parts —
        // see `Checker::signature_types` (`bd tsr-0hc`). Recorded here because
        // this is the last point the `Signature` exists.
        self.signature_types.insert(built, vec![signature]);
        built
    }
}
