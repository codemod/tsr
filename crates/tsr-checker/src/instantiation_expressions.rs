//! Instantiation expressions: `f<number>`, `obj.m<U>`, `typeof C<T>`.
//!
//! Ported from `checkExpressionWithTypeArguments` (`checker.go:10637`) and
//! `getInstantiationExpressionType` (`checker.go:10660`) with its two
//! closures, `getInstantiatedSignatures` and `getInstantiatedTypePart`, at the
//! pinned `vendor/typescript-go` @ `5b1047d`.
//!
//! # The cache and its boundaries
//!
//! - **Native operation:** `c.instantiationExpressionTypes`, keyed by
//!   `InstantiationExpressionKey{nodeId, typeId}` and written once per key
//!   after `getInstantiatedType` completes.
//! - **Key identity and owner:** `InstantiationExpressionLinks::types`,
//!   keyed by the `ExpressionWithTypeArguments`/`TypeQueryNode` id, the
//!   expression type's [`TypeId`] and the open alias-evaluation frames
//!   ([`Checker::flattened_alias_bindings`]); private to this Checker.
//!   Upstream's key has no frame half because upstream computes the node
//!   once, over the declared type parameters, and an alias instantiation
//!   maps that one result through its mapper. This port evaluates an alias
//!   body under a frame instead (`alias_evaluation_bindings`), so the type
//!   arguments under the node resolve to the frame's bindings and the frame
//!   is part of the input: `typeof C<T>` under `T = number` and under
//!   `T = string` are two results (`tsr-2zk.1102`). The frame half is the
//!   same flattening `type_literal_key` uses, so the two caches partition
//!   alike. No receiver or print mode enters the key.
//! - **Publication states:** absent (not computed), active (`None`: a
//!   re-entry answers the gap, see `get_instantiation_expression_type`) or
//!   completed. A gap in any
//!   part (an unresolved signature list, a signature that cannot be
//!   instantiated) completes the entry as the port's gap, `errorType`, rather
//!   than a partial object type, and publishes no diagnostic.
//! - **Expensive work boundary:** the signature instantiation, done once per
//!   key; the minted object copies the source's property table rather than
//!   re-deriving it per read. With no frame open (the common case) the frame
//!   half is an empty vector and allocates nothing.
//!
//! # Diagnostics
//!
//! Upstream reports TS2635 and `checkTypeArguments`' constraint failures as a
//! side effect of the computation. This port keeps reports on the check walk
//! (ADR-0040): the computation parks its reports per node in
//! `InstantiationExpressionLinks::reports` and the walk's visit
//! ([`Checker::check_instantiation_expression_reports`]) forces the type and
//! drains them. Recomputation for a new expression type (a new key) parks
//! again, as upstream would report again. A computation under an open alias
//! frame parks nothing: it stands in for upstream's mapper over the one
//! frame-free computation, which is the one that reports.
//!
//! `docs/parity/notes/r5-instexpr.md` records the representation choices.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind, TypeNode};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::objects::{AnonymousProperty, Member};
use crate::signatures::{Signature, SignatureKind};
use crate::types::{TypeData, TypeId};

/// `c.instantiationExpressionTypes` and the reports its computations park
/// for the check walk. Owned by the Checker; see the module header for the
/// key identity and publication states.
/// One extends type's entry in `InstantiationExpressionLinks::conditional_extends`:
/// the registered type parameters it mentions (in registry order) with their
/// printed names, and its permissive and restrictive instantiations once made.
pub(crate) struct ExtendsInstantiations {
    pub(crate) parameters: std::rc::Rc<[TypeId]>,
    pub(crate) names: std::rc::Rc<[String]>,
    pub(crate) permissive: Option<TypeId>,
    pub(crate) restrictive: Option<TypeId>,
}

