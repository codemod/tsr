//! Calls: `f(x)`, `a.b()`, and what a call expression's type is.
//!
//! Ported from `Checker.checkCallExpression` into `resolveCallExpression` and
//! `getResolvedSignature` (`checker.go:8951`, `:9995`), reduced to the single
//! question this port can answer exactly: *given the callee's type, which
//! signature is called, and what does it return?*
//!
//! # Overload resolution, over the arguments where a `false` can be trusted
//!
//! `resolveCall` (`checker.go:8843`) chooses among candidate signatures by
//! **assignability** — it builds an argument list, runs inference for generic
//! candidates, and picks the first candidate every argument is assignable to,
//! falling back to the one with the fewest failures for error reporting.
//!
//! [`Checker::choose_overload`] ports the selection. What makes it a port rather
//! than a guess is that it is the first thing in this checker to depend on a
//! **negative** answer from the relater — skipping a candidate is what lets the
//! next one win — and the relater's `false` is only sound over some of the type
//! space. So selection runs over [`SELECTABLE`] argument and parameter types and
//! answers `errorType` everywhere else. Taking the first candidate instead would
//! produce a plausible type for every overloaded call in the corpus and be wrong
//! for most of them, which is the exact failure the `errorType`-not-`anyType`
//! discipline exists to prevent.
//!
//! The corpus shape this targets: of the calls to a locally declared overloaded
//! `function` in the `.types` baselines, 523 are to a non-generic set whose
//! candidates differ by **parameter type**, against 59 differing only by arity
//! and 84 with a generic candidate. Assignability, not arity, is what those
//! calls need, and they are spread over 189 files — the largest is 40 sites, and
//! the top ten hold 26% — so this is corpus-wide rather than one file's shape.
//!
//! # Arguments are not *reported on*, and that is visible
//!
//! Upstream checks each argument against the parameter it lands on and reports.
//! Arguments are read here only to select among overloads; nothing is reported,
//! because every diagnostic is `bd tsr-5e7.6` and arity checking without a full
//! assignability relation would report on shapes it cannot judge. The **return
//! type is unaffected** by this for a non-generic signature,
//! which is why it is a sound reduction rather than a shortcut. A generic
//! signature's return type *does* depend on the arguments, and that is
//! [`crate::inference`]'s question — it answers the shapes whose type arguments
//! can be read straight off an argument position and `errorType` for the rest,
//! which is still most of them (`docs/architecture/checker-notes-infer.md`).

use tsr_ast::{CallExpression, Expression, TaggedTemplateExpression};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    signatures::Signature,
    types::{TypeData, TypeId},
};

/// The type domains over which this port's [`Checker::is_type_assignable_to`]
/// answers `false` only when the relation genuinely does not hold.
///
/// Overload selection is the first caller that depends on a **negative** answer
/// from the relater: skipping a candidate is what makes the next one win. Every
/// other caller so far only depends on a positive one. The relater's module docs
/// record that two distinct object types answer `false` because structural
/// comparison is narrow, not because they are unrelated — a false negative there
/// would silently promote the *next* overload, which is precisely the plausible
/// wrong answer this port refuses to produce.
///
/// So selection runs only where a `false` is trustworthy: the domains
/// `isSimpleTypeRelatedTo` decides on flags alone. This is deliberately
/// *narrower* than upstream's `TypeFlagsPrimitive` — `ENUM_LIKE`,
/// `ES_SYMBOL_LIKE`, `TEMPLATE_LITERAL` and `STRING_MAPPING` are left out,
/// because an enum literal's relation runs through its declared type, a unique
/// symbol's through its declaration, and the other two are unported. A candidate
/// or argument outside this set makes the whole call a gap rather than a guess.
const SELECTABLE: TypeFlags = TypeFlags::STRING
    .union(TypeFlags::STRING_LITERAL)
    .union(TypeFlags::NUMBER)
    .union(TypeFlags::NUMBER_LITERAL)
    .union(TypeFlags::BIG_INT)
    .union(TypeFlags::BIG_INT_LITERAL)
    .union(TypeFlags::BOOLEAN)
    .union(TypeFlags::BOOLEAN_LITERAL)
    .union(TypeFlags::VOID)
    .union(TypeFlags::UNDEFINED)
    .union(TypeFlags::NULL)
    .union(TypeFlags::NEVER);

