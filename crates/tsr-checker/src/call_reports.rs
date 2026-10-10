//! The reporting pass of call resolution: `isSignatureApplicable`
//! (`checker.go:9256`) run with `reportErrors`, over
//! `getEffectiveCallArguments` (`checker.go:30042`), as
//! `reportCallResolutionErrors` (`checker.go:9649`) runs it for the last
//! entry of `candidatesForArgumentError`, and as `resolveCall`
//! (`checker.go:8843`) runs it for a single non-generic candidate.
//!
//! Native has one applicability check, called with `reportErrors == false`
//! while `chooseOverload` (`checker.go:9025`) chooses and once more with
//! `reportErrors == true` when nothing was chosen. This port chooses on the
//! type road (`calls.rs`, `inference.rs`) and reports from the diagnostic walk,
//! so every report site in `calls.rs` (a single non-generic candidate, a single
//! generic candidate's instantiation, the last failing overload) hands its
//! candidate here, and this file is the only place an argument error of a
//! call, `new` or tagged template is issued. The design and the declines it
//! keeps are recorded in `docs/parity/notes/r6-callreport.md`.
//!
//! # Checker port convention (`docs/conventions.md`)
//!
//! - **Native operation:** `isSignatureApplicable` with `reportErrors`, which
//!   checks each argument with `checkExpressionWithContextualType(arg,
//!   getTypeAtPosition(signature, i))` (uncached) and relates it with
//!   `checkTypeRelatedToAndOptionallyElaborate`.
//! - **Key identity and owner:** none. No cache, side table or mapper is
//!   added. Each argument's type is the one the type road published for the
//!   node (`node_types`), or the one the overload walk recorded while it
//!   checked the argument under the failing candidate
//!   (`OverloadArgumentFailure::checked`), passed in by the caller.
//! - **Publication states:** read only. The pass never evicts or re-checks an
//!   argument, so a report cannot change a printed type.
//! - **Receiver/alias context:** the `this` argument is
//!   `getThisArgumentOfCall`'s receiver (`this_argument_type_of_call`).
//! - **Expensive work:** one relation per argument up to the first failure,
//!   on the failure path only: the diagnostic walk reaches here only for a
//!   call the type road could not resolve, or whose candidate it must
//!   re-check. A successful call never enters.
//!
//! # Argument types
//!
//! Native's uncached `checkExpressionWithContextualType` and this port's
//! cached type agree when the argument's type cannot depend on the contextual
//! type, or when it was already checked under this candidate's parameter. A
//! context-sensitive function is assigned its contextual parameter types once
//! (`contextuallyCheckFunctionExpressionOrObjectLiteralMethod`, guarded by
//! `NodeCheckFlagsContextChecked`, `checker.go:10155`), so after resolution
//! its type is the same under any later context: its cached type is native's.
//! Where the cached type was taken under another context (an array or class
//! literal, or a context-sensitive object literal, checked while a generic
//! candidate was still being inferred) the argument is declined
//! ([`ApplicabilityReport::Declined`]); see the notes for each.

use tsr_ast::{Expression, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::messages;

use crate::{
    checker::Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
    signatures::Signature,
    types::{TypeData, TypeId},
};

/// One entry of `getEffectiveCallArguments` (`checker.go:30042`).
#[derive(Clone, Copy)]
pub(crate) enum ReportArgument<'a> {
    /// A written argument expression (a spread element included: it stays a
    /// spread argument, `isSpreadArgument`).
    Written(Expression<'a>),
    /// `createSyntheticExpression(parent, type, isSpread, …)`: a tagged
    /// template's `TemplateStringsArray` (located at the template) or one
    /// element of a spread tuple (located at the spread).
    Synthetic {
        at: NodeId,
        r#type: TypeId,
        #[expect(
            dead_code,
            reason = "isSpreadArgument; read once getSpreadArgumentType is ported (r7-calls note §1)"
        )]
        spread: bool,
    },
}

