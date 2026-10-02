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
                    kind: crate::signatures::SignatureKind::Call,
                    type_parameters: Vec::new(),
                    this_parameter: None,
                    parameters: vec![crate::signatures::Parameter {
                        name: "args".to_owned(),
                        optional: false,
                        rest: true,
                        r#type: arguments,
                        written_text: None,
                    }],
                    r#type: self.intrinsics.void,
                    written_return: None,
                    predicate: None,
                };
                let contextual = if parameters[index].dot_dot_dot_token.is_some() {
                    Some(self.signature_rest_type_at_position(&signature, index))
                } else {
                    self.signature_type_at_position(&signature, index).or_else(|| {
                        parameters[index].initializer.is_none().then_some(self.intrinsics.undefined)
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
            return Some(self.intrinsics.undefined);
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
        if let Some(signature) =
            self.node_types.get(&function).and_then(|ty| self.signature_types.get(ty)).and_then(
                |signatures| signatures.iter().find(|signature| signature.declaration == function),
            )
            && let Some(parameter) = signature.parameters.get(index)
            && parameter.r#type != self.intrinsics.error
        {
            return Some(parameter.r#type);
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
    fn signatures_identical(left: &Signature, right: &Signature) -> bool {
        if left.kind != right.kind
            || !left.type_parameters.is_empty()
            || !right.type_parameters.is_empty()
            || left.parameters.len() != right.parameters.len()
        {
            return false;
        }
        left.parameters
            .iter()
            .zip(&right.parameters)
            .all(|(a, b)| a.r#type == b.r#type && a.optional == b.optional && a.rest == b.rest)
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
                    if !Self::signatures_identical(existing, &signature) {
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

    fn contextual_call_signature(
        &mut self,
        contextual: TypeId,
        function: Option<NodeId>,
    ) -> Option<ContextualSignature> {
        if contextual == self.intrinsics.error {
            return None;
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
    fn expand_contextual_tuple_rest(&self, mut signature: Signature) -> Signature {
        let Some(rest) = signature.parameters.last().filter(|p| p.rest) else {
            return signature;
        };
        let Some((elements, _)) = self.tuple_element_lists.get(&rest.r#type) else {
            return signature;
        };
        let rest_type = rest.r#type;
        let rest_name = rest.name.clone();
        let expanded: Vec<_> = elements
            .iter()
            .enumerate()
            .map(|(i, &t)| crate::signatures::Parameter {
                name: self
                    .tuple_labels
                    .get(&rest_type)
                    .and_then(|labels| labels.get(i))
                    .and_then(Clone::clone)
                    .unwrap_or_else(|| format!("{rest_name}_{i}")),
                optional: self
                    .tuple_optional_masks
                    .get(&rest_type)
                    .and_then(|mask| mask.get(i))
                    .copied()
                    .unwrap_or(false),
                rest: false,
                r#type: t,
                written_text: None,
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
            let a_type = a.map_or(self.intrinsics.unknown, |p| p.r#type);
            let b_type = b.map_or(self.intrinsics.unknown, |p| p.r#type);
            parameters.push(crate::signatures::Parameter {
                name,
                optional: i >= left_min && i >= right_min,
                rest: false,
                r#type: self.get_union_type(&[a_type, b_type]),
                written_text: None,
            });
        }
        left.parameters = parameters;
        left.this_parameter = match (left.this_parameter, right.this_parameter) {
            (Some(mut a), Some(b)) => {
                a.r#type = self.get_union_type(&[a.r#type, b.r#type]);
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
        let parent = self.nodes.parent(node)?;
        match self.node_map.get(parent)? {
            // `getContextualTypeForInitializerExpression` (`checker.go:29423`) →
            // `getContextualTypeForVariableLikeDeclaration` (`checker.go:29438`),
            // whose first three lines are the whole of this arm: if the
            // declaration has a type node, the contextual type *is* that type.
            //
            // Restricted to a `VariableDeclaration`. The other variable-like
            // carriers of an annotation — a `PropertyDeclaration`, a
            // `PropertySignature`, a parameter with a function-typed annotation
            // and a function initialiser — reach the same upstream function, but
            // each is a separate corpus shape and none is measured, so each is a
            // gap rather than an untested generalisation.
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
                let annotation = declaration.r#type?;
                Some(self.get_type_from_type_node(annotation))
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
                let annotation = declaration.r#type?;
                Some(self.get_type_from_type_node(annotation))
            }
            Node::CallExpression(call) => self.contextual_type_for_argument(call, node),
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
            // §68.1: `ParenthesizedExpression` (`checker.go:29392`) — the
            // one-line recursion, un-rejected with §68's own argument.
            Node::ParenthesizedExpression(_) => self.get_contextual_type(parent),
            // §68.2: an ARRAY LITERAL element's contextual type is the
            // array's contextual ELEMENT type
            // (`getContextualTypeForElementExpression`, `checker.go:29972`) —
            // the Array-reference and tuple halves this port can read.
            Node::ArrayLiteralExpression(literal) => {
                let contextual = self.get_contextual_type(parent)?;
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
                    literal.elements.len(),
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
                let signature = self.contextual_signature(parent)?;
                self.contextual_return_expression_slot(parent, signature.r#type, true)
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
                let annotation = match self.node_map.get(function)? {
                    Node::FunctionDeclaration(f) => f.r#type,
                    Node::FunctionExpression(f) => f.r#type,
                    Node::ArrowFunction(f) => f.r#type,
                    Node::MethodDeclaration(f) => f.r#type,
                    _ => None,
                };
                if let Some(annotation) = annotation {
                    let contextual = self.get_type_from_type_node(annotation);
                    return self.contextual_return_expression_slot(function, contextual, false);
                }
                // getContextualReturnType also uses the non-generic contextual
                // signature of function expressions and object literal methods.
                let signature = self.contextual_signature(function)?;
                self.contextual_return_expression_slot(function, signature.r#type, true)
            }
            _ => None,
        }
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
        let contextual = if generator {
            self.contextual_generator_iteration_type(contextual, 1)
        } else {
            Some(contextual)
        }?;
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

    /// getContextualTypeForYieldOperand (internal/checker/checker.go), for
    /// the global iterator/iterable references whose iteration slots are known.
    fn contextual_type_for_yield_operand(
        &mut self,
        yield_id: NodeId,
        delegates: bool,
    ) -> Option<TypeId> {
        let function = self.containing_function(yield_id)?;
        let (annotation, modifiers) = match self.node_map.get(function)? {
            Node::FunctionDeclaration(node) => (node.r#type, node.modifiers),
            Node::FunctionExpression(node) => (node.r#type, node.modifiers),
            Node::MethodDeclaration(node) => (node.r#type, node.modifiers),
            _ => return None,
        };
        let contextual = if let Some(annotation) = annotation {
            self.get_type_from_type_node(annotation)
        } else {
            self.contextual_signature(function)?.r#type
        };
        let (target, arguments) = self.type_reference_targets.get(&contextual)?.clone();
        let supported = [
            ("Iterator", 3),
            ("Iterable", 3),
            ("IterableIterator", 3),
            ("Generator", 3),
            ("AsyncIterator", 3),
            ("AsyncIterable", 3),
            ("AsyncIterableIterator", 3),
            ("AsyncGenerator", 3),
        ]
        .into_iter()
        .any(|(name, arity)| {
            self.global_type_symbol_with_arity(name, arity).is_some_and(|symbol| {
                self.binder.merged_symbol(symbol) == self.binder.merged_symbol(target)
            })
        });
        if !supported {
            return None;
        }
        let yielded = *arguments.first()?;
        let is_async = crate::check::has_modifier(modifiers, tsr_ast::SyntaxKind::AsyncKeyword);
        if !delegates {
            return if is_async {
                self.contextual_awaited_type_no_alias(yielded)
            } else {
                Some(yielded)
            };
        }
        let returned = self.get_contextual_type(yield_id).unwrap_or(self.intrinsics.never);
        let next = arguments.get(2).copied().unwrap_or(self.intrinsics.unknown);
        let generator = self.global_type_symbol_with_arity("Generator", 3)?;
        let sync = self.create_type_reference(generator, vec![yielded, returned, next]);
        if is_async {
            let generator = self.global_type_symbol_with_arity("AsyncGenerator", 3)?;
            let asynchronous = self.create_type_reference(generator, vec![yielded, returned, next]);
            Some(self.get_union_type(&[sync, asynchronous]))
        } else {
            Some(sync)
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
    /// spreads instead receive the union of the remaining possible elements.
    pub(crate) fn contextual_type_for_element_expression(
        &mut self,
        contextual: TypeId,
        index: usize,
        length: usize,
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
                return Some(element_type(self, &elements[index]));
            }
            let offset = if last_spread.is_none_or(|spread| index > spread) {
                length.saturating_sub(index)
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
            let skip = last_spread.map_or(fixed_end, |spread| fixed_end.min(length - spread));
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
        self.tuple_spread_array_element(contextual)
    }

    /// The rest-argument context used by `getSpreadArgumentType`, alongside
    /// ordinary `getTypeAtPosition` parameters (internal/checker/checker.go).
    fn contextual_argument_type(
        &mut self,
        signature: &Signature,
        index: usize,
        argument_count: usize,
    ) -> Option<TypeId> {
        let rest = signature.parameters.iter().position(|parameter| parameter.rest);
        if let Some(rest) = rest
            && index >= rest
        {
            if self
                .store
                .get(signature.parameters[rest].r#type)
                .flags
                .contains(crate::flags::TypeFlags::TYPE_PARAMETER)
            {
                let index_type = self.store.intern(
                    crate::flags::TypeFlags::NUMBER_LITERAL,
                    crate::types::TypeData::NumberLiteral((index - rest).to_string()),
                );
                return self.resolved_indexed_access_type(
                    signature.parameters[rest].r#type,
                    index_type,
                    false,
                );
            }
            return self.contextual_type_for_element_expression(
                signature.parameters[rest].r#type,
                index - rest,
                argument_count - rest,
                None,
                None,
            );
        }
        signature.parameters.get(index).map(|parameter| parameter.r#type)
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
    fn contextual_type_for_object_literal_named_element(
        &mut self,
        element: NodeId,
        property_name: PropertyName<'a>,
    ) -> Option<TypeId> {
        // `c.hasBindableName(element)` (`checker.go:29927`) reduced to the names
        // `get_property_of_type` can be keyed by. See "Not ported" above.
        let name = match property_name {
            PropertyName::Identifier(name) => name.text.to_string(),
            PropertyName::StringLiteral(name) => name.text.to_string(),
            PropertyName::NumericLiteral(name) => name.text.to_string(),
            PropertyName::ComputedPropertyName(name) => self.late_bound_symbol_member_name(name)?.0,
            _ => return None,
        };
        // `objectLiteral := element.Parent` (`checker.go:29924`). No check that
        // the parent *is* an `ObjectLiteralExpression`: a `PropertyAssignment`
        // has no other possible parent in this AST, so a guard here could not be
        // reddened by any mutation and would read as evidence for a decision that
        // was never made. Same call, same reason, as the initialiser guard above.
        let object_literal = self.nodes.parent(element)?;
        let contextual = self.get_contextual_type(object_literal)?;
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
        } else {
            match self.get_property_of_type(contextual, &name) {
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
        if let Some((map, type_parameters, names)) =
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
            let parameter_type =
                self.contextual_argument_type(&single, index, call.arguments.len())?;
            // §946: upstream's PASS ONE — the parameter type as WRITTEN, before
            // the fixing mapper below replaces this signature's type parameters
            // with `unknown`. Only the freshness query asks for it, and it asks
            // through `contextual_prefers_uninstantiated`.
            if self.contextual_prefers_uninstantiated
                || (call.type_arguments.is_empty()
                    && self
                        .mapped_types
                        .get(&parameter_type)
                        .is_some_and(|info| info.name_type.is_none()))
                || self
                    .uninstantiated_context_node
                    .is_some_and(|node| call.arguments[index].node_id() == Some(node))
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
            let contextual_call = call
                .node_id
                .is_some_and(|call_id| self.get_contextual_type_of_call(call_id).is_some());
            let Some(type_parameter_ids) = self.type_parameter_types(&single) else {
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
            let written: Vec<TypeId> = call
                .type_arguments
                .iter()
                .map(|argument| self.get_type_from_type_node(*argument))
                .collect();
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
            return Some(parameter_type);
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
                    && (signature
                        .parameters
                        .iter()
                        .any(|p| self.mentions_any_type_parameter(p.r#type, depth - 1))
                        || self.mentions_any_type_parameter(signature.r#type, depth - 1))
                {
                    return true;
                }
            }
        }
        false
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
