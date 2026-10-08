//! Per-checker memo tables for answers native caches in symbol or type links
//! and this port recomputed per query. Each table mirrors one native cache;
//! its key, owner, publication states, context and expensive work boundary
//! are recorded in `docs/parity/notes/r4-perf.md` (the checker port
//! convention, `docs/conventions.md`).

use rustc_hash::FxHashMap;
use tsr_ast::{NodeId, SyntaxKind};
use tsr_binder::SymbolId;

use crate::checker::Checker;
use crate::types::TypeId;

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
    pub(crate) structured_property_names: FxHashMap<SymbolId, Vec<String>>,
    /// Reused buffer for [`Checker::memo_frames`]' scope owners.
    owners_scratch: Vec<NodeId>,
}

/// [`PerfLinks::heritage_bases`]' key: base symbol, reference location, the
/// first written type argument's node and the written argument count. A
/// written argument list is one contiguous node list, so its first node and
/// length name the slice.
pub(crate) type HeritageBaseKey = (SymbolId, Option<NodeId>, Option<NodeId>, usize);

impl Checker<'_, '_> {
    /// Whether an answer computed now may be published. Native resolves a
    /// type's members once and publishes them unconditionally; this port's
    /// resolution can answer provisionally inside an active resolution (a
    /// circular read answers error) and inside a flow loop (an incomplete
    /// loop type), so those do not publish. Read only after
    /// [`Self::memo_frames`] admitted the request, which already excluded the
    /// mapper frames.
    pub(crate) fn signature_links_publishable(&self) -> bool {
        self.resolutions.depth() == 0 && self.flow_loop_stack.is_empty()
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