/// What [`Checker::report_signature_applicability`] concluded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ApplicabilityReport {
    /// Every argument relates; nothing was reported.
    Applicable,
    /// The first failure was reported.
    Reported,
    /// An argument whose type this port cannot certify as native's, or a
    /// relation it cannot decide, stopped the walk; nothing more is
    /// reported for the call.
    Declined,
}

/// How the candidate's argument types were published.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateContext {
    /// The candidate is the call's own non-generic signature: every argument
    /// was checked under its parameters as contextual type.
    Declared,
    /// The candidate is an instantiation (a generic candidate's inference
    /// result, or a failing overload): arguments were checked while it was
    /// being inferred or under another candidate.
    Instantiated,
}

impl<'a> Checker<'a, '_> {
    /// `getEffectiveCallArguments` (`checker.go:30042`) for a call, `new` or
    /// tagged template. A tagged template's first argument is the synthetic
    /// `TemplateStringsArray` at its template, followed by each span's
    /// expression; a spread of a tuple type becomes one synthetic argument per
    /// element (`getElementTypes`, a rest element as its array type and
    /// spread). `None` when a span has no expression, when the global
    /// `TemplateStringsArray` is missing, or when a spread's type is a gap.
    pub(crate) fn report_call_arguments(
        &mut self,
        node: NodeId,
    ) -> Option<Vec<ReportArgument<'a>>> {
        let arguments: &'a [Expression<'a>] = match self.node_map.get(node)? {
            tsr_ast::Node::CallExpression(call) => call.arguments,
            tsr_ast::Node::NewExpression(new) => new.arguments,
            tsr_ast::Node::TaggedTemplateExpression(tagged) => {
                let template = tagged.template?;
                let at = Expression::from(template).node_id()?;
                let strings = self.global_type_symbol_with_arity("TemplateStringsArray", 0)?;
                let strings = self.get_declared_type_of_symbol(strings);
                let mut effective =
                    vec![ReportArgument::Synthetic { at, r#type: strings, spread: false }];
                if let tsr_ast::TemplateLiteral::TemplateExpression(expression) = template {
                    for span in expression.template_spans {
                        effective.push(ReportArgument::Written(span.expression?));
                    }
                }
                return Some(effective);
            }
            _ => return None,
        };
        let Some(first_spread) =
            arguments.iter().position(|argument| matches!(argument, Expression::SpreadElement(_)))
        else {
            return Some(arguments.iter().copied().map(ReportArgument::Written).collect());
        };
        let mut effective: Vec<ReportArgument<'a>> =
            arguments[..first_spread].iter().copied().map(ReportArgument::Written).collect();
        for &argument in &arguments[first_spread..] {
            let Expression::SpreadElement(spread) = argument else {
                effective.push(ReportArgument::Written(argument));
                continue;
            };
            let at = argument.node_id()?;
            let spread_type = self.check_expression(spread.expression?);
            if self.is_gap(spread_type) {
                return None;
            }
            match self.tuple_spread_elements(spread_type) {
                Some(elements) => effective.extend(
                    elements.into_iter().map(|(r#type, spread)| ReportArgument::Synthetic {
                        at,
                        r#type,
                        spread,
                    }),
                ),
                None => effective.push(ReportArgument::Written(argument)),
            }
        }
        Some(effective)
    }

    /// `getElementTypes` with each element's `ElementFlagsVariable`, as
    /// `getEffectiveCallArguments` reads a spread tuple: a required or
    /// optional element is its type (an optional one carries `undefined`
    /// under `strictNullChecks`, as `getTypeFromTupleTypeNode`'s
    /// `addOptionality` gives it), a rest or variadic element its array type,
    /// spread. `None` when the type is not a tuple.
    fn tuple_spread_elements(&mut self, t: TypeId) -> Option<Vec<(TypeId, bool)>> {
        if let Some((elements, _)) = self.tuple_element_lists.get(&t).cloned() {
            let mask = self.tuple_optional_masks.get(&t).cloned();
            return Some(
                elements
                    .into_iter()
                    .enumerate()
                    .map(|(index, element)| {
                        let optional = mask
                            .as_ref()
                            .and_then(|mask| mask.get(index))
                            .copied()
                            .unwrap_or(false);
                        let element = if optional && self.strict_null_checks {
                            self.get_union_type(&[element, self.intrinsics.undefined])
                        } else {
                            element
                        };
                        (element, false)
                    })
                    .collect(),
            );
        }
        let (elements, _) = self.variadic_tuple_elements.get(&t).cloned()?;
        Some(
            elements
                .into_iter()
                .map(|element| {
                    if element.spread {
                        (element.r#type, true)
                    } else if element.optional && self.strict_null_checks {
                        (self.get_union_type(&[element.r#type, self.intrinsics.undefined]), false)
                    } else {
                        (element.r#type, false)
                    }
                })
                .collect(),
        )
    }

    /// `isSignatureApplicable` (`checker.go:9256`) with `reportErrors` and
    /// the assignable relation, as `reportCallResolutionErrors` runs it.
    ///
    /// The `this` argument first ([`Checker::report_this_argument`]); then
    /// each argument against `getTypeAtPosition`, related with
    /// `checkTypeRelatedToAndOptionallyElaborate` at `getEffectiveCheckNode`
    /// (`report_argument_failure`: `elaborateError`, then the TS2345 head);
    /// the first failure is the report. `checked` holds, per effective
    /// argument, the type the overload walk checked it as under this
    /// candidate, where it did.
    ///
    /// A signature with a non-array rest type (`getNonArrayRestType`) is
    /// declined: its trailing arguments are related as one
    /// `getSpreadArgumentType` tuple, not ported here yet.
    pub(crate) fn report_signature_applicability(
        &mut self,
        node: NodeId,
        arguments: &[ReportArgument<'a>],
        signature: &Signature,
        context: CandidateContext,
        checked: &[Option<TypeId>],
    ) -> ApplicabilityReport {
        match self.report_this_argument(node, signature, true) {
            Ternary::Related => {}
            Ternary::NotRelated => return ApplicabilityReport::Reported,
            Ternary::Unknown => return ApplicabilityReport::Declined,
        }
        if self.signature_non_array_rest_type(signature).is_some() {
            return ApplicabilityReport::Declined;
        }
        for (position, argument) in arguments.iter().enumerate() {
            let Some(target) = self.signature_type_at_position(signature, position) else {
                return ApplicabilityReport::Declined;
            };
            if self.is_gap(target) {
                return ApplicabilityReport::Declined;
            }
            let (at, source, expression) = match *argument {
                ReportArgument::Synthetic { at, r#type, .. } => (at, r#type, None),
                ReportArgument::Written(expression) => {
                    if matches!(expression, Expression::OmittedExpression(_)) {
                        continue;
                    }
                    let checked = checked.get(position).copied().flatten();
                    let source = match checked {
                        Some(checked) => checked,
                        None if self.report_argument_type_is_not_upstreams(expression) => {
                            return ApplicabilityReport::Declined;
                        }
                        None => self.check_expression(expression),
                    };
                    let Some(at) = Self::effective_check_expression(expression).node_id() else {
                        return ApplicabilityReport::Declined;
                    };
                    (at, source, checked.is_none().then_some(expression))
                }
            };
            let literal = expression.is_some_and(|expression| {
                matches!(
                    Self::effective_check_expression(expression),
                    Expression::ObjectLiteralExpression(_)
                )
            });
            let verdict = match context {
                // Every argument was checked under this parameter, and
                // `report_argument_failure` relates the pair itself (with the
                // union-target object literal's discriminated excess check,
                // which `relate_ternary` does not make).
                CandidateContext::Declared => None,
                CandidateContext::Instantiated => {
                    match self.relate_ternary(source, target, Relation::Assignable) {
                        Ternary::Related => continue,
                        Ternary::Unknown => return ApplicabilityReport::Declined,
                        Ternary::NotRelated => Some(Ternary::NotRelated),
                    }
                }
            };
            // `hasExcessProperties` (`relater.go:2714`) runs on the fresh
            // literal before any structural comparison. Against a non-union
            // target it reads only the literal's property names, which the
            // instantiated context's literal preservation does not change, so
            // its report needs no certification of the member types. A union
            // target's discriminant reduction reads the members' types, which
            // do change, and stays behind the certification below.
            if literal
                && verdict == Some(Ternary::NotRelated)
                && !matches!(self.store.get(target).data, TypeData::Union { .. })
                && self.report_fresh_literal_excess_property(at, source, target)
            {
                return ApplicabilityReport::Reported;
            }
            if let Some(expression) = expression
                && !self.report_argument_type_is_certified(expression, source, target, context)
            {
                return ApplicabilityReport::Declined;
            }
            // `checkTypeRelatedToAndOptionallyElaborate`: a fresh object
            // literal's excess property fails the relation itself and is its
            // report (TS2353, `isKnownProperty` in `isRelatedTo`). This port's
            // relater does not test excess properties, so the check runs
            // first.
            if literal {
                let before = self.diagnostics.len();
                self.check_excess_properties(target, at);
                if self.diagnostics.len() != before {
                    return ApplicabilityReport::Reported;
                }
            }
            if self.report_argument_failure(at, source, target) {
                return ApplicabilityReport::Reported;
            }
            if verdict == Some(Ternary::NotRelated) && !literal {
                return ApplicabilityReport::Declined;
            }
        }
        ApplicabilityReport::Applicable
    }

    /// Whether a written argument's published type (`source`) is the type
    /// `checkExpressionWithContextualType(arg, target)` gives it natively,
    /// asked once the pair fails; `false` declines the report.
    ///
    /// - A function or arrow is assigned its contextual parameter types once,
    ///   so its published type is native's under any later context.
    /// - Under the call's own declared signature
    ///   ([`CandidateContext::Declared`]) every argument was checked with
    ///   this parameter as contextual type, so its published type is
    ///   native's.
    /// - Under an instantiation, an object literal's published type is
    ///   native's unless a literal member could keep a literal type under the
    ///   target (`getWidenedLiteralLikeTypeForContextualType`) or the target
    ///   has a member this port cannot resolve; an array literal is native's
    ///   unless either side is tuple-like or could contain type variables
    ///   (tuple-ness follows the context), or an element could keep a
    ///   literal; a class expression (a re-check re-creates the class)
    ///   declines.
    ///
    /// A mapped type with an `as` clause is a published type this port
    /// computes by a road native does not take, and declines everywhere.
    fn report_argument_type_is_certified(
        &mut self,
        argument: Expression<'a>,
        source: TypeId,
        target: TypeId,
        context: CandidateContext,
    ) -> bool {
        if self.mapped_types.get(&source).is_some_and(|info| info.name_type.is_some()) {
            return false;
        }
        let inner = Self::effective_check_expression(argument);
        if matches!(inner, Expression::ArrowFunction(_) | Expression::FunctionExpression(_))
            || context == CandidateContext::Declared
        {
            return true;
        }
        match inner {
            // The object literal's published type was checked under the
            // uninstantiated parameter; its property names and the types of
            // its non-literal members do not follow the contextual type, so
            // only a member that could keep a literal type under the
            // instantiated target declines. Type variables on either side
            // are the enclosing declaration's, which inference left alone
            // (indexedAccessRelation's `this.setState({ a: a })` against
            // `Pick<S & State<T>, "a">`).
            Expression::ObjectLiteralExpression(_) => {
                !(self.literal_member_may_keep_literal(inner, target, 3)
                    || self.absent_member_is_unreadable(source, target))
            }
            Expression::ArrayLiteralExpression(_) => {
                !(self.is_tuple_like_type(source)
                    || self.is_tuple_like_type(target)
                    || self.could_contain_type_variables_at_head(source, 3)
                    || self.could_contain_type_variables_at_head(target, 3)
                    || self.literal_member_may_keep_literal(inner, target, 3))
            }
            Expression::ClassExpression(_) => false,
            _ => !self.is_context_sensitive_argument(&argument),
        }
    }

    /// `getEffectiveCheckNode` (`checker.go:9381`): the argument without its
    /// parentheses and `satisfies` wrappers.
    pub(crate) fn effective_check_expression(argument: Expression<'a>) -> Expression<'a> {
        let mut argument = argument;
        loop {
            argument = match argument {
                Expression::ParenthesizedExpression(inner) => match inner.expression {
                    Some(expression) => expression,
                    None => return argument,
                },
                Expression::SatisfiesExpression(inner) => match inner.expression {
                    Some(expression) => expression,
                    None => return argument,
                },
                _ => return argument,
            };
        }
    }

    /// `isSignatureApplicable`'s `this`-argument arm (`checker.go:9260`): a
    /// signature whose `this` type (`getThisTypeOfSignature`) is present and
    /// not `void` applies only when the call's `this` argument
    /// (`getThisArgumentOfCall`/`getThisArgumentType`, `checker.go:9345`;
    /// `void` for a bare call) is related to it. A `new` call and a call of
    /// a `super` property skip the arm. With `report`, a failure is
    /// `checkTypeRelatedToEx` at the `this` argument node (the call node
    /// when there is none) under
    /// `The_this_context_of_type_0_is_not_assignable_to_method_s_this_of_type_1`
    /// (TS2684); no elaboration. `Unknown` when the relation is undecided:
    /// the caller declines, as for an undecided argument.
    pub(crate) fn report_this_argument(
        &mut self,
        node: NodeId,
        signature: &Signature,
        report: bool,
    ) -> Ternary {
        let Some(this_type) =
            signature.this_parameter.as_ref().map(|parameter| self.parameter_type(parameter))
        else {
            return Ternary::Related;
        };
        if this_type == self.intrinsics.void {
            return Ternary::Related;
        }
        match self.node_map.get(node) {
            Some(tsr_ast::Node::CallExpression(call)) => {
                let super_property =
                    call.expression.and_then(|callee| callee.node_id()).is_some_and(|callee| {
                        match self.node_map.get(callee) {
                            Some(tsr_ast::Node::PropertyAccessExpression(access)) => {
                                access.expression
                            }
                            Some(tsr_ast::Node::ElementAccessExpression(access)) => {
                                access.expression
                            }
                            _ => None,
                        }
                        .and_then(|receiver| receiver.node_id())
                        .is_some_and(|receiver| {
                            self.nodes.kind(receiver) == SyntaxKind::SuperKeyword
                        })
                    });
                if super_property {
                    return Ternary::Related;
                }
            }
            Some(tsr_ast::Node::NewExpression(_)) => return Ternary::Related,
            _ => {}
        }
        let source = self.this_argument_type_of_call(Some(node));
        let verdict = self.relate_ternary(source, this_type, Relation::Assignable);
        if verdict == Ternary::NotRelated && report {
            let at = self
                .this_argument_of_call(node)
                .and_then(|(receiver, _)| receiver.node_id())
                .unwrap_or(node);
            let span = self.error_span(at);
            self.report_relation_failure(
                at,
                span,
                None,
                source,
                this_type,
                Some(&messages::THE_THIS_CONTEXT_OF_TYPE_0_IS_NOT_ASSIGNABLE_TO_METHOD_S_THIS_OF_TYPE_1),
            );
        }
        verdict
    }

    /// Argument shapes whose type this port computes without a mechanism
    /// upstream applies, so a failed relation is not upstream's answer:
    ///
    /// - `a ?? b`, `a || b` and `c ? a : b` union their operands with
    ///   `UnionReductionSubtype`, which this port does not have;
    /// - an identifier naming an auto-typed `let x = []` array, whose
    ///   evolved element types upstream regularizes
    ///   (`getRegularTypeOfObjectLiteral` in `addEvolvingArrayElementType`)
    ///   and this port leaves fresh.
    pub(crate) fn report_argument_type_is_not_upstreams(&self, argument: Expression<'_>) -> bool {
        let mut argument = argument;
        while let Expression::ParenthesizedExpression(inner) = argument {
            let Some(expression) = inner.expression else { return true };
            argument = expression;
        }
        match argument {
            Expression::ConditionalExpression(_) => true,
            Expression::BinaryExpression(binary) => binary.operator_token.is_some_and(|token| {
                matches!(token.kind, SyntaxKind::QuestionQuestionToken | SyntaxKind::BarBarToken)
            }),
            Expression::Identifier(identifier) => {
                let Some(id) = identifier.node_id else { return false };
                let Some(symbol) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    id,
                    identifier.text,
                    SymbolFlags::VALUE,
                ) else {
                    return false;
                };
                let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
                    return false;
                };
                matches!(self.node_map.get(declaration),
                    Some(tsr_ast::Node::VariableDeclaration(variable))
                        if variable.r#type.is_none()
                            && matches!(variable.initializer,
                                Some(Expression::ArrayLiteralExpression(array)) if array.elements.is_empty()))
            }
            _ => false,
        }
    }
    /// The type-variable half of `couldContainTypeVariables` (`checker.go`),
    /// bounded to `depth` levels of reference arguments and signature
    /// parameters/returns. Moved here from `calls.rs` (its callers there and
    /// the reporting pass share it).
    ///
    /// A refusal, not an upstream branch: upstream reports a signature-less
    /// generic callee like any other, but this port's generic machinery
    /// (non-nullable filtering of a deferred conditional, homomorphic mapped
    /// apparent types) answers a type upstream does not hold for several such
    /// callees, and an empty list read off one of those is not "not callable".
    /// The overload reporters decline a negative relation over such a source
    /// or target for the same reason.
    pub(crate) fn could_contain_type_variables_at_head(&mut self, t: TypeId, depth: u8) -> bool {
        let ty = self.store.get(t);
        if ty.flags.intersects(TypeFlags::INSTANTIABLE) {
            return true;
        }
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } = &ty.data {
            let types = types.clone();
            return types
                .into_iter()
                .any(|member| self.could_contain_type_variables_at_head(member, depth));
        }
        if depth == 0 {
            return false;
        }
        if let Some((_, arguments)) = self.type_reference_targets.get(&t).cloned()
            && arguments
                .into_iter()
                .any(|argument| self.could_contain_type_variables_at_head(argument, depth - 1))
        {
            return true;
        }
        if let Some(signatures) = self.signature_types.get(&t).cloned() {
            return signatures.iter().any(|signature| {
                !signature.type_parameters.is_empty()
                    || signature.parameters.iter().any(|parameter| {
                        let parameter_type = self.parameter_type(parameter);
                        self.could_contain_type_variables_at_head(parameter_type, depth - 1)
                    })
                    || self.could_contain_type_variables_at_head(signature.r#type, depth - 1)
            });
        }
        false
    }

    /// Whether `source` lacks a member of `target` whose symbol this port
    /// cannot resolve although it types it (a member inherited through an
    /// instantiated generic base, `interface Q extends P<string>`). The
    /// relater reads such a member's optionality from its symbol
    /// (`property_flags`), so its rejection of the absent member is not
    /// upstream's `propertiesRelatedTo` verdict and is not reported.
    pub(crate) fn absent_member_is_unreadable(&mut self, source: TypeId, target: TypeId) -> bool {
        let Some(names) = self.get_property_names_of_type(target) else { return false };
        names.iter().any(|name| {
            self.get_type_of_property_of_type(source, name).is_none()
                && self.get_property_of_type(target, name).is_none()
        })
    }

    /// Whether a member of an object or array literal written as a fresh
    /// literal (a string, numeric, bigint or template literal, `true`,
    /// `false`, a negated number) meets a target member type under which
    /// `getWidenedLiteralLikeTypeForContextualType` (`checker.go`) keeps the
    /// literal (`isLiteralOfContextualType`: a contextual literal of the same
    /// kind, a string context for a string literal including template
    /// literals, string mappings and `keyof`, or a type variable). The
    /// published type widened that member under the context the literal was
    /// checked in while the candidate was inferred; under the instantiated
    /// target it may not be, so the published type is not native's. Nested
    /// object and array literals are followed `depth` levels; a computed
    /// member name answers `true`.
    fn literal_member_may_keep_literal(
        &mut self,
        literal: Expression<'a>,
        target: TypeId,
        depth: u32,
    ) -> bool {
        let members: Vec<(Option<TypeId>, Expression<'a>)> = match literal {
            Expression::ObjectLiteralExpression(object) => {
                let mut members = Vec::new();
                for property in object.properties {
                    let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) =
                        property
                    else {
                        continue;
                    };
                    let Some(initializer) = assignment.initializer else { continue };
                    let name = match &assignment.name {
                        tsr_ast::PropertyName::Identifier(identifier) => Some(identifier.text),
                        tsr_ast::PropertyName::StringLiteral(literal) => Some(literal.text),
                        tsr_ast::PropertyName::NumericLiteral(literal) => Some(literal.text),
                        _ => None,
                    };
                    let Some(name) = name else { return true };
                    members.push((self.get_type_of_property_of_type(target, name), initializer));
                }
                members
            }
            Expression::ArrayLiteralExpression(array) => {
                let element = self.signature_array_element(target);
                array
                    .elements
                    .iter()
                    .map(|&element_expression| (element, element_expression))
                    .collect()
            }
            _ => return false,
        };
        for (member, initializer) in members {
            let Some(member) = member else { continue };
            let inner = Self::effective_check_expression(initializer);
            let kind = match inner {
                Expression::StringLiteral(_)
                | Expression::NoSubstitutionTemplateLiteral(_)
                | Expression::TemplateExpression(_) => {
                    TypeFlags::STRING_LITERAL
                        | TypeFlags::INDEX
                        | TypeFlags::TEMPLATE_LITERAL
                        | TypeFlags::STRING_MAPPING
                }
                Expression::NumericLiteral(_) => TypeFlags::NUMBER_LITERAL,
                Expression::BigIntLiteral(_) => TypeFlags::BIG_INT_LITERAL,
                Expression::KeywordExpression(keyword)
                    if matches!(
                        keyword.kind,
                        SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword
                    ) =>
                {
                    TypeFlags::BOOLEAN_LITERAL
                }
                Expression::PrefixUnaryExpression(unary)
                    if unary.operator.kind == SyntaxKind::MinusToken =>
                {
                    match unary.operand {
                        Some(Expression::NumericLiteral(_)) => TypeFlags::NUMBER_LITERAL,
                        Some(Expression::BigIntLiteral(_)) => TypeFlags::BIG_INT_LITERAL,
                        _ => continue,
                    }
                }
                Expression::ObjectLiteralExpression(_) | Expression::ArrayLiteralExpression(_) => {
                    if depth > 0 && self.literal_member_may_keep_literal(inner, member, depth - 1) {
                        return true;
                    }
                    continue;
                }
                _ => continue,
            };
            if self.contextual_type_keeps_literal(member, kind) {
                return true;
            }
        }
        false
    }

    /// `isLiteralOfContextualType`'s test of a contextual type against a
    /// literal of the given kind (`kind`, the contextual flags that keep it):
    /// a union or intersection keeps it when a constituent does; a type
    /// variable keeps it (its constraint is not consulted here, which can
    /// only decline more).
    fn contextual_type_keeps_literal(&mut self, contextual: TypeId, kind: TypeFlags) -> bool {
        let ty = self.store.get(contextual);
        if ty.flags.intersects(kind) {
            return true;
        }
        if ty.flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE) {
            return true;
        }
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } = &ty.data {
            let types = types.clone();
            return types.into_iter().any(|part| self.contextual_type_keeps_literal(part, kind));
        }
        false
    }
}
