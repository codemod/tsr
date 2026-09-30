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

/// A snapshot of the active `InferenceContext` used by an inner call's
/// `cloneInferenceContext(..., NoDefault)` (internal/checker/checker.go).
#[derive(Clone, Debug)]
pub(crate) struct InferenceContextSnapshot {
    pub(crate) signature: Signature,
    pub(crate) inferences: Vec<InferenceInfo>,
    pub(crate) return_inferences: Vec<InferenceInfo>,
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

    fn check_generic_call_worker(
        &mut self,
        signature: &Signature,
        call: Option<NodeId>,
        arguments: &[Expression<'_>],
        instantiated: Option<&mut Option<Signature>>,
        overload_failure: bool,
    ) -> TypeId {
        let error = self.intrinsics.error;
        // inferSignatureInstantiationForOverloadFailure (checker.go) skips
        // context-sensitive arguments. A function requiring more parameters
        // than its callback context makes applicability fail by arity alone.
        let skip_context_sensitive =
            overload_failure || self.generic_callback_arity_failure(signature, arguments);
        // Upstream checks every argument (`checkExpression` through
        // `getEffectiveCallArguments`) whatever it then does with them, and the
        // arguments are needed here anyway. A spread has no single position to
        // land on, so it is a gap — but only after the arguments are checked,
        // so the gap does not swallow their own lines.
        let mut argument_types: Vec<TypeId> = Vec::with_capacity(arguments.len());
        let mut spread = false;
        for &argument in arguments {
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
                argument_types.push(self.check_expression(argument));
            }
        }
        if spread {
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
        let Some(parameters) = self.type_parameter_types(signature) else {
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
            if !self.mentions_type_parameter(returned, &parameters, &names) {
                return returned;
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
            return self.instantiate_type(returned, &map, &parameters, &names);
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
        // A SPREAD argument at or after the rest position keeps the decline:
        // upstream gives such an element `ElementFlagsVariadic` or
        // `ElementFlagsRest`, and this port's tuple side table has no
        // per-element flags at all (§950's forcing constraint), so the tuple
        // built here would silently claim a required element where upstream
        // records a variadic one.
        let spread_rest_position = rest_parameters
            .iter()
            .filter(|parameter| rest_element(self, parameter).is_none())
            .filter_map(|parameter| {
                signature.parameters.iter().position(|other| other.name == parameter.name)
            })
            .min();
        //
        // **A `TYPE_PARAMETER`-only gate was tried here and removed.** Upstream
        // tests `restType.flags&TypeFlagsTypeParameter != 0` one branch above
        // (`checker.go:9467`) but only to set `impliedArity` — it gates nothing.
        // Adding it as a gate measured **19 adverse to 18**, i.e. nothing, and
        // it would have been a restriction this port invented. Recorded because
        // the hypothesis it tested was wrong: the adverse rows are NOT
        // tuple-typed rests.
        let non_array_rest_is_inferrable = match spread_rest_position {
            Some(position) => !arguments
                .iter()
                .skip(position)
                .any(|argument| matches!(argument, tsr_ast::Expression::SpreadElement(_))),
            None => true,
        };
        if !non_array_rest_is_inferrable {
            return decline;
        }
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
        let mut infos: Vec<InferenceInfo> = Vec::new();
        // inferTypeArguments (checker.go) makes a weak ReturnType inference
        // into the final context and an independent ordinary-priority pass
        // for returnMapper. Our contextual-type road supplies written types;
        // unannotated binding-pattern contexts are not computed here.
        let mut return_mapper: Vec<InferenceInfo> = Vec::new();
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
                &parameters,
                &mut infos,
                0,
                InferencePriority::RETURN_TYPE,
            );
            self.infer_from_types(return_source, returned, &parameters, &mut return_mapper, 0);
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
        let mut inferred_type_parameters = Vec::new();
        for (index, parameter) in signature.parameters.iter().enumerate() {
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
                    // §951: the non-array rest — `getSpreadArgumentType`
                    // (`checker.go:29500`) without the spread-argument and
                    // iterated-element branches, which the guard above
                    // excluded. Each remaining argument is one REQUIRED
                    // element, widened exactly as upstream widens it:
                    //
                    // ```go
                    // t = c.getWidenedLiteralType(argType)
                    // info.flags = ElementFlagsRequired
                    // ```
                    //
                    // The widening is what makes `f("a", 1)` infer
                    // `[string, number]` rather than `["a", 1]`; a literal
                    // element would be a wrong answer that happens to print
                    // plausibly. The `inConstContext` /
                    // `hasPrimitiveContextualType` branch that keeps the
                    // literal is not ported — `isConstTypeVariable` needs the
                    // `const` modifier on the type parameter, and this arm
                    // declines nothing by widening: a const-modified rest is
                    // measured in `typeParameterConstModifiers`.
                    let mut spread_elements = Vec::with_capacity(arguments.len());
                    for position in index..arguments.len() {
                        let Some(&argument) = argument_types.get(position) else { continue };
                        let widened = self.get_widened_literal_type(argument);
                        spread_elements.push(widened);
                    }
                    let spread = self.create_tuple_type(spread_elements, false);
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
            // The consumption rule: serve the pass-1 inferences as the
            // instantiated contexts for the deferred arguments, marking the
            // consumed parameters fixed. Parameters whose types still
            // mention unmapped type parameters serve UNINSTANTIATED (the
            // SS75 semantics) rather than half-instantiated (the first
            // reunion's measured 363-G-to-W cause).
            // SS135 intra-expression harvest (inferFromIntraExpressionSites,
            // inference.go:1285): a deferred OBJECT LITERAL's
            // non-context-sensitive property values infer against the
            // parameter's corresponding property types BEFORE the serve map
            // builds, so the literal's context-sensitive members see the
            // committed inferences - callIt's produce fixes T for consume.
            for &index in &deferred {
                let Some(parameter_type) = signature.parameters.get(index).map(|p| p.r#type) else {
                    continue;
                };
                // SS138: ARRAY literal elements harvest against tuple
                // element types - the SS68.2 element road serves the memo's
                // instantiated tuple, so harvest is the missing half.
                if let Some(&Expression::ArrayLiteralExpression(array)) = arguments.get(index) {
                    let elements = self.tuple_element_lists.get(&parameter_type).cloned();
                    if let Some((element_types, _)) = elements {
                        for (position, &element) in array.elements.iter().enumerate() {
                            if self.is_context_sensitive_argument(&element) {
                                continue;
                            }
                            let Some(&element_type) = element_types.get(position) else {
                                continue;
                            };
                            let checked = self.check_expression(element);
                            self.infer_from_types(
                                checked,
                                element_type,
                                &parameters,
                                &mut buckets[index],
                                0,
                            );
                        }
                    }
                    continue;
                }
                let Some(&Expression::ObjectLiteralExpression(literal)) = arguments.get(index)
                else {
                    continue;
                };
                // inferFromIntraExpressionSites processes the non-contextual
                // sites before a fixing read. Context-sensitive sites then
                // contribute in their written order.
                for contextual_pass in [false, true] {
                    if contextual_pass && skip_context_sensitive {
                        continue;
                    }
                    for property in literal.properties {
                        let (property_name, value, method) = match property {
                            tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                                let Some(value) = assignment.initializer else { continue };
                                (assignment.name, Some(value), None)
                            }
                            tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => {
                                (method.name, None, Some(method))
                            }
                            _ => continue,
                        };
                        let contextual_site = value.as_ref().map_or_else(
                            || {
                                method.is_some_and(|method| {
                                    method.node_id.is_some_and(|id| {
                                        self.is_context_sensitive_function_like(id)
                                    })
                                })
                            },
                            |argument| self.is_context_sensitive_argument(argument),
                        );
                        if contextual_site != contextual_pass {
                            continue;
                        }
                        let site_id = value
                            .and_then(|value| value.node_id())
                            .or_else(|| method.and_then(|method| method.node_id));
                        let name = match property_name {
                            tsr_ast::PropertyName::Identifier(name) => name.text,
                            tsr_ast::PropertyName::StringLiteral(name) => name.text,
                            _ => continue,
                        };
                        let Some(property_symbol) = self.get_property_of_type(parameter_type, name)
                        else {
                            continue;
                        };
                        let property_type = self.get_type_of_symbol(property_symbol);
                        // SS141: a REFERENCE parameter's member carries the
                        // target's own type parameters - instantiate through the
                        // reference so inference sees the call's parameters.
                        let property_type = {
                            let image =
                                self.instantiate_for_reference(parameter_type, property_type);
                            if image == error { property_type } else { image }
                        };
                        // Context-sensitive methods and property values use
                        // the same intra-expression mapper before their return
                        // types contribute to the following member's context.
                        if contextual_site && let Some(literal_id) = literal.node_id {
                            let so_far: Vec<(TypeId, TypeId)> = {
                                let mut merged: Vec<InferenceInfo> = Vec::new();
                                for bucket in &buckets {
                                    for info in bucket {
                                        merge_info(&mut merged, info);
                                    }
                                }
                                // InferenceTypeMapper.Map fixes a candidate at
                                // the first contextual read, before checking the
                                // body of this site or any following member.
                                if let Some(site_id) = site_id {
                                    let previous =
                                        self.intra_expression_member_maps.remove(&literal_id);
                                    let functions = value.map_or_else(
                                        || vec![site_id],
                                        |value| self.context_sensitive_functions(value),
                                    );
                                    let mut consumed = Vec::new();
                                    for function in functions {
                                        if let Some(contextual) =
                                            self.contextual_signature(function)
                                        {
                                            consumed.extend(
                                                self.consumed_contextual_parameter_types(
                                                    function,
                                                    &contextual,
                                                ),
                                            );
                                        }
                                    }
                                    if let Some(previous) = previous {
                                        self.intra_expression_member_maps
                                            .insert(literal_id, previous);
                                    }
                                    if !consumed.is_empty() {
                                        // A fixing read also fixes an inference
                                        // with no candidates to its fallback.
                                        for (position, &parameter) in parameters.iter().enumerate()
                                        {
                                            if !merged
                                                .iter()
                                                .any(|info| info.type_parameter == parameter)
                                                && consumed.iter().any(|&ty| {
                                                    self.mentions_type_parameter(
                                                        ty,
                                                        &[parameter],
                                                        &[names[position]],
                                                    )
                                                })
                                            {
                                                merged.push(InferenceInfo {
                                                    type_parameter: parameter,
                                                    priority: InferencePriority::MAX_VALUE,
                                                    candidates: Vec::new(),
                                                    contra_candidates: Vec::new(),
                                                    fixed_type: None,
                                                    is_fixed: true,
                                                    top_level: true,
                                                });
                                            }
                                        }
                                        for info in &mut merged {
                                            let Some(position) = parameters
                                                .iter()
                                                .position(|&p| p == info.type_parameter)
                                            else {
                                                continue;
                                            };
                                            if consumed.iter().any(|&t| {
                                                self.mentions_type_parameter(
                                                    t,
                                                    &[info.type_parameter],
                                                    &[names[position]],
                                                )
                                            }) {
                                                info.is_fixed = true;
                                            }
                                        }
                                    }
                                }
                                // The non-fixing mapper still resolves every read
                                // through getInferredType. Missing candidates use
                                // their fallback, then remain open for later sites.
                                let mut map = Vec::new();
                                for (position, &parameter) in parameters.iter().enumerate() {
                                    let inferred = merged
                                        .iter()
                                        .find(|info| {
                                            info.type_parameter == parameter
                                                && (info.has_candidates()
                                                    || info.fixed_type.is_some())
                                        })
                                        .map(|info| {
                                            self.inferred_type_from_info(
                                                info, signature, position, &merged,
                                            )
                                            .unwrap_or(error)
                                        });
                                    let inferred = inferred.unwrap_or_else(|| {
                                        let declaration = &signature.type_parameters[position];
                                        declaration.default.or(declaration.constraint).map_or(
                                            self.intrinsics.unknown,
                                            |fallback| {
                                                self.instantiate_type(
                                                    fallback,
                                                    &map,
                                                    &parameters,
                                                    &names,
                                                )
                                            },
                                        )
                                    });
                                    map.push((parameter, inferred));
                                }
                                for info in merged.iter_mut().filter(|info| info.is_fixed) {
                                    info.fixed_type = map
                                        .iter()
                                        .find(|(tp, _)| *tp == info.type_parameter)
                                        .map(|(_, image)| *image);
                                    for bucket in &mut buckets {
                                        if let Some(existing) = bucket.iter_mut().find(|existing| {
                                            existing.type_parameter == info.type_parameter
                                        }) {
                                            existing.is_fixed = true;
                                            existing.fixed_type = info.fixed_type;
                                        }
                                    }
                                    if !buckets[index].iter().any(|existing| {
                                        existing.type_parameter == info.type_parameter
                                    }) {
                                        buckets[index].push(info.clone());
                                    }
                                }
                                map
                            };
                            self.intra_expression_member_maps.insert(
                                literal_id,
                                (
                                    so_far,
                                    parameters.clone(),
                                    names.iter().map(ToString::to_string).collect(),
                                ),
                            );
                            if let Some(value_id) = site_id {
                                self.evict_subtree(value_id);
                            }
                        }
                        let checked = match (value, site_id) {
                            (Some(value), _) => self.check_expression(value),
                            (None, Some(method)) => self.get_type_of_function_expression(method),
                            _ => continue,
                        };
                        self.infer_from_types(
                            checked,
                            property_type,
                            &parameters,
                            &mut buckets[index],
                            0,
                        );
                    }
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
                let Some(mut partial) = self.inference_map(&infos, signature, &parameters) else {
                    return decline;
                };
                // argument-partial OVER returnMapper: mapper entries fill only
                // parameters the arguments left empty.
                let Some(return_map) = self.inference_map(&return_mapper, signature, &parameters)
                else {
                    return decline;
                };
                for entry in return_map {
                    if !partial.iter().any(|&(tp, _)| tp == entry.0) {
                        partial.push(entry);
                    }
                }
                for (position, &type_parameter) in parameters.iter().enumerate() {
                    if !partial.iter().any(|&(tp, _)| tp == type_parameter)
                        && let Some(constraint) =
                            signature.type_parameters.get(position).and_then(|tp| tp.constraint)
                    {
                        partial.push((type_parameter, constraint));
                    }
                }
                // The FIXING mapper's final leg (`getInferredType`,
                // `inference.go:1317`, same fallback as the resolution loop's
                // no-candidate arm below): a parameter no argument or constraint
                // reached fixes to `unknown` at serve time. This supersedes the
                // SS75 uninstantiated-serve — the map is now TOTAL, so contexts
                // always serve instantiated; `someGenerics6(n => n, ...)` is the
                // head case (upstream: `(n: unknown) => unknown`; the ladder
                // test's third flip).
                // §134 returnMapper guard, same as arm (a)'s: a call in
                // contextual position keeps its unfixed parameters (upstream's
                // returnMapper is a live source there); only a
                // statement-position call fills to `unknown`.
                // §134 refined per-parameter: the returnMapper sources only
                // parameters that APPEAR in the return type, so a contextual
                // call protects exactly those; everything else fixes
                // (contextualTypingTwoInstancesOfSameTypeParameter's giveback
                // found the coarse call-level guard wrong by 4).
                let contextual_call = self.get_contextual_type_of_call(call_id).is_some();
                let mut filled = false;
                for (position, &type_parameter) in parameters.iter().enumerate() {
                    if partial.iter().any(|&(tp, _)| tp == type_parameter) {
                        continue;
                    }
                    if contextual_call
                        && self.mentions_type_parameter(
                            returned,
                            &[type_parameter],
                            &[names[position]],
                        )
                    {
                        continue;
                    }
                    partial.push((type_parameter, self.intrinsics.unknown));
                    filled = true;
                }
                // Totality, not fill-count, is what licenses the unconditional
                // serve below.
                let filled = filled
                    && parameters
                        .iter()
                        .all(|&type_parameter| partial.iter().any(|&(tp, _)| tp == type_parameter));
                // InferenceTypeMapper.Map fixes a parameter when a contextual
                // signature actually reads it, then calls getInferredType. Probe
                // those signatures against the uninstantiated serving memo so the
                // parameter identities survive the contextual property lookup.
                if owns_memo {
                    // Probe the raw parameter identities, before the intra-expression
                    // substitution maps the object members to their inferred types.
                    let mut member_maps = Vec::new();
                    for &index in &deferred {
                        if let Some(Expression::ObjectLiteralExpression(literal)) =
                            arguments.get(index)
                            && let Some(id) = literal.node_id
                            && let Some(map) = self.intra_expression_member_maps.remove(&id)
                        {
                            member_maps.push((id, map));
                        }
                    }
                    let mut consumed_types = Vec::new();
                    for &index in &deferred {
                        for function in self.context_sensitive_functions(arguments[index]) {
                            if let Some(contextual) = self.contextual_signature(function) {
                                consumed_types.extend(
                                    self.consumed_contextual_parameter_types(function, &contextual),
                                );
                            }
                        }
                    }
                    self.intra_expression_member_maps.extend(member_maps);
                    self.call_inference_signatures.remove(&call_id);
                    for (position, &type_parameter) in parameters.iter().enumerate() {
                        if !consumed_types.iter().any(|&contextual| {
                            self.mentions_type_parameter(
                                contextual,
                                &[type_parameter],
                                &[names[position]],
                            )
                        }) {
                            continue;
                        }
                        let Some(info_index) =
                            infos.iter().position(|info| info.type_parameter == type_parameter)
                        else {
                            infos.push(InferenceInfo {
                                type_parameter,
                                priority: InferencePriority::MAX_VALUE,
                                candidates: Vec::new(),
                                contra_candidates: Vec::new(),
                                fixed_type: partial
                                    .iter()
                                    .find(|(tp, _)| *tp == type_parameter)
                                    .map(|(_, image)| *image),
                                is_fixed: true,
                                top_level: true,
                            });
                            continue;
                        };
                        infos[info_index].is_fixed = true;
                        let info = &infos[info_index];
                        if !info.has_candidates() && info.fixed_type.is_none() {
                            infos[info_index].fixed_type = partial
                                .iter()
                                .find(|(tp, _)| *tp == type_parameter)
                                .map(|(_, image)| *image);
                            continue;
                        }
                        let Some(inferred) =
                            self.inferred_type_from_info(info, signature, position, &infos)
                        else {
                            return decline;
                        };
                        partial.retain(|&(parameter, _)| parameter != type_parameter);
                        partial.push((type_parameter, inferred));
                        infos[info_index].fixed_type = Some(inferred);
                    }
                }
                let mut memo = signature.clone();
                for parameter in &mut memo.parameters {
                    let image =
                        self.instantiate_type(parameter.r#type, &partial, &parameters, &names);

                    // With the fill the map is TOTAL and the image always
                    // serves; without it (contextual-position call) the SS75
                    // mentions guard returns — a half-instantiated context was
                    // the first reunion's measured 363-G-to-W cause.
                    if image != error
                        && (filled || !self.mentions_type_parameter(image, &parameters, &names))
                    {
                        parameter.r#type = image;
                    }
                }
                if owns_memo {
                    self.call_inference_signatures.insert(call_id, memo);
                    if !inferred_type_parameters.is_empty() || !return_mapper.is_empty() {
                        self.higher_order_context_calls.insert(call_id);
                    }
                }
                // SS135: member-map registration for deferred literal arguments.
                let mut registered_literals: Vec<tsr_ast::NodeId> = Vec::new();
                if owns_memo {
                    for &index in &deferred {
                        if let Some(&Expression::ObjectLiteralExpression(literal)) =
                            arguments.get(index)
                            && let Some(literal_id) = literal.node_id
                        {
                            self.intra_expression_member_maps.insert(
                                literal_id,
                                (
                                    partial.clone(),
                                    parameters.clone(),
                                    names.iter().map(ToString::to_string).collect(),
                                ),
                            );
                            registered_literals.push(literal_id);
                        }
                    }
                }
                for &index in &deferred {
                    if let Some(context) = self.active_inference_contexts.get_mut(&call_id) {
                        context.inferences.clone_from(&infos);
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
                    // The member maps PERSIST - member reads are LAZY (the
                    // walker asks for property/parameter types long after this
                    // call resolved), and upstream's answer is stable because
                    // the resolved signature's mapper never expires. A window
                    // here measured ZERO: every read arrived after removal.
                    let _ = registered_literals;
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
        let mut map = Vec::with_capacity(parameters.len());
        for (position, &type_parameter) in parameters.iter().enumerate() {
            let mut candidate = None;
            if let Some(info) = infos.iter().find(|info| info.type_parameter == type_parameter)
                && (info.has_candidates() || info.fixed_type.is_some())
            {
                let Some(resolved) =
                    self.inferred_type_from_info(info, signature, position, &infos)
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
            // §798: a `const` type parameter's inferred tuple is READONLY.
            // `f(['a', ['b', 'c']])` on `<const T>(x: T)` records
            // `readonly ["a", readonly ["b", "c"]]` for the call while the
            // literal itself stays `["a", ["b", "c"]]` — upstream applies the
            // readonly here, over the const type variable, not at
            // `checkArrayLiteral`. §797 had it at the literal, which made the
            // call lines right and the literal lines wrong; this moves it.
            let candidate = match candidate {
                Some(inferred)
                    if signature
                        .type_parameters
                        .get(position)
                        .is_some_and(|parameter| parameter.is_const) =>
                {
                    Some(self.readonly_tuple_image(inferred))
                }
                other => other,
            };
            match candidate {
                Some(inferred) if inferred != error => map.push((type_parameter, inferred)),
                Some(_) => return decline,
                None => {
                    // `fillMissingTypeArguments` (`checker.go:19458`), reduced
                    // to the fallback leg of `getInferredType`
                    // (`inference.go:1406`): with **no candidates**, an
                    // uninferred type parameter takes its default, instantiated
                    // with the substitutions resolved so far — `then`'s
                    // `TResult2 = never` is the head case, reached by
                    // `p.then(f)` and `p.catch()`.
                    //
                    // The guard is what keeps this from guessing: the default
                    // applies only when **no supplied argument could have been
                    // an inference source** for this parameter — no supplied
                    // bare position, and no supplied argument whose parameter's
                    // type *mentions* it. Upstream would run `inferFromTypes`
                    // structurally over such an argument (unported), so
                    // substituting the default there would answer
                    // `Promise<boolean>` where upstream infers
                    // `Promise<number>` — a confident wrong line. Those calls
                    // stay gaps.
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
                            )
                        });
                    if structural_source_supplied
                        && !infos
                            .iter()
                            .any(|info| info.type_parameter == type_parameter && info.is_fixed)
                    {
                        return decline;
                    }
                    let Some(default) = signature
                        .type_parameters
                        .get(position)
                        .and_then(|parameter| parameter.default)
                    else {
                        // `getInferredType`'s final fallback
                        // (`inference.go:1406`): no candidates, no default,
                        // no possible source — `unknownType`
                        // (`checker-notes-narrow.md` §36). §403: `anyType`
                        // at a JS call site, the same site-file split §389
                        // measured (`plainJSGrammarErrors3`'s
                        // `new Promise(undefined) : any`).
                        let fallback = if call.is_some_and(|id| self.in_js_file(id)) {
                            self.intrinsics.any
                        } else {
                            self.intrinsics.unknown
                        };
                        let fallback = if skip_context_sensitive
                            && let Some(constraint) = signature.type_parameters[position].constraint
                        {
                            self.instantiate_type(constraint, &map, &parameters, &names)
                        } else {
                            fallback
                        };
                        if fallback == error {
                            return decline;
                        }
                        map.push((type_parameter, fallback));
                        continue;
                    };
                    // A default may reference an earlier parameter
                    // (`T = U`), which is why it is instantiated with the
                    // map built so far — upstream fills left to right for
                    // the same reason.
                    let image = self.instantiate_type(default, &map, &parameters, &names);
                    if image == error {
                        return decline;
                    }
                    map.push((type_parameter, image));
                }
            }
        }
        // inferSignatureInstantiationForOverloadFailure uses a fresh inference
        // context with SkipContextSensitive. A definite applicability failure
        // therefore discards the fixing caused by callbacks in the normal pass.
        // Unknown relations cannot establish this error-recovery path.
        if !skip_context_sensitive
            && (arguments.iter().any(|argument| self.is_context_sensitive_argument(argument))
                || argument_types.iter().any(|argument| self.signature_types.get(argument).is_some_and(|signatures| matches!(signatures.as_slice(), [signature] if !signature.type_parameters.is_empty()))))
            && !signature.parameters.iter().any(|parameter| parameter.rest)
        {
            let failed =
                argument_types.iter().zip(&signature.parameters).any(|(&argument, parameter)| {
                    let image = self.instantiate_type(parameter.r#type, &map, &parameters, &names);
                    argument != error
                        && image != error
                        && self.generic_argument_is_inapplicable(argument, image)
                });
            if failed {
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
            let mut instance = signature.clone();
            for parameter in &mut instance.parameters {
                let image = self.instantiate_type(parameter.r#type, &map, &parameters, &names);
                if image == error {
                    // A caller may request the resolved signature for later
                    // contextual reads. Unsupported parameter substitution
                    // does not invalidate an independently resolved return.
                    return returned_image;
                }
                parameter.r#type = image;
            }
            instance.r#type = returned_image;
            instance.type_parameters = Vec::new();
            *slot = Some(instance);
            return returned_image;
        }
        let returned = self.instantiate_type(returned, &map, &parameters, &names);
        self.propagate_return_type_parameters(returned, &inferred_type_parameters)
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
        use crate::relater::{Relation, Ternary};
        let inferred =
            self.unconstrained_inferred_type_from_info(info, signature, position, infos)?;
        let Some(constraint) = signature.type_parameters.get(position).and_then(|p| p.constraint)
        else {
            return Some(inferred);
        };
        let owned = self.type_parameter_types(signature)?;
        let names: Vec<_> = signature.type_parameters.iter().map(|p| p.name.as_str()).collect();
        // Dependent constraints need the context's non-fixing mapper. Keep
        // them on the existing resolution road until that mapper is complete.
        if self.mentions_type_parameter(constraint, &owned, &names)
            || self.relate_ternary(inferred, constraint, Relation::Assignable)
                != Ternary::NotRelated
        {
            return Some(inferred);
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
                return Some(self.get_union_type(&filtered));
            }
        }
        let covariant = (!info.candidates.is_empty())
            .then(|| self.inferred_covariant_type(info, signature, position))
            .flatten();
        let contravariant = (!info.contra_candidates.is_empty())
            .then(|| self.inferred_contravariant_type(info))
            .flatten();
        let fallback = if covariant == Some(inferred) { contravariant } else { covariant };
        Some(
            fallback
                .filter(|&t| {
                    self.relate_ternary(t, constraint, Relation::Assignable) != Ternary::NotRelated
                })
                .unwrap_or(constraint),
        )
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
        let context = loop {
            let Some(node) = parent else { return t };
            if let Some(context) = self.active_inference_contexts.get(&node) {
                break context.clone();
            }
            parent = self.nodes.parent(node);
        };
        let Some(parameters) = self.type_parameter_types(&context.signature) else { return t };
        let names: Vec<_> =
            context.signature.type_parameters.iter().map(|p| p.name.as_str()).collect();
        let silent = if let Some(silent) = self.silent_never_type {
            silent
        } else {
            let silent =
                self.store.new_named(crate::flags::TypeFlags::NEVER, "never".to_owned(), None);
            self.silent_never_type = Some(silent);
            silent
        };
        let mut map = Vec::new();
        for (position, &parameter) in parameters.iter().enumerate() {
            let source_infos = if !no_default
                && context.return_inferences.iter().any(|info| info.type_parameter == parameter)
            {
                &context.return_inferences
            } else {
                &context.inferences
            };
            let image = source_infos
                .iter()
                .find(|info| info.type_parameter == parameter)
                .filter(|info| info.has_candidates() || info.fixed_type.is_some())
                .and_then(|info| {
                    self.inferred_type_from_info(info, &context.signature, position, source_infos)
                })
                .unwrap_or_else(|| {
                    if no_default {
                        silent
                    } else {
                        let declaration = &context.signature.type_parameters[position];
                        let fallback = declaration
                            .default
                            .or(declaration.constraint)
                            .unwrap_or(self.intrinsics.unknown);
                        self.instantiate_type(fallback, &map, &parameters, &names)
                    }
                });
            map.push((parameter, image));
        }
        self.instantiate_type(t, &map, &parameters, &names)
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

    /// Resolve mapper entries through getInferredType rather than flattening
    /// multiple candidates into substitutions for the same type parameter.
    fn inference_map(
        &mut self,
        infos: &[InferenceInfo],
        signature: &Signature,
        parameters: &[TypeId],
    ) -> Option<Vec<(TypeId, TypeId)>> {
        let mut map = Vec::new();
        for (position, &parameter) in parameters.iter().enumerate() {
            if let Some(info) = infos.iter().find(|info| info.type_parameter == parameter)
                && (info.has_candidates() || info.fixed_type.is_some())
            {
                let image = self.inferred_type_from_info(info, signature, position, infos)?;
                map.push((parameter, image));
            }
        }
        Some(map)
    }

    /// getCovariantInference (internal/checker/inference.go), shared by final
    /// type-argument resolution and a contextual fixing mapper. Object/array
    /// literal candidate normalization and getWidenedType remain separate ports.
    fn inferred_covariant_type(
        &mut self,
        info: &InferenceInfo,
        signature: &Signature,
        position: usize,
    ) -> Option<TypeId> {
        let never = self.intrinsics.never;
        let has_other = info.candidates.iter().any(|&candidate| candidate != never);
        let primitive_constraint = self.parameter_has_primitive_constraint(signature, position)
            || signature.type_parameters.get(position).is_some_and(|parameter| parameter.is_const);
        let widen_literals = !primitive_constraint
            && info.top_level
            && (info.is_fixed
                || !self
                    .is_type_parameter_at_top_level_in_return_type(signature, info.type_parameter));
        let mut base = Vec::with_capacity(info.candidates.len());
        for &candidate in &info.candidates {
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
        if let [single] = base.as_slice() {
            Some(*single)
        } else if info.priority.intersects(InferencePriority::IMPLIES_COMBINATION) {
            self.union_with_subtype_reduction(&base)
        } else {
            self.covariant_combination(&base)
        }
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
        if !source.type_parameters.is_empty() && target.type_parameters.is_empty() {
            return self.compare_signature_ternary(source, target) == Some(Ternary::NotRelated);
        }
        source.type_parameters.is_empty()
            && target.type_parameters.is_empty()
            && target.r#type != self.intrinsics.void
            && primitive_mismatch(self, source.r#type, target.r#type)
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
        let Some(return_signatures) = self.signature_types.get(&returned) else { return false };
        let [return_signature] = return_signatures.as_slice() else { return false };
        if !return_signature.type_parameters.is_empty()
            || parameters.iter().all(|parameter| {
                existing
                    .iter()
                    .any(|info| info.type_parameter == *parameter && info.has_candidates())
            })
        {
            return false;
        }
        let (Some(source_signatures), Some(target_signatures)) =
            (self.signature_types.get(&source), self.signature_types.get(&target))
        else {
            return false;
        };
        let ([source_signature], [target_signature]) =
            (source_signatures.as_slice(), target_signatures.as_slice())
        else {
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
        let (source_signature, target_signature) =
            (source_signature.clone(), target_signature.clone());
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
        let (Some(source_signatures), Some(target_signatures)) =
            (self.signature_types.get(&source), self.signature_types.get(&target))
        else {
            return false;
        };
        let ([source_signature], [target_signature]) =
            (source_signatures.as_slice(), target_signatures.as_slice())
        else {
            return false;
        };
        if source_signature.type_parameters.is_empty()
            || !target_signature.type_parameters.is_empty()
            || matches!(source_signature.kind, crate::signatures::SignatureKind::Call)
                != matches!(target_signature.kind, crate::signatures::SignatureKind::Call)
        {
            return false;
        }
        let (mut source_signature, target_signature) =
            (source_signature.clone(), target_signature.clone());
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
        let TypeData::Anonymous { symbol, .. } = self.store.get(returned).data else {
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
    fn infer_from_types(
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
        self.infer_from_types_within(source, target, target, parameters, out, depth);
        (self.inference_contravariant, self.inference_bivariant, self.inference_priority) = saved;
    }

    /// The single-variadic middle of `inferFromObjectTypes`
    /// (`internal/checker/inference.go`), using `sliceTupleType`'s mutable slice.
    /// Optional suffixes need speculative inference priority and are left to
    /// that path; multiple variadic elements need implied arity.
    fn infer_from_variadic_tuple(
        &mut self,
        source: TypeId,
        target: TypeId,
        original: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) -> bool {
        let Some((elements, _)) = self.variadic_tuple_elements.get(&target).cloned() else {
            return false;
        };
        if let [element] = elements.as_slice()
            && element.spread
            && parameters.contains(&element.r#type)
        {
            add_directional_candidate(
                out,
                element.r#type,
                source,
                self.inference_contravariant && !self.inference_bivariant,
                self.inference_priority,
            );
            return true;
        }
        let Some((source_elements, _)) = self.tuple_element_lists.get(&source).cloned() else {
            return false;
        };
        let mut rest_index = None;
        let mut targets = Vec::with_capacity(elements.len());
        for (index, element) in elements.iter().enumerate() {
            if element.optional || element.r#type == self.intrinsics.error {
                return false;
            }
            if element.spread && rest_index.replace(index).is_some() {
                return false;
            }
            targets.push(element.r#type);
        }
        let Some(start) = rest_index else { return false };
        let array_rest = self.tuple_spread_array_element(targets[start]);
        if !parameters.contains(&targets[start]) && array_rest.is_none() {
            return false;
        }
        let end_skip = targets.len() - start - 1;
        if source_elements.len() < start + end_skip {
            return false;
        }
        let end = source_elements.len() - end_skip;
        for index in 0..start {
            self.infer_from_types_within(
                source_elements[index],
                targets[index],
                original,
                parameters,
                out,
                depth + 1,
            );
        }
        if let Some(array_rest) = array_rest {
            // A zero-length slice contributes no array element inference.
            if start < end {
                let middle = self.get_union_type(&source_elements[start..end]);
                self.infer_from_types_within(
                    middle,
                    array_rest,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
            }
        } else {
            let sliced = if let Some(mask) = self.tuple_optional_masks.get(&source).cloned() {
                let labels = self
                    .tuple_labels
                    .get(&source)
                    .cloned()
                    .unwrap_or_else(|| vec![None; source_elements.len()]);
                let elements: Vec<_> = source_elements[start..end]
                    .iter()
                    .copied()
                    .zip(mask[start..end].iter().copied())
                    .collect();
                self.create_optional_tuple_type(&elements, &labels[start..end], false)
            } else {
                self.create_tuple_type(source_elements[start..end].to_vec(), false)
            };
            self.infer_from_types_within(
                sliced,
                targets[start],
                original,
                parameters,
                out,
                depth + 1,
            );
        }
        for index in 0..end_skip {
            self.infer_from_types_within(
                source_elements[end + index],
                targets[start + 1 + index],
                original,
                parameters,
                out,
                depth + 1,
            );
        }
        true
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

    /// `id` with every tuple in it — itself and its elements, recursively —
    /// re-minted readonly. §798.
    ///
    /// `getWidenedType` over a const type variable makes the inferred tuple
    /// readonly ALL THE WAY DOWN: `f(['a', ['b', 'c']])` is
    /// `readonly ["a", readonly ["b", "c"]]`. A non-tuple is returned
    /// unchanged, which is what keeps `f("b")` at `"b"`.
    fn readonly_tuple_image(&mut self, id: TypeId) -> TypeId {
        if let Some((elements, _)) = self.tuple_element_lists.get(&id).cloned() {
            let mapped: Vec<TypeId> =
                elements.into_iter().map(|element| self.readonly_tuple_image(element)).collect();
            return self.create_tuple_type(mapped, true);
        }
        // §799: the OBJECT arm. `f({ a: 1 })` on `<const T>(x: T)` is
        // `{ readonly a: 1; }` — the same `getWidenedType` over the const type
        // variable that makes a tuple readonly marks an object's members
        // readonly.
        //
        // FLAT only. `Member::Property` carries its type as printed TEXT, so a
        // nested object cannot be re-minted from here the way a nested tuple
        // can (tuple elements are `TypeId`s). A member list carrying anything
        // but properties — a signature, an index — declines whole rather than
        // marking half of it.
        let crate::types::TypeData::Named { members: Some(owner), .. } = self.store.get(id).data
        else {
            return id;
        };
        // §800: the literal's OWN members first. `spread_members_of` re-derives
        // them from the `__object` symbol, where each type comes back widened —
        // which is why §799 landed `{ readonly a: number; }` for a literal that
        // had correctly printed `{ a: 1; }`.
        let members = if let Some(members) = self.object_literal_members.get(&id).cloned() {
            members
        } else if let Some(members) = self.spread_members_of(id) {
            members
        } else {
            return id;
        };
        if members.is_empty()
            || !members
                .iter()
                .all(|member| matches!(member, crate::objects::Member::Property { .. }))
        {
            return id;
        }
        let readonly: Vec<crate::objects::Member> = members
            .into_iter()
            .map(|member| match member {
                crate::objects::Member::Property { name, optional, printed, .. } => {
                    crate::objects::Member::Property { name, optional, readonly: true, printed }
                }
                other => other,
            })
            .collect();
        let text = crate::objects::render_object_type(&readonly);
        self.store.new_named(crate::flags::TypeFlags::OBJECT, text, Some(owner))
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

    fn infer_from_types_within(
        &mut self,
        source: TypeId,
        target: TypeId,
        original: TypeId,
        parameters: &[TypeId],
        out: &mut Vec<InferenceInfo>,
        depth: usize,
    ) {
        // Not a stack guard: a recursive generic type
        // (`interface List<T> { next: List<List<T>> }`) can nest a reference
        // arbitrarily, and `instantiate_type`'s own limit sits on the other
        // side of the walk. Sixteen is far past anything the corpus reaches.
        if depth > 16 {
            return;
        }
        if self.infer_from_variadic_tuple(source, target, original, parameters, out, depth) {
            return;
        }
        if parameters.contains(&target) {
            if self.any_function_type == Some(source)
                || self.contains_silent_never(source, &mut Vec::new())
            {
                return;
            }
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
        // §801: a TUPLE target infers ELEMENT-WISE from a tuple source.
        // `f4<const T>(x: [T, T])` called with `[[1, "x"], [2, "y"]]` infers
        // `T` from BOTH positions and unions them —
        // `readonly [1, "x"] | readonly [2, "y"]`
        // (`jsdocTemplateTag6.types:137`). A tuple is not a type REFERENCE in
        // this port, so the reference arm below never saw the pair and the
        // whole call answered `errorType`.
        //
        // Equal length only. A length mismatch is upstream's variadic
        // arithmetic (a rest element absorbing several positions), which this
        // port does not have — inferring positionally across a mismatch would
        // pair the wrong source with the wrong parameter.
        if let Some((target_elements, _)) = self.tuple_element_lists.get(&target).cloned()
            && let Some((source_elements, _)) = self.tuple_element_lists.get(&source).cloned()
            && target_elements.len() == source_elements.len()
        {
            for (t, s) in target_elements.iter().zip(source_elements.iter()) {
                self.infer_from_types_within(*s, *t, original, parameters, out, depth + 1);
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
        if let TypeData::Union { types, .. } = &self.store.get(target).data {
            let constituents = types.clone();
            if constituents.iter().filter(|t| parameters.contains(t)).count() > 1 {
                return;
            }
            if matches!(self.store.get(source).data, TypeData::Union { .. }) {
                return;
            }
            // §465: a `Promise<X>` source against a `PromiseLike<T>`
            // constituent (or the reverse) infers argument-wise — upstream
            // reaches the match structurally through the `then` member, and
            // its REGULAR priority beats the naked constituent's candidate
            // (`InferencePriorityNakedTypeVariable`, inference.go), so the
            // naked `TResult1` is NOT consulted: `p.then(() =>
            // Promise.resolve(1))` infers `TResult1 := number`, not
            // `Promise<number>`. Gated to the global Promise/PromiseLike
            // pair, whose argument slots correspond by construction.
            if let Some((source_symbol, source_args)) =
                self.type_reference_targets.get(&source).cloned()
                && source_args.len() == 1
            {
                let source_symbol = self.binder.merged_symbol(source_symbol);
                let promise_like_pair = ["Promise", "PromiseLike"].iter().any(|name| {
                    self.global_type_symbol(name)
                        .is_some_and(|s| self.binder.merged_symbol(s) == source_symbol)
                });
                if promise_like_pair {
                    for constituent in &constituents {
                        let Some((constituent_symbol, constituent_args)) =
                            self.type_reference_targets.get(constituent).cloned()
                        else {
                            continue;
                        };
                        if constituent_args.len() != 1 {
                            continue;
                        }
                        let constituent_symbol = self.binder.merged_symbol(constituent_symbol);
                        let matches = ["Promise", "PromiseLike"].iter().any(|name| {
                            self.global_type_symbol(name)
                                .is_some_and(|s| self.binder.merged_symbol(s) == constituent_symbol)
                        });
                        if matches {
                            self.infer_from_types_within(
                                source_args[0],
                                constituent_args[0],
                                original,
                                parameters,
                                out,
                                depth + 1,
                            );
                            return;
                        }
                    }
                }
            }
            if let Some((source_symbol, _)) = self.type_reference_targets.get(&source).cloned()
                && constituents.iter().any(|c| {
                    self.type_reference_targets.get(c).is_some_and(|(s, _)| *s != source_symbol)
                })
            {
                return;
            }
            // `inferToMultipleTypes` (`inference.go:700`) strikes the target
            // constituents the source already matches **before** anything
            // reaches the naked type variable. `f1(1, "hello")` against
            // `<T>(x: T, y: string | T) => T` is the case: `"hello"` matches
            // the `string` constituent, so upstream infers nothing from that
            // position and the answer is `1`
            // (`baselines/reference/submodule/conformance/unionTypeInference.types:27`).
            // Without this the naked `T` also collects `"hello"`, the two
            // positions disagree and a right line becomes a gap — which is how
            // the bar's second leg found it.
            //
            // `is_type_assignable_to` decides this over exactly the domain it
            // is proved on — primitives, literals and unions of them
            // (`crate::relater`) — and answers `false` between two object types
            // rather than guessing, which is a refusal in the safe direction
            // here: it leaves the position contributing a candidate, and a
            // disagreeing candidate gaps.
            //
            // `never` and `any` are excluded as sources: both are assignable
            // to everything, so they would strike every union position and
            // contribute nothing anywhere. Upstream infers *from* them
            // normally — `never` is a real candidate — and including them cost
            // **84 converted lines** against the two the strike was added for,
            // measured over the corpus pair.
            let source_is_wildcard =
                source == self.intrinsics.never || source == self.intrinsics.any;
            // §787: the strike must skip every constituent that MENTIONS an
            // inference parameter, not only one that IS one.
            //
            // `!parameters.contains(&c)` excludes a NAKED `T` and nothing else,
            // so `readonly T[] | null` — the shape every lib collection
            // constructor uses — was struck whole: `Array<number>` is
            // assignable to `ReadonlyArray<T>`, the strike fired, and `T`
            // collected no candidate at all. `new Set([0, 1, 2])` answered
            // `Set<any>` through the `T = any` default.
            //
            // Upstream's `inferToMultipleTypes` (`inference.go:700`) cannot
            // reach that state: its first pass matches constituents
            // IDENTICALLY, and a constituent carrying an uninferred parameter
            // is identical to nothing. Striking on assignability instead is
            // this port's approximation, and it is sound only where the
            // constituent is closed.
            if !source_is_wildcard
                && constituents.clone().into_iter().any(|c| {
                    !self.mentions_type_parameter(c, parameters, &[])
                        && self.is_type_assignable_to(source, c)
                })
            {
                return;
            }
            for constituent in constituents {
                self.infer_from_types_within(
                    source,
                    constituent,
                    original,
                    parameters,
                    out,
                    depth + 1,
                );
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
        let target_names =
            if self.target_could_contain_parameter(target, parameters, &mut Vec::new()) {
                self.property_names_of(target)
            } else {
                Vec::new()
            };
        if !target_names.is_empty() {
            for name in &target_names {
                let (Some(target_member), Some(source_member)) = (
                    self.get_type_of_property_of_type(target, name),
                    self.get_type_of_property_of_type(source, name),
                ) else {
                    continue;
                };
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
                for info in &target_infos {
                    let Some(from) =
                        source_infos.iter().find(|candidate| candidate.key == info.key)
                    else {
                        continue;
                    };
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
        }
        for kind in
            [crate::signatures::SignatureKind::Call, crate::signatures::SignatureKind::Construct]
        {
            let (Some(target_signatures), Some(source_signatures)) = (
                self.signatures_of_type_kind(target, kind),
                self.signatures_of_type_kind(source, kind),
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
                self.infer_from_signature_parameters(&s, &t, original, parameters, out, depth);
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
            let target_rest =
                self.normalize_variadic_tuple(target_elements[target_start..].to_vec(), false);
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
            } else {
                self.normalize_variadic_tuple(source_elements[paired..].to_vec(), false)
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
    fn signature_for_inference(
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
        if let Some(&operand) = self.deferred_keyof_operands.get(&id) {
            let operand = self.instantiate_type(operand, map, parameters, names);
            return self.resolved_keyof_type(operand).unwrap_or(error);
        }
        if let Some((object, index, include_undefined)) =
            self.deferred_indexed_access_types.get(&id).copied()
        {
            let object = self.instantiate_type(object, map, parameters, names);
            let index = self.instantiate_type(index, map, parameters, names);
            if object == error || index == error {
                return error;
            }
            return self
                .resolved_indexed_access_type(object, index, include_undefined)
                .unwrap_or(error);
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
            // variables in the first place. `awaited_type_no_alias`'s domain
            // is exactly the concrete types: a type-variable argument
            // declines there and the reference keeps its written name, which
            // is also upstream's spelling for it. This is what turns
            // `resolve<T>(value: T): Promise<Awaited<T>>` instantiated at
            // `string` into `Promise<string>`.
            if let [argument] = substituted.as_slice()
                && self.global_type_symbol("Awaited").is_some_and(|awaited| {
                    self.binder.merged_symbol(awaited) == self.binder.merged_symbol(symbol)
                })
                && let Some(awaited) = self.awaited_type_no_alias(*argument)
            {
                return awaited;
            }
            // §136: a rebuild keeps the source reference's written display
            // arity — the spelling survives instantiation.
            let display = self.reference_display_arity.get(&id).copied();
            return self.create_type_reference_with_display(symbol, substituted, display);
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
    /// property-only type literals. Keep the owner for member identity and
    /// capture mapped member types for subsequent reads and instantiations.
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
        for property in &mut properties {
            let original = property.r#type;
            property.r#type = self.instantiate_type(original, map, parameters, names);
            if property.r#type == self.intrinsics.error {
                return self.intrinsics.error;
            }
            if property.r#type != original {
                property.printed_type = self.type_to_string(property.r#type);
            }
            rendered.push(crate::objects::Member::Property {
                name: property.printed_name.clone(),
                optional: property.optional,
                readonly: property.readonly,
                printed: property.printed_type.clone(),
            });
        }
        let text = crate::objects::render_object_type(&rendered);
        let minted = self.store.new_named(crate::flags::TypeFlags::OBJECT, text, owner);
        self.anonymous_properties.insert(minted, (properties, true));
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
            let Some(image) = self.instantiate_signature(signature, map, parameters, names) else {
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
        let (text, signature_node) = match printed.as_slice() {
            [] => return error,
            [signature] => (self.signature_to_string(signature), true),
            many => {
                let mut out = String::from("{ ");
                for signature in many {
                    out.push_str(&crate::objects::signature_member_text(self, signature));
                    out.push_str("; ");
                }
                out.push('}');
                (out, false)
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
        if renamed_print {
            self.alias_named_signature_types.insert(minted);
        }
        // Recorded in `signature_types` too, so an instantiated signature can
        // be instantiated again — `C<T>` inside `D<U>` reaches that.
        self.signature_types.insert(minted, instantiated);
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
    /// - SHADOW (`typeParameterShadowsOtherTypeParameterInScope`,
    ///   `nodebuilderimpl.go:1396`): a parameter whose name resolves at the
    ///   print SITE to a DIFFERENT type-parameter symbol renames — uniformly
    ///   `name_1`, however many collide, and without claiming the suffix.
    /// - BYTEXT (`:1420`): at a site where the name resolves to nothing, a
    ///   LATER signature's same-named distinct parameter takes the first
    ///   free `name_n` and claims it.
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
            let fresh_name = shadowed.then(|| format!("{}_1", parameter.name));
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
        let own: Vec<_> = signature.type_parameters.iter().map(|p| p.name.clone()).collect();
        let map: Vec<_> = parameters
            .iter()
            .zip(arguments)
            .filter_map(|((id, name), argument)| (!own.contains(name)).then_some((*id, argument)))
            .collect();
        let ids: Vec<_> =
            parameters.iter().filter(|(_, name)| !own.contains(name)).map(|(id, _)| *id).collect();
        let names: Vec<_> = parameters
            .iter()
            .filter(|(_, name)| !own.contains(name))
            .map(|(_, name)| name.as_str())
            .collect();
        self.instantiate_signature(signature, &map, &ids, &names)
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
        self.mentions_type_parameter_inner(id, parameters, names, &mut Vec::new())
    }

    /// Type-parameter identity through the type graph, including bound generic
    /// signatures. The printed fallback is retained only for shapes without
    /// structural metadata; a same-named bound parameter never matches it.
    fn mentions_type_parameter_inner(
        &self,
        id: TypeId,
        parameters: &[TypeId],
        names: &[&str],
        visited: &mut Vec<TypeId>,
    ) -> bool {
        if parameters.contains(&id) {
            return true;
        }
        if visited.contains(&id) {
            return false;
        }
        visited.push(id);
        let ty = self.store.get(id);
        if ty.flags.intersects(
            crate::flags::TypeFlags::TYPE_PARAMETER
                | crate::flags::TypeFlags::PRIMITIVE
                | crate::flags::TypeFlags::ANY_OR_UNKNOWN
                | crate::flags::TypeFlags::NEVER,
        ) {
            return false;
        }
        if let Some((_, arguments)) = self.type_reference_targets.get(&id) {
            return arguments
                .iter()
                .any(|&ty| self.mentions_type_parameter_inner(ty, parameters, names, visited));
        }
        if let Some(signatures) = self.signature_types.get(&id) {
            return signatures.iter().any(|signature| {
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
                    .any(|ty| self.mentions_type_parameter_inner(ty, parameters, names, visited))
            });
        }
        if let Some((properties, _)) = self.anonymous_properties.get(&id) {
            return properties.iter().any(|property| {
                self.mentions_type_parameter_inner(property.r#type, parameters, names, visited)
            });
        }
        if let Some((elements, _)) = self.tuple_element_lists.get(&id) {
            return elements
                .iter()
                .any(|&ty| self.mentions_type_parameter_inner(ty, parameters, names, visited));
        }
        let constituents: &[TypeId] = match &ty.data {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. } => types,
            _ => &[],
        };
        if !constituents.is_empty() {
            return constituents
                .iter()
                .any(|&t| self.mentions_type_parameter_inner(t, parameters, names, visited));
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
        let source = "declare function f<T>(x: T): T;\nvar a = f(null);";
        assert_eq!(generic_call_with_strictness(source, "f", true), "null");
        assert_eq!(generic_call_with_strictness(source, "f", false), "error");
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
        existing.top_level &= from.top_level;
        existing.is_fixed |= from.is_fixed;
        if existing.fixed_type.is_none() {
            existing.fixed_type = from.fixed_type;
        }
    }
}

impl Checker<'_, '_> {
    /// `isTypeParameterAtTopLevel` (`inference.go:1493-1499`), verbatim over
    /// the shapes this port has: the type IS the parameter, or a union whose
    /// constituents contain it at top level. Intersections and conditionals
    /// are the two arms whose `TypeData` this port does not walk; both answer
    /// `false`, the conservative side (it widens where upstream might not,
    /// and the pair watches that).
    fn is_type_parameter_at_top_level(&self, id: TypeId, parameter: TypeId) -> bool {
        if id == parameter {
            return true;
        }
        match &self.store.get(id).data {
            crate::types::TypeData::Union { types, .. } => {
                types.iter().any(|&t| self.is_type_parameter_at_top_level(t, parameter))
            }
            _ => false,
        }
    }

    /// `isTypeParameterAtTopLevelInReturnType` (`inference.go:1501-1507`)
    /// over the return type; the type-predicate leg is unreachable here (a
    /// construct signature carries none).
    fn is_type_parameter_at_top_level_in_return_type(
        &self,
        signature: &Signature,
        parameter: TypeId,
    ) -> bool {
        self.is_type_parameter_at_top_level(signature.r#type, parameter)
    }

    /// `hasPrimitiveConstraint`'s test as this port already spelled it inside
    /// [`Checker::same_base_literal_supertype`], lifted so the single-candidate
    /// road can ask the same question (§162).
    fn parameter_has_primitive_constraint(
        &self,
        signature: &Signature,
        parameter_position: usize,
    ) -> bool {
        use crate::flags::TypeFlags;
        signature.type_parameters.get(parameter_position).and_then(|tp| tp.constraint).is_some_and(
            |constraint| {
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