impl Checker<'_, '_> {
    /// The type of a call expression.
    ///
    /// Ported from `Checker.checkCallExpression` (`checker.go:8951`) into
    /// `getReturnTypeOfSignature` on the resolved signature.
    ///
    /// Not ported, each answering `errorType`: an optional chain (`f?.()`), a
    /// `super(…)` call, an `import(…)` call, a call with explicit type arguments,
    /// a call to a callee with no call signature, and an overloaded call outside
    /// what [`Checker::choose_overload`] can decide.
    pub fn check_call_expression(&mut self, node: &CallExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        if node.question_dot_token.is_some() {
            return error;
        }
        let Some(callee) = node.expression else { return error };
        let callee_type = self.check_expression(callee);
        let Some(signature) = self.resolve_call_signature(callee_type, Some(node.arguments)) else {
            return error;
        };
        // Upstream would now report on the arguments; see the module docs for
        // why this does not, and why the return type is the same either way.
        if !signature.type_parameters.is_empty() {
            // A generic signature's return type depends on the arguments, so it
            // needs inference (`inferTypeArguments`, `checker.go:9390`). Answering
            // the uninstantiated return type would print `T` where upstream prints
            // what `T` was inferred as, so [`crate::inference`] answers the shapes
            // it can read a candidate off directly and `errorType` for the rest.
            return self.check_generic_call(&signature, node.node_id, node.arguments);
        }
        // `checkNoTypeArguments` (`checker.go:23157`): type arguments on a
        // signature that takes none is an error, and answering the return type
        // would quietly drop them.
        if !node.type_arguments.is_empty() {
            return error;
        }
        signature.r#type
    }

