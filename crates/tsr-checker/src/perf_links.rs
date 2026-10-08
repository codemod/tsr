//! Per-checker memo tables for answers native caches in symbol or type links
//! and this port recomputed per query. Each table mirrors one native cache;
//! its key, owner, publication states, context and expensive work boundary
//! are recorded in `docs/parity/notes/r4-perf.md` (the checker port
//! convention, `docs/conventions.md`).

use rustc_hash::{FxHashMap, FxHashSet};
use tsr_ast::{NodeId, SyntaxKind};
use tsr_binder::SymbolId;

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::signatures::SignatureKind;
use crate::types::{TypeData, TypeId};

/// Memo tables owned by one [`Checker`]. Nothing here is shared between
/// checkers: every key and value names ids of the owning checker's stores.
#[derive(Default)]
pub(crate) struct PerfLinks {
    /// A heritage entity name's resolved symbol, keyed by the name's
    /// expression node and the requested meaning: native `resolveEntityName`
    /// publishing `links.resolvedSymbol` on the heritage expression. Holds
    /// only `Some` answers (`r4-perf.md` §3).
    pub(crate) heritage_entity_symbols: FxHashMap<(NodeId, u32), SymbolId>,
    /// A heritage reference's instantiated base type, keyed by the base
    /// symbol, the reference location and the written argument slice (its
    /// first node and length): native `getTypeFromClassOrInterfaceReference`
    /// under `resolveBaseTypesOfInterface`/`resolveBaseTypesOfClass`, whose
    /// answer lands in `resolvedBaseTypes`. Holds only decided answers
    /// (`r4-perf.md` §3).
    pub(crate) heritage_bases: FxHashMap<HeritageBaseKey, TypeId>,
    /// `(receiver, declared member type, this argument) -> (receiver
    /// target's polymorphic this at publication, member type seen through
    /// the receiver)`: native `getTypeOfInstantiatedSymbol` publishing the
    /// instantiated member symbol's `links.resolvedType`. Holds only decided
    /// answers (`r4-perf2.md` §2).
    pub(crate) reference_member_types:
        FxHashMap<(TypeId, TypeId, TypeId), (Option<TypeId>, TypeId)>,
    /// A class's or interface's instance property names, own members in
    /// declaration order then each base's, de-duplicated: native
    /// `resolveObjectTypeMembers` publishing the declared type's
    /// `resolvedProperties`, read by name. Holds only decided answers
    /// (`r4-perf2.md` §3).
    pub(crate) structured_property_names: FxHashMap<SymbolId, std::rc::Rc<[String]>>,
    /// A class's, interface's or type literal's own-then-inherited index
    /// infos (`false`) or a class's static ones (`true`), values
    /// uninstantiated: native `resolveObjectTypeMembers` /
    /// `resolveAnonymousTypeMembers` publishing the declared type's
    /// `indexInfos`. Holds only decided answers (`r4-perf3.md` §5).
    pub(crate) symbol_index_infos:
        FxHashMap<(SymbolId, bool), Vec<crate::index_signatures::IndexInfo>>,
    /// The `(owner, is_static)` entries of `Checker::late_bound_member_names`
    /// whose `late_bound_members_of` computation is running: their parked
    /// empty list is a placeholder, not a completed answer. Native resolves
    /// late-bound members inside `getResolvedMembersOrExportsOfSymbol`, whose
    /// in-progress state is the `lateSymbol` links; a reader that publishes
    /// (`structured_property_names`) must not take the placeholder for the
    /// answer (`r4-perf2.md` §3, `r4-perf3.md` §3).
    pub(crate) late_bound_active: FxHashSet<(SymbolId, bool)>,
    /// Whether a receiver has call and construct signatures — the facts
    /// `getPropertyOfTypeEx`'s miss reads from the receiver's resolved
    /// members to choose the `Function`/`Object` fallback. Holds only decided
    /// answers for receivers with no `signature_types` entry
    /// (`r5-perf4.md` §2).
    pub(crate) receiver_signature_kinds: FxHashMap<TypeId, (bool, bool)>,
    /// Reused buffer for [`Checker::memo_frames`]' scope owners.
    owners_scratch: Vec<NodeId>,
}

/// [`PerfLinks::heritage_bases`]' key: base symbol, reference location, the
/// first written type argument's node and the written argument count. A
/// written argument list is one contiguous node list, so its first node and
/// length name the slice.
pub(crate) type HeritageBaseKey = (SymbolId, Option<NodeId>, Option<NodeId>, usize);

/// [`Checker::publication_mark`]'s answer.
#[derive(Clone, Copy)]
pub(crate) struct PublicationMark {
    top_level: bool,
    observations: u64,
}

