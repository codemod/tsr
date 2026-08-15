//! Type argument inference for a call to a generic signature.
//!
//! Ported from `Checker.inferTypeArguments` (`checker.go:9390`) and the part of
//! `Checker.getSignatureInstantiation` (`checker.go:19293`) a return type needs,
//! reduced to the inference rules that need **no type relation**.
//!
//! # Why this is a slice and not the algorithm
//!
//! Upstream's inference is `inferTypes` (`inference.go:53`) — a structural walk
//! of source against target that accumulates candidates under a priority
//! lattice, tracks contravariant positions separately, and resolves each type
//! parameter through `getInferredType` (`inference.go:1406`) with fallbacks to
//! the constraint and the default. Every part of that machinery exists to handle
//! a type parameter that appears *somewhere inside* a parameter's type — `T[]`,
//! `(x: T) => U`, `Partial<T>`, a mapped type — because then the candidate has
//! to be dug out of the argument's type rather than read off it.
//!
//! [`Checker::infer_from_types`] (`bd tsr-g30h`) is four arms of that walk, and
//! its own doc comment says which and why the rest cannot be expressed here.
//! **There is still no priority lattice and no contravariant bucket**: every
//! position is walked covariantly and two positions that disagree gap the whole
//! call. The rule below is arm 1 of four.
//!
//! Measured over the corpus source carried in the `.types` baselines
//! (`vendor/typescript-go/testdata/baselines/reference/submodule`), of 1,145
//! declarations initialised by a call to a locally declared generic function:
//!
//! | shape | count | share |
//! |---|---|---|
//! | no type parameter written bare in a parameter position | 605 | 53% |
//! | every type parameter written bare in a parameter position | 370 | 32% |
//! | explicit type arguments, `f<string>(x)` | 93 | 8% |
//! | some, but not all, written bare | 77 | 7% |
//!
//! The 53% row is the cliff. `bd tsr-g30h` takes the part of it that the two
//! substitution reverse indices make decomposable — references against
//! references, signatures against signatures, and a restricted union arm — and
//! refuses the rest; `docs/architecture/checker-notes-infer2.md` sizes both
//! halves by counterfactual. The bare-parameter row remains the one where the
//! candidate *is* the argument type, a lookup rather than an inference.
//!
//! # Nothing is widened, and that is not an omission
//!
//! `getCovariantInference` (`inference.go`) widens an inferred literal only when
//! the type parameter "was fixed during inference or does not occur at top level
//! in the return type". Both cases this module answers have the type parameter
//! *as* the return type, so upstream does not widen either, and the oracle says
//! so directly
//! (`baselines/reference/submodule/conformance/callGenericFunctionWithZeroTypeArguments.types:10`):
//!
//! ```text
//! var r = f(1);
//! >r : number
//! >f(1) : 1
//! >f : <T>(x: T) => T
//! ```
//!
//! The call is `1`; the `number` on `r` comes from the *declaration site*,
//! where [`Checker::get_widened_literal_type`] already runs. A port that widened
//! the candidate would print `number` on the call line and be wrong while
//! looking right on the variable, which is exactly why that fixture is the test.
//!
//! The other half of the rule is visible in
//! `conformance/genericCallWithConstraintsTypeArgumentInference2.types:17`,
//! where `<T, U extends T>(t: T) => U` applied to `1` prints `number`: `T` does
//! *not* occur at top level in the return type there, so upstream widens it,
//! and `U` falls back to its constraint. That call has a type parameter with no
//! bare parameter position and is a gap here — which is the reason this module
//! does not have to know about widening at all.
//!
//! # What answers `errorType`
//!
//! A spread argument, a rest parameter, a type parameter [`Checker::infer_from_types`]
//! finds no candidate for, two positions disagreeing about the same type
//! parameter, a candidate that is `null` or `undefined` (upstream widens it and
//! this port does not), a return type [`Checker::instantiate_type`] cannot
//! rebuild, an argument whose own type is a gap, and — for a call with written
//! type arguments — the wrong count of them, a defaulted type parameter, or an
//! argument that does not resolve.
//!
//! # Written type arguments need substitution, not inference, and that is why
//! they are here
//!
//! `f<string>(x)` skips inference entirely: `checkTypeArguments`
//! (`checker.go:9269`) validates what the caller wrote and
//! `getSignatureInstantiation` (`checker.go:19293`) substitutes it. So the two
//! paths share their *second* half — the substitution — which is why they share
//! a module, and why the same limit binds both. Of the 40 explicit-type-argument
//! calls in the corpus whose callee has a written return annotation, 14 return a
//! bare type parameter and are answered, 5 mention no type parameter and are
//! free, and **21 return a type that merely contains one** — `T[]`, `[T, U]`,
//! `C<T>` — which needs a structural rebuild.
//!
//! That rebuild is now [`Checker::instantiate_type`], and it covers exactly the
//! shapes whose construction is interned on a `(symbol, arguments)` pair:
//! `T[]`, `Array<T>`, `C<T>` and a union of those. **`[T, U]` and `(x: T) => U`
//! are still gaps**, and not by oversight — a tuple and a function type are not
//! built through `create_type_reference`, so there is no pair to reverse and
//! nothing to rebuild them from. They become answerable when they gain a
//! structured `TypeData`, not before.
//!
//! **The constraint check is not ported.** `f<string>(x)` where `T extends
//! number` is an error upstream and answers `string` here. That is the one
//! wrong answer in this module rather than a gap, and it is accepted because
//! gapping every constrained type parameter would gap the correct calls too,
//! and `is_type_assignable_to` cannot judge a constraint outside the primitive
//! domains anyway (`crate::calls::SELECTABLE` records the same limit). Every
//! such call is already a diagnostic upstream, so the line is not one a correct
//! program contains.

