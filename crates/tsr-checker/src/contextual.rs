//! Contextual typing: what an *unannotated* parameter's type comes from when
//! the function it belongs to is written where a function type is expected.
//!
//! Ported from `Checker.getContextuallyTypedParameterType` (`checker.go:29458`),
//! reduced to its single largest corpus arm — a function expression or arrow
//! passed as a **call argument**, whose parameters are typed from the
//! corresponding parameter of the callee's signature.
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
//! is the arm this module ports, and the other two are gaps that stay gaps.
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

use tsr_ast::{BindingName, Expression, Node, NodeId, ParameterDeclaration};

use crate::{
    checker::Checker,
    signatures::Signature,
    types::{TypeData, TypeId},
};

impl<'a> Checker<'a, '_> {
    /// The type an unannotated parameter takes from its context, if any.
    ///
    /// Ported from `Checker.getContextuallyTypedParameterType`
    /// (`checker.go:29458`), argument-position arm only.
    ///
    /// `None` is upstream's nil: no contextual type is available, and the caller
    /// turns that into the implicit `any`. See the module documentation for why
    /// `None` rather than `errorType` is the honest answer *here specifically*.
    ///
    /// Not ported, each answering `None`:
    ///
    /// - **An immediately invoked function expression** (`(x => x)(1)`), which
    ///   upstream types from the *argument expressions* rather than from a
    ///   signature (`checker.go:29463`). It is a separate mechanism — the
    ///   corpus's `contextuallyTypedIife` cases are 28 of the 925 — and folding
    ///   it in here would share none of the code below.
    /// - **Every non-argument position**: a variable with a function-type
    ///   annotation, an object-literal member, a `return`, a JSX attribute. Each
    ///   needs its own arm of `getContextualType` (`checker.go:29290`).
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
        let parameters = self.contextualisable_parameters(function)?;

        // `slices.Index(fn.Parameters(), parameter)` (`checker.go:29489`). A
        // `this` parameter would make this index disagree with the signature's,
        // which is exactly what upstream's `GetThisParameter` subtraction on the
        // same line repairs; here it is a gap instead, so the two indices are
        // the same index or there is no answer.
        let index = parameters.iter().position(|p| p.node_id == Some(parameter))?;
        if parameters.iter().any(|p| p.dot_dot_dot_token.is_some() || is_this_parameter(p)) {
            return None;
        }

        let signature = self.contextual_signature_from_call(function)?;
        let contextual = signature.parameters.get(index)?;
        // A contextual parameter that is itself optional or rest carries a type
        // whose relationship to the position is not the plain one — upstream
        // reaches those through `getRestTypeAtPosition` and the optionality
        // rules in `crate::optionality`. Neither is asserted from here.
        if contextual.optional || contextual.rest {
            return None;
        }
        Some(contextual.r#type)
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
    fn contextualisable_parameters(
        &self,
        function: NodeId,
    ) -> Option<&'a [&'a ParameterDeclaration<'a>]> {
        match self.node_map.get(function)? {
            Node::ArrowFunction(node) => Some(node.parameters),
            Node::FunctionExpression(node) => Some(node.parameters),
            _ => None,
        }
    }

    /// The signature `function` is contextually typed by, when it sits directly
    /// in the argument list of a call.
    ///
    /// Ported from `Checker.getContextualSignature` (`checker.go:10264`) reached
    /// through `getContextualTypeForArgumentAtIndex` (`checker.go:29772`),
    /// collapsed to the one path that needs no argument checked — see the module
    /// documentation on the recursion this avoids.
    ///
    /// Upstream's union handling (`getContextualSignature`'s
    /// `compareSignaturesIdentical` loop) is not here: a union-typed parameter
    /// answers `None`, because picking one member's signature is a guess and
    /// building the union signature needs `createUnionSignature`.
    fn contextual_signature_from_call(&mut self, function: NodeId) -> Option<Signature> {
        let call = self.nodes.parent(function)?;
        let Some(Node::CallExpression(call)) = self.node_map.get(call) else {
            return None;
        };
        // `getEffectiveCallArguments` (`checker.go:29772`'s `argIndex` domain).
        // A spread anywhere before this argument makes the index meaningless, so
        // it is a gap rather than an off-by-one waiting to happen.
        if call.arguments.iter().any(|a| matches!(a, Expression::SpreadElement(_))) {
            return None;
        }
        let argument = call.arguments.iter().position(|a| a.node_id() == Some(function))?;

        let callee = call.expression?;
        let callee_type = self.check_expression(callee);
        let parameter =
            self.single_call_signature(callee_type)?.parameters.into_iter().nth(argument)?;
        self.single_call_signature(parameter.r#type)
    }

    /// The one call signature of `id`, or `None` if it has any other number of
    /// them or is generic.
    ///
    /// Ported from `Checker.getSignaturesOfType` (`checker.go:19806` via
    /// [`Checker::get_signatures_of_symbol`]) with the two reductions the module
    /// documentation justifies: an overload set is `None` because choosing needs
    /// arguments checked, and a generic signature is `None` because its
    /// parameter types would print as their own type parameters — `T`, not the
    /// type the caller supplied.
    fn single_call_signature(&mut self, id: TypeId) -> Option<Signature> {
        let TypeData::Anonymous { symbol, .. } = self.store.get(id).data else {
            return None;
        };
        match self.get_signatures_of_symbol(symbol)?.as_slice() {
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