impl Checker<'_, '_> {
    /// Open a computation whose answer a memo may publish under
    /// [`Self::publishable_since`]. Native resolves a type's members once and
    /// publishes them unconditionally; this port's resolution can answer
    /// provisionally inside an active resolution (a circular read answers
    /// error) and inside a flow loop (an incomplete loop type), so those do
    /// not publish. Read only after [`Self::memo_frames`] admitted the
    /// request, which already excluded the mapper frames.
    pub(crate) fn publication_mark(&self) -> PublicationMark {
        PublicationMark {
            top_level: self.resolutions.depth() == 0,
            observations: self.resolutions.observations(),
        }
    }

    /// Whether the computation opened at `mark` may publish: no flow loop is
    /// active, and either no resolution was open when it began
    /// (the rule before `r4-perf3.md` §2) or nothing it did
    /// depended on the open frames — no cycle closed and no probe saw a frame
    /// (`resolution::Resolutions::observations`). A circular read inside an
    /// active resolution is how such an answer turns provisional, and it is
    /// exactly what the count sees (`r4-perf3.md` §2).
    pub(crate) fn publishable_since(&self, mark: PublicationMark) -> bool {
        self.flow_loop_stack.is_empty()
            && (mark.top_level || self.resolutions.observations() == mark.observations)
    }

    /// Whether `receiver` has call and construct signatures, as
    /// `signatures_of_type_kind` answers them (an unresolved `None` counts as
    /// none): the two facts `getPropertyOfTypeEx` (`checker.go:18918`) reads
    /// from the receiver's resolved members — `resolved.CallSignatures`,
    /// `resolved.ConstructSignatures` — to pick the `Function` family before
    /// `Object`. Native resolves them once per type; this memo publishes the
    /// pair per receiver (`r5-perf4.md` §2).
    ///
    /// Admitted by [`Self::memo_frames`] over the declarations of the
    /// receiver's symbol, for a receiver that is not a mapped type, not
    /// instantiable and has no `signature_types` entry; a hit is honoured
    /// only while it still has none, so a list re-homed onto a reserved
    /// identity (`declared.rs`'s `complete_object` callers) or attached later
    /// is read fresh. Published when both lists were decided (`Some`), the
    /// receiver is its own apparent type, and [`Self::publishable_since`].
    pub(crate) fn receiver_signature_kinds(&mut self, receiver: TypeId) -> (bool, bool) {
        let symbol = match self.store.get(receiver).data {
            TypeData::Named { members: Some(owner), .. } => Some(owner),
            TypeData::Anonymous { symbol, .. } => Some(symbol),
            _ => None,
        };
        let eligible = symbol.is_some()
            && !self.signature_types.contains_key(&receiver)
            && !self.mapped_types.contains_key(&receiver)
            && !self.store.get(receiver).flags.intersects(TypeFlags::INSTANTIABLE);
        let binder = self.binder;
        let frames = match symbol {
            Some(symbol) if eligible => {
                self.memo_frames(&binder.symbols().get(symbol).declarations, None)
            }
            _ => None,
        };
        let Some(frames) = frames else {
            return self.receiver_signature_kinds_worker(receiver).0;
        };
        let kinds = if let Some(&kinds) = self.perf_links.receiver_signature_kinds.get(&receiver) {
            kinds
        } else {
            let mark = self.publication_mark();
            let (kinds, decided) = self.receiver_signature_kinds_worker(receiver);
            if decided
                && self.publishable_since(mark)
                && !self.signature_types.contains_key(&receiver)
                && self.apparent_type(receiver) == receiver
            {
                self.perf_links.receiver_signature_kinds.insert(receiver, kinds);
            }
            kinds
        };
        self.alias_evaluation_bindings = frames;
        kinds
    }

    /// [`Self::receiver_signature_kinds`]' worker: the pair, and whether both
    /// lists were decided.
    fn receiver_signature_kinds_worker(&mut self, receiver: TypeId) -> ((bool, bool), bool) {
        let call = self.signatures_of_type_kind(receiver, SignatureKind::Call);
        let construct = self.signatures_of_type_kind(receiver, SignatureKind::Construct);
        let has = |list: &Option<Vec<_>>| list.as_ref().is_some_and(|list| !list.is_empty());
        ((has(&call), has(&construct)), call.is_some() && construct.is_some())
    }