use tsr_ast::{Expression, Node, NodeId};

use crate::{
    checker::Checker,
    signatures::Signature,
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
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
        let error = self.intrinsics.error;
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
            if is_context_sensitive_argument(&argument) {
                argument_types.push(self.intrinsics.error);
            } else {
                argument_types.push(self.check_expression(argument));
            }
        }
        if spread {
            for &argument in arguments {
                if is_context_sensitive_argument(&argument) {
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
        // §35's contained leg (`checker-notes-callres.md`): a type
        // parameter whose CONSTRAINT is a minted deferred type (`keyof T`)
        // steers upstream's literal retention (`isLiteralOfContextualType`
        // keeps `'b'` under a keyof-constrained parameter) — machinery this
        // inference lacks, and the mint had been the accidental gate. The
        // call declines as it always had.
        if signature.type_parameters.iter().any(|parameter| {
            parameter
                .constraint
                .is_some_and(|constraint| self.unresolved_types.contains(&constraint))
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
        if benign && instantiated.is_none() && !arguments.iter().any(is_context_sensitive_argument)
        {
            return returned;
        }
        let decline = if benign { returned } else { error };

        // A rest parameter makes position-to-argument mapping a tuple problem
        // (`getSpreadArgumentType`, `checker.go`), so the whole signature is a
        // gap rather than the rest position alone.
        if signature.parameters.iter().any(|parameter| parameter.rest) {
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
        // Stage 1, third spec (the read's literal text): the return seed
        // builds a SEPARATE returnMapper consulted ONLY when instantiating
        // contextual types (the memo below); it never enters the inference
        // set and never the final map.
        let mut return_mapper: Vec<InferenceInfo> = Vec::new();
        if let Some(call_id) = call
            && let Some(outer) = self.get_contextual_type_of_call(call_id)
        {
            self.infer_from_types(outer, returned, &parameters, &mut return_mapper, 0);
        }
        let mut deferred: Vec<usize> = Vec::new();
        // SS140: candidates collect into PER-ARGUMENT buckets merged in
        // index order below, so a later argument's candidate cannot outrank
        // an earlier literal member's (the E1-vs-E2 order bug) - while the
        // EXECUTION order stays exactly SS135's (interleaving the checks
        // themselves measured -610: early member checks freeze the summit
        // families through the caches).
        let mut buckets: Vec<Vec<InferenceInfo>> = vec![Vec::new(); arguments.len().max(1)];
        for (index, parameter) in signature.parameters.iter().enumerate() {
            let Some(&argument_expression) = arguments.get(index) else { continue };
            if is_context_sensitive_argument(&argument_expression) {
                deferred.push(index);
                continue;
            }
            let Some(&argument) = argument_types.get(index) else { continue };
            self.infer_from_types(argument, parameter.r#type, &parameters, &mut buckets[index], 0);
        }
        if !deferred.is_empty()
            && let Some(call_id) = call
        {
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
                            if is_context_sensitive_argument(&element) {
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
                for property in literal.properties {
                    // SS138: a NON-context-sensitive method member harvests
                    // its signature type against the property type.
                    if let tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) = property
                        && !method.parameters.iter().any(|p| p.r#type.is_none())
                        && let Some(method_id) = method.node_id
                    {
                        let name = match method.name {
                            tsr_ast::PropertyName::Identifier(name) => name.text,
                            tsr_ast::PropertyName::StringLiteral(name) => name.text,
                            _ => continue,
                        };
                        let Some(property_symbol) = self.get_property_of_type(parameter_type, name)
                        else {
                            continue;
                        };
                        let property_type = self.get_type_of_symbol(property_symbol);
                        let checked = self.get_type_of_function_expression(method_id);
                        if checked != error {
                            self.infer_from_types(
                                checked,
                                property_type,
                                &parameters,
                                &mut buckets[index],
                                0,
                            );
                        }
                        continue;
                    }
                    let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) =
                        property
                    else {
                        continue;
                    };
                    let Some(value) = assignment.initializer else { continue };
                    let name = match assignment.name {
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
                        let image = self.instantiate_for_reference(parameter_type, property_type);
                        if image == error { property_type } else { image }
                    };
                    // SS137 (slice 2): members process IN ORDER, a
                    // context-sensitive value checking under the inferences
                    // accumulated so far (upstream's non-omitted pass with
                    // inferFromIntraExpressionSites firing per site) - its
                    // RETURN then contributes: `produce: _a => 0` infers
                    // T := number for `consume`.
                    if is_context_sensitive_argument(&value)
                        && let Some(literal_id) = literal.node_id
                    {
                        let so_far: Vec<(TypeId, TypeId)> = {
                            let mut merged: Vec<InferenceInfo> = Vec::new();
                            for bucket in &buckets {
                                for info in bucket {
                                    for &candidate in &info.candidates {
                                        add_candidate(&mut merged, info.type_parameter, candidate);
                                    }
                                }
                            }
                            flatten_infos(&merged)
                        };
                        self.intra_expression_member_maps.insert(
                            literal_id,
                            (
                                so_far,
                                parameters.clone(),
                                names.iter().map(ToString::to_string).collect(),
                            ),
                        );
                        if let Some(value_id) = value.node_id() {
                            self.evict_subtree(value_id);
                        }
                    }
                    let checked = self.check_expression(value);
                    self.infer_from_types(
                        checked,
                        property_type,
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
            let mut partial: Vec<(TypeId, TypeId)> = flatten_infos(&infos);
            // argument-partial OVER returnMapper: mapper entries fill only
            // parameters the arguments left empty.
            for entry in flatten_infos(&return_mapper) {
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
                    && self.mentions_type_parameter(returned, &[type_parameter], &[names[position]])
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
            let mut memo = signature.clone();
            for parameter in &mut memo.parameters {
                let image = self.instantiate_type(parameter.r#type, &partial, &parameters, &names);
                // With the fill the map is TOTAL and the image always
                // serves; without it (contextual-position call) the SS75
                // mentions guard returns — a half-instantiated context was
                // the first reunion's measured 363-G-to-W cause.
                if image != error
                    && (filled || !self.mentions_type_parameter(image, &parameters, &names))
                {
                    parameter.r#type = image;
                    for &(consumed, _) in &partial {
                        if let Some(info) = infos.iter_mut().find(|i| i.type_parameter == consumed)
                        {
                            info.is_fixed = true;
                        }
                    }
                }
            }
            let owns_memo = !self.call_inference_signatures.contains_key(&call_id);
            if owns_memo {
                self.call_inference_signatures.insert(call_id, memo);
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
                    self.infer_from_types(checked, parameter.r#type, &parameters, &mut infos, 0);
                }
            }
            if owns_memo {
                self.call_inference_signatures.remove(&call_id);
                // The member maps PERSIST - member reads are LAZY (the
                // walker asks for property/parameter types long after this
                // call resolved), and upstream's answer is stable because
                // the resolved signature's mapper never expires. A window
                // here measured ZERO: every read arrived after removal.
                let _ = registered_literals;
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
        let candidates: Vec<(TypeId, TypeId)> = flatten_infos(&infos);
        let mut map = Vec::with_capacity(parameters.len());
        for (position, &type_parameter) in parameters.iter().enumerate() {
            // `getCovariantInference` (`inference.go`) unions the candidates.
            // This port cannot build that union in general — it would need the
            // subtype reduction refused at `bd tsr-eak` — but `never` is the
            // **identity** for union and `add_type_to_union` (`crate::unions`)
            // drops it, so a `never` candidate beside any other contributes
            // nothing and can be struck without deciding anything.
            //
            // `f([], 3)` against `<T>(arr: T[], elemnt: T) => T` is the case
            // that forced this: `T[]` against `never[]` yields `never`, `T`
            // against `3` yields `3`, and upstream answers `3`
            // (`baselines/reference/submodule/compiler/undefinedInferentialTyping.types:12`).
            // Without the strike the two disagree and the call gaps — which is
            // how the bar's second leg found it.
            let never = self.intrinsics.never;
            let has_other = candidates
                .iter()
                .any(|&(from, inferred)| from == type_parameter && inferred != never);
            // This parameter's candidates in add order, never-struck (see
            // above), deduped — `getCovariantInference`'s input.
            let list: Vec<TypeId> = {
                let mut seen = Vec::new();
                for &(from, inferred) in &candidates {
                    if from == type_parameter
                        && !(has_other && inferred == never)
                        && !seen.contains(&inferred)
                    {
                        seen.push(inferred);
                    }
                }
                seen
            };
            // §162 → the pipeline (`getCovariantInference`,
            // `inference.go:1434`): the widening decision now reads the REAL
            // `topLevel`/`isFixed` fields (the foundation's steps 2/4) —
            // `widenLiteralTypes := !primitiveConstraint && inference.topLevel
            // && (inference.isFixed || !isTypeParameterAtTopLevelInReturnType(..))`.
            // `typeArgumentsWithStringLiteralTypes01` sits on the isFixed
            // disjunct (a consumed context widens DESPITE a top-level return);
            // `<T>(x: T) => T` on the unfixed one (keeps `5`, upstream too).
            let top_level = infos
                .iter()
                .find(|info| info.type_parameter == type_parameter)
                .is_none_or(|info| info.top_level);
            let is_fixed = infos
                .iter()
                .find(|info| info.type_parameter == type_parameter)
                .is_some_and(|info| info.is_fixed);
            let primitive_constraint = self.parameter_has_primitive_constraint(signature, position);
            let widen_literals = !primitive_constraint
                && top_level
                && (is_fixed
                    || !self
                        .is_type_parameter_at_top_level_in_return_type(signature, type_parameter));
            let mut candidate = None;
            if !list.is_empty() {
                // Stage 2 — base candidates under the widening decision
                // (`inference.go:1444-1451`); a primitive-flavored constraint
                // keeps literals regular, widening maps each to its base.
                let base: Vec<TypeId> = {
                    let mut out = Vec::with_capacity(list.len());
                    for &t in &list {
                        let mapped = if primitive_constraint {
                            self.get_regular_type_of_literal_type(t)
                        } else if widen_literals {
                            self.get_widened_literal_type(t)
                        } else {
                            t
                        };
                        if !out.contains(&mapped) {
                            out.push(mapped);
                        }
                    }
                    out
                };
                // Stage 3 — combination (`getCommonSupertype`,
                // `inference.go:1530`): same-base literal sets union;
                // everything else takes the leftmost-supertype walk, with the
                // relater's third verdict declining the call whole — a gap
                // beats a wrong. Stage 4 (`getWidenedType`) is identity here:
                // this port's candidates carry no freshness to erase, and
                // object-literal widening is unported at this site (stated).
                let resolved =
                    if base.len() == 1 { Some(base[0]) } else { self.covariant_combination(&base) };
                let Some(resolved) = resolved else { return decline };
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
                            }) && self.mentions_type_parameter(
                                parameter.r#type,
                                &[type_parameter],
                                &[name],
                            )
                        });
                    if structural_source_supplied {
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
        if let Some(slot) = instantiated {
            let mut instance = signature.clone();
            for parameter in &mut instance.parameters {
                let image = self.instantiate_type(parameter.r#type, &map, &parameters, &names);
                if image == error {
                    return decline;
                }
                parameter.r#type = image;
            }
            let returned_image = self.instantiate_type(returned, &map, &parameters, &names);
            if returned_image == error {
                return decline;
            }
            instance.r#type = returned_image;
            instance.type_parameters = Vec::new();
            *slot = Some(instance);
            return returned_image;
        }
        self.instantiate_type(returned, &map, &parameters, &names)
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
    ///    argument. **Variance is not consulted**: upstream picks
    ///    covariant/contravariant/invariant per position, and every position
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
        // Every entry walk's ORIGINAL target is the target it starts from;
        // the recursion below preserves it so the top-level test
        // (`inference.go:207-208`) reads the whole parameter type, not the
        // constituent the walk has descended to.
        self.infer_from_types_within(source, target, target, parameters, out, depth);
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
        if parameters.contains(&target) {
            add_candidate(out, target, source);
            // `inference.go:207-208`: a candidate arriving where the walk's
            // original target does not carry the parameter at TOP LEVEL marks
            // the inference nested, and the widening decision reads it.
            if !self.is_type_parameter_at_top_level(original, target)
                && let Some(info) = out.iter_mut().find(|i| i.type_parameter == target)
            {
                info.top_level = false;
            }
            return;
        }
        let target_reference = self.type_reference_targets.get(&target).cloned();
        let source_reference = self.type_reference_targets.get(&source).cloned();
        if let (Some((ts, ta)), Some((ss, sa))) = (target_reference, source_reference) {
            if ts == ss && ta.len() == sa.len() {
                for (t, s) in ta.iter().zip(sa.iter()) {
                    self.infer_from_types_within(*s, *t, original, parameters, out, depth + 1);
                }
            }
            return;
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
            if !source_is_wildcard
                && constituents
                    .iter()
                    .any(|&c| !parameters.contains(&c) && self.is_type_assignable_to(source, c))
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
        let (Some(target_signatures), Some(source_signatures)) =
            (self.signature_types.get(&target), self.signature_types.get(&source))
        else {
            return;
        };
        let ([t], [s]) = (target_signatures.as_slice(), source_signatures.as_slice()) else {
            return;
        };
        // A signature carrying its own type parameters is refused: upstream
        // erases them first (`getErasedSignature`, `checker.go:19700`), which
        // this port has no route to.
        if !t.type_parameters.is_empty() || !s.type_parameters.is_empty() {
            return;
        }
        if t.parameters.iter().chain(&s.parameters).any(|parameter| parameter.rest) {
            return;
        }
        let (t, s) = (t.clone(), s.clone());
        for (tp, sp) in t.parameters.iter().zip(s.parameters.iter()) {
            self.infer_from_types_within(
                sp.r#type,
                tp.r#type,
                original,
                parameters,
                out,
                depth + 1,
            );
        }
        self.infer_from_types_within(s.r#type, t.r#type, original, parameters, out, depth + 1);
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
            if let Some(evaluated) = self.evaluate_conditional_alias(symbol, &substituted) {
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
            return self.create_tuple_type(substituted, readonly);
        }
        error
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

    /// One signature with every carried type substituted, or `None` when any
    /// part refuses.
    fn instantiate_signature(
        &mut self,
        mut signature: Signature,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> Option<Signature> {
        let error = self.intrinsics.error;
        let substitute = |checker: &mut Self, id: TypeId| -> Option<TypeId> {
            let image = checker.instantiate_type(id, map, parameters, names);
            (image != error).then_some(image)
        };
        for parameter in &mut signature.type_parameters {
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

    /// Whether a type mentions any of `parameters`, anywhere.
    ///
    /// Stands in for asking `couldContainTypeVariablesWorker` (`checker.go:22184`) of an
    /// instantiated type, and is deliberately **over-eager**: it answers the
    /// identity and the constituents of a union or an intersection
    /// structurally, and then falls back to scanning the *printed* form for a
    /// type parameter's name as a whole identifier.
    ///
    /// The scan exists because a type that merely *contains* a type parameter
    /// carries no structural evidence of it here — `T[]`, `C<T>` and
    /// `(x: T) => void` are all a [`TypeData::Named`] or
    /// [`TypeData::Anonymous`] whose payload is a string. Without the scan, a
    /// signature returning `T[]` would look parameter-free and be answered with
    /// the uninstantiated `T[]`, which prints `T[]` where upstream prints
    /// `number[]`. A false positive costs a gap; a false negative costs a wrong
    /// answer, so the bias is chosen.
    pub(crate) fn mentions_type_parameter(
        &self,
        id: TypeId,
        parameters: &[TypeId],
        names: &[&str],
    ) -> bool {
        if parameters.contains(&id) {
            return true;
        }
        let ty = self.store.get(id);
        let constituents: &[TypeId] = match &ty.data {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. } => types,
            _ => &[],
        };
        if constituents.iter().any(|&t| self.mentions_type_parameter(t, parameters, names)) {
            return true;
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
    fn a_generic_source_signature_is_refused_because_erasure_is_unported() {
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
            "error"
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
    pub(crate) candidates: Vec<TypeId>,
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

pub(crate) fn add_candidate(
    infos: &mut Vec<InferenceInfo>,
    type_parameter: TypeId,
    candidate: TypeId,
) {
    if let Some(info) = infos.iter_mut().find(|i| i.type_parameter == type_parameter) {
        // `inference.go:183`: a FIXED inference set refuses new candidates —
        // the consumption rule's other half, and the whole content of the
        // `typeParameterFixing*` families. §483's pair measured its absence
        // directly (the fixing families were 8 of the 25 G→W).
        if info.is_fixed {
            return;
        }
        // Upstream dedups at the add site (`slices.Contains`,
        // `inference.go:202`); the resolver's union/supertype stages assume
        // the same.
        if !info.candidates.contains(&candidate) {
            info.candidates.push(candidate);
        }
    } else {
        infos.push(InferenceInfo {
            type_parameter,
            candidates: vec![candidate],
            is_fixed: false,
            top_level: true,
        });
    }
}

/// Fold one collection's infos into another, preserving the two fields the
/// flat pair merge used to drop: `top_level` ANDs (one nested source marks the
/// parameter nested for good, `inference.go:208`) and `is_fixed` ORs.
pub(crate) fn merge_info(infos: &mut Vec<InferenceInfo>, from: &InferenceInfo) {
    for &candidate in &from.candidates {
        add_candidate(infos, from.type_parameter, candidate);
    }
    if let Some(existing) = infos.iter_mut().find(|i| i.type_parameter == from.type_parameter) {
        existing.top_level &= from.top_level;
        existing.is_fixed |= from.is_fixed;
    }
}

pub(crate) fn flatten_infos(infos: &[InferenceInfo]) -> Vec<(TypeId, TypeId)> {
    let mut out = Vec::new();
    for info in infos {
        for &candidate in &info.candidates {
            out.push((info.type_parameter, candidate));
        }
    }
    out
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
                let flags = self.store.get(constraint).flags;
                flags.intersects(
                    TypeFlags::STRING
                        | TypeFlags::NUMBER
                        | TypeFlags::BOOLEAN
                        | TypeFlags::BIG_INT
                        | TypeFlags::UNIT,
                ) || matches!(
                    &self.store.get(constraint).data,
                    crate::types::TypeData::Union { types, .. }
                        if types.iter().all(|&t| {
                            self.store.get(t).flags.intersects(TypeFlags::UNIT)
                        })
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
    fn evict_subtree(&mut self, root: NodeId) {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            self.node_types.remove(&id);
            if let Some(symbol) = self.binder.symbol_of(id) {
                self.symbol_types.remove(&symbol);
            }
            if let Some(node) = self.node_map.get(id) {
                tsr_ast::for_each_child_id(node, |child| stack.push(child));
            }
        }
    }
}

/// A function-like argument with any unannotated parameter (upstream's
/// isContextSensitive slice relevant to call inference).
pub(crate) fn is_context_sensitive_argument(argument: &Expression<'_>) -> bool {
    let parameters = match argument {
        Expression::ArrowFunction(node) => node.parameters,
        Expression::FunctionExpression(node) => node.parameters,
        // SS135: a literal CONTAINING a context-sensitive function is itself
        // context-sensitive (upstream isContextSensitive walks object and
        // array literals) - it defers so its members can consume the
        // intra-expression inferences harvested from its other members.
        Expression::ObjectLiteralExpression(node) => {
            return node.properties.iter().any(|property| match property {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.initializer.as_ref().is_some_and(is_context_sensitive_argument)
                }
                // SS138: a method member with an unannotated parameter makes
                // the literal context-sensitive exactly as an arrow value
                // does (upstream isContextSensitive on the method).
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => {
                    method.parameters.iter().any(|p| p.r#type.is_none())
                }
                _ => false,
            });
        }
        Expression::ArrayLiteralExpression(node) => {
            return node.elements.iter().any(is_context_sensitive_argument);
        }
        _ => return false,
    };
    parameters.iter().any(|p| p.r#type.is_none())
}
