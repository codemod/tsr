//! Type argument inference for generic signatures.
//!
//! Ported from inferTypeArguments (internal/checker/checker.go) and inferTypes /
//! getInferredType (internal/checker/inference.go). The walk accumulates
//! covariant and contravariant candidates separately, measures reference
//! variances, and applies inference priority before choosing candidates.
//!
//! Contextual return inference is weaker than argument inference. Outer
//! inference contexts carry independent ordinary and return mappers, and
//! callbacks fix only the parameters read by their contextual types.
//!
//! Substitution rebuilds references, unions, intersections, tuples, signatures,
//! indexed accesses, and anonymous property types when their metadata is known.
//! Unknown structural metadata remains a gap. Union matching, general mapped
//! inference, recursive generic signature relations, and dependent constraint
//! resolution still have unported parts; their local guards record the limits.
//!
//! Closed constraints filter pure return candidates, or select the compatible
//! candidate direction before falling back to the constraint. Written type
//! argument constraint diagnostics are not implemented here.
//!
//! Literal inference and declaration widening are separate. For example,
//! conformance/callGenericFunctionWithZeroTypeArguments.types records `f(1)`
//! as `1`, while the variable initialized by that call is `number`. The tests
//! assert the call expression so declaration widening cannot hide an incorrect
//! inference result.

use tsr_ast::{Expression, Node, NodeId};

use crate::{
    checker::Checker,
    signatures::Signature,
    types::{TypeData, TypeId},
};

/// Identity of two concrete signature views for contextual instantiation.
/// Types include return, this, predicates, inputs, constraints, defaults and
/// owned parameter identities. Original inputs affect callback comparison.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SignatureContextKey {
    source: NodeId,
    target: NodeId,
    source_parameters: Vec<TypeId>,
    target_parameters: Vec<TypeId>,
    source_types: Vec<TypeId>,
    target_types: Vec<TypeId>,
    source_original_inputs: Vec<TypeId>,
    target_original_inputs: Vec<TypeId>,
}

bitflags::bitflags! {
    /// Ported from `InferencePriority` (`internal/checker/checker.go`).
    /// Lower numeric priorities replace weaker candidates; certain flags
    /// request a union/intersection rather than a common super/subtype.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct InferencePriority: u16 {
        const NONE = 0;
        const NAKED_TYPE_VARIABLE = 1 << 0;
        const SPECULATIVE_TUPLE = 1 << 1;
        const SUBSTITUTE_SOURCE = 1 << 2;
        const HOMOMORPHIC_MAPPED_TYPE = 1 << 3;
        const PARTIAL_HOMOMORPHIC_MAPPED_TYPE = 1 << 4;
        const MAPPED_TYPE_CONSTRAINT = 1 << 5;
        const CONTRAVARIANT_CONDITIONAL = 1 << 6;
        const RETURN_TYPE = 1 << 7;
        const LITERAL_KEYOF = 1 << 8;
        const NO_CONSTRAINTS = 1 << 9;
        const ALWAYS_STRICT = 1 << 10;
        const MAX_VALUE = 1 << 11;
        const IMPLIES_COMBINATION = Self::RETURN_TYPE.bits()
            | Self::MAPPED_TYPE_CONSTRAINT.bits() | Self::LITERAL_KEYOF.bits();
    }
}

bitflags::bitflags! {
    /// InferenceContext fallback modes (internal/checker/checker.go).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct InferenceFlags: u8 {
        const NONE = 0;
        const NO_DEFAULT = 1 << 0;
        const ANY_DEFAULT = 1 << 1;
    }
}

/// A snapshot of the active `InferenceContext` used by an inner call's
/// `cloneInferenceContext(..., NoDefault)` (internal/checker/checker.go).
#[derive(Clone, Debug)]
pub(crate) struct InferenceContextSnapshot {
    pub(crate) signature: Signature,
    pub(crate) inferences: Vec<InferenceInfo>,
    pub(crate) return_inferences: Vec<InferenceInfo>,
    pub(crate) flags: InferenceFlags,
    pub(crate) inferential: bool,
    pub(crate) intra_expression_sites: Vec<(NodeId, TypeId)>,
    /// InferenceContext.outerReturnMapper (createOuterReturnMapper,
    /// inference.go:1423): built once from the first snapshot an inner call
    /// requests, then reused by every later inner call of this context.
    pub(crate) outer_return_map: Option<Vec<(TypeId, TypeId)>>,
}

/// cloneTypeParameter / instantiateSignatureEx (internal/checker/checker.go).
/// A fresh parameter retains its target and the composed constraint mapper.
#[derive(Clone, Debug)]
pub(crate) struct InstantiatedTypeParameter {
    pub(crate) target: TypeId,
    pub(crate) map: Vec<(TypeId, TypeId)>,
    pub(crate) parameters: Vec<TypeId>,
    pub(crate) names: Vec<String>,
}