#[derive(Default)]
pub(crate) struct InstantiationExpressionLinks {
    /// `(node, expression type, open alias frames) -> result`: `None` while
    /// the computation is active, `Some` once completed.
    types: rustc_hash::FxHashMap<InstantiationExpressionKey, Option<TypeId>>,
    /// Reports parked by a computation, drained once by the walk's visit.
    reports: rustc_hash::FxHashMap<NodeId, Vec<(NodeId, Diagnostic)>>,
    /// `Checker::indexed_type_literal_member`'s publication state
    /// (declared.rs, `r6-declared.md` §2): the type-literal instantiation
    /// keys whose selected member is resolving (inserted before, removed
    /// after), and the literal nodes a re-entrant read sent back to the eager
    /// road (inserted once, never removed).
    pub(crate) lazy_member_reads: rustc_hash::FxHashSet<crate::declared::TypeLiteralKey>,
    pub(crate) eager_indexed_literals: rustc_hash::FxHashSet<NodeId>,
    /// getIndexType's `resolvedIndexType` for the deferred `keyof T` mint of
    /// `get_type_from_type_node_worker` (declared.rs): `(operand type,
    /// printed text) -> mint`, written once when the mint is made, never
    /// invalidated (`r6-declared.md` §2).
    pub(crate) deferred_keyof_mints: rustc_hash::FxHashMap<(TypeId, String), TypeId>,
    /// getPermissiveInstantiation / getRestrictiveInstantiation's per-type
    /// caches for a conditional's extends type, read by
    /// `Checker::extends_instantiation` (declared.rs): `extends -> entry`,
    /// or `None` when it mentions no type parameter. The entry's parameter
    /// list is written with it; each instantiation slot is written once, when
    /// first asked for; never invalidated (`r6-declared3.md` §1).
    pub(crate) conditional_extends: rustc_hash::FxHashMap<TypeId, Option<ExtendsInstantiations>>,
    /// getRestrictiveTypeParameter's per-parameter clone
    /// (`CachedTypeKindRestrictiveTypeParameter`), read by
    /// `Checker::restrictive_type_parameter` (declared.rs): `parameter ->
    /// unconstrained clone`. Written once per parameter, never invalidated
    /// (`r6-declared3.md` §1).
    pub(crate) restrictive_type_parameters: rustc_hash::FxHashMap<TypeId, TypeId>,
    /// The merged symbols of the global `Iterable`, `IterableIterator`,
    /// `AsyncIterable` and `AsyncIterableIterator` at arity 3, whose printed
    /// references elide default-identical trailing arguments
    /// (`Checker::reference_print_arity`, declared.rs). Resolved once, on
    /// first use, as native resolves them at checker creation
    /// (checker.go:1088-1097); never invalidated.
    pub(crate) iterable_elision_targets: Option<std::rc::Rc<[tsr_binder::SymbolId]>>,
    /// `isTypeParameterPossiblyReferenced(tp, node)` (checker.go:22403),
    /// keyed `(declaration node, type-parameter symbol)`: the filter
    /// getObjectTypeInstantiation applies once per declaration and stores in
    /// `typeNodeLinks.outerTypeParameters`. Held here, in a links struct the
    /// declared lane owns, because `Checker`'s field list is main's; read by
    /// `Checker::type_literal_key` (declared.rs), which takes `&self`, hence
    /// the cell. Pure syntax plus name resolution: written once per key,
    /// never invalidated (`r6-declared.md` §1).
    pub(crate) possibly_referenced:
        std::cell::RefCell<rustc_hash::FxHashMap<(NodeId, tsr_binder::SymbolId), bool>>,
}

/// `InstantiationExpressionKey{nodeId, typeId}` plus the open alias frames
/// (module header).
type InstantiationExpressionKey = (NodeId, TypeId, Vec<(tsr_binder::SymbolId, TypeId)>);

/// `checkTypeArguments`' answer: the filled arguments, or `nil` after a
/// reported constraint failure.
enum TypeArgumentCheck {
    Satisfied(Vec<TypeId>),
    Failed,
}

/// What `getInstantiatedType`'s closures accumulate across one
/// `getInstantiationExpressionType` call.
#[derive(Default)]
struct InstantiationState {
    has_some_applicable_signature: bool,
    non_applicable_type: Option<TypeId>,
    /// A part this port cannot compute: the whole answer is the gap.
    gap: bool,
}

