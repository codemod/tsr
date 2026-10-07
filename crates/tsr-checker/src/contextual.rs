//! Contextual typing: what an *unannotated* parameter's type comes from when
//! the function it belongs to is written where a function type is expected.
//!
//! Ported from `Checker.getContextuallyTypedParameterType` (`checker.go:29458`),
//! reduced to the three corpus arms that need no type inference: a function
//! expression or arrow passed as a **call argument**, typed from the
//! corresponding parameter of the callee's signature; one that is the
//! initialiser of a variable carrying a **type annotation**, typed from the
//! annotation; and one that is the value of a **member of an object literal**
//! which itself has a contextual type, typed from the matching property of it.
//!
//! ```text
//! declare function each(callback: (item: string) => void): void;
//! each(item => item.length);
//! //   ^^^^ `string`, from `callback`, not the implicit `any`
//! ```
//!
//! # Why this is worth a module of its own, and what it is expected to move
//!
//! Contextual typing did not exist in this port at all, and it is the named
//! owner of the largest *known-wrong* population the checker has: the `any`
//! receiver arm in [`crate::members`] carries a measured **+555 wrong
//! property-access lines**, every one of them an unannotated parameter that
//! answered the implicit `any` here and then answered `any` again on the member.
//! Those lines cost no gradient — the parameter's own line was already wrong —
//! but they cost diagnostic separability, which is what this project's method
//! rests on.
//!
//! So the expected shape of the win is unusual and worth stating before the
//! measurement rather than after: **this turns wrong lines into right ones
//! without moving the gap column.** A flat gradient with an improved right/wrong
//! split is the success case, not the failure case.
//!
//! # Concentration: measured before writing, and it is diffuse
//!
//! Counted over `vendor/typescript-go/testdata/baselines/reference/submodule`
//! at the pinned commit, by the signature a contextually typed function leaves
//! in a `.types` baseline — an assertion whose subject is a function expression
//! with **unannotated** parameters and whose type has **typed** parameters, at
//! least one of them not `any`:
//!
//! - **925** such function expressions, in **349** files. The largest single
//!   file holds 42 (4.5%); the top 25 files hold 36%.
//! - Their assertion-line footprint — lines whose subject roots at one of those
//!   parameters — is **5,230 lines across 325 files**, 0.88% of the corpus's
//!   594,122 assertion lines, top 10 files 30%.
//!
//! That check exists because this project has been burned three times by a large
//! row that turned out to be one file. This one is not: it is genuinely spread,
//! which is what makes it worth porting and also what makes any single test
//! fixture a poor proxy for it.
//!
//! # What is reachable without inference, measured rather than assumed
//!
//! The obvious shape — `arr.map(x => x.foo)` — is **not** reachable. `map` is
//! generic and `every` is an overload set, and this port has no
//! `inferTypeArguments`. Classifying the 925 by what their callee's printed type
//! is:
//!
//! | callee | count | reachable |
//! |---|---|---|
//! | one non-generic signature | 548 (59%) | yes — this module |
//! | generic | 183 (20%) | no: needs inference |
//! | overload set | 102 (11%) | no: needs the full `resolveCall` |
//! | not classified by the counter | 92 (10%) | — |
//!
//! The reachable bucket is itself spread over 206 files, largest 37 (6.7%). It
//! is the first arm this module ports, and the generic and overloaded callees
//! are gaps that stay gaps.
//!
//! The second arm — an arrow initialising an **annotated variable**,
//! `const f: (x: T) => U = x => …` — is smaller but cheaper still: **75
//! functions in 32 files**, largest 8 (10.7%), and it needs no call machinery at
//! all, because `getContextualTypeForVariableLikeDeclaration`
//! (`checker.go:29438`) returns the annotation's type directly.
//!
//! The third arm — a **member of an object literal** that itself has a
//! contextual type — is the largest remaining one: 297 functions in 98 files, of
//! which **66 in 33 files** sit in an object literal whose own context this port
//! can already answer. The per-arm measurement that ranked it above `return`
//! (26), array element (33) and parenthesised expression (101, but ~32
//! reachable in 7 files) is in
//! [`Checker::contextual_type_for_object_literal_element`] and in
//! `docs/architecture/checker-notes-ctx.md`.
//!
//! # A gap here is the implicit `any`, and that is not a violated rule
//!
//! Everywhere else in this checker an unported form answers `errorType`, because
//! a confident wrong answer is worse than an honest "don't know". This module
//! answers `None`, and one level up in
//! `Checker::get_type_for_variable_like_declaration` that becomes `anyType`.
//!
//! That is correct rather than a lapse: for a parameter, `anyType` **is**
//! upstream's answer when no contextual type is available (`checker.go:18264`,
//! reached from a nil `getContextuallyTypedParameterType`). `function f(x) {}`
//! types `x` as `any` in TypeScript too. Returning `errorType` from an
//! unreachable arm here would convert lines that are currently *right* into
//! wrong ones. The rule this module is actually bound by is the stronger one it
//! is derived from: **never invent an answer**. `None` invents nothing — it
//! leaves behaviour exactly as it was before this module existed, so the only
//! lines this module can move are ones it types deliberately.
//!
//! # The recursion upstream guards, and how this avoids needing the guard
//!
//! `getContextualTypeForArgumentAtIndex` (`checker.go:29772`) resolves the
//! call's signature to find the argument's contextual type — while it is being
//! reached *from* checking that same call's argument. Upstream breaks the cycle
//! with signature links: it parks `anySignature`/`resolvingSignature` in
//! `signatureLinks` for the duration (`checker.go:29785`).
//!
//! This port has no such cache, so it cannot use that guard, and it must not
//! call [`crate::calls`]'s `resolve_call_signature`: that function now takes the
//! argument list and hands it to `choose_overload`, which **checks every
//! argument** — including the very arrow whose parameter this is trying to type.
//! Instead this module reads the callee's signatures directly and requires there
//! to be exactly one, non-generic. Selecting that signature needs no argument
//! checked at all, so the cycle is not broken, it is never entered. The cost is
//! the 102 overloaded callees in the table above, and paying it is what makes
//! this arm safe without a links table (`bd` follow-up: signature links).

use tsr_ast::{
    BindingName, CallExpression, Expression, Node, NodeId, ParameterDeclaration,
    PropertyAssignment, PropertyName,
};

use crate::{
    checker::Checker,
    signatures::Signature,
    types::{TypeData, TypeId},
};

/// A resolved contextual-signature lookup, distinct from an unsupported lookup.
pub(crate) enum ContextualSignature {
    Absent,
    Present(Box<Signature>),
}

impl ContextualSignature {
    fn into_signature(self) -> Option<Signature> {
        match self {
            Self::Absent => None,
            Self::Present(signature) => Some(*signature),
        }
    }
}

