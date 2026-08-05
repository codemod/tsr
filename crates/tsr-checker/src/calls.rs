//! Calls: `f(x)`, `a.b()`, and what a call expression's type is.
//!
//! Ported from `Checker.checkCallExpression` into `resolveCallExpression` and
//! `getResolvedSignature` (`checker.go:8951`, `:9995`), reduced to the single
//! question this port can answer exactly: *given the callee's type, which
//! signature is called, and what does it return?*
//!
//! # Overload resolution is a cliff, and it is not approached
//!
//! `resolveCall` (`checker.go:9563`) chooses among candidate signatures by
//! **assignability** — it builds an argument list, runs inference for generic
//! candidates, and picks the first candidate every argument is assignable to,
//! falling back to the one with the fewest failures for error reporting. There is
//! no assignability in this port and no inference, so a callee with more than one
//! call signature is a gap. Taking the first candidate would produce a plausible
//! type for every overloaded call in the corpus and be wrong for most of them,
//! which is the exact failure the `errorType`-not-`anyType` discipline exists to
//! prevent.
//!
//! What that leaves is the single-signature call, which is the common shape:
//! `f(1)` where `f` is one function declaration.
//!
//! # Arguments are not checked, and that is visible
//!
//! Upstream checks each argument against the parameter it lands on and reports.
//! This computes the callee's signature and returns its return type without
//! looking at the arguments at all, because every diagnostic is `bd tsr-5e7.6`
//! and arity checking without assignability would report on shapes it cannot
//! judge. The **return type is unaffected** by this for a non-generic signature,
//! which is why it is a sound reduction rather than a shortcut: a generic
//! signature's return type *does* depend on the arguments, so a call to one is a
//! gap.

use tsr_ast::CallExpression;

use crate::{
    checker::Checker,
    signatures::Signature,
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
    /// The type of a call expression.
    ///
    /// Ported from `Checker.checkCallExpression` (`checker.go:8951`) into
    /// `getReturnTypeOfSignature` on the resolved signature.
    ///
    /// Not ported, each answering `errorType`: an optional chain (`f?.()`), a
    /// `super(…)` call, an `import(…)` call, a call with explicit type arguments,
    /// and a call to a callee with anything other than exactly one call
    /// signature.
    pub fn check_call_expression(&mut self, node: &CallExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        if node.question_dot_token.is_some() || !node.type_arguments.is_empty() {
            return error;
        }
        let Some(callee) = node.expression else { return error };
        let callee_type = self.check_expression(callee);
        let Some(signature) = self.resolve_call_signature(callee_type) else {
            return error;
        };
        // Upstream would now check the arguments and report; see the module docs
        // for why this does not, and why the return type is the same either way.
        if !signature.type_parameters.is_empty() {
            // A generic signature's return type depends on the arguments, so it
            // needs inference (`inferTypeArguments`, `checker.go:9310`). Answering
            // the uninstantiated return type would print `T` where upstream prints
            // what `T` was inferred as.
            return error;
        }
        signature.r#type
    }

    /// The single call signature of a type, or `None`.
    ///
    /// Ported from `getSignaturesOfType(t, SignatureKindCall)`
    /// (`checker.go:21470`) followed by the part of `resolveCall`
    /// (`checker.go:9563`) that is decidable without assignability: when there is
    /// exactly one candidate, resolution has nothing to choose and the answer is
    /// that candidate.
    ///
    /// Only an **anonymous object type** has signatures here — the shape a
    /// function, method, class, enum or value-module symbol has, and the shape a
    /// function expression or arrow is ([`TypeData::Anonymous`]). An interface
    /// with a call signature member, and a function *type node*
    /// (`(x: number) => void` in annotation position), both still resolve to
    /// nothing: the first needs call-signature members and the second is an
    /// unported type node. Both are gaps rather than wrong answers, and both are
    /// named in `docs/architecture/checker.md`.
    fn resolve_call_signature(&mut self, callee: TypeId) -> Option<Signature> {
        let TypeData::Anonymous { symbol, .. } = self.store.get(callee).data else {
            return None;
        };
        let signatures = self.get_signatures_of_symbol(symbol)?;
        match signatures.as_slice() {
            [signature] => Some(signature.clone()),
            // Zero: the callee is a class, an enum or a namespace — upstream
            // reports "this expression is not callable" and answers `errorType`.
            //
            // Two or more: an overload set, which needs assignability. **This
            // arm is live, and it is the second-largest blocker on a call in
            // the corpus** — 357 of the 1,496 declarations initialised by a
            // call to a locally declared `function`, 24%, against 884 (59%)
            // stopped one line up by the generic test in
            // [`Checker::check_call_expression`]. (Counted from the corpus
            // source carried in the `.types` baselines; declarations, not
            // assertion lines.)
            //
            // It used to be unreachable, and this comment used to say so. What
            // falsified that is `6b701cc`: until an overload set had a printed
            // type, the callee was already `errorType` and never arrived here.
            // Now it arrives, resolves to an anonymous type carrying its
            // symbol, and stops on this arm. The old note's own closing
            // sentence — "it becomes the only thing between a call and a guess
            // the moment overload sets print" — is the present tense.
            //
            // Choosing among candidates is `resolveCall` (`checker.go:9563`),
            // which picks by assignability; there is no relation here, so
            // taking the first candidate would answer a plausible wrong type
            // for every overloaded call in the corpus.
            _ => None,
        }
    }
}