impl<'a> Checker<'a, '_> {
    /// `checkExpressionWithTypeArguments` (`checker.go:10637`) for the
    /// expression form, `f<number>`. A heritage clause's entry never reaches
    /// here: it is a type reference, not an expression.
    pub(crate) fn check_expression_with_type_arguments(&mut self, node: NodeId) -> TypeId {
        let Some(Node::ExpressionWithTypeArguments(with_arguments)) = self.node_map.get(node)
        else {
            return self.intrinsics.error;
        };
        let Some(expression) = with_arguments.expression else { return self.intrinsics.error };
        let expression_type = self.check_expression(expression);
        self.get_instantiation_expression_type(expression_type, node)
    }

    /// The type arguments `node` carries: an instantiation expression's or a
    /// type query's (`Node.TypeArgumentList()`).
    fn instantiation_type_arguments(&self, node: NodeId) -> Option<&'a [TypeNode<'a>]> {
        match self.node_map.get(node)? {
            Node::ExpressionWithTypeArguments(with_arguments) => {
                Some(with_arguments.type_arguments)
            }
            Node::TypeQueryNode(query) => Some(query.type_arguments),
            // resolveImportSymbolType's Value arm (`checker.go:24659`).
            Node::ImportTypeNode(import) => Some(import.type_arguments),
            _ => None,
        }
    }

    /// `getInstantiationExpressionType` (`checker.go:10660`).
    ///
    /// `node` is the `ExpressionWithTypeArguments` or the `TypeQueryNode`; a
    /// query without type arguments answers its expression type unchanged.
    pub(crate) fn get_instantiation_expression_type(
        &mut self,
        expression_type: TypeId,
        node: NodeId,
    ) -> TypeId {
        let Some(type_arguments) = self.instantiation_type_arguments(node) else {
            return self.intrinsics.error;
        };
        if Some(expression_type) == self.silent_never_type
            || self.is_error(expression_type)
            || !self.has_type_argument_list(node, type_arguments)
        {
            return expression_type;
        }
        let frames = self.flattened_alias_bindings();
        let under_alias_frame = !frames.is_empty();
        let key = (node, expression_type, frames);
        match self.instantiation_expressions.types.get(&key) {
            Some(&Some(cached)) => return cached,
            // Re-entered while active: `typeof f<T>` inside `f`'s own
            // signature. Upstream resolves signature members lazily and never
            // re-enters here; this port instantiates parameter types eagerly,
            // so the cycle answers the gap rather than recursing.
            Some(None) => return self.intrinsics.error,
            None => {}
        }
        self.instantiation_expressions.types.insert(key.clone(), None);
        let reported_before = self.diagnostics.len();
        let mut state = InstantiationState::default();
        let minted_from = self.store.len();
        let mut result =
            self.get_instantiated_type(expression_type, node, type_arguments, &mut state);
        let mut reports = self.diagnostics.split_off(reported_before);
        if state.gap || self.is_gap(result) {
            // A partial answer is a wrong answer that looks like a right one;
            // the reports of a computation this port could not finish are not
            // published either.
            result = self.intrinsics.error;
            reports.clear();
        } else {
            let error_type = if state.has_some_applicable_signature {
                state.non_applicable_type
            } else {
                Some(expression_type)
            };
            if let Some(error_type) = error_type
                && let Some(file) = self.source_file_of_for_diagnostics(node)
            {
                let span = self.type_argument_list_span(node, type_arguments);
                let printed = self.type_to_string(error_type);
                reports.push((
                    file,
                    Diagnostic::with_args(
                        &messages::TYPE_0_HAS_NO_SIGNATURES_FOR_WHICH_THE_TYPE_ARGUMENT_LIST_IS_APPLICABLE,
                        span,
                        [printed],
                    ),
                ));
            }
        }
        // createAnonymousTypeNodeEx's typeof-node reuse
        // (`crate::instantiation_type_query_reuse`): frame-free results only.
        if !under_alias_frame && !self.is_gap(result) {
            result = self.reuse_instantiation_type_query_node(
                node,
                expression_type,
                result,
                minted_from,
            );
        }
        self.instantiation_expressions.types.insert(key, Some(result));
        if !reports.is_empty() && !under_alias_frame {
            self.instantiation_expressions.reports.entry(node).or_default().extend(reports);
        }
        result
    }

    /// `node.TypeArgumentList() == nil`: a type query written without `<…>`
    /// has no list; an instantiation expression always has one, possibly
    /// empty (`fx<>`, TS1099).
    fn has_type_argument_list(&self, node: NodeId, type_arguments: &[TypeNode<'a>]) -> bool {
        !type_arguments.is_empty()
            || self.nodes.kind(node) == SyntaxKind::ExpressionWithTypeArguments
    }

    /// TS2635's location: `skipTrivia(text, typeArguments.Pos())` to
    /// `typeArguments.End()` — the written arguments, or the empty width
    /// before the closing `>` of an empty list.
    fn type_argument_list_span(
        &self,
        node: NodeId,
        type_arguments: &[TypeNode<'a>],
    ) -> tsr_core::Span {
        let first = type_arguments.first().and_then(TypeNode::node_id);
        let last = type_arguments.last().and_then(TypeNode::node_id);
        if let (Some(first), Some(last)) = (first, last) {
            let start = self.nodes.span(first).start;
            let end = self.nodes.span(last).end;
            return tsr_core::Span { start, end };
        }
        let end = self.nodes.span(node).end.saturating_sub(1);
        tsr_core::Span { start: end, end }
    }

    /// `getInstantiatedType`, the closure over union constituents.
    fn get_instantiated_type(
        &mut self,
        t: TypeId,
        node: NodeId,
        type_arguments: &'a [TypeNode<'a>],
        state: &mut InstantiationState,
    ) -> TypeId {
        let mut has_signatures = false;
        let mut has_applicable_signature = false;
        let result = self.get_instantiated_type_part(
            t,
            node,
            type_arguments,
            state,
            &mut has_signatures,
            &mut has_applicable_signature,
        );
        state.has_some_applicable_signature |= has_applicable_signature;
        if has_signatures && !has_applicable_signature && state.non_applicable_type.is_none() {
            state.non_applicable_type = Some(t);
        }
        result
    }

    /// `getInstantiatedTypePart`.
    fn get_instantiated_type_part(
        &mut self,
        t: TypeId,
        node: NodeId,
        type_arguments: &'a [TypeNode<'a>],
        state: &mut InstantiationState,
        has_signatures: &mut bool,
        has_applicable_signature: &mut bool,
    ) -> TypeId {
        if state.gap {
            return t;
        }
        let flags = self.store.get(t).flags;
        if flags.contains(TypeFlags::OBJECT) {
            // `resolveStructuredTypeMembers` creates signatures whose return
            // types resolve lazily, so `declare function h<T>(): typeof h<T>`
            // never re-enters `h`'s return upstream. This port completes
            // returns when it reads signatures; reading one whose return is
            // being resolved would close a cycle native never forms (TS2577).
            // The part is the gap instead.
            if self.signature_return_is_active(t) {
                state.gap = true;
                return t;
            }
            let (Some(call), Some(construct)) = (
                self.signatures_of_type_kind(t, SignatureKind::Call),
                self.signatures_of_type_kind(t, SignatureKind::Construct),
            ) else {
                state.gap = true;
                return t;
            };
            let Some(instantiated_call) = self.get_instantiated_signatures(&call, type_arguments)
            else {
                state.gap = true;
                return t;
            };
            let Some(instantiated_construct) =
                self.get_instantiated_signatures(&construct, type_arguments)
            else {
                state.gap = true;
                return t;
            };
            *has_signatures |= !call.is_empty() || !construct.is_empty();
            *has_applicable_signature |=
                !instantiated_call.is_empty() || !instantiated_construct.is_empty();
            if !same_signatures(&instantiated_call, &call)
                || !same_signatures(&instantiated_construct, &construct)
            {
                let minted = self.mint_instantiation_expression_type(
                    t,
                    instantiated_call,
                    instantiated_construct,
                );
                state.gap |= minted.is_none();
                return minted.unwrap_or(t);
            }
        } else if flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE) {
            if let Some(constraint) = self.base_constraint_of_type(t) {
                let instantiated = self.get_instantiated_type_part(
                    constraint,
                    node,
                    type_arguments,
                    state,
                    has_signatures,
                    has_applicable_signature,
                );
                if instantiated != constraint {
                    return instantiated;
                }
            }
        } else if let TypeData::Union { types, .. } = &self.store.get(t).data {
            // `mapType(t, getInstantiatedType)`: each constituent runs the
            // outer closure, with its own signature bookkeeping.
            let types = types.clone();
            let mut mapped = Vec::with_capacity(types.len());
            let mut changed = false;
            for constituent in types {
                let image = self.get_instantiated_type(constituent, node, type_arguments, state);
                changed |= image != constituent;
                mapped.push(image);
            }
            if state.gap {
                return t;
            }
            return if changed { self.get_union_type(&mapped) } else { t };
        } else if let TypeData::Intersection { types, .. } = &self.store.get(t).data {
            let types = types.clone();
            let mut mapped = Vec::with_capacity(types.len());
            let mut changed = false;
            for constituent in types {
                let image = self.get_instantiated_type_part(
                    constituent,
                    node,
                    type_arguments,
                    state,
                    has_signatures,
                    has_applicable_signature,
                );
                changed |= image != constituent;
                mapped.push(image);
            }
            if state.gap {
                return t;
            }
            return if changed { self.get_intersection_type(&mapped, None) } else { t };
        }
        t
    }

    /// A type query with type arguments whose leftmost name is the very
    /// symbol whose type or return is being resolved: `typeof f<T>` in `f`'s
    /// own parameter or return annotation. Upstream reaches the query only
    /// after `f`'s anonymous type exists, and reads its signatures lazily;
    /// this port would re-enter the active resolution and record a cycle
    /// native never forms. Such a query is the gap.
    pub(crate) fn type_query_closes_eager_cycle(&self, query: &tsr_ast::TypeQueryNode<'_>) -> bool {
        if query.type_arguments.is_empty() {
            return false;
        }
        let mut name = query.expr_name;
        while let Some(tsr_ast::EntityName::QualifiedName(qualified)) = name {
            name = qualified.left;
        }
        let Some(tsr_ast::EntityName::Identifier(identifier)) = name else { return false };
        let Some(location) = identifier.node_id else { return false };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            location,
            identifier.text,
            SymbolFlags::VALUE,
        ) else {
            return false;
        };
        let symbol = self.binder.merged_symbol(symbol);
        self.resolutions.on_stack(symbol, crate::resolution::PropertyName::Type)
            || self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
                self.resolutions.active_signature_keys(declaration).next().is_some()
            })
    }

    /// Whether a declaration of `t`'s symbol has its return type on the
    /// resolution stack.
    fn signature_return_is_active(&self, t: TypeId) -> bool {
        let TypeData::Anonymous { symbol, .. } = self.store.get(t).data else { return false };
        let symbol = self.binder.merged_symbol(symbol);
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            self.resolutions.active_signature_keys(declaration).next().is_some()
        })
    }

    /// `getInstantiatedSignatures`: the generic signatures whose
    /// type-parameter window admits the written count
    /// (`hasCorrectTypeArgumentArity`), each through `checkTypeArguments`
    /// (reporting) and `getSignatureInstantiation`. A signature whose type
    /// arguments fail their constraints stays uninstantiated, as upstream's
    /// `core.SameMap` keeps it. `None` when an instantiation is a gap.
    fn get_instantiated_signatures(
        &mut self,
        signatures: &[Signature],
        type_arguments: &'a [TypeNode<'a>],
    ) -> Option<Vec<Signature>> {
        let mut result = Vec::with_capacity(signatures.len());
        for signature in signatures {
            if signature.type_parameters.is_empty()
                || !has_correct_type_argument_arity(signature, type_arguments.len())
            {
                continue;
            }
            match self.check_instantiation_type_arguments(signature, type_arguments)? {
                TypeArgumentCheck::Satisfied(arguments) => {
                    result.push(self.get_signature_instantiation(signature, &arguments)?);
                }
                TypeArgumentCheck::Failed => result.push(signature.clone()),
            }
        }
        Some(result)
    }

    /// `checkTypeArguments` (`checker.go:9222`) with `reportErrors`: the
    /// written arguments filled by `fillMissingTypeArguments`, then each
    /// written argument against its parameter's constraint under that mapper;
    /// the first failure is reported at the argument node and answers
    /// [`TypeArgumentCheck::Failed`]. `None` is a side this port cannot decide.
    fn check_instantiation_type_arguments(
        &mut self,
        signature: &Signature,
        type_arguments: &'a [TypeNode<'a>],
    ) -> Option<TypeArgumentCheck> {
        let parameters = self.type_parameter_types(signature)?;
        let written: Vec<TypeId> =
            type_arguments.iter().map(|&argument| self.get_type_from_type_node(argument)).collect();
        if written.iter().any(|&argument| self.is_gap(argument)) {
            return None;
        }
        let is_javascript = self.in_js_file(signature.declaration);
        let arguments =
            self.fill_missing_type_arguments(signature, &parameters, written, is_javascript)?;
        let names: Vec<String> =
            signature.type_parameters.iter().map(|parameter| parameter.name.clone()).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let map: Vec<(TypeId, TypeId)> =
            parameters.iter().copied().zip(arguments.iter().copied()).collect();
        for (position, &argument_node) in type_arguments.iter().enumerate() {
            let Some(constraint) = self.type_parameter_constraint(parameters[position]) else {
                continue;
            };
            let target = self.instantiate_type(constraint, &map, &parameters, &names);
            let source = arguments[position];
            if self.is_gap(target) {
                return None;
            }
            match self.relate_ternary(source, target, crate::relater::Relation::Assignable) {
                crate::relater::Ternary::Related => {}
                crate::relater::Ternary::Unknown => return None,
                crate::relater::Ternary::NotRelated => {
                    let at = argument_node.node_id()?;
                    let span = self.error_span(at);
                    self.report_relation_failure(
                        at,
                        span,
                        None,
                        source,
                        target,
                        Some(&messages::TYPE_0_DOES_NOT_SATISFY_THE_CONSTRAINT_1),
                    );
                    return Some(TypeArgumentCheck::Failed);
                }
            }
        }
        Some(TypeArgumentCheck::Satisfied(arguments))
    }

    /// `fillMissingTypeArguments` (`checker.go:19458`): defaults instantiated
    /// over the arguments so far, else `getDefaultTypeArgumentType` —
    /// `unknown`, or `any` in a JavaScript file.
    fn fill_missing_type_arguments(
        &mut self,
        signature: &Signature,
        parameters: &[TypeId],
        mut arguments: Vec<TypeId>,
        is_javascript: bool,
    ) -> Option<Vec<TypeId>> {
        if arguments.len() >= parameters.len() {
            return Some(arguments);
        }
        let names: Vec<String> =
            signature.type_parameters.iter().map(|parameter| parameter.name.clone()).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let base_default =
            if is_javascript { self.intrinsics.any } else { self.intrinsics.unknown };
        while arguments.len() < parameters.len() {
            let position = arguments.len();
            let filled = match signature.type_parameters[position].default {
                Some(default) => {
                    let mut default = default;
                    if is_javascript
                        && (default == self.intrinsics.unknown
                            || default == self.intrinsics.empty_object)
                    {
                        default = self.intrinsics.any;
                    }
                    // "Map invalid forward references in default types to the
                    // error type": parameters not yet filled map to errorType.
                    let mut map: Vec<(TypeId, TypeId)> =
                        parameters.iter().copied().zip(arguments.iter().copied()).collect();
                    for &later in &parameters[position..] {
                        map.push((later, self.intrinsics.native_error));
                    }
                    let image = self.instantiate_type(default, &map, parameters, &names);
                    if self.is_gap(image) {
                        return None;
                    }
                    image
                }
                None => base_default,
            };
            arguments.push(filled);
        }
        Some(arguments)
    }

    /// `getSignatureInstantiation` (`checker.go:19293`) without inferred type
    /// parameters: `instantiateSignature` with the type parameters erased.
    fn get_signature_instantiation(
        &mut self,
        signature: &Signature,
        arguments: &[TypeId],
    ) -> Option<Signature> {
        let parameters = self.type_parameter_types(signature)?;
        let names: Vec<String> =
            signature.type_parameters.iter().map(|parameter| parameter.name.clone()).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let map: Vec<(TypeId, TypeId)> =
            parameters.iter().copied().zip(arguments.iter().copied()).collect();
        let mut instance = signature.clone();
        instance.type_parameters.clear();
        self.instantiate_signature(instance, &map, &parameters, &names)
    }

    /// `newObjectType(ObjectFlagsAnonymous|ObjectFlagsInstantiationExpressionType,
    /// t.symbol)` with `setStructuredTypeMembers(result, resolved.members,
    /// callSignatures, constructSignatures, resolved.indexInfos)`.
    ///
    /// The members are `t`'s resolved table, copied as this port's
    /// authoritative property table ([`Checker::anonymous_properties`]); the
    /// signatures are recorded in [`Checker::signature_types`]; the index infos
    /// in [`Checker::object_literal_index_infos`]. The type keeps `t`'s symbol,
    /// so property lookup reaches the same declarations. `None` when the
    /// table cannot be read completely.
    fn mint_instantiation_expression_type(
        &mut self,
        t: TypeId,
        call: Vec<Signature>,
        construct: Vec<Signature>,
    ) -> Option<TypeId> {
        let properties = self.resolved_property_table(t)?;
        let indexes = self.get_index_infos_of_type(t)?;
        let mut signatures = call;
        signatures.extend(construct);
        // createTypeNodeFromObjectType: call, then construct signatures, then
        // index infos, then properties; a lone signature with nothing else
        // prints as the arrow form.
        let (text, signature_node) = if let [signature] = signatures.as_slice()
            && properties.is_empty()
            && indexes.is_empty()
        {
            (self.signature_to_string(signature), true)
        } else {
            let mut members: Vec<Member> = signatures
                .iter()
                .map(|signature| Member::Signature {
                    printed: crate::objects::signature_member_text(self, signature),
                })
                .collect();
            for index in &indexes {
                members.extend(self.index_info_members(index)?);
            }
            members.extend(self.anonymous_property_members(&properties)?);
            (crate::objects::render_object_type(&members), false)
        };
        let minted = match self.store.get(t).data {
            TypeData::Anonymous { symbol, .. } => {
                self.store.new_anonymous(TypeFlags::OBJECT, text, symbol, signature_node)
            }
            TypeData::Named { members, .. } => {
                self.store.new_named(TypeFlags::OBJECT, text, members)
            }
            _ => return None,
        };
        // An empty table adds nothing the symbol's own lookup lacks, and an
        // entry would route a lone signature through the callable-expando
        // re-render, which prints braces.
        if !properties.is_empty() {
            self.anonymous_properties.insert(minted, (properties, true));
        }
        self.signature_types.insert(minted, signatures);
        if !indexes.is_empty() {
            self.object_literal_index_infos.insert(minted, indexes);
        }
        if !signature_node {
            // The braces text renders methods and index rows; the
            // callable-expando re-render knows property syntax only.
            self.alias_named_signature_types.insert(minted);
        }
        Some(minted)
    }

    /// `resolved.members` as an ordered property table: `t`'s own table when it
    /// carries one, else every property name `getPropertiesOfType` answers,
    /// each read through `t` (so a reference's members arrive substituted).
    /// Order is `getNamedMembers`' (`checker.go:22049`): for a class or
    /// interface owner, members declared inside it first, then the rest.
    fn resolved_property_table(&mut self, t: TypeId) -> Option<Vec<AnonymousProperty>> {
        if let Some((properties, _)) = self.anonymous_properties.get(&t) {
            return Some(properties.clone());
        }
        let names = self.get_property_names_of_type(t)?;
        let owner = match self.store.get(t).data {
            TypeData::Anonymous { symbol, .. } => Some(symbol),
            TypeData::Named { members, .. } => members,
            _ => None,
        }
        .map(|symbol| self.binder.merged_symbol(symbol))
        .filter(|&symbol| {
            self.binder
                .symbols()
                .get(symbol)
                .flags
                .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
        });
        let mut contained = Vec::with_capacity(names.len());
        let mut rest = Vec::new();
        for name in names {
            let origin = self.get_property_of_type(t, &name)?;
            let optional = self.property_is_optional(origin);
            let mut property_type = self.get_type_of_property_of_type(t, &name)?;
            if self.is_gap(property_type) {
                return None;
            }
            if optional {
                property_type = self.remove_missing_or_undefined_type(property_type);
            }
            let flags = self.binder.symbols().get(origin).flags;
            let method = flags.contains(SymbolFlags::METHOD);
            let readonly = self.is_readonly_property_of_type(t, &name);
            let printed_name = self.callable_property_name(origin, &name);
            let property = AnonymousProperty {
                accessor_write: None,
                method,
                origin: Some(origin),
                checked_declaration: None,
                printed_slot: crate::objects::PrintedSlot::printed(
                    self.type_to_string(property_type),
                ),
                slot: crate::objects::PropertySlot::resolved(property_type),
                name,
                printed_name,
                optional,
                readonly,
            };
            let inside =
                owner.is_some_and(|owner| self.is_declaration_contained_by_symbol(origin, owner));
            if owner.is_none() || inside {
                contained.push(property);
            } else {
                rest.push(property);
            }
        }
        contained.extend(rest);
        Some(contained)
    }

    /// `isDeclarationContainedBy` (`checker.go`): some declaration of `symbol`
    /// lies inside some declaration of `container`.
    fn is_declaration_contained_by_symbol(
        &self,
        symbol: tsr_binder::SymbolId,
        container: tsr_binder::SymbolId,
    ) -> bool {
        let containers = &self.binder.symbols().get(container).declarations;
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            let mut current = self.nodes.parent(declaration);
            while let Some(node) = current {
                if containers.contains(&node) {
                    return true;
                }
                current = self.nodes.parent(node);
            }
            false
        })
    }

    /// The check walk's half of `checkExpressionWithTypeArguments`: the
    /// `instanceof` right-operand rule (TS2848), then the type (forced here so
    /// its reports exist) and the reports its computation parked.
    pub(crate) fn check_instantiation_expression_reports(&mut self, node: NodeId) {
        match self.node_map.get(node) {
            Some(Node::ExpressionWithTypeArguments(with_arguments)) => {
                if self
                    .nodes
                    .parent(node)
                    .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::HeritageClause)
                {
                    return;
                }
                self.check_instanceof_instantiation_expression(node);
                self.check_expression(Expression::ExpressionWithTypeArguments(with_arguments));
            }
            // A type node's type is resolved by whatever reads it; the walk
            // only publishes what that computation parked.
            Some(Node::TypeQueryNode(_)) => {}
            _ => return,
        }
        if let Some(reports) = self.instantiation_expressions.reports.remove(&node) {
            for (file, diagnostic) in reports {
                self.report(file, diagnostic);
            }
        }
    }

    /// `checkExpressionWithTypeArguments`' first rule: an instantiation
    /// expression inside the right operand of `instanceof`
    /// (`WalkUpParenthesizedExpressions`, `isNodeDescendantOf`).
    fn check_instanceof_instantiation_expression(&mut self, node: NodeId) {
        let mut parent = self.nodes.parent(node);
        while let Some(current) = parent
            && self.nodes.kind(current) == SyntaxKind::ParenthesizedExpression
        {
            parent = self.nodes.parent(current);
        }
        let Some(parent) = parent else { return };
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(parent) else { return };
        if binary.operator_token.is_none_or(|token| token.kind != SyntaxKind::InstanceOfKeyword) {
            return;
        }
        let Some(right) = binary.right.and_then(|right| right.node_id()) else { return };
        let mut current = Some(node);
        while let Some(at) = current {
            if at == right {
                if let Some(file) = self.source_file_of_for_diagnostics(node) {
                    let span = self.error_span(node);
                    self.report(
                        file,
                        Diagnostic::new(
                            &messages::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_NOT_BE_AN_INSTANTIATION_EXPRESSION,
                            span,
                        ),
                    );
                }
                return;
            }
            current = self.nodes.parent(at);
        }
    }
}