impl<'a> Checker<'a, '_> {
    /// The type an unannotated parameter takes from its context, if any.
    ///
    /// Ported from `Checker.getContextuallyTypedParameterType`
    /// (`checker.go:29458`), for the arms [`Checker::get_contextual_type`]
    /// supplies.
    ///
    /// `None` is upstream's nil: no contextual type is available, and the caller
    /// turns that into the implicit `any`. See the module documentation for why
    /// `None` rather than `errorType` is the honest answer *here specifically*.
    ///
    /// Not ported, each answering `None`:
    ///
    /// - **A rest parameter of an IIFE** (`((...n) => n)(1, 2)`), which needs
    ///   `getSpreadArgumentType` (`checker.go:29468`). The positional arm
    ///   below would give it the wrong answer, so it declines. The rest of the
    ///   IIFE road IS ported — see §768 and
    ///   [`Checker::immediately_invoked_call`]; this bullet used to say the
    ///   whole mechanism was unported and was corrected when it landed.
    /// - **Every context beyond the three above**: a `return`, a JSX attribute,
    ///   a binary operand, an array element, a parenthesised expression. Each
    ///   needs its own arm of `getContextualType` (`checker.go:29343`); the
    ///   populations and the concentration that rejected them are in
    ///   [`Checker::get_contextual_type`].
    /// - **A rest parameter or a `this` parameter** on the contextually typed
    ///   function, which shift what "the parameter at this index" means —
    ///   upstream calls `getRestTypeAtPosition` for the first and subtracts the
    ///   second from the index. Both are answerable later; neither is guessed.
    /// - **A generic or overloaded callee**, and a callee that is not a call at
    ///   all. See the module documentation.
    pub(crate) fn get_contextually_typed_parameter_type(
        &mut self,
        parameter: NodeId,
    ) -> Option<TypeId> {
        let function = self.nodes.parent(parameter)?;
        // §175 (`checker-notes-narrow.md`): an unannotated SETTER value
        // parameter takes the ACCESSOR's type, which upstream resolves in a
        // fixed order (`checker.go:18520-18529`): the getter's annotation,
        // the setter's, then the GETTER'S BODY return inference. So
        // `set bar(n)` beside `get bar() { return 0; }` is `number`, not
        // the implicit any (`inferSetterParamType`). This sits on the
        // contextual road because that is the road a parameter's type
        // actually travels.
        if self.nodes.kind(function) == tsr_ast::SyntaxKind::SetAccessor
            && self.paired_get_accessor(function).is_some()
            && let Some(accessor) = self.binder.symbol_of(function)
        {
            // `getTypeOfAccessors` on the SHARED symbol (`checker.go:18515`)
            // — the getter and setter bind to one symbol and its type IS the
            // accessor's type. Read through the symbol rather than the
            // getter's signature because `signature_parts_of` has no
            // GetAccessor arm at all (the audit's fifth missing arm), so the
            // signature road answers None here.
            let accessor_type = self.get_type_of_symbol(accessor);
            if accessor_type != self.intrinsics.error {
                return Some(accessor_type);
            }
        }
        // §768 (`checker.go:29463`-`:29484`): the IIFE arm, which runs BEFORE
        // the contextual-signature road below and is a different road
        // entirely — the type comes from the ARGUMENTS of the call that
        // immediately invokes this function, not from any contextual type the
        // function itself has. `(jake => { })("build")` types `jake` as
        // `string`.
        //
        // Found by §767: after `depend.rs` grew step arms, an un-annotated
        // PARAMETER became the gap board's second-largest root (1,059 lines /
        // 249 cases) with `contextuallyTypedIife` at its head, which is
        // exactly this shape.
        if let Some(iife) = self.immediately_invoked_call(function)
            && let Some(parameters) = self.contextualisable_parameters(function)
            && let Some(index) = parameters.iter().position(|p| p.node_id == Some(parameter))
        {
            let Some(Node::CallExpression(call)) = self.node_map.get(iife) else {
                return None;
            };
            // getEffectiveCallArguments expands tuple spreads. Their element
            // types, optionality and labels form the same effective positions
            // used by getSpreadArgumentType (internal/checker/checker.go).
            if call
                .arguments
                .iter()
                .any(|argument| matches!(argument, tsr_ast::Expression::SpreadElement(_)))
            {
                let mut elements = Vec::new();
                for argument in call.arguments {
                    let (argument, spread) = match argument {
                        tsr_ast::Expression::SpreadElement(spread) => (spread.expression?, true),
                        argument => (*argument, false),
                    };
                    let r#type = self.check_expression(argument);
                    if r#type == self.intrinsics.error || spread && !self.tuple_array_like(r#type) {
                        return None;
                    }
                    elements.push(crate::tuples::TupleElement {
                        r#type: if spread { r#type } else { self.get_widened_literal_type(r#type) },
                        spread,
                        optional: false,
                        label: None,
                    });
                }
                let arguments = self.normalize_variadic_tuple(elements, false);
                if arguments == self.intrinsics.error {
                    return None;
                }
                let signature = Signature {
                    declaration: function,
                    target: None,
                    union_contains_abstract: false,
                    non_inferrable: false,
                    kind: crate::signatures::SignatureKind::Call,
                    type_parameters: Vec::new(),
                    this_parameter: None,
                    parameters: vec![crate::signatures::Parameter::new(
                        "args".to_owned(),
                        false,
                        true,
                        arguments,
                        None,
                    )],
                    r#type: self.intrinsics.void,
                    written_return: None,
                    predicate: None,
                };
                let contextual = if parameters[index].dot_dot_dot_token.is_some() {
                    Some(self.signature_rest_type_at_position(&signature, index))
                } else {
                    self.signature_type_at_position(&signature, index).or_else(|| {
                        parameters[index]
                            .initializer
                            .is_none()
                            .then_some(self.intrinsics.undefined_widening)
                    })
                }?;
                return (contextual != self.intrinsics.error).then_some(contextual);
            }
            if parameters[index].dot_dot_dot_token.is_some() {
                let mut elements = Vec::new();
                for argument in call.arguments.iter().skip(index) {
                    let argument_type = self.check_expression(*argument);
                    if argument_type == self.intrinsics.error {
                        return None;
                    }
                    elements.push(self.get_widened_literal_type(argument_type));
                }
                return Some(self.create_tuple_type(elements, false));
            }
            if let Some(argument) = call.arguments.get(index) {
                let argument_type = self.check_expression(*argument);
                if argument_type == self.intrinsics.error {
                    return None;
                }
                return Some(self.get_widened_literal_type(argument_type));
            }
            // `:29477`-`:29480`: past the arguments, a parameter WITH an
            // initializer takes its initializer's type (upstream answers nil
            // and lets the ordinary road run); one without is `undefined`.
            if parameters[index].initializer.is_some() {
                return None;
            }
            return Some(self.intrinsics.undefined_widening);
        }
        let parameters = self.contextualisable_parameters(function)?;

        // `slices.Index(fn.Parameters(), parameter)` (`checker.go:29489`). A
        // `this` parameter would make this index disagree with the signature's,
        // which is exactly what upstream's `GetThisParameter` subtraction on the
        // same line repairs; here it is a gap instead, so the two indices are
        // the same index or there is no answer.
        let index = parameters.iter().position(|p| p.node_id == Some(parameter))?;
        if parameters.iter().any(|p| is_this_parameter(p)) {
            return None;
        }
        // §86.1: the function's OWN trailing rest no longer bails the whole
        // list — `(a, b, ...rest)` under `(...args: [number, boolean,
        // ...string[]]) => void` types `a: number` positionally and `rest:
        // string[]` from the tail. A rest anywhere but last still declines.
        let own_rest = parameters.iter().position(|p| p.dot_dot_dot_token.is_some());
        match own_rest {
            Some(position) if position + 1 != parameters.len() => return None,
            _ => {}
        }
        let asking_for_rest = own_rest == Some(index);

        // Contextually checked parameter types survive later inference reads
        // (assignContextualParameterTypes, checker.go). Pattern parameters have
        // no binder symbol here, so recover their stored type from the checked
        // function signature instead of recomputing a consumed context.
        if let Some(parameter) = self
            .node_types
            .get(&function)
            .and_then(|ty| self.signature_types.get(ty))
            .and_then(|signatures| {
                signatures.iter().find(|signature| signature.declaration == function)
            })
            .and_then(|signature| signature.parameters.get(index))
            .cloned()
        {
            let parameter_type = self.parameter_type(&parameter);
            if parameter_type != self.intrinsics.error {
                return Some(parameter_type);
            }
        }
        let signature = self.contextual_signature(function)?;
        // getContextuallyTypedParameterType delegates both ordinary and rest
        // positions to the effective signature (internal/checker/checker.go).
        let mut contextual = if asking_for_rest {
            Some(self.signature_rest_type_at_position(&signature, index))
        } else {
            self.signature_type_at_position(&signature, index)
        }?;
        // assignContextualParameterTypes (internal/checker/checker.go) allows
        // an initializer to widen a contextual parameter, but only when the
        // contextual type is assignable to the widened initializer type.
        if !asking_for_rest && let Some(initializer) = parameters[index].initializer {
            use crate::relater::{Relation, Ternary};
            // getTypeOfParameter includes undefined for optional/defaulted
            // positions; stored signature types omit it for printing.
            let comparison = if self.strict_null_checks
                && self.signature_parameter_includes_undefined(&signature, index)
            {
                self.get_union_type(&[contextual, self.intrinsics.undefined])
            } else {
                contextual
            };
            let initializer_type = self.check_expression(initializer);
            if initializer_type != self.intrinsics.error
                && self.relate_ternary(initializer_type, comparison, Relation::Assignable)
                    == Ternary::NotRelated
            {
                let widened =
                    self.widen_type_inferred_from_initializer(parameter, initializer_type);
                if self.relate_ternary(comparison, widened, Relation::Assignable)
                    == Ternary::Related
                {
                    contextual = widened;
                }
            }
        }
        (contextual != self.intrinsics.error).then_some(contextual)
    }

    /// The parameter list of `function`, if it is a form that can be
    /// contextually typed.
    ///
    /// Ported from `Checker.isContextSensitiveFunctionOrObjectLiteralMethod`
    /// (`checker.go:29495`), minus the object-literal method — which needs the
    /// object literal's own contextual type, an arm this module does not have.
    ///
    /// The `isContextSensitiveFunctionLikeDeclaration` half of upstream's test
    /// holds by construction rather than by a check: the only caller reaches
    /// here for a parameter that has no type annotation, which is what makes a
    /// function context-sensitive in the first place.
    /// `GetImmediatelyInvokedFunctionExpression` (`ast/utilities.go:1853`).
    /// §768.
    ///
    /// The call that immediately invokes `function`, looking through any
    /// number of parentheses — `((function (x) { }))("!")` counts, which is
    /// what `contextuallyTypedIife`'s "Lots of Irritating Superfluous
    /// Parentheses" block is there to check.
    pub(crate) fn immediately_invoked_call(&self, function: NodeId) -> Option<NodeId> {
        if !matches!(
            self.nodes.kind(function),
            tsr_ast::SyntaxKind::FunctionExpression | tsr_ast::SyntaxKind::ArrowFunction
        ) {
            return None;
        }
        let mut previous = function;
        let mut parent = self.nodes.parent(function)?;
        while self.nodes.kind(parent) == tsr_ast::SyntaxKind::ParenthesizedExpression {
            previous = parent;
            parent = self.nodes.parent(parent)?;
        }
        let Some(Node::CallExpression(call)) = self.node_map.get(parent) else {
            return None;
        };
        (call.expression.and_then(|e| e.node_id()) == Some(previous)).then_some(parent)
    }

    fn contextualisable_parameters(
        &self,
        function: NodeId,
    ) -> Option<&'a [&'a ParameterDeclaration<'a>]> {
        if !self.is_context_sensitive_function_like(function) {
            return None;
        }
        match self.node_map.get(function)? {
            Node::ArrowFunction(node) => Some(node.parameters),
            Node::FunctionExpression(node) => Some(node.parameters),
            Node::MethodDeclaration(node)
                if self.nodes.parent(function).is_some_and(|parent| {
                    self.nodes.kind(parent) == tsr_ast::SyntaxKind::ObjectLiteralExpression
                }) =>
            {
                Some(node.parameters)
            }
            _ => None,
        }
    }

    pub(crate) fn get_contextual_type_of_call(&mut self, call: NodeId) -> Option<TypeId> {
        self.get_contextual_type(call)
    }

    /// `compareSignaturesIdentical` (`relater.go:3103`) reduced to the question
    /// [`Checker::contextual_signature`]'s union branch asks: may these two
    /// contextual signatures be treated as one?
    ///
    /// Upstream's version is a full relation walk with a `Ternary` result and an
    /// `IgnoreThisTypes`/`IgnoreReturnTypes` flag set. This is the conservative
    /// core: same kind, same shape, and every corresponding type **the same
    /// interned `TypeId`**. Structural-but-not-identical types therefore answer
    /// `false` here where upstream may answer `true`, which costs a contextual
    /// type and never invents one.
    ///
    /// Generic signatures decline outright: upstream relates them under a
    /// unification of their type parameters, and comparing them by `TypeId`
    /// would compare two *different* parameter symbols and wrongly say "not
    /// identical" — or, worse, wrongly say identical if they happen to intern
    /// together. A decline is the honest answer for a test this cannot make.
    fn signatures_identical(&mut self, left: &Signature, right: &Signature) -> bool {
        if left.kind != right.kind
            || !left.type_parameters.is_empty()
            || !right.type_parameters.is_empty()
            || left.parameters.len() != right.parameters.len()
        {
            return false;
        }
        for (a, b) in left.parameters.iter().zip(&right.parameters) {
            if !(self.parameter_type(a) == self.parameter_type(b)
                && a.optional == b.optional
                && a.rest == b.rest)
            {
                return false;
            }
        }
        true
    }

    pub(crate) fn contextual_signature(&mut self, function: NodeId) -> Option<Signature> {
        self.contextual_signature_result(function)?.into_signature()
    }

    /// `getContextualSignature` (checker.go). The outer `None` is an
    /// unresolved port path; `Absent` is upstream's computed nil result.
    pub(crate) fn contextual_signature_result(
        &mut self,
        function: NodeId,
    ) -> Option<ContextualSignature> {
        let result = self.contextual_signature_result_worker(function)?;
        if self.contextual_prefers_uninstantiated {
            return Some(result);
        }
        let ContextualSignature::Present(signature) = result else { return Some(result) };
        if self.live_inference_context(function).is_some() {
            self.infer_contextual_annotations(function, &signature);
            let non_fixing_rest = signature.parameters.last().is_some_and(|parameter| {
                parameter.rest && {
                    let parameter_type = self.parameter_type(parameter);
                    self.store
                        .get(parameter_type)
                        .flags
                        .contains(crate::flags::TypeFlags::TYPE_PARAMETER)
                }
            });
            let consumed = if non_fixing_rest {
                Vec::new()
            } else {
                self.consumed_contextual_parameter_types(function, &signature)
            };
            let (map, parameters, names) = self.live_contextual_mapper(function, &consumed)?;
            let names: Vec<_> = names.iter().map(String::as_str).collect();
            let returned = signature.r#type;
            return self.instantiate_signature(*signature, &map, &parameters, &names).map(
                |mut signature| {
                    signature.r#type = returned;
                    ContextualSignature::Present(Box::new(signature))
                },
            );
        }
        let mut parent = self.nodes.parent(function);
        while let Some(node) = parent {
            if let Some((map, parameters, names)) =
                self.contextual_signature_mappers.get(&node).cloned()
            {
                let names: Vec<_> = names.iter().map(String::as_str).collect();
                return self
                    .instantiate_signature(*signature, &map, &parameters, &names)
                    .map(|signature| ContextualSignature::Present(Box::new(signature)));
            }
            parent = self.nodes.parent(node);
        }
        Some(ContextualSignature::Present(signature))
    }

    /// instantiateContextualType (checker.go) keeps object templates intact
    /// while resolving an instantiable contextual operand through the mapper.
    pub(crate) fn instantiate_contextual_inference_type(
        &mut self,
        ty: TypeId,
        node: NodeId,
    ) -> TypeId {
        if self.contextual_prefers_uninstantiated {
            return ty;
        }
        if let Some((map, parameters, names)) = self.live_contextual_mapper(node, &[]) {
            let names: Vec<_> = names.iter().map(String::as_str).collect();
            let image = self.instantiate_instantiable_types(ty, &map, &parameters, &names);
            if !self
                .store
                .get(image)
                .flags
                .intersects(crate::flags::TypeFlags::ANY | crate::flags::TypeFlags::UNKNOWN)
            {
                return image;
            }
            if let Some((map, parameters, names)) = self.live_contextual_return_mapper(node) {
                let names: Vec<_> = names.iter().map(String::as_str).collect();
                let image = self.instantiate_instantiable_types(ty, &map, &parameters, &names);
                if !self
                    .store
                    .get(image)
                    .flags
                    .intersects(crate::flags::TypeFlags::ANY | crate::flags::TypeFlags::UNKNOWN)
                {
                    return image;
                }
            }
            return ty;
        }
        let mut parent = self.nodes.parent(node);
        while let Some(node) = parent {
            if let Some((map, parameters, names)) =
                self.contextual_signature_mappers.get(&node).cloned()
            {
                let names: Vec<_> = names.iter().map(String::as_str).collect();
                let image = self.instantiate_instantiable_types(ty, &map, &parameters, &names);
                return if self
                    .store
                    .get(image)
                    .flags
                    .intersects(crate::flags::TypeFlags::ANY | crate::flags::TypeFlags::UNKNOWN)
                {
                    ty
                } else {
                    image
                };
            }
            parent = self.nodes.parent(node);
        }
        ty
    }

    /// getApparentTypeOfContextualType maps union operands while preserving
    /// mapped templates. An unconstrained instantiable type has unknown as its
    /// apparent type, hence no contextual call signature.
    pub(crate) fn apparent_contextual_type(&mut self, ty: TypeId) -> TypeId {
        if self.contextual_prefers_uninstantiated || self.mapped_types.contains_key(&ty) {
            return ty;
        }
        if let TypeData::Union { types, .. } = &self.store.get(ty).data {
            let types = types.clone();
            let types: Vec<_> =
                types.into_iter().map(|ty| self.apparent_contextual_type(ty)).collect();
            return self.get_union_type_without_reduction(&types);
        }
        let ty = if self.store.get(ty).flags.intersects(crate::flags::TypeFlags::INSTANTIABLE) {
            self.base_constraint_of_type(ty).unwrap_or(self.intrinsics.unknown)
        } else {
            ty
        };
        self.apparent_type(ty)
    }

    /// getContextualThisParameterType's object literal arm (checker.go).
    /// An explicit contextual signature has already taken precedence.
    pub(crate) fn contextual_object_this_type(&mut self, function: NodeId) -> Option<TypeId> {
        use tsr_ast::SyntaxKind;
        if !matches!(
            self.nodes.kind(function),
            SyntaxKind::FunctionExpression
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
        ) || !(self.no_implicit_this || self.in_js_file(function))
        {
            return None;
        }
        let mut parent = self.nodes.parent(function)?;
        if self.nodes.kind(parent) == SyntaxKind::PropertyAssignment {
            parent = self.nodes.parent(parent)?;
        }
        if self.nodes.kind(parent) != SyntaxKind::ObjectLiteralExpression {
            return None;
        }
        let literal = parent;
        // `checker.go:12049-12054`: with no contextual type for the containing
        // literal, `this` is `getWidenedType(checkExpressionCached(literal))` —
        // the literal's own type.
        let Some(contextual) = self.get_contextual_type(literal) else {
            if !self.node_types.contains_key(&literal)
                && self.object_literal_member_return_is_resolving(literal)
            {
                // This port's literal check prints member signatures eagerly,
                // so checking an uncached literal while one of its members'
                // return type resolves re-enters that member; upstream's
                // literal check never asks for a return type. Decline to the
                // pre-existing receiver (docs/parity/notes/contextual.md §6).
                return None;
            }
            let own = self.check_expression_at_node(literal);
            return Some(self.widen_object_literal_freshness(own));
        };
        let contextual = self.instantiate_contextual_inference_type(contextual, literal);
        let contextual = self.apparent_contextual_type(contextual);
        // getThisTypeOfObjectLiteralFromContextualType also checks directly
        // enclosing literals through property assignments.
        let mut current_literal = literal;
        let mut current_context = contextual;
        loop {
            if let Some(this_type) =
                self.this_type_from_contextual_type(current_context, &mut Vec::new())
            {
                // Native instantiates the entire marker argument, including
                // object members, with the non-fixing inference mapper.
                let mapper = self.live_contextual_mapper(literal, &[]).or_else(|| {
                    self.nodes
                        .ancestors(literal)
                        .find_map(|node| self.contextual_signature_mappers.get(&node).cloned())
                });
                let this_type = if let Some((map, parameters, names)) = mapper {
                    let names: Vec<_> = names.iter().map(String::as_str).collect();
                    self.instantiate_type(this_type, &map, &parameters, &names)
                } else {
                    this_type
                };
                // Native aliases already have their body's semantic flags.
                // Retain this port's alias display unless reduction eliminates
                // the composite entirely (for example D & X with D = any).
                let body = self.binding_type_alias_body(this_type);
                return Some(if matches!(self.store.get(body).data, TypeData::Intrinsic { .. }) {
                    body
                } else {
                    this_type
                });
            }
            let Some(assignment) = self.nodes.parent(current_literal) else { break };
            if self.nodes.kind(assignment) != SyntaxKind::PropertyAssignment {
                break;
            }
            let Some(outer) = self.nodes.parent(assignment) else { break };
            let Some(outer_context) = self.get_contextual_type(outer) else { break };
            current_literal = outer;
            current_context = self.apparent_contextual_type(outer_context);
        }
        let contextual = self.discriminate_union_root(contextual, literal);
        let contextual = self.get_non_nullable_type(contextual);
        Some(self.widen_object_literal_freshness(contextual))
    }

    /// Whether a function-valued member of `literal` (a method, an accessor or
    /// a property's function expression) has its return type on the
    /// resolution stack. No upstream counterpart: upstream resolves those
    /// return types lazily, after `checkObjectLiteral` has cached the literal.
    fn object_literal_member_return_is_resolving(&mut self, literal: NodeId) -> bool {
        use tsr_ast::{Expression, Node, ObjectLiteralElementLike};
        let Some(Node::ObjectLiteralExpression(object)) = self.node_map.get(literal) else {
            return false;
        };
        object.properties.iter().any(|property| {
            let member = match property {
                ObjectLiteralElementLike::MethodDeclaration(method) => method.node_id,
                ObjectLiteralElementLike::GetAccessorDeclaration(accessor) => accessor.node_id,
                ObjectLiteralElementLike::SetAccessorDeclaration(accessor) => accessor.node_id,
                ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    match assignment.initializer {
                        Some(Expression::FunctionExpression(function)) => function.node_id,
                        Some(Expression::ArrowFunction(function)) => function.node_id,
                        _ => None,
                    }
                }
                _ => None,
            };
            member.is_some_and(|member| {
                self.resolutions.active_signature_keys(member).next().is_some()
                    // A getter's return resolves inside its symbol's `Type`
                    // frame (`getTypeOfAccessors`), not a signature frame.
                    || (matches!(
                        property,
                        ObjectLiteralElementLike::GetAccessorDeclaration(_)
                            | ObjectLiteralElementLike::SetAccessorDeclaration(_)
                    ) && self.binder.symbol_of(member).is_some_and(|symbol| {
                        self.resolutions.on_stack(symbol, crate::resolution::PropertyName::Type)
                    }))
            })
        })
    }

    /// getContextualThisParameterType's assignment arm (checker.go).
    /// The receiver of `obj.member = function () {}` supplies the contextual
    /// this type when no signature or containing literal supplied one.
    pub(crate) fn contextual_assignment_this_type(&mut self, function: NodeId) -> Option<TypeId> {
        use tsr_ast::{Expression, Node, SyntaxKind};
        if self.nodes.kind(function) != SyntaxKind::FunctionExpression
            || !(self.no_implicit_this || self.in_js_file(function))
        {
            return None;
        }
        let mut parent = self.nodes.parent(function)?;
        while self.nodes.kind(parent) == SyntaxKind::ParenthesizedExpression {
            parent = self.nodes.parent(parent)?;
        }
        let Node::BinaryExpression(assignment) = self.node_map.get(parent)? else {
            return None;
        };
        if assignment.operator_token.is_none_or(|token| !token.kind.is_assignment_operator()) {
            return None;
        }
        let receiver = match assignment.left? {
            Expression::PropertyAccessExpression(access) => access.expression?,
            Expression::ElementAccessExpression(access) => access.expression?,
            _ => return None,
        };
        // Native ignores the CommonJS module-exports symbol here: an exported
        // constructor's instance receiver must not become the exports object.
        // The binder gives MODULE_EXPORTS only to its CommonJS pseudo-locals;
        // ordinary local variables named exports do not carry that flag.
        if self.in_js_file(function)
            && let Expression::Identifier(identifier) = receiver
            && let Some(symbol) = self.binder.resolve_name(
                self.nodes,
                self.node_map,
                identifier.node_id?,
                identifier.text,
                tsr_binder::SymbolFlags::VALUE,
            )
            && self
                .binder
                .symbols()
                .get(symbol)
                .flags
                .contains(tsr_binder::SymbolFlags::MODULE_EXPORTS)
        {
            return None;
        }
        let receiver = self.check_expression(receiver);
        Some(self.widen_object_literal_freshness(receiver))
    }

    /// getThisTypeFromContextualType (checker.go): union the marker results,
    /// taking the first direct marker in each intersection constituent.
    fn this_type_from_contextual_type(
        &mut self,
        contextual: TypeId,
        seen: &mut Vec<TypeId>,
    ) -> Option<TypeId> {
        if seen.contains(&contextual) {
            return None;
        }
        seen.push(contextual);
        let body = self.binding_type_alias_body(contextual);
        let result = if body == contextual {
            match self.store.get(contextual).data.clone() {
                TypeData::Union { types, .. } => {
                    let mut results = Vec::new();
                    for ty in types {
                        if let Some(result) = self.this_type_from_contextual_type(ty, seen) {
                            results.push(result);
                        }
                    }
                    (!results.is_empty()).then(|| self.get_union_type(&results))
                }
                TypeData::Intersection { types, .. } => {
                    types.into_iter().find_map(|ty| self.this_type_from_contextual_type(ty, seen))
                }
                _ => self.this_type_argument(contextual),
            }
        } else {
            self.this_type_from_contextual_type(body, seen)
        };
        seen.pop();
        result
    }

    /// getThisTypeArgument (checker.go) recognizes the global marker by identity.
    fn this_type_argument(&self, ty: TypeId) -> Option<TypeId> {
        let (target, arguments) = self.type_reference_targets.get(&ty)?;
        (self.global_type_symbol_with_arity("ThisType", 1) == Some(*target))
            .then(|| arguments.first().copied())
            .flatten()
    }

    fn contextual_signature_result_worker(
        &mut self,
        function: NodeId,
    ) -> Option<ContextualSignature> {
        // getApparentTypeOfContextualType routes object literal methods through
        // getContextualTypeForObjectLiteralMethod (internal/checker/checker.go).
        let contextual = match self.node_map.get(function)? {
            Node::MethodDeclaration(method)
                if self.nodes.parent(function).is_some_and(|parent| {
                    self.nodes.kind(parent) == tsr_ast::SyntaxKind::ObjectLiteralExpression
                }) =>
            {
                self.contextual_type_for_object_literal_named_element(function, method.name)?
            }
            _ => self.get_contextual_type(function)?,
        };
        let contextual = self.instantiate_contextual_inference_type(contextual, function);
        let contextual = self.apparent_contextual_type(contextual);
        if let TypeData::Union { types, .. } = &self.store.get(contextual).data {
            let constituents = types.clone();
            let mut found: Option<Signature> = None;
            for constituent in constituents {
                let ContextualSignature::Present(signature) =
                    self.contextual_call_signature(constituent, Some(function))?
                else {
                    continue;
                };
                let signature = *signature;
                if let Some(existing) = &found {
                    // getContextualSignature compares parameters while ignoring
                    // this and return types, then unions the return types.
                    if !self.signatures_identical(existing, &signature) {
                        return Some(ContextualSignature::Absent);
                    }
                    let return_type = self.get_union_type(&[existing.r#type, signature.r#type]);
                    let existing = found.as_mut().expect("a contextual signature");
                    existing.r#type = return_type;
                    existing.written_return = None;
                    existing.predicate = None;
                    continue;
                }
                found = Some(signature);
            }
            return Some(found.map_or(ContextualSignature::Absent, |signature| {
                ContextualSignature::Present(Box::new(signature))
            }));
        }
        self.contextual_call_signature(contextual, Some(function))
    }

    /// Ported from `Checker.getContextualCallSignature`, `isAritySmaller` and
    /// `getIntersectedSignatures` (checker.go). Type-owned signatures include
    /// callable interfaces; arity is measured after expanding fixed tuple rests.
    pub(crate) fn contextual_signature_of_type(&mut self, contextual: TypeId) -> Option<Signature> {
        self.contextual_call_signature(contextual, None)?.into_signature()
    }

    /// Fallback when JSX apparent-union discrimination (jsx.go:275) cannot be
    /// certified by the bounded port. Only publish a union callback context
    /// if its callable constituents have the same input signature,
    /// using getContextualSignature's existing identity check. An unresolved
    /// signature or distinct inputs decline, rather than treating a missing
    /// discrimination result as native's computed absence of a signature.
    pub(crate) fn certified_jsx_property_context(
        &mut self,
        props: TypeId,
        name: &str,
    ) -> Option<TypeId> {
        let field = self.contextual_property_type(props, name)?;
        if let TypeData::Union { types, .. } = self.store.get(field).data.clone() {
            let mut found: Option<Signature> = None;
            for part in types {
                let ContextualSignature::Present(signature) =
                    self.contextual_call_signature(part, None)?
                else {
                    continue;
                };
                if let Some(previous) = &found
                    && !self.signatures_identical(previous, &signature)
                {
                    return None;
                }
                found = Some(*signature);
            }
        }
        Some(field)
    }

    pub(crate) fn contextual_call_signature(
        &mut self,
        contextual: TypeId,
        function: Option<NodeId>,
    ) -> Option<ContextualSignature> {
        if contextual == self.intrinsics.error {
            return None;
        }
        // Native canonical empty-object identities have resolved empty call
        // sets; unlike an unsupported object getter this is a computed nil.
        if contextual == self.intrinsics.empty_object
            || contextual == self.intrinsics.unknown_empty_object
        {
            return Some(ContextualSignature::Absent);
        }
        if self.store.get(contextual).flags.intersects(
            crate::flags::TypeFlags::PRIMITIVE
                | crate::flags::TypeFlags::ANY
                | crate::flags::TypeFlags::UNKNOWN,
        ) {
            return Some(ContextualSignature::Absent);
        }
        // An unevaluated alias's symbol has no call members of its own.
        // An empty table there cannot establish that its body is non-callable.
        if !self.signature_types.contains_key(&contextual)
            && matches!(self.store.get(contextual).data,
                TypeData::Named { members: Some(owner), .. }
                if self.binder.symbols().get(owner).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS))
        {
            return None;
        }
        let parameters = match function.and_then(|function| self.node_map.get(function)) {
            Some(Node::ArrowFunction(f)) => f.parameters,
            Some(Node::FunctionExpression(f)) => f.parameters,
            Some(Node::MethodDeclaration(f)) => f.parameters,
            _ => &[][..],
        };
        let required = parameters
            .iter()
            .filter(|p| !is_this_parameter(p))
            .take_while(|p| {
                p.initializer.is_none()
                    && p.question_token.is_none()
                    && p.dot_dot_dot_token.is_none()
            })
            .count();
        let mut signatures = Vec::new();
        for signature in self.call_signatures_of_type(contextual)? {
            let signature = self.instantiate_signature_for_reference(contextual, signature)?;
            let expanded = self.expand_contextual_tuple_rest(signature.clone());
            if expanded.parameters.iter().any(|p| p.rest) || expanded.parameters.len() >= required {
                signatures.push(signature);
            }
        }
        let mut signatures = signatures.into_iter();
        let Some(mut combined) = signatures.next() else {
            return Some(ContextualSignature::Absent);
        };
        for signature in signatures {
            if !self.no_implicit_any {
                return Some(ContextualSignature::Absent);
            }
            let left = self.expand_contextual_tuple_rest(combined);
            let right = self.expand_contextual_tuple_rest(signature);
            combined = self.combine_contextual_overload_signatures(left, right)?;
        }
        Some(ContextualSignature::Present(Box::new(combined)))
    }

    /// `getExpandedParameters` (checker.go), the fixed tuple-rest case.
    fn expand_contextual_tuple_rest(&mut self, mut signature: Signature) -> Signature {
        let Some(rest) = signature.parameters.last().filter(|p| p.rest) else {
            return signature;
        };
        let rest_type = self.parameter_type(rest);
        let Some((elements, _)) = self.tuple_element_lists.get(&rest_type) else {
            return signature;
        };
        let rest_name = rest.name.clone();
        let expanded: Vec<_> = elements
            .iter()
            .enumerate()
            .map(|(i, &t)| {
                crate::signatures::Parameter::new(
                    self.tuple_labels
                        .get(&rest_type)
                        .and_then(|labels| labels.get(i))
                        .and_then(Clone::clone)
                        .unwrap_or_else(|| format!("{rest_name}_{i}")),
                    self.tuple_optional_masks
                        .get(&rest_type)
                        .and_then(|mask| mask.get(i))
                        .copied()
                        .unwrap_or(false),
                    false,
                    t,
                    None,
                )
            })
            .collect();
        signature.parameters.pop();
        signature.parameters.extend(expanded);
        signature
    }

    /// `combineUnionOrIntersectionMemberSignatures` and
    /// `combineUnionOrIntersectionParameters` (checker.go), at isUnion=false.
    /// Generic unification and effective array rests remain deferred.
    fn combine_contextual_overload_signatures(
        &mut self,
        mut left: Signature,
        right: Signature,
    ) -> Option<Signature> {
        if !left.type_parameters.is_empty()
            || !right.type_parameters.is_empty()
            || left.parameters.iter().chain(&right.parameters).any(|p| p.rest)
        {
            return None;
        }
        let minimum = |signature: &Signature| {
            signature.parameters.iter().rposition(|p| !p.optional).map_or(0, |i| i + 1)
        };
        let left_min = minimum(&left);
        let right_min = minimum(&right);
        let mut parameters = Vec::new();
        for i in 0..left.parameters.len().max(right.parameters.len()) {
            let a = left.parameters.get(i);
            let b = right.parameters.get(i);
            let name = match (a, b) {
                (Some(a), Some(b)) if a.name == b.name => a.name.clone(),
                (Some(a), None) => a.name.clone(),
                (None, Some(b)) => b.name.clone(),
                _ => format!("arg{i}"),
            };
            let a_type = a.map_or(self.intrinsics.unknown, |p| self.parameter_type(p));
            let b_type = b.map_or(self.intrinsics.unknown, |p| self.parameter_type(p));
            parameters.push(crate::signatures::Parameter::new(
                name,
                i >= left_min && i >= right_min,
                false,
                self.get_union_type(&[a_type, b_type]),
                None,
            ));
        }
        left.parameters = parameters;
        left.this_parameter = match (left.this_parameter, right.this_parameter) {
            (Some(mut a), Some(b)) => {
                let a_type = self.parameter_type(&a);
                let b_type = self.parameter_type(&b);
                a.set_type(self.get_union_type(&[a_type, b_type]));
                a.written_text = None;
                Some(a)
            }
            (a, b) => a.or(b),
        };
        left.r#type = self.get_intersection_type(&[left.r#type, right.r#type], None);
        left.written_return = None;
        left.predicate = None;
        Some(left)
    }

    /// The type `node` is expected to have, from where it is written.
    ///
    /// Ported from `Checker.getContextualType` (`checker.go:29343`) — the
    /// dispatch every context reaches, a `switch` on the **parent's** kind. Three
    /// of its twenty arms are here.
    ///
    /// # Why this is a dispatch on an expression and not a helper on a function
    ///
    /// The first version of this module had no such function: it asked "is this
    /// *function* a call argument, or the initialiser of an annotated variable?"
    /// and answered a `Signature` directly. That shape cannot express the
    /// object-literal arm, whose subject is the enclosing **object literal** —
    /// an expression that is not a function and has no signature. Upstream's own
    /// structure is the fix and it is a strict generalisation: each of the two
    /// existing arms was already `single_call_signature(<a type>)`, so factoring
    /// the type out changes no answer. The object-literal arm is then a
    /// recursive call rather than a fourth special case, which is exactly how
    /// `checker.go:29925`'s `getApparentTypeOfContextualType(objectLiteral)`
    /// reaches it.
    ///
    /// # The seventeen arms that are not here
    ///
    /// Each was measured over
    /// `vendor/typescript-go/testdata/baselines/reference/submodule` before being
    /// left out; the counts and the concentration are in
    /// `docs/architecture/checker-notes-ctx.md`. The three largest omissions:
    ///
    /// - **`ParenthesizedExpression`** (`checker.go:29392`), a one-line recursion
    ///   — 101 contextually typed functions, but only ~32 of them in a context
    ///   this port can already answer, and those sit in 7 files with
    ///   `parenthesizedContexualTyping{1,2,3}` holding most of them. Rejected on
    ///   concentration, not on cost.
    /// - **`ReturnStatement`** (`checker.go:29621`) — 26 functions in 15 files,
    ///   top 10 files 81%.
    /// - **`ArrayLiteralExpression`** (`checker.go:29380`, via
    ///   `getContextualTypeForElementExpression`, `checker.go:29972`) — 33
    ///   functions in 15 files, top 10 files 85%, and every one of them needs
    ///   tuple types this port does not have.
    ///
    /// A `NewExpression` shares upstream's `CallExpression` arm and would cost
    /// one pattern here, but it is not measured separately, so it is a gap.
    pub(crate) fn get_contextual_type(&mut self, node: NodeId) -> Option<TypeId> {
        // A JS `@satisfies` is a reparsed `SatisfiesExpression` parent
        // upstream, whose arm answers its type node (ADR-0046).
        if let Some(satisfies) = self.jsdoc_satisfies_contextual_type(node) {
            return Some(satisfies);
        }
        let parent = self.nodes.parent(node)?;
        match self.node_map.get(parent)? {
            // `getContextualTypeForInitializerExpression` (`checker.go:29423`) →
            // `getContextualTypeForVariableLikeDeclaration` (`checker.go:29438`),
            // whose first three lines are the whole of this arm: if the
            // declaration has a type node, the contextual type *is* that type.
            //
            // This arm handles variable declarations. Written parameter and
            // property annotations have their separately measured arms below;
            // other variable-like carriers remain unsupported here.
            //
            // No check that `node` is the *initialiser*. A `VariableDeclaration`
            // has three children — name, type annotation, initialiser — and only
            // the initialiser is an expression, so the parent test already
            // establishes it. An earlier draft asserted it anyway; no mutation
            // could make the assertion observable, including the one that looked
            // most dangerous — typing the annotation's own parameter from the
            // signature it belongs to — because such a parameter's parent is the
            // `FunctionType` node and [`Checker::contextualisable_parameters`]
            // turns it away one level up. Removed rather than kept as decoration,
            // which is the call `crate::members` records making for the same
            // reason.
            Node::VariableDeclaration(declaration) => {
                // In a JS file the reparsed `@type` tag is the declaration's
                // type node (`reparseHosted`'s `KindJSDocTypeTag` arm).
                let annotation =
                    declaration.r#type.or_else(|| self.jsdoc_type_annotation(parent))?;
                Some(self.get_type_from_type_node(annotation))
            }
            Node::ParameterDeclaration(declaration) => {
                if declaration.initializer.and_then(|initializer| initializer.node_id())
                    != Some(node)
                {
                    return None;
                }
                // A written annotation precedes contextual-signature/default
                // inference in getContextualTypeForVariableLikeDeclaration.
                let annotation = declaration.r#type?;
                Some(self.get_type_from_type_node(annotation))
            }
            Node::BindingElement(element) => {
                if element.initializer.and_then(|initializer| initializer.node_id()) != Some(node) {
                    return None;
                }
                self.contextual_type_for_binding_element(parent)
            }
            // §154 (`checker-notes-ctx.md`): the assertion family — `x as T`
            // and `<T>x` answer the asserted type as context, EXCEPT `as
            // const` (isConstContext's business, not a contextual type);
            // `x satisfies T` answers its type node the same way.
            Node::AsExpression(assertion) => {
                let annotation = assertion.r#type?;
                if crate::assertions::is_const_type_reference(annotation) {
                    return None;
                }
                Some(self.get_type_from_type_node(annotation))
            }
            Node::TypeAssertion(assertion) => {
                let annotation = assertion.r#type?;
                if crate::assertions::is_const_type_reference(annotation) {
                    return None;
                }
                Some(self.get_type_from_type_node(annotation))
            }
            Node::SatisfiesExpression(node) => {
                let annotation = node.r#type?;
                Some(self.get_type_from_type_node(annotation))
            }
            // §153 (`checker-notes-ctx.md`): a CLASS property's annotation is
            // the same `getContextualTypeForVariableLikeDeclaration` road —
            // the gap the arm above's comment recorded. Initializer position
            // is established the same way (name and annotation are not
            // expressions); computed names carry no annotation relevant here.
            Node::PropertyDeclaration(declaration) => {
                if let Some(annotation) = declaration.r#type {
                    return Some(self.get_type_from_type_node(annotation));
                }
                let class = self.nodes.parent(parent)?;
                if self.nodes.kind(class) != tsr_ast::SyntaxKind::ClassExpression
                    || !crate::check::has_modifier(
                        declaration.modifiers,
                        tsr_ast::SyntaxKind::StaticKeyword,
                    )
                    || declaration.initializer.and_then(|initializer| initializer.node_id())
                        != Some(node)
                {
                    return None;
                }
                // getContextualTypeForStaticPropertyDeclaration: only a
                // named property of the enclosing apparent context supplies
                // context; an index signature does not supply a fallback.
                let contextual = self.get_contextual_type(class)?;
                let contextual = self.apparent_contextual_type(contextual);
                let name = match declaration.name {
                    PropertyName::Identifier(name) => name.text.to_string(),
                    PropertyName::StringLiteral(name) => name.text.to_string(),
                    PropertyName::NumericLiteral(name) => {
                        crate::printing::normalise_number(name.text)
                    }
                    PropertyName::ComputedPropertyName(name) => {
                        let name_type = self.check_expression(name.expression?);
                        self.property_name_from_index(name_type)?
                    }
                    _ => return None,
                };
                self.get_type_of_property_of_type(contextual, &name)
            }
            // getContextualTypeForJsxExpression/Attribute/ChildJsxExpression
            // (pinned jsx.go): the wrapper is transparent, while body children
            // index the semantic child list, not trivia/empty expressions.
            // Native getContextualType also passes through a non-null
            // assertion (checker.go:29394), without stripping its context.
            Node::JsxExpression(_)
            | Node::ParenthesizedExpression(_)
            | Node::NonNullExpression(_) => self.get_contextual_type(parent),
            Node::JsxAttribute(_) | Node::JsxSpreadAttribute(_) => {
                self.jsx_attribute_context(parent)
            }
            Node::JsxElement(_) => self.jsx_child_context(parent, node),
            Node::JsxOpeningElement(opening)
                if opening.attributes.and_then(|attributes| attributes.node_id) == Some(node) =>
            {
                self.jsx_attributes_context(parent)
            }
            Node::JsxSelfClosingElement(opening)
                if opening.attributes.and_then(|attributes| attributes.node_id) == Some(node) =>
            {
                self.jsx_attributes_context(parent)
            }
            Node::CallExpression(call) => self.contextual_type_for_argument(call, node),
            Node::Decorator(_) => self.contextual_type_for_decorator(parent),
            Node::TemplateSpan(_) => {
                let template_id = self.nodes.parent(parent)?;
                let Node::TemplateExpression(template) = self.node_map.get(template_id)? else {
                    return None;
                };
                let tagged_id = self.nodes.parent(template_id)?;
                let Node::TaggedTemplateExpression(tagged) = self.node_map.get(tagged_id)? else {
                    return None;
                };
                let index = template.template_spans.iter().position(|span| {
                    span.expression.and_then(|expression| expression.node_id()) == Some(node)
                })?;
                self.contextual_type_for_template_substitution(
                    tagged,
                    index,
                    template.template_spans.len(),
                )
            }
            // §155 (`checker-notes-ctx.md`): a NEW argument's context through
            // the §90 arity road — the sole non-generic constructor's written
            // annotation at this position. The recursion the module doc
            // guards is never entered: `sole_constructor_parameters` reads
            // declarations, not signatures.
            Node::NewExpression(new_expression) => {
                let index = new_expression
                    .arguments
                    .iter()
                    .position(|argument| argument.node_id() == Some(node))?;
                if let Some(call) = new_expression.node_id {
                    if self.contextual_prefers_uninstantiated
                        && let Some(context) = self.active_inference_contexts.get(&call).cloned()
                    {
                        return self.contextual_argument_type(
                            &context.signature,
                            index,
                            new_expression.arguments.len(),
                        );
                    }
                    if let Some(context) = self
                        .active_inference_contexts
                        .get(&call)
                        .filter(|context| context.inferential)
                        .cloned()
                    {
                        return self.contextual_argument_type(
                            &context.signature,
                            index,
                            new_expression.arguments.len(),
                        );
                    }
                    if let Some(signature) = self
                        .call_inference_signatures
                        .get(&call)
                        .cloned()
                        .or_else(|| self.resolved_call_signatures.get(&call).cloned())
                    {
                        return self.contextual_argument_type(
                            &signature,
                            index,
                            new_expression.arguments.len(),
                        );
                    }
                    // inferTypeArguments checks each argument with the current
                    // candidate's parameter type (checkExpressionWithContextualType),
                    // so an overloaded construct set's candidate under inference
                    // supplies the context before any signature is resolved.
                    // The call road keeps its own resolving ladder: the same
                    // read there regressed tupleTypeInference's arity-ranked
                    // overloads (docs/architecture/checker-99-construct-argument-contexts.md).
                    if let Some(context) = self.active_inference_contexts.get(&call).cloned() {
                        return self.single_generic_candidate_argument_type(
                            &context.signature,
                            Some(call),
                            new_expression.arguments,
                            new_expression.type_arguments,
                            index,
                        );
                    }
                    // getContextualTypeForArgument resolves a NewExpression
                    // exactly as a CallExpression (checker.go); a single
                    // generic construct signature supplies its parameter type
                    // through the same fixing rules as the call road.
                    if let Some(callee) = new_expression.expression
                        && self.resolving_signature_calls.insert(call)
                    {
                        let callee_type = self.check_expression(callee);
                        let contextual = match self
                            .signatures_of_type_kind(
                                callee_type,
                                crate::signatures::SignatureKind::Construct,
                            )
                            .as_deref()
                        {
                            Some([single]) if !single.type_parameters.is_empty() => {
                                let single = single.clone();
                                Some(self.single_generic_candidate_argument_type(
                                    &single,
                                    Some(call),
                                    new_expression.arguments,
                                    new_expression.type_arguments,
                                    index,
                                ))
                            }
                            _ => None,
                        };
                        self.resolving_signature_calls.remove(&call);
                        if let Some(contextual) = contextual {
                            return contextual;
                        }
                    }
                }
                if !new_expression.type_arguments.is_empty() {
                    return None;
                }
                let callee = new_expression.expression.and_then(|e| e.node_id())?;
                let arity = self.sole_constructor_parameters(callee)?;
                if !arity.check_argument_types {
                    return None;
                }
                let (annotation, _optional) = arity.annotations.get(index).copied().flatten()?;
                self.type_from_annotation_id(annotation)
            }
            // §152 (`checker-notes-ctx.md`): `getContextualTypeForBinaryOperand`'s
            // equals arm (`checker.go:29809`) — the RIGHT operand of plain `=`
            // answers the LEFT operand's type. The §98 walk built the same
            // rule for object-literal members; this is it at the dispatch
            // site, with the same reentrancy guard and JS-file decline.
            Node::BinaryExpression(binary) => {
                self.contextual_type_for_binary_operand(parent, binary, node)
            }
            // SS115: a ternary BRANCH answers the conditional's own context;
            // the CONDITION answers nil
            // (getContextualTypeForConditionalOperand, checker.go:30022).
            Node::ConditionalExpression(conditional) => {
                let is_branch = conditional.when_true.and_then(|e| e.node_id()) == Some(node)
                    || conditional.when_false.and_then(|e| e.node_id()) == Some(node);
                if !is_branch {
                    return None;
                }
                self.get_contextual_type(parent)
            }
            // getContextualTypeForAwaitOperand (checker.go:29750) supplies
            // the awaited context and its PromiseLike form to the operand.
            Node::AwaitExpression(_) => {
                let contextual = self.get_contextual_type(parent)?;
                let awaited = self.contextual_awaited_type_no_alias(contextual)?;
                let promise = self.global_type_symbol("PromiseLike")?;
                let promise = self.create_type_reference(promise, vec![awaited]);
                Some(self.get_union_type(&[awaited, promise]))
            }
            Node::YieldExpression(yield_expression) => self.contextual_type_for_yield_operand(
                parent,
                yield_expression.asterisk_token.is_some(),
            ),
            Node::PropertyAssignment(element) => {
                self.contextual_type_for_object_literal_element(parent, element)
            }
            // §68 (`checker-notes-narrow.md`): `getContextualTypeForReturnExpression`
            // (`checker.go:29621`), the WRITTEN-annotation half — a returned
            // expression's contextual type is the enclosing function's
            // declared return type. Rejected once at 26 functions;
            // `generatedContextualTyping` alone holds 62 aligned lines now.
            // §68.2: an ARRAY LITERAL element's contextual type is the
            // array's contextual ELEMENT type
            // (`getContextualTypeForElementExpression`, `checker.go:29972`) —
            // the Array-reference and tuple halves this port can read.
            Node::ArrayLiteralExpression(literal) => {
                let contextual = self.get_contextual_type(parent)?;
                let contextual = self.instantiate_contextual_inference_type(contextual, parent);
                let contextual = self.apparent_contextual_type(contextual);
                let index = literal.elements.iter().position(|e| e.node_id() == Some(node))?;
                let first_spread =
                    literal.elements.iter().position(|e| matches!(e, Expression::SpreadElement(_)));
                let last_spread = literal
                    .elements
                    .iter()
                    .rposition(|e| matches!(e, Expression::SpreadElement(_)));
                self.contextual_type_for_element_expression(
                    contextual,
                    index,
                    Some(literal.elements.len()),
                    first_spread,
                    last_spread,
                )
            }
            // §68.3: a CONCISE arrow body's contextual type is the arrow's
            // own contextual signature's return
            // (`getContextualReturnType`, `checker.go:29648` region).
            Node::ArrowFunction(arrow)
                if arrow.body.and_then(|b| tsr_ast::Node::from(b).node_id()) == Some(node)
                    && self.nodes.kind(node) != tsr_ast::SyntaxKind::Block =>
            {
                // `getContextualReturnType` (`checker.go:29665`) reads the
                // written return annotation before any contextual signature,
                // for an arrow's concise body exactly as for a `return`
                // (`checker.go:29358` routes both to the same function).
                let contextual = self.get_contextual_return_type(parent).ok()??;
                self.contextual_return_expression_slot(parent, contextual, false)
            }
            Node::ReturnStatement(_) => {
                let mut function = self.nodes.parent(parent)?;
                loop {
                    match self.nodes.kind(function) {
                        // §171 (`checker-notes-narrow.md`): the widened
                        // census applied to this lane — upstream ascends to
                        // the containing function through `IsFunctionLike`,
                        // which includes ACCESSORS and the CONSTRUCTOR, and
                        // `expressions.rs`'s control_flow_container lists
                        // all seven kinds while this enumeration stopped at
                        // four. A `return` inside a get accessor walked PAST
                        // it to an outer function and took that function's
                        // contextual return type.
                        tsr_ast::SyntaxKind::FunctionDeclaration
                        | tsr_ast::SyntaxKind::FunctionExpression
                        | tsr_ast::SyntaxKind::ArrowFunction
                        | tsr_ast::SyntaxKind::MethodDeclaration
                        | tsr_ast::SyntaxKind::GetAccessor
                        | tsr_ast::SyntaxKind::SetAccessor
                        | tsr_ast::SyntaxKind::Constructor => break,
                        tsr_ast::SyntaxKind::SourceFile => return None,
                        _ => function = self.nodes.parent(function)?,
                    }
                }
                // getContextualReturnType: the annotation, the (filtered)
                // contextual signature's return, then an IIFE's own context.
                let contextual = self.get_contextual_return_type(function).ok()??;
                self.contextual_return_expression_slot(function, contextual, false)
            }
            _ => None,
        }
    }

    /// `getContextualReturnType` (`checker.go:29665`). `Ok(None)` is upstream's
    /// nil — the function provably has no contextual return type; `Err` is a
    /// lookup this port cannot finish, which callers keep as a gap.
    ///
    /// The annotation arm includes native getReturnTypeFromAnnotation's
    /// getter annotation and bound paired-setter annotation. Reading this raw
    /// context never resolves the accessor symbol or checks its getter body.
    /// Constructor return context remains with its class-type producer.
    pub(crate) fn get_contextual_return_type(
        &mut self,
        function: NodeId,
    ) -> Result<Option<TypeId>, crate::iteration::Unsupported> {
        use crate::flags::TypeFlags;
        let (annotation, generator) = match self.node_map.get(function) {
            Some(Node::FunctionDeclaration(f)) => (f.r#type, f.asterisk_token.is_some()),
            Some(Node::FunctionExpression(f)) => (f.r#type, f.asterisk_token.is_some()),
            Some(Node::ArrowFunction(f)) => (f.r#type, false),
            Some(Node::MethodDeclaration(f)) => (f.r#type, f.asterisk_token.is_some()),
            Some(Node::GetAccessorDeclaration(getter)) => {
                let annotation = getter.r#type.or_else(|| {
                    let symbol = self.binder.symbol_of(function)?;
                    self.binder.symbols().get(symbol).declarations.iter().find_map(|&id| {
                        let Node::SetAccessorDeclaration(setter) = self.node_map.get(id)? else {
                            return None;
                        };
                        setter.parameters.first()?.r#type
                    })
                });
                (annotation, false)
            }
            _ => (None, false),
        };
        if let Some(annotation) = annotation {
            return Ok(Some(self.get_type_from_type_node(annotation)));
        }
        // getContextualSignatureForFunctionLikeDeclaration: only function
        // expressions, arrows and object-literal methods are contextually
        // typed, so every other kind is decidably `Absent`.
        // An immediately invoked function is a callee, a position
        // `getContextualType` answers nil for (through any parentheses), so
        // its signature is decidably absent without asking.
        let contextually_typed = match self.nodes.kind(function) {
            tsr_ast::SyntaxKind::FunctionExpression | tsr_ast::SyntaxKind::ArrowFunction => true,
            tsr_ast::SyntaxKind::MethodDeclaration => {
                self.nodes.parent(function).is_some_and(|parent| {
                    self.nodes.kind(parent) == tsr_ast::SyntaxKind::ObjectLiteralExpression
                })
            }
            _ => false,
        };
        let signature = if !contextually_typed || self.immediately_invoked_call(function).is_some()
        {
            ContextualSignature::Absent
        } else {
            self.contextual_signature_result(function).ok_or(())?
        };
        match signature {
            ContextualSignature::Present(signature) => {
                let return_type = signature.r#type;
                if return_type == self.intrinsics.error {
                    return Err(());
                }
                let is_async = self.contextual_function_is_async(function);
                let keep = TypeFlags::ANY
                    | TypeFlags::UNKNOWN
                    | TypeFlags::VOID
                    | TypeFlags::TYPE_PARAMETER
                    | TypeFlags::CONDITIONAL
                    | TypeFlags::SUBSTITUTION
                    | TypeFlags::INDEXED_ACCESS;
                if generator {
                    let mut undecided = false;
                    let filtered = self.filter_type(return_type, |checker, t| {
                        checker.store.get(t).flags.intersects(keep)
                            || checker
                                .generator_instantiation_assignable_to_return_type(t, is_async)
                                .unwrap_or_else(|()| {
                                    undecided = true;
                                    false
                                })
                    });
                    // An undecidable relation on a single (non-union) type
                    // keeps that type, the answer this lookup gave before the
                    // filter was ported; on a union it is a gap. Boundary in
                    // `docs/parity/notes/destructure-iteration.md` §6.
                    if undecided {
                        let union = self.store.get(return_type).flags.contains(TypeFlags::UNION);
                        return if union { Err(()) } else { Ok(Some(return_type)) };
                    }
                    return Ok(Some(filtered));
                }
                if is_async {
                    return Ok(Some(self.async_contextual_return_type(return_type)));
                }
                Ok(Some(return_type))
            }
            ContextualSignature::Absent => {
                let Some(call) = self.immediately_invoked_call(function) else { return Ok(None) };
                // `getContextualType(iife)`. A yield operand position keeps
                // upstream's nil apart from an unfinished lookup; elsewhere a
                // `None` is nil only where `has_no_contextual_type` proves it.
                let mut position = call;
                let mut parent = self.nodes.parent(call);
                while let Some(id) = parent
                    && self.nodes.kind(id) == tsr_ast::SyntaxKind::ParenthesizedExpression
                {
                    position = id;
                    parent = self.nodes.parent(id);
                }
                if let Some(id) = parent
                    && let Some(Node::YieldExpression(yield_expression)) = self.node_map.get(id)
                    && yield_expression.expression.and_then(|e| e.node_id()) == Some(position)
                {
                    let delegates = yield_expression.asterisk_token.is_some();
                    return self.contextual_type_for_yield_operand_result(id, delegates);
                }
                match self.get_contextual_type(call) {
                    Some(contextual) => Ok(Some(contextual)),
                    None if self.has_no_contextual_type(call) => Ok(None),
                    None => Err(()),
                }
            }
        }
    }

    /// `getContextualIterationType` (`checker.go:29656`).
    pub(crate) fn get_contextual_iteration_type(
        &mut self,
        kind: crate::iteration::IterationTypeKind,
        function: NodeId,
    ) -> Result<Option<TypeId>, crate::iteration::Unsupported> {
        let is_async = self.contextual_function_is_async(function);
        let Some(contextual) = self.get_contextual_return_type(function)? else {
            return Ok(None);
        };
        self.get_iteration_type_of_generator_function_return_type(kind, contextual, is_async)
    }

    /// getContextualTypeForReturnExpression's generator return slot
    /// (checker.go:29627). Other iterable shapes need iteration protocol lookup.
    pub(crate) fn contextual_return_expression_slot(
        &mut self,
        function: NodeId,
        contextual: TypeId,
        filter_signature: bool,
    ) -> Option<TypeId> {
        if contextual == self.intrinsics.error {
            return None;
        }
        let generator = match self.node_map.get(function) {
            Some(Node::FunctionDeclaration(f)) => f.asterisk_token.is_some(),
            Some(Node::FunctionExpression(f)) => f.asterisk_token.is_some(),
            Some(Node::MethodDeclaration(f)) => f.asterisk_token.is_some(),
            _ => false,
        };
        // A union is first narrowed to the constituents that have a RETURN
        // iteration type, then that slot is read (`checker.go:29628-29637`).
        let contextual = if generator {
            let is_async = self.contextual_function_is_async(function);
            let kind = crate::iteration::IterationTypeKind::Return;
            let mut undecided = false;
            let filtered =
                if self.store.get(contextual).flags.contains(crate::flags::TypeFlags::UNION) {
                    self.filter_type(contextual, |checker, t| {
                        checker
                            .get_iteration_type_of_generator_function_return_type(kind, t, is_async)
                            .unwrap_or_else(|()| {
                                undecided = true;
                                None
                            })
                            .is_some()
                    })
                } else {
                    contextual
                };
            if undecided {
                return None;
            }
            let returned = self
                .get_iteration_type_of_generator_function_return_type(kind, filtered, is_async)
                .ok()??;
            self.unwrap_contextual_awaited_slot(returned, is_async)
        } else {
            contextual
        };
        if self.contextual_function_is_async(function) {
            let contextual = if filter_signature && !generator {
                self.async_contextual_return_type(contextual)
            } else {
                contextual
            };
            let awaited = self.contextual_awaited_type_no_alias(contextual)?;
            let promise = self.global_type_symbol("PromiseLike")?;
            let promise = self.create_type_reference(promise, vec![awaited]);
            Some(self.get_union_type(&[awaited, promise]))
        } else {
            Some(contextual)
        }
    }

    /// The async resolver's `getAwaitedType` wraps a generic slot in
    /// `Awaited<T>` (`isAwaitedTypeNeeded`, `checker.go:31392`). Upstream's
    /// `isConstTypeVariable` sees through that wrapper by its conditional
    /// constraint (`checker.go:13656`); `is_const_type_variable`
    /// (`assertions.rs`) does not yet, so a contextual slot keeps the
    /// unwrapped type the pre-port read gave. Bounded deviation, recorded in
    /// `docs/parity/notes/destructure-iteration.md` §6; delete once that
    /// helper answers `true` for `Awaited<T>` of a const `T`.
    fn unwrap_contextual_awaited_slot(&mut self, slot: TypeId, is_async: bool) -> TypeId {
        if is_async { self.unwrap_awaited_type(slot) } else { slot }
    }

    /// getContextualReturnType filters async signature results to promises or
    /// instantiable/any/unknown/void types (checker.go:29683). Written return
    /// annotations precede this filtering in the upstream function.
    fn async_contextual_return_type(&mut self, contextual: TypeId) -> TypeId {
        use crate::flags::TypeFlags;
        let types = match self.store.get(contextual).data.clone() {
            TypeData::Union { types, .. } => types,
            _ => vec![contextual],
        };
        let types: Vec<_> = types
            .into_iter()
            .filter(|&ty| {
                self.store.get(ty).flags.intersects(
                    TypeFlags::ANY
                        | TypeFlags::UNKNOWN
                        | TypeFlags::VOID
                        | TypeFlags::TYPE_PARAMETER
                        | TypeFlags::CONDITIONAL
                        | TypeFlags::SUBSTITUTION
                        | TypeFlags::INDEXED_ACCESS,
                ) || self.contextual_promised_type(ty).is_some()
            })
            .collect();
        self.get_union_type(&types)
    }

    fn contextual_promised_type(&mut self, contextual: TypeId) -> Option<TypeId> {
        let (target, arguments) = self.type_reference_targets.get(&contextual)?.clone();
        let [argument] = arguments.as_slice() else { return None };
        ["Promise", "PromiseLike"]
            .iter()
            .any(|name| {
                self.global_type_symbol(name).is_some_and(|symbol| {
                    self.binder.merged_symbol(symbol) == self.binder.merged_symbol(target)
                })
            })
            .then_some(*argument)
    }

    fn contextual_function_is_async(&self, function: NodeId) -> bool {
        let modifiers = match self.node_map.get(function) {
            Some(Node::FunctionDeclaration(f)) => f.modifiers,
            Some(Node::FunctionExpression(f)) => f.modifiers,
            Some(Node::ArrowFunction(f)) => f.modifiers,
            Some(Node::MethodDeclaration(f)) => f.modifiers,
            _ => return false,
        };
        crate::check::has_modifier(modifiers, tsr_ast::SyntaxKind::AsyncKeyword)
    }

    /// getAwaitedTypeNoAlias for contextual return slots (checker.go:31270).
    /// Generic variables retain their identity; Awaited<T> aliases belong to
    /// expression/return construction, not this contextual query.
    fn contextual_awaited_type_no_alias(&mut self, contextual: TypeId) -> Option<TypeId> {
        self.contextual_awaited_type_with_stack(contextual, &mut Vec::new())
    }

    fn contextual_awaited_type_with_stack(
        &mut self,
        contextual: TypeId,
        stack: &mut Vec<TypeId>,
    ) -> Option<TypeId> {
        if stack.contains(&contextual) {
            return None;
        }
        stack.push(contextual);
        let result =
            if self.store.get(contextual).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER) {
                Some(contextual)
            } else if let TypeData::Union { types, .. } = self.store.get(contextual).data.clone() {
                let types = types
                    .into_iter()
                    .map(|ty| self.contextual_awaited_type_with_stack(ty, stack))
                    .collect::<Option<Vec<_>>>();
                types.map(|types| self.get_union_type(&types))
            } else if let Some(promised) = self.contextual_promised_type(contextual) {
                self.contextual_awaited_type_with_stack(promised, stack)
            } else {
                self.awaited_type_no_alias(contextual)
            };
        stack.pop();
        result
    }

    /// getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded's async
    /// promised-type lookup (checker.go:20407).
    pub(crate) fn contextual_return_widening_type(
        &mut self,
        function: NodeId,
        contextual: TypeId,
    ) -> Option<TypeId> {
        if !self.contextual_function_is_async(function) {
            return Some(contextual);
        }
        self.contextual_promised_type(contextual)
    }

    pub(crate) fn contextual_generator_iteration_type(
        &mut self,
        contextual: TypeId,
        slot: usize,
    ) -> Option<TypeId> {
        let (target, arguments) = self.type_reference_targets.get(&contextual)?.clone();
        let supported = [
            "Iterator",
            "Iterable",
            "IterableIterator",
            "Generator",
            "AsyncIterator",
            "AsyncIterable",
            "AsyncIterableIterator",
            "AsyncGenerator",
        ]
        .into_iter()
        .any(|name| {
            self.global_type_symbol_with_arity(name, 3).is_some_and(|symbol| {
                self.binder.merged_symbol(symbol) == self.binder.merged_symbol(target)
            })
        });
        supported.then(|| arguments.get(slot).copied()).flatten()
    }

    /// Ported from Checker.getContextualTypeForBindingElement
    /// (`internal/checker/checker.go`), preserving annotation/parameter context
    /// precedence. Defaults do not remove undefined from projection.
    /// Initializer fallback still requires explicit-context declaration checking
    /// so implied defaults cannot recursively request their holder's initializer.
    fn contextual_type_for_binding_element(&mut self, declaration: NodeId) -> Option<TypeId> {
        let Node::BindingElement(element) = self.node_map.get(declaration)? else { return None };
        // Native rejects pattern-valued names and computed nonliteral syntax
        // before entering the holder's contextual lookup.
        let name = match element.property_name {
            Some(PropertyName::Identifier(name)) => name.text.to_string(),
            Some(PropertyName::StringLiteral(name)) => name.text.to_string(),
            Some(PropertyName::NumericLiteral(name)) => {
                crate::printing::normalise_number(name.text)
            }
            Some(PropertyName::ComputedPropertyName(name)) => {
                let expression = name.expression?;
                if !matches!(
                    expression,
                    Expression::StringLiteral(_)
                        | Expression::NumericLiteral(_)
                        | Expression::NoSubstitutionTemplateLiteral(_)
                ) {
                    return None;
                }
                let name_type = self.check_expression(expression);
                self.property_name_from_index(name_type)?
            }
            Some(_) => return None,
            None => match element.name? {
                BindingName::Identifier(name) => name.text.to_string(),
                BindingName::BindingPattern(_) => return None,
            },
        };
        let pattern = self.nodes.parent(declaration)?;
        let array = match self.nodes.kind(pattern) {
            tsr_ast::SyntaxKind::ObjectBindingPattern => false,
            tsr_ast::SyntaxKind::ArrayBindingPattern => true,
            _ => return None,
        };
        let holder = self.nodes.parent(pattern)?;
        let parent_type = if let Some(annotation) = self.type_annotation_of(holder) {
            Some(self.get_type_from_type_node(annotation))
        } else {
            match self.nodes.kind(holder) {
                tsr_ast::SyntaxKind::BindingElement => {
                    self.contextual_type_for_binding_element(holder)
                }
                tsr_ast::SyntaxKind::Parameter => {
                    self.get_contextually_typed_parameter_type(holder)
                }
                _ => None,
            }
        };
        let parent_type = parent_type?;
        if array {
            let Node::BindingPattern(pattern) = self.node_map.get(pattern)? else { return None };
            // Binding holes occupy positions, and the binding pattern's length
            // does not align a variadic source's ending fixed elements.
            let index =
                pattern.elements.iter().position(|element| element.node_id == Some(declaration))?;
            return self.contextual_type_for_element_expression(
                parent_type,
                index,
                None,
                None,
                None,
            );
        }
        // Ordinary projection intentionally refuses a nullable outer holder;
        // contextual_property_type's nullable mapping would change that contract.
        self.get_type_of_property_of_type(parent_type, &name)
    }

    /// Ported from Checker.getContextualTypeForBinaryOperand
    /// (`internal/checker/checker.go`). Assignment declarations in JavaScript
    /// and synthesized binding-pattern contexts remain unsupported.
    fn contextual_type_for_binary_operand(
        &mut self,
        binary_id: NodeId,
        binary: &'a tsr_ast::BinaryExpression<'a>,
        operand: NodeId,
    ) -> Option<TypeId> {
        use tsr_ast::SyntaxKind;
        if let Some(annotation) = binary.r#type {
            return Some(self.get_type_from_type_node(annotation));
        }
        let right = binary.right.and_then(|expression| expression.node_id()) == Some(operand);
        match binary.operator_token?.kind {
            SyntaxKind::BarBarToken | SyntaxKind::QuestionQuestionToken => {
                if let Some(contextual) = self.get_contextual_type(binary_id) {
                    return Some(contextual);
                }
                if !right {
                    return None;
                }
            }
            SyntaxKind::AmpersandAmpersandToken | SyntaxKind::CommaToken => {
                return right.then(|| self.get_contextual_type(binary_id)).flatten();
            }
            SyntaxKind::EqualsToken
            | SyntaxKind::AmpersandAmpersandEqualsToken
            | SyntaxKind::BarBarEqualsToken
            | SyntaxKind::QuestionQuestionEqualsToken => {
                if !right || self.in_js_file(operand) {
                    return None;
                }
                // getContextualTypeForAssignmentExpression avoids resolving
                // the containing callable while checking an expando's own
                // initializer. Only an annotated variable supplies context.
                if self.binder.symbol_of(binary_id).is_some() {
                    let (receiver, name) = match binary.left? {
                        tsr_ast::Expression::PropertyAccessExpression(access) => {
                            let Some(tsr_ast::MemberName::Identifier(name)) = access.name else {
                                return None;
                            };
                            (access.expression?, Some(name.text.to_owned()))
                        }
                        tsr_ast::Expression::ElementAccessExpression(access) => {
                            let name = match access.argument_expression {
                                Some(tsr_ast::Expression::StringLiteral(name)) => {
                                    Some(name.text.to_owned())
                                }
                                Some(tsr_ast::Expression::NumericLiteral(name)) => {
                                    Some(crate::printing::normalise_number(name.text))
                                }
                                _ => None,
                            };
                            (access.expression?, name)
                        }
                        _ => return None,
                    };
                    if let tsr_ast::Expression::Identifier(receiver) = receiver {
                        let symbol = self.binder.resolve_name(
                            self.nodes,
                            self.node_map,
                            receiver.node_id?,
                            receiver.text,
                            tsr_binder::SymbolFlags::VALUE,
                        )?;
                        let symbol =
                            self.binder.symbols().get(symbol).export_symbol.unwrap_or(symbol);
                        let merged = self.binder.merged_symbol(symbol);
                        // TypeScript classes are not expando initializers.
                        // Their synthetic prototype retains its ordinary
                        // contextual type, even if the binder attached an
                        // assignment to a merged function declaration.
                        if !self
                            .binder
                            .symbols()
                            .get(merged)
                            .flags
                            .contains(tsr_binder::SymbolFlags::CLASS)
                        {
                            let declaration = self.binder.symbols().get(symbol).value_declaration?;
                            if self.nodes.kind(declaration) != SyntaxKind::VariableDeclaration {
                                return None;
                            }
                            let annotation = self.type_annotation_of(declaration)?;
                            let annotated = self.get_type_from_type_node(annotation);
                            return name
                                .and_then(|name| self.contextual_property_type(annotated, &name));
                        }
                    } else {
                        return None;
                    }
                }
            }
            _ => return None,
        }
        let left = binary.left?;
        if !self.narrow_value_stack.insert(binary_id) {
            return None;
        }
        let checked = self.check_expression(left);
        self.narrow_value_stack.remove(&binary_id);
        (checked != self.intrinsics.error).then_some(checked)
    }

    /// `getContextualTypeForYieldOperand` (`checker.go:29719`), on
    /// [`Self::get_contextual_return_type`] and the iteration-types engine
    /// (`iteration.rs`). An undecidable step answers `None` (a gap).
    fn contextual_type_for_yield_operand(
        &mut self,
        yield_id: NodeId,
        delegates: bool,
    ) -> Option<TypeId> {
        self.contextual_type_for_yield_operand_result(yield_id, delegates).ok().flatten()
    }

    /// [`Self::contextual_type_for_yield_operand`] keeping upstream's nil
    /// (`Ok(None)`) apart from an unfinished lookup (`Err`), for the IIFE arm
    /// of [`Self::get_contextual_return_type`].
    fn contextual_type_for_yield_operand_result(
        &mut self,
        yield_id: NodeId,
        delegates: bool,
    ) -> Result<Option<TypeId>, crate::iteration::Unsupported> {
        use crate::iteration::IterationTypeKind;
        let Some(function) = self.containing_function(yield_id) else { return Ok(None) };
        let Some(mut contextual) = self.get_contextual_return_type(function)? else {
            return Ok(None);
        };
        let is_async = self.contextual_function_is_async(function);
        if !delegates && self.store.get(contextual).flags.contains(crate::flags::TypeFlags::UNION) {
            let mut undecided = false;
            contextual = self.filter_type(contextual, |checker, t| {
                checker
                    .get_iteration_type_of_generator_function_return_type(
                        IterationTypeKind::Return,
                        t,
                        is_async,
                    )
                    .unwrap_or_else(|()| {
                        undecided = true;
                        None
                    })
                    .is_some()
            });
            if undecided {
                return Err(());
            }
        }
        if !delegates {
            let yielded = self.get_iteration_type_of_generator_function_return_type(
                IterationTypeKind::Yield,
                contextual,
                is_async,
            )?;
            return Ok(
                yielded.map(|yielded| self.unwrap_contextual_awaited_slot(yielded, is_async))
            );
        }
        let types =
            self.get_iteration_types_of_generator_function_return_type(contextual, is_async)?;
        let yielded = types.yield_type.unwrap_or_else(|| self.get_silent_never_type());
        let returned =
            self.get_contextual_type(yield_id).unwrap_or_else(|| self.get_silent_never_type());
        let next = types.next_type.unwrap_or(self.intrinsics.unknown);
        let sync = self.create_generator_type(yielded, returned, next, false)?;
        if is_async {
            let asynchronous = self.create_generator_type(yielded, returned, next, true)?;
            Ok(Some(self.get_union_type(&[sync, asynchronous])))
        } else {
            Ok(Some(sync))
        }
    }

    /// getContextualTypeForSubstitutionExpression and getEffectiveCallArguments
    /// (internal/checker/checker.go). The synthetic strings argument occupies
    /// position zero. This port's inferred tag memo is shifted past that slot.
    fn contextual_type_for_template_substitution(
        &mut self,
        tagged: &'a tsr_ast::TaggedTemplateExpression<'a>,
        index: usize,
        substitution_count: usize,
    ) -> Option<TypeId> {
        let call_id = tagged.node_id?;
        if let Some(memo) = self.call_inference_signatures.get(&call_id).cloned() {
            return self.contextual_argument_type(&memo, index, substitution_count);
        }
        if !self.resolving_signature_calls.insert(call_id) {
            return Some(self.intrinsics.any);
        }
        let answer = self.contextual_type_for_template_substitution_resolving(
            tagged,
            index,
            substitution_count,
        );
        self.resolving_signature_calls.remove(&call_id);
        answer
    }

    fn contextual_type_for_template_substitution_resolving(
        &mut self,
        tagged: &'a tsr_ast::TaggedTemplateExpression<'a>,
        index: usize,
        substitution_count: usize,
    ) -> Option<TypeId> {
        let receiver = self.check_expression(tagged.tag?);
        let candidates = self.call_signatures_of_type(receiver)?;
        let mut signature = if let [single] = candidates.as_slice() {
            single.clone()
        } else {
            let mut applicable = candidates
                .into_iter()
                .filter(|candidate| Self::arity_accepts(candidate, substitution_count + 1));
            let chosen = applicable.next()?;
            if applicable.next().is_some() {
                return None;
            }
            chosen
        };
        signature = self.instantiate_signature_for_reference(receiver, signature)?;
        if !signature.type_parameters.is_empty() {
            // Unresolved inference has no serving memo. Written arguments can
            // still decide the context without checking a substitution again.
            if tagged.type_arguments.len() != signature.type_parameters.len() {
                return None;
            }
            let parameters = self.type_parameter_types(&signature)?;
            let written: Vec<_> = tagged
                .type_arguments
                .iter()
                .map(|&argument| self.get_type_from_type_node(argument))
                .collect();
            if written.contains(&self.intrinsics.error) {
                return None;
            }
            let names: Vec<_> =
                signature.type_parameters.iter().map(|parameter| parameter.name.clone()).collect();
            let name_refs: Vec<_> = names.iter().map(String::as_str).collect();
            let map: Vec<_> = parameters.iter().copied().zip(written).collect();
            let contextual =
                self.contextual_argument_type(&signature, index + 1, substitution_count + 1)?;
            let image = self.instantiate_type(contextual, &map, &parameters, &name_refs);
            return (image != self.intrinsics.error).then_some(image);
        }
        self.contextual_argument_type(&signature, index + 1, substitution_count + 1)
    }

    /// `getContextualTypeForElementExpression` (internal/checker/checker.go).
    /// A known suffix aligns from the end of a rest tuple; positions around
    /// spreads or an unknown length instead receive the remaining element union.
    pub(crate) fn contextual_type_for_element_expression(
        &mut self,
        contextual: TypeId,
        index: usize,
        length: Option<usize>,
        first_spread: Option<usize>,
        last_spread: Option<usize>,
    ) -> Option<TypeId> {
        if let TypeData::Union { types, .. } = &self.store.get(contextual).data {
            let types = types.clone();
            let mut mapped = Vec::new();
            for part in types {
                match self.contextual_type_for_element_expression(
                    part,
                    index,
                    length,
                    first_spread,
                    last_spread,
                ) {
                    Some(element) => mapped.push(element),
                    // A missing object lookup may be an unresolved mapped/index
                    // signature. Dropping it would fabricate a contextual
                    // signature from the other union constituents.
                    None if self.store.get(part).flags.intersects(
                        crate::flags::TypeFlags::OBJECT | crate::flags::TypeFlags::TYPE_PARAMETER,
                    ) =>
                    {
                        return None;
                    }
                    None => {}
                }
            }
            return (!mapped.is_empty()).then(|| self.get_union_type_without_reduction(&mapped));
        }
        let elements = if let Some((elements, _)) = self.variadic_tuple_elements.get(&contextual) {
            Some(elements.clone())
        } else if let Some((types, _)) = self.tuple_element_lists.get(&contextual) {
            let mask = self.tuple_optional_masks.get(&contextual);
            Some(
                types
                    .iter()
                    .enumerate()
                    .map(|(position, &r#type)| crate::tuples::TupleElement {
                        r#type,
                        spread: false,
                        optional: mask.is_some_and(|m| m.get(position) == Some(&true)),
                        label: None,
                    })
                    .collect(),
            )
        } else {
            None
        };
        if let Some(elements) = elements {
            let fixed_start =
                elements.iter().position(|element| element.spread).unwrap_or(elements.len());
            let element_type = |checker: &mut Self, element: &crate::tuples::TupleElement| {
                if element.optional {
                    checker.get_union_type_without_reduction(&[
                        element.r#type,
                        checker.intrinsics.undefined,
                    ])
                } else {
                    element.r#type
                }
            };
            if first_spread.is_none_or(|spread| index < spread) && index < fixed_start {
                let element = &elements[index];
                // Native removes implicit missing only in the optional fixed
                // prefix. Metadata stores the underlying type separately from
                // optionality, so real undefined must survive this read.
                return Some(if element.optional && self.exact_optional_property_types {
                    self.remove_missing_type(element.r#type)
                } else {
                    element_type(self, element)
                });
            }
            let offset = if last_spread.is_none_or(|spread| index > spread) {
                length.map_or(0, |length| length.saturating_sub(index))
            } else {
                0
            };
            let fixed_end = if offset > 0 && fixed_start < elements.len() {
                elements.iter().rev().take_while(|element| !element.spread).count()
            } else {
                0
            };
            if offset > 0 && offset <= fixed_end {
                return Some(element_type(self, &elements[elements.len() - offset]));
            }
            let start = first_spread.map_or(fixed_start, |spread| fixed_start.min(spread));
            let skip = match (length, last_spread) {
                (Some(length), Some(spread)) => fixed_end.min(length - spread),
                _ => fixed_end,
            };
            let end = elements.len() - skip;
            if start >= end {
                return None;
            }
            let mut types = Vec::with_capacity(end - start);
            for element in &elements[start..end] {
                let r#type = if element.spread {
                    self.resolved_indexed_access_type(
                        element.r#type,
                        self.intrinsics.number,
                        false,
                    )?
                } else {
                    element_type(self, element)
                };
                types.push(r#type);
            }
            return Some(self.get_union_type_without_reduction(&types));
        }
        if first_spread.is_none_or(|spread| index < spread)
            && let Some(property) =
                self.get_type_of_property_of_type(contextual, &index.to_string())
        {
            return Some(property);
        }
        if first_spread.is_none_or(|spread| index < spread)
            && let Some(info) = self.get_applicable_index_info(contextual, self.intrinsics.number)
        {
            return Some(info.value);
        }
        self.tuple_spread_array_element(contextual).or_else(|| self.for_of_element_type(contextual))
    }

    /// The rest-argument context used by `getSpreadArgumentType`, alongside
    /// ordinary `getTypeAtPosition` parameters (internal/checker/checker.go).
    pub(crate) fn contextual_argument_type(
        &mut self,
        signature: &Signature,
        index: usize,
        argument_count: usize,
    ) -> Option<TypeId> {
        let rest = signature.parameters.iter().position(|parameter| parameter.rest);
        if let Some(rest) = rest
            && index >= rest
        {
            let rest_type = self.parameter_type(&signature.parameters[rest]);
            if self.store.get(rest_type).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER) {
                let index_type = self.store.intern(
                    crate::flags::TypeFlags::NUMBER_LITERAL,
                    crate::types::TypeData::NumberLiteral((index - rest).to_string()),
                );
                return self.resolved_indexed_access_type(rest_type, index_type, false);
            }
            return self.contextual_type_for_element_expression(
                rest_type,
                index - rest,
                Some(argument_count - rest),
                None,
                None,
            );
        }
        signature.parameters.get(index).map(|parameter| self.parameter_type(parameter))
    }

    /// The type an object literal's property assignment is expected to have.
    ///
    /// ```text
    /// interface Handlers { onTick: (tick: string) => void }
    /// const handlers: Handlers = { onTick: value => value };
    /// //                                   ^^^^^ `string`, via `Handlers.onTick`
    /// ```
    ///
    /// Ported from `Checker.getContextualTypeForObjectLiteralElement`
    /// (`checker.go:29920`), which is two steps: the object literal's own
    /// contextual type, then the matching property of it.
    ///
    /// # Measured, and why this arm and not another
    ///
    /// 297 contextually typed function expressions in 98 files sit in an object
    /// literal member — the largest arm after the call argument, and the second
    /// largest of the twenty. But the population that matters is the one whose
    /// **prerequisite is met**, because this arm answers nothing unless the
    /// enclosing object literal already has a contextual type. Splitting the 297
    /// by what supplies the object literal's own context:
    ///
    /// | the object literal is… | count | reachable today |
    /// |---|---|---|
    /// | an argument to a generic callee | 82 | no — needs inference |
    /// | initialiser of a simply annotated declaration | 47 | **yes** |
    /// | initialiser of an unannotated declaration | 28 | no |
    /// | a member of another object literal | 23 | no — needs this arm to nest through an index signature |
    /// | an array/tuple element | 22 | no — needs tuple types |
    /// | initialiser of a **union**-annotated declaration | 20 | no — `getApparentTypeOfContextualType` |
    /// | an argument to a one-signature callee | 19 | **yes** |
    /// | parenthesised, in a block, a `return`, other | 56 | no |
    ///
    /// **66 in 33 files**, footprint 259 assertion lines, largest file 21%, top
    /// 10 files 61%. That is the same order as the annotated-variable arm's 75,
    /// and it costs less, because everything below already exists.
    ///
    /// # Not ported
    ///
    /// - **A computed property name.** Upstream checks the name expression and
    ///   uses it if it is usable as a property name (`checker.go:29934`); here a
    ///   `ComputedPropertyName` answers `None`. So do a numeric, bigint and
    ///   template-literal name: [`Checker::get_property_of_type`] is keyed by the
    ///   source text and the literal-to-name mapping is upstream's
    ///   `getLiteralTypeFromPropertyName`, which is not ported.
    /// - **The index-signature fallback** (`checker.go:29946`), which is what
    ///   upstream answers when no property matches. Without it a member whose
    ///   name is absent from the contextual type is a gap, and gapping it is why
    ///   the 23 nested-object-literal cases above are counted as unreachable
    ///   rather than as an untested claim.
    /// - **`ShorthandPropertyAssignment`**, which shares upstream's arm
    ///   (`checker.go:29376`) but cannot hold a function expression, so it could
    ///   never reach [`Checker::get_contextually_typed_parameter_type`].
    /// - **A `SpreadAssignment`**, a separate upstream branch (`checker.go:29378`).
    fn contextual_type_for_object_literal_element(
        &mut self,
        element: NodeId,
        assignment: &'a PropertyAssignment<'a>,
    ) -> Option<TypeId> {
        // `if t := element.Type(); t != nil && !ast.IsObjectLiteralMethod(element)`
        // (`checker.go:29921`). A `PropertyAssignment` cannot be an object
        // literal method, so the second half of upstream's test is the pattern
        // match above rather than a check here.
        if let Some(annotation) = assignment.r#type {
            return Some(self.get_type_from_type_node(annotation));
        }
        self.contextual_type_for_object_literal_named_element(element, assignment.name)
    }

    /// The common property lookup of getContextualTypeForObjectLiteralElement
    /// and getContextualTypeForObjectLiteralMethod. A method's return annotation
    /// is not its contextual function type.
    pub(crate) fn contextual_type_for_object_literal_named_element(
        &mut self,
        element: NodeId,
        property_name: PropertyName<'a>,
    ) -> Option<TypeId> {
        // `objectLiteral := element.Parent` (`checker.go:29924`). No check that
        // the parent *is* an `ObjectLiteralExpression`: a `PropertyAssignment`
        // has no other possible parent in this AST, so a guard here could not be
        // reddened by any mutation and would read as evidence for a decision that
        // was never made. Same call, same reason, as the initialiser guard above.
        let object_literal = self.nodes.parent(element)?;
        let contextual = self.get_contextual_type(object_literal)?;
        let contextual = self.instantiate_contextual_inference_type(contextual, object_literal);
        let contextual = self.apparent_contextual_type(contextual);
        // `c.hasBindableName(element)` (`checker.go:29927`) reduced to the names
        // `get_property_of_type` can be keyed by. See "Not ported" above.
        let name = match property_name {
            PropertyName::Identifier(name) => name.text.to_string(),
            PropertyName::StringLiteral(name) => name.text.to_string(),
            PropertyName::NumericLiteral(name) => name.text.to_string(),
            PropertyName::ComputedPropertyName(name) => {
                if let Some((name, _)) = self.late_bound_symbol_member_name(name) {
                    name
                } else {
                    // A dynamic name has no property to look up; upstream goes
                    // straight to the index fallback (`checker.go:29946-29955`)
                    // with `getLiteralTypeFromPropertyName(name)`, which for a
                    // computed name is the regular type of its expression.
                    let expression = name.expression?;
                    let name_type = self.check_expression(expression);
                    let name_type = self.get_regular_type_of_literal_type(name_type);
                    return self.contextual_index_value_for_name_type(contextual, name_type);
                }
            }
            _ => return None,
        };
        // `getTypeOfPropertyOfContextualTypeEx` (`checker.go:29932`). Upstream
        // maps over a union here.
        //
        // **§927 corrects this comment**, which used to end *"this port does not,
        // so a union-typed context finds nothing and gaps"*. The port does — as
        // [`Checker::contextual_property_type`], carrying §98's discrimination
        // guard and the intersection arm — and this caller simply did not reach
        // for it. `var x: I1<number> | I2<number> = { m: a => a }` answered
        // `error` for every member while the identical literal under a single
        // constituent typed correctly.
        //
        // Generic mapped targets substitute the property key before concrete
        // lookup; other contexts retain the concrete/union lookup order.
        let property_type = if let Some(mapped) =
            self.generic_mapped_contextual_property_type(contextual, &name)
        {
            mapped
        } else if matches!(self.store.get(contextual).data, TypeData::Intersection { .. }) {
            self.contextual_property_type(contextual, &name)?
        } else {
            match self.get_property_of_type(contextual, &name) {
                Some(_)
                    if !self.type_reference_targets.contains_key(&contextual)
                        && self
                            .anonymous_properties
                            .get(&contextual)
                            .is_some_and(|(_, instantiated)| *instantiated) =>
                {
                    // Instantiated anonymous properties retain their declaration
                    // origins, but their semantic values are already substituted.
                    self.get_type_of_property_of_type(contextual, &name)?
                }
                Some(property) => self.get_type_of_symbol(property),
                None => self.union_contextual_property_type(contextual, &name, object_literal)?,
            }
        };
        // SS141: a reference context's member instantiates through the
        // reference (Computed<T>'s read serves () => T_call, not the
        // target's own parameter).
        let property_type = {
            let image = self.instantiate_for_reference(contextual, property_type);
            if image == self.intrinsics.error { property_type } else { image }
        };
        // SS135: a member of a literal re-checking under a serve memo reads
        // its type through the pass-1 substitution - the object parameter
        // itself is symbol-backed (uninstantiable structurally), so the
        // instantiation happens here, at the property read.
        if !self.contextual_prefers_uninstantiated
            && self.live_inference_context(object_literal).is_none()
            && let Some((map, type_parameters, names)) =
                self.intra_expression_member_maps.get(&object_literal).cloned()
        {
            let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let image = self.instantiate_type(property_type, &map, &type_parameters, &name_refs);
            if image != self.intrinsics.error {
                return Some(image);
            }
        }
        Some(property_type)
    }

    /// getContextualTypeForObjectLiteralElement's index fallback
    /// (`checker.go:29946-29955`): `mapTypeEx(t, findApplicableIndexInfo(
    /// getIndexInfosOfStructuredType(t), nameType).valueType, noReductions)`.
    fn contextual_index_value_for_name_type(
        &mut self,
        contextual: TypeId,
        name_type: TypeId,
    ) -> Option<TypeId> {
        let constituents = match &self.store.get(contextual).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![contextual],
        };
        let mut values = Vec::new();
        for constituent in constituents {
            if let Some(info) = self.get_applicable_index_info(constituent, name_type) {
                values.push(info.value);
            }
        }
        match values.as_slice() {
            [] => None,
            [value] => Some(*value),
            _ => Some(self.get_union_type_without_reduction(&values)),
        }
    }

    /// [`Checker::contextual_property_type`] for an object literal's member,
    /// under the two guards the corpus measured — §927.
    ///
    /// The undiscriminated union walk answers where upstream first runs
    /// `discriminateTypeByDiscriminableItems` (`checker.go:30779`), which is not
    /// ported. Both guards exist because the walk without them cost **19
    /// `RIGHT->WRONG`**, in two families that name their own causes:
    ///
    /// 1. **A primitive constituent.** `compiler/contextualOverloadListFromUnion`
    ///    `WithPrimitiveNoImplicitAny` is a regression test for exactly this:
    ///    with `type Rule = string | FullRule`, upstream supplies **no**
    ///    contextual type for `FullRule`'s members and the parameters are
    ///    implicit `any` (the case is named for the `noImplicitAny` error). The
    ///    walk found `FullRule`'s member and typed them, 4 rows R→W.
    ///
    /// 2. **A unit answer out of a multi-constituent union.** `missingDiscriminants`
    ///    writes `const item1: Item = { subkind: 1, kind: "b" }` where `Item`'s
    ///    constituents declare `subkind: 0` and `subkind: 1`. Discrimination on
    ///    `kind: "b"` picks the constituent with **no** `subkind`, so upstream
    ///    has no contextual type and the literal widens to `number`. The walk
    ///    unions `0 | 1`, which keeps `1` fresh — 15 rows R→W across
    ///    `missingDiscriminants`, `missingDiscriminants2`,
    ///    `excessPropertyCheckWithUnions` and
    ///    `excessPropertyCheckWithMultipleDiscriminants`.
    ///
    ///    **Whether a literal member survives is precisely what discrimination
    ///    decides**, so this port declines it. That is §98's guard generalised:
    ///    `mixed_unit_and_base` caught the literal-versus-base case, and this
    ///    catches literal-versus-literal, which is the same question.
    ///
    /// Both guards would be removed by porting discrimination, and that is how
    /// you would know this entry was a stopgap rather than an answer.
    fn union_contextual_property_type(
        &mut self,
        contextual: TypeId,
        name: &str,
        literal: NodeId,
    ) -> Option<TypeId> {
        let TypeData::Union { types, .. } = &self.store.get(contextual).data else {
            return self.contextual_property_type(contextual, name);
        };
        let constituents = types.clone();
        // A value-or-promise return context is S | PromiseLike<S>. A property
        // absent from the promise branch has exactly S as its context; it does
        // not depend on discriminating arbitrary object-union alternatives.
        for &constituent in &constituents {
            if let Some(value) = self.contextual_promised_type(constituent)
                && self.contextual_property_type(constituent, name).is_none()
            {
                let remaining: Vec<_> =
                    constituents.iter().copied().filter(|&ty| ty != constituent).collect();
                let remaining = self.get_union_type(&remaining);
                if value == remaining {
                    return self.contextual_property_type(value, name);
                }
            }
        }
        // §938: **discriminate first.** `discriminateTypeByDiscriminableItems`
        // (`checker.go:30779`) selects the constituent the literal's own
        // context-free members identify, and the member lookup then happens on
        // that ONE type — which is what §927's second guard was standing in for.
        //
        // The port has had this since §750 as `discriminate_union_root`; §927
        // declined instead of calling it, and paid 11 wins for the decline.
        //
        // When discrimination narrows to a single constituent, its answer is
        // authoritative INCLUDING a miss: `missingDiscriminants` writes
        // `const item1: Item = { subkind: 1, kind: "b" }` and upstream picks the
        // `{ kind: "b" }` constituent, which has no `subkind` at all — so there
        // is no contextual type and the literal widens to `number`. Returning
        // `None` here is that answer, not a decline.
        let discriminated = self.discriminate_union_root(contextual, literal);
        if discriminated != contextual
            && !matches!(self.store.get(discriminated).data, TypeData::Union { .. })
        {
            // Read the instantiated property, not its template declaration.
            // A mapped union constituent may share a symbol whose written type
            // still mentions the mapped key even after discrimination.
            return self.get_type_of_property_of_type(discriminated, name);
        }
        // §927's FIRST guard is **removed by §938**, which is exactly the
        // prediction §927 recorded: *"removing both guards is how you would know
        // discrimination had landed"*. With the discriminating road above, the
        // primitive-constituent decline measures **+6 `WRONG->RIGHT`, zero
        // adverse** — `contextualOverloadListFromUnionWithPrimitiveNoImplicitAny`
        // no longer needs a guard, because the shape it protected is now reached
        // by a road that answers it.
        //
        // **§927's SECOND guard stays, and that half of the prediction is
        // wrong.** Removing it after §938 measures +6 `WRONG->RIGHT` against
        // **3 `RIGHT->WRONG`** (`excessPropertyCheckWithUnions`): a literal with
        // no CONTEXT-FREE discriminant leaves `discriminate_union_root`
        // answering the union unchanged, and the undiscriminated walk below is
        // still guessing there. Measured, not assumed — and the measurement is
        // what separates the two guards, which §927 had no way to tell apart.
        let member = self.contextual_property_type(contextual, name)?;
        // The unit may be a CONSTITUENT of the answer rather than the answer:
        // `subkind: 0` and `subkind: 1` union to `0 | 1`, which is not itself a
        // unit type. Testing the leaves is what `mixed_unit_and_base` does, and
        // for the same reason.
        let unit_leaf = match &self.store.get(member).data {
            TypeData::Union { types, .. } => {
                let leaves = types.clone();
                leaves
                    .iter()
                    .any(|&l| self.store.get(l).flags.intersects(crate::flags::TypeFlags::UNIT))
            }
            _ => self.store.get(member).flags.intersects(crate::flags::TypeFlags::UNIT),
        };
        if constituents.len() > 1 && unit_leaf {
            return None;
        }
        Some(member)
    }

    /// The type an expression is expected to have when it sits directly in the
    /// argument list of a call.
    ///
    /// Ported from `Checker.getContextualTypeForArgumentAtIndex`
    /// (`checker.go:29772`), collapsed to the one path that needs no argument
    /// checked — see the module documentation on the recursion this avoids.
    pub(crate) fn contextual_type_for_argument(
        &mut self,
        call: &'a CallExpression<'a>,
        argument: NodeId,
    ) -> Option<TypeId> {
        // `getEffectiveCallArguments` (`checker.go:29772`'s `argIndex` domain).
        // A spread anywhere before this argument makes the index meaningless, so
        // it is a gap rather than an off-by-one waiting to happen.
        if call.arguments.iter().any(|a| matches!(a, Expression::SpreadElement(_))) {
            return None;
        }
        // `None` when the expression is the call's *callee* rather than one of
        // its arguments, which is the immediately invoked function expression
        // upstream handles at `checker.go:29463` from the argument expressions.
        let index = call.arguments.iter().position(|a| a.node_id() == Some(argument))?;
        // getContextualTypeForArgumentAtIndex's import-call arm
        // (`checker.go:29773`): `import(specifier, options)` resolves no
        // signature — the specifier is `string`, the options
        // `getGlobalImportCallOptionsType` (the empty object type when the
        // library lacks it), any further argument `any`.
        if matches!(call.expression, Some(Expression::KeywordExpression(keyword))
            if keyword.kind == tsr_ast::SyntaxKind::ImportKeyword)
        {
            return Some(match index {
                0 => self.intrinsics.string,
                1 => self
                    .global_type_symbol("ImportCallOptions")
                    .map_or(self.intrinsics.empty_object, |options| {
                        self.get_declared_type_of_symbol(options)
                    }),
                _ => self.intrinsics.any,
            });
        }

        // The const/freshness query needs parameter identities before the
        // fixing mapper, including after a completed call memo is available.
        if self.contextual_prefers_uninstantiated {
            if let Some(call_id) = call.node_id
                && let Some(context) = self.active_inference_contexts.get(&call_id).cloned()
            {
                return self.contextual_argument_type(
                    &context.signature,
                    index,
                    call.arguments.len(),
                );
            }
            let callee = call.expression?;
            let Some(call_id) = call.node_id else {
                return self.contextual_type_for_argument_resolving(call, callee, index);
            };
            if !self.resolving_signature_calls.insert(call_id) {
                return Some(self.intrinsics.any);
            }
            let contextual = self.contextual_type_for_argument_resolving(call, callee, index);
            self.resolving_signature_calls.remove(&call_id);
            return contextual;
        }

        if let Some(call_id) = call.node_id
            && let Some(context) = self
                .active_inference_contexts
                .get(&call_id)
                .filter(|context| context.inferential)
                .cloned()
        {
            let contextual =
                self.contextual_argument_type(&context.signature, index, call.arguments.len())?;
            if self.mapped_types.get(&contextual).is_some_and(|info| info.name_type.is_some()) {
                let (map, parameters, names) = self.live_contextual_mapper(argument, &[])?;
                let names: Vec<_> = names.iter().map(String::as_str).collect();
                return Some(self.instantiate_type(contextual, &map, &parameters, &names));
            }
            return Some(contextual);
        }

        // Reunion memo consult (checker-notes-callres2.md): a pass-1
        // instantiated candidate for THIS call outranks every stateless
        // road - it is what upstream's resolved-signature threading gives
        // the second pass.
        if let Some(call_id) = call.node_id
            && let Some(memo) = self.call_inference_signatures.get(&call_id).cloned()
        {
            return self.contextual_argument_type(&memo, index, call.arguments.len());
        }
        if let Some(call_id) = call.node_id
            && let Some(resolved) = self.resolved_call_signatures.get(&call_id).cloned()
        {
            return self.contextual_argument_type(&resolved, index, call.arguments.len());
        }
        let callee = call.expression?;
        // §469 — the signature-links table's read, `checker.go:29785`: while
        // this call's signature is already resolving, don't resolve again —
        // upstream answers `resolvingSignature`, whose `getTypeAtPosition` is
        // `anyType` at every index (`relater.go:1757`, parameterless and
        // restless, so `tryGetTypeAtPosition` is nil and the wrapper fills
        // `any`). This is the cycle-breaker §445's refusal named: everything
        // below resolves the callee, and the callee's own type computation
        // can reach this very call's contextual road again. The park spans
        // exactly the resolving section — the memo consult above is upstream's
        // completed-`resolvedSignature` read and must stay reachable on
        // re-entry paths that arrive after pass-1 populated it.
        let Some(call_id) = call.node_id else {
            return self.contextual_type_for_argument_resolving(call, callee, index);
        };
        if !self.resolving_signature_calls.insert(call_id) {
            return Some(self.intrinsics.any);
        }
        let contextual = self.contextual_type_for_argument_resolving(call, callee, index);
        self.resolving_signature_calls.remove(&call_id);
        contextual
    }

    /// Resolve a call (or `new`) with a context-sensitive argument before the
    /// diagnostic walk descends into its arguments.
    ///
    /// Upstream checks the statement around a call first —
    /// `checkExpressionStatement` (`checker.go:7329`), the initializer of
    /// `checkVariableLikeDeclaration` — and that `checkExpression` resolves
    /// the call, during which `contextuallyCheckFunctionExpressionOrObjectLiteralMethod`
    /// (`checker.go:10152`) runs `assignContextualParameterTypes`
    /// (`checker.go:10349`) on each callback under the inferred signature.
    /// The callback's body is checked later, by `checkNodeDeferred`
    /// (`checker.go:2484`), so every read inside it sees parameters typed by
    /// the resolved signature. This port's walk runs its rules pre-order and
    /// reaches a callback's body (`{ ..._ }` in `map(m, (_) => ({ ..._ }))`)
    /// with the call unresolved; the parameter's type then came from
    /// [`Checker::contextual_type_for_argument`]'s stateless road, whose
    /// fixing mapper answers `unknown`, and TS2698 fired.
    ///
    /// Only calls with a context-sensitive argument are resolved here: those
    /// are the calls whose resolution publishes callback parameter types
    /// (`resolved_call_signatures`, `node_types` of the callback) that later
    /// walk reads consume. Resolving every call also exposes an unrelated
    /// argument-type defect (`intersectionSatisfiesConstraint`: a reference
    /// typed `T & {…}` where upstream's `getNarrowableTypeForReference`
    /// substitutes the constraint), so the wider form waits on that fix. The
    /// work is the call's own `check_expression`, memoized in `node_types`;
    /// no cache is added.
    pub(crate) fn resolve_call_before_callback_bodies(
        &mut self,
        node: NodeId,
        arguments: &[Expression<'_>],
    ) {
        if self.file_has_parse_errors
            || !arguments.iter().any(|argument| self.is_context_sensitive_argument(argument))
        {
            return;
        }
        self.check_expression_at_node(node);
    }

    /// The resolving section of [`Checker::contextual_type_for_argument`] —
    /// everything that computes the callee's type, split out so the §469
    /// sentinel can park around it with one insert/remove pair rather than
    /// one per early return.
    fn contextual_type_for_argument_resolving(
        &mut self,
        call: &'a CallExpression<'a>,
        callee: Expression<'a>,
        index: usize,
    ) -> Option<TypeId> {
        let callee_type = self.check_expression(callee);
        // `getContextualTypeForArgumentAtIndex` (`checker.go:29772`) reads the
        // call's resolved signature. An untyped call resolves to
        // `anySignature` (`resolveUntypedCall`, `checker.go:9902`), and an
        // error callee checks its arguments in `resolveErrorCall` while the
        // signature is still `resolvingSignature`; both have no parameters, so
        // `getTypeAtPosition` is `any` at every index (`relater.go:1757`).
        if callee_type == self.intrinsics.error
            || self.is_untyped_call_target(callee_type)
            || self.is_untyped_function_typed_callee(callee_type)
        {
            return Some(self.intrinsics.any);
        }
        if let Some(signature) = self.single_call_signature(callee_type) {
            return self.contextual_argument_type(&signature, index, call.arguments.len());
        }
        // Iteration 4 arm (a) (checker-notes-callres2.md, the priority
        // read): a SINGLE GENERIC candidate's parameter type flows AS-IS -
        // upstream checks every argument with its parameter type as
        // context, generic or not (checkExpressionWithContextualType,
        // checker.go:9485), and the arrow ADOPTS the type parameters (the
        // SS75 semantics, extended from the annotation road to here). The
        // SS70 mention guard stays on the multi-candidate agreement path
        // below, where position-stability is a real question.
        if let Some(signatures) = self.call_signatures_of_type(callee_type)
            && let [single] = signatures.as_slice()
            && !single.type_parameters.is_empty()
        {
            let single = single.clone();
            return self.single_generic_candidate_argument_type(
                &single,
                call.node_id,
                call.arguments,
                call.type_arguments,
                index,
            );
        }
        // §70 (`checker-notes-narrow.md`): OVERLOADED/GENERIC callees whose
        // every candidate AGREES on the parameter's type at this index — the
        // agreement is what upstream's per-candidate contextual pass
        // converges to when the position's type mentions no type parameter
        // (`parenthesizedContexualTyping2`'s FuncType callbacks, 73 lines).
        let candidates = self.call_signatures_of_type(callee_type)?;
        // SS114 family 1: when the candidates DISAGREE at this index (or a
        // candidate lacks the position), upstream would contextually type
        // through the RESOLVED signature - the first discriminator of which
        // is arity. The single candidate whose parameter count equals the
        // call's argument count decides; ties or no match keep the SS70
        // agreement requirement. Found by round 4's PARAM-SYM instrumentation:
        // thirteen None positions on the head case, all at mixed-arity
        // overload pairs.
        let by_arity: Vec<&Signature> =
            candidates.iter().filter(|c| c.parameters.len() == call.arguments.len()).collect();
        if let [chosen] = by_arity.as_slice() {
            let parameter_type =
                self.contextual_argument_type(chosen, index, call.arguments.len())?;
            if self.mentions_any_type_parameter(parameter_type, 2) {
                return None;
            }
            return Some(parameter_type);
        }
        let mut agreed: Option<TypeId> = None;
        for candidate in &candidates {
            let parameter_type =
                self.contextual_argument_type(candidate, index, call.arguments.len())?;
            if self.mentions_any_type_parameter(parameter_type, 2) {
                return None;
            }
            match agreed {
                None => agreed = Some(parameter_type),
                Some(t) if t == parameter_type => {}
                Some(_) => return None,
            }
        }
        agreed
    }

    /// The one call signature of `id`, or `None` if it has any other number of
    /// them or is generic.
    ///
    /// Ported from `Checker.getSignaturesOfType` (`checker.go:18959` via
    /// [`Checker::get_signatures_of_symbol`]) with the two reductions the module
    /// documentation justifies: an overload set is `None` because choosing needs
    /// arguments checked, and a generic signature is `None` because its
    /// parameter types would print as their own type parameters — `T`, not the
    /// type the caller supplied.
    /// §70's mention walk: whether `id` is, or transitively contains (to
    /// `depth` levels through signature parameters/returns and reference
    /// arguments), a type-parameter type. Conservative: ANY type-parameter
    /// mention declines, not just the candidate's own — a callback's inner
    /// generic re-binds its names and cannot leak the outer parameter.
    pub(crate) fn mentions_any_type_parameter(&mut self, id: TypeId, depth: u8) -> bool {
        if self.type_parameter_symbols.contains_key(&id) {
            return true;
        }
        if depth == 0 {
            return false;
        }
        if let Some((_, arguments)) = self.type_reference_targets.get(&id).cloned()
            && arguments.iter().any(|&a| self.mentions_any_type_parameter(a, depth - 1))
        {
            return true;
        }
        if let Some(signatures) = self.signatures_of_type(id) {
            let signatures = signatures.clone();
            for signature in &signatures {
                if signature.type_parameters.is_empty()
                    && (signature.parameters.iter().any(|p| {
                        let parameter_type = self.parameter_type(p);
                        self.mentions_any_type_parameter(parameter_type, depth - 1)
                    }) || self.mentions_any_type_parameter(signature.r#type, depth - 1))
                {
                    return true;
                }
            }
        }
        false
    }

    /// Iteration 4 arm (a) of [`Checker::contextual_type_for_argument`],
    /// shared by call and construct signatures: getContextualTypeForArgument
    /// (checker.go) reads the resolving signature for both `CallExpression` and
    /// `NewExpression`, so a single generic candidate supplies its parameter
    /// type, fixed to `unknown` where no inference can reach it.
    fn single_generic_candidate_argument_type(
        &mut self,
        single: &Signature,
        call_id: Option<NodeId>,
        arguments: &'a [Expression<'a>],
        type_arguments: &'a [tsr_ast::TypeNode<'a>],
        index: usize,
    ) -> Option<TypeId> {
        let parameter_type = self.contextual_argument_type(single, index, arguments.len())?;
        // §946: upstream's PASS ONE — the parameter type as WRITTEN, before
        // the fixing mapper below replaces this signature's type parameters
        // with `unknown`. Only the freshness query asks for it, and it asks
        // through `contextual_prefers_uninstantiated`.
        if self.contextual_prefers_uninstantiated
            || (type_arguments.is_empty()
                && self
                    .mapped_types
                    .get(&parameter_type)
                    .is_some_and(|info| info.name_type.is_none()))
            || self
                .uninstantiated_context_node
                .is_some_and(|node| arguments[index].node_id() == Some(node))
        {
            return Some(parameter_type);
        }
        // The third rung (the ladder test's final flip): upstream's
        // FIXING mapper — a context consumed with no inference
        // candidates fixes its type parameters to `unknown`
        // (`getInferredType`'s final leg, `inference.go:1317`). This
        // road fires only when NO memo exists, i.e. no pass-1
        // candidates were collected for this call, so the fill is
        // total: `someGenerics6(n => n, ...)` wants
        // `(n: unknown) => unknown`, not the adopted `(n: A) => A`.
        // §134 returnMapper guard, per-parameter: a call in CONTEXTUAL
        // position has a return-position inference source (upstream's
        // returnMapper), which sources exactly the parameters that
        // APPEAR in the return type — those stay adopted
        // (genericContextualTypes1's compose/pipe shapes); the rest
        // fix to `unknown` even there
        // (contextualTypingTwoInstancesOfSameTypeParameter).
        let contextual_call =
            call_id.is_some_and(|call_id| self.get_contextual_type_of_call(call_id).is_some());
        let Some(type_parameter_ids) = self.type_parameter_types(single) else {
            return Some(parameter_type);
        };
        let names: Vec<&str> = single.type_parameters.iter().map(|p| p.name.as_str()).collect();
        let unknown = self.intrinsics.unknown;
        let returned = single.r#type;
        // §834: a WRITTEN type argument is not a fill. The `unknown` below is
        // upstream's FIXING mapper, for a context with no inference
        // candidates — but `someGenerics6<number>(n => n)` has `A` decided by
        // the programmer, and upstream instantiates the signature from the
        // written arguments before any argument is checked. So each position
        // takes its written argument where one exists and `unknown` only
        // where none does.
        let written: Vec<TypeId> =
            type_arguments.iter().map(|argument| self.get_type_from_type_node(*argument)).collect();
        let error = self.intrinsics.error;
        let map: Vec<(TypeId, TypeId)> = type_parameter_ids
            .iter()
            .enumerate()
            .filter(|&(position, &t)| {
                // A written argument overrides the return-mapper guard: that
                // guard exists to keep an INFERRED parameter adopted, and
                // there is nothing to infer at a position the source fixed.
                written.get(position).is_some_and(|&a| a != error)
                    || !(contextual_call
                        && self.mentions_type_parameter(returned, &[t], &[names[position]]))
            })
            .map(|(position, &t)| match written.get(position) {
                Some(&argument) if argument != error => (t, argument),
                _ => (t, unknown),
            })
            .collect();
        if map.is_empty() {
            return Some(parameter_type);
        }
        let image = self.instantiate_type(parameter_type, &map, &type_parameter_ids, &names);
        if image != self.intrinsics.error {
            return Some(image);
        }
        Some(parameter_type)
    }

    pub(crate) fn single_call_signature(&mut self, id: TypeId) -> Option<Signature> {
        // Read resolved signatures before declaration symbols: a receiver's
        // type arguments have already been applied to this type. Unresolved
        // named types still require the contextual overload/intersection path.
        if !self.signature_types.contains_key(&id)
            && !matches!(
                self.store.get(id).data,
                TypeData::Anonymous { .. } | TypeData::Union { .. }
            )
        {
            return None;
        }
        match self.call_signatures_of_type(id)?.as_slice() {
            [signature] if signature.type_parameters.is_empty() => Some(signature.clone()),
            _ => None,
        }
    }
}