    /// The type of a tagged template: ``tag`a${b}c` ``.
    ///
    /// Ported from `Checker.checkTaggedTemplateExpression` (`checker.go:10034`),
    /// which is three lines once the grammar checks are set aside: resolve the
    /// tag's signature and return its return type.
    ///
    /// # The same reduction as a call, because upstream treats it as one
    ///
    /// `getResolvedSignature` is the *same* entry point a call expression uses —
    /// a tagged template is a call whose arguments are the template strings
    /// array and the substitutions. So this shares
    /// [`Checker::resolve_call_signature`] rather than growing a second
    /// resolution path, but passes no argument list, so it keeps the restriction
    /// to a callee with exactly one call signature: choosing among several needs
    /// the argument types, and a tagged template's arguments are the template
    /// strings array and the substitutions, neither of which this port builds.
    ///
    /// The arguments are not checked, which is sound here for the same reason it
    /// is sound for a call — a non-generic signature's return type does not
    /// depend on them. **The template is still checked**, so its own line is
    /// populated; today that line is usually a gap, because
    /// `TemplateExpression` is unported and correctly stays so
    /// (`checker-notes-arrays.md` records why it is a workstream). A tag applied
    /// to a template with no substitutions gets a real answer for the template,
    /// since that is a `NoSubstitutionTemplateLiteral`.
    ///
    /// Not ported, each answering `errorType`: an optional chain (``tag?.`x` ``,
    /// **unobservable** — the grammar prohibits it and the parser rejects it, so
    /// the guard mirrors `check_call_expression` rather than covering a reachable
    /// case), explicit type arguments, a generic tag signature, and a tag whose type has
    /// anything other than exactly one call signature. The generic case is the
    /// one that matters most — `String.raw` and every typed template helper is
    /// generic — which is why the 289 gap lines this form carries will not all
    /// close here.
    pub(crate) fn check_tagged_template_expression(
        &mut self,
        node: &TaggedTemplateExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        if node.question_dot_token.is_some() || !node.type_arguments.is_empty() {
            return error;
        }
        let Some(tag) = node.tag else { return error };
        let tag_type = self.check_expression(tag);
        // Checked for its own line; the template's type does not reach the
        // answer, exactly as a call's arguments do not.
        if let Some(template) = node.template {
            self.check_expression(template.into());
        }
        // `None` for the argument list: a tagged template's arguments are the
        // template strings array and the substitutions, neither of which this
        // port builds, so an overloaded tag stays a gap.
        let Some(signature) = self.resolve_call_signature(tag_type, None) else {
            return error;
        };
        if !signature.type_parameters.is_empty() {
            // A generic tag's return type depends on the inferred arguments,
            // which for a tagged template means inferring from the template
            // strings array and each substitution.
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
    fn resolve_call_signature(
        &mut self,
        callee: TypeId,
        arguments: Option<&[Expression<'_>]>,
    ) -> Option<Signature> {
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
            // Choosing among candidates is `resolveCall` (`checker.go:8843`),
            // which picks by assignability. That is what
            // [`Checker::choose_overload`] does, over the argument domains
            // where a *negative* assignability answer can be trusted.
            candidates => self.choose_overload(candidates, arguments?),
        }
    }

    /// The first candidate every argument is assignable to, or `None`.
    ///
    /// Ported from `Checker.chooseOverload` (`checker.go:9025`), which walks the
    /// candidate list in declaration order, keeps the ones `hasCorrectArity`
    /// (`checker.go:9107`) admits, and returns the first whose parameters every
    /// argument satisfies under the assignable relation. Declaration order is
    /// load-bearing — it is the whole tie-break — and
    /// [`Checker::get_signatures_of_symbol`] preserves it.
    ///
    /// # What is reduced away, each answering `None` so the call is `errorType`
    ///
    /// Upstream runs three passes over the candidates (subtype, strict-subtype,
    /// assignable) and, on total failure, reports against the candidate with the
    /// fewest problems. Only the assignable pass is here: the earlier two exist
    /// to prefer a more specific candidate when several match, and this reduction
    /// answers `None` rather than guessing whenever that could bite — see below.
    ///
    /// - **A generic candidate anywhere in the set.** Selecting it needs
    ///   `inferTypeArguments`; its return type would print `T`.
    /// - **A spread argument** (`f(...xs)`), which changes what arity means.
    /// - **A rest or `this` parameter** on any candidate, for the same reason.
    /// - **Any argument or parameter type outside [`SELECTABLE`]** — the domains
    ///   where a `false` from the relater is a real `false`. This is the guard
    ///   that keeps a narrow structural relation from promoting the wrong
    ///   overload, and it is why the object-typed overload sets in the corpus
    ///   stay gaps.
    /// - **More than one arity-and-assignability match with different return
    ///   types.** Upstream's subtype pass would pick among them by specificity;
    ///   without it, taking the first is a guess. Where every match returns the
    ///   *same* type the pass could not have changed the answer, so it is taken.
    fn choose_overload(
        &mut self,
        candidates: &[Signature],
        arguments: &[Expression<'_>],
    ) -> Option<Signature> {
        if candidates.iter().any(|candidate| {
            !candidate.type_parameters.is_empty()
                || candidate.this_parameter.is_some()
                || candidate.parameters.iter().any(|parameter| parameter.rest)
                || !candidate.parameters.iter().all(|p| self.is_selectable(p.r#type))
        }) {
            return None;
        }
        let mut argument_types = Vec::with_capacity(arguments.len());
        for &argument in arguments {
            if matches!(argument, Expression::SpreadElement(_)) {
                return None;
            }
            let argument_type = self.check_expression(argument);
            if !self.is_selectable(argument_type) {
                return None;
            }
            argument_types.push(argument_type);
        }
        let mut chosen: Option<&Signature> = None;
        for candidate in candidates {
            if !has_correct_arity(candidate, argument_types.len()) {
                continue;
            }
            let applicable =
                argument_types.iter().zip(&candidate.parameters).all(|(&argument, parameter)| {
                    self.is_type_assignable_to(argument, parameter.r#type)
                });
            if !applicable {
                continue;
            }
            match chosen {
                // Upstream's subtype pass would decide this; see the doc
                // comment. Same return type either way means it could not have.
                Some(first) if first.r#type != candidate.r#type => return None,
                Some(_) => {}
                None => chosen = Some(candidate),
            }
        }
        chosen.cloned()
    }

    /// Whether a `false` from [`Checker::is_type_assignable_to`] about this type
    /// means the relation does not hold. See [`SELECTABLE`].
    fn is_selectable(&self, id: TypeId) -> bool {
        // A union is selectable when every constituent is: the relation
        // distributes over it, so a `false` is as trustworthy as the worst
        // constituent's. A union carrying a symbol is an enum or a named alias,
        // and falls through to the flag test, which rejects it: a union's own
        // flags are `UNION`, not the union of its constituents' flags.
        if let TypeData::Union { types, symbol: None, .. } = &self.store.get(id).data {
            return types.iter().all(|&t| self.is_selectable(t));
        }
        let flags = self.store.get(id).flags;
        !flags.is_empty() && SELECTABLE.contains(flags)
    }
}

/// Whether a signature accepts exactly this many arguments.
///
/// Ported from `Checker.hasCorrectArity` (`checker.go:9107`), reduced to the two
/// bounds that survive once rest parameters, spread arguments and the
/// signature-help trailing comma are excluded by
/// [`Checker::choose_overload`]: at least the required parameters, at most all
/// of them.
fn has_correct_arity(candidate: &Signature, argument_count: usize) -> bool {
    let required = candidate.parameters.iter().take_while(|p| !p.optional).count();
    argument_count >= required && argument_count <= candidate.parameters.len()
}