/// `hasCorrectTypeArgumentArity` (`checker.go:9214`).
fn has_correct_type_argument_arity(signature: &Signature, count: usize) -> bool {
    let minimum = signature
        .type_parameters
        .iter()
        .rposition(|parameter| parameter.default.is_none())
        .map_or(0, |index| index + 1);
    count == 0 || count >= minimum && count <= signature.type_parameters.len()
}

/// `core.Same`: upstream compares slice identity, which `core.SameMap`
/// preserves exactly when no element changed. Here a filtered or instantiated
/// list differs in length or in some signature's carried types.
fn same_signatures(left: &[Signature], right: &[Signature]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| {
            a.type_parameters.len() == b.type_parameters.len()
                && a.r#type == b.r#type
                && a.declaration == b.declaration
                && a.target.is_none() == b.target.is_none()
        })
}

#[cfg(test)]
mod tests {
    use crate::checker::Checker;

    /// `tsr-2zk.1102`: under frame-bound alias evaluation, `typeof f<T>`
    /// evaluated with `T = number` and then with `T = string` are two results,
    /// not the first one returned twice.
    #[test]
    fn instantiation_expressions_are_keyed_on_the_alias_frame() {
        let arena = tsr_core::Arena::new();
        let source = "declare function f<T>(x: T): T;\ntype F<T> = typeof f<T>;";
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let tsr_ast::Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[1]
        else {
            panic!("F alias")
        };
        let symbol = bound.symbol_of(alias.node_id.unwrap()).unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let (number, string) = (checker.intrinsics.number, checker.intrinsics.string);
        let with_number = checker.evaluate_alias_body(symbol, &[number]).expect("F<number>");
        let with_string = checker.evaluate_alias_body(symbol, &[string]).expect("F<string>");
        assert_eq!(checker.type_to_string(with_number), "(x: number) => number");
        assert_eq!(checker.type_to_string(with_string), "(x: string) => string");
    }
}
