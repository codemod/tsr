//! Per-checker memo tables for answers native caches in symbol or type links
//! and this port recomputed per query. Each table mirrors one native cache;
//! its key, owner, publication states, context and expensive work boundary
//! are recorded in `docs/parity/notes/r4-perf.md` (the checker port
//! convention, `docs/conventions.md`).

use rustc_hash::FxHashMap;
use tsr_ast::{NodeId, SyntaxKind};
use tsr_binder::SymbolId;

use crate::checker::Checker;
use crate::signatures::{Signature, SignatureKind};
use crate::types::TypeId;

/// Memo tables owned by one [`Checker`]. Nothing here is shared between
/// checkers: every key and value names ids of the owning checker's stores.
#[derive(Default)]
pub(crate) struct PerfLinks {
    /// The declared-plus-inherited call (`[0]`), construct (`[1]`) and
    /// abstract-construct (`[2]`) signatures of a merged interface or
    /// type-literal symbol, before the receiver's mapper: native
    /// `resolveObjectTypeMembers`' `callSignatures` / `constructSignatures`
    /// on the symbol's declared type, read through `getSignaturesOfType`.
    /// Holds only decided answers (`r4-perf.md` §2).
    pub(crate) interface_signatures: [FxHashMap<SymbolId, Vec<Signature>>; 3],
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
    /// Reused buffer for [`Checker::memo_frames`]' scope owners.
    owners_scratch: Vec<NodeId>,
}

/// [`PerfLinks::heritage_bases`]' key: base symbol, reference location, the
/// first written type argument's node and the written argument count. A
/// written argument list is one contiguous node list, so its first node and
/// length name the slice.
pub(crate) type HeritageBaseKey = (SymbolId, Option<NodeId>, Option<NodeId>, usize);

pub(crate) const fn signature_kind_slot(kind: SignatureKind) -> usize {
    match kind {
        SignatureKind::Call => 0,
        SignatureKind::Construct => 1,
        SignatureKind::AbstractConstruct => 2,
    }
}

impl Checker<'_, '_> {
    /// Whether an answer computed now may be published. Native resolves a
    /// type's members once and publishes them unconditionally; this port's
    /// resolution can answer provisionally inside an active resolution (a
    /// circular read answers error) and inside a flow loop (an incomplete
    /// loop type), so those do not publish. Read only after
    /// [`Self::interface_signature_frames`] admitted the request, which
    /// already excluded the mapper frames.
    pub(crate) fn signature_links_publishable(&self) -> bool {
        self.resolutions.depth() == 0 && self.flow_loop_stack.is_empty()
    }

    /// Whether a computed signature list may be published: every member's
    /// return is materialized (a lazy return's pending sentinel and a
    /// provisional circular answer both read as `errorType`, and the
    /// completion rewrites `signature_types`, not this table).
    pub(crate) fn signatures_decided(&self, signatures: &[Signature]) -> bool {
        signatures.iter().all(|signature| signature.r#type != self.intrinsics.error)
    }

    /// Admit a [`PerfLinks::interface_signatures`] request for the merged
    /// `symbol`: `None` when the answer may depend on the caller's frames,
    /// else the caller's alias-evaluation frames, taken so the computation
    /// runs without them (the caller restores them).
    ///
    /// Print mode and a mapped-template frame are never admitted
    /// (`docs/architecture/mapper-mode.md`). An alias-evaluation frame binds
    /// type-parameter symbols; the answer can read one only from inside that
    /// parameter's scope, so the request is admitted when no bound
    /// parameter's owner encloses one of `symbol`'s declarations. Bases need
    /// no check: a heritage name resolves lexically, and nothing declared
    /// inside a type-parameter scope (a function body, an alias, a conditional
    /// type) is nameable from outside it.
    pub(crate) fn interface_signature_frames(
        &mut self,
        symbol: SymbolId,
    ) -> Option<Vec<FxHashMap<SymbolId, TypeId>>> {
        let binder = self.binder;
        self.memo_frames(&binder.symbols().get(symbol).declarations, None)
    }

    /// Admit a memo request whose answer is computed from the syntax under
    /// `anchors` and `extra` (and from what that syntax names lexically):
    /// `None` when the caller's frames may change it, else the caller's
    /// alias-evaluation frames, taken so the computation runs without them.
    /// The caller restores them. See [`Self::interface_signature_frames`] for
    /// the rule.
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
