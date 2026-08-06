//! Type argument inference for a call to a generic signature.
//!
//! Ported from `Checker.inferTypeArguments` (`checker.go:9390`) and the part of
//! `Checker.getSignatureInstantiation` (`checker.go:19293`) a return type needs,
//! reduced to the **one inference rule that needs no type relation**: a type
//! parameter written bare as a parameter's type is whatever the argument at that
//! position turned out to be.
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
//! The 53% row is the cliff and it is not approached here. The bare-parameter
//! row is the one where the candidate *is* the argument type, which is a lookup
//! rather than an inference, and it is what this module does.
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
//! A spread argument, a rest parameter, a return type that mentions a type
//! parameter in any position other than *being* one, a type parameter with no
//! bare parameter position, two bare positions disagreeing about the same type
//! parameter, an argument whose own type is a gap, and — for a call with
//! written type arguments — the wrong count of them, a defaulted type
//! parameter, or an argument that does not resolve.
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
        let error = self.intrinsics.error;
        // Upstream checks every argument (`checkExpression` through
        // `getEffectiveCallArguments`) whatever it then does with them, and the
        // arguments are needed here anyway. A spread has no single position to
        // land on, so it is a gap — but only after the arguments are checked,
        // so the gap does not swallow their own lines.
        let mut argument_types = Vec::with_capacity(arguments.len());
        let mut spread = false;
        for &argument in arguments {
            if matches!(argument, Expression::SpreadElement(_)) {
                spread = true;
            }
            argument_types.push(self.check_expression(argument));
        }
        if spread {
            return error;
        }

        let returned = signature.r#type;
        if returned == error {
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
            if written.len() != parameters.len() || written.contains(&error) {
                return error;
            }
            if !self.mentions_type_parameter(returned, &parameters, &names) {
                return returned;
            }
            let map = parameters.iter().copied().zip(written).collect::<Vec<_>>();
            return self.instantiate_type(returned, &map, &parameters, &names);
        }

        // A return type that mentions no type parameter of this signature does
        // not depend on inference at all: `f<T>(x: T): string` is `string`
        // however `T` resolves. Upstream reaches the same answer the long way,
        // by instantiating a type that the mapper leaves alone.
        if !self.mentions_type_parameter(returned, &parameters, &names) {
            return returned;
        }

        // A rest parameter makes position-to-argument mapping a tuple problem
        // (`getSpreadArgumentType`, `checker.go`), so the whole signature is a
        // gap rather than the rest position alone.
        if signature.parameters.iter().any(|parameter| parameter.rest) {
            return error;
        }
        // One candidate per type parameter, from the positions typed by that
        // parameter *bare*. Inference from `x: T[]` against `number[]` is
        // `inferFromTypes` (`checker.go:21287`) and is not ported, so such a
        // position contributes nothing — which leaves its type parameter
        // unmapped, and an unmapped mention is what makes the answer below
        // `errorType` rather than a guess.
        let mut map = Vec::with_capacity(parameters.len());
        for &type_parameter in &parameters {
            let mut candidate = None;
            for (index, parameter) in signature.parameters.iter().enumerate() {
                if parameter.r#type != type_parameter {
                    continue;
                }
                let Some(&inferred) = argument_types.get(index) else {
                    // The position was not supplied. Upstream would fall back to
                    // the constraint or the default; neither is ported.
                    return error;
                };
                match candidate {
                    // Two bare positions for one type parameter: upstream unions
                    // the candidates (`getCovariantInference`), which needs a
                    // union of types this port would have to build without
                    // knowing whether subtype reduction applies.
                    Some(previous) if previous != inferred => return error,
                    _ => candidate = Some(inferred),
                }
            }
            match candidate {
                Some(inferred) if inferred != error => map.push((type_parameter, inferred)),
                Some(_) => return error,
                None => {}
            }
        }
        self.instantiate_type(returned, &map, &parameters, &names)
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
    pub(crate) fn instantiate_type(
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
            return self.create_type_reference(symbol, substituted);
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
        // Re-rendered exactly as the bake sites render: one signature is a
        // `FunctionTypeNode`, several are the type-literal form. An empty list
        // is unreachable (neither site records one) and refuses.
        let (text, signature_node) = match instantiated.as_slice() {
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
        let minted =
            self.store.new_anonymous(crate::flags::TypeFlags::OBJECT, text, symbol, signature_node);
        // Recorded in `signature_types` too, so an instantiated signature can
        // be instantiated again — `C<T>` inside `D<U>` reaches that.
        self.signature_types.insert(minted, instantiated);
        self.instantiated_signatures.insert(key, minted);
        self.minted_signature_types.insert(minted);
        minted
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
                parameter.constraint = Some(substitute(self, constraint)?);
            }
            if let Some(default) = parameter.default {
                parameter.default = Some(substitute(self, default)?);
            }
        }
        if let Some(this_parameter) = &mut signature.this_parameter {
            this_parameter.r#type = substitute(self, this_parameter.r#type)?;
        }
        for parameter in &mut signature.parameters {
            parameter.r#type = substitute(self, parameter.r#type)?;
        }
        signature.r#type = substitute(self, signature.r#type)?;
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
    fn type_parameter_types(&mut self, signature: &Signature) -> Option<Vec<TypeId>> {
        let declarations = match self.node_map.get(signature.declaration)? {
            Node::FunctionDeclaration(node) => node.type_parameters,
            Node::FunctionExpression(node) => node.type_parameters,
            Node::ArrowFunction(node) => node.type_parameters,
            Node::MethodDeclaration(node) => node.type_parameters,
            Node::MethodSignatureDeclaration(node) => node.type_parameters,
            Node::CallSignatureDeclaration(node) => node.type_parameters,
            Node::ConstructSignatureDeclaration(node) => node.type_parameters,
            Node::FunctionTypeNode(node) => node.type_parameters,
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
    fn mentions_type_parameter(&self, id: TypeId, parameters: &[TypeId], names: &[&str]) -> bool {
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
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(
            parsed.diagnostics.is_empty(),
            "fixture must parse: {:?}",
            parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
        );
        let bound = tsr_binder::bind(
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);

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
        // `<T>(a: T, b: T) => T` applied to `(1, "s")` is `1 | "s"` upstream —
        // `getCovariantInference` unions the candidates. This port has no
        // subtype reduction to decide that union, so it gaps. The plausible
        // wrong implementation is "first candidate wins", which prints `1`.
        //
        // A *semantic* gap, not an unported syntactic form: every part of the
        // fixture is understood, which is what stops the test from quietly
        // becoming a no-op if the forms in it get ported.
        assert_eq!(
            generic_call(
                "function both<T>(a: T, b: T): T { return a; }\nconst a = both(1, \"s\");",
                "both"
            ),
            "error"
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
        // An *unmapped* type parameter is the third way in: `U` appears in the
        // return type and in no bare parameter position, so inference leaves it
        // unmapped and the whole answer is a gap rather than a half-substituted
        // `C<string, U>`.
        assert_eq!(
            generic_call(
                "interface C<T> { }\ndeclare function m<T, U>(x: T): C<U>;\nconst a = m(\"s\");",
                "m"
            ),
            "error"
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
}