/// Whether `parameter` is the `this` parameter.
///
/// Ported from `ast.GetThisParameter` (`utilities.go`), which upstream reaches
/// on `checker.go:29489` to correct the parameter index. Here it only decides a
/// gap, so the identifier text is the whole test — a `this` parameter cannot be
/// a binding pattern.
fn is_this_parameter(parameter: &ParameterDeclaration<'_>) -> bool {
    matches!(parameter.name, Some(BindingName::Identifier(name)) if name.text == "this")
}

#[cfg(test)]
mod tests {
    use tsr_ast::{Node, NodeId};

    use super::{BindingName, Checker};
    use crate::types::TypeData;

    fn field_context(source: &str) -> Option<String> {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.ts", text: source },
        );
        let initializer = (0..parsed.nodes.len())
            .find_map(|index| {
                #[allow(clippy::cast_possible_truncation)]
                let id = NodeId::new(index as u32);
                match parsed.node_map.get(id) {
                    Some(Node::PropertyDeclaration(field)) => field.initializer?.node_id(),
                    _ => None,
                }
            })
            .expect("a field initializer");
        let mut checker = crate::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.get_contextual_type(initializer).map(|ty| checker.type_to_string(ty))
    }

    #[test]
    fn static_class_expression_fields_use_the_contextual_property_type() {
        for source in [
            r#"interface I { x: { a: "right" }; } const C: I = class { static x = { a: "right" }; };"#,
            r#"interface I<T> { x: { a: T }; } const C: I<"right"> = class { static x = { a: "right" }; };"#,
            r#"interface I { x: { a: "right" }; } const C: I = ((class { static x = { a: "right" }; }));"#,
            r#"interface I { x: { a: "right" }; } const key = "x"; const C: I = class { static [key] = { a: "right" }; };"#,
            r#"interface I { x: { a: "right" }; } const C = class { static x = { a: "right" }; } as I;"#,
            r#"interface I { x: { a: "right" }; } const C = class { static x = { a: "right" }; } satisfies I;"#,
            r#"const C: { "a-b": { a: "right" } } = class { static "a-b" = { a: "right" }; };"#,
            r#"const C: { 1: { a: "right" } } = class { static 1.0 = { a: "right" }; };"#,
        ] {
            assert_eq!(field_context(source).as_deref(), Some("{ a: \"right\"; }"), "{source}");
        }
        assert_eq!(
            field_context(r#"const C: { fn: (value: "right") => "right" } = class { static fn = value => value; };"#).as_deref(),
            Some("(value: \"right\") => \"right\"")
        );
    }

    #[test]
    fn static_field_context_preserves_annotations_and_absence() {
        assert_eq!(
            field_context("const C: { x: 'right' } = class { static x: 'left' = 'left'; };")
                .as_deref(),
            Some("\"left\"")
        );
        for source in [
            "const C = class { static x = 'right'; };",
            "class C { static x = 'right'; }",
            "const C: { new(): { x: 'right' } } = class { x = 'right'; };",
            "const C: { other: 'right' } = class { static x = 'right'; };",
            "declare const key: string; const C: { [key: string]: 'right' } = class { static [key] = 'right'; };",
        ] {
            assert_eq!(field_context(source), None, "{source}");
        }
    }

    #[test]
    fn written_parameter_defaults_supply_annotation_context() {
        for source in [
            r#"interface I { x: { a: "right" }; } function f(c: I = class { static x = { a: "right" }; }) {}"#,
            r#"interface I<T> { x: { a: T }; } function f(c: I<"right"> = ((class { static x = { a: "right" }; }))) {}"#,
            r#"interface I { x: { a: "right" }; } class C { constructor(c: I = class { static x = { a: "right" }; }) {} }"#,
        ] {
            assert_eq!(field_context(source).as_deref(), Some("{ a: \"right\"; }"), "{source}");
        }
        assert_eq!(
            field_context(r#"function f(c: { x: (value: "right") => "right" } = class { static x = value => value; }) {}"#).as_deref(),
            Some("(value: \"right\") => \"right\"")
        );
        assert_eq!(
            field_context(r#"function f(c = class { static x = { a: "right" }; }) {}"#),
            None
        );
        assert_eq!(
            field_context(
                r#"function f(c: { x: "right" } = class { static x: "left" = "left"; }) {}"#
            )
            .as_deref(),
            Some("\"left\"")
        );
    }

    fn binding_contexts(
        source: &str,
        strict: bool,
        exact: bool,
        warm: bool,
    ) -> Vec<Option<String>> {
        let source = format!("interface Array<T> {{ [n: number]: T }} {source}");
        let source = source.as_str();
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.ts", text: source },
        );
        let mut checker = crate::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_strict_null_checks(strict);
        checker.exact_optional_property_types = exact;
        let mut initializers = Vec::new();
        for index in 0..parsed.nodes.len() {
            #[allow(clippy::cast_possible_truncation)]
            let id = NodeId::new(index as u32);
            if warm && matches!(parsed.node_map.get(id), Some(Node::TypeAliasDeclaration(_))) {
                let symbol = bound.symbol_of(id).expect("alias symbol");
                checker.get_declared_type_of_symbol(symbol);
            }
            if let Some(Node::BindingElement(element)) = parsed.node_map.get(id)
                && let Some(initializer) = element.initializer
            {
                if let Some(name) = element.name.and_then(|name| name.node_id()) {
                    assert_eq!(checker.get_contextual_type(name), None, "initializer context only");
                }
                if warm {
                    checker.get_type_for_binding_element(id);
                }
                initializers.push(initializer.node_id().expect("registered initializer"));
            }
        }
        initializers
            .into_iter()
            .map(|id| checker.get_contextual_type(id).map(|ty| checker.type_to_string(ty)))
            .collect()
    }

    #[test]
    fn annotated_object_binding_defaults_project_names_without_default_adjustment() {
        let source = r#"
            declare let input: { right?: "right"; left: "left"; 1: "numeric"; };
            const { right: chosen = "wrong", left = "wrong", [1.0]: digit = "wrong" }:
                { right?: "right"; left: "left"; 1: "numeric"; } = input;
        "#;
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                assert_eq!(
                    binding_contexts(source, strict, exact, warm),
                    vec![
                        Some(if strict { "\"right\" | undefined" } else { "\"right\"" }.into()),
                        Some("\"left\"".into()),
                        Some("\"numeric\"".into()),
                    ],
                    "strict={strict} exact={exact} warm={warm}"
                );
            }
        }
    }

    #[test]
    fn annotated_nested_object_binding_context_does_not_strip_nullable_holders() {
        for warm in [false, true] {
            let required = r#"declare let input: { outer: { c: "right" } };
                let { outer: { c: chosen = "wrong" } }: { outer: { c: "right" } } = input;"#;
            assert_eq!(
                binding_contexts(required, true, false, warm),
                vec![Some("\"right\"".into())]
            );
            let optional = r#"declare let input: { outer?: { c: "right" } };
                let { outer: { c: chosen = "wrong" } = { c: "right" } }:
                    { outer?: { c: "right" } } = input;"#;
            for exact in [false, true] {
                let contexts = binding_contexts(optional, true, exact, warm);
                assert_eq!(
                    contexts,
                    vec![None, Some("{ c: \"right\"; } | undefined".into())],
                    "inner context must not strip outer undefined"
                );
            }
        }
    }

    #[test]
    fn object_binding_context_preserves_unannotated_array_and_dynamic_boundaries() {
        for source in [
            "const { c = 'right' } = {};",
            "let [chosen = 'right'] = [];",
            "declare const key: 'c'; let { [key]: chosen = 'right' }: { c: 'right' } = { c: 'right' };",
            "let { missing: chosen = 'right' }: { c: 'right' } = { c: 'right' };",
        ] {
            assert_eq!(binding_contexts(source, true, false, false), vec![None], "{source}");
        }
        let literal = "let { ['c']: chosen = 'wrong' }: { c: 'right' } = { c: 'right' };";
        assert_eq!(binding_contexts(literal, true, false, false), vec![Some("\"right\"".into())]);
    }

    #[test]
    fn annotated_array_binding_defaults_project_optional_prefixes_and_holes() {
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                for explicit in [false, true] {
                    let slot = if explicit { "('right' | undefined)?" } else { "'right'?" };
                    let expected = if strict && (!exact || explicit) {
                        "\"right\" | undefined"
                    } else {
                        "\"right\""
                    };
                    for default in ["'wrong'", "undefined"] {
                        let source = format!(
                            "type Target = ['skip', {slot}]; type Linked = Target; \
                             declare const input: Linked; let [, chosen = {default}]: Linked = input;"
                        );
                        assert_eq!(
                            binding_contexts(&source, strict, exact, warm),
                            vec![Some(expected.into())],
                            "strict={strict} exact={exact} warm={warm} explicit={explicit} \
                             default={default}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn annotated_array_binding_unknown_length_keeps_both_tail_candidates() {
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                for explicit in [false, true] {
                    let suffix = if explicit { "'end' | undefined" } else { "'end'" };
                    let source = format!(
                        "type Target = ['head', ...'middle'[], {suffix}]; type Linked = Target; \
                         declare const input: Linked; \
                         let [prefix = 'wrong', chosen = 'wrong', later = undefined]: Linked = input;"
                    );
                    let tail = if strict && explicit {
                        "\"end\" | \"middle\" | undefined"
                    } else {
                        "\"end\" | \"middle\""
                    };
                    assert_eq!(
                        binding_contexts(&source, strict, exact, warm),
                        vec![Some("\"head\"".into()), Some(tail.into()), Some(tail.into())],
                        "strict={strict} exact={exact} warm={warm} explicit={explicit}"
                    );
                }
            }
        }
    }

    #[test]
    fn annotated_array_binding_recursion_preserves_syntax_and_object_boundaries() {
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                let source = "type Target = { items: ['skip', 'right'?] }; type Linked = Target; \
                              declare const input: Linked; \
                              let { items: [, chosen = 'wrong'] }: Linked = input;";
                let expected = if strict && !exact { "\"right\" | undefined" } else { "\"right\"" };
                assert_eq!(
                    binding_contexts(source, strict, exact, warm),
                    vec![Some(expected.into())]
                );
                for (target, binding) in [
                    ("[{ c: 'right' }]", "[{ c: chosen = 'wrong' }]"),
                    ("[['right']]", "[[chosen = 'wrong']]"),
                    ("{ items: ['right'] }", "{ [key]: [chosen = 'wrong'] }"),
                ] {
                    let source = format!(
                        "declare const key: 'items'; type Target = {target}; type Linked = Target; \
                         declare const input: Linked; let {binding}: Linked = input;"
                    );
                    assert_eq!(binding_contexts(&source, strict, exact, warm), vec![None]);
                }
                let nullable = "type Target = { items: ['right'] } | undefined; type Linked = Target; \
                                declare const input: Linked; \
                                let { items: [chosen = 'wrong'] }: Linked = input;";
                assert_eq!(
                    binding_contexts(nullable, strict, exact, warm),
                    vec![if strict { None } else { Some("\"right\"".into()) }]
                );
                for source in [
                    "let [chosen = 'wrong'] = ['right'];",
                    "function f([chosen = 'wrong'] = ['right']) {}",
                ] {
                    // Native admits initialized rest and some implied contexts;
                    // those fallbacks are deliberately unsupported in this unit.
                    assert_eq!(binding_contexts(source, strict, exact, warm), vec![None]);
                }
            }
        }
    }

    #[test]
    fn annotated_array_binding_context_does_not_coerce_wrong_default_type() {
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            let source = "interface Array<T> { [n: number]: T } \
                          type Target = ['skip', 'right'?]; type Linked = Target; \
                          declare const input: Linked; let [, chosen = 'wrong']: Linked = input;";
            let arena = tsr_core::Arena::new();
            let parsed = tsr_parser::parse(&arena, source);
            assert!(parsed.diagnostics.is_empty());
            let bound = tsr_binder::bind(
                &arena,
                parsed.source_file,
                &parsed.nodes,
                tsr_binder::FileInfo { name: "t.ts", text: source },
            );
            let mut checker = crate::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            checker.set_strict_null_checks(strict);
            checker.exact_optional_property_types = exact;
            for index in 0..parsed.nodes.len() {
                #[allow(clippy::cast_possible_truncation)]
                let id = NodeId::new(index as u32);
                if let Some(Node::BindingElement(element)) = parsed.node_map.get(id)
                    && let Some(default) = element.initializer
                {
                    let expected =
                        if strict && !exact { "\"right\" | undefined" } else { "\"right\"" };
                    let contextual = checker
                        .get_contextual_type(default.node_id().expect("default"))
                        .expect("annotated element context");
                    assert_eq!(checker.type_to_string(contextual), expected);
                    let actual = checker.check_expression(default);
                    assert_eq!(checker.type_to_string(actual), "\"wrong\"");
                }
            }
        }
    }

    fn tuple_context(
        source: &str,
        mode: (bool, bool),
        warm: bool,
        index: usize,
        length: Option<usize>,
        spreads: (Option<usize>, Option<usize>),
    ) -> Option<String> {
        let source = format!("interface Array<T> {{ [n: number]: T }} {source}");
        let source = source.as_str();
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{source}");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.ts", text: source },
        );
        let mut checker = crate::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.apply_compiler_options(&tsr_core::CompilerOptions {
            strict: tsr_core::Tristate::from_bool(mode.0),
            exact_optional_property_types: tsr_core::Tristate::from_bool(mode.1),
            ..tsr_core::CompilerOptions::default()
        });
        let mut annotation = None;
        for position in 0..parsed.nodes.len() {
            #[allow(clippy::cast_possible_truncation)]
            let node = NodeId::new(position as u32);
            match parsed.node_map.get(node) {
                Some(Node::VariableDeclaration(variable)) => annotation = variable.r#type,
                Some(Node::TypeAliasDeclaration(_)) if warm => {
                    let symbol = bound.symbol_of(node).expect("alias symbol");
                    checker.get_declared_type_of_symbol(symbol);
                }
                _ => {}
            }
        }
        let contextual = checker.get_type_from_type_node(annotation.expect("tuple annotation"));
        checker
            .contextual_type_for_element_expression(contextual, index, length, spreads.0, spreads.1)
            .map(|ty| checker.type_to_string(ty))
    }

    #[test]
    fn contextual_tuple_prefix_removes_only_exact_optional_missing() {
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                for explicit in [false, true] {
                    let slot = if explicit { "(I | undefined)?" } else { "I?" };
                    let expected =
                        if strict && (!exact || explicit) { "I | undefined" } else { "I" };
                    for tail in ["", ", ...'tail'[]"] {
                        let source = format!(
                            "interface I {{ tag: 'prefix' }} type Target = [{slot}{tail}]; \
                             type Linked = Target; declare const v: Linked;"
                        );
                        for length in [None, Some(1)] {
                            assert_eq!(
                                tuple_context(
                                    &source,
                                    (strict, exact),
                                    warm,
                                    0,
                                    length,
                                    (None, None)
                                )
                                .as_deref(),
                                Some(expected),
                                "strict={strict} exact={exact} warm={warm} explicit={explicit} \
                                 tail={tail} length={length:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn contextual_tuple_unknown_tail_does_not_align_a_known_suffix() {
        for mode in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                for explicit in [false, true] {
                    let suffix = if explicit { "'end' | undefined" } else { "'end'" };
                    let source = format!(
                        "type Target = ['head', ...'middle'[], {suffix}]; \
                         type Linked = Target; declare const v: Linked;"
                    );
                    let unknown = if mode.0 && explicit {
                        "\"end\" | \"middle\" | undefined"
                    } else {
                        "\"end\" | \"middle\""
                    };
                    let ending = if mode.0 && explicit { "\"end\" | undefined" } else { "\"end\"" };
                    for (index, length, spreads, expected) in [
                        (0, None, (None, None), "\"head\""),
                        (1, None, (None, None), unknown),
                        (2, None, (None, None), unknown),
                        (1, Some(2), (None, None), ending),
                        (1, Some(3), (None, None), "\"middle\""),
                        (2, Some(3), (None, None), ending),
                        (1, Some(3), (Some(1), Some(1)), unknown),
                        (2, Some(3), (Some(1), Some(1)), ending),
                        (1, None, (Some(1), Some(1)), unknown),
                    ] {
                        assert_eq!(
                            tuple_context(&source, mode, warm, index, length, spreads).as_deref(),
                            Some(expected),
                            "mode={mode:?} warm={warm} explicit={explicit} index={index} \
                             length={length:?} spreads={spreads:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn contextual_tuple_missing_removal_is_prefix_only_and_keeps_real_undefined() {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, "");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.ts", text: "" },
        );
        let mut checker = crate::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.apply_compiler_options(&tsr_core::CompilerOptions {
            strict: tsr_core::Tristate::True,
            exact_optional_property_types: tsr_core::Tristate::True,
            ..tsr_core::CompilerOptions::default()
        });
        let missing = checker.intrinsics.missing;
        let undefined = checker.intrinsics.undefined;
        let slot = checker.store.intern(
            crate::flags::TypeFlags::STRING_LITERAL,
            TypeData::StringLiteral("slot".into()),
        );
        let optional = checker.get_union_type_without_reduction(&[slot, missing, undefined]);
        let tuple = checker.create_tuple_type(vec![optional], false);
        checker.tuple_optional_masks.insert(tuple, vec![true]);
        for length in [None, Some(1)] {
            let prefix = checker
                .contextual_type_for_element_expression(tuple, 0, length, None, None)
                .expect("fixed prefix");
            let TypeData::Union { types, .. } = &checker.store.get(prefix).data else {
                panic!("explicit undefined must survive");
            };
            assert!(types.contains(&slot));
            assert!(types.contains(&undefined));
            assert!(!types.contains(&missing));
            // At a spread position this same element is read by the slice
            // branch, not the fixed-prefix branch. Do not strip its missing.
            let slice = checker
                .contextual_type_for_element_expression(tuple, 0, length, Some(0), Some(0))
                .expect("slice");
            let TypeData::Union { types, .. } = &checker.store.get(slice).data else {
                panic!("slice union");
            };
            assert!(types.contains(&missing));
            assert!(types.contains(&undefined));
        }
        let unreduced =
            checker.get_union_type_without_reduction(&[checker.intrinsics.string, slot]);
        let tuple = checker.create_tuple_type(vec![unreduced], false);
        assert_eq!(
            checker.contextual_type_for_element_expression(tuple, 0, None, None, None),
            Some(unreduced)
        );
        let TypeData::Union { types, .. } = &checker.store.get(unreduced).data else {
            panic!("no subtype reduction");
        };
        assert!(types.contains(&slot));
    }

    #[test]
    fn contextual_tuple_reader_preserves_incomplete_union_refusal() {
        let source = "type Target = ['prefix'] | { named: 'other' }; \
                      type Linked = Target; declare const v: Linked;";
        for mode in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                assert_eq!(tuple_context(source, mode, warm, 0, None, (None, None)), None);
            }
        }
    }

    #[test]
    fn omitted_iife_raw_contexts_distinguish_active_ordinary_and_absent_values() {
        let mut mismatches = Vec::new();
        for strict in [false, true] {
            for (source, name, kind) in [
                ("((observed) => 42)();", "observed", "active"),
                ("((observed?) => 42)();", "observed", "active"),
                ("((observed) => 42)(...([] as []));", "observed", "active"),
                ("((observed?) => 42)(...([] as []));", "observed", "active"),
                ("((first, observed?) => 42)(73);", "observed", "active"),
                ("((first, observed?) => 42)(...([73] as [number]));", "observed", "active"),
                ("(function(undefined) { return 42; })();", "undefined", "active"),
                ("(function(undefined?) { return 42; })(...([] as []));", "undefined", "active"),
                ("((observed) => observed)(undefined as undefined);", "observed", "ordinary"),
                ("((observed?) => observed)(undefined as undefined);", "observed", "ordinary"),
                (
                    "((observed?) => observed)(...([undefined as undefined] as [undefined]));",
                    "observed",
                    "ordinary",
                ),
                ("((observed = 37) => observed)(undefined as undefined);", "observed", "ordinary"),
                // The loose global is the widening twin (checker.go:1345),
                // so the supplied argument is too.
                ("((observed?) => observed)(undefined);", "observed", "active"),
                ("((observed = 37) => observed)();", "observed", "absent"),
                ("((observed = 37) => observed)(...([] as []));", "observed", "absent"),
                ("(function(undefined = 37) { return undefined; })();", "undefined", "absent"),
                ("(function(undefined) { return undefined; })(101);", "undefined", "number"),
                (
                    "(function(undefined?) { return undefined; })(...([101] as [number]));",
                    "undefined",
                    "number",
                ),
            ] {
                let arena = tsr_core::Arena::new();
                let parsed = tsr_parser::parse(&arena, source);
                assert!(parsed.diagnostics.is_empty());
                let bound = tsr_binder::bind(
                    &arena,
                    parsed.source_file,
                    &parsed.nodes,
                    tsr_binder::FileInfo { name: "test.ts", text: source },
                );
                let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
                checker.apply_compiler_options(&tsr_core::CompilerOptions {
                    strict: tsr_core::Tristate::False,
                    strict_null_checks: if strict {
                        tsr_core::Tristate::True
                    } else {
                        tsr_core::Tristate::False
                    },
                    ..Default::default()
                });
                let i = *checker.intrinsics();
                let expected = match kind {
                    "active" => Some(i.undefined_widening),
                    "ordinary" => Some(i.undefined),
                    "number" => Some(i.number),
                    "absent" => None,
                    _ => unreachable!(),
                };
                let id = (0..parsed.nodes.len())
                    .map(|index| tsr_ast::NodeId::new(u32::try_from(index).unwrap()))
                    .find(|&id| matches!(parsed.node_map.get(id),
                        Some(Node::ParameterDeclaration(parameter))
                            if matches!(parameter.name, Some(BindingName::Identifier(n)) if n.text == name)))
                    .unwrap();
                let actual = checker.get_contextually_typed_parameter_type(id);
                println!(
                    "RAW_CONTEXT\tstrict={strict}\tkind={kind}\tname={name}\t{source:?}\tactual={actual:?}\texpected={expected:?}"
                );
                if actual != expected {
                    mismatches.push((strict, source, actual, expected));
                }
            }
        }
        assert!(mismatches.is_empty(), "raw identity mismatches: {mismatches:?}");
    }

    #[test]
    fn instantiated_anonymous_context_uses_semantic_values_and_preserves_error() {
        let source = r#"interface I<T> { x: { a: T }; }
function f(c: I<"right"> = class { static x = { a: "right" }; }) {}"#;
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.ts", text: source },
        );
        let mut field = None;
        let mut literal = None;
        for index in 0..parsed.nodes.len() {
            let id = NodeId::new(u32::try_from(index).expect("fixture node"));
            match parsed.node_map.get(id) {
                Some(Node::PropertyDeclaration(node)) => {
                    field = node.initializer.and_then(|initializer| initializer.node_id());
                }
                Some(Node::PropertyAssignment(node)) => {
                    literal = node.initializer.and_then(|initializer| initializer.node_id());
                }
                _ => {}
            }
        }
        let mut checker = crate::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let context = checker.get_contextual_type(field.expect("field")).expect("context");
        assert!(!checker.type_reference_targets.contains_key(&context));
        assert!(checker.anonymous_properties[&context].1);
        let semantic = checker.get_type_of_property_of_type(context, "a").expect("property");
        let symbol = checker.get_property_of_type(context, "a").expect("origin symbol");
        let declared = checker.get_type_of_symbol(symbol);
        assert_ne!(semantic, declared);
        assert_eq!(checker.type_to_string(semantic), "\"right\"");
        assert_eq!(checker.type_to_string(declared), "T");
        let literal = literal.expect("literal");
        assert_eq!(checker.get_contextual_type(literal), Some(semantic));

        // Some(error) is an existing semantic value, not an absent property or
        // permission to fall back to the original declaration's type parameter.
        let error = checker.intrinsics.error;
        checker.anonymous_properties.get_mut(&context).expect("captured").0[0].slot =
            crate::objects::PropertySlot::resolved(error);
        assert_eq!(checker.get_contextual_type(literal), Some(error));
    }
}
