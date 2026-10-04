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
        // Reuse successful nodes in the same captured-binding/template context,
        // but only after the current signature and alias eligibility checks.
        let key = self.type_literal_key(id);
        if let Some(&ty) = self.type_literal_types.get(&key) {
            return ty;
        }
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
        self.type_literal_types.insert(key, built);
        built
    }
}

#[cfg(test)]
mod node_identity_tests {
    use tsr_ast::{Node, SourceFile, Statement, TypeNode};

    use super::*;
    use crate::{signatures::SignatureKind, types::TypeData};

    fn with_checker(
        source: &str,
        test: impl for<'a, 'b> FnOnce(&mut Checker<'a, 'b>, &[TypeNode<'a>], &'a SourceFile<'a>),
    ) {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "fixture.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let mut pending = vec![Node::SourceFile(parsed.source_file)];
        let mut types = Vec::new();
        while let Some(node) = pending.pop() {
            tsr_ast::push_children(node, &mut pending);
            match node {
                Node::FunctionTypeNode(node) => types.push(TypeNode::FunctionTypeNode(node)),
                Node::ConstructorTypeNode(node) => types.push(TypeNode::ConstructorTypeNode(node)),
                _ => {}
            }
        }
        test(&mut checker, &types, parsed.source_file);
    }

    #[test]
    fn raw_nodes_reuse_success_without_merging_foreign_parameters_or_kinds() {
        with_checker(
            "declare const first: <T>(value: T) => T;
             declare const second: <T>(value: T) => T;
             declare const make: new<T>(value: T) => { value: T };",
            |checker, nodes, _| {
                let mut identities = rustc_hash::FxHashSet::default();
                let mut parameters = rustc_hash::FxHashSet::default();
                for &node in nodes {
                    let first = checker.get_type_from_type_node(node);
                    assert_ne!(first, checker.intrinsics.error);
                    let repeated = checker.get_type_from_type_node(node);
                    println!(
                        "RAW_NODE {:?} ids={first:?}/{repeated:?}",
                        Node::from(node).node_id()
                    );
                    assert_eq!(first, repeated, "same node must retain identity");
                    assert!(identities.insert(first), "different nodes must stay distinct");
                    let signature = &checker.signature_types[&first][0];
                    assert!(parameters.insert(signature.parameters[0].r#type));
                    assert_eq!(checker.type_to_string(signature.parameters[0].r#type), "T");
                    assert_eq!(
                        signature.kind,
                        if matches!(node, TypeNode::FunctionTypeNode(_)) {
                            SignatureKind::Call
                        } else {
                            SignatureKind::Construct
                        }
                    );
                }
                assert_eq!(identities.len(), 3);
                assert_eq!(parameters.len(), 3);
            },
        );
    }

    #[test]
    fn binding_and_mapped_template_keys_are_separate_in_both_lookup_orders() {
        for number_first in [false, true] {
            with_checker(
                "type Capture<T> = { call: (value: T) => T; make: new(value: T) => { value: T } };",
                |checker, nodes, file| {
                    let Statement::TypeAliasDeclaration(alias) = file.statements[0] else {
                        panic!("Capture alias")
                    };
                    let symbol = checker
                        .binder
                        .symbol_of(alias.type_parameters[0].node_id.unwrap())
                        .unwrap();
                    let string = checker.intrinsics.string;
                    let number = checker.store.intern_literal(
                        TypeFlags::NUMBER_LITERAL,
                        TypeData::NumberLiteral("37".into()),
                        false,
                    );
                    let arguments = if number_first { [number, string] } else { [string, number] };
                    let mut identities = rustc_hash::FxHashSet::default();
                    let mut observations = Vec::new();
                    for mapped_depth in [0, 1] {
                        checker.mapped_template_depth = mapped_depth;
                        for argument in arguments {
                            checker
                                .alias_evaluation_bindings
                                .push([(symbol, argument)].into_iter().collect());
                            for &node in nodes {
                                let first = checker.get_type_from_type_node(node);
                                let repeated = checker.get_type_from_type_node(node);
                                println!(
                                    "CONTEXT number_first={number_first} mapped={mapped_depth} argument={argument:?} node={:?} ids={first:?}/{repeated:?}",
                                    Node::from(node).node_id()
                                );
                                assert_eq!(first, repeated, "warm context must reuse its node");
                                assert!(
                                    identities.insert(first),
                                    "different contexts must not merge"
                                );
                                observations.push((mapped_depth, argument, node, first));
                                let signature = checker.signature_types[&first][0].clone();
                                assert_eq!(signature.parameters[0].r#type, argument);
                                if signature.kind == SignatureKind::Call {
                                    assert_eq!(signature.r#type, argument);
                                } else {
                                    assert_eq!(
                                        checker.get_type_of_property_of_type(
                                            signature.r#type,
                                            "value"
                                        ),
                                        Some(argument)
                                    );
                                }
                            }
                            checker.alias_evaluation_bindings.pop();
                        }
                    }
                    for (mapped_depth, argument, node, expected) in observations.into_iter().rev() {
                        checker.mapped_template_depth = mapped_depth;
                        checker
                            .alias_evaluation_bindings
                            .push([(symbol, argument)].into_iter().collect());
                        assert_eq!(
                            checker.get_type_from_type_node(node),
                            expected,
                            "older context must remain reusable"
                        );
                        checker.alias_evaluation_bindings.pop();
                    }
                    assert_eq!(identities.len(), 8);
                },
            );
        }
    }

    #[test]
    fn cached_generic_alias_success_never_bypasses_current_eligibility() {
        with_checker(
            "type FunctionAlias<T> = (value: T) => T;
             type ConstructorAlias<T> = new(value: T) => { value: T };",
            |checker, nodes, _| {
                for &node in nodes {
                    let id = Node::from(node).node_id().unwrap();
                    let alias = checker.alias_symbol_for_type_node(id).unwrap();
                    assert_eq!(checker.get_type_from_type_node(node), checker.intrinsics.error);
                    assert_eq!(checker.cached_type_literal(id), None);
                    checker.variadic_alias_in_progress.insert(alias);
                    let success = checker.get_type_from_type_node(node);
                    assert_ne!(success, checker.intrinsics.error);
                    assert_eq!(checker.get_type_from_type_node(node), success);
                    checker.variadic_alias_in_progress.remove(&alias);
                    assert_eq!(checker.get_type_from_type_node(node), checker.intrinsics.error);
                    checker.variadic_alias_in_progress.insert(alias);
                    assert_eq!(checker.get_type_from_type_node(node), success);
                    checker.variadic_alias_in_progress.remove(&alias);
                }
            },
        );
    }

    #[test]
    fn named_function_and_constructor_nodes_keep_alias_spelling_and_shape() {
        with_checker(
            "type NamedFunction = (value: 1) => \"out\";
             type NamedConstructor = new(value: \"head\") => { tail: 37 };",
            |checker, nodes, _| {
                for &node in nodes {
                    let first = checker.get_type_from_type_node(node);
                    let repeated = checker.get_type_from_type_node(node);
                    println!(
                        "NAMED_NODE {:?} ids={first:?}/{repeated:?}",
                        Node::from(node).node_id()
                    );
                    assert_eq!(first, repeated);
                    let function = matches!(node, TypeNode::FunctionTypeNode(_));
                    assert_eq!(
                        checker.type_to_string(first),
                        if function { "NamedFunction" } else { "NamedConstructor" }
                    );
                    assert!(checker.alias_named_signature_types.contains(&first));
                    assert!(matches!(
                        checker.store.get(first).data,
                        TypeData::Anonymous { signature: false, .. }
                    ));
                    let signature = checker.signature_types[&first][0].clone();
                    assert_eq!(
                        checker.type_to_string(signature.parameters[0].r#type),
                        if function { "1" } else { "\"head\"" }
                    );
                    if function {
                        assert_eq!(signature.kind, SignatureKind::Call);
                        assert_eq!(checker.type_to_string(signature.r#type), "\"out\"");
                    } else {
                        assert_eq!(signature.kind, SignatureKind::Construct);
                        let tail =
                            checker.get_type_of_property_of_type(signature.r#type, "tail").unwrap();
                        assert_eq!(checker.type_to_string(tail), "37");
                    }
                }
            },
        );
    }

    #[test]
    fn a_transient_failed_predicate_does_not_cache_success_or_poison_recovery() {
        with_checker(
            "type Guard<T> = { test: (value: unknown) => value is T };",
            |checker, nodes, file| {
                let Statement::TypeAliasDeclaration(alias) = file.statements[0] else {
                    panic!("Guard alias")
                };
                let symbol =
                    checker.binder.symbol_of(alias.type_parameters[0].node_id.unwrap()).unwrap();
                let node = nodes[0];
                let id = Node::from(node).node_id().unwrap();
                checker
                    .alias_evaluation_bindings
                    .push([(symbol, checker.intrinsics.error)].into_iter().collect());
                assert_eq!(checker.get_type_from_type_node(node), checker.intrinsics.error);
                assert_eq!(checker.cached_type_literal(id), None);
                checker.alias_evaluation_bindings.pop();
                checker
                    .alias_evaluation_bindings
                    .push([(symbol, checker.intrinsics.string)].into_iter().collect());
                let success = checker.get_type_from_type_node(node);
                assert_ne!(success, checker.intrinsics.error);
                assert_eq!(checker.get_type_from_type_node(node), success);
                assert_eq!(
                    checker.signature_types[&success][0].predicate.as_ref().unwrap().r#type,
                    Some(checker.intrinsics.string)
                );
                checker.alias_evaluation_bindings.pop();
            },
        );
    }
}