impl Checker<'_, '_> {
    /// The signature-less inference context used by getConditionalType.
    /// getTypeFromInference preserves candidates rather than applying the
    /// signature's argument widening or common-supertype selection.
    pub(crate) fn infer_conditional_parameters(
        &mut self,
        source: TypeId,
        target: TypeId,
        parameters: &[TypeId],
    ) -> Vec<(TypeId, Option<TypeId>)> {
        let mut infos = Vec::new();
        self.infer_from_types_with_priority(
            source,
            target,
            parameters,
            &mut infos,
            0,
            InferencePriority::NO_CONSTRAINTS | InferencePriority::ALWAYS_STRICT,
        );
        parameters
            .iter()
            .map(|&parameter| {
                let inferred =
                    if let Some(info) = infos.iter().find(|i| i.type_parameter == parameter) {
                        if !info.candidates.is_empty() {
                            Some(self.get_union_type(&info.candidates))
                        } else if !info.contra_candidates.is_empty() {
                            Some(self.get_intersection_type(&info.contra_candidates, None))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                (parameter, inferred)
            })
            .collect()
    }

    /// getInferredType for signature-less contexts with direct parameter
    /// dependencies. Install the provisional candidate before resolving a
    /// constraint, as upstream does before consulting nonFixingMapper.
    pub(crate) fn resolve_conditional_inferences(
        &mut self,
        inferences: &[(TypeId, Option<TypeId>)],
        constraints: &[Option<TypeId>],
    ) -> Vec<(TypeId, TypeId)> {
        fn resolve(
            checker: &mut Checker<'_, '_>,
            index: usize,
            inferences: &[(TypeId, Option<TypeId>)],
            constraints: &[Option<TypeId>],
            resolved: &mut [Option<TypeId>],
        ) -> TypeId {
            if let Some(inferred) = resolved[index] {
                return inferred;
            }
            let candidate = inferences[index].1;
            let mut inferred = candidate.unwrap_or(checker.intrinsics.unknown);
            resolved[index] = Some(inferred);
            if let Some(mut constraint) = constraints[index] {
                if let Some(dependency) =
                    inferences.iter().position(|&(parameter, _)| parameter == constraint)
                {
                    constraint = resolve(checker, dependency, inferences, constraints, resolved);
                }
                if candidate.is_none()
                    || checker.relate_ternary(
                        inferred,
                        constraint,
                        crate::relater::Relation::Assignable,
                    ) == crate::relater::Ternary::NotRelated
                {
                    inferred = constraint;
                }
            }
            resolved[index] = Some(inferred);
            inferred
        }
        let mut resolved = vec![None; inferences.len()];
        (0..inferences.len())
            .map(|index| {
                let inferred = resolve(self, index, inferences, constraints, &mut resolved);
                (inferences[index].0, inferred)
            })
            .collect()
    }

    /// getThisArgumentOfCall/getThisArgumentType (checker.go:9345). A bare
    /// call uses void; a property or indexed call retains its receiver through
    /// transparent wrappers and optional-chain marker removal.
    pub(crate) fn this_argument_type_of_call(&mut self, call: Option<NodeId>) -> TypeId {
        let Some(node) = call.and_then(|call| self.node_map.get(call)) else {
            return self.intrinsics.void;
        };
        if let Node::BinaryExpression(binary) = node {
            return binary.right.map_or(self.intrinsics.void, |right| self.check_expression(right));
        }
        let expression = match node {
            Node::CallExpression(call) => call.expression,
            Node::TaggedTemplateExpression(template) => template.tag,
            Node::Decorator(decorator) if !self.legacy_decorators => {
                decorator.expression.map(Expression::from)
            }
            _ => None,
        };
        let Some(callee) = expression.and_then(|expression| expression.node_id()) else {
            return self.intrinsics.void;
        };
        let callee = self.skip_outer_expressions(callee);
        let (receiver, optional) = match self.node_map.get(callee) {
            Some(Node::PropertyAccessExpression(access)) => {
                (access.expression, access.question_dot_token.is_some())
            }
            Some(Node::ElementAccessExpression(access)) => {
                (access.expression, access.question_dot_token.is_some())
            }
            _ => return self.intrinsics.void,
        };
        let Some(receiver) = receiver else { return self.intrinsics.void };
        let raw = self.check_expression(receiver);
        self.get_optional_expression_type(raw, receiver.node_id(), optional)
    }

    /// The type of a call whose resolved signature is generic.
    ///
    /// Ported from `Checker.inferTypeArguments` (`checker.go:9390`) followed by
    /// `Checker.getSignatureInstantiation` (`checker.go:19293`), collapsed: the
    /// signature is never instantiated as a whole, only its return type is
    /// answered, because the return type is the only part of a call's signature
    /// a `.types` baseline records for the call itself.
    ///
    /// Answers `errorType` for every shape the module docs list.
    pub(crate) fn check_generic_call(
        &mut self,
        signature: &Signature,
        call: Option<NodeId>,
        arguments: &[Expression<'_>],
    ) -> TypeId {
        self.check_generic_call_with(signature, call, arguments, None)
    }

    /// [`Checker::check_generic_call`] with an out-slot for the WHOLE
    /// instantiated signature — `getSignatureInstantiation`
    /// (`checker.go:19293`) as the §487 overload walk needs it: parameter
    /// types instantiated with the same final map the return is, type
    /// parameters cleared so the caller sees a concrete candidate. The slot
    /// stays `None` on every decline path, which is the walk's "cannot decide
    /// this candidate" signal.
    pub(crate) fn check_generic_call_with(
        &mut self,
        signature: &Signature,
        call: Option<NodeId>,
        arguments: &[Expression<'_>],
        instantiated: Option<&mut Option<Signature>>,
    ) -> TypeId {
        self.check_generic_call_with_mode(signature, call, arguments, instantiated, false)
    }

    fn check_generic_call_with_mode(
        &mut self,
        signature: &Signature,
        call: Option<NodeId>,
        arguments: &[Expression<'_>],
        instantiated: Option<&mut Option<Signature>>,
        overload_failure: bool,
    ) -> TypeId {
        let previous = call.and_then(|call| {
            self.active_inference_contexts.insert(
                call,
                InferenceContextSnapshot {
                    signature: signature.clone(),
                    inferences: Vec::new(),
                    return_inferences: Vec::new(),
                    inferential: false,
                    intra_expression_sites: Vec::new(),
                    outer_return_map: None,
                    flags: if self.in_js_file(call) {
                        InferenceFlags::ANY_DEFAULT
                    } else {
                        InferenceFlags::NONE
                    },
                },
            )
        });
        let result = self.check_generic_call_worker(
            signature,
            call,
            arguments,
            instantiated,
            overload_failure,
        );
        if let Some(call) = call {
            if let Some(previous) = previous {
                self.active_inference_contexts.insert(call, previous);
            } else {
                self.active_inference_contexts.remove(&call);
            }
        }
        result
    }

    /// inferTypeArguments collects contextual return candidates before any
    /// argument expression is checked (checker.go:9390). Nested generic calls
    /// must see this snapshot when they instantiate their outer context.
    fn contextual_return_inferences(
        &mut self,
        returned: TypeId,
        parameters: &[TypeId],
        call: Option<NodeId>,
        skip_binding_patterns: bool,
    ) -> (Vec<InferenceInfo>, Vec<InferenceInfo>) {
        let mut infos = Vec::new();
        // inferTypeArguments (checker.go) makes a weak ReturnType inference
        // into the final context and an independent ordinary-priority pass
        // for returnMapper. Our contextual-type road supplies written types;
        // unannotated binding-pattern contexts are not computed here.
        let mut return_mapper = Vec::new();
        let previous_uninstantiated = self.uninstantiated_context_node;
        self.uninstantiated_context_node = call;
        let contextual_return = call.and_then(|call| self.get_contextual_type_of_call(call));
        self.uninstantiated_context_node = previous_uninstantiated;
        if let Some(call_id) = call
            && let Some(outer) = contextual_return
        {
            // An outer context's uninferred parameters map to silentNever
            // under NoDefault. Keeping their identities would make an inner
            // default infer an unresolved outer variable instead of its default.
            // `inferTypeArguments` instantiates a single generic contextual
            // signature with its own parameters so they remain actual types,
            // rather than being erased by signature inference (checker.go).
            let outer = if let Some(signatures) = self.signature_types.get(&outer).cloned()
                && let [signature] = signatures.as_slice()
                && !signature.type_parameters.is_empty()
            {
                let mut signature = signature.clone();
                signature.type_parameters.clear();
                let text = self.type_to_string(outer);
                let source = self.store.new_named(crate::flags::TypeFlags::OBJECT, text, None);
                self.signature_types.insert(source, vec![signature]);
                source
            } else {
                outer
            };
            let return_source = self.instantiate_outer_inference_context(outer, call_id, false);
            let outer = self.instantiate_outer_inference_context(outer, call_id, true);
            self.infer_from_types_with_priority(
                outer,
                returned,
                parameters,
                &mut infos,
                0,
                InferencePriority::RETURN_TYPE,
            );
            self.infer_from_types(return_source, returned, parameters, &mut return_mapper, 0);
        } else if let Some(call_id) = call
            && !skip_binding_patterns
            && let Some(pattern) = self.binding_pattern_return_context(call_id)
        {
            // isFromBindingPattern: the pattern's implied type feeds
            // `context.returnMapper` only, never `context.inferences`.
            let return_source = self.instantiate_outer_inference_context(pattern, call_id, false);
            self.infer_from_types(return_source, returned, parameters, &mut return_mapper, 0);
        }
        (infos, return_mapper)
    }

    /// The contextual type `inferTypeArguments` (`checker.go:9390`) reads
    /// from an unannotated array binding pattern when it does not pass
    /// `ContextFlagsSkipBindingPatterns` (some type parameter has no
    /// default): `getContextualTypeForVariableLikeDeclaration`'s
    /// `getTypeFromBindingPattern(name, true, false)`
    /// (`getTypeFromArrayBindingPattern`, `checker.go:17957`). Each plain
    /// name element is `nonInferrableAnyType`, a hole `any`, so the implied
    /// type of `const [a, b] = f(...)` is `[any, any]`.
    ///
    /// Built per call, uncached: the tuple is interned by
    /// [`Checker::create_tuple_type`]. Only that shape is computed (no
    /// cached contextual type changes); a pattern with a rest, a default, a
    /// nested pattern, or no element (`createIterableType(any)`) answers
    /// `None`, as does an object pattern. `nonInferrableAnyType` is this
    /// port's `any`: inference infers whole tuples from the pattern, never
    /// its element alone, in the shapes accepted here.
    fn binding_pattern_return_context(&mut self, call: NodeId) -> Option<TypeId> {
        let parent = self.nodes.parent(call)?;
        let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(parent) else {
            return None;
        };
        if declaration.r#type.is_some()
            || declaration.initializer.and_then(|initializer| initializer.node_id()) != Some(call)
            || self.in_js_file(call)
        {
            return None;
        }
        let Some(tsr_ast::BindingName::BindingPattern(pattern)) = declaration.name else {
            return None;
        };
        if self.nodes.kind(pattern.node_id?) != tsr_ast::SyntaxKind::ArrayBindingPattern
            || pattern.elements.is_empty()
        {
            return None;
        }
        let any = self.intrinsics.any;
        let mut elements = Vec::with_capacity(pattern.elements.len());
        for element in pattern.elements {
            let element_id = element.node_id?;
            match self.node_map.get(element_id)? {
                Node::OmittedExpression(_) => elements.push(any),
                Node::BindingElement(binding)
                    if binding.dot_dot_dot_token.is_none()
                        && binding.initializer.is_none()
                        && matches!(binding.name, Some(tsr_ast::BindingName::Identifier(_))) =>
                {
                    elements.push(any);
                }
                _ => return None,
            }
        }
        Some(self.create_tuple_type(elements, false))
    }

    /// checkExpressionWithContextualType's literal regularization
    /// (checker.go): an argument literal loses freshness when it is a literal
    /// of `instantiateContextualType(paramType, arg, ContextFlagsNone)`.
    /// With a return mapper that instantiation incorporates only return-type
    /// inferences and drops the boolean pair (#48363); otherwise the raw
    /// parameter type answers through its base constraint.
    #[allow(clippy::type_complexity)]
    fn contextual_argument_literal_source(
        &mut self,
        source: TypeId,
        parameter_type: TypeId,
        return_map: Option<&(Vec<(TypeId, TypeId)>, Vec<TypeId>)>,
        names: &[&str],
    ) -> TypeId {
        use crate::flags::TypeFlags;
        if !self.store.get(source).fresh {
            return source;
        }
        let mut contextual = parameter_type;
        if let Some((map, parameters)) = return_map
            && self.maybe_type_of_kind(parameter_type, TypeFlags::INSTANTIABLE)
        {
            contextual =
                self.instantiate_instantiable_types(parameter_type, map, parameters, names);
            if let TypeData::Union { types, .. } = &self.store.get(contextual).data
                && types.contains(&self.intrinsics.regular_true)
                && types.contains(&self.intrinsics.regular_false)
            {
                let (t, f) = (self.intrinsics.regular_true, self.intrinsics.regular_false);
                contextual = self.filter_type(contextual, |_, part| part != t && part != f);
            }
        }
        if self.is_literal_of_contextual_type(source, contextual) == Some(true) {
            self.get_regular_type_of_literal_type(source)
        } else {
            source
        }
    }

    /// getMutableArrayOrTupleType (checker.go:29571). Preserve mutable generic
    /// array identities; readonly inputs become mutable variadic tuple images.
    fn mutable_spread_argument_type(&mut self, source: TypeId) -> TypeId {
        if let TypeData::Union { types, .. } = self.store.get(source).data.clone() {
            let parts: Vec<_> =
                types.into_iter().map(|part| self.mutable_spread_argument_type(part)).collect();
            return self.get_union_type(&parts);
        }
        if source == self.intrinsics.any || self.const_context_is_mutable_array_like(source) {
            return source;
        }
        self.normalize_variadic_tuple(
            vec![crate::tuples::TupleElement {
                r#type: source,
                spread: true,
                optional: false,
                label: None,
            }],
            false,
        )
    }

    /// getEffectiveCallArguments and getSpreadArgumentType (checker.go:30042,
    /// :29500), for the inference tail after ordinary positional parameters.
    fn inference_spread_argument_type(
        &mut self,
        arguments: &[Expression<'_>],
        checked: &[TypeId],
        start: usize,
        rest: TypeId,
    ) -> Option<TypeId> {
        use crate::{flags::TypeFlags, tuples::TupleElement};
        // Tuple spreads become synthetic arguments before contextual widening.
        let mut effective = Vec::new();
        for (position, &argument) in arguments.iter().enumerate().skip(start) {
            if let Expression::SpreadElement(node) = argument {
                let source = self.check_expression(node.expression?);
                if self.is_error(source) {
                    return None;
                }
                if let Some((types, _)) = self.tuple_element_lists.get(&source).cloned() {
                    let optional = self.tuple_optional_masks.get(&source).cloned();
                    let labels = self.tuple_labels.get(&source).cloned();
                    for (index, mut ty) in types.into_iter().enumerate() {
                        if self.strict_null_checks
                            && optional
                                .as_ref()
                                .and_then(|m| m.get(index))
                                .copied()
                                .unwrap_or(false)
                        {
                            ty = self.get_union_type(&[ty, self.intrinsics.undefined]);
                        }
                        let label = labels.as_ref().and_then(|l| l.get(index)).cloned().flatten();
                        effective.push((ty, false, label, None));
                    }
                } else if let Some((elements, _)) =
                    self.variadic_tuple_elements.get(&source).cloned()
                {
                    for element in elements {
                        let mut ty = element.r#type;
                        if element.optional && self.strict_null_checks {
                            ty = self.get_union_type(&[ty, self.intrinsics.undefined]);
                        }
                        effective.push((ty, element.spread, element.label, None));
                    }
                } else {
                    effective.push((source, true, None, None));
                }
            } else {
                effective.push((*checked.get(position)?, false, None, Some(argument)));
            }
        }
        let in_const_context = self.is_const_type_variable(rest, 0);
        if let [(source, true, _, _)] = effective.as_slice() {
            let source = *source;
            if self.tuple_array_like(source) || source == self.intrinsics.any {
                return Some(self.mutable_spread_argument_type(source));
            }
            let element = self.array_spread_element_type(source)?;
            let array =
                self.global_type_symbol(if in_const_context { "ReadonlyArray" } else { "Array" })?;
            return Some(self.create_type_reference(array, vec![element]));
        }
        let count = effective.len();
        let mut elements = Vec::with_capacity(count);
        for (index, (source, spread, label, expression)) in effective.into_iter().enumerate() {
            let ty = if spread {
                if self.tuple_array_like(source) || source == self.intrinsics.any {
                    source
                } else {
                    let element = self.array_spread_element_type(source)?;
                    let array = self.global_type_symbol("Array")?;
                    self.create_type_reference(array, vec![element])
                }
            } else {
                let key = self
                    .store
                    .intern(TypeFlags::NUMBER_LITERAL, TypeData::NumberLiteral(index.to_string()));
                let contextual = if self.tuple_element_lists.contains_key(&rest)
                    || self.variadic_tuple_elements.contains_key(&rest)
                {
                    self.contextual_type_for_element_expression(
                        rest,
                        index,
                        Some(count),
                        None,
                        None,
                    )
                } else {
                    self.resolved_indexed_access_type(rest, key, false)
                }
                .unwrap_or(self.intrinsics.unknown);
                let source = if let Some(expression) = expression {
                    self.const_literal_inference_source(
                        expression,
                        source,
                        contextual,
                        in_const_context,
                    )
                } else {
                    source
                };
                // checkExpressionWithContextualType regularizes literals that
                // match a primitive-constrained contextual variable before the
                // spread builder applies ordinary literal widening.
                let source = if self.is_literal_of_contextual_type(source, contextual) == Some(true)
                {
                    self.get_regular_type_of_literal_type(source)
                } else {
                    source
                };
                if in_const_context
                    || self.maybe_type_of_kind(
                        contextual,
                        TypeFlags::PRIMITIVE
                            | TypeFlags::INDEX
                            | TypeFlags::TEMPLATE_LITERAL
                            | TypeFlags::STRING_MAPPING,
                    )
                {
                    self.get_regular_type_of_literal_type(source)
                } else {
                    self.get_widened_literal_type(source)
                }
            };
            elements.push(TupleElement { r#type: ty, spread, optional: false, label });
        }
        let readonly = in_const_context && !self.const_context_is_mutable_array_like(rest);
        Some(self.normalize_variadic_tuple(elements, readonly))
    }

    fn check_generic_call_worker(
        &mut self,
        signature: &Signature,
        call: Option<NodeId>,
        arguments: &[Expression<'_>],
        instantiated: Option<&mut Option<Signature>>,
        overload_failure: bool,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let parameter_types = self.type_parameter_types(signature);
        let (mut infos, return_mapper) = if self.written_type_arguments(call).is_none()
            && let Some(parameters) = parameter_types.as_deref()
        {
            let skip_binding_patterns =
                signature.type_parameters.iter().all(|parameter| parameter.default.is_some());
            self.contextual_return_inferences(
                signature.r#type,
                parameters,
                call,
                skip_binding_patterns,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        if let Some(call) = call
            && let Some(context) = self.active_inference_contexts.get_mut(&call)
        {
            context.inferences.clone_from(&infos);
            context.return_inferences.clone_from(&return_mapper);
        }
        // context.returnMapper = getMapperFromContext(cloneInferredPartOfContext
        // (returnContext)) (checker.go inferTypeArguments): only parameters
        // with return candidates are mapped; the rest stay themselves.
        let names = signature.type_parameters.iter().map(|p| p.name.as_str()).collect::<Vec<_>>();
        let return_literal_map = if let Some(parameters) = parameter_types.as_deref()
            && return_mapper
                .iter()
                .any(|info| !info.candidates.is_empty() || !info.contra_candidates.is_empty())
        {
            let flags = call
                .and_then(|call| self.active_inference_contexts.get(&call))
                .map_or(InferenceFlags::NONE, |context| context.flags);
            self.resolved_inference_map(&return_mapper, signature, parameters, flags).map(|map| {
                let map = map
                    .into_iter()
                    .filter(|(parameter, _)| {
                        return_mapper.iter().any(|info| {
                            info.type_parameter == *parameter
                                && (!info.candidates.is_empty()
                                    || !info.contra_candidates.is_empty())
                        })
                    })
                    .collect::<Vec<_>>();
                (map, parameters.to_vec())
            })
        } else {
            None
        };

        // inferSignatureInstantiationForOverloadFailure (checker.go) skips
        // context-sensitive arguments. A function requiring more parameters
        // than its callback context makes applicability fail by arity alone.
        let skip_context_sensitive =
            overload_failure || self.generic_callback_arity_failure(signature, arguments);
        // Upstream checks every argument (`checkExpression` through
        // `getEffectiveCallArguments`) whatever it then does with them, and the
        // arguments are needed here anyway. Spreads in a non-array rest tail
        // are expanded by inference_spread_argument_type below; unsupported
        // positional spread calls still check their argument nodes first.
        let mut argument_types: Vec<TypeId> = Vec::with_capacity(arguments.len());
        let mut skipped_generic_arguments = vec![false; arguments.len()];
        let mut spread = false;
        let mut preceding_inferences = infos.clone();
        let infer_preceding = self.written_type_arguments(call).is_none()
            && !arguments.iter().any(|argument| matches!(argument, Expression::SpreadElement(_)));
        for (index, &argument) in arguments.iter().enumerate() {
            if matches!(argument, Expression::SpreadElement(_)) {
                spread = true;
            }
            // Context-sensitive arguments type in phase 2 (ORDER, not
            // exclusion); the placeholder is error and phase 2 overwrites.
            if self.is_context_sensitive_argument(&argument) {
                argument_types.push(if skip_context_sensitive {
                    self.intrinsics.undefined
                } else {
                    self.intrinsics.error
                });
            } else {
                // checkExpressionWithContextualType pushes the inference
                // context while checking every ordinary argument
                // (checker.go:9485, :7486). This lets a contextual return
                // candidate instantiate nested object/array member contexts
                // before their literals widen.
                let use_active_context = return_literal_map.is_some()
                    && matches!(
                        argument,
                        Expression::ObjectLiteralExpression(_)
                            | Expression::ArrayLiteralExpression(_)
                            | Expression::ArrowFunction(_)
                            | Expression::FunctionExpression(_)
                    );
                let previous_inferential = if use_active_context
                    && let Some(call) = call
                    && let Some(context) = self.active_inference_contexts.get_mut(&call)
                {
                    Some(std::mem::replace(&mut context.inferential, true))
                } else {
                    None
                };
                let source = self.check_expression(argument);
                if let Some(previous_inferential) = previous_inferential
                    && let Some(call) = call
                    && let Some(context) = self.active_inference_contexts.get_mut(&call)
                {
                    context.inferential = previous_inferential;
                }
                // inferSignatureInstantiationForOverloadFailure adds both
                // SkipContextSensitive and SkipGenericFunctions (checker.go:
                // 9575). The latter replaces a single-signature generic
                // function with anyFunctionType only when its contextual
                // signature is non-generic (checker.go:7599). `undefined` is
                // this port's existing no-candidate placeholder for the retry.
                if overload_failure
                    && let Some(parameter) = signature.parameters.get(index)
                    && self.overload_failure_skips_generic_argument(source, parameter.r#type)
                {
                    skipped_generic_arguments[index] = true;
                    argument_types.push(self.intrinsics.undefined);
                    continue;
                }
                let source = match (signature.parameters.get(index), &return_literal_map) {
                    (Some(parameter), map) if !parameter.rest => self
                        .contextual_argument_literal_source(
                            source,
                            parameter.r#type,
                            map.as_ref(),
                            &names,
                        ),
                    _ => source,
                };
                argument_types.push(source);
                // Ordinary positional arguments contribute before the next
                // expression is checked (inferTypeArguments, checker.go:9485).
                // Keep the final per-argument buckets separate: this snapshot
                // only serves nested calls while their arguments are checked.
                if infer_preceding
                    && let Some(parameters) = parameter_types.as_deref()
                    && let Some(parameter) = signature.parameters.get(index)
                    && !parameter.rest
                {
                    let source = if signature.type_parameters.iter().any(|p| p.is_const) {
                        self.const_literal_inference_source(
                            argument,
                            source,
                            parameter.r#type,
                            false,
                        )
                    } else {
                        source
                    };
                    self.infer_from_types(
                        source,
                        parameter.r#type,
                        parameters,
                        &mut preceding_inferences,
                        0,
                    );
                    if let Some(call) = call
                        && let Some(context) = self.active_inference_contexts.get_mut(&call)
                    {
                        context.inferences.clone_from(&preceding_inferences);
                    }
                }
            }
        }
        // getSpreadArgumentType can consume spreads in a non-array rest tail.
        // Spreads before the rest still require effective positional arguments.
        let spread_in_rest_tail = signature.parameters.last().is_some_and(|parameter| {
            parameter.rest
                && !self.type_reference_targets.get(&parameter.r#type).is_some_and(|(target, _)| {
                    ["Array", "ReadonlyArray"].iter().any(|name| {
                        self.global_type_symbol(name).is_some_and(|array| {
                            self.binder.merged_symbol(array) == self.binder.merged_symbol(*target)
                        })
                    })
                })
                && arguments[..signature.parameters.len().saturating_sub(1).min(arguments.len())]
                    .iter()
                    .all(|argument| !matches!(argument, Expression::SpreadElement(_)))
        });
        if spread && !spread_in_rest_tail {
            for &argument in arguments {
                if self.is_context_sensitive_argument(&argument) {
                    let _ = self.check_expression(argument);
                }
            }
            // §341: the one spread shape this inference can decide — a SINGLE
            // spread argument against a single bare `...s: T[]` rest
            // parameter. The spread expression's own check already answers
            // the ELEMENT type (§286), and `getSpreadArgumentType`'s tuple
            // problem collapses to `T := element`:
            // `foo(...new SymbolIterator)` is `symbol`
            // (`iteratorSpreadInCall11`). Every other spread shape keeps the
            // gap this arm always was.
            if !arguments.is_empty()
                && arguments.iter().all(|a| matches!(a, Expression::SpreadElement(_)))
                && let [parameter] = signature.parameters.as_slice()
                && parameter.rest
                && let Some(parameters) = self.type_parameter_types(signature)
                && let [type_parameter] = parameters.as_slice()
                && let Some((target, type_args)) =
                    self.type_reference_targets.get(&parameter.r#type).cloned()
                && type_args.as_slice() == [*type_parameter]
                && self.global_type_symbol("Array").is_some_and(|array| {
                    self.binder.merged_symbol(target) == self.binder.merged_symbol(array)
                })
                && !argument_types.is_empty()
                && argument_types.iter().all(|&element| element != error)
            {
                // §543: N spreads, not one. `getSpreadArgumentType`
                // (`checker.go:31285`) builds ONE type out of every spread
                // argument, so against a bare `...s: T[]` the inference is
                // `T := union of the element types` — and for a single spread
                // that union is the element itself, which is why this
                // generalises §341 rather than replacing it.
                //
                // `foo(...new SymbolIterator, ...new _StringIterator)` against
                // `foo<T>(...s: T[])` is `string | symbol`
                // (`iteratorSpreadInCall7-10`). Before this, ONE spread worked
                // and TWO gapped the whole call — a shape whose minimal repro
                // is in `checker-notes-sitename.md` §21.
                //
                // **All-spread only.** A mix (`foo(1, ...new A)`) unions the
                // fixed arguments' types in too, and the fixed half has its own
                // literal-widening question (`getSpreadArgumentType` widens
                // through `checkExpressionWithContextualType`); that is a
                // separate measurement and keeps the gap it always had.
                // **The FIRST spread's element, not the union of them.**
                // Upstream infers `T` through the ordinary candidate machinery
                // (`getInferredType` -> `getCommonSupertype`, `inference.go`), and
                // with no common supertype among the candidates the first one
                // wins. `foo(...new SymbolIterator, ...new _StringIterator)`
                // records `symbol`, not `string | symbol` — measured: the union
                // moved the line from `any` to `string | symbol`, which is a
                // different wrong answer and scored no transition at all.
                let element = argument_types[0];
                let names =
                    signature.type_parameters.iter().map(|p| p.name.as_str()).collect::<Vec<_>>();
                let map = vec![(*type_parameter, element)];
                return self.instantiate_type(signature.r#type, &map, &parameters, &names);
            }
            return error;
        }

        let returned = signature.r#type;
        if returned == error {
            return error;
        }
        // A deferred keyof constraint now participates in primitive literal
        // retention (hasPrimitiveConstraint). Other unresolved constraints
        // still cannot support inference or constraint checking.
        if signature.type_parameters.iter().any(|parameter| {
            parameter.constraint.is_some_and(|constraint| {
                self.unresolved_types.contains(&constraint)
                    && !self.deferred_keyof_types.contains(&constraint)
            })
        }) {
            return error;
        }
        let Some(parameters) = parameter_types else {
            return error;
        };
        let names = signature.type_parameters.iter().map(|p| p.name.as_str()).collect::<Vec<_>>();
        // `f<string>(x)`: the caller wrote the type arguments, so there is
        // nothing to infer and substitution is all that is left.
        //
        // `checkTypeArguments` (`checker.go:9269`) fails the whole call when the
        // count is wrong and upstream resolves to the error signature, so the
        // arity guard sits *above* the return-type shortcut below: a call with
        // the wrong arity is an error even when its return type mentions no type
        // parameter. Defaults would make some shorter lists legal
        // (`fillMissingTypeArguments`); none is ported, so a signature with a
        // defaulted type parameter is a gap rather than a guess.
        if let Some(written) = self.written_type_arguments(call) {
            // `fillMissingTypeArguments` (`checker.go:19458`), the written
            // half (§38): a PARTIAL list is legal when defaults (or the
            // `unknown` fallback) cover the tail; a list longer than the
            // parameters is still the arity error.
            if written.len() > parameters.len() || written.contains(&error) {
                // §439: the ARITY error still resolves the call — upstream
                // reports TS2558 and answers through the error signature, so
                // a return that mentions no type parameter is its own answer
                // ('f<number, string, number>() : void',
                // `callWithWrongNumberOfTypeArguments`).
                if !written.contains(&error)
                    && !self.mentions_type_parameter(returned, &parameters, &names)
                {
                    return returned;
                }
                return error;
            }
            let mut map = Vec::with_capacity(parameters.len());
            for (position, &type_parameter) in parameters.iter().enumerate() {
                if let Some(&argument) = written.get(position) {
                    map.push((type_parameter, argument));
                    continue;
                }
                let image = match signature
                    .type_parameters
                    .get(position)
                    .and_then(|parameter| parameter.default)
                {
                    Some(default) => {
                        let filled = self.instantiate_type(default, &map, &parameters, &names);
                        if filled == error {
                            return error;
                        }
                        filled
                    }
                    None => self.intrinsics.unknown,
                };
                map.push((type_parameter, image));
            }
            let answer = self.instantiate_type(returned, &map, &parameters, &names);
            if answer != error
                && let Some(slot) = instantiated
            {
                *slot = self.instantiate_signature(signature.clone(), &map, &parameters, &names);
            }
            return answer;
        }

        // A return type that mentions no type parameter of this signature does
        // not depend on inference at all: `f<T>(x: T): string` is `string`
        // however `T` resolves. Upstream reaches the same answer the long way,
        // by instantiating a type that the mapper leaves alone.
        //
        // SS135: the shortcut is taken ONLY when no argument is
        // context-sensitive - the machinery below has SIDE EFFECTS the
        // arguments need (the serve memo that contextually types them:
        // `callIt<T>(obj): void` still fixes T for `obj`'s members). For a
        // benign return every decline below answers `returned` rather than
        // `error`, which reproduces the shortcut's answer exactly - the
        // machinery is run for its side effects, never to change the call's
        // own type.
        let benign = !self.mentions_type_parameter(returned, &parameters, &names);
        if benign
            && instantiated.is_none()
            && !arguments.iter().any(|argument| self.is_context_sensitive_argument(argument))
        {
            return returned;
        }
        let decline = if benign { returned } else { error };

        // A rest parameter makes position-to-argument mapping a tuple problem
        // (`getSpreadArgumentType`, `checker.go`), so the whole signature was a
        // gap rather than the rest position alone.
        //
        // **§939 narrows that to the tuple case it was actually about.** A rest
        // written `...items: T[]` is not a tuple problem: every argument from
        // that position on infers against the ELEMENT type, which is the arm
        // added to the loop below. A rest over anything else — a tuple, a
        // variadic, a type this port cannot read an element out of — still
        // declines the whole signature.
        //
        // The blanket form made `r1<T>(...items: T[]): T` answer `error` for
        // `r1(1)`, and with it `["a"].concat(["b"])`: an `Array`-shaped rest is
        // how every variadic lib signature is written.
        let rest_element = |checker: &mut Self, parameter: &crate::signatures::Parameter| {
            checker.type_reference_targets.get(&parameter.r#type).cloned().and_then(
                |(target, type_arguments)| match type_arguments.as_slice() {
                    [single] => ["Array", "ReadonlyArray"]
                        .iter()
                        .any(|name| {
                            checker
                                .global_type_symbol(name)
                                .map(|s| checker.binder.merged_symbol(s))
                                == Some(checker.binder.merged_symbol(target))
                        })
                        .then_some(*single),
                    _ => None,
                },
            )
        };
        let rest_parameters: Vec<crate::signatures::Parameter> =
            signature.parameters.iter().filter(|parameter| parameter.rest).cloned().collect();
        // §951: a rest parameter whose type is NOT array-shaped is upstream's
        // `getNonArrayRestType` (`relater.go:1858`), and it has its own arm
        // rather than being a decline — `inferTypeArguments`
        // (`checker.go:9489`) builds a TUPLE from the arguments at the rest
        // position and infers that against the rest type:
        //
        // ```go
        // if restType != nil && c.couldContainTypeVariables(restType) {
        //     spreadType := c.getSpreadArgumentType(args, argCount, len(args), restType, …)
        //     c.inferTypes(context.inferences, spreadType, restType, …)
        // }
        // ```
        //
        // `rest_element` above is that predicate exactly, negated: upstream's
        // name for "the rest is not an array" is the name of the function this
        // port was using to decline. The blanket decline made
        // `f<T extends unknown[]>(...args: T): T` answer `error` for
        // `f("a", 1)` — the shape every variadic-tuple case in the corpus is
        // written in, and the reason `genericRestParameters1`,
        // `strictBindCallApply1` and `variadicTuples1/2` carry the largest
        // tuple-print residue (§950 sized the family at 1,380 lines).
        //
        let spread_rest_position = rest_parameters
            .iter()
            .filter(|parameter| rest_element(self, parameter).is_none())
            .filter_map(|parameter| {
                signature.parameters.iter().position(|other| other.name == parameter.name)
            })
            .min();
        // One candidate per type parameter, from the positions typed by that
        // parameter *bare*. Inference from `x: T[]` against `number[]` is
        // `inferFromTypes` (`checker.go:21287`) and is not ported, so such a
        // position contributes nothing — which leaves its type parameter
        // unmapped, and an unmapped mention is what makes the answer below
        // `errorType` rather than a guess.
        // `hasCorrectArity` (`checker.go:8710`), the half a default can meet: a
        // call missing an argument for a *required* parameter is an error
        // upstream before inference starts. Without this, `pick(1)` against
        // `<T, U>(a: T, b: U): T` would answer `1` — the loop below no longer
        // fails on an unsupplied bare position, because an unsupplied
        // *optional* position (`p.then()`) is exactly what the default fill
        // exists for.
        let required =
            signature.parameters.iter().filter(|parameter| !parameter.optional && !parameter.rest);
        if argument_types.len() < required.count() {
            return decline;
        }
        // `inferTypes` (`inference.go:53`): every supplied argument walked
        // against its parameter's type, accumulating `(type parameter,
        // candidate)` pairs. See [`Checker::infer_from_types`] for which of
        // upstream's arms are ported and why the rest cannot be.
        // The summit unit (checker-notes-callres2.md): two-phase argument
        // ORDER — non-context-sensitive arguments infer first, then the
        // context-sensitive ones check (their contexts served instantiated
        // through the consumption rule below) and infer; the resolver then
        // answers per parameter over the collector.
        // inferTypeArguments (checker.go:9467) sets the non-array rest's
        // implied arity before this and ordinary argument inference.
        let implied_rest = spread_rest_position.and_then(|position| {
            let parameter = signature.parameters.get(position)?.r#type;
            parameters
                .contains(&parameter)
                .then_some((parameter, arguments.len().saturating_sub(position)))
                .filter(|_| {
                    !arguments[position.min(arguments.len())..]
                        .iter()
                        .any(|argument| matches!(argument, Expression::SpreadElement(_)))
                })
        });
        if let Some((parameter, arity)) = implied_rest {
            if let Some(info) = infos.iter_mut().find(|info| info.type_parameter == parameter) {
                info.implied_arity = Some(arity);
            } else {
                infos.push(InferenceInfo {
                    type_parameter: parameter,
                    priority: InferencePriority::MAX_VALUE,
                    implied_arity: Some(arity),
                    candidates: Vec::new(),
                    contra_candidates: Vec::new(),
                    fixed_type: None,
                    is_fixed: false,
                    top_level: true,
                });
            }
        }
        // inferTypeArguments infers the receiver against the signature's
        // this type after contextual returns and before ordinary arguments.
        if let Some(this_parameter) = &signature.this_parameter
            && self.target_could_contain_parameter(
                this_parameter.r#type,
                &parameters,
                &mut Vec::new(),
            )
        {
            let this_argument = self.this_argument_type_of_call(call);
            self.infer_from_types(this_argument, this_parameter.r#type, &parameters, &mut infos, 0);
        }
        if let Some(call) = call
            && let Some(context) = self.active_inference_contexts.get_mut(&call)
        {
            context.inferences.clone_from(&infos);
            context.return_inferences.clone_from(&return_mapper);
        }
        let mut deferred: Vec<usize> = Vec::new();
        // SS140: candidates collect into PER-ARGUMENT buckets merged in
        // index order below, so a later argument's candidate cannot outrank
        // an earlier literal member's (the E1-vs-E2 order bug) - while the
        // EXECUTION order stays exactly SS135's (interleaving the checks
        // themselves measured -610: early member checks freeze the summit
        // families through the caches).
        let mut buckets: Vec<Vec<InferenceInfo>> = vec![Vec::new(); arguments.len().max(1)];
        // Each bucket shares the context's arity metadata, without sharing
        // candidates: candidate order still follows argument position.
        if let Some((parameter, arity)) = implied_rest {
            for bucket in &mut buckets {
                bucket.push(InferenceInfo {
                    type_parameter: parameter,
                    priority: InferencePriority::MAX_VALUE,
                    implied_arity: Some(arity),
                    candidates: Vec::new(),
                    contra_candidates: Vec::new(),
                    fixed_type: None,
                    is_fixed: false,
                    top_level: true,
                });
            }
        }
        let mut inferred_type_parameters = Vec::new();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            // CheckModeSkipGenericFunctions produces anyFunctionType, an
            // ObjectFlagsNonInferrableType. The placeholder above preserves
            // argument indexing; it must not become an `undefined` candidate.
            if skipped_generic_arguments.get(index) == Some(&true) {
                continue;
            }
            // §939: a REST parameter takes EVERY argument from its position on,
            // each inferred against the rest's ELEMENT type.
            //
            // The loop is otherwise strictly positional, so `...items: T[]` saw
            // exactly one argument and inferred it against `T[]` rather than
            // `T` — which matches nothing, leaves `T` unmapped, and answers
            // `error` for the whole call. `r1<T>(...items: T[]): T` called
            // `r1(1, 2)` was `error`, and so was `["a"].concat(["b"])`: that
            // shape is how every variadic lib signature is written.
            //
            // The element type is read the same way §341's spread arm reads it —
            // an `Array`/`ReadonlyArray` reference with one argument. A rest
            // over a TUPLE is §86's positional expansion and keeps its own road.
            if parameter.rest {
                let Some(element) = rest_element(self, parameter) else {
                    // getSpreadArgumentType constructs const literal source
                    // views, including mutability, before collecting candidates.
                    let Some(spread) = self.inference_spread_argument_type(
                        arguments,
                        &argument_types,
                        index,
                        parameter.r#type,
                    ) else {
                        return decline;
                    };
                    let bucket = index.min(buckets.len() - 1);
                    self.infer_from_types(
                        spread,
                        parameter.r#type,
                        &parameters,
                        &mut buckets[bucket],
                        0,
                    );
                    continue;
                };
                for (position, &argument_expression) in arguments.iter().enumerate().skip(index) {
                    if self.is_context_sensitive_argument(&argument_expression) {
                        if !skip_context_sensitive
                            || matches!(
                                argument_expression,
                                Expression::ObjectLiteralExpression(_)
                                    | Expression::ArrayLiteralExpression(_)
                            )
                        {
                            deferred.push(position);
                        }
                        continue;
                    }
                    let Some(&argument) = argument_types.get(position) else { continue };
                    self.infer_from_types(
                        argument,
                        element,
                        &parameters,
                        &mut buckets[position],
                        0,
                    );
                }
                continue;
            }
            let Some(&argument_expression) = arguments.get(index) else { continue };
            if self.is_context_sensitive_argument(&argument_expression) {
                deferred.push(index);
                continue;
            }
            let Some(&argument) = argument_types.get(index) else { continue };
            let argument = if signature.type_parameters.iter().any(|parameter| parameter.is_const) {
                self.const_literal_inference_source(
                    argument_expression,
                    argument,
                    parameter.r#type,
                    false,
                )
            } else {
                argument
            };
            let mut existing: Vec<_> = buckets.iter().flatten().cloned().collect();
            for info in &return_mapper {
                if !existing.iter().any(|current| current.type_parameter == info.type_parameter) {
                    existing.push(info.clone());
                }
            }
            if !skip_context_sensitive
                && self.infer_higher_order_argument(
                    argument,
                    parameter.r#type,
                    returned,
                    (signature, &parameters, &existing),
                    &mut buckets[index],
                    &mut inferred_type_parameters,
                )
            {
                continue;
            }
            if !skip_context_sensitive
                && self.infer_generic_function_in_context(
                    argument,
                    parameter.r#type,
                    (signature, &parameters, &existing),
                    &mut buckets[index],
                )
            {
                continue;
            }
            self.infer_from_types(argument, parameter.r#type, &parameters, &mut buckets[index], 0);
        }
        if !deferred.is_empty()
            && let Some(call_id) = call
        {
            // inferFromIntraExpressionSites reads the candidate signature's
            // uninstantiated context. Without it, a member lookup takes the
            // stateless fallback and prematurely fills its parameters with unknown.
            let owns_memo = !self.call_inference_signatures.contains_key(&call_id);
            if owns_memo {
                self.call_inference_signatures.insert(call_id, signature.clone());
                if !skip_context_sensitive {
                    self.contextual_signature_mappers.remove(&call_id);
                }
            }
            // inferTypeArguments first checks the argument with
            // SkipContextSensitive: retain object data while callbacks are
            // anyFunctionType, before a contextual read fixes the candidates.
            for &index in &deferred {
                let Some(&argument) = arguments.get(index) else { continue };
                let Some(parameter) = signature.parameters.get(index) else { continue };
                if matches!(
                    argument,
                    Expression::ObjectLiteralExpression(_) | Expression::ArrayLiteralExpression(_)
                ) && let Some(source) = self.context_free_object_inference_type(argument)
                {
                    self.infer_from_types(
                        source,
                        parameter.r#type,
                        &parameters,
                        &mut buckets[index],
                        0,
                    );
                }
            }
            // SkipContextSensitive retains a return-only signature when the
            // function has no contextual parameters. Nested contextual
            // functions become anyFunctionType until the fixing pass.
            for &index in &deferred {
                let Some(mut argument) = arguments.get(index).copied() else { continue };
                while let Expression::ParenthesizedExpression(node) = argument {
                    let Some(inner) = node.expression else { break };
                    argument = inner;
                }
                if !matches!(
                    argument,
                    Expression::ArrowFunction(_) | Expression::FunctionExpression(_)
                ) {
                    continue;
                }
                let Some(declaration) = argument.node_id() else { continue };
                let Some(parameter) = signature.parameters.get(index) else { continue };
                let Some(contexts) = self.call_signatures_of_type(parameter.r#type) else {
                    continue;
                };
                let [context] = contexts.as_slice() else { continue };
                if self.mentions_type_parameter(context.r#type, &parameters, &names)
                    && let Some(source) = self.context_free_function_type(declaration)
                {
                    self.infer_from_types(
                        source,
                        parameter.r#type,
                        &parameters,
                        &mut buckets[index],
                        0,
                    );
                }
            }
            for bucket in buckets.drain(..) {
                for info in bucket {
                    merge_info(&mut infos, &info);
                }
            }
            if let Some(context) = self.active_inference_contexts.get_mut(&call_id) {
                context.inferences.clone_from(&infos);
            }
            if !skip_context_sensitive {
                let flags = if self.in_js_file(call_id) {
                    InferenceFlags::ANY_DEFAULT
                } else {
                    InferenceFlags::NONE
                };
                if owns_memo && (!inferred_type_parameters.is_empty() || !return_mapper.is_empty())
                {
                    self.higher_order_context_calls.insert(call_id);
                }
                if owns_memo {
                    for &index in &deferred {
                        if let Some(Expression::ObjectLiteralExpression(literal)) =
                            arguments.get(index)
                            && let Some(id) = literal.node_id
                        {
                            self.intra_expression_member_maps.remove(&id);
                        }
                    }
                }
                for &index in &deferred {
                    if let Some(context) = self.active_inference_contexts.get_mut(&call_id) {
                        context.inferences.clone_from(&infos);
                        context.inferential = owns_memo;
                    }
                    // SS135: the argument may have been checked EAGERLY during
                    // overload selection (calls.rs) with no memo present - the
                    // cached answers of its whole subtree are pre-context and
                    // must not survive into this contextual re-check (the
                    // summit's freeze pattern, third recurrence).
                    if let Some(id) = arguments[index].node_id() {
                        self.evict_subtree(id);
                    }
                    let checked = self.check_expression(arguments[index]);
                    if let Some(context) = self.active_inference_contexts.get_mut(&call_id) {
                        infos.clone_from(&context.inferences);
                        context.inferential = false;
                        context.intra_expression_sites.clear();
                    }

                    if let Some(slot) = argument_types.get_mut(index) {
                        *slot = checked;
                    }
                    if let Some(parameter) = signature.parameters.get(index) {
                        self.infer_from_types(
                            checked,
                            parameter.r#type,
                            &parameters,
                            &mut infos,
                            0,
                        );
                    }
                }
                if owns_memo {
                    self.call_inference_signatures.remove(&call_id);
                    self.higher_order_context_calls.remove(&call_id);
                    let Some(map) =
                        self.resolved_inference_map(&infos, signature, &parameters, flags)
                    else {
                        return decline;
                    };
                    self.contextual_signature_mappers.insert(
                        call_id,
                        (map, parameters.clone(), names.iter().map(ToString::to_string).collect()),
                    );
                }
            } else if owns_memo {
                self.call_inference_signatures.remove(&call_id);
            }
        } else {
            for bucket in buckets.drain(..) {
                for info in bucket {
                    merge_info(&mut infos, &info);
                }
            }
            for &index in &deferred {
                let checked = self.check_expression(arguments[index]);
                if let Some(slot) = argument_types.get_mut(index) {
                    *slot = checked;
                }
            }
        }
        let mut candidates = Vec::with_capacity(parameters.len());
        for (position, &type_parameter) in parameters.iter().enumerate() {
            let mut candidate = None;
            if let Some(info) = infos.iter().find(|info| info.type_parameter == type_parameter)
                && (info.has_candidates() || info.fixed_type.is_some())
            {
                let Some(resolved) =
                    self.unconstrained_inferred_type_from_info(info, signature, position, &infos)
                else {
                    return decline;
                };
                candidate = Some(resolved);
            }
            // `getWidenedType` (`checker.go:16090`): with `strictNullChecks`
            // **off** upstream widens a `null` or `undefined` inference to
            // `any`, and nothing on this port's inference path widens.
            // Measured, not assumed — `checker-notes-infer2.md` §2.2 records 19
            // own-node lines wanting `Promise<any>` where the unwidened
            // candidate prints `Promise<null>`.
            //
            // **The strictness test is load-bearing and was missing on the
            // first run**, which fired the bar's second leg: under
            // `strictNullChecks` upstream does *not* widen, so a `null`
            // candidate is the right answer and refusing it turned six right
            // lines in `strictNullChecksNoWidening` and
            // `undefinedInferentialTyping` into gaps. §5 of
            // `checker-notes-infer2.md` records the diagnosis.
            if let Some(inferred) = candidate
                && !self.strict_null_checks
                && matches!(self.type_to_string(inferred).as_str(), "null" | "undefined")
            {
                return decline;
            }
            match candidate {
                Some(inferred) if inferred != error => candidates.push((type_parameter, inferred)),
                Some(_) => return decline,
                None => {
                    // Structural inference is still incomplete. Keep its existing
                    // refusal when a supplied source could contain a candidate;
                    // absence from our collector does not prove native absence.
                    // Candidate-free parameters admitted here are resolved below
                    // with getInferredType's default and constraint mapper.
                    let name = names[position];
                    let structural_source_supplied =
                        signature.parameters.iter().enumerate().any(|(index, parameter)| {
                            // §401: a NULL/UNDEFINED argument yields no
                            // structural candidate upstream either — the
                            // fixture family is `utils.fold(null)` reading
                            // `unknown` (`genericFunctionsWithOptionalParameters1/2`),
                            // so such a position must not veto the fallback.
                            argument_types.get(index).is_some_and(|&argument| {
                                !self
                                    .type_of(argument)
                                    .flags
                                    .intersects(crate::flags::TypeFlags::NULLABLE)
                                    && self
                                        .intersection_inference_source(
                                            argument,
                                            parameter.r#type,
                                            &parameters,
                                        )
                                        .is_none_or(|(remaining, _)| remaining.is_some())
                            }) && self.mentions_type_parameter(
                                parameter.r#type,
                                &[type_parameter],
                                &[name],
                            ) && !argument_types.get(index).is_some_and(|&source| {
                                self.predicate_only_inference_has_no_source(
                                    source,
                                    parameter.r#type,
                                    type_parameter,
                                    name,
                                )
                            })
                        });
                    if structural_source_supplied
                        && !infos
                            .iter()
                            .any(|info| info.type_parameter == type_parameter && info.is_fixed)
                    {
                        return decline;
                    }
                }
            }
        }
        // Resolve constraints through the non-fixing mapper after collecting all
        // candidates. Its provisional entries break dependent-constraint cycles.
        let mut map: Vec<_> = infos
            .iter()
            .filter_map(|info| info.fixed_type.map(|t| (info.type_parameter, t)))
            .collect();
        let flags = if call.is_some_and(|id| self.in_js_file(id)) {
            InferenceFlags::ANY_DEFAULT
        } else {
            InferenceFlags::NONE
        };
        for position in 0..parameters.len() {
            if self.resolve_inference_with_constraints(
                (signature, &infos),
                position,
                &parameters,
                &candidates,
                &mut map,
                flags,
            ) == error
            {
                return decline;
            }
        }
        // inferSignatureInstantiationForOverloadFailure uses a fresh inference
        // context with SkipContextSensitive. A definite applicability failure
        // discards provisional inference from the normal pass. Already assigned
        // callback parameter types retain their fixed mapper entries.
        // Unknown relations cannot establish this error-recovery path.
        if !skip_context_sensitive
            && (arguments.iter().any(|argument| self.is_context_sensitive_argument(argument))
                || argument_types.iter().any(|argument| self.signature_types.get(argument).is_some_and(|signatures| matches!(signatures.as_slice(), [signature] if !signature.type_parameters.is_empty()))))
        {
            let arity_failed = if spread { false } else {
                let mut effective = signature.clone();
                for parameter in &mut effective.parameters {
                    parameter.r#type = self.instantiate_type(parameter.r#type, &map, &parameters, &names);
                }
                arguments.len() < self.signature_min_argument_count(&effective)
                    || (!self.signature_has_effective_rest(&effective) && arguments.len() > self.signature_parameter_count(&effective))
            };
            let failed = arity_failed ||
                argument_types.iter().zip(signature.parameters.iter().take_while(|p| !p.rest)).any(|(&argument, parameter)| {
                    let image = self.instantiate_type(parameter.r#type, &map, &parameters, &names);
                    argument != error
                        && image != error
                        && self.generic_argument_is_inapplicable(argument, image)
                });
            if failed {
                if let Some(call) = call {
                    let fixed_map = parameters.iter().map(|&parameter| {
                        let fixed = infos.iter().find(|info| info.type_parameter == parameter && info.is_fixed).and_then(|info| info.fixed_type);
                        (parameter, fixed.unwrap_or(parameter))
                    }).collect();
                    self.contextual_signature_mappers.insert(call, (fixed_map, parameters.clone(), names.iter().map(ToString::to_string).collect()));
                }
                return self.check_generic_call_with_mode(
                    signature,
                    call,
                    arguments,
                    instantiated,
                    true,
                );
            }
        }
        if overload_failure && let Some(call_id) = call {
            // The error signature is checked again after inference with
            // SkipContextSensitive. Its callback contexts use the recovered
            // type arguments, not the cached normal-pass fixing mapper.
            if let Some(context) =
                self.instantiate_signature(signature.clone(), &map, &parameters, &names)
            {
                let previous = self.call_inference_signatures.insert(call_id, context);
                for &argument in arguments {
                    if self.is_context_sensitive_argument(&argument) {
                        if let Some(id) = argument.node_id() {
                            self.evict_subtree(id);
                        }
                        self.check_expression(argument);
                    }
                }
                if let Some(previous) = previous {
                    self.call_inference_signatures.insert(call_id, previous);
                } else {
                    self.call_inference_signatures.remove(&call_id);
                }
            }
        }
        if let Some(slot) = instantiated {
            let returned_image = self.instantiate_type(returned, &map, &parameters, &names);
            let returned_image =
                self.propagate_return_type_parameters(returned_image, &inferred_type_parameters);
            if returned_image == error {
                return decline;
            }
            // getSignatureInstantiation erases the signature's own type
            // parameters before substituting its carried types. Reinstantiating
            // their constraints here can re-enter recursive alias resolution.
            let mut instance = signature.clone();
            instance.type_parameters.clear();
            let Some(mut instance) =
                self.instantiate_signature(instance, &map, &parameters, &names)
            else {
                // Unsupported parameter substitution does not invalidate an
                // independently resolved return, but cannot supply a candidate.
                return returned_image;
            };
            instance.r#type = returned_image;
            *slot = Some(instance);
            return returned_image;
        }
        let returned = self.instantiate_type(returned, &map, &parameters, &names);
        self.propagate_return_type_parameters(returned, &inferred_type_parameters)
    }

    /// The non-fixing resolution in `getInferredType` and `newBackreferenceMapper`
    /// (`internal/checker/inference.go`, `internal/checker/mapper.go`). The map
    /// also stores provisional results so recursive constraints see the native
    /// candidate/default/fallback rather than re-entering inference indefinitely.
    fn resolve_inference_with_constraints(
        &mut self,
        context: (&Signature, &[InferenceInfo]),
        position: usize,
        parameters: &[TypeId],
        candidates: &[(TypeId, TypeId)],
        map: &mut Vec<(TypeId, TypeId)>,
        flags: InferenceFlags,
    ) -> TypeId {
        use crate::relater::{Relation, Ternary};
        let (signature, infos) = context;
        let parameter = parameters[position];
        if let Some(&(_, inferred)) = map.iter().find(|&&(source, _)| source == parameter) {
            return inferred;
        }
        let info = infos.iter().find(|info| info.type_parameter == parameter);
        if let Some(fixed) = info.and_then(|info| info.fixed_type) {
            map.push((parameter, fixed));
            return fixed;
        }
        let candidate = if let Some(&(_, candidate)) =
            candidates.iter().find(|&&(source, _)| source == parameter)
        {
            Some(candidate)
        } else if let Some(info) = info.filter(|info| info.has_candidates()) {
            let inferred = self
                .unconstrained_inferred_type_from_info(info, signature, position, infos)
                .unwrap_or(self.intrinsics.error);
            if inferred == self.intrinsics.error {
                map.push((parameter, inferred));
                return inferred;
            }
            Some(inferred)
        } else {
            None
        };
        let no_default = flags.contains(InferenceFlags::NO_DEFAULT);
        let fallback = if no_default {
            self.get_silent_never_type()
        } else if flags.contains(InferenceFlags::ANY_DEFAULT) {
            self.intrinsics.any
        } else {
            self.intrinsics.unknown
        };
        let slot = map.len();
        let mut inferred = candidate.unwrap_or(fallback);
        map.push((parameter, inferred));
        let names: Vec<_> = signature.type_parameters.iter().map(|p| p.name.as_str()).collect();
        let declaration = &signature.type_parameters[position];
        if candidate.is_none()
            && !no_default
            && let Some(default) = declaration.default
        {
            // Defaults resolve earlier parameters through the non-fixing mapper;
            // self/forward references always map to unknown, even in JS files.
            let mut default_map = Vec::with_capacity(parameters.len());
            for (index, &source) in parameters.iter().enumerate() {
                let image = if index < position
                    && self.mentions_type_parameter(default, &[source], &[names[index]])
                {
                    self.resolve_inference_with_constraints(
                        context, index, parameters, candidates, map, flags,
                    )
                } else {
                    self.intrinsics.unknown
                };
                if image == self.intrinsics.error {
                    map[slot].1 = image;
                    return image;
                }
                default_map.push((source, image));
            }
            inferred = self.instantiate_type(default, &default_map, parameters, &names);
            map[slot].1 = inferred;
            if inferred == self.intrinsics.error {
                return inferred;
            }
        }
        if let Some(constraint) = declaration.constraint {
            for (index, &source) in parameters.iter().enumerate() {
                if self.mentions_type_parameter(constraint, &[source], &[names[index]])
                    && self.resolve_inference_with_constraints(
                        context, index, parameters, candidates, map, flags,
                    ) == self.intrinsics.error
                {
                    map[slot].1 = self.intrinsics.error;
                    return self.intrinsics.error;
                }
            }
            let constraint = self.instantiate_type(constraint, map, parameters, &names);
            if constraint == self.intrinsics.error {
                map[slot].1 = constraint;
                return constraint;
            }
            if candidate.is_some()
                && let Some(info) = infos.iter().find(|info| info.type_parameter == parameter)
            {
                inferred = self
                    .inferred_type_with_constraint(info, signature, position, inferred, constraint);
            } else if (!no_default && declaration.default.is_none())
                || self.relate_ternary(inferred, constraint, Relation::Assignable)
                    == Ternary::NotRelated
            {
                inferred = constraint;
            }
        }
        map[slot].1 = inferred;
        inferred
    }

    /// inferFromSignatures falls back to ordinary return types when predicates
    /// do not match. A parameter mentioned only in the target predicate then
    /// has no inference source, rather than an unsupported structural source.
    fn predicate_only_inference_has_no_source(
        &mut self,
        source: TypeId,
        target: TypeId,
        parameter: TypeId,
        name: &str,
    ) -> bool {
        let (Some(source), Some(target)) = (
            self.signature_types.get(&source).cloned(),
            self.signature_types.get(&target).cloned(),
        ) else {
            return false;
        };
        let ([source], [target]) = (source.as_slice(), target.as_slice()) else { return false };
        if target.predicate.is_none()
            || !source.type_parameters.is_empty()
            || !target.type_parameters.is_empty()
        {
            return false;
        }
        let (_, returned) = source.inference_return_types(target);
        !self.mentions_type_parameter(returned, &[parameter], &[name])
            && !target
                .parameters
                .iter()
                .any(|p| self.mentions_type_parameter(p.r#type, &[parameter], &[name]))
            && !target
                .this_parameter
                .as_ref()
                .is_some_and(|p| self.mentions_type_parameter(p.r#type, &[parameter], &[name]))
    }

    /// Ported from `getInferredType` (`internal/checker/inference.go`), selecting
    /// between the independently collected input and output candidates.
    fn inferred_type_from_info(
        &mut self,
        info: &InferenceInfo,
        signature: &Signature,
        position: usize,
        infos: &[InferenceInfo],
    ) -> Option<TypeId> {
        let parameters = self.type_parameter_types(signature)?;
        let mut context = infos.to_vec();
        if let Some(existing) =
            context.iter_mut().find(|existing| existing.type_parameter == info.type_parameter)
        {
            *existing = info.clone();
        } else {
            context.push(info.clone());
        }
        let inferred = self.resolve_inference_with_constraints(
            (signature, &context),
            position,
            &parameters,
            &[],
            &mut Vec::new(),
            InferenceFlags::NONE,
        );
        (inferred != self.intrinsics.error).then_some(inferred)
    }

    /// instantiateInstantiableTypes (internal/checker/checker.go). Object
    /// templates retain their parameters until a contextual member is read.
    pub(crate) fn instantiate_instantiable_types(
        &mut self,
        ty: TypeId,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> TypeId {
        if self.store.get(ty).flags.intersects(crate::flags::TypeFlags::INSTANTIABLE) {
            return self.instantiate_type(ty, map, parameters, names);
        }
        // Native aliases already carry their body's flags. Reference shells
        // here must expose a union/intersection or instantiable body before
        // this walk; object bodies still retain their uninstantiated template.
        // evaluate_alias_body retains its private Checker cache keyed by alias
        // SymbolId and ordered argument TypeIds; this read uses the caller's
        // mapper and existing depth guard, without publishing a new mapper or
        // member image. Body evaluation and its bindings remain with that owner.
        if let Some((symbol, arguments)) = self.type_reference_targets.get(&ty).cloned()
            && self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
            && self.instantiation_depth < 100
            && let Some(body) = self.evaluate_alias_body(symbol, &arguments)
            && body != ty
            && self.store.get(body).flags.intersects(
                crate::flags::TypeFlags::INSTANTIABLE
                    | crate::flags::TypeFlags::UNION
                    | crate::flags::TypeFlags::INTERSECTION,
            )
        {
            self.instantiation_depth += 1;
            let image = self.instantiate_instantiable_types(body, map, parameters, names);
            self.instantiation_depth -= 1;
            return image;
        }
        match self.store.get(ty).data.clone() {
            TypeData::Union { types, .. } => {
                let types: Vec<_> = types
                    .into_iter()
                    .map(|ty| self.instantiate_instantiable_types(ty, map, parameters, names))
                    .collect();
                self.get_union_type_without_reduction(&types)
            }
            TypeData::Intersection { types, symbol, .. } => {
                let types: Vec<_> = types
                    .into_iter()
                    .map(|ty| self.instantiate_instantiable_types(ty, map, parameters, names))
                    .collect();
                self.get_intersection_type(&types, symbol)
            }
            _ => ty,
        }
    }

    /// getInferredTypes/getMapperFromContext (internal/checker/inference.go).
    /// Fixed results are cache entries; other candidates resolve with the same
    /// recursive constraint/default mapper used by final call inference.
    pub(crate) fn resolved_inference_map(
        &mut self,
        infos: &[InferenceInfo],
        signature: &Signature,
        parameters: &[TypeId],
        flags: InferenceFlags,
    ) -> Option<Vec<(TypeId, TypeId)>> {
        let mut map = Vec::new();
        let mut ordered = Vec::with_capacity(parameters.len());
        for (position, &parameter) in parameters.iter().enumerate() {
            let inferred = self.resolve_inference_with_constraints(
                (signature, infos),
                position,
                parameters,
                &[],
                &mut map,
                flags,
            );
            if inferred == self.intrinsics.error {
                return None;
            }
            ordered.push((parameter, inferred));
        }
        Some(ordered)
    }

    /// getInferredType's constraint filter and alternate-variance fallback.
    /// The caller supplies a constraint instantiated by its inference mapper.
    fn inferred_type_with_constraint(
        &mut self,
        info: &InferenceInfo,
        signature: &Signature,
        position: usize,
        inferred: TypeId,
        constraint: TypeId,
    ) -> TypeId {
        use crate::relater::{Relation, Ternary};
        if self.relate_ternary(inferred, constraint, Relation::Assignable) != Ternary::NotRelated {
            return inferred;
        }
        // getInferredType (internal/checker/inference.go) filters pure return
        // speculation against the constraint before considering a fallback.
        if info.priority == InferencePriority::RETURN_TYPE {
            let constituents = match &self.store.get(inferred).data {
                TypeData::Union { types, .. } => types.clone(),
                _ => vec![inferred],
            };
            let filtered: Vec<_> = constituents
                .into_iter()
                .filter(|&t| {
                    self.relate_ternary(t, constraint, Relation::Assignable) != Ternary::NotRelated
                })
                .collect();
            if !filtered.is_empty() {
                return self.get_union_type(&filtered);
            }
        }
        let covariant = (!info.candidates.is_empty())
            .then(|| self.inferred_covariant_type(info, signature, position))
            .flatten();
        let contravariant = (!info.contra_candidates.is_empty())
            .then(|| self.inferred_contravariant_type(info))
            .flatten();
        let fallback = if covariant == Some(inferred) { contravariant } else { covariant };
        fallback
            .filter(|&t| {
                self.relate_ternary(t, constraint, Relation::Assignable) != Ternary::NotRelated
            })
            .unwrap_or(constraint)
    }

    fn unconstrained_inferred_type_from_info(
        &mut self,
        info: &InferenceInfo,
        signature: &Signature,
        position: usize,
        infos: &[InferenceInfo],
    ) -> Option<TypeId> {
        use crate::{
            flags::TypeFlags,
            relater::{Relation, Ternary},
        };
        if let Some(fixed) = info.fixed_type {
            return Some(fixed);
        }
        let covariant = if info.candidates.is_empty() {
            None
        } else {
            Some(self.inferred_covariant_type(info, signature, position)?)
        };
        let contravariant = if info.contra_candidates.is_empty() {
            None
        } else {
            Some(self.inferred_contravariant_type(info)?)
        };
        let (Some(covariant), Some(contravariant)) = (covariant, contravariant) else {
            return covariant.or(contravariant);
        };
        let mut accepts_covariant = false;
        let mut unknown = false;
        for &candidate in &info.contra_candidates {
            match self.relate_ternary(covariant, candidate, Relation::Assignable) {
                Ternary::Related => accepts_covariant = true,
                Ternary::Unknown => unknown = true,
                Ternary::NotRelated => {}
            }
        }
        if !accepts_covariant && unknown {
            return None;
        }
        let mut conflicting = false;
        for other in infos {
            if other.type_parameter != info.type_parameter
                && self.type_parameter_constraint(other.type_parameter) != Some(info.type_parameter)
            {
                continue;
            }
            for &candidate in &other.candidates {
                match self.relate_ternary(candidate, covariant, Relation::Assignable) {
                    Ternary::NotRelated => conflicting = true,
                    Ternary::Unknown => return None,
                    Ternary::Related => {}
                }
            }
        }
        Some(
            if !self.type_of(covariant).flags.intersects(TypeFlags::NEVER | TypeFlags::ANY)
                && accepts_covariant
                && !conflicting
            {
                covariant
            } else {
                contravariant
            },
        )
    }

    /// `getMapperFromContext(cloneInferenceContext(outer, NoDefault))`, used
    /// by `inferTypeArguments` (internal/checker/checker.go).
    fn instantiate_outer_inference_context(
        &mut self,
        t: TypeId,
        node: NodeId,
        no_default: bool,
    ) -> TypeId {
        let mut parent = self.nodes.parent(node);
        let (outer_call, context) = loop {
            let Some(node) = parent else { return t };
            if let Some(context) = self.active_inference_contexts.get(&node) {
                break (node, context.clone());
            }
            parent = self.nodes.parent(node);
        };
        let Some(parameters) = self.type_parameter_types(&context.signature) else { return t };
        let names: Vec<_> =
            context.signature.type_parameters.iter().map(|p| p.name.as_str()).collect();
        if !no_default && let Some(map) = &context.outer_return_map {
            return self.instantiate_type(t, map, &parameters, &names);
        }
        let selected: Vec<_> = parameters
            .iter()
            .filter_map(|&parameter| {
                let source_infos = if !no_default
                    && context.return_inferences.iter().any(|info| info.type_parameter == parameter)
                {
                    &context.return_inferences
                } else {
                    &context.inferences
                };
                source_infos.iter().find(|info| info.type_parameter == parameter).cloned()
            })
            .collect();
        let flags = context.flags
            | if no_default { InferenceFlags::NO_DEFAULT } else { InferenceFlags::NONE };
        let Some(map) =
            self.resolved_inference_map(&selected, &context.signature, &parameters, flags)
        else {
            return self.intrinsics.error;
        };
        if !no_default && let Some(context) = self.active_inference_contexts.get_mut(&outer_call) {
            context.outer_return_map = Some(map.clone());
        }
        self.instantiate_type(t, &map, &parameters, &names)
    }

    /// getInferenceContext searches the active contextual argument chain.
    pub(crate) fn live_inference_context(&self, node: NodeId) -> Option<NodeId> {
        let mut parent = self.nodes.parent(node);
        while let Some(node) = parent {
            if let Some(context) = self.active_inference_contexts.get(&node) {
                return context.inferential.then_some(node);
            }
            parent = self.nodes.parent(node);
        }
        None
    }

    /// addIntraExpressionInferenceSite records a completed context-sensitive
    /// member, never a callback whose body has not been checked yet.
    pub(crate) fn add_intra_expression_inference_site(&mut self, node: NodeId, ty: TypeId) {
        if let Some(call) = self.live_inference_context(node)
            && let Some(context) = self.active_inference_contexts.get_mut(&call)
        {
            context.intra_expression_sites.push((node, ty));
        }
    }

    pub(crate) fn infer_contextual_annotations(&mut self, node: NodeId, signature: &Signature) {
        let Some(call) = self.live_inference_context(node) else { return };
        let pairs = self.contextual_annotation_inferences(node, signature);
        if pairs.is_empty() {
            return;
        }
        let Some(mut context) = self.active_inference_contexts.get(&call).cloned() else { return };
        let Some(parameters) = self.type_parameter_types(&context.signature) else { return };
        for (source, target) in pairs {
            self.infer_from_types(source, target, &parameters, &mut context.inferences, 0);
        }
        if let Some(current) = self.active_inference_contexts.get_mut(&call) {
            current.inferences = context.inferences;
        }
    }

    /// InferenceTypeMapper.Map consumes completed sites before the first fixing
    /// read. Non-fixing reads recompute provisional results from current candidates.
    #[allow(clippy::type_complexity)]
    pub(crate) fn live_contextual_mapper(
        &mut self,
        node: NodeId,
        consumed: &[TypeId],
    ) -> Option<(Vec<(TypeId, TypeId)>, Vec<TypeId>, Vec<String>)> {
        let call = self.live_inference_context(node)?;
        let mut context = self.active_inference_contexts.get(&call)?.clone();
        let parameters = self.type_parameter_types(&context.signature)?;
        let names: Vec<_> =
            context.signature.type_parameters.iter().map(|p| p.name.clone()).collect();
        let fixing: Vec<_> = parameters
            .iter()
            .enumerate()
            .filter_map(|(index, &parameter)| {
                (!context
                    .inferences
                    .iter()
                    .any(|info| info.type_parameter == parameter && info.is_fixed)
                    && consumed.iter().any(|&ty| {
                        self.mentions_type_parameter(ty, &[parameter], &[names[index].as_str()])
                    }))
                .then_some(parameter)
            })
            .collect();
        if !fixing.is_empty() {
            let sites = std::mem::take(
                &mut self.active_inference_contexts.get_mut(&call)?.intra_expression_sites,
            );
            for (site, ty) in sites {
                let previous = self.contextual_prefers_uninstantiated;
                self.contextual_prefers_uninstantiated = true;
                let target = match self.node_map.get(site) {
                    Some(tsr_ast::Node::MethodDeclaration(method)) => {
                        self.contextual_type_for_object_literal_named_element(site, method.name)
                    }
                    _ => self.get_contextual_type(site),
                };
                self.contextual_prefers_uninstantiated = previous;
                if let Some(target) = target {
                    self.infer_from_types(ty, target, &parameters, &mut context.inferences, 0);
                }
            }
            for parameter in fixing {
                if let Some(info) =
                    context.inferences.iter_mut().find(|info| info.type_parameter == parameter)
                {
                    info.is_fixed = true;
                } else {
                    context.inferences.push(InferenceInfo {
                        type_parameter: parameter,
                        priority: InferencePriority::MAX_VALUE,
                        implied_arity: None,
                        candidates: Vec::new(),
                        contra_candidates: Vec::new(),
                        fixed_type: None,
                        is_fixed: true,
                        top_level: true,
                    });
                }
            }
        }
        let map = self.resolved_inference_map(
            &context.inferences,
            &context.signature,
            &parameters,
            context.flags,
        )?;
        for info in context.inferences.iter_mut().filter(|info| info.is_fixed) {
            info.fixed_type = map
                .iter()
                .find(|(parameter, _)| *parameter == info.type_parameter)
                .map(|(_, ty)| *ty);
        }
        self.active_inference_contexts.get_mut(&call)?.inferences = context.inferences;
        Some((map, parameters, names))
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn live_contextual_return_mapper(
        &mut self,
        node: NodeId,
    ) -> Option<(Vec<(TypeId, TypeId)>, Vec<TypeId>, Vec<String>)> {
        let call = self.live_inference_context(node)?;
        let context = self.active_inference_contexts.get(&call)?.clone();
        if context.return_inferences.is_empty() {
            return None;
        }
        let parameters = self.type_parameter_types(&context.signature)?;
        let names = context.signature.type_parameters.iter().map(|p| p.name.clone()).collect();
        let map = self.resolved_inference_map(
            &context.return_inferences,
            &context.signature,
            &parameters,
            context.flags,
        )?;
        Some((map, parameters, names))
    }

    /// Native silentNeverType marks an absent inference, distinct from never.
    pub(crate) fn get_silent_never_type(&mut self) -> TypeId {
        if let Some(silent) = self.silent_never_type {
            return silent;
        }
        let silent = self.store.new_named(crate::flags::TypeFlags::NEVER, "never".to_owned(), None);
        self.silent_never_type = Some(silent);
        silent
    }

    /// `ObjectFlagsNonInferrableType` propagation through instantiated type
    /// arguments (internal/checker/checker.go, inference.go). silentNever is
    /// an internal absence of inference, distinct from a real never candidate.
    fn contains_silent_never(&self, t: TypeId, seen: &mut Vec<TypeId>) -> bool {
        if Some(t) == self.silent_never_type {
            return true;
        }
        if seen.contains(&t) {
            return false;
        }
        seen.push(t);
        let contains = if let Some((_, arguments)) = self.type_reference_targets.get(&t) {
            arguments.iter().any(|&t| self.contains_silent_never(t, seen))
        } else if let Some((elements, _)) = self.tuple_element_lists.get(&t) {
            elements.iter().any(|&t| self.contains_silent_never(t, seen))
        } else if let Some((elements, _)) = self.variadic_tuple_elements.get(&t) {
            elements.iter().any(|element| self.contains_silent_never(element.r#type, seen))
        } else if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            &self.store.get(t).data
        {
            types.iter().any(|&t| self.contains_silent_never(t, seen))
        } else {
            false
        };
        seen.pop();
        contains
    }

    /// Ported from `getContravariantInference` and `getCommonSubtype`
    /// (`internal/checker/inference.go`). Priority-driven intersections remain
    /// separate from this ordinary candidate-selection path.
    fn inferred_contravariant_type(&mut self, info: &InferenceInfo) -> Option<TypeId> {
        use crate::relater::{Relation, Ternary};
        if info.priority.intersects(InferencePriority::IMPLIES_COMBINATION) {
            return Some(self.get_intersection_type(&info.contra_candidates, None));
        }
        let mut inferred = None;
        for &candidate in &info.contra_candidates {
            let Some(previous) = inferred else {
                inferred = Some(candidate);
                continue;
            };
            match self.relate_ternary(candidate, previous, Relation::Subtype) {
                Ternary::Related => inferred = Some(candidate),
                Ternary::Unknown => return None,
                Ternary::NotRelated => {}
            }
        }
        inferred
    }

    /// getCovariantInference (internal/checker/inference.go), shared by final
    /// type-argument resolution and a contextual fixing mapper. Literal candidates
    /// are combined before selecting and widening their common supertype.
    fn inferred_covariant_type(
        &mut self,
        info: &InferenceInfo,
        signature: &Signature,
        position: usize,
    ) -> Option<TypeId> {
        let candidates = self.union_object_and_array_literal_candidates(&info.candidates)?;
        let never = self.intrinsics.never;
        let has_other = candidates.iter().any(|&candidate| candidate != never);
        let primitive_constraint = self.parameter_has_primitive_constraint(signature, position)
            || signature.type_parameters.get(position).is_some_and(|parameter| parameter.is_const);
        let widen_literals = !primitive_constraint
            && info.top_level
            && (info.is_fixed
                || !self
                    .is_type_parameter_at_top_level_in_return_type(signature, info.type_parameter));
        let mut base = Vec::with_capacity(candidates.len());
        for &candidate in &candidates {
            if has_other && candidate == never {
                continue;
            }
            let candidate = if primitive_constraint {
                self.get_regular_type_of_literal_type(candidate)
            } else if widen_literals {
                self.get_widened_literal_type(candidate)
            } else {
                candidate
            };
            if !base.contains(&candidate) {
                base.push(candidate);
            }
        }
        let inferred = if let [single] = base.as_slice() {
            Some(*single)
        } else if info.priority.intersects(InferencePriority::IMPLIES_COMBINATION) {
            self.union_with_subtype_reduction(&base)
        } else {
            self.covariant_combination(&base)
        }?;
        // getCovariantInference ends with getWidenedType, even when literal
        // primitive candidates intentionally retain their precise types.
        Some(self.widen_object_literal_freshness(inferred))
    }

    /// The primitive argument and callback-return cases of
    /// getSignatureApplicabilityError (internal/checker/checker.go).
    /// Other structural relations require the remaining applicability walk.
    fn generic_argument_is_inapplicable(&mut self, source: TypeId, target: TypeId) -> bool {
        use crate::{
            flags::TypeFlags,
            relater::{Relation, Ternary},
        };
        let primitive_mismatch = |checker: &mut Self, source: TypeId, target: TypeId| {
            checker.store.get(source).flags.intersects(TypeFlags::PRIMITIVE)
                && checker.store.get(target).flags.intersects(TypeFlags::PRIMITIVE)
                && checker.relate_ternary(source, target, Relation::Assignable)
                    == Ternary::NotRelated
        };
        if primitive_mismatch(self, source, target) {
            return true;
        }
        let (Some(sources), Some(targets)) =
            (self.call_signatures_of_type(source), self.call_signatures_of_type(target))
        else {
            return false;
        };
        let ([source], [target]) = (sources.as_slice(), targets.as_slice()) else { return false };
        let source_required = source.parameters.iter().filter(|p| !p.optional && !p.rest).count();
        let target_parameters = self.signature_tuple_arguments(target);
        if !target_parameters.iter().any(|p| p.spread) && source_required > target_parameters.len()
        {
            return true;
        }
        if !source.type_parameters.is_empty() && target.type_parameters.is_empty() {
            return self.compare_signature_ternary(source, target) == Some(Ternary::NotRelated);
        }
        source.type_parameters.is_empty()
            && target.type_parameters.is_empty()
            && target.r#type != self.intrinsics.void
            && primitive_mismatch(self, source.r#type, target.r#type)
    }

    /// The `SkipGenericFunctions` branch of
    /// `instantiateTypeWithSingleGenericCallSignature` (checker.go:7599).
    /// A generic value is skipped only when both sides have one signature of
    /// the same kind and the contextual signature is non-generic.
    fn overload_failure_skips_generic_argument(&mut self, source: TypeId, target: TypeId) -> bool {
        let Some(source_signature) = self.single_call_or_construct_signature(source, true) else {
            return false;
        };
        if source_signature.type_parameters.is_empty() {
            return false;
        }
        let target = self.get_non_nullable_type(target);
        let Some(target_signature) = self.single_call_or_construct_signature(target, false) else {
            return false;
        };
        source_signature.kind == target_signature.kind
            && target_signature.type_parameters.is_empty()
    }

    /// `isAritySmaller` and `inferSignatureInstantiationForOverloadFailure`
    /// (checker.go), the decisive fixed callback-arity failure.
    fn generic_callback_arity_failure(
        &mut self,
        signature: &Signature,
        arguments: &[Expression<'_>],
    ) -> bool {
        if signature.parameters.iter().any(|p| p.rest) {
            return false;
        }
        for (argument, parameter) in arguments.iter().zip(&signature.parameters) {
            if !self.is_context_sensitive_argument(argument) {
                continue;
            }
            let source_parameters = match argument {
                Expression::ArrowFunction(f) => f.parameters,
                Expression::FunctionExpression(f) => f.parameters,
                _ => continue,
            };
            let required = source_parameters
                .iter()
                .filter(|p| !Self::is_this_parameter_declaration(p))
                .take_while(|p| {
                    p.initializer.is_none()
                        && p.question_token.is_none()
                        && p.dot_dot_dot_token.is_none()
                })
                .count();
            let Some(targets) = self.call_signatures_of_type(parameter.r#type) else { continue };
            let [target] = targets.as_slice() else { continue };
            if !target.parameters.iter().any(|p| p.rest) && target.parameters.len() < required {
                return true;
            }
        }
        false
    }

    /// The propagation branch of `instantiateTypeWithSingleGenericCallSignature`
    /// (`internal/checker/checker.go`). Adopt fresh candidates only when
    /// parameter inference contributes and does not overlap existing inferences.
    fn infer_higher_order_argument(
        &mut self,
        source: TypeId,
        target: TypeId,
        returned: TypeId,
        context: (&Signature, &[TypeId], &[InferenceInfo]),
        out: &mut Vec<InferenceInfo>,
        propagated: &mut Vec<crate::signatures::TypeParameter>,
    ) -> bool {
        let (_, parameters, existing) = context;
        let Some(return_signature) = self.single_call_or_construct_signature(returned, false)
        else {
            return false;
        };
        if !return_signature.type_parameters.is_empty()
            || parameters.iter().all(|parameter| {
                existing
                    .iter()
                    .any(|info| info.type_parameter == *parameter && info.has_candidates())
            })
        {
            return false;
        }
        let target = self.get_non_nullable_type(target);
        let (Some(source_signature), Some(target_signature)) = (
            self.single_call_or_construct_signature(source, true),
            self.single_call_or_construct_signature(target, false),
        ) else {
            return false;
        };
        if source_signature.type_parameters.is_empty()
            || !target_signature.type_parameters.is_empty()
        {
            return false;
        }
        if matches!(source_signature.kind, crate::signatures::SignatureKind::Call)
            != matches!(target_signature.kind, crate::signatures::SignatureKind::Call)
        {
            return false;
        }
        let Some(own) = self.type_parameter_types(&source_signature) else { return false };
        if own.len() != source_signature.type_parameters.len() {
            return false;
        }
        let mut used: Vec<_> = propagated.iter().map(|p| p.name.clone()).collect();
        let mut unique = source_signature.type_parameters.clone();
        let mut map = Vec::new();
        for (parameter, &original) in unique.iter_mut().zip(&own) {
            let mut image = original;
            if used.contains(&parameter.name) {
                let base = parameter.name.trim_end_matches(|c: char| c.is_ascii_digit());
                let base = if base.is_empty() { &parameter.name } else { base };
                let mut index = 1;
                let mut name = format!("{base}{index}");
                while used.contains(&name) {
                    index += 1;
                    name = format!("{base}{index}");
                }
                image = self.store.new_named(
                    crate::flags::TypeFlags::TYPE_PARAMETER,
                    name.clone(),
                    None,
                );
                if let Some(&symbol) = self.type_parameter_symbols.get(&original) {
                    self.type_parameter_symbols.insert(image, symbol);
                }
                parameter.name = name;
            }
            parameter.resolved_type = Some(image);
            used.push(parameter.name.clone());
            map.push((original, image));
        }
        let source_names: Vec<_> =
            source_signature.type_parameters.iter().map(|p| p.name.clone()).collect();
        for &(original, image) in &map {
            if original != image {
                self.instantiated_type_parameters.insert(
                    image,
                    InstantiatedTypeParameter {
                        target: original,
                        map: map.clone(),
                        parameters: own.clone(),
                        names: source_names.clone(),
                    },
                );
            }
        }
        let names: Vec<_> =
            source_signature.type_parameters.iter().map(|p| p.name.as_str()).collect();
        let Some(mut instantiated) =
            self.instantiate_signature(source_signature.clone(), &map, &own, &names)
        else {
            return false;
        };
        for (parameter, unique) in instantiated.type_parameters.iter_mut().zip(&unique) {
            parameter.name.clone_from(&unique.name);
        }
        let mut candidates = Vec::new();
        let saved = (self.inference_contravariant, self.inference_bivariant);
        self.inference_contravariant = true;
        self.inference_bivariant = false;
        self.apply_to_parameter_types(
            &instantiated,
            &target_signature,
            None,
            parameters,
            &mut candidates,
            0,
        );
        (self.inference_contravariant, self.inference_bivariant) = saved;
        if candidates.iter().all(|info| !info.has_candidates()) {
            return false;
        }
        let (source_return, target_return) = instantiated.inference_return_types(&target_signature);
        self.infer_from_types(source_return, target_return, parameters, &mut candidates, 0);
        if candidates.iter().any(|candidate| {
            existing.iter().any(|info| {
                candidate.type_parameter == info.type_parameter && info.has_candidates()
            })
        }) {
            return false;
        }
        for candidate in &candidates {
            merge_info(out, candidate);
        }
        propagated.extend(instantiated.type_parameters);
        true
    }

    /// The fallback of `instantiateTypeWithSingleGenericCallSignature`:
    /// `instantiateSignatureInContextOf` (checker.go) infers the argument's
    /// own parameters from the surrounding signature under the current mapper.
    fn infer_generic_function_in_context(
        &mut self,
        source: TypeId,
        target: TypeId,
        context: (&Signature, &[TypeId], &[InferenceInfo]),
        out: &mut Vec<InferenceInfo>,
    ) -> bool {
        let (outer_signature, parameters, existing) = context;
        let target = self.get_non_nullable_type(target);
        let (Some(source_signature), Some(target_signature)) = (
            self.single_call_or_construct_signature(source, true),
            self.single_call_or_construct_signature(target, false),
        ) else {
            return false;
        };
        if source_signature.type_parameters.is_empty()
            || !target_signature.type_parameters.is_empty()
            || matches!(source_signature.kind, crate::signatures::SignatureKind::Call)
                != matches!(target_signature.kind, crate::signatures::SignatureKind::Call)
        {
            return false;
        }
        let mut source_signature = source_signature;
        let source_required =
            source_signature.parameters.iter().filter(|p| !p.optional && !p.rest).count();
        if !target_signature.parameters.iter().any(|p| p.rest)
            && source_required > target_signature.parameters.len()
        {
            return false;
        }
        let Some(own) = self.type_parameter_types(&source_signature) else { return false };
        let outer_names: Vec<_> = parameters.iter().map(|&p| self.type_to_string(p)).collect();
        let outer_names: Vec<_> = outer_names.iter().map(String::as_str).collect();
        // instantiateSignatureInContextOf uses a non-fixing mapper when
        // the contextual rest type is a type parameter. Every other input
        // mapper read fixes the consumed outer inference before owned inference.
        let non_fixing = target_signature.parameters.last().is_some_and(|parameter| {
            parameter.rest
                && self
                    .type_of(parameter.r#type)
                    .flags
                    .contains(crate::flags::TypeFlags::TYPE_PARAMETER)
        });
        let mut fixed_updates = Vec::new();
        let mut outer_map = Vec::with_capacity(parameters.len());
        for (position, &parameter) in parameters.iter().enumerate() {
            let consumed = target_signature
                .parameters
                .iter()
                .any(|input| self.mentions_type_parameter(input.r#type, &[parameter], &[]))
                || target_signature.this_parameter.as_ref().is_some_and(|input| {
                    self.mentions_type_parameter(input.r#type, &[parameter], &[])
                });
            let mut info =
                existing.iter().find(|info| info.type_parameter == parameter).cloned().unwrap_or(
                    InferenceInfo {
                        type_parameter: parameter,
                        priority: InferencePriority::MAX_VALUE,
                        implied_arity: None,
                        candidates: Vec::new(),
                        contra_candidates: Vec::new(),
                        fixed_type: None,
                        is_fixed: false,
                        top_level: true,
                    },
                );
            if consumed && !non_fixing {
                info.is_fixed = true;
            }
            let image = if info.has_candidates() || info.fixed_type.is_some() {
                let Some(image) =
                    self.inferred_type_from_info(&info, outer_signature, position, existing)
                else {
                    return false;
                };
                image
            } else {
                outer_signature
                    .type_parameters
                    .get(position)
                    .and_then(|parameter| parameter.default.or(parameter.constraint))
                    .unwrap_or(self.intrinsics.unknown)
            };
            outer_map.push((parameter, image));
            if consumed && !non_fixing {
                info.fixed_type = Some(image);
                fixed_updates.push(info);
            }
        }
        let Some(contextual) = self.instantiate_signature(
            target_signature.clone(),
            &outer_map,
            parameters,
            &outer_names,
        ) else {
            return false;
        };
        let mut inferences = Vec::new();
        self.apply_to_parameter_types(
            &contextual,
            &source_signature,
            None,
            &own,
            &mut inferences,
            0,
        );
        let own_names: Vec<_> =
            source_signature.type_parameters.iter().map(|p| p.name.clone()).collect();
        let own_names: Vec<_> = own_names.iter().map(String::as_str).collect();
        let mut own_map = Vec::with_capacity(own.len());
        for (position, (&parameter, declaration)) in
            own.iter().zip(&source_signature.type_parameters).enumerate()
        {
            let info = inferences.iter().find(|info| info.type_parameter == parameter);
            let image = if let Some(info) = info
                && info.has_candidates()
            {
                let Some(image) =
                    self.inferred_type_from_info(info, &source_signature, position, &inferences)
                else {
                    return false;
                };
                image
            } else {
                declaration.default.or(declaration.constraint).unwrap_or(self.intrinsics.unknown)
            };
            // Contextual signature instantiation infers covariantly from
            // inputs. Conflicting inputs retain the error-recovery path.
            if info
                .into_iter()
                .flat_map(|info| info.candidates.iter().chain(&info.contra_candidates))
                .any(|&candidate| {
                    self.relate_ternary(candidate, image, crate::relater::Relation::Assignable)
                        == crate::relater::Ternary::NotRelated
                })
            {
                return false;
            }
            own_map.push((parameter, image));
        }
        for (index, declaration) in source_signature.type_parameters.iter().enumerate() {
            if let Some(constraint) = declaration.constraint {
                let constraint = self.instantiate_type(constraint, &own_map, &own, &own_names);
                if constraint == self.intrinsics.error {
                    return false;
                }
                let checked =
                    self.instantiate_type(own_map[index].1, &outer_map, parameters, &outer_names);
                if self.relate_ternary(checked, constraint, crate::relater::Relation::Assignable)
                    == crate::relater::Ternary::NotRelated
                {
                    own_map[index].1 = constraint;
                }
            }
        }
        source_signature.type_parameters.clear();
        let Some(instantiated) =
            self.instantiate_signature(source_signature, &own_map, &own, &own_names)
        else {
            return false;
        };
        for info in fixed_updates {
            merge_info(out, &info);
        }
        self.infer_from_signature_parameters(
            &instantiated,
            &target_signature,
            target,
            parameters,
            out,
            0,
        );
        let (source_return, target_return) = instantiated.inference_return_types(&target_signature);
        self.infer_from_types(source_return, target_return, parameters, out, 0);
        true
    }

    /// getCanonicalSignature / createCanonicalSignature (checker.go). Reuse
    /// original unconstrained identities when comparing generic signatures.
    pub(crate) fn canonical_signature(&mut self, mut signature: Signature) -> Option<Signature> {
        if signature.type_parameters.is_empty() {
            return Some(signature);
        }
        let parameters = self.type_parameter_types(&signature)?;
        let owned_names: Vec<_> =
            signature.type_parameters.iter().map(|p| p.name.clone()).collect();
        let names: Vec<_> = owned_names.iter().map(String::as_str).collect();
        let mut map = Vec::with_capacity(parameters.len());
        for &parameter in &parameters {
            let target = self.instantiated_type_parameters.get(&parameter).map(|p| p.target);
            let image = match target {
                Some(target) if self.type_parameter_constraint(target).is_none() => target,
                _ => parameter,
            };
            map.push((parameter, image));
        }
        signature.type_parameters.clear();
        self.instantiate_signature(signature, &map, &parameters, &names)
    }

    /// `instantiateSignatureInContextOf` without an outer inference context,
    /// used by `compareSignaturesRelated`. Inputs have ordinary priority;
    /// return types supply lower-priority candidates for remaining parameters.
    pub(crate) fn instantiate_signature_in_context(
        &mut self,
        signature: Signature,
        contextual: &Signature,
    ) -> Option<Signature> {
        let missing = self.intrinsics.error;
        let types = |signature: &Signature| {
            [
                signature.r#type,
                signature.this_parameter.as_ref().map_or(missing, |parameter| parameter.r#type),
                signature
                    .predicate
                    .as_ref()
                    .and_then(|predicate| predicate.r#type)
                    .unwrap_or(missing),
            ]
            .into_iter()
            .chain(signature.parameters.iter().map(|parameter| parameter.r#type))
            .chain(signature.type_parameters.iter().flat_map(|parameter| {
                [parameter.constraint.unwrap_or(missing), parameter.default.unwrap_or(missing)]
            }))
            .collect()
        };
        let key = SignatureContextKey {
            source: signature.declaration,
            target: contextual.declaration,
            source_parameters: self.type_parameter_types(&signature)?,
            target_parameters: self.type_parameter_types(contextual)?,
            source_types: types(&signature),
            target_types: types(contextual),
            source_original_inputs: signature
                .target
                .iter()
                .flat_map(|signature| signature.parameters.iter().map(|parameter| parameter.r#type))
                .collect(),
            target_original_inputs: contextual
                .target
                .iter()
                .flat_map(|signature| signature.parameters.iter().map(|parameter| parameter.r#type))
                .collect(),
        };
        if let Some(cached) = self.signature_context_cache.get(&key) {
            return Some(cached.clone());
        }
        if !self.signature_context_in_progress.insert(key.clone()) {
            return None;
        }
        let result = self.instantiate_signature_in_context_worker(signature, contextual);
        self.signature_context_in_progress.remove(&key);
        if let Some(result) = &result {
            self.signature_context_cache.insert(key, result.clone());
        }
        result
    }

    fn instantiate_signature_in_context_worker(
        &mut self,
        mut signature: Signature,
        contextual: &Signature,
    ) -> Option<Signature> {
        let own = self.type_parameter_types(&signature)?;
        let saved =
            (self.inference_contravariant, self.inference_bivariant, self.inference_priority);
        self.inference_contravariant = false;
        self.inference_bivariant = false;
        self.inference_priority = InferencePriority::NONE;
        let saved_observed = self.inference_observed_priority;
        self.inference_observed_priority = i32::from(InferencePriority::MAX_VALUE.bits());
        let mut inferences = Vec::new();
        self.apply_to_parameter_types(contextual, &signature, None, &own, &mut inferences, 0);
        let (source_return, target_return) = contextual.inference_return_types(&signature);
        self.infer_from_types_with_priority(
            source_return,
            target_return,
            &own,
            &mut inferences,
            0,
            InferencePriority::RETURN_TYPE,
        );
        (self.inference_contravariant, self.inference_bivariant, self.inference_priority) = saved;
        self.inference_observed_priority = saved_observed;
        let owned_names: Vec<_> =
            signature.type_parameters.iter().map(|parameter| parameter.name.clone()).collect();
        let names: Vec<_> = owned_names.iter().map(String::as_str).collect();
        let mut map = Vec::new();
        for (position, (&parameter, declaration)) in
            own.iter().zip(&signature.type_parameters).enumerate()
        {
            let inferred = if let Some(info) = inferences
                .iter()
                .find(|info| info.type_parameter == parameter && info.has_candidates())
            {
                self.inferred_type_from_info(info, &signature, position, &inferences)?
            } else {
                declaration.default.or(declaration.constraint).unwrap_or(self.intrinsics.unknown)
            };
            map.push((parameter, inferred));
        }
        for (index, declaration) in signature.type_parameters.iter().enumerate() {
            if let Some(constraint) = declaration.constraint {
                let constraint = self.instantiate_type(constraint, &map, &own, &names);
                if self.is_error(constraint) {
                    return None;
                }
                if self.relate_ternary(
                    map[index].1,
                    constraint,
                    crate::relater::Relation::Assignable,
                ) == crate::relater::Ternary::NotRelated
                {
                    map[index].1 = constraint;
                }
            }
        }
        signature.type_parameters.clear();
        self.instantiate_signature(signature, &map, &own, &names)
    }

    /// `getSignatureInstantiation` adds propagated parameters to its single
    /// returned function signature (`internal/checker/checker.go`).
    fn propagate_return_type_parameters(
        &mut self,
        returned: TypeId,
        parameters: &[crate::signatures::TypeParameter],
    ) -> TypeId {
        if parameters.is_empty() || self.is_error(returned) {
            return returned;
        }
        let Some(signatures) = self.signature_types.get(&returned).cloned() else {
            return returned;
        };
        let [signature] = signatures.as_slice() else { return returned };
        // getOrCreateTypeFromSignature (checker.go:19370) creates a fresh
        // isolated signature type, including when the return had an alias name.
        let (TypeData::Anonymous { symbol, .. } | TypeData::Named { members: Some(symbol), .. }) =
            self.store.get(returned).data
        else {
            return returned;
        };
        let mut signature = signature.clone();
        signature.type_parameters = parameters.to_vec();
        let text = self.signature_to_string(&signature);
        let id = self.store.new_anonymous(crate::flags::TypeFlags::OBJECT, text, symbol, true);
        self.signature_types.insert(id, vec![signature]);
        self.minted_signature_types.insert(id);
        id
    }

    /// `inferFromTypes` (`inference.go:1236`) — walk a *source* type against a
    /// *target* type, pushing `(type parameter, candidate)` for every match.
    ///
    /// # Why only four arms
    ///
    /// Upstream's walk reads structure straight off the type. Here a type's
    /// payload is a **printed string** ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)
    /// and what followed from it), so a shape is decomposable only where a side
    /// table already records how it was built. Two exist, and both were written
    /// for *substitution* — the opposite direction:
    /// [`Checker::type_reference_targets`] (`bd tsr-4qx`) and
    /// [`Checker::signature_types`] (`bd tsr-0hc`). That is the whole budget,
    /// and it is a large one: `T[]` **is** `Array<T>` through
    /// [`Checker::create_type_reference`], and an array literal's type is built
    /// by the same function, so `T[]` against `number[]`, `Promise<T>` against
    /// `Promise<string>` and `C<T>` against `C<X>` are one arm.
    ///
    /// 1. **Identity** (`inference.go:1236`) — the target *is* a type
    ///    parameter; the candidate is the source. This is the rule that shipped
    ///    before `bd tsr-g30h`.
    /// 2. **`inferFromTypeArguments`** (`inference.go:1046`) — two references
    ///    to the same target symbol with equal argument counts, argument for
    ///    argument. Measured contravariant positions reverse the direction;
    ///    unmeasured reference positions retain the existing covariant walk. Each position
    ///    here is walked covariantly, which is safe only because a
    ///    disagreement between two positions gaps the whole call rather than
    ///    picking one.
    /// 3. **`inferToMultipleTypes`** (`inference.go:700`), restricted — the
    ///    target is a union, the source is not, at most one constituent *is* a
    ///    type parameter, and no reference constituent faces a reference source
    ///    of a different symbol. The source is inferred into every constituent
    ///    and the ones that cannot match contribute nothing. `p.then(f)` is the
    ///    head case: `then`'s parameter is
    ///    `((value: T) => …) | null | undefined`.
    /// 4. **`inferFromSignature`** (`inference.go:1112`) — one call signature
    ///    each, neither generic, no rest parameters, positions paired up to the
    ///    shorter list (`applyToParameterTypes`, `inference.go:1140`, so
    ///    `p.then(() => 1)` pairs against `(value: T) => …`), then the return
    ///    types.
    ///
    /// # The two restrictions that are refusals, not omissions
    ///
    /// Arm 3 refuses a **union source** and a **second naked type variable**
    /// because that is `inferToMultipleTypes`' branch that strikes the matched
    /// constituents and re-unions the remainder — it needs
    /// [`Checker::get_union_type`] plus a subtype decision this port does not
    /// have (`removeSubtypes`, refused at `bd tsr-eak`).
    ///
    /// Arm 3 also refuses a reference source facing a reference constituent of
    /// a **different symbol**. `Promise<void>` against
    /// `TResult1 | PromiseLike<TResult1>`: upstream matches the reference
    /// constituent through `Promise`'s base type and infers `void`; with no
    /// base-type walk here the naked variable would swallow the whole thing and
    /// print `Promise<Promise<void>>`. Measured, not assumed —
    /// `docs/architecture/checker-notes-infer2.md` §2.2 records that this one
    /// guard moved the counterfactual from 161 converted / 59 wrong to 157 / 35.
    ///
    /// Everything else — object-type members, index signatures, tuples, mapped
    /// and conditional types, `keyof`, intersections — contributes **no
    /// candidate**, which leaves its type parameter unmapped, which gaps the
    /// whole call. The largest such family is object members
    /// (`{ keys: T[] }` against `{ keys: string[] }`) and it needs a members
    /// reverse index that does not exist.
    pub(crate) fn infer_from_types(
        &mut self,
        source: TypeId,
        target: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) {
        self.infer_from_types_with_priority(
            source,
            target,
            parameters,
            out,
            depth,
            InferencePriority::NONE,
        );
    }

    /// Ported from `inferTypes` (`internal/checker/inference.go`).
    fn infer_from_types_with_priority(
        &mut self,
        source: TypeId,
        target: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
        priority: InferencePriority,
    ) {
        // Every entry walk's ORIGINAL target is the target it starts from;
        // the recursion below preserves it so the top-level test
        // (`inference.go:207-208`) reads the whole parameter type, not the
        // constituent the walk has descended to.
        let saved =
            (self.inference_contravariant, self.inference_bivariant, self.inference_priority);
        self.inference_contravariant = false;
        self.inference_bivariant = false;
        self.inference_priority = priority;
        let saved_observed = self.inference_observed_priority;
        self.inference_observed_priority = i32::from(InferencePriority::MAX_VALUE.bits());
        let saved_pairs = std::mem::take(&mut self.inference_visited_pairs);
        let saved_source = std::mem::take(&mut self.inference_source_stack);
        let saved_target = std::mem::take(&mut self.inference_target_stack);
        let saved_expanding = std::mem::take(&mut self.inference_expanding);
        self.infer_from_types_within(source, target, target, parameters, out, depth);
        self.inference_visited_pairs = saved_pairs;
        self.inference_source_stack = saved_source;
        self.inference_target_stack = saved_target;
        self.inference_expanding = saved_expanding;
        self.inference_observed_priority = saved_observed;
        (self.inference_contravariant, self.inference_bivariant, self.inference_priority) = saved;
    }

    /// Tuple element arguments and flags for inferFromObjectTypes.
    fn inference_tuple_elements(&mut self, id: TypeId) -> Option<Vec<crate::tuples::TupleElement>> {
        if let Some((elements, _)) = self.variadic_tuple_elements.get(&id) {
            return Some(elements.clone());
        }
        let (types, _) = self.tuple_element_lists.get(&id)?.clone();
        let mask = self.tuple_optional_masks.get(&id).cloned();
        let labels = self.tuple_labels.get(&id).cloned();
        Some(
            types
                .into_iter()
                .enumerate()
                .map(|(i, mut ty)| {
                    let optional =
                        mask.as_ref().and_then(|mask| mask.get(i)).copied().unwrap_or(false);
                    if optional && self.strict_null_checks && !self.exact_optional_property_types {
                        ty = self.get_union_type(&[ty, self.intrinsics.undefined]);
                    }
                    crate::tuples::TupleElement {
                        r#type: ty,
                        spread: false,
                        optional,
                        label: labels.as_ref().and_then(|labels| labels.get(i)).cloned().flatten(),
                    }
                })
                .collect(),
        )
    }

    /// getElementTypeOfSliceOfTupleType (checker.go:24830). A variadic slot
    /// contributes its numeric indexed access; a rest slot its array element.
    fn inference_tuple_slice_element(
        &mut self,
        elements: &[crate::tuples::TupleElement],
    ) -> Option<TypeId> {
        if elements.is_empty() {
            return None;
        }
        let mut types = Vec::with_capacity(elements.len());
        for element in elements {
            let ty = if element.spread {
                self.tuple_index_type(element.r#type, self.intrinsics.number, false)?
            } else {
                element.r#type
            };
            types.push(ty);
        }
        Some(self.get_union_type(&types))
    }

    /// Fixed tuple base constraints used by the adjacent variadic/rest rules.
    fn inference_fixed_tuple_arity(&mut self, parameter: TypeId) -> Option<usize> {
        let mut constraint = parameter;
        let mut seen = Vec::new();
        while self.store.get(constraint).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER) {
            if seen.contains(&constraint) {
                return None;
            }
            seen.push(constraint);
            constraint = self.type_parameter_constraint(constraint)?;
        }
        self.tuple_element_lists.get(&constraint).map(|(types, _)| types.len())
    }

    /// Ported from inferFromObjectTypes (internal/checker/inference.go:714-809).
    /// Rest slots are represented here by their array operand; upstream stores
    /// the element argument instead. Convert at the inference boundary.
    fn infer_from_tuple_types(
        &mut self,
        source: TypeId,
        target: TypeId,
        original: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) -> bool {
        let Some(targets) = self.inference_tuple_elements(target) else {
            // inferFromObjectTypes delegates tuple-to-array inference to its
            // numeric index signatures. Tuple metadata supplies that index.
            if let Some(sources) = self.inference_tuple_elements(source)
                && !self.store.get(target).flags.contains(crate::flags::TypeFlags::ANY)
                && let Some(element) = self.tuple_spread_array_element(target)
            {
                let source_element = if sources.is_empty() {
                    self.intrinsics.never
                } else if let Some(element) = self.inference_tuple_slice_element(&sources) {
                    element
                } else {
                    return true;
                };
                self.infer_from_types_within(
                    source_element,
                    element,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
                return true;
            }
            return false;
        };
        // inferFromTypes visits each source union constituent before entering
        // inferFromObjectTypes. Tuple metadata belongs to those constituents.
        if let TypeData::Union { types, .. } = &self.store.get(source).data {
            for constituent in types.clone() {
                self.infer_from_types_within(
                    constituent,
                    target,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
            }
            return true;
        }
        let source_elements = self.inference_tuple_elements(source);
        let source_array = if source_elements.is_none()
            && !self.store.get(source).flags.contains(crate::flags::TypeFlags::ANY)
        {
            self.tuple_spread_array_element(source)
        } else {
            None
        };
        if source_elements.is_none() && source_array.is_none() {
            return false;
        }
        let target_rest: Vec<_> = targets
            .iter()
            .map(|e| if e.spread { self.tuple_spread_array_element(e.r#type) } else { None })
            .collect();
        let sources = source_elements.unwrap_or_default();
        let source_rest: Vec<_> = sources
            .iter()
            .map(|e| if e.spread { self.tuple_spread_array_element(e.r#type) } else { None })
            .collect();
        // tupleTypesDefinitelyUnrelated rejects incompatible tuple arities
        // before element inference; array sources have no tuple arity.
        if source_array.is_none() {
            let target_variadic =
                targets.iter().enumerate().any(|(i, e)| e.spread && target_rest[i].is_none());
            let target_variable = targets.iter().any(|e| e.spread);
            let source_variable = sources.iter().any(|e| e.spread);
            let target_min = targets
                .iter()
                .enumerate()
                .filter(|(i, e)| !e.optional && (!e.spread || target_rest[*i].is_none()))
                .count();
            let source_min = sources
                .iter()
                .enumerate()
                .filter(|(i, e)| !e.optional && (!e.spread || source_rest[*i].is_none()))
                .count();
            let source_fixed = sources.iter().take_while(|e| !e.spread).count();
            if (!target_variadic && target_min > source_min)
                || (!target_variable && (source_variable || targets.len() < source_fixed))
            {
                return true;
            }
        }
        if source_array.is_none()
            && sources.len() == targets.len()
            && sources.iter().zip(&targets).enumerate().all(|(i, (s, t))| {
                s.spread == t.spread
                    && s.optional == t.optional
                    && source_rest[i].is_some() == target_rest[i].is_some()
            })
        {
            for (source, target) in sources.iter().zip(&targets) {
                self.infer_from_types_within(
                    source.r#type,
                    target.r#type,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
            }
            return true;
        }
        let start_length = sources
            .iter()
            .take_while(|e| !e.spread)
            .count()
            .min(targets.iter().take_while(|e| !e.spread).count());
        let target_ending = targets.iter().rev().take_while(|e| !e.spread).count();
        let end_length = if targets.iter().any(|e| e.spread) {
            sources.iter().rev().take_while(|e| !e.spread).count().min(target_ending)
        } else {
            0
        };
        for i in 0..start_length {
            self.infer_from_types_within(
                sources[i].r#type,
                targets[i].r#type,
                original,
                parameters,
                out,
                depth + 1,
            );
        }
        let source_middle = sources.len().saturating_sub(start_length + end_length);
        let remaining_source_rest =
            if source_middle == 1 { source_rest[start_length] } else { None };
        if let Some(rest) = source_array.or(remaining_source_rest) {
            for target in &targets[start_length..targets.len() - end_length] {
                let ty = if target.spread {
                    let Some(array) = self.global_type_symbol("Array") else { return true };
                    self.create_type_reference(array, vec![rest])
                } else {
                    rest
                };
                self.infer_from_types_within(
                    ty,
                    target.r#type,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
            }
        } else {
            let middle_length = targets.len().saturating_sub(start_length + end_length);
            if middle_length == 2 {
                let first = start_length;
                let second = first + 1;
                if targets[first].spread && targets[second].spread {
                    match (target_rest[first], target_rest[second]) {
                        (None, None) => {
                            if let Some(arity) = out
                                .iter()
                                .find(|info| info.type_parameter == targets[first].r#type)
                                .and_then(|info| info.implied_arity)
                            {
                                let skip = (end_length + sources.len()).saturating_sub(arity);
                                let first_slice =
                                    self.slice_tuple_type(source, start_length, skip).unwrap();
                                let second_slice = self
                                    .slice_tuple_type(source, start_length + arity, end_length)
                                    .unwrap();
                                self.infer_from_types_within(
                                    first_slice,
                                    targets[first].r#type,
                                    original,
                                    parameters,
                                    out,
                                    depth + 1,
                                );
                                self.infer_from_types_within(
                                    second_slice,
                                    targets[second].r#type,
                                    original,
                                    parameters,
                                    out,
                                    depth + 1,
                                );
                            }
                        }
                        (None, Some(rest)) if parameters.contains(&targets[first].r#type) => {
                            if let Some(arity) =
                                self.inference_fixed_tuple_arity(targets[first].r#type)
                            {
                                let skip = sources.len().saturating_sub(start_length + arity);
                                let slice =
                                    self.slice_tuple_type(source, start_length, skip).unwrap();
                                self.infer_from_types_within(
                                    slice,
                                    targets[first].r#type,
                                    original,
                                    parameters,
                                    out,
                                    depth + 1,
                                );
                                let start = start_length + arity;
                                let end = sources.len().saturating_sub(end_length);
                                if start < end
                                    && let Some(ty) =
                                        self.inference_tuple_slice_element(&sources[start..end])
                                {
                                    self.infer_from_types_within(
                                        ty,
                                        rest,
                                        original,
                                        parameters,
                                        out,
                                        depth + 1,
                                    );
                                }
                            }
                        }
                        (Some(rest), None) if parameters.contains(&targets[second].r#type) => {
                            if let Some(arity) =
                                self.inference_fixed_tuple_arity(targets[second].r#type)
                                && let Some(end) = sources.len().checked_sub(target_ending)
                                && let Some(start) = end.checked_sub(arity)
                                && start >= start_length
                            {
                                let trailing = self
                                    .normalize_variadic_tuple(sources[start..end].to_vec(), false);
                                let rest_end = sources.len().saturating_sub(end_length + arity);
                                if start_length < rest_end
                                    && let Some(ty) = self.inference_tuple_slice_element(
                                        &sources[start_length..rest_end],
                                    )
                                {
                                    self.infer_from_types_within(
                                        ty,
                                        rest,
                                        original,
                                        parameters,
                                        out,
                                        depth + 1,
                                    );
                                }
                                self.infer_from_types_within(
                                    trailing,
                                    targets[second].r#type,
                                    original,
                                    parameters,
                                    out,
                                    depth + 1,
                                );
                            }
                        }
                        _ => {}
                    }
                }
            } else if middle_length == 1 && targets[start_length].spread {
                if let Some(rest) = target_rest[start_length] {
                    let end = sources.len().saturating_sub(end_length);
                    if start_length < end
                        && let Some(ty) =
                            self.inference_tuple_slice_element(&sources[start_length..end])
                    {
                        self.infer_from_types_within(
                            ty,
                            rest,
                            original,
                            parameters,
                            out,
                            depth + 1,
                        );
                    }
                } else {
                    let slice = self.slice_tuple_type(source, start_length, end_length).unwrap();
                    let saved = self.inference_priority;
                    if targets.last().is_some_and(|element| element.optional) {
                        self.inference_priority |= InferencePriority::SPECULATIVE_TUPLE;
                    }
                    self.infer_from_types_within(
                        slice,
                        targets[start_length].r#type,
                        original,
                        parameters,
                        out,
                        depth + 1,
                    );
                    self.inference_priority = saved;
                }
            }
        }
        for i in 0..end_length {
            self.infer_from_types_within(
                sources[sources.len() - i - 1].r#type,
                targets[targets.len() - i - 1].r#type,
                original,
                parameters,
                out,
                depth + 1,
            );
        }
        true
    }

    /// inferReverseMappedType (internal/checker/inference.go:1066). Expanding
    /// source and target stacks stop recursive reverse mappings, whose result
    /// is then unknown.
    fn reverse_mapped_member_type(
        &mut self,
        source: TypeId,
        target: TypeId,
        info: &crate::mapped::MappedTypeInfo,
        operand: TypeId,
        constraint: TypeId,
    ) -> TypeId {
        if let Some(&cached) = self.reverse_mapped_member_cache.get(&(source, target, constraint)) {
            return cached;
        }
        self.reverse_mapped_source_stack.push(source);
        self.reverse_mapped_target_stack.push(target);
        let saved = self.reverse_expanding;
        if self.is_deeply_nested_type(source, &self.reverse_mapped_source_stack, 2) {
            self.reverse_expanding.0 = true;
        }
        if self.is_deeply_nested_type(target, &self.reverse_mapped_target_stack, 2) {
            self.reverse_expanding.1 = true;
        }
        let inferred = if self.reverse_expanding == (true, true) {
            self.intrinsics.unknown
        } else {
            self.reverse_mapped_member_type_worker(source, info, operand)
        };
        self.reverse_mapped_source_stack.pop();
        self.reverse_mapped_target_stack.pop();
        self.reverse_expanding = saved;
        self.reverse_mapped_member_cache.insert((source, target, constraint), inferred);
        inferred
    }

    /// inferReverseMappedTypeWorker (internal/checker/inference.go:1091). Treat
    /// the indexed access shared by the constraint/template as the variable.
    fn reverse_mapped_member_type_worker(
        &mut self,
        source: TypeId,
        info: &crate::mapped::MappedTypeInfo,
        operand: TypeId,
    ) -> TypeId {
        let Some(parameter) = self.resolved_indexed_access_type(operand, info.parameter, false)
        else {
            return self.intrinsics.unknown;
        };
        let mut inferences = Vec::new();
        let template = self.mapped_template_type(info);
        self.infer_from_types(source, template, &[parameter], &mut inferences, 0);
        let inferred =
            if let Some(info) = inferences.iter().find(|info| info.type_parameter == parameter) {
                if !info.candidates.is_empty() {
                    self.get_union_type(&info.candidates)
                } else if !info.contra_candidates.is_empty() {
                    self.get_intersection_type(&info.contra_candidates, None)
                } else {
                    self.intrinsics.unknown
                }
            } else {
                self.intrinsics.unknown
            };
        // inferReverseMappedTypeWorker ends with getWidenedType (inference.go:1096).
        self.widen_object_literal_freshness(inferred)
    }

    /// isPartiallyInferableType (inference.go). Non-inferable object images
    /// still expose their ordinary data properties to reverse mapped inference.
    fn is_partially_inferable_type(&self, ty: TypeId) -> bool {
        if !self.non_inferrable_types.contains(&ty) {
            return true;
        }
        if self.object_literal_members.contains_key(&ty)
            && let Some((properties, _)) = self.anonymous_properties.get(&ty)
        {
            return properties
                .iter()
                .any(|property| self.is_partially_inferable_type(property.r#type));
        }
        self.tuple_element_lists.get(&ty).is_some_and(|(elements, _)| {
            elements.iter().any(|&element| self.is_partially_inferable_type(element))
        })
    }

    /// createReverseMappedType (inference.go:1014). Arrays and tuples map
    /// their elements immediately; other sources produce a reverse mapped
    /// object whose members resolveReverseMappedTypeMembers reads on demand.
    fn reverse_homomorphic_mapped_type(
        &mut self,
        source: TypeId,
        target: TypeId,
        info: &crate::mapped::MappedTypeInfo,
        operand: TypeId,
        constraint: TypeId,
    ) -> Option<TypeId> {
        if !self.is_partially_inferable_type(source) {
            return None;
        }
        if let Some(&cached) = self.reverse_mapped_cache.get(&(source, target, constraint)) {
            if let Some(cached) = cached
                && self.reverse_property_anonymous.last() != Some(&false)
            {
                self.complete_reverse_mapped_type(cached);
            }
            return cached;
        }
        self.reverse_mapped_cache
            .insert((source, target, constraint), Some(self.intrinsics.unknown));
        let result = if let Some((elements, readonly)) =
            self.tuple_element_lists.get(&source).cloned()
        {
            let mask = self
                .tuple_optional_masks
                .get(&source)
                .cloned()
                .unwrap_or_else(|| vec![false; elements.len()]);
            let labels = self
                .tuple_labels
                .get(&source)
                .cloned()
                .unwrap_or_else(|| vec![None; elements.len()]);
            let elements: Vec<_> = elements
                .into_iter()
                .enumerate()
                .map(|(i, ty)| {
                    (
                        self.reverse_mapped_member_type(ty, target, info, operand, constraint),
                        mask[i] && info.optionality != Some(true),
                    )
                })
                .collect();
            Some(self.create_optional_tuple_type(&elements, &labels, readonly))
        } else if !self.store.get(source).flags.contains(crate::flags::TypeFlags::ANY)
            && let Some(element) = self.tuple_spread_array_element(source)
        {
            let element =
                self.reverse_mapped_member_type(element, target, info, operand, constraint);
            let readonly = self.type_reference_targets.get(&source).is_some_and(|(symbol, _)| {
                self.binder.symbols().get(*symbol).name == "ReadonlyArray"
            });
            let name = if readonly { "ReadonlyArray" } else { "Array" };
            self.global_type_symbol(name)
                .map(|array| self.create_type_reference(array, vec![element]))
        } else {
            let pending = crate::mapped::ReverseMappedInfo {
                source,
                target,
                info: info.clone(),
                operand,
                constraint,
            };
            let (members, index) = self.reverse_mapped_member_plan(&pending);
            if index.is_none() && self.property_names_of(source).is_empty() {
                None
            } else {
                let placeholder = Self::reverse_mapped_text(&members, index.as_ref(), None);
                let owner = match self.store.get(source).data {
                    TypeData::Named { members, .. } => members,
                    _ => None,
                };
                let reversed = self.store.new_named(
                    crate::flags::TypeFlags::OBJECT,
                    placeholder.clone(),
                    owner,
                );
                self.reverse_placeholder_texts.insert(reversed, placeholder);
                self.pending_reverse_mapped.insert(reversed, pending);
                // The node builder prints a reverse mapping nested under a
                // property of a non-anonymous source type as a placeholder
                // (shouldUsePlaceholderForProperty, nodebuilderimpl.go:2302), so its
                // members are resolved only when a consumer reads them.
                if self.reverse_property_anonymous.last() != Some(&false) {
                    self.complete_reverse_mapped_type(reversed);
                }
                Some(reversed)
            }
        };
        self.reverse_mapped_cache.insert((source, target, constraint), result);
        result
    }

    /// The property names, modifiers and source types that
    /// resolveReverseMappedTypeMembers (inference.go:1099) gives a reverse
    /// mapped object, after getLimitedConstraint filtering.
    fn reverse_mapped_member_plan(
        &mut self,
        pending: &crate::mapped::ReverseMappedInfo,
    ) -> (Vec<crate::objects::AnonymousProperty>, Option<crate::index_signatures::IndexInfo>) {
        let source = pending.source;
        let info = &pending.info;
        let names = self.property_names_of(source);
        let index = self
            .get_index_infos_of_type(source)
            .and_then(|infos| infos.into_iter().find(|info| info.key == self.intrinsics.string));
        let parts = info.constraint_intersection.clone().or_else(|| {
            match &self.store.get(info.constraint).data {
                TypeData::Intersection { types, .. } => Some(types.clone()),
                _ => None,
            }
        });
        let limited = parts
            .map(|types| {
                // getIndexType caches one index type per operand, so a
                // re-minted `keyof U` is still the reverse constraint.
                let operand = self.deferred_keyof_operands.get(&pending.constraint).copied();
                let types: Vec<_> = types
                    .into_iter()
                    .filter(|&ty| {
                        ty != pending.constraint
                            && (operand.is_none()
                                || self.deferred_keyof_operands.get(&ty).copied() != operand)
                    })
                    .collect();
                self.get_intersection_type(&types, None)
            })
            .filter(|&ty| ty != self.intrinsics.never);
        let mut members = Vec::new();
        for name in names {
            if let Some(limited) = limited {
                let key = self.literal_type_of_property(source, &name);
                if !self.is_type_assignable_to(key, limited) {
                    continue;
                }
            }
            let Some(ty) = self.get_type_of_property_of_type(source, &name) else {
                continue;
            };
            let property = self.get_property_of_type(source, &name);
            let captured = self.anonymous_properties.get(&source).and_then(|(properties, _)| {
                properties.iter().find(|property| property.name == name)
            });
            let optional = info.optionality != Some(true)
                && captured.map_or_else(
                    || property.is_some_and(|symbol| self.property_is_optional(symbol)),
                    |property| property.optional,
                );
            let readonly = info.readonly != Some(true)
                && captured.map_or_else(
                    || property.is_some_and(|symbol| self.is_readonly_property(symbol)),
                    |property| property.readonly,
                );
            let printed_name =
                captured.map_or_else(|| name.clone(), |property| property.printed_name.clone());
            let origin = captured.and_then(|property| property.origin).or(property);
            members.push(crate::objects::AnonymousProperty {
                accessor_write: None,
                method: false,
                origin,
                checked_declaration: None,
                name,
                printed_name,
                printed_type: String::new(),
                optional,
                readonly,
                r#type: ty,
            });
        }
        (members, index)
    }

    /// Render a reverse mapped object. Without resolved member types every
    /// property prints the node builder's `any` placeholder.
    fn reverse_mapped_text(
        members: &[crate::objects::AnonymousProperty],
        index: Option<&crate::index_signatures::IndexInfo>,
        resolved: Option<&[String]>,
    ) -> String {
        let mut rendered: Vec<_> = members
            .iter()
            .enumerate()
            .map(|(i, member)| crate::objects::Member::Property {
                name: member.printed_name.clone(),
                optional: member.optional,
                readonly: member.readonly,
                printed: resolved.map_or_else(|| "any".to_string(), |types| types[i].clone()),
            })
            .collect();
        if index.is_some() {
            rendered.push(crate::objects::Member::Index {
                readonly: false,
                name: "x".to_string(),
                key: "string".to_string(),
                // createTypeNodesFromResolvedType always elides reverse
                // mapped index values (nodebuilderimpl.go:2646).
                value: "any".to_string(),
            });
        }
        crate::objects::render_object_type(&rendered)
    }

    /// resolveReverseMappedTypeMembers and getTypeOfReverseMappedSymbol
    /// (inference.go:1099,1145) for a pending reverse mapped object.
    pub(crate) fn complete_reverse_mapped_type(&mut self, reversed: TypeId) {
        let Some(pending) = self.pending_reverse_mapped.remove(&reversed) else { return };
        let (mut members, index) = self.reverse_mapped_member_plan(&pending);
        // A reverse mapping of `{[K in keyof T[K_1]]: T[K_1]}` is that of
        // `{[K in keyof T]: T}` (replaceIndexedAccess, inference.go:1133).
        let (target, info, operand, constraint) =
            self.simplified_reverse_mapping(&pending).unwrap_or_else(|| {
                (pending.target, pending.info.clone(), pending.operand, pending.constraint)
            });
        let mut texts = Vec::with_capacity(members.len());
        for member in &mut members {
            let anonymous = self.is_anonymous_object_type(member.r#type);
            self.reverse_property_anonymous.push(anonymous);
            let ty =
                self.reverse_mapped_member_type(member.r#type, target, &info, operand, constraint);
            self.reverse_property_anonymous.pop();
            let text = match self.reverse_placeholder_texts.get(&ty) {
                Some(placeholder) if !anonymous => placeholder.clone(),
                _ => self.type_to_string(ty),
            };
            member.r#type = ty;
            member.printed_type.clone_from(&text);
            texts.push(text);
        }
        let reversed_index = index.map(|index| {
            // Native creates the reverse index value lazily, and its node
            // builder prints `any` without reading that value's members.
            // Keep a nested reverse object pending until an index consumer
            // actually asks for its members, as we do for property placeholders.
            self.reverse_property_anonymous.push(false);
            let value = self.reverse_mapped_member_type(
                index.value,
                pending.target,
                &pending.info,
                pending.operand,
                pending.constraint,
            );
            self.reverse_property_anonymous.pop();
            crate::index_signatures::IndexInfo {
                components: None,
                declaration: None,
                key: index.key,
                readonly: false,
                value,
            }
        });
        let text = Self::reverse_mapped_text(&members, reversed_index.as_ref(), Some(&texts));

        let owner = match self.store.get(pending.source).data {
            TypeData::Named { members, .. } => members,
            _ => None,
        };
        let resolved = self.store.new_named(crate::flags::TypeFlags::OBJECT, text, owner);
        self.store.complete_object(reversed, resolved);
        self.anonymous_properties.insert(reversed, (members, true));
        if let Some(index) = reversed_index {
            self.object_literal_index_infos.insert(reversed, vec![index]);
        }
    }

    /// resolveReverseMappedTypeMembers' replaceIndexedAccess simplification
    /// (inference.go:1133): T[K] as the shared variable reverses like T.
    fn simplified_reverse_mapping(
        &mut self,
        pending: &crate::mapped::ReverseMappedInfo,
    ) -> Option<(TypeId, crate::mapped::MappedTypeInfo, TypeId, TypeId)> {
        use crate::flags::TypeFlags;
        let &(object, index, _) = self.deferred_indexed_access_types.get(&pending.operand)?;
        if !self.store.get(object).flags.contains(TypeFlags::TYPE_PARAMETER)
            || !self.store.get(index).flags.contains(TypeFlags::TYPE_PARAMETER)
        {
            return None;
        }
        let zero = self.store.intern_literal(
            TypeFlags::NUMBER_LITERAL,
            TypeData::NumberLiteral("0".to_string()),
            false,
        );
        let wrapped = self.create_tuple_type(vec![object], false);
        let target = self.instantiate_type(
            pending.target,
            &[(index, zero), (object, wrapped)],
            &[index, object],
            &[],
        );
        self.ensure_mapped_type_info(target);
        let info = self.mapped_types.get(&target).cloned()?;
        let constraint = self.resolved_keyof_type(object)?;
        Some((target, info, object, constraint))
    }

    /// `ObjectFlagsAnonymous`: object literal, type literal and function types,
    /// as opposed to interface, class and type reference instances.
    fn is_anonymous_object_type(&self, ty: TypeId) -> bool {
        // An alias instantiation keeps the object flags of its body.
        if let Some(&(symbol, _)) = self.type_reference_targets.get(&ty) {
            let symbol = self.binder.symbols().get(symbol);
            return symbol.flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
                && symbol.declarations.first().and_then(|&id| self.node_map.get(id)).is_some_and(
                    |node| {
                        matches!(node, tsr_ast::Node::TypeAliasDeclaration(alias)
                        if matches!(
                            alias.r#type,
                            Some(
                                tsr_ast::TypeNode::TypeLiteralNode(_)
                                    | tsr_ast::TypeNode::FunctionTypeNode(_)
                                    | tsr_ast::TypeNode::ConstructorTypeNode(_)
                            )
                        ))
                    },
                );
        }
        match self.store.get(ty).data {
            TypeData::Anonymous { .. } => true,
            TypeData::Named { members: Some(owner), .. } => {
                !self.type_reference_targets.contains_key(&ty)
                    && self.binder.symbols().get(self.binder.merged_symbol(owner)).flags.intersects(
                        tsr_binder::SymbolFlags::TYPE_LITERAL
                            | tsr_binder::SymbolFlags::OBJECT_LITERAL
                            | tsr_binder::SymbolFlags::FUNCTION
                            | tsr_binder::SymbolFlags::METHOD,
                    )
            }
            TypeData::Named { members: None, .. } => {
                self.anonymous_properties.contains_key(&ty)
                    && !self.type_reference_targets.contains_key(&ty)
                    && !self.mapped_types.contains_key(&ty)
            }
            _ => false,
        }
    }

    /// getLiteralTypeFromProperty for the known string/numeric property names
    /// used when a reverse mapped intersection constraint filters source keys.
    pub(crate) fn literal_type_of_property(&mut self, source: TypeId, name: &str) -> TypeId {
        use crate::flags::TypeFlags;
        // Native 5b1047d getLiteralTypeFromProperty reads the concrete source
        // symbol's nameType before its valueDeclaration. A computed literal's
        // semantic name need not exist in the raw binder table: its declaration
        // is retained by the Checker-local member image. Use that origin before
        // ordinary symbol lookup; do not infer a key type from printed_name.
        // This reads the existing image; it publishes no additional member table.
        let origin = self
            .anonymous_properties
            .get(&source)
            .and_then(|(properties, _)| properties.iter().find(|property| property.name == name))
            .and_then(|property| property.origin)
            .or_else(|| self.get_property_of_type(source, name));
        let declaration =
            origin.and_then(|symbol| self.binder.symbols().get(symbol).value_declaration);
        let property =
            declaration.and_then(|id| self.node_map.get(id)).and_then(|node| match node {
                tsr_ast::Node::PropertyAssignment(p) => Some(p.name),
                tsr_ast::Node::PropertySignatureDeclaration(p) => Some(p.name),
                tsr_ast::Node::PropertyDeclaration(p) => Some(p.name),
                tsr_ast::Node::MethodSignatureDeclaration(p) => Some(p.name),
                tsr_ast::Node::MethodDeclaration(p) => Some(p.name),
                _ => None,
            });
        match property {
            Some(tsr_ast::PropertyName::NumericLiteral(literal)) => self.store.intern_literal(
                TypeFlags::NUMBER_LITERAL,
                TypeData::NumberLiteral(crate::printing::normalise_number(literal.text)),
                false,
            ),
            Some(tsr_ast::PropertyName::ComputedPropertyName(computed)) => {
                computed.expression.map_or(self.intrinsics.error, |expression| {
                    let ty = self.check_expression(expression);
                    self.get_regular_type_of_literal_type(ty)
                })
            }
            _ => self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(name.to_owned()),
                false,
            ),
        }
    }

    /// inferToMappedType (internal/checker/inference.go:948). Key parameter
    /// constraints contribute keyof candidates before reverse template inference.
    fn infer_to_mapped_type(
        &mut self,
        source: TypeId,
        target: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) -> bool {
        self.ensure_mapped_type_info(target);
        let Some(info) = self.mapped_types.get(&target).cloned() else { return false };
        // inferFromObjectTypes only reverses mappings without an as clause.
        if info.name_type.is_some() {
            return false;
        }
        self.infer_to_mapped_constraint(
            source,
            target,
            &info,
            info.constraint,
            parameters,
            out,
            depth,
            &mut Vec::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_to_mapped_constraint(
        &mut self,
        source: TypeId,
        target: TypeId,
        info: &crate::mapped::MappedTypeInfo,
        constraint: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
        visiting: &mut Vec<TypeId>,
    ) -> bool {
        if visiting.contains(&constraint) {
            return false;
        }
        visiting.push(constraint);
        let result = self.infer_to_mapped_constraint_worker(
            source, target, info, constraint, parameters, out, depth, visiting,
        );
        visiting.pop();
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_to_mapped_constraint_worker(
        &mut self,
        source: TypeId,
        target: TypeId,
        info: &crate::mapped::MappedTypeInfo,
        constraint: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
        visiting: &mut Vec<TypeId>,
    ) -> bool {
        use crate::flags::TypeFlags;
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            &self.store.get(constraint).data
        {
            let types = types.clone();
            let mut result = false;
            for constraint in types {
                result |= self.infer_to_mapped_constraint(
                    source, target, info, constraint, parameters, out, depth, visiting,
                );
            }
            return result;
        }
        if let Some(&operand) = self.deferred_keyof_operands.get(&constraint) {
            if parameters.contains(&operand)
                && !out.iter().any(|info| info.type_parameter == operand && info.is_fixed)
                && let Some(inferred) =
                    self.reverse_homomorphic_mapped_type(source, target, info, operand, constraint)
            {
                let priority = self.inference_priority
                    | if self.non_inferrable_types.contains(&source) {
                        InferencePriority::PARTIAL_HOMOMORPHIC_MAPPED_TYPE
                    } else {
                        InferencePriority::HOMOMORPHIC_MAPPED_TYPE
                    };
                self.inference_observed_priority =
                    self.inference_observed_priority.min(i32::from(priority.bits()));
                add_directional_candidate(
                    out,
                    operand,
                    inferred,
                    self.inference_contravariant && !self.inference_bivariant,
                    priority,
                );
            }
            return true;
        }
        if self.store.get(constraint).flags.contains(TypeFlags::TYPE_PARAMETER) {
            if let Some(keys) = self.resolved_keyof_type(source) {
                let saved = self.inference_priority;
                self.inference_priority |= InferencePriority::MAPPED_TYPE_CONSTRAINT;
                self.infer_from_types_within(
                    keys,
                    constraint,
                    constraint,
                    parameters,
                    out,
                    depth + 1,
                );
                self.inference_priority = saved;
            }
            if let Some(extended) = self.type_parameter_constraint(constraint)
                && self.infer_to_mapped_constraint(
                    source, target, info, extended, parameters, out, depth, visiting,
                )
            {
                return true;
            }
            let mut values = Vec::new();
            for name in self.property_names_of(source) {
                if let Some(value) = self.get_type_of_property_of_type(source, &name) {
                    values.push(value);
                }
            }
            if let Some(indexes) = self.get_index_infos_of_type(source) {
                values.extend(indexes.into_iter().map(|index| index.value));
            }
            let values = self.get_union_type(&values);
            let template = self.mapped_template_type(info);
            self.infer_from_types_within(values, template, template, parameters, out, depth + 1);
            return true;
        }
        false
    }

    /// §937's `couldContainTypeVariables` (`checker.go:22184`) — whether a
    /// target can contribute anything to inference at all.
    ///
    /// **It does not walk members, and that is the whole point.** The first
    /// build of this predicate did, and the conformance run went from ~4 minutes
    /// to not finishing in 20; memoising it and bounding the arm changed
    /// nothing, because the expense was the design. Upstream reads a cached
    /// `ObjectFlags` bit and, for an anonymous object, answers `true` from the
    /// **symbol's flags alone** (`checker.go:22194`) — a type literal, object
    /// literal, function, method or class symbol could contain a type variable,
    /// and no member is examined to decide it.
    ///
    /// So: a reference consults its arguments, a union its constituents, an
    /// object-ish symbol answers `true` outright, everything else `false`.
    ///
    /// **Separate from [`Checker::mentions_type_parameter`] on purpose.** That
    /// one is §787's union strike-out gate, whose documented conservative
    /// direction is `false`, and it is measured for that use.
    ///
    /// `visiting` guards the reference/union recursion; the memo below is what
    /// keeps repeated queries cheap.
    fn target_could_contain_parameter(
        &mut self,
        id: TypeId,
        parameters: &[TypeId],
        visiting: &mut Vec<TypeId>,
    ) -> bool {
        if parameters.contains(&id) {
            return true;
        }
        if visiting.contains(&id) || visiting.len() > 16 {
            return false;
        }
        let top_level = visiting.is_empty();
        let key = (id, parameters.to_vec());
        if top_level && let Some(&cached) = self.could_contain_parameter_cache.get(&key) {
            return cached;
        }
        visiting.push(id);
        let answer = self.could_contain_parameter_inner(id, parameters, visiting);
        visiting.pop();
        if top_level {
            self.could_contain_parameter_cache.insert(key, answer);
        }
        answer
    }

    fn could_contain_parameter_inner(
        &mut self,
        id: TypeId,
        parameters: &[TypeId],
        visiting: &mut Vec<TypeId>,
    ) -> bool {
        // A REFERENCE could contain one if any of its arguments could.
        if let Some((_, arguments)) = self.type_reference_targets.get(&id).cloned()
            && arguments
                .into_iter()
                .any(|argument| self.target_could_contain_parameter(argument, parameters, visiting))
        {
            return true;
        }
        // Tuple references expose their element arguments just like named
        // references. Iterable<readonly [K, V]> contains both K and V.
        if let Some(elements) = self.inference_tuple_elements(id)
            && elements.into_iter().any(|element| {
                self.target_could_contain_parameter(element.r#type, parameters, visiting)
            })
        {
            return true;
        }
        // A UNION, likewise over its constituents.
        if let TypeData::Union { types, .. } = &self.store.get(id).data {
            let constituents = types.clone();
            if constituents.into_iter().any(|constituent| {
                self.target_could_contain_parameter(constituent, parameters, visiting)
            }) {
                return true;
            }
        }
        // **An ANONYMOUS object answers `true` from its SYMBOL'S FLAGS alone**
        // (`checker.go:22194`): a type literal, object literal, function, method
        // or class symbol with declarations could contain a type variable, and
        // upstream decides that without looking at a single member.
        if let TypeData::Anonymous { symbol, .. } = self.store.get(id).data {
            let flags = self.binder.symbols().get(self.binder.merged_symbol(symbol)).flags;
            if flags.intersects(
                tsr_binder::SymbolFlags::FUNCTION
                    | tsr_binder::SymbolFlags::METHOD
                    | tsr_binder::SymbolFlags::CLASS
                    | tsr_binder::SymbolFlags::TYPE_LITERAL
                    | tsr_binder::SymbolFlags::OBJECT_LITERAL,
            ) {
                return true;
            }
        }
        if let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data {
            let flags = self.binder.symbols().get(self.binder.merged_symbol(owner)).flags;
            if flags.intersects(
                tsr_binder::SymbolFlags::TYPE_LITERAL | tsr_binder::SymbolFlags::OBJECT_LITERAL,
            ) {
                return true;
            }
        }
        false
    }

    /// Whether two reference targets are the `Array`/`ReadonlyArray` pair, in
    /// either order. §787.
    ///
    /// Kept to those two globals rather than any structurally-compatible pair:
    /// inference that admits a target it cannot justify produces a CANDIDATE,
    /// and a wrong candidate is a confident wrong answer rather than a missing
    /// one. `Array` and `ReadonlyArray` are the pair upstream names, and their
    /// single type argument occupies the same slot by construction.
    fn is_array_like_pair(
        &mut self,
        first: tsr_binder::SymbolId,
        second: tsr_binder::SymbolId,
    ) -> bool {
        if first == second {
            return true;
        }
        let resolve = |checker: &mut Self, name: &str| {
            checker.global_type_symbol(name).map(|symbol| checker.binder.merged_symbol(symbol))
        };
        let array = resolve(self, "Array");
        let readonly = resolve(self, "ReadonlyArray");
        let (Some(array), Some(readonly)) = (array, readonly) else { return false };
        let first = self.binder.merged_symbol(first);
        let second = self.binder.merged_symbol(second);
        let pair = [first, second];
        pair.contains(&array) && pair.contains(&readonly)
    }

    /// `inferFromMatchingTypes` and `inferToMultipleTypes` (inference.go), for
    /// an intersection with one naked inference variable and no nested ones.
    /// An identical source constituent is removed before inference; consuming
    /// the entire source deliberately produces no candidate.
    fn intersection_inference_source(
        &mut self,
        source: TypeId,
        target: TypeId,
        parameters: &[TypeId],
    ) -> Option<(Option<TypeId>, TypeId)> {
        let TypeData::Intersection { types, .. } = &self.store.get(target).data else {
            return None;
        };
        let targets = types.clone();
        let variables: Vec<_> =
            targets.iter().copied().filter(|id| parameters.contains(id)).collect();
        let [variable] = variables.as_slice() else { return None };
        let names: Vec<_> = parameters.iter().map(|&id| self.type_to_string(id)).collect();
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        if targets
            .iter()
            .any(|&id| id != *variable && self.mentions_type_parameter(id, parameters, &names))
        {
            return None;
        }
        if matches!(self.store.get(source).data, TypeData::Union { .. }) {
            return Some((Some(source), *variable));
        }
        let sources = match &self.store.get(source).data {
            TypeData::Intersection { types, .. } => types.clone(),
            _ => vec![source],
        };
        // inferFromMatchingTypes also infers from identical matches. A match
        // of the naked variable contributes before the lower-priority remainder.
        if sources.contains(variable) {
            return Some((Some(*variable), *variable));
        }
        let remaining: Vec<_> = sources.into_iter().filter(|id| !targets.contains(id)).collect();
        let source = if remaining.is_empty() {
            None
        } else {
            Some(self.get_intersection_type(&remaining, None))
        };
        Some((source, *variable))
    }

    /// inferToTemplateLiteralType's constrained literal choice (inference.go:566).
    fn preferred_template_inference(&mut self, source: TypeId, target: TypeId) -> TypeId {
        use crate::flags::TypeFlags;
        let TypeData::StringLiteral(value) = &self.store.get(source).data else {
            return source;
        };
        let value = value.clone();
        let constraint = self.template_base_constraint(target);
        if constraint == target || self.store.get(constraint).flags.contains(TypeFlags::ANY) {
            return source;
        }
        let constraints = match &self.store.get(constraint).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![constraint],
        };
        if constraints.iter().any(|&ty| self.store.get(ty).flags.contains(TypeFlags::STRING)) {
            return source;
        }
        let number = crate::template_match::template_number(&value, true);
        let bigint = crate::template_match::template_bigint(&value, true);
        let mut preferred = None;
        for constraint in constraints {
            let flags = self.store.get(constraint).flags;
            let candidate =
                if flags.contains(TypeFlags::TEMPLATE_LITERAL)
                    && self.is_type_assignable_to(source, constraint)
                {
                    Some((0, source))
                } else if flags.contains(TypeFlags::STRING_MAPPING)
                    && self.string_mapping_types.get(&constraint).copied().is_some_and(
                        |(symbol, _)| self.apply_string_mapping(symbol, &value) == value,
                    )
                {
                    Some((1, source))
                } else if let TypeData::StringLiteral(literal) = &self.store.get(constraint).data {
                    (literal == &value).then_some((2, constraint))
                } else if flags.contains(TypeFlags::NUMBER) {
                    number.as_ref().map(|number| {
                        (
                            3,
                            self.store.intern_literal(
                                TypeFlags::NUMBER_LITERAL,
                                TypeData::NumberLiteral(number.clone()),
                                false,
                            ),
                        )
                    })
                } else if let TypeData::NumberLiteral(literal) = &self.store.get(constraint).data {
                    number.as_ref().filter(|number| *number == literal).map(|_| (5, constraint))
                } else if flags.contains(TypeFlags::BIG_INT) {
                    bigint.as_ref().map(|bigint| {
                        (
                            6,
                            self.store.intern_literal(
                                TypeFlags::BIG_INT_LITERAL,
                                TypeData::BigIntLiteral(bigint.clone()),
                                false,
                            ),
                        )
                    })
                } else if let TypeData::BigIntLiteral(literal) = &self.store.get(constraint).data {
                    bigint
                        .as_ref()
                        .filter(|bigint| bigint.as_str() == literal.trim_end_matches('n'))
                        .map(|_| (7, constraint))
                } else if let TypeData::BooleanLiteral(literal) = self.store.get(constraint).data {
                    (value == if literal { "true" } else { "false" }).then_some((9, constraint))
                } else if flags.contains(TypeFlags::UNDEFINED) && value == "undefined" {
                    Some((10, constraint))
                } else if flags.contains(TypeFlags::NULL) && value == "null" {
                    Some((11, constraint))
                } else {
                    None
                };
            if let Some((rank, ty)) = candidate
                && preferred.is_none_or(|(old_rank, _)| rank < old_rank)
            {
                preferred = Some((rank, ty));
            }
        }
        preferred.map_or(source, |(_, ty)| ty)
    }

    fn infer_from_types_within(
        &mut self,
        source: TypeId,
        target: TypeId,
        original: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) {
        // Retain the port's outer recursion cap for inference paths that do
        // not yet go through invokeOnce. Structural member inference also
        // applies the native recursion-identity guard below.
        if depth > 16 {
            self.inference_observed_priority = -1;
            return;
        }
        // inferFromTypes (`inference.go:66`, `:151`): nothing is inferred
        // into a `NoInfer<T>` target.
        if source == self.intrinsics.error || self.no_infer_base_type(target).is_some() {
            return;
        }
        if self.infer_from_tuple_types(source, target, original, parameters, out, depth) {
            return;
        }
        if parameters.contains(&target) {
            if self.non_inferrable_types.contains(&source)
                || self.contains_silent_never(source, &mut Vec::new())
            {
                return;
            }
            self.inference_observed_priority =
                self.inference_observed_priority.min(i32::from(self.inference_priority.bits()));
            // inferFromTypes keeps both candidates and topLevel unchanged once
            // the inference is fixed (internal/checker/inference.go).
            if out.iter().any(|info| info.type_parameter == target && info.is_fixed) {
                return;
            }
            add_directional_candidate(
                out,
                target,
                source,
                self.inference_contravariant && !self.inference_bivariant,
                self.inference_priority,
            );
            // `inference.go:207-208`: a candidate arriving where the walk's
            // original target does not carry the parameter at TOP LEVEL marks
            // the inference nested, and the widening decision reads it.
            if !self.inference_priority.contains(InferencePriority::RETURN_TYPE)
                && !self.is_type_parameter_at_top_level(original, target)
                && let Some(info) = out.iter_mut().find(|i| i.type_parameter == target)
            {
                info.top_level = false;
            }
            return;
        }
        if let (Some((source_symbol, source_inner)), Some((target_symbol, target_inner))) = (
            self.string_mapping_types.get(&source).copied(),
            self.string_mapping_types.get(&target).copied(),
        ) {
            if source_symbol == target_symbol {
                self.infer_from_types_within(
                    source_inner,
                    target_inner,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
            }
            return;
        }
        if let Some(parts) = self.template_literal_parts.get(&target).cloned()
            && !matches!(self.store.get(source).data, TypeData::Union { .. })
        {
            let matches = self.template_literal_inferences(source, &parts);
            if matches.is_some() || parts.texts.iter().all(String::is_empty) {
                for (index, hole) in parts.types.into_iter().enumerate() {
                    let source =
                        matches.as_ref().map_or(self.intrinsics.never, |matches| matches[index]);
                    let source = if parameters.contains(&hole) {
                        self.preferred_template_inference(source, hole)
                    } else {
                        source
                    };
                    self.infer_from_types_within(
                        source,
                        hole,
                        original,
                        parameters,
                        out,
                        depth + 1,
                    );
                }
            }
            return;
        }
        if let Some((remaining, variable)) =
            self.intersection_inference_source(source, target, parameters)
        {
            if let Some(source) = remaining {
                self.infer_from_types_within(
                    source,
                    variable,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
            }
            return;
        }
        let target_reference = self.type_reference_targets.get(&target).cloned();
        let source_reference = self.type_reference_targets.get(&source).cloned();
        if let (Some((ts, ta)), Some((ss, sa))) = (target_reference, source_reference) {
            // §787: `Array` and `ReadonlyArray` are ONE reference target for
            // inference. `mk<T>(values: readonly T[])` called with `[0, 1, 2]`
            // puts an `Array<number>` source against a `ReadonlyArray<T>`
            // target, and the identity test below refused it — so `T` got no
            // candidate and the whole call answered `errorType`, printed `any`.
            //
            // Upstream infers argument-wise here: `inferFromObjectTypes`
            // (`inference.go`) admits two references whose targets differ when
            // both are array-like, because a mutable array IS a readonly one
            // and their single type argument occupies the same slot.
            //
            // This is not a niche shape. `readonly T[]` is how every modern lib
            // signature spells an array parameter — `new Set(values)`,
            // `Promise.all`, `Array.from`, `concat` — so a single missing pair
            // made `new Set([0, 1, 2])` answer `any`. `compiler/setMethods` is
            // 180 wrong lines against 37 right for exactly this reason.
            let same_target = ts == ss || self.is_array_like_pair(ts, ss);
            if same_target && ta.len() == sa.len() {
                let variances = self.inference_variances(ts);
                for (index, (t, s)) in ta.iter().zip(sa.iter()).enumerate() {
                    let saved = self.inference_contravariant;
                    if variances.as_ref().and_then(|variances| variances.get(index))
                        == Some(&crate::variances::Variance::Contravariant)
                    {
                        self.inference_contravariant = !saved;
                    }
                    self.infer_from_types_within(*s, *t, original, parameters, out, depth + 1);
                    self.inference_contravariant = saved;
                }
                return;
            }
            // inferFromObjectTypes (internal/checker/inference.go) continues
            // structurally when the reference targets differ. Candidate
            // direction and priority resolve the resulting inferences.
        }
        // Native dispatch matches references before conditional targets.
        // invokeOnce(inferToConditionalType) must precede branch reads: a
        // recursive conditional can return the same reference from a branch.
        if self.mapped_conditional_branches.contains_key(&target)
            || self.conditional_inference_nodes.contains_key(&target)
            || self
                .type_reference_targets
                .get(&target)
                .is_some_and(|(symbol, _)| self.alias_declares_conditional(*symbol))
        {
            let key = (source, target);
            if let Some(&priority) = self.inference_visited_pairs.get(&key) {
                self.inference_observed_priority = self.inference_observed_priority.min(priority);
                return;
            }
            self.inference_visited_pairs.insert(key, -1);
            let saved = self.inference_observed_priority;
            self.inference_observed_priority = i32::from(InferencePriority::MAX_VALUE.bits());
            self.infer_to_conditional_type(source, target, original, parameters, out, depth);
            self.inference_visited_pairs.insert(key, self.inference_observed_priority);
            self.inference_observed_priority = self.inference_observed_priority.min(saved);
            return;
        }
        // Native aliases already expose their instantiable/union/intersection
        // operands to inferFromTypes. Expose this port's reference shell body
        // before selecting its structural dispatch. An original alias target
        // must also expose that same body for native's top-level widening test.
        if let Some((symbol, arguments)) = self.type_reference_targets.get(&target).cloned()
            && self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
            && let Some(body) = self.evaluate_alias_body(symbol, &arguments)
            && body != target
            && self.store.get(body).flags.intersects(
                crate::flags::TypeFlags::INSTANTIABLE
                    | crate::flags::TypeFlags::UNION
                    | crate::flags::TypeFlags::INTERSECTION,
            )
        {
            let original = if original == target { body } else { original };
            self.infer_from_types_within(source, body, original, parameters, out, depth + 1);
            return;
        }
        if let TypeData::Union { types, .. } = self.store.get(target).data.clone() {
            self.infer_to_union(source, &types, original, parameters, out, depth);
            return;
        }
        // inferToMultipleTypes (inference.go): infer into the structured
        // intersection constituents before its one naked variable. More than
        // one naked variable supplies no direct inference candidates.
        if let TypeData::Intersection { types, .. } = self.store.get(target).data.clone() {
            let mut variables = Vec::new();
            for target in types {
                if parameters.contains(&target) {
                    variables.push(target);
                } else {
                    self.infer_from_types_within(
                        source,
                        target,
                        original,
                        parameters,
                        out,
                        depth + 1,
                    );
                }
            }
            if let [variable] = variables.as_slice() {
                let saved = self.inference_priority;
                self.inference_priority |= InferencePriority::NAKED_TYPE_VARIABLE;
                self.infer_from_types_within(
                    source,
                    *variable,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
                self.inference_priority = saved;
            }
            return;
        }
        // §937: `inferFromProperties` (`inference.go`), the arm every other
        // structural shape already had. `f<T>(a: { x: T })` called with
        // `{ x: "s" }` collected NO candidate for `T` and answered `error`,
        // while `T[]`, `Array<T>`, `(v: T) => void`, `Promise<T>` and a bare
        // `T` all inferred correctly — **the one structural position left out
        // was the commonest one an argument takes**, the options-bag object.
        //
        // For each property of the TARGET, recurse against the source's property
        // of the same name. A target property the source lacks contributes
        // nothing, which is upstream's behaviour and not a failure.
        //
        // Runs BEFORE the signature arm and does not return: upstream's
        // `inferFromObjectTypes` does properties, then index signatures, then
        // signatures, and a type may carry both.
        //
        // **Gated on `couldContainTypeVariables`, and that gate is load-bearing
        // for RUNTIME, not correctness.** The first build ran this arm
        // ungated and the conformance run went from ~20 seconds to not
        // finishing in ten minutes, because a lib-typed target drags in
        // `Array`, `String` and friends and the walk descended through all of
        // their members to learn nothing.
        //
        // The fix was **not** a budget. Two were tried — memoising a
        // member-walking predicate, then capping depth and member count — and
        // neither moved the runtime, because the expense was the shape of the
        // predicate rather than its volume. Upstream's
        // `couldContainTypeVariables` (`checker.go:22184`) reads a cached
        // `ObjectFlags` bit and decides an anonymous object from its SYMBOL'S
        // FLAGS, touching no member at all; written that way
        // ([`Checker::target_could_contain_parameter`]) the run is back to 20
        // seconds with the arm unbounded.
        // inferFromTypes avoids apparent constraint reads under NoConstraints
        // for instantiable and intersection sources in this structural arm.
        if self.inference_priority.contains(InferencePriority::NO_CONSTRAINTS)
            && self.store.get(source).flags.intersects(
                crate::flags::TypeFlags::TYPE_PARAMETER
                    | crate::flags::TypeFlags::INDEXED_ACCESS
                    | crate::flags::TypeFlags::CONDITIONAL
                    | crate::flags::TypeFlags::INTERSECTION,
            )
        {
            return;
        }
        if let TypeData::Union { types, .. } = &self.store.get(source).data {
            for constituent in types.clone() {
                self.infer_from_types_within(
                    constituent,
                    target,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
            }
            return;
        }
        if self.infer_to_mapped_type(source, target, parameters, out, depth) {
            return;
        }
        // invokeOnce (inference.go:335) remembers both completed pairs and
        // active circularities, preserving the best observed priority.
        let key = (source, target);
        if let Some(&priority) = self.inference_visited_pairs.get(&key) {
            self.inference_observed_priority = self.inference_observed_priority.min(priority);
            return;
        }
        self.inference_visited_pairs.insert(key, -1);
        let saved_priority = self.inference_observed_priority;
        self.inference_observed_priority = i32::from(InferencePriority::MAX_VALUE.bits());
        let saved_expanding = self.inference_expanding;
        self.inference_source_stack.push(source);
        self.inference_target_stack.push(target);
        self.inference_expanding.0 |=
            self.is_deeply_nested_type(source, &self.inference_source_stack, 2);
        self.inference_expanding.1 |=
            self.is_deeply_nested_type(target, &self.inference_target_stack, 2);
        if self.inference_expanding == (true, true) {
            self.inference_observed_priority = -1;
        } else {
            self.infer_from_members(source, target, original, parameters, out, depth);
        }
        self.inference_source_stack.pop();
        self.inference_target_stack.pop();
        self.inference_expanding = saved_expanding;
        self.inference_visited_pairs.insert(key, self.inference_observed_priority);
        self.inference_observed_priority = self.inference_observed_priority.min(saved_priority);
    }

    /// inferToConditionalType (inference.go:554): conditional sources match
    /// check/extends/true/false operands without changing inference priority.
    /// Conditional targets are not unions: structured branches infer first,
    /// then naked parameters receive lower-priority candidates from the source.
    fn infer_to_conditional_type(
        &mut self,
        source: TypeId,
        target: TypeId,
        original: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) {
        if self.mapped_conditionals.contains_key(&source)
            || self.conditional_inference_nodes.contains_key(&source)
            || self
                .type_reference_targets
                .get(&source)
                .is_some_and(|(symbol, _)| self.alias_declares_conditional(*symbol))
        {
            let Some(from) = self.conditional_inference_operands(source) else { return };
            let to = if source == target {
                from
            } else {
                let Some(to) = self.conditional_inference_operands(target) else { return };
                to
            };
            for (source, target) in from.into_iter().zip(to) {
                self.infer_from_types_within(source, target, original, parameters, out, depth + 1);
            }
            return;
        }
        let Some((yes, no)) = self.conditional_inference_branches(target) else { return };
        let saved = self.inference_priority;
        if self.inference_contravariant {
            self.inference_priority |= InferencePriority::CONTRAVARIANT_CONDITIONAL;
        }
        for branch in [yes, no] {
            if !parameters.contains(&branch) {
                self.infer_from_types_within(source, branch, original, parameters, out, depth + 1);
            }
        }
        self.inference_priority |= InferencePriority::NAKED_TYPE_VARIABLE;
        for branch in [yes, no] {
            if parameters.contains(&branch) {
                self.infer_from_types_within(source, branch, original, parameters, out, depth + 1);
            }
        }
        self.inference_priority = saved;
    }

    /// The structural member portion of inferFromObjectTypes (inference.go).
    fn infer_from_members(
        &mut self,
        source: TypeId,
        target: TypeId,
        original: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) {
        let target_names =
            if self.target_could_contain_parameter(target, parameters, &mut Vec::new()) {
                // inferFromProperties includes late-bound members such as
                // Symbol.iterator. The spelling-suggestion enumeration omits
                // them and would lose inference through Iterable<T>.
                let captured = self.property_names_of(target);
                if self.anonymous_properties.contains_key(&target) {
                    captured
                } else {
                    self.get_property_names_of_type(target).unwrap_or(captured)
                }
            } else {
                Vec::new()
            };
        if !target_names.is_empty() {
            for name in &target_names {
                let (Some(mut target_member), Some(mut source_member)) = (
                    self.get_type_of_property_of_type(target, name),
                    self.get_type_of_property_of_type(source, name),
                ) else {
                    continue;
                };
                // inferFromProperties (inference.go:829) removes only the
                // distinct missing constituent carried by optional symbols.
                // Explicit `undefined` therefore remains an inference
                // candidate under exactOptionalPropertyTypes.
                if self.exact_optional_property_types {
                    if self
                        .get_property_of_type(target, name)
                        .is_some_and(|property| self.property_is_optional(property))
                    {
                        target_member = self.remove_missing_type(target_member);
                    }
                    if self
                        .get_property_of_type(source, name)
                        .is_some_and(|property| self.property_is_optional(property))
                    {
                        source_member = self.remove_missing_type(source_member);
                    }
                }
                self.infer_from_types_within(
                    source_member,
                    target_member,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
            }
        }
        // §941: `inferFromIndexTypes` (`inference.go`). §937 added the property
        // arm and stopped there; an INDEX signature is the other half of
        // `inferFromObjectTypes`, and it is what a `ConcatArray<T>` target needs.
        //
        // `["a"].concat(["b"])` was `error` because `concat`'s rest element is
        // `ConcatArray<T>` while the argument is `Array<string>`: two references
        // with DIFFERENT targets, which §787's argument-wise arm admits only for
        // the array pair. Upstream relates them structurally instead, and
        // `ConcatArray<T>` carries `[n: number]: T` — so the element type flows
        // from the source's own index info.
        //
        // Gated on the same `couldContainTypeVariables` answer the property arm
        // uses, for the runtime reason §937 records.
        if !target_names.is_empty() || self.get_index_infos_of_type(target).is_some() {
            let target_infos = self.get_index_infos_of_type(target).unwrap_or_default();
            if !target_infos.is_empty()
                && self.target_could_contain_parameter(target, parameters, &mut Vec::new())
            {
                let source_infos = self.get_index_infos_of_type(source).unwrap_or_default();
                let inferable = self.is_object_type_with_inferable_index(source);
                let source_names = if inferable {
                    self.get_property_names_of_type(source)
                        .unwrap_or_else(|| self.property_names_of(source))
                } else {
                    Vec::new()
                };
                let saved = self.inference_priority;
                if self.mapped_types.contains_key(&source)
                    && self.mapped_types.contains_key(&target)
                {
                    self.inference_priority |= InferencePriority::HOMOMORPHIC_MAPPED_TYPE;
                }
                for info in &target_infos {
                    if inferable {
                        let mut values = Vec::new();
                        for name in &source_names {
                            let key = self.literal_type_of_property(source, name);
                            if !self.is_applicable_index_type(key, info.key) {
                                continue;
                            }
                            let Some(mut value) = self.get_type_of_property_of_type(source, name)
                            else {
                                continue;
                            };
                            let captured_optional = self
                                .anonymous_properties
                                .get(&source)
                                .and_then(|(properties, _)| {
                                    properties.iter().find(|p| p.name == *name)
                                })
                                .map(|property| property.optional);
                            let optional = captured_optional.unwrap_or_else(|| {
                                self.get_property_of_type(source, name)
                                    .is_some_and(|symbol| self.property_is_optional(symbol))
                            });
                            if optional {
                                value = self.remove_missing_or_undefined_type(value);
                            }
                            values.push(value);
                        }
                        for from in &source_infos {
                            if self.is_applicable_index_type(from.key, info.key) {
                                values.push(from.value);
                            }
                        }
                        if !values.is_empty() {
                            let value = self.get_union_type(&values);
                            self.infer_from_types_within(
                                value,
                                info.value,
                                original,
                                parameters,
                                out,
                                depth + 1,
                            );
                        }
                    }
                    if let Some(from) = self.get_applicable_index_info(source, info.key) {
                        self.infer_from_types_within(
                            from.value,
                            info.value,
                            original,
                            parameters,
                            out,
                            depth + 1,
                        );
                    }
                }
                self.inference_priority = saved;
            }
        }
        for kind in
            [crate::signatures::SignatureKind::Call, crate::signatures::SignatureKind::Construct]
        {
            let (Some(target_signatures), Some(source_signatures)) = (
                self.signatures_of_type_kind(target, kind),
                self.signature_shapes_of_type_kind(source, kind),
            ) else {
                continue;
            };
            if source_signatures.is_empty() {
                continue;
            }
            // inferFromSignatures matches the last signatures first, repeating
            // the first source when the target has additional overloads.
            for (index, target_signature) in target_signatures.iter().enumerate() {
                let source_index =
                    (source_signatures.len() + index).saturating_sub(target_signatures.len());
                let Some(t) = self.signature_for_inference(target_signature.clone(), true) else {
                    continue;
                };
                let Some(s) =
                    self.signature_for_inference(source_signatures[source_index].clone(), false)
                else {
                    continue;
                };
                if !s.non_inferrable {
                    self.infer_from_signature_parameters(&s, &t, original, parameters, out, depth);
                }
                // applyToReturnTypes (inference.go:895-907) reads the target
                // first. Parameters<F> has target any, so its source's native
                // return slot must stay lazy while parameter inference runs.
                let target_return = t.predicate.as_ref().and_then(|p| p.r#type).unwrap_or(t.r#type);
                if !self.target_could_contain_parameter(target_return, parameters, &mut Vec::new())
                {
                    continue;
                }
                let Some(s) = self.complete_signature_return(s) else { continue };
                let (source_return, target_return) = s.inference_return_types(&t);
                self.infer_from_types_within(
                    source_return,
                    target_return,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
            }
        }
    }

    /// inferFromMatchingTypes followed by inferToMultipleTypes for a union
    /// (internal/checker/inference.go). Structured matches consume source
    /// constituents before naked variables receive the less specific remainder.
    fn infer_to_union(
        &mut self,
        source: TypeId,
        targets: &[TypeId],
        original: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) {
        use crate::flags::TypeFlags;
        let mut sources = match &self.store.get(source).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![source],
        };
        let mut targets = targets.to_vec();
        for closely in [false, true] {
            let mut matched_sources = Vec::new();
            let mut matched_targets = Vec::new();
            let mut ordered_targets = targets.clone();
            if closely {
                ordered_targets
                    .sort_by_key(|&t| (std::cmp::Reverse(self.inference_type_depth(t, 3)), t));
            }
            for target in ordered_targets {
                for &from in &sources {
                    let matches = if closely {
                        let references = matches!(
                            (self.type_reference_targets.get(&from), self.type_reference_targets.get(&target)),
                            (Some((s, _)), Some((t, _))) if self.binder.merged_symbol(*s) == self.binder.merged_symbol(*t));
                        let objects = self.type_of(from).flags.contains(TypeFlags::OBJECT)
                            && self.type_of(target).flags.contains(TypeFlags::OBJECT)
                            && matches!((&self.store.get(from).data, &self.store.get(target).data),
                                (TypeData::Named { members: Some(s), .. }, TypeData::Named { members: Some(t), .. })
                                    if self.binder.merged_symbol(*s) == self.binder.merged_symbol(*t));
                        references || objects
                    } else {
                        self.get_regular_type_of_literal_type(from)
                            == self.get_regular_type_of_literal_type(target)
                            || self.type_of(target).flags.contains(TypeFlags::STRING)
                                && self.type_of(from).flags.contains(TypeFlags::STRING_LITERAL)
                            || self.type_of(target).flags.contains(TypeFlags::NUMBER)
                                && self.type_of(from).flags.contains(TypeFlags::NUMBER_LITERAL)
                    };
                    if matches {
                        self.infer_from_types_within(
                            from,
                            target,
                            original,
                            parameters,
                            out,
                            depth + 1,
                        );
                        matched_sources.push(from);
                        matched_targets.push(target);
                    }
                }
            }
            sources.retain(|s| !matched_sources.contains(s));
            targets.retain(|t| !matched_targets.contains(t));
        }
        if targets.is_empty() {
            return;
        }
        if sources.is_empty() {
            let saved = self.inference_priority;
            self.inference_priority |= InferencePriority::NAKED_TYPE_VARIABLE;
            let target = self.get_union_type(&targets);
            self.infer_from_types_within(source, target, original, parameters, out, depth + 1);
            self.inference_priority = saved;
            return;
        }
        if let [target] = targets.as_slice() {
            let source = self.get_union_type(&sources);
            self.infer_from_types_within(source, *target, original, parameters, out, depth + 1);
            return;
        }
        let variables: Vec<_> =
            targets.iter().copied().filter(|t| parameters.contains(t)).collect();
        let mut matched = vec![false; sources.len()];
        let mut circular = false;
        for &target in &targets {
            if parameters.contains(&target) {
                continue;
            }
            for (index, &from) in sources.iter().enumerate() {
                let saved = self.inference_observed_priority;
                self.inference_observed_priority = i32::from(InferencePriority::MAX_VALUE.bits());
                self.infer_from_types_within(from, target, original, parameters, out, depth + 1);
                matched[index] |=
                    self.inference_observed_priority == i32::from(self.inference_priority.bits());
                circular |= self.inference_observed_priority == -1;
                self.inference_observed_priority = self.inference_observed_priority.min(saved);
            }
        }
        if let [variable] = variables.as_slice()
            && !circular
        {
            let unmatched: Vec<_> = sources
                .iter()
                .copied()
                .enumerate()
                .filter_map(|(i, t)| (!matched[i]).then_some(t))
                .collect();
            if !unmatched.is_empty() {
                let remainder = self.get_union_type(&unmatched);
                self.infer_from_types_within(
                    remainder,
                    *variable,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
                return;
            }
        }
        let saved = self.inference_priority;
        self.inference_priority |= InferencePriority::NAKED_TYPE_VARIABLE;
        let source = self.get_union_type(&sources);
        for variable in variables {
            self.infer_from_types_within(source, variable, original, parameters, out, depth + 1);
        }
        self.inference_priority = saved;
    }

    /// getTypeDepth (internal/checker/inference.go), bounded generic nesting.
    fn inference_type_depth(&self, t: TypeId, limit: usize) -> usize {
        if limit == 0 {
            return 0;
        }
        if let Some((_, arguments)) = self.type_reference_targets.get(&t) {
            1 + arguments
                .iter()
                .map(|&t| self.inference_type_depth(t, limit - 1))
                .max()
                .unwrap_or(0)
        } else if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            &self.store.get(t).data
        {
            types.iter().map(|&t| self.inference_type_depth(t, limit)).max().unwrap_or(0)
        } else {
            0
        }
    }

    /// `applyToParameterTypes` (`internal/checker/inference.go`), including
    /// fixed tuple-rest positions and the remaining source parameter tuple.
    fn infer_from_signature_parameters(
        &mut self,
        s: &Signature,
        t: &Signature,
        original: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) {
        let saved = (self.inference_contravariant, self.inference_bivariant);
        if self.strict_function_types
            || self.inference_priority.contains(InferencePriority::ALWAYS_STRICT)
        {
            self.inference_contravariant = !self.inference_contravariant;
        }
        self.inference_bivariant |= matches!(
            self.nodes.kind(t.declaration),
            tsr_ast::SyntaxKind::MethodDeclaration
                | tsr_ast::SyntaxKind::MethodSignature
                | tsr_ast::SyntaxKind::Constructor
        );
        self.apply_to_parameter_types(s, t, Some(original), parameters, out, depth);
        (self.inference_contravariant, self.inference_bivariant) = saved;
    }

    /// `applyToParameterTypes` visits inputs without choosing their variance.
    /// Contextual instantiation uses covariant inference, while structural
    /// signature inference supplies its strict-function direction separately.
    fn apply_to_parameter_types(
        &mut self,
        s: &Signature,
        t: &Signature,
        original: Option<TypeId>,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) {
        let source_elements = self.signature_tuple_arguments(s);
        let target_elements = self.signature_tuple_arguments(t);
        let source_start = source_elements.iter().position(|element| element.spread);
        let target_start = target_elements.iter().position(|element| element.spread);
        let target_fixed = target_start.unwrap_or(target_elements.len());
        let paired = if source_start.is_some() {
            target_fixed
        } else {
            source_elements.len().min(target_fixed)
        };
        if let (Some(source_this), Some(target_this)) = (&s.this_parameter, &t.this_parameter) {
            self.infer_from_types_within(
                source_this.r#type,
                target_this.r#type,
                original.unwrap_or(target_this.r#type),
                parameters,
                out,
                depth + 1,
            );
        }
        let source_rest = source_start
            .map(|start| self.normalize_variadic_tuple(source_elements[start..].to_vec(), false));
        for (index, target_element) in target_elements.iter().take(paired).enumerate() {
            let source_type = if source_start.is_none_or(|start| index < start) {
                source_elements[index].r#type
            } else {
                let index_type = self.store.intern_literal(
                    crate::flags::TypeFlags::NUMBER_LITERAL,
                    TypeData::NumberLiteral((index - source_start.unwrap()).to_string()),
                    false,
                );
                self.tuple_index_type(source_rest.unwrap(), index_type, false)
                    .unwrap_or(self.intrinsics.error)
            };
            self.infer_from_types_within(
                source_type,
                target_element.r#type,
                original.unwrap_or(target_element.r#type),
                parameters,
                out,
                depth + 1,
            );
        }
        if let Some(target_start) = target_start {
            // getEffectiveRestType retains a non-tuple operand directly. In
            // particular ...args: P targets P, not a synthetic [...P]; the
            // bare parameter must capture a source union as one candidate.
            let target_rest = t
                .parameters
                .last()
                .filter(|parameter| {
                    parameter.rest
                        && !self.tuple_element_lists.contains_key(&parameter.r#type)
                        && !self.variadic_tuple_elements.contains_key(&parameter.r#type)
                })
                .map_or_else(
                    || {
                        self.normalize_variadic_tuple(
                            target_elements[target_start..].to_vec(),
                            false,
                        )
                    },
                    |parameter| parameter.r#type,
                );
            let source_slice = if let Some(start) = source_start
                && paired > start
            {
                let element = self
                    .tuple_index_type(source_rest.unwrap(), self.intrinsics.number, false)
                    .unwrap_or(self.intrinsics.error);
                let Some(array) = self.global_type_symbol_with_arity("Array", 1) else {
                    return;
                };
                self.create_type_reference(array, vec![element])
            } else if let Some(parameter) = s.parameters.last()
                && parameter.rest
                && paired == s.parameters.len() - 1
                && !self.tuple_element_lists.contains_key(&parameter.r#type)
                && !self.variadic_tuple_elements.contains_key(&parameter.r#type)
            {
                parameter.r#type
            } else {
                // applyToParameterTypes chooses readonly on the source rest
                // tuple itself, preserving labels and the types of its elements.
                let readonly = self.is_const_type_variable(target_rest, 0)
                    && !self.const_context_is_mutable_array_like(target_rest);
                self.normalize_variadic_tuple(source_elements[paired..].to_vec(), readonly)
            };
            self.infer_from_types_within(
                source_slice,
                target_rest,
                original.unwrap_or(target_rest),
                parameters,
                out,
                depth + 1,
            );
        }
    }

    /// The tuple used by `getRestTypeAtPosition` and `applyToParameterTypes`
    /// (`internal/checker/relater.go`, `inference.go`). Tuple rest parameters
    /// contribute their element flags and labels; other rests remain variadic.
    pub(crate) fn signature_tuple_arguments(
        &self,
        signature: &Signature,
    ) -> Vec<crate::tuples::TupleElement> {
        let mut elements = Vec::new();
        for parameter in &signature.parameters {
            if parameter.rest {
                if let Some((types, _)) = self.tuple_element_lists.get(&parameter.r#type) {
                    let mask = self.tuple_optional_masks.get(&parameter.r#type);
                    let labels = self.tuple_labels.get(&parameter.r#type);
                    elements.extend(types.iter().enumerate().map(|(index, &t)| {
                        crate::tuples::TupleElement {
                            r#type: t,
                            spread: false,
                            optional: mask
                                .and_then(|mask| mask.get(index))
                                .copied()
                                .unwrap_or(false),
                            label: labels.and_then(|labels| labels.get(index)).cloned().flatten(),
                        }
                    }));
                    continue;
                }
                if let Some((rest, _)) = self.variadic_tuple_elements.get(&parameter.r#type) {
                    elements.extend(rest.iter().cloned());
                    continue;
                }
            }
            elements.push(crate::tuples::TupleElement {
                r#type: parameter.r#type,
                spread: parameter.rest,
                optional: parameter.optional,
                label: Some(parameter.name.clone()),
            });
        }
        elements
    }

    /// `getBaseSignature` for a source and `getErasedSignature` for a target
    /// (`internal/checker/checker.go`). Source parameters map to constraints
    /// or unknown, with interdependent constraints expanded before erasure.
    pub(crate) fn signature_for_inference(
        &mut self,
        mut signature: Signature,
        erase: bool,
    ) -> Option<Signature> {
        if signature.type_parameters.is_empty() {
            return Some(signature);
        }
        let own = self.type_parameter_types(&signature)?;
        if own.len() != signature.type_parameters.len() {
            return None;
        }
        let names: Vec<_> = signature.type_parameters.iter().map(|p| p.name.clone()).collect();
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        let eraser: Vec<_> =
            own.iter().map(|&parameter| (parameter, self.intrinsics.any)).collect();
        let map = if erase {
            eraser
        } else {
            let mut constraints: Vec<_> = signature
                .type_parameters
                .iter()
                .map(|p| p.constraint.unwrap_or(self.intrinsics.unknown))
                .collect();
            let immediate: Vec<_> = own.iter().copied().zip(constraints.iter().copied()).collect();
            for _ in 1..own.len() {
                for constraint in &mut constraints {
                    *constraint = self.instantiate_type(*constraint, &immediate, &own, &names);
                }
            }
            for constraint in &mut constraints {
                *constraint = self.instantiate_type(*constraint, &eraser, &own, &names);
            }
            own.iter().copied().zip(constraints).collect()
        };
        signature.type_parameters.clear();
        self.instantiate_signature(signature, &map, &own, &names)
    }

    /// `Checker.instantiateType` (`checker.go:22100`) — substitution, over the
    /// shapes this port can rebuild.
    ///
    /// Upstream's mapper walks a structured type: a `TypeReference` carries its
    /// target and `resolvedTypeArguments`, so `instantiateTypeWorker`
    /// (`checker.go:22220`) rebuilds it by mapping the arguments. Here a
    /// reference's payload is a *printed string*, and the pair it was built
    /// from lives in the intern map as a key. [`Checker::type_reference_targets`]
    /// makes that key reachable from the id, and this function is what it is
    /// for.
    ///
    /// The four arms, in order:
    ///
    /// 1. **Identity** — `id` is a mapped type parameter, so it becomes its
    ///    image. `T` with `T := number` is `number`.
    /// 2. **Unchanged** — `id` mentions no type parameter of this signature, so
    ///    substitution is the identity on it. Upstream reaches the same answer
    ///    through `couldContainTypeVariables`.
    /// 3. **Reference** — `id` came from `create_type_reference`, so its
    ///    arguments are substituted and the reference rebuilt through the same
    ///    function. Rebuilding through it rather than around it is what keeps
    ///    `Array<number>` and `number[]` one interned type.
    /// 4. **Union** — constituents substituted, then [`Checker::get_union_type`]
    ///    (`unions.rs`), because a union of substituted members may collapse
    ///    (`T | string` with `T := string`) and only that function knows how.
    ///
    /// Anything else answers **`errorType`**, and that includes an *unmapped*
    /// type parameter: it falls past arm 1, mentions itself so it fails arm 2,
    /// and is neither a reference nor a union. A tuple and a function type land
    /// here too — they are a `TypeData::Named` or `TypeData::Anonymous` holding
    /// text, built without an intern key, so there is no pair to reverse.
    ///
    /// # The recursion limit is upstream's, and it is not a stack guard
    ///
    /// `instantiateTypeWithAlias` (`checker.go:22111`) stops at an
    /// `instantiationDepth` of 100 or an `instantiationCount` of 5,000,000 and
    /// answers `errorType`, because an infinite generic type — `interface
    /// List<T> { next: List<List<T>> }` — perpetually mints new type identities
    /// and no cache can terminate it. Until member instantiation existed
    /// (`bd tsr-4qx`) every argument reached here was one a *written* type node
    /// already produced, so the recursion was bounded by the source nesting and
    /// this function carried no limit, deliberately (`bd tsr-el3.2`). Member
    /// instantiation is what breaks that bound, and the guard is a **hard
    /// prerequisite** for it: the failure mode of landing members first is a
    /// hung corpus run, not a wrong number.
    ///
    /// Upstream's guard sits after its `couldContainTypeVariables` early-out
    /// and before the worker; the guard here sits after arm 2, which is the
    /// same position. The count divergence — per checker rather than per
    /// statement — is recorded on the field
    /// ([`Checker::instantiation_count`](crate::checker)). Upstream reports
    /// `Type_instantiation_is_excessively_deep_and_possibly_infinite`; this
    /// port has no diagnostics, so the `errorType` is the whole observable.
    pub fn instantiate_type(
        &mut self,
        id: TypeId,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> TypeId {
        if let Some(&(_, image)) = map.iter().find(|&&(from, _)| from == id) {
            return image;
        }
        // A return mapper may inspect the original signature only after its
        // declaration-owned lazy return completes. Active/unsupported originals
        // decline before a no-type-parameter decision or mapper image is stored.
        if !self.complete_pending_signature_returns_of_type(id) {
            return self.intrinsics.error;
        }
        if !self.mentions_type_parameter(id, parameters, names) {
            return id;
        }
        if self.instantiation_depth == 100 || self.instantiation_count >= 5_000_000 {
            return self.intrinsics.error;
        }
        // §107: the print-clone road substitutes identity for FOREIGN
        // (enclosing) type parameters; real instantiation keeps the
        // deliberate unmapped-parameter refusal below.
        if self.identity_unmapped_type_parameters
            && self.store.get(id).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER)
        {
            return id;
        }
        self.instantiation_count += 1;
        self.instantiation_depth += 1;
        let result = self.instantiate_type_worker(id, map, parameters, names);
        self.instantiation_depth -= 1;
        result
    }

    /// `isNoInferTargetType` (`checker.go:27401`): whether `getNoInferType`
    /// keeps a `NoInfer<T>` wrapper around `t` — "a more conservative and
    /// predictable form of couldContainTypeVariables". A `NoInfer` reference
    /// (this port's substitution type with an `unknown` constraint) is not a
    /// target itself; TSR has no other substitution types.
    pub(crate) fn is_no_infer_target_type(&self, t: TypeId) -> bool {
        use crate::flags::TypeFlags;
        if self.no_infer_base_type(t).is_some() {
            return false;
        }
        let ty = self.store.get(t);
        match &ty.data {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. }
                if ty.flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION) =>
            {
                return types.iter().any(|&member| self.is_no_infer_target_type(member));
            }
            _ => {}
        }
        if ty.flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION) {
            return false;
        }
        ty.flags.contains(TypeFlags::OBJECT) && !self.is_empty_anonymous_object_type(t)
            || ty.flags.intersects(TypeFlags::INSTANTIABLE - TypeFlags::SUBSTITUTION)
                && !self.is_pattern_template(t)
    }

    /// getPropertyTypeForIndexType's not-found leg (`checker.go:27085`) on
    /// inputs this port can certify complete: a literal key, a non-generic
    /// object whose property list is complete and lacks the key, no
    /// property through the apparent type, and no index signature at all.
    fn indexed_access_is_certainly_absent(&mut self, object: TypeId, index: TypeId) -> bool {
        use crate::flags::TypeFlags;
        let Some(name) = self.property_name_from_index(index) else { return false };
        let apparent = self.apparent_type(object);
        let flags = self.store.get(apparent).flags;
        if !flags.contains(TypeFlags::OBJECT)
            || flags
                .intersects(TypeFlags::UNION | TypeFlags::INTERSECTION | TypeFlags::INSTANTIABLE)
            || self.mapped_types.contains_key(&apparent)
            || self.unresolved_types.contains(&apparent)
        {
            return false;
        }
        // An object-literal or type-literal image carries its complete
        // captured property list (`getPropertiesOfType`).
        (self.anonymous_properties.contains_key(&apparent)
            || self.declared_members_are_complete(apparent))
            && !self.property_names_of(apparent).contains(&name)
            && self.get_type_of_property_of_type(apparent, &name).is_none()
            && self.get_index_infos_of_type(apparent).is_some_and(|infos| infos.is_empty())
    }

    /// The recursive body of [`Checker::instantiate_type`] — arms 3 and 4 and
    /// the fallback — split out so the depth counter cannot be unbalanced by an
    /// early return. `instantiateTypeWorker` (`checker.go:22220`), for the
    /// shapes this port can rebuild.
    fn instantiate_type_worker(
        &mut self,
        id: TypeId,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> TypeId {
        let error = self.intrinsics.error;
        if self.mapped_conditionals.contains_key(&id) {
            return self.instantiate_mapped_conditional(id, map, parameters, names);
        }
        if self.conditional_inference_nodes.contains_key(&id) {
            return self.instantiate_conditional_node(id, map, parameters, names);
        }
        if let Some((symbol, target)) = self.string_mapping_types.get(&id).copied() {
            let target = self.instantiate_type(target, map, parameters, names);
            return self.get_string_mapping_type(symbol, target);
        }
        if let Some(parts) = self.template_literal_parts.get(&id).cloned() {
            let types: Vec<_> = parts
                .types
                .into_iter()
                .map(|ty| self.instantiate_type(ty, map, parameters, names))
                .collect();
            return self.get_template_literal_type(&parts.texts, &types);
        }
        if let Some(&operand) = self.deferred_keyof_operands.get(&id) {
            let operand = self.instantiate_type(operand, map, parameters, names);
            return self.resolved_keyof_type(operand).unwrap_or(error);
        }
        if let Some((object, index, include_undefined)) =
            self.deferred_indexed_access_types.get(&id).copied()
            && !self.type_reference_targets.contains_key(&id)
        {
            let object = self.instantiate_type(object, map, parameters, names);
            let index = self.instantiate_type(index, map, parameters, names);
            if object == error || index == error {
                return error;
            }
            if let Some(resolved) =
                self.resolved_indexed_access_type(object, index, include_undefined)
            {
                return resolved;
            }
            // getIndexedAccessTypeEx (`checker.go:26927`): with no access
            // node, a lookup getIndexedAccessTypeOrUndefined answers nil for
            // instantiates to `unknownType`, not `errorType`. Only a certified absence
            // (getPropertyTypeForIndexType's not-found leg) is that nil here;
            // every other `None` is this port's "not computed".
            if self.indexed_access_is_certainly_absent(object, index) {
                return self.intrinsics.unknown;
            }
            return error;
        }
        if let Some((symbol, arguments)) = self.type_reference_targets.get(&id).cloned() {
            let mut substituted = Vec::with_capacity(arguments.len());
            for argument in arguments {
                let image = self.instantiate_type(argument, map, parameters, names);
                if image == error {
                    return error;
                }
                substituted.push(image);
            }
            // instantiateTypeWorker's substitution arm (`checker.go:22277`):
            // a `NoInfer<T>` instantiates to `getNoInferType` of the
            // instantiated base, which drops the wrapper unless the base
            // could still contain type variables.
            if let [base] = substituted.as_slice()
                && self.is_no_infer_alias(symbol)
                && !self.is_no_infer_target_type(*base)
            {
                return *base;
            }
            // §91 (`checker-notes-narrow.md`): a CONDITIONAL alias body
            // evaluates at the rebuild when its keys are computable — the
            // resolved branch does not carry the alias
            // (`getConditionalTypeInstantiation`; chain1/chain3 pin the
            // plain/conditional split). Any refusal falls back to the name.
            if let Some(evaluated) = self.evaluate_conditional_alias(symbol, &substituted, None) {
                return evaluated;
            }
            // §463: the GLOBAL `Awaited<T>` alias at a CONCRETE argument IS
            // the awaited type — upstream evaluates the alias's conditional
            // body, whose spec purpose is agreeing with `getAwaitedType` on
            // every non-generic input, and `createAwaitedTypeIfNeeded`
            // (`checker.go:31410`) only mints the alias form for type
            // variables in the first place. No-alias awaiting now preserves
            // generic parameter identity for async aggregation, so this
            // concrete shortcut must explicitly retain a generic alias.
            // This is what turns
            // `resolve<T>(value: T): Promise<Awaited<T>>` instantiated at
            // `string` into `Promise<string>`.
            if let [argument] = substituted.as_slice()
                && self.global_type_symbol("Awaited").is_some_and(|awaited| {
                    self.binder.merged_symbol(awaited) == self.binder.merged_symbol(symbol)
                })
                && !self.spread_generic_flags(*argument, &mut Vec::new()).0
                && let Some(awaited) = self.awaited_type_no_alias(*argument)
            {
                return awaited;
            }
            // §136: a rebuild keeps the source reference's written display
            // arity — the spelling survives instantiation.
            let display = self.reference_display_arity.get(&id).copied();
            let rebuilt = self.create_type_reference_with_display(symbol, substituted, display);
            // instantiateTypeWorker retains an instantiated callable object's
            // signatures (checker.go). Alias references preserve their name,
            // but a newly rebuilt argument list must also retain that callable
            // body; otherwise Mapper<string, unknown> loses its context.
            if self.alias_named_signature_types.contains(&id)
                && !self.signature_types.contains_key(&rebuilt)
                && let Some(signatures) = self.signature_types.get(&id).cloned()
            {
                let instantiated: Option<Vec<_>> = signatures
                    .into_iter()
                    .map(|signature| {
                        self.instantiate_signature_with_fresh_parameters(
                            signature, map, parameters, names,
                        )
                    })
                    .collect();
                if let Some(signatures) = instantiated {
                    self.signature_types.insert(rebuilt, signatures);
                    self.alias_named_signature_types.insert(rebuilt);
                }
            }
            return rebuilt;
        }
        if self.mapped_types.contains_key(&id) {
            return self.instantiate_mapped_type(id, map, parameters, names);
        }
        if let TypeData::Union { types, .. } = &self.store.get(id).data {
            let types = types.clone();
            let mut substituted = Vec::with_capacity(types.len());
            for constituent in types {
                let image = self.instantiate_type(constituent, map, parameters, names);
                if image == error {
                    return error;
                }
                substituted.push(image);
            }
            return self.get_union_type(&substituted);
        }
        // instantiateTypeWorker maps intersection constituents just as it
        // maps union constituents; contextual mapped templates depend on it.
        if let TypeData::Intersection { types, symbol, .. } = self.store.get(id).data.clone() {
            let mut substituted = Vec::with_capacity(types.len());
            for constituent in types {
                let image = self.instantiate_type(constituent, map, parameters, names);
                if image == error {
                    return error;
                }
                substituted.push(image);
            }
            return self.get_intersection_type(&substituted, symbol);
        }
        if let Some(instantiated) = self.instantiate_type_literal(id, map, parameters, names) {
            return instantiated;
        }
        if self.signature_types.contains_key(&id) {
            return self.instantiate_signature_type(id, map, parameters, names);
        }
        if let Some((properties, _)) = self.anonymous_properties.get(&id).cloned() {
            return self.instantiate_anonymous_properties(id, properties, map, parameters, names);
        }
        // A variadic tuple normalizes after its type arguments are mapped
        // (`instantiateTypeWorker` -> `createNormalizedTupleType`, checker.go).
        // Map the captured types rather than re-resolving syntax: a conditional
        // alias's inferred bindings may no longer be in scope at this point.
        if let Some((mut elements, readonly)) = self.variadic_tuple_elements.get(&id).cloned() {
            for element in &mut elements {
                element.r#type = self.instantiate_type(element.r#type, map, parameters, names);
                if element.r#type == error {
                    return error;
                }
            }
            return self.normalize_variadic_tuple(elements, readonly);
        }
        // Arm 6 (§37, `checker-notes-narrow.md`): a tuple carries its element
        // ids in `tuple_element_lists`, so it substitutes element-wise and
        // re-mints through the same constructor.
        if let Some((elements, readonly)) = self.tuple_element_lists.get(&id).cloned() {
            let mut substituted = Vec::with_capacity(elements.len());
            for element in elements {
                let image = self.instantiate_type(element, map, parameters, names);
                if image == error {
                    return error;
                }
                substituted.push(image);
            }
            // `instantiateTypeWorker` rebuilds a reference against the original
            // tuple target: optionality and labels belong to that target, not
            // to the substituted element types (`checker.go`).
            if let Some(mask) = self.tuple_optional_masks.get(&id).cloned() {
                let labels = self
                    .tuple_labels
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| vec![None; substituted.len()]);
                let elements: Vec<_> = substituted.into_iter().zip(mask).collect();
                return self.create_optional_tuple_type(&elements, &labels, readonly);
            }
            return self.create_tuple_type(substituted, readonly);
        }
        error
    }

    /// `instantiateAnonymousType` and `instantiateSymbol` (checker.go), for
    /// captured anonymous objects. Keep member flags and index provenance while
    /// mapping semantic values for subsequent reads and instantiations.
    fn instantiate_anonymous_properties(
        &mut self,
        id: TypeId,
        mut properties: Vec<crate::objects::AnonymousProperty>,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> TypeId {
        let key = (id, map.to_vec());
        if let Some(&cached) = self.instantiated_objects.get(&key) {
            return cached;
        }
        let TypeData::Named { members: owner, .. } = self.store.get(id).data else {
            return self.intrinsics.error;
        };
        let mut rendered = Vec::with_capacity(properties.len());
        let mut indexes = self.object_literal_index_infos.get(&id).cloned().unwrap_or_default();
        for index in &mut indexes {
            index.key = self.instantiate_type(index.key, map, parameters, names);
            index.value = self.instantiate_type(index.value, map, parameters, names);
            if index.key == self.intrinsics.error || index.value == self.intrinsics.error {
                return self.intrinsics.error;
            }
            let Some(members) = self.index_info_members(index) else {
                return self.intrinsics.error;
            };
            rendered.extend(members);
        }
        for property in &mut properties {
            let original = property.r#type;
            property.r#type = self.instantiate_type(original, map, parameters, names);
            if let Some(write) = &mut property.accessor_write {
                write.r#type = self.instantiate_type(write.r#type, map, parameters, names);
                if write.r#type == self.intrinsics.error {
                    return self.intrinsics.error;
                }
            }
            if property.r#type == self.intrinsics.error {
                return self.intrinsics.error;
            }
            if property.r#type != original {
                property.printed_type = self.type_to_string(property.r#type);
            }
            let Some(members) = self.anonymous_property_members(std::slice::from_ref(property))
            else {
                return self.intrinsics.error;
            };
            rendered.extend(members);
        }
        let text = crate::objects::render_object_type(&rendered);
        let minted = self.store.new_named(crate::flags::TypeFlags::OBJECT, text, owner);
        self.anonymous_properties.insert(minted, (properties, true));
        self.object_literal_index_infos.insert(minted, indexes);
        self.instantiated_objects.insert(key, minted);
        minted
    }

    /// Arm 5: a baked signature type, rebuilt with substituted parts.
    ///
    /// Ported from `instantiateSignature` (`checker.go:19640`) over this port's
    /// [`Signature`], for the types the two bake sites recorded in
    /// [`Checker::signature_types`](crate::checker) — `bd tsr-0hc`. Upstream
    /// clones the signature with a merged mapper and instantiates its types
    /// lazily; here every carried [`TypeId`] is substituted eagerly and the
    /// text re-rendered through the same code that rendered the original, so
    /// the instantiated form can only print what the proven renderer prints.
    ///
    /// A signature's **own** type parameters (`then<TResult1 = T>`) are
    /// distinct types from the receiver's, miss the map by identity, and
    /// survive unrenamed — upstream's behaviour. A part that cannot be
    /// substituted refuses the whole type: `errorType`, a gap.
    ///
    /// The minted type keeps the *uninstantiated* symbol so the `signature`
    /// bit and union parenthesisation survive; `resolve_call_signature`
    /// (`crate::calls`) must therefore gap on it — reading the symbol's
    /// declarations back would answer the uninstantiated return type, a wrong
    /// line. That guard tests [`Checker::is_instantiated_signature_type`].
    fn instantiate_signature_type(
        &mut self,
        id: TypeId,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> TypeId {
        let error = self.intrinsics.error;
        let key = (id, map.to_vec());
        if let Some(&cached) = self.instantiated_signatures.get(&key) {
            return cached;
        }
        let signatures = self.signature_types.get(&id).cloned().unwrap_or_default();
        let mut instantiated = Vec::with_capacity(signatures.len());
        for signature in signatures {
            let Some(image) =
                self.instantiate_signature_with_fresh_parameters(signature, map, parameters, names)
            else {
                return error;
            };
            instantiated.push(image);
        }
        // §90.1 (`checker-notes-narrow.md`): the TEXT renders from a
        // print-renamed clone — upstream's `instantiateSignature` clones
        // retained own type parameters (`checker.go:19654`) and the node
        // builder prints the clones disambiguated (`r_1`). The rename is
        // print-only: `signature_types` keeps the un-renamed signatures,
        // because call-side inference identifies own parameters by the
        // declaration's TypeIds and a semantic rename severs that link
        // (measured: 182 conversions lost).
        let printed: Vec<Signature> = instantiated
            .iter()
            .map(|signature| self.rename_own_type_parameters_for_print(signature.clone()))
            .collect();
        // Re-rendered exactly as the bake sites render: one signature is a
        // `FunctionTypeNode`, several are the type-literal form. An empty list
        // is unreachable (neither site records one) and refuses.
        let mut properties = self
            .anonymous_properties
            .get(&id)
            .map(|(properties, _)| properties.clone())
            .unwrap_or_default();
        for property in &mut properties {
            property.r#type = self.instantiate_type(property.r#type, map, parameters, names);
            if let Some(write) = &mut property.accessor_write {
                write.r#type = self.instantiate_type(write.r#type, map, parameters, names);
                if write.r#type == self.intrinsics.error {
                    return self.intrinsics.error;
                }
            }
            if property.r#type == error {
                return error;
            }
            property.printed_type = self.type_to_string(property.r#type);
        }
        let (text, signature_node) = match printed.as_slice() {
            [] => return error,
            [signature] if properties.is_empty() => (self.signature_to_string(signature), true),
            signatures => {
                let mut members: Vec<_> = signatures
                    .iter()
                    .map(|signature| crate::objects::Member::Signature {
                        printed: crate::objects::signature_member_text(self, signature),
                    })
                    .collect();
                members.extend(crate::callable_expandos::property_members(&properties));
                (crate::objects::render_object_type(&members), false)
            }
        };
        let TypeData::Anonymous { symbol, .. } = self.store.get(id).data else {
            return error;
        };
        // §90.1's second half: when the rename changed the print, the §89
        // keep-text set stops `type_to_string_at`'s composite re-render from
        // rebuilding the STORED (un-renamed) structure at assertion sites —
        // the same trap §89 closed for alias names, one bake further in.
        let renamed_print = printed.iter().zip(&instantiated).any(|(printed, stored)| {
            printed
                .type_parameters
                .iter()
                .map(|p| &p.name)
                .ne(stored.type_parameters.iter().map(|p| &p.name))
        });
        let minted =
            self.store.new_anonymous(crate::flags::TypeFlags::OBJECT, text, symbol, signature_node);
        if !properties.is_empty() {
            self.anonymous_properties.insert(minted, (properties, true));
        }
        if renamed_print {
            self.alias_named_signature_types.insert(minted);
        }
        // Recorded in `signature_types` too, so an instantiated signature can
        // be instantiated again — `C<T>` inside `D<U>` reaches that.
        self.signature_types.insert(minted, instantiated);
        let mapper = if let Some(previous) = self.instantiated_signature_mappers.get(&id).cloned() {
            previous
                .into_iter()
                .map(|(source, image)| {
                    (source, self.instantiate_type(image, map, parameters, names))
                })
                .collect()
        } else {
            map.to_vec()
        };
        self.instantiated_signature_mappers.insert(minted, mapper);
        self.instantiated_signatures.insert(key, minted);
        self.minted_signature_types.insert(minted);
        minted
    }

    /// §107: push a signature's own (post-rename name, symbol) pairs onto
    /// the render scope; the caller truncates back to its saved depth.
    pub(crate) fn push_render_type_parameter_scope(
        &mut self,
        signature: &crate::signatures::Signature,
    ) {
        let declarations = match self.node_map.get(signature.declaration) {
            Some(Node::FunctionDeclaration(node)) => node.type_parameters,
            Some(Node::FunctionExpression(node)) => node.type_parameters,
            Some(Node::ArrowFunction(node)) => node.type_parameters,
            Some(Node::MethodDeclaration(node)) => node.type_parameters,
            Some(Node::MethodSignatureDeclaration(node)) => node.type_parameters,
            Some(Node::CallSignatureDeclaration(node)) => node.type_parameters,
            Some(Node::ConstructSignatureDeclaration(node)) => node.type_parameters,
            Some(Node::FunctionTypeNode(node)) => node.type_parameters,
            _ => return,
        };
        for (parameter, declaration) in signature.type_parameters.iter().zip(declarations) {
            if let Some(symbol) = declaration.node_id.and_then(|id| self.binder.symbol_of(id)) {
                self.render_type_parameter_scope.push((parameter.name.clone(), symbol));
            }
        }
    }

    /// §102's print-only clone, the DECODED two-mechanism form
    /// (`promisePermutations` lines 6/15/24/33/62 are the proof set):
    ///
    /// A shadowed parameter uses the shared node-builder allocation, skipping
    /// inherited names and names that resolve to other parameter symbols.
    /// The substitution below remains a print-only clone; stored signatures
    /// and their parameter identities are untouched.
    ///
    /// The site's own signature always prints plain — its name resolves to
    /// itself. Falls back unchanged where identities cannot be established.
    pub(crate) fn rename_type_parameters_for_site(
        &mut self,
        signature: crate::signatures::Signature,
        reference: NodeId,
        claimed: &mut rustc_hash::FxHashSet<String>,
    ) -> crate::signatures::Signature {
        if signature.type_parameters.is_empty() {
            for parameter in &signature.type_parameters {
                claimed.insert(parameter.name.clone());
            }
            return signature;
        }
        let Some(own) = self.type_parameter_types(&signature) else {
            for parameter in &signature.type_parameters {
                claimed.insert(parameter.name.clone());
            }
            return signature;
        };
        if own.len() != signature.type_parameters.len() {
            return signature;
        }
        // Own parameter SYMBOLS, via the declaration — the identity the
        // shadow test compares against.
        let declarations = match self.node_map.get(signature.declaration) {
            Some(Node::FunctionDeclaration(node)) => node.type_parameters,
            Some(Node::FunctionExpression(node)) => node.type_parameters,
            Some(Node::ArrowFunction(node)) => node.type_parameters,
            Some(Node::MethodDeclaration(node)) => node.type_parameters,
            Some(Node::MethodSignatureDeclaration(node)) => node.type_parameters,
            Some(Node::CallSignatureDeclaration(node)) => node.type_parameters,
            Some(Node::ConstructSignatureDeclaration(node)) => node.type_parameters,
            Some(Node::FunctionTypeNode(node)) => node.type_parameters,
            _ => return signature,
        };
        let mut map = Vec::new();
        let mut renames: Vec<Option<String>> = Vec::with_capacity(own.len());
        for ((parameter, &own_type), declaration) in
            signature.type_parameters.iter().zip(&own).zip(declarations)
        {
            let own_symbol = declaration.node_id.and_then(|id| self.binder.symbol_of(id));
            let render_shadow = own_symbol.is_some_and(|own_symbol| {
                self.render_type_parameter_scope
                    .iter()
                    .rev()
                    .find(|(name, _)| *name == parameter.name)
                    .is_some_and(|&(_, symbol)| symbol != own_symbol)
            });
            let shadowed = render_shadow
                || own_symbol.is_some_and(|own_symbol| {
                    self.binder
                        .resolve_name(
                            self.nodes,
                            self.node_map,
                            reference,
                            &parameter.name,
                            tsr_binder::SymbolFlags::TYPE,
                        )
                        .is_some_and(|found| {
                            found != own_symbol
                                && self
                                    .binder
                                    .symbols()
                                    .get(found)
                                    .flags
                                    .contains(tsr_binder::SymbolFlags::TYPE_PARAMETER)
                        })
                });
            // SHADOW ONLY: `underscoreTest1:3229/3233/3237` print [T,T_1],
            // [T_1,T] and [T_1,T_1] per site, and asyncFunctionReturnType
            // holds ZERO renames at neutral sites — the byText half does not
            // exist in this corpus and regressed 763 lines when built.
            let fresh_name = shadowed.then(|| {
                self.allocate_type_parameter_name(own_type, own_symbol.unwrap(), reference)
            });
            if let Some(fresh_name) = fresh_name {
                let fresh = self.store.new_named(
                    crate::flags::TypeFlags::TYPE_PARAMETER,
                    fresh_name.clone(),
                    None,
                );
                map.push((own_type, fresh));
                renames.push(Some(fresh_name));
            } else {
                renames.push(None);
            }
        }
        if map.is_empty() {
            return signature;
        }
        let names: Vec<&str> =
            signature.type_parameters.iter().map(|parameter| parameter.name.as_str()).collect();
        let saved = self.identity_unmapped_type_parameters;
        self.identity_unmapped_type_parameters = true;
        let instantiated = self.instantiate_signature(signature.clone(), &map, &own, &names);
        self.identity_unmapped_type_parameters = saved;
        let Some(mut instantiated) = instantiated else {
            return signature;
        };
        for (parameter, rename) in instantiated.type_parameters.iter_mut().zip(renames) {
            if let Some(fresh_name) = rename {
                parameter.name = fresh_name;
            }
        }
        instantiated
    }

    /// §90.1's print-only clone: own type parameters respelled `name_1` and
    /// every occurrence substituted to a fresh mint carrying the new text.
    /// Falls back to the input unchanged when the own parameters cannot be
    /// identified or any part refuses — an un-renamed print is the old
    /// behaviour, not an error.
    fn rename_own_type_parameters_for_print(&mut self, signature: Signature) -> Signature {
        if signature.type_parameters.is_empty() {
            return signature;
        }
        // The gate is EMPIRICAL, not upstream-derived: the corpus renames
        // instantiated own parameters only when the signature lives in a TYPE
        // ALIAS body AND its return references that same alias — the
        // recursive-container print (`longObjectInstantiationChain2`'s
        // `Type<t>` member returning `Type<merge<t, r>>`). Interface members
        // keep plain names (the promise family — 1,276 R→W measured on the
        // alias-less gate), and so do alias members returning OTHER aliases
        // (`nonInferrableTypePropagation1` — 4 R→W measured on the
        // alias-only gate). Upstream's `typeParameterToName` byText/shadow
        // mechanics (`nodebuilderimpl.go:1404`) need a print-context study
        // to port faithfully — the §20.1 refusal's territory.
        let mut containing_alias = None;
        let mut current = self.nodes.parent(signature.declaration);
        while let Some(id) = current {
            if matches!(self.nodes.kind(id), tsr_ast::SyntaxKind::TypeAliasDeclaration) {
                containing_alias = self.binder.symbol_of(id);
                break;
            }
            current = self.nodes.parent(id);
        }
        let Some(containing_alias) = containing_alias else { return signature };
        let returns_container = self
            .type_reference_targets
            .get(&signature.r#type)
            .is_some_and(|(target, _)| *target == containing_alias);
        if !returns_container {
            return signature;
        }
        // A METHOD member (`pipe<A, B>(...): Thing<B>`) keeps plain names in
        // the same recursive-container shape (`nonInferrableTypePropagation1`,
        // 4 R→W measured); only the property-typed FunctionTypeNode form
        // renames in the corpus.
        if !matches!(self.nodes.kind(signature.declaration), tsr_ast::SyntaxKind::FunctionType) {
            return signature;
        }
        let Some(own) = self.type_parameter_types(&signature) else { return signature };
        if own.len() != signature.type_parameters.len() {
            return signature;
        }
        let mut map = Vec::with_capacity(own.len());
        let mut fresh_names = Vec::with_capacity(own.len());
        for (parameter, &own_type) in signature.type_parameters.iter().zip(&own) {
            let fresh_name = format!("{}_1", parameter.name);
            let fresh = self.store.new_named(
                crate::flags::TypeFlags::TYPE_PARAMETER,
                fresh_name.clone(),
                None,
            );
            map.push((own_type, fresh));
            fresh_names.push(fresh_name);
        }
        let names: Vec<&str> =
            signature.type_parameters.iter().map(|parameter| parameter.name.as_str()).collect();
        let Some(mut renamed) = self.instantiate_signature(signature.clone(), &map, &own, &names)
        else {
            return signature;
        };
        for (parameter, fresh_name) in renamed.type_parameters.iter_mut().zip(fresh_names) {
            parameter.name = fresh_name;
        }
        renamed
    }

    /// `instantiateSignatureEx` (checker.go), using an object reference's
    /// mapper while preserving the signature's own type parameters.
    pub(crate) fn instantiate_signature_for_reference(
        &mut self,
        receiver: TypeId,
        signature: Signature,
    ) -> Option<Signature> {
        let Some((symbol, arguments)) = self.type_reference_targets.get(&receiver).cloned() else {
            return Some(signature);
        };
        let parameters = self.local_type_parameter_types_of(symbol)?;
        if parameters.len() != arguments.len() {
            return None;
        }
        let map: Vec<_> =
            parameters.iter().zip(arguments).map(|((id, _), argument)| (*id, argument)).collect();
        let ids: Vec<_> = parameters.iter().map(|(id, _)| *id).collect();
        let names: Vec<_> = parameters.iter().map(|(_, name)| name.as_str()).collect();
        self.instantiate_signature_with_fresh_parameters(signature, &map, &ids, &names)
    }

    /// instantiateSignatureEx with retained type parameters (checker.go).
    /// Own parameters map to fresh identities before applying the outer map.
    pub(crate) fn instantiate_signature_with_fresh_parameters(
        &mut self,
        mut signature: Signature,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> Option<Signature> {
        if signature.type_parameters.is_empty() {
            return self.instantiate_signature(signature, map, parameters, names);
        }
        let own = self.type_parameter_types(&signature)?;
        let mut combined = Vec::with_capacity(own.len() + map.len());
        let mut sources = own.clone();
        let mut source_names: Vec<_> =
            signature.type_parameters.iter().map(|p| p.name.clone()).collect();
        for (parameter, &original) in signature.type_parameters.iter_mut().zip(&own) {
            let fresh = self.store.new_named(
                crate::flags::TypeFlags::TYPE_PARAMETER,
                parameter.name.clone(),
                None,
            );
            if let Some(&symbol) = self.type_parameter_symbols.get(&original) {
                self.type_parameter_symbols.insert(fresh, symbol);
            }
            parameter.resolved_type = Some(original);
            combined.push((original, fresh));
        }
        combined.extend_from_slice(map);
        sources.extend_from_slice(parameters);
        source_names.extend(names.iter().map(|name| (*name).to_owned()));
        for &(original, fresh) in &combined[..own.len()] {
            self.instantiated_type_parameters.insert(
                fresh,
                InstantiatedTypeParameter {
                    target: original,
                    map: combined.clone(),
                    parameters: sources.clone(),
                    names: source_names.clone(),
                },
            );
        }
        let names: Vec<_> = source_names.iter().map(String::as_str).collect();
        self.instantiate_signature(signature, &combined, &sources, &names)
    }

    /// One signature with every carried type substituted, or `None` when any
    /// part refuses.
    pub(crate) fn instantiate_signature(
        &mut self,
        mut signature: Signature,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> Option<Signature> {
        let target = std::sync::Arc::new(signature.clone());
        let error = self.intrinsics.error;
        let substitute = |checker: &mut Self, id: TypeId| -> Option<TypeId> {
            let image = checker.instantiate_type(id, map, parameters, names);
            (image != error).then_some(image)
        };
        for parameter in &mut signature.type_parameters {
            if let Some(id) = parameter.resolved_type {
                parameter.resolved_type = Some(substitute(self, id)?);
            }
            if let Some(constraint) = parameter.constraint {
                let image = substitute(self, constraint)?;
                if image != constraint {
                    parameter.written_constraint = None;
                }
                parameter.constraint = Some(image);
            }
            if let Some(default) = parameter.default {
                parameter.default = Some(substitute(self, default)?);
            }
        }
        if let Some(this_parameter) = &mut signature.this_parameter {
            this_parameter.r#type = substitute(self, this_parameter.r#type)?;
        }
        for parameter in &mut signature.parameters {
            let image = substitute(self, parameter.r#type)?;
            // Upstream's node reuse is conditional on the written node still
            // denoting the current type (`tryReuseExistingTypeNode`); once
            // substitution changes the type, the written `typeof a` text is no
            // longer its print and must not survive the instantiation.
            if image != parameter.r#type {
                parameter.written_text = None;
            }
            parameter.r#type = image;
        }
        let image = substitute(self, signature.r#type)?;
        if image != signature.r#type {
            signature.written_return = None;
        }
        signature.r#type = image;
        // `instantiateTypePredicate` (`relater.go:2101`) substitutes the
        // predicate's type and leaves its kind and parameter name alone. The
        // predicate is carried as a `TypeId` rather than as rendered text
        // precisely so this is a substitution and not a discard — see
        // [`crate::signatures::TypePredicate`].
        if let Some(predicate) = &mut signature.predicate
            && let Some(id) = predicate.r#type
        {
            predicate.r#type = Some(substitute(self, id)?);
        }
        signature.target = Some(target);
        Some(signature)
    }

    /// Whether `id` was minted by [`Checker::instantiate_signature_type`] —
    /// the guard `resolve_call_signature` (`crate::calls`) gaps on.
    pub(crate) fn is_instantiated_signature_type(&self, id: TypeId) -> bool {
        self.minted_signature_types.contains(&id)
    }

    /// The [`TypeId`] of each of a signature's own type parameters, in order.
    ///
    /// Ported from `Checker.getTypeParametersForTypeAndSymbol` (`checker.go`)
    /// for the function-like half: the declaration's `typeParameters` nodes,
    /// each asked for the type its symbol declares. Going through the
    /// **declaration** rather than matching [`crate::signatures::TypeParameter`]
    /// by name is what makes the identity exact — two type parameters can print
    /// `T` and be different types, and a nested generic makes that reachable.
    ///
    /// `None` when the declaration is not function-like or any type parameter
    /// has no symbol, which keeps a partial map from producing a partial
    /// substitution.
    pub(crate) fn type_parameter_types(&mut self, signature: &Signature) -> Option<Vec<TypeId>> {
        if let Some(types) = signature
            .type_parameters
            .iter()
            .map(|parameter| parameter.resolved_type)
            .collect::<Option<Vec<_>>>()
        {
            return Some(types);
        }
        let declarations = match self.node_map.get(signature.declaration)? {
            Node::FunctionDeclaration(node) => node.type_parameters,
            Node::FunctionExpression(node) => node.type_parameters,
            Node::ArrowFunction(node) => node.type_parameters,
            Node::MethodDeclaration(node) => node.type_parameters,
            Node::MethodSignatureDeclaration(node) => node.type_parameters,
            Node::CallSignatureDeclaration(node) => node.type_parameters,
            Node::ConstructSignatureDeclaration(node) => node.type_parameters,
            Node::FunctionTypeNode(node) => node.type_parameters,
            // §162 (`checker-notes-narrow.md`): a CONSTRUCTOR's type
            // parameters are its CLASS's — upstream reaches them through
            // `declaration.Parent.Symbol()` (`checker.go:20060`), the same
            // hop that gives the constructor its return type.
            Node::ConstructorDeclaration(_) => {
                let parent = self.nodes.parent(signature.declaration)?;
                match self.node_map.get(parent)? {
                    Node::ClassDeclaration(class) => class.type_parameters,
                    Node::ClassExpression(class) => class.type_parameters,
                    _ => return None,
                }
            }
            _ => return None,
        };
        let symbols = declarations
            .iter()
            .map(|declaration| declaration.node_id.and_then(|id| self.binder.symbol_of(id)))
            .collect::<Option<Vec<_>>>()?;
        Some(symbols.into_iter().map(|symbol| self.get_declared_type_of_symbol(symbol)).collect())
    }

    /// Whether a type contains any of the owned inference parameters.
    ///
    /// Follows reference arguments, signatures (including predicates), object
    /// properties, tuple elements and union/intersection constituents by identity.
    /// A generic signature's same-named bound parameter is a different type.
    /// Shapes without structural metadata retain the printed-identifier fallback
    /// until their type graph has been ported.
    pub(crate) fn mentions_type_parameter(
        &self,
        id: TypeId,
        parameters: &[TypeId],
        names: &[&str],
    ) -> bool {
        self.mentions_type_parameter_inner(
            id,
            &|candidate| parameters.contains(&candidate),
            names,
            &mut Vec::new(),
        )
    }

    /// The same graph walk with membership in this checker's current registry.
    /// Avoids materializing all registered identities for every conditional.
    pub(crate) fn mentions_registered_type_parameter(&self, id: TypeId) -> bool {
        self.mentions_type_parameter_inner(
            id,
            &|candidate| self.type_parameter_symbols.contains_key(&candidate),
            &[],
            &mut Vec::new(),
        )
    }

    /// Type-parameter identity through the type graph, including bound generic
    /// signatures. The printed fallback is retained only for shapes without
    /// structural metadata; a same-named bound parameter never matches it.
    fn mentions_type_parameter_inner(
        &self,
        id: TypeId,
        is_parameter: &impl Fn(TypeId) -> bool,
        names: &[&str],
        visited: &mut Vec<TypeId>,
    ) -> bool {
        if is_parameter(id) {
            return true;
        }
        if visited.contains(&id) {
            return false;
        }
        if let Some((_, target)) = self.string_mapping_types.get(&id) {
            visited.push(id);
            return self.mentions_type_parameter_inner(*target, is_parameter, names, visited);
        }
        if let Some(parts) = self.template_literal_parts.get(&id) {
            visited.push(id);
            return parts
                .types
                .iter()
                .any(|&ty| self.mentions_type_parameter_inner(ty, is_parameter, names, visited));
        }
        let ty = self.store.get(id);
        if ty.flags.intersects(
            crate::flags::TypeFlags::TYPE_PARAMETER
                | crate::flags::TypeFlags::PRIMITIVE
                | crate::flags::TypeFlags::ANY_OR_UNKNOWN
                | crate::flags::TypeFlags::NEVER,
        ) {
            return false;
        }
        // Terminal leaves have no followed edges and need no cycle marker.
        visited.push(id);
        if let Some(&operand) = self.deferred_keyof_operands.get(&id) {
            return self.mentions_type_parameter_inner(operand, is_parameter, names, visited);
        }
        if let Some(&(object, index, _)) = self.deferred_indexed_access_types.get(&id) {
            return self.mentions_type_parameter_inner(object, is_parameter, names, visited)
                || self.mentions_type_parameter_inner(index, is_parameter, names, visited);
        }
        if let Some((_, arguments)) = self.type_reference_targets.get(&id) {
            return arguments
                .iter()
                .any(|&ty| self.mentions_type_parameter_inner(ty, is_parameter, names, visited));
        }
        if let Some(info) = self.mapped_conditionals.get(&id) {
            return info
                .operands
                .iter()
                .any(|&ty| self.mentions_type_parameter_inner(ty, is_parameter, names, visited));
        }
        if let Some(info) = self.mapped_types.get(&id) {
            return self.mentions_type_parameter_inner(
                info.constraint,
                is_parameter,
                names,
                visited,
            ) || self.mentions_type_parameter_inner(
                info.template,
                is_parameter,
                names,
                visited,
            ) || info.name_type.is_some_and(|ty| {
                self.mentions_type_parameter_inner(ty, is_parameter, names, visited)
            });
        }
        if self.object_literal_index_infos.get(&id).is_some_and(|infos| {
            infos.iter().any(|info| {
                self.mentions_type_parameter_inner(info.key, is_parameter, names, visited)
                    || self.mentions_type_parameter_inner(info.value, is_parameter, names, visited)
            })
        }) {
            return true;
        }
        if let Some(signatures) = self.signature_types.get(&id) {
            if signatures.iter().any(|signature| {
                signature
                    .parameters
                    .iter()
                    .map(|p| p.r#type)
                    .chain(signature.this_parameter.iter().map(|p| p.r#type))
                    .chain(std::iter::once(signature.r#type))
                    .chain(signature.predicate.iter().filter_map(|predicate| predicate.r#type))
                    .chain(
                        signature
                            .type_parameters
                            .iter()
                            .flat_map(|p| [p.constraint, p.default].into_iter().flatten()),
                    )
                    .any(|ty| self.mentions_type_parameter_inner(ty, is_parameter, names, visited))
            }) {
                return true;
            }
            return self.anonymous_properties.get(&id).is_some_and(|(properties, _)| {
                properties.iter().any(|property| {
                    self.mentions_type_parameter_inner(
                        property.r#type,
                        is_parameter,
                        names,
                        visited,
                    )
                })
            });
        }
        if let Some((properties, _)) = self.anonymous_properties.get(&id) {
            return properties.iter().any(|property| {
                self.mentions_type_parameter_inner(property.r#type, is_parameter, names, visited)
            });
        }
        if let Some((elements, _)) = self.tuple_element_lists.get(&id) {
            return elements
                .iter()
                .any(|&ty| self.mentions_type_parameter_inner(ty, is_parameter, names, visited));
        }
        let constituents: &[TypeId] = match &ty.data {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. } => types,
            _ => &[],
        };
        if !constituents.is_empty() {
            return constituents
                .iter()
                .any(|&t| self.mentions_type_parameter_inner(t, is_parameter, names, visited));
        }
        let text = crate::printing::type_to_string(ty);
        names.iter().any(|name| mentions_identifier(&text, name))
    }
}

/// Whether `text` contains `name` as a whole identifier.
///
/// A substring test would make `T` match `Test`; an identifier test is what
/// makes the scan in [`Checker::mentions_type_parameter`] usable at all.
fn mentions_identifier(text: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let is_part = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(offset) = text[from..].find(name) {
        let start = from + offset;
        let end = start + name.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if !before.is_some_and(is_part) && !after.is_some_and(is_part) {
            return true;
        }
        // Advance by one *character*, not one byte.
        from = start + text[start..].chars().next().map_or(1, char::len_utf8);
        if from >= bytes.len() {
            break;
        }
    }
    false
}

/// Resolving the **written** type arguments of a call needs the node at the
/// checker's own lifetime, which a `CallExpression<'_>` handed down from
/// [`Checker::check_expression`] does not have. The way across is the one
/// [`Checker::check_assertion`] uses: carry the [`NodeId`] and re-fetch the
/// typed node from `node_map`, which yields it at `'a`.
impl Checker<'_, '_> {
    /// The types the caller wrote in `f<string>(x)`, or `None` when none were
    /// written and the call needs inference instead.
    ///
    /// Ported from `Checker.checkTypeArguments` (`checker.go:9269`), minus the
    /// constraint check — see [`Checker::check_generic_call`] for what that
    /// costs. An argument that does not resolve is kept as `errorType` rather
    /// than collapsing the list, so the caller can tell "no type arguments"
    /// from "type arguments this port cannot read".
    fn written_type_arguments(&mut self, call: Option<NodeId>) -> Option<Vec<TypeId>> {
        let nodes = match call.and_then(|id| self.node_map.get(id)) {
            Some(Node::CallExpression(node)) => node.type_arguments,
            Some(Node::NewExpression(node)) => node.type_arguments,
            _ => return None,
        };
        if nodes.is_empty() {
            return None;
        }
        Some(nodes.iter().map(|argument| self.get_type_from_type_node(*argument)).collect())
    }
}

#[cfg(test)]
mod tests {
    use tsr_ast::{Expression, Statement};
    use tsr_core::Arena;

    use super::mentions_identifier;
    use crate::Checker;

    #[test]
    fn dependent_conditional_constraint_uses_the_inferred_outer_mapper() {
        use crate::relater::{Relation, Ternary};

        let arena = Arena::new();
        let source = r#"enum First { A = "a", B = "b" }
enum Second { A = "a", C = "c" }
declare function owner<U, T extends U extends string ? First : number>(value: T, other: U): [T];
const value = First.A;"#;
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_strict_null_checks(true);
        let Statement::FunctionDeclaration(owner) = parsed.source_file.statements[2] else {
            panic!("owner function");
        };
        let signature = checker
            .get_signatures_of_symbol(bound.symbol_of(owner.node_id.unwrap()).unwrap())
            .unwrap()
            .remove(0);
        let parameters = checker.type_parameter_types(&signature).unwrap();
        let constraint = signature.type_parameters[1].constraint.unwrap();
        assert!(checker.conditional_inference_nodes.contains_key(&constraint));
        let Statement::VariableStatement(variable) = parsed.source_file.statements[3] else {
            panic!("enum member candidate");
        };
        let expression = variable.declaration_list.unwrap().declarations[0].initializer.unwrap();
        let candidate = checker.check_expression(expression);
        let mut infos = Vec::new();
        for (parameter, value) in
            [(parameters[0], checker.intrinsics.string), (parameters[1], candidate)]
        {
            super::add_directional_candidate(
                &mut infos,
                parameter,
                value,
                false,
                super::InferencePriority::NONE,
            );
        }
        let mut map = Vec::new();
        let inferred_u = checker.resolve_inference_with_constraints(
            (&signature, &infos),
            0,
            &parameters,
            &[],
            &mut map,
            super::InferenceFlags::NONE,
        );
        assert_eq!(inferred_u, checker.intrinsics.string);
        let branch = checker.instantiate_type(constraint, &map, &parameters, &["U", "T"]);
        assert_eq!(checker.type_to_string(branch), "First");
        let inferred_t = checker.resolve_inference_with_constraints(
            (&signature, &infos),
            1,
            &parameters,
            &[],
            &mut map,
            super::InferenceFlags::NONE,
        );
        assert_eq!(checker.type_to_string(inferred_t), "First.A");
        assert!(!checker.generic_argument_is_inapplicable(candidate, inferred_t));
        let numeric = checker.instantiate_type(
            constraint,
            &[(parameters[0], checker.intrinsics.boolean)],
            &parameters,
            &["U", "T"],
        );
        assert_eq!(numeric, checker.intrinsics.number);
        assert!(checker.generic_argument_is_inapplicable(candidate, numeric));
        let Statement::EnumDeclaration(second) = parsed.source_file.statements[1] else {
            panic!("foreign enum");
        };
        let foreign =
            checker.get_declared_type_of_symbol(bound.symbol_of(second.node_id.unwrap()).unwrap());
        assert_eq!(
            checker.relate_ternary(candidate, foreign, Relation::Assignable),
            Ternary::NotRelated
        );

        // Compose, rather than discard, a root's previously captured U := T
        // binding when the subsequent mapper supplies T := string.
        let Some(tsr_ast::TypeNode::ConditionalTypeNode(node)) =
            owner.type_parameters[1].constraint
        else {
            panic!("retained conditional root");
        };
        let mut bindings = rustc_hash::FxHashMap::default();
        bindings.insert(checker.type_parameter_symbols[&parameters[0]], parameters[1]);
        checker.alias_evaluation_bindings.push(bindings);
        let captured =
            checker.get_type_from_type_node(tsr_ast::TypeNode::ConditionalTypeNode(node));
        checker.alias_evaluation_bindings.pop();
        let composed = checker.instantiate_type(
            captured,
            &[(parameters[1], checker.intrinsics.string)],
            &parameters,
            &["U", "T"],
        );
        assert_eq!(checker.type_to_string(composed), "First");

        let unsupported = checker.store.new_named(
            crate::flags::TypeFlags::CONDITIONAL,
            "U extends string ? First : number".to_string(),
            None,
        );
        assert_eq!(
            checker.instantiate_type(unsupported, &map, &parameters, &["U", "T"]),
            checker.intrinsics.error,
        );
        assert_eq!(
            checker.instantiate_conditional_node(unsupported, &map, &parameters, &["U", "T"]),
            checker.intrinsics.error,
        );
    }

    #[test]
    fn conditional_constraint_slice_refuses_deferred_default_and_return_roots() {
        use crate::{flags::TypeFlags, types::TypeData};

        let arena = Arena::new();
        let source = r#"enum First { A = "a", B = "b" }
declare function owner<U,
    T extends ((U extends string ? U extends "only" ? First : number : boolean)),
    D = U extends string ? First : number>(): U extends string ? First : number;
declare function deferred<U, V,
    T extends (U extends string ? V extends number ? First : number : boolean)>(): void;"#;
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_strict_null_checks(true);
        let Statement::FunctionDeclaration(owner) = parsed.source_file.statements[1] else {
            panic!("owner function");
        };
        let signature = checker
            .get_signatures_of_symbol(bound.symbol_of(owner.node_id.unwrap()).unwrap())
            .unwrap()
            .remove(0);
        let parameters = checker.type_parameter_types(&signature).unwrap();
        let names = ["U", "T", "D"];
        let constraint = signature.type_parameters[1].constraint.unwrap();
        for (text, expected) in [("only", "First"), ("other", "number")] {
            let argument = checker.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(text.to_string()),
                false,
            );
            let result = checker.instantiate_type(
                constraint,
                &[(parameters[0], argument)],
                &parameters,
                &names,
            );
            assert_eq!(checker.type_to_string(result), expected);
        }
        for root in [signature.type_parameters[2].default.unwrap(), signature.r#type] {
            // Native can instantiate these too. This dependent-constraint unit
            // keeps their unsupported consumers on the pre-existing gap path.
            assert_eq!(
                checker.instantiate_type(
                    root,
                    &[(parameters[0], checker.intrinsics.string)],
                    &parameters,
                    &names
                ),
                checker.intrinsics.error,
            );
        }
        // Native retains a mapped deferred conditional here; the port must not
        // emit the original written U instead of the substituted T.
        assert_eq!(
            checker.instantiate_type(
                constraint,
                &[(parameters[0], parameters[1])],
                &parameters,
                &names
            ),
            checker.intrinsics.error,
        );
        let Statement::FunctionDeclaration(deferred) = parsed.source_file.statements[2] else {
            panic!("deferred branch owner");
        };
        let deferred = checker
            .get_signatures_of_symbol(bound.symbol_of(deferred.node_id.unwrap()).unwrap())
            .unwrap()
            .remove(0);
        let parameters = checker.type_parameter_types(&deferred).unwrap();
        // Selecting the outer true branch still cannot serialize the inner V
        // conditional under its captured mapper. Do not expose written syntax.
        assert_eq!(
            checker.instantiate_type(
                deferred.type_parameters[2].constraint.unwrap(),
                &[(parameters[0], checker.intrinsics.string)],
                &parameters,
                &["U", "V", "T"]
            ),
            checker.intrinsics.error,
        );
        assert!(checker.alias_evaluation_bindings.is_empty());
    }

    #[test]
    fn conditional_default_primitive_test_does_not_follow_distributive_or_variable_constraints() {
        use crate::flags::TypeFlags;

        let arena = Arena::new();
        let source = r"type TrueAny<W> = W extends string ? any : number;
type FalseObject<W> = W extends string ? {} : any;
type VariableOnly<W, V extends string> = W extends string ? any : V;
type Restricted<U extends string> = U extends string ? {} : number;
declare function owner<U extends string, V extends string, W,
    A extends TrueAny<W>, B extends FalseObject<W>, C extends VariableOnly<W, V>,
    D extends Restricted<U>, E extends string>(): void;";
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_strict_null_checks(true);
        let Statement::FunctionDeclaration(declaration) = parsed.source_file.statements[4] else {
            panic!("the fixture must declare its owner function");
        };
        let owner = bound.symbol_of(declaration.node_id.unwrap()).unwrap();
        let signature = checker.get_signatures_of_symbol(owner).unwrap().remove(0);
        let variable = signature.type_parameters[5].constraint.unwrap();
        let restricted = signature.type_parameters[6].constraint.unwrap();
        assert_eq!(checker.base_constraint_of_type(variable), Some(checker.intrinsics.string));
        let narrowed = checker.base_constraint_of_type(restricted).unwrap();
        assert!(checker.store.get(narrowed).flags.contains(TypeFlags::OBJECT));
        for (position, expected) in [(3, true), (4, false), (5, false), (6, true), (7, true)] {
            assert_eq!(
                checker.parameter_has_primitive_constraint(&signature, position),
                expected,
                "constraint at position {position}"
            );
        }
    }

    #[test]
    fn conditional_primitive_candidates_keep_named_enum_member_identity() {
        use crate::relater::{Relation, Ternary};

        let arena = Arena::new();
        let source = r#"enum First { A = "a", B = "b" }
enum Second { A = "a", C = "c" }
declare function owner<U, T extends U extends string ? First : number>(value: T): [T];
const value = First.A;"#;
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_strict_null_checks(true);
        let Statement::EnumDeclaration(first) = parsed.source_file.statements[0] else {
            panic!("first enum");
        };
        let Statement::EnumDeclaration(second) = parsed.source_file.statements[1] else {
            panic!("second enum");
        };
        let first =
            checker.get_declared_type_of_symbol(bound.symbol_of(first.node_id.unwrap()).unwrap());
        let second =
            checker.get_declared_type_of_symbol(bound.symbol_of(second.node_id.unwrap()).unwrap());
        let Statement::FunctionDeclaration(owner) = parsed.source_file.statements[2] else {
            panic!("owner function");
        };
        let signature = checker
            .get_signatures_of_symbol(bound.symbol_of(owner.node_id.unwrap()).unwrap())
            .unwrap()
            .remove(0);
        let Statement::VariableStatement(variable) = parsed.source_file.statements[3] else {
            panic!("enum member candidate");
        };
        let expression = variable.declaration_list.unwrap().declarations[0].initializer.unwrap();
        let candidate = checker.check_expression(expression);
        let mut infos = Vec::new();
        super::add_directional_candidate(
            &mut infos,
            signature.parameters[0].r#type,
            candidate,
            false,
            super::InferencePriority::NONE,
        );
        let inferred = checker.inferred_covariant_type(&infos[0], &signature, 1).unwrap();
        assert_eq!(checker.type_to_string(inferred), "First.A");
        assert_eq!(checker.relate_ternary(inferred, first, Relation::Assignable), Ternary::Related);
        assert_eq!(
            checker.relate_ternary(inferred, second, Relation::Assignable),
            Ternary::NotRelated
        );
    }

    #[test]
    fn registered_parameter_membership_preserves_identity_graphs_and_current_registry() {
        use crate::flags::TypeFlags;

        let arena = Arena::new();
        let source = "function owner<T>(value: T): T { return value; }";
        let parsed = tsr_parser::parse(&arena, source);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let Statement::FunctionDeclaration(declaration) = parsed.source_file.statements[0] else {
            panic!("the fixture must declare a function");
        };
        let owner = bound.symbol_of(declaration.node_id.unwrap()).unwrap();
        let signature = checker.get_signatures_of_symbol(owner).unwrap().remove(0);
        let registered = signature.parameters[0].r#type;
        assert!(checker.type_parameter_symbols.contains_key(&registered));
        let unregistered = checker.store.new_named(TypeFlags::TYPE_PARAMETER, "T".into(), None);
        assert_ne!(registered, unregistered);
        let number = checker.intrinsics().number;
        let fallback = checker.store.new_named(TypeFlags::OBJECT, "Fallback<T>".into(), None);

        // A cycle must not hide the later registered argument, and an unrelated
        // same-named parameter must never match through printed-name recovery.
        let positive_cycle = checker.store.new_named(TypeFlags::OBJECT, "Positive".into(), None);
        checker
            .type_reference_targets
            .insert(positive_cycle, (owner, vec![positive_cycle, registered]));
        let negative_cycle = checker.store.new_named(TypeFlags::OBJECT, "Negative".into(), None);
        checker
            .type_reference_targets
            .insert(negative_cycle, (owner, vec![negative_cycle, unregistered]));

        let callable = checker.store.new_named(TypeFlags::OBJECT, "Callable".into(), None);
        let mut nested = signature.clone();
        nested.parameters[0].r#type = positive_cycle;
        nested.r#type = number;
        checker.signature_types.insert(callable, vec![nested]);

        let constrained = checker.store.new_named(TypeFlags::OBJECT, "Constrained".into(), None);
        let mut nested = signature.clone();
        nested.parameters[0].r#type = number;
        nested.r#type = number;
        nested.type_parameters[0].constraint = Some(registered);
        checker.signature_types.insert(constrained, vec![nested]);

        let defaulted = checker.store.new_named(TypeFlags::OBJECT, "Defaulted".into(), None);
        let mut nested = signature.clone();
        nested.parameters[0].r#type = number;
        nested.r#type = number;
        nested.type_parameters[0].default = Some(registered);
        checker.signature_types.insert(defaulted, vec![nested]);

        let shadow = checker.store.new_named(TypeFlags::OBJECT, "Shadow".into(), None);
        let mut nested = signature;
        nested.parameters[0].r#type = unregistered;
        nested.r#type = unregistered;
        checker.signature_types.insert(shadow, vec![nested]);

        let cases = [
            (registered, true),
            (unregistered, false),
            (number, false),
            (fallback, false),
            (positive_cycle, true),
            (negative_cycle, false),
            (callable, true),
            (constrained, true),
            (defaulted, true),
            (shadow, false),
        ];
        let parameters: Vec<_> = checker.type_parameter_symbols.keys().copied().collect();
        for (ty, expected) in cases {
            assert_eq!(checker.mentions_registered_type_parameter(ty), expected, "{ty:?}");
            assert_eq!(checker.mentions_type_parameter(ty, &parameters, &[]), expected);
        }
        assert!(checker.mentions_type_parameter(fallback, &[], &["T"]));
        assert!(!checker.mentions_type_parameter(unregistered, &[registered], &["T"]));

        // Registry changes are visible immediately, without caching an answer
        // obtained before the semantic graph finished being populated.
        checker.type_parameter_symbols.remove(&registered);
        for (ty, _) in cases {
            assert!(!checker.mentions_registered_type_parameter(ty), "{ty:?}");
        }
        checker.type_parameter_symbols.insert(unregistered, owner);
        assert!(checker.mentions_registered_type_parameter(negative_cycle));
        assert!(checker.mentions_registered_type_parameter(shadow));
        assert!(!checker.mentions_registered_type_parameter(positive_cycle));
    }

    /// Ask [`Checker::check_generic_call`] directly, with the signature of
    /// `function` and the arguments of the call the last statement initialises.
    ///
    /// Deliberately **not** routed through `check_call_expression`: the call
    /// site there is one line, and going round it keeps these tests measuring
    /// inference rather than signature resolution.
    fn generic_call(source: &str, function: &str) -> String {
        generic_call_with_strictness(source, function, true)
    }

    /// [`generic_call`] with `strictNullChecks` chosen explicitly — the flag
    /// `getWidenedType` consults, and the one the `null`-candidate refusal in
    /// [`Checker::check_generic_call`] is gated on.
    fn generic_call_with_strictness(source: &str, function: &str, strict: bool) -> String {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(
            parsed.diagnostics.is_empty(),
            "fixture must parse: {:?}",
            parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
        );
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_strict_null_checks(strict);

        let declaration = parsed
            .source_file
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::FunctionDeclaration(node)
                    if node.name.is_some_and(|name| name.text == function) =>
                {
                    Some(*node)
                }
                _ => None,
            })
            .expect("the fixture must declare the function");
        let symbol = bound
            .symbol_of(declaration.node_id.expect("a registered node"))
            .expect("the function must be bound");
        let signatures = checker.get_signatures_of_symbol(symbol).expect("a signature");
        let [signature] = signatures.as_slice() else {
            panic!("the fixture must declare exactly one signature");
        };
        let signature = signature.clone();

        let last = parsed.source_file.statements.len() - 1;
        let Statement::VariableStatement(statement) = parsed.source_file.statements[last] else {
            panic!("the last statement must be a variable statement");
        };
        let initialiser = statement
            .declaration_list
            .and_then(|list| list.declarations.first().copied())
            .and_then(|declaration| declaration.initializer)
            .expect("an initialiser");
        let Expression::CallExpression(call) = initialiser else {
            panic!("the initialiser must be a call");
        };
        let id = checker.check_generic_call(&signature, call.node_id, call.arguments);
        checker.type_to_string(id)
    }

    #[test]
    fn a_bare_type_parameter_is_the_argument_type_unwidened() {
        // `>f(1) : 1`, from the baseline quoted in the module docs. Three wrong
        // implementations are separated here: widening the candidate prints
        // `number` — and would still look right on the *variable*, which is what
        // makes this the load-bearing fixture; answering the uninstantiated
        // return type prints `T`; answering `anyType` prints `any`.
        assert_eq!(generic_call("function f<T>(x: T): T { return x; }\nconst a = f(1);", "f"), "1");
        assert_eq!(
            generic_call("function f<T>(x: T): T { return x; }\nconst a = f(\"s\");", "f"),
            "\"s\""
        );
    }

    #[test]
    fn a_candidate_comes_from_the_parameters_own_position() {
        // `<T, U>(a: T, b: U) => U` applied to `(1, "s")` is `"s"`. An
        // implementation that collects candidates without tracking which
        // parameter they belong to — first argument wins, or last — prints `1`.
        assert_eq!(
            generic_call(
                "function pick<T, U>(a: T, b: U): U { return b; }\nconst a = pick(1, \"s\");",
                "pick"
            ),
            "\"s\""
        );
        // The mirror, so that "always take the last argument" fails too.
        assert_eq!(
            generic_call(
                "function pick<T, U>(a: T, b: U): T { return a; }\nconst a = pick(1, \"s\");",
                "pick"
            ),
            "1"
        );
    }

    #[test]
    fn a_return_type_free_of_type_parameters_needs_no_inference() {
        // `<T>(x: T) => string` is `string` however `T` resolves. An
        // implementation that gaps every generic signature prints `error`; one
        // that answers the candidate regardless of the return type prints `1`.
        assert_eq!(
            generic_call("function f<T>(x: T): string { return \"\"; }\nconst a = f(1);", "f"),
            "string"
        );
    }

    #[test]
    fn two_candidates_for_one_type_parameter_are_a_gap() {
        // `<T>(a: T, b: T) => T` applied to `(1, "s")`: the bases disagree, so
        // `literalTypesWithSameBaseType` declines the union and
        // `getSingleCommonSupertype` (`inference.go:1555`) resolves the
        // conflict — the leftmost type no right neighbor supersedes, which is
        // `1`. Upstream then reports TS2345 on `"s"`; the diagnostic is the
        // other lane's, the call's type is this one's. Before the pipeline
        // this was a decline, and this test pinned the gap; it now pins the
        // conflict rule instead.
        assert_eq!(
            generic_call(
                "function both<T>(a: T, b: T): T { return a; }\nconst a = both(1, \"s\");",
                "both"
            ),
            "1"
        );
        // The agreeing case still answers, so the guard is not "two parameters
        // are a gap".
        assert_eq!(
            generic_call(
                "function both<T>(a: T, b: T): T { return a; }\nconst a = both(1, 1);",
                "both"
            ),
            "1"
        );
    }

    #[test]
    fn a_return_type_that_contains_a_type_parameter_is_rebuilt() {
        // `T[]` and `C<T>` are the 21 of 40 written-argument calls the module
        // docs count. Both are a reference interned on `(symbol, arguments)`,
        // which is the only thing that makes them rebuildable.
        //
        // Three wrong implementations are separated. Answering the
        // uninstantiated return type prints `T[]` / `C<T>` — that is what this
        // module did before the reverse index and is the failure the test is
        // named for. Answering the *written argument* rather than the
        // substituted reference prints `string` / `number`, which is the
        // shortcut a one-line "identity case" invites. Answering `errorType`
        // prints `error`.
        assert_eq!(
            generic_call(
                "interface Array<T> { }\ndeclare function f<T>(x: T): T[];\nconst a = f<string>(\"s\");",
                "f"
            ),
            "string[]"
        );
        assert_eq!(
            generic_call(
                "interface C<T> { }\ndeclare function g<T>(x: T): C<T>;\nconst a = g<number>(1);",
                "g"
            ),
            "C<number>"
        );
        // The inference path reaches the same rebuild through a candidate
        // rather than a written argument. `n` is annotated so the answer is
        // `number` rather than the literal type `1`, which keeps this measuring
        // substitution and not widening.
        assert_eq!(
            generic_call(
                "interface Array<T> { }\ndeclare const n: number;\ndeclare function f<T>(x: T): T[];\nconst a = f(n);",
                "f"
            ),
            "number[]"
        );
        // Nested, so that a rebuild one level deep is separated from a general
        // one: `C<T[]>` needs the inner reference rebuilt before the outer.
        assert_eq!(
            generic_call(
                "interface Array<T> { }\ninterface C<T> { }\ndeclare function g<T>(x: T): C<T[]>;\nconst a = g<string>(\"s\");",
                "g"
            ),
            "C<string[]>"
        );
    }

    #[test]
    fn a_shape_with_no_intern_key_is_still_a_gap() {
        // This test's first assertion was born asserting `error`: a function
        // type had no `(symbol, arguments)` pair to reverse and nothing to
        // rebuild it from. `bd tsr-0hc` gave signature types their own reverse
        // index (`Checker::signature_types`), so the boundary moved and the
        // assertion flips to the substituted form. What still gaps — and what
        // this test now guards — is a shape with *neither* index: an unmapped
        // type parameter below.
        //
        // A **tuple** return type belongs in this list and is deliberately not
        // asserted here: `[T, U]` gaps *earlier*, in
        // `get_signature_from_declaration`, which answers `None` when the
        // return annotation does not resolve — so the call never reaches this
        // module and a fixture for it would pin the wrong function's behaviour.
        // Measured, not assumed: the assertion was written, panicked on "a
        // signature", and was removed rather than weakened.
        assert_eq!(
            generic_call(
                "declare function k<T>(x: T): (y: T) => T;\nconst a = k<string>(\"s\");",
                "k"
            ),
            "(y: string) => string"
        );
        // §36 changed the third way in: an uninferable `U` with no default
        // and no possible source takes upstream's `unknownType` fallback
        // (`inference.go:1406`), so the answer is `C<unknown>` — upstream's
        // own — rather than the pre-§36 gap.
        assert_eq!(
            generic_call(
                "interface C<T> { }\ndeclare function m<T, U>(x: T): C<U>;\nconst a = m(\"s\");",
                "m"
            ),
            "C<unknown>"
        );
    }

    #[test]
    fn instantiation_past_depth_100_is_refused_as_upstream_refuses_it() {
        // `checker.go:22111`: at an `instantiationDepth` of 100 upstream reports
        // `Type_instantiation_is_excessively_deep_and_possibly_infinite` and
        // answers `errorType`. A written return type nested 150 deep —
        // `C<C<…<T>…>>` — drives the substitution one frame per level, so it
        // crosses the limit with no infinite type involved, which is what makes
        // it writable as a fixture. Upstream refuses the same program, so the
        // `error` here is upstream's answer and not a port-side gap.
        //
        // Without the guard this fixture still terminates (the nesting is
        // finite) and prints the 150-deep instantiation, so the assertion
        // separates guard-present from guard-absent. The existing nested test
        // above is the control on the other side: depth 2 must keep answering.
        // The count leg (5,000,000) has no writable fixture and is untested.
        let mut nested = String::from("T");
        for _ in 0..150 {
            nested = format!("C<{nested}>");
        }
        let source = format!(
            "interface C<T> {{ }}\ndeclare function g<T>(x: T): {nested};\nconst a = g<number>(1);"
        );
        assert_eq!(generic_call(&source, "g"), "error");
    }

    #[test]
    fn an_identifier_scan_is_not_a_substring_scan() {
        assert!(mentions_identifier("T", "T"));
        assert!(mentions_identifier("T[]", "T"));
        assert!(mentions_identifier("(x: T) => void", "T"));
        assert!(mentions_identifier("C<T>", "T"));
        // The cases a substring test would get wrong.
        assert!(!mentions_identifier("Test", "T"));
        assert!(!mentions_identifier("number[]", "T"));
        assert!(!mentions_identifier("T2", "T"));
        assert!(!mentions_identifier("_T", "T"));
    }

    /// Every expected string below was taken from a `.types` baseline **before
    /// it was written down**, and each fixture's source is the baseline's own
    /// source. `docs/conventions.md` records five intuition-written
    /// expectations across earlier sessions, all five wrong and the port right
    /// every time — and records separately that a rule stated in a file header
    /// is not applied by being stated.
    ///
    /// Arm 4 of [`Checker::infer_from_types`] — one signature against another —
    /// has **no synthetic fixture here**, and that is stated rather than
    /// hidden: no baseline gives it a small enough exact expectation, and every
    /// reduction of one that suggested itself would have been an expectation
    /// written from intuition. It is exercised by the corpus instead (14
    /// converted lines in `compiler/promisePermutations3`, 4 in
    /// `conformance/genericCallWithGenericSignatureArguments`), and its
    /// *refusal* boundary is asserted below.
    #[test]
    fn a_reference_argument_is_decomposed_against_a_reference_parameter() {
        // `conformance/neverInference.types:20` — `>f1(neverArray) : never`.
        // `T[]` against `never[]`: both are `Array` references through
        // `create_type_reference`, which is the only reason the candidate can
        // be dug out at all. Before `bd tsr-g30h` this was a gap.
        assert_eq!(
            generic_call(
                "interface Array<T> { }\ndeclare function f1<T>(x: T[]): T;\ndeclare const neverArray: never[];\nvar a2 = f1(neverArray);",
                "f1"
            ),
            "never"
        );
    }

    #[test]
    fn a_never_candidate_is_struck_when_another_candidate_exists() {
        // `compiler/undefinedInferentialTyping.types:12` — `>f([], 3) : 3`.
        // `T[]` against `never[]` yields `never` and `T` against `3` yields
        // `3`. Upstream unions the candidates and `never` is the union's
        // identity, so the answer is `3`. The two plausible wrong
        // implementations are "the candidates disagree, gap" (prints `error`)
        // and "first candidate wins" (prints `never`).
        assert_eq!(
            generic_call(
                "interface Array<T> { }\nfunction f<T>(arr: T[], elemnt: T): T { return null; }\nvar a = f([], 3);",
                "f"
            ),
            "3"
        );
    }

    #[test]
    fn a_union_parameter_strikes_the_constituent_its_argument_matches() {
        // `conformance/unionTypeInference.types:19` — `>f1(1, "hello") : 1`.
        // `"hello"` matches the `string` constituent of `string | T`, so
        // upstream infers nothing from that position. An implementation that
        // pours the source into the naked `T` regardless collects `"hello"`
        // beside `1`, and the two disagree — which prints `error`.
        assert_eq!(
            generic_call(
                "declare function f1<T>(x: T, y: string | T): T;\nconst a2 = f1(1, \"hello\");",
                "f1"
            ),
            "1"
        );
        // `conformance/unionTypeInference.types:82` — `>f3(5) : 5`. The mirror:
        // `5` matches neither `string` nor `false`, so the naked `T` is where
        // the candidate lands. Without the union arm this is a gap.
        assert_eq!(
            generic_call(
                "declare function f3<T>(x: string | false | T): T;\nconst c1 = f3(5);",
                "f3"
            ),
            "5"
        );
    }

    #[test]
    fn the_union_arm_still_refuses_what_needs_a_union_built() {
        // `conformance/unionTypeInference.types:13` — `>f1(1, 2) : 1 | 2`.
        // Both positions yield a candidate, the candidates disagree, and both
        // are literals of ONE base — `literalTypesWithSameBaseType`
        // (`inference.go:1601`) unions them, and `T` is top-level in the
        // return with no consumption, so the union keeps its literals. This
        // was the pipeline's registered conversion fixture while the port
        // declined it; the name keeps the history.
        assert_eq!(
            generic_call(
                "declare function f1<T>(x: T, y: string | T): T;\nconst a1 = f1(1, 2);",
                "f1"
            ),
            "1 | 2"
        );
    }

    #[test]
    fn a_generic_source_signature_uses_its_base_constraint() {
        // `conformance/genericCallWithFunctionTypedArguments.types:17` —
        // `>foo(<U>(x: U) => '') : unknown`. Upstream erases the source
        // signature's own type parameters (`getErasedSignature`) before
        // inferring; with no route to that, arm 4 refuses a generic signature
        // on either side rather than inferring `U` into `T`.
        assert_eq!(
            generic_call(
                "function foo<T>(x: (a: T) => T) { return x(null); }\nvar r = foo(<U>(x: U) => '');",
                "foo"
            ),
            "unknown"
        );
    }

    #[test]
    fn a_null_candidate_is_refused_only_where_upstream_would_widen_it() {
        // A pair, because the whole content of the rule is the flag.
        // `getWidenedType` (`checker.go:16090`) widens a `null` inference to
        // `any` **only** with `strictNullChecks` off, and this port does not
        // widen — so the non-strict side must gap and the strict side must
        // answer. Refusing both is what the first corpus run did, and it turned
        // six right lines in `conformance/strictNullChecksNoWidening` and
        // `compiler/undefinedInferentialTyping` into gaps; that is how the
        // bar's second leg found the defect.
        //
        // Since `nullWideningType` exists (docs/parity/notes/contextual.md §7)
        // the non-strict candidate is the widening twin and widens to `any`,
        // as upstream's does.
        let source = "declare function f<T>(x: T): T;\nvar a = f(null);";
        assert_eq!(generic_call_with_strictness(source, "f", true), "null");
        assert_eq!(generic_call_with_strictness(source, "f", false), "any");
    }
}

/// The `InferenceInfo` collector (`checker-notes-callres2.md`, the foundation
/// design, step 1): per-parameter candidate lists as the COLLECTION model,
/// flattened to the wire pairs at the boundary. Step 1 is
/// behavior-identical — within-parameter order is preserved and the sole
/// consumer scans per-parameter; a future consumer MUST NOT read
/// cross-parameter order off the flattened list (it differs from the old
/// interleaving; see the execution note in checker-notes-callres2.md).
/// Priorities, contra lists, resolution, and fixing arrive as steps 2-4.
#[derive(Clone, Debug)]
pub(crate) struct InferenceInfo {
    pub(crate) type_parameter: TypeId,
    pub(crate) priority: InferencePriority,
    /// Supplied rest-argument count (inferTypeArguments, checker.go:9467).
    pub(crate) implied_arity: Option<usize>,
    pub(crate) candidates: Vec<TypeId>,
    /// `InferenceInfo.contraCandidates`, kept separate from output positions.
    pub(crate) contra_candidates: Vec<TypeId>,
    /// Cached `InferenceInfo.inferredType` after a fixing mapper read.
    pub(crate) fixed_type: Option<TypeId>,
    /// Upstream InferenceInfo.isFixed: set by the consumption rule when an
    /// inferred type is served for contextual instantiation; read by the
    /// widening decision.
    pub(crate) is_fixed: bool,
    /// Upstream InferenceInfo.topLevel (`inference.go:1626`, cleared at
    /// `:208`): true until a candidate arrives from a position where the
    /// walk's ORIGINAL target does not carry the parameter at top level.
    /// Read by `getCovariantInference`'s widening decision
    /// (`inference.go:1442`).
    pub(crate) top_level: bool,
}

impl InferenceInfo {
    fn has_candidates(&self) -> bool {
        !self.candidates.is_empty() || !self.contra_candidates.is_empty()
    }
}

fn add_directional_candidate(
    infos: &mut Vec<InferenceInfo>,
    type_parameter: TypeId,
    candidate: TypeId,
    contravariant: bool,
    priority: InferencePriority,
) {
    if let Some(info) = infos.iter_mut().find(|i| i.type_parameter == type_parameter) {
        // `inference.go:183`: a FIXED inference set refuses new candidates —
        // the consumption rule's other half, and the whole content of the
        // `typeParameterFixing*` families. §483's pair measured its absence
        // directly (the fixing families were 8 of the 25 G→W).
        if info.is_fixed {
            return;
        }
        if priority.bits() < info.priority.bits() {
            info.candidates.clear();
            info.contra_candidates.clear();
            info.top_level = true;
            info.priority = priority;
        }
        if priority != info.priority {
            return;
        }
        // Upstream dedups at the add site (`slices.Contains`,
        // `inference.go:202`); the resolver's union/supertype stages assume
        // the same.
        let candidates =
            if contravariant { &mut info.contra_candidates } else { &mut info.candidates };
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    } else {
        infos.push(InferenceInfo {
            type_parameter,
            priority,
            implied_arity: None,
            candidates: if contravariant { Vec::new() } else { vec![candidate] },
            contra_candidates: if contravariant { vec![candidate] } else { Vec::new() },
            fixed_type: None,
            is_fixed: false,
            top_level: true,
        });
    }
}

/// Fold one collection's infos into another, preserving the two fields the
/// flat pair merge used to drop: `top_level` ANDs (one nested source marks the
/// parameter nested for good, `inference.go:208`) and `is_fixed` ORs.
pub(crate) fn merge_info(infos: &mut Vec<InferenceInfo>, from: &InferenceInfo) {
    if !from.has_candidates()
        && !infos.iter().any(|info| info.type_parameter == from.type_parameter)
    {
        infos.push(from.clone());
        return;
    }
    for &candidate in &from.candidates {
        add_directional_candidate(infos, from.type_parameter, candidate, false, from.priority);
    }
    for &candidate in &from.contra_candidates {
        add_directional_candidate(infos, from.type_parameter, candidate, true, from.priority);
    }
    if let Some(existing) = infos.iter_mut().find(|i| i.type_parameter == from.type_parameter) {
        if existing.implied_arity.is_none() {
            existing.implied_arity = from.implied_arity;
        }
        existing.top_level &= from.top_level;
        existing.is_fixed |= from.is_fixed;
        if existing.fixed_type.is_none() {
            existing.fixed_type = from.fixed_type;
        }
    }
}

impl Checker<'_, '_> {
    /// isTypeParameterAtTopLevel (inference.go:1493-1499). Conditional
    /// branches count as top level through three nested conditionals; beyond
    /// that native deliberately retains literal inference candidates.
    fn is_type_parameter_at_top_level(&mut self, id: TypeId, parameter: TypeId) -> bool {
        self.is_type_parameter_at_top_level_with_depth(id, parameter, 0)
    }

    fn is_type_parameter_at_top_level_with_depth(
        &mut self,
        id: TypeId,
        parameter: TypeId,
        depth: usize,
    ) -> bool {
        if id == parameter {
            return true;
        }
        if let crate::types::TypeData::Union { types, .. }
        | crate::types::TypeData::Intersection { types, .. } = self.store.get(id).data.clone()
        {
            return types
                .iter()
                .any(|&t| self.is_type_parameter_at_top_level_with_depth(t, parameter, depth));
        }
        depth < 3
            && self.conditional_inference_branches(id).is_some_and(|(yes, no)| {
                self.is_type_parameter_at_top_level_with_depth(yes, parameter, depth + 1)
                    || self.is_type_parameter_at_top_level_with_depth(no, parameter, depth + 1)
            })
    }

    /// `isTypeParameterAtTopLevelInReturnType` (`inference.go:1501-1507`)
    /// over the return type; the type-predicate leg is unreachable here (a
    /// construct signature carries none).
    fn is_type_parameter_at_top_level_in_return_type(
        &mut self,
        signature: &Signature,
        parameter: TypeId,
    ) -> bool {
        self.is_type_parameter_at_top_level(signature.r#type, parameter)
    }

    /// hasPrimitiveConstraint (inference.go:1482) tests a conditional's default
    /// branch constraint, not its distributive or recursively resolved base.
    fn parameter_has_primitive_constraint(
        &mut self,
        signature: &Signature,
        parameter_position: usize,
    ) -> bool {
        use crate::flags::TypeFlags;
        signature.type_parameters.get(parameter_position).and_then(|tp| tp.constraint).is_some_and(
            |constraint| {
                let constraint =
                    self.default_constraint_of_conditional_type(constraint).unwrap_or(constraint);
                self.maybe_type_of_kind(
                    constraint,
                    TypeFlags::PRIMITIVE
                        | TypeFlags::INDEX
                        | TypeFlags::TEMPLATE_LITERAL
                        | TypeFlags::STRING_MAPPING,
                )
            },
        )
    }

    /// `getCommonSupertype` (`inference.go:1530`) — the pipeline's stage-3
    /// combination, with the relater's third verdict honored: any `Unknown`
    /// pair answers `None` and the caller declines the call whole, because a
    /// wrong pick here is a confident wrong line where upstream computed a
    /// supertype this port could not verify.
    ///
    /// The nullable strip-and-restore is upstream's own first move under
    /// `strictNullChecks`; its `filterType` over a UNION candidate's
    /// constituents is unported, so a union carrying a nullable constituent
    /// declines rather than mis-stripping.
    fn covariant_combination(&mut self, types: &[TypeId]) -> Option<TypeId> {
        use crate::flags::TypeFlags;
        let mut primary: Vec<TypeId> = Vec::with_capacity(types.len());
        let mut restore: Vec<TypeId> = Vec::new();
        if self.strict_null_checks {
            for &t in types {
                let flags = self.store.get(t).flags;
                if flags.intersects(TypeFlags::NULLABLE) {
                    if !restore.contains(&t) {
                        restore.push(t);
                    }
                    continue;
                }
                if let TypeData::Union { types: constituents, .. } = &self.store.get(t).data
                    && constituents
                        .iter()
                        .any(|&c| self.store.get(c).flags.intersects(TypeFlags::NULLABLE))
                {
                    return None;
                }
                primary.push(t);
            }
            if primary.is_empty() {
                return None;
            }
        } else {
            primary.extend_from_slice(types);
        }
        let supertype = if self.literal_types_with_same_base_type(&primary) {
            self.get_union_type_unprinted(&primary)
        } else {
            self.single_common_supertype(&primary)?
        };
        if restore.is_empty() {
            Some(supertype)
        } else {
            let mut all = vec![supertype];
            all.extend(restore);
            Some(self.get_union_type_unprinted(&all))
        }
    }

    /// `literalTypesWithSameBaseType` (`inference.go:1601`), verbatim: every
    /// non-`never` candidate is a literal (its base differs from itself) and
    /// all bases agree.
    fn literal_types_with_same_base_type(&mut self, types: &[TypeId]) -> bool {
        use crate::flags::TypeFlags;
        let mut common: Option<TypeId> = None;
        for &t in types {
            if self.store.get(t).flags.intersects(TypeFlags::NEVER) {
                continue;
            }
            let base = self.get_base_type_of_literal_type(t);
            if common.is_none() {
                common = Some(base);
            }
            if base == t || common != Some(base) {
                return false;
            }
        }
        true
    }

    /// `getSingleCommonSupertype` (`inference.go:1555`): the leftmost type no
    /// right neighbor strictly supersedes, verified against every candidate;
    /// failing the verification, the same walk under the regular subtype
    /// relation, unverified — upstream returns that leftmost as-is. Every
    /// relation question is asked through [`Checker::relate_ternary`], and an
    /// `Unknown` anywhere answers `None`: this port must not pick a supertype
    /// it cannot decide.
    fn single_common_supertype(&mut self, types: &[TypeId]) -> Option<TypeId> {
        use crate::relater::{Relation, Ternary};
        let mut candidate: Option<TypeId> = None;
        for &t in types {
            match candidate {
                None => candidate = Some(t),
                Some(current) => match self.relate_ternary(current, t, Relation::StrictSubtype) {
                    Ternary::Related => candidate = Some(t),
                    Ternary::NotRelated => {}
                    Ternary::Unknown => return None,
                },
            }
        }
        let strict = candidate?;
        let mut all_strict = true;
        for &t in types {
            if t == strict {
                continue;
            }
            match self.relate_ternary(t, strict, Relation::StrictSubtype) {
                Ternary::Related => {}
                Ternary::NotRelated => {
                    all_strict = false;
                    break;
                }
                Ternary::Unknown => return None,
            }
        }
        if all_strict {
            return Some(strict);
        }
        let mut leftmost: Option<TypeId> = None;
        for &t in types {
            match leftmost {
                None => leftmost = Some(t),
                Some(current) => match self.relate_ternary(current, t, Relation::Subtype) {
                    Ternary::Related => leftmost = Some(t),
                    Ternary::NotRelated => {}
                    Ternary::Unknown => return None,
                },
            }
        }
        leftmost
    }

    /// SS135: forget every cached answer under `root` so a contextual
    /// re-check recomputes rather than serving pre-context state - the
    /// summit's freeze pattern generalized from "the arrow and its
    /// parameters" to the whole argument subtree (an object literal's
    /// member arrows live two levels down).
    pub(crate) fn evict_subtree(&mut self, root: NodeId) {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            self.node_types.remove(&id);
            self.contextual_this_parameters.remove(&id);
            self.resolved_call_signatures.remove(&id);
            if let Some(symbol) = self.binder.symbol_of(id) {
                self.symbol_types.remove(&symbol);
            }
            if let Some(node) = self.node_map.get(id) {
                tsr_ast::for_each_child_id(node, |child| stack.push(child));
            }
        }
    }
}