    /// Admit a memo request whose answer is computed from the syntax under
    /// `anchors` and `extra` (and from what that syntax names lexically):
    /// `None` when the caller's frames may change it, else the caller's
    /// alias-evaluation frames, taken so the computation runs without them.
    /// The caller restores them.
    ///
    /// Print mode and a mapped-template frame are never admitted
    /// (`docs/architecture/mapper-mode.md`). An alias-evaluation frame binds
    /// type-parameter symbols; the answer can read one only from inside that
    /// parameter's scope, so the request is admitted when no bound
    /// parameter's owner encloses an anchor. Bases need no check: a heritage
    /// name resolves lexically, and nothing declared inside a type-parameter
    /// scope (a function body, an alias, a conditional type) is nameable from
    /// outside it.
    pub(crate) fn memo_frames(
        &mut self,
        anchors: &[NodeId],
        extra: Option<NodeId>,
    ) -> Option<Vec<FxHashMap<SymbolId, TypeId>>> {
        if self.mapped_template_depth > 0 || self.identity_unmapped_type_parameters {
            return None;
        }
        if self.alias_evaluation_bindings.iter().all(FxHashMap::is_empty) {
            return Some(std::mem::take(&mut self.alias_evaluation_bindings));
        }
        let mut owners = std::mem::take(&mut self.perf_links.owners_scratch);
        owners.clear();
        let admitted = self.collect_bound_scope_owners(&mut owners)
            && anchors.iter().copied().chain(extra).all(|anchor| {
                let mut current = Some(anchor);
                while let Some(node) = current {
                    if owners.contains(&node) {
                        return false;
                    }
                    current = self.nodes.parent(node);
                }
                true
            });
        self.perf_links.owners_scratch = owners;
        admitted.then(|| std::mem::take(&mut self.alias_evaluation_bindings))
    }

    /// The scope owner of every parameter the open alias-evaluation frames
    /// bind; `false` when one has no recognised owner.
    fn collect_bound_scope_owners(&self, owners: &mut Vec<NodeId>) -> bool {
        for frame in &self.alias_evaluation_bindings {
            for &parameter in frame.keys() {
                for &declaration in &self.binder.symbols().get(parameter).declarations {
                    let Some(owner) = self.type_parameter_scope_owner(declaration) else {
                        return false;
                    };
                    if !owners.contains(&owner) {
                        owners.push(owner);
                    }
                }
            }
        }
        true
    }

    /// The node whose subtree is a type parameter declaration's scope: its
    /// declaring alias, signature, class, interface or mapped type, or for an
    /// `infer` declaration the enclosing conditional type. `None` (never
    /// admitted) for any other shape, including JSDoc `@template`.
    fn type_parameter_scope_owner(&self, declaration: NodeId) -> Option<NodeId> {
        if self.nodes.kind(declaration) != SyntaxKind::TypeParameter {
            return None;
        }
        let owner = self.nodes.parent(declaration)?;
        match self.nodes.kind(owner) {
            SyntaxKind::InferType => {
                let mut current = self.nodes.parent(owner);
                while let Some(node) = current {
                    if self.nodes.kind(node) == SyntaxKind::ConditionalType {
                        return Some(node);
                    }
                    current = self.nodes.parent(node);
                }
                None
            }
            SyntaxKind::JSDocTemplateTag => None,
            _ => Some(owner),
        }
    }
}

#[cfg(test)]
mod receiver_signature_kinds_tests {
    use tsr_ast::{HasNodeId, NodeId};
    use tsr_core::Arena;

    use crate::Checker;
    use crate::signatures::SignatureKind;

    fn with_checker(source: &str, test: impl FnOnce(&mut Checker<'_, '_>, NodeId)) {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let root = parsed.source_file.node_id().unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        test(&mut checker, root);
    }

    #[test]
    fn callable_and_plain_receivers_publish_distinct_kinds() {
        with_checker(
            "interface Callable { (): void } interface Newable { new (): object } interface Plain { x: number }",
            |checker, root| {
                let mut kinds = Vec::new();
                for name in ["Callable", "Newable", "Plain"] {
                    let symbol = checker.binder.lookup_local(root, name).unwrap();
                    let ty = checker.get_declared_type_of_symbol(symbol);
                    kinds.push(checker.receiver_signature_kinds(ty));
                    assert!(checker.perf_links.receiver_signature_kinds.contains_key(&ty));
                    // A hit answers what the worker answered.
                    assert_eq!(checker.receiver_signature_kinds(ty), kinds[kinds.len() - 1]);
                }
                assert_eq!(kinds, [(true, false), (false, true), (false, false)]);
            },
        );
    }

    #[test]
    fn a_later_signature_list_bypasses_the_published_answer() {
        with_checker(
            "interface Callable { (): void } interface Plain { x: number }",
            |checker, root| {
                let callable = checker.binder.lookup_local(root, "Callable").unwrap();
                let callable = checker.get_declared_type_of_symbol(callable);
                let plain = checker.binder.lookup_local(root, "Plain").unwrap();
                let plain = checker.get_declared_type_of_symbol(plain);
                assert_eq!(checker.receiver_signature_kinds(plain), (false, false));
                // What `declared.rs` does when it re-homes a resolved literal's
                // list onto its reserved identity after `complete_object`.
                let signatures =
                    checker.signatures_of_type_kind(callable, SignatureKind::Call).unwrap();
                checker.signature_types.insert(plain, signatures);
                assert_eq!(checker.receiver_signature_kinds(plain), (true, false));
            },
        );
    }
}
