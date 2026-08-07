//! `checkExpression` and the expression forms that do not have a module of
//! their own.
//!
//! Ported from `internal/checker/checker.go`. The binary operators live in
//! [`crate::binary`] and property access in [`crate::members`], because those
//! two are large enough to be worked on independently; everything else that
//! answers "what is the type of this expression" is here.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;

use crate::{
    checker::Checker,
    flags::TypeFlags,
    printing,
    types::{TypeData, TypeId},
};

/// `AssignmentKind` (`internal/checker/utilities.go`): how a reference is
/// written, which decides whether `checkIdentifier` narrows it at all.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssignmentTargetKind {
    None,
    Definite,
    Compound,
}

impl Checker<'_, '_> {
    /// The type of an expression.
    ///
    /// Ported from `Checker.checkExpression` (`internal/checker/checker.go`),
    /// restricted to the forms this slice covers. Anything else yields
    /// `errorType` — **not** `anyType`: `errorType` is upstream's marker for "could
    /// not be computed", and using `any` would claim a real answer.
    pub fn check_expression(&mut self, expression: Expression<'_>) -> TypeId {
        let node = Node::from(expression);
        if let Some(id) = node.node_id()
            && let Some(&cached) = self.node_types.get(&id)
        {
            return cached;
        }

        self.computations += 1;
        let computed = self.check_expression_worker(expression);

        // Never persist a type computed during loop fixpoint analysis: the
        // walk hands out transient so-far unions, and an entry stamped from
        // one would outlive convergence (`foo(x)` resolved against a
        // provisional `string` stays `number` forever). Upstream's
        // formulation clears `flowLoopStack` before any computation it will
        // cache — `Checker.checkExpressionCachedEx`
        // (`internal/checker/checker.go:7517`); suppressing the write while
        // the stack is non-empty is the dual for this port's single cache.
        // See docs/architecture/checker-notes-narrow.md §12.6.
        if self.flow_loop_stack.is_empty()
            && let Some(id) = node.node_id()
        {
            self.node_types.insert(id, computed);
        }
        computed
    }

    /// `checkTemplateExpression` (`checker.go:7976`) — see the dispatch
    /// arm's comment and `checker-notes-narrow.md` §24 for the decline set.
    fn check_template_expression(&mut self, node: &tsr_ast::TemplateExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let mut span_types = Vec::with_capacity(node.template_spans.len());
        for span in node.template_spans {
            let Some(expression) = span.expression else { return error };
            span_types.push(self.check_expression(expression));
        }
        if span_types.contains(&error) {
            return error;
        }
        // The fold: every span a string/number literal whose stored data IS
        // the evaluated text. A part whose SOURCE is longer than its cooked
        // text carries escape sequences — the scanner's legacy-octal cooking
        // diverges from upstream's there (`octalLiteralAndEscapeSequence`,
        // the §24 measurement's second family), so escaped templates decline
        // to gaps rather than fold wrongly.
        let escaped = |id: Option<tsr_ast::NodeId>, text: &str, delimiters: usize| {
            id.is_some_and(|id| {
                let span = self.nodes.span(id);
                (span.end - span.start) as usize != text.len() + delimiters
            })
        };
        let head_escaped = node.head.is_some_and(|head| escaped(head.node_id, head.text, 3));
        let any_part_escaped = head_escaped
            || node.template_spans.iter().any(|span| match span.literal {
                Some(tsr_ast::TemplateMiddleOrTail::TemplateMiddle(part)) => {
                    escaped(part.node_id, part.text, 3)
                }
                Some(tsr_ast::TemplateMiddleOrTail::TemplateTail(part)) => {
                    escaped(part.node_id, part.text, 2)
                }
                None => true,
            });
        let mut folded: Option<String> =
            if any_part_escaped { None } else { node.head.map(|head| head.text.to_string()) };
        for (span, &span_type) in node.template_spans.iter().zip(&span_types) {
            let Some(previous) = folded else { break };
            let piece = match &self.store.get(span_type).data {
                crate::types::TypeData::StringLiteral(text)
                | crate::types::TypeData::NumberLiteral(text) => Some(text.clone()),
                _ => None,
            };
            folded = match (piece, span.literal) {
                (Some(piece), Some(literal)) => {
                    let tail = match literal {
                        tsr_ast::TemplateMiddleOrTail::TemplateMiddle(part) => part.text,
                        tsr_ast::TemplateMiddleOrTail::TemplateTail(part) => part.text,
                    };
                    Some(previous + &piece + tail)
                }
                _ => None,
            };
        }
        if let Some(value) = folded {
            return self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(value),
                true,
            );
        }
        // The three §24 declines: a const context, the element-access
        // argument position (a template-literal context), and any span whose
        // literal kind the fold cannot evaluate is NOT declined — only the
        // CONTEXT questions are, because they change the ANSWER's shape.
        if let Some(id) = node.node_id {
            let mut current = self.nodes.parent(id);
            while let Some(parent) = current {
                match self.nodes.kind(parent) {
                    SyntaxKind::AsExpression
                    | SyntaxKind::TypeAssertionExpression
                    | SyntaxKind::ElementAccessExpression => {
                        return error;
                    }
                    SyntaxKind::ParenthesizedExpression => {
                        current = self.nodes.parent(parent);
                    }
                    _ => break,
                }
            }
        }
        self.intrinsics.string
    }

    fn check_expression_worker(&mut self, expression: Expression<'_>) -> TypeId {
        match expression {
            // Literal *expressions* produce **fresh** literal types, which is
            // what lets `let x = "a"` widen to `string` while `let x: "a"`
            // does not — a literal type node produces the regular form. See
            // `Checker::get_widened_literal_type`.
            Expression::StringLiteral(node) => self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(node.text.to_string()),
                true,
            ),
            Expression::NoSubstitutionTemplateLiteral(node) => self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(node.text.to_string()),
                true,
            ),
            Expression::NumericLiteral(node) => self.store.intern_literal(
                TypeFlags::NUMBER_LITERAL,
                TypeData::NumberLiteral(printing::normalise_number(node.text)),
                true,
            ),
            Expression::BigIntLiteral(node) => self.store.intern_literal(
                TypeFlags::BIG_INT_LITERAL,
                // A bigint literal's text carries its trailing `n`; the type's
                // payload is the digits, and `type_to_string` puts the `n` back.
                TypeData::BigIntLiteral(node.text.trim_end_matches('n').to_string()),
                true,
            ),
            Expression::KeywordExpression(node) => match node.kind {
                SyntaxKind::ThisKeyword => {
                    node.node_id.map_or(self.intrinsics.error, |id| self.check_this_expression(id))
                }
                // `checkSuperExpression` (`checker.go:7854`).
                SyntaxKind::SuperKeyword => {
                    node.node_id.map_or(self.intrinsics.error, |id| self.check_super_expression(id))
                }
                SyntaxKind::TrueKeyword => self.intrinsics.true_type,
                SyntaxKind::FalseKeyword => self.intrinsics.false_type,
                SyntaxKind::NullKeyword => self.intrinsics.null,
                // `undefined` is an identifier rather than a keyword in the
                // grammar, so it does not arrive here.
                _ => self.intrinsics.error,
            },
            // Ported from `Checker.checkIdentifier`: resolve the name, take the
            // symbol's type, and narrow it by the control flow reaching here.
            //
            // **Narrowing is partial**, and [`crate::flow`] says exactly which
            // guards are ported. The property that makes that safe is upstream's
            // own: `narrowType`'s default arm returns the type unchanged, so an
            // unported guard leaves the declared type rather than producing a
            // wrong one.
            //
            // Note that `let x = "a"` is `string` here *and* upstream —
            // `getTypeAtFlowAssignment` reduces only when the declared type is a
            // union, so that is not a narrowing gap however much it looks like
            // one (`bd tsr-4sc.11`).
            Expression::Identifier(node) => {
                let Some(id) = node.node_id else { return self.intrinsics.error };
                // `SymbolFlags::VALUE` is upstream's meaning for an identifier
                // expression (`checkIdentifier` -> `getResolvedSymbol`). It is
                // what keeps an enclosing class's type parameter from being
                // resolved here — see `BindResult::resolve_name`.
                match self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    id,
                    node.text,
                    SymbolFlags::VALUE,
                ) {
                    Some(symbol) => {
                        let declared = self.get_type_of_symbol(symbol);
                        // `getNarrowedTypeOfSymbol` (`checker.go`): only a
                        // variable or parameter reference is narrowed. A class,
                        // interface, enum or function reference is not, and
                        // narrowing one anyway would answer a question upstream
                        // does not ask.
                        if self.is_narrowable_symbol(symbol) {
                            let node_id = node.node_id.expect("checked above");
                            match self.assignment_target_kind(node_id) {
                                // `checker.go:11109`: a variable in a definite
                                // assignment-target position is returned at its
                                // DECLARED type — no flow analysis. This is what
                                // makes `x` in `x = foo(x)` print the full
                                // declared union, and an auto-typed target print
                                // `any` (`controlFlowSelfReferentialLoop.types:160`;
                                // `checker-notes-narrow.md` §12.7). A
                                // compound-like assignment (`x = x + 1`) reads at
                                // the literal's base, exactly as `x += 1` would.
                                AssignmentTargetKind::Definite => {
                                    if self.is_in_compound_like_assignment(node_id) {
                                        self.get_base_type_of_literal_type(declared)
                                    } else {
                                        declared
                                    }
                                }
                                // `checker.go:11196`: the TARGET of a compound
                                // assignment reads at the literal's base — `x |= …`
                                // sees `boolean`, not the narrowed `true`
                                // (`bitwiseCompoundAssignmentOperators.types`;
                                // `checker-notes-narrow.md` §10).
                                AssignmentTargetKind::Compound => {
                                    let flowed = self.get_flow_type_of_reference(
                                        node_id,
                                        Some(symbol),
                                        declared,
                                    );
                                    self.get_base_type_of_literal_type(flowed)
                                }
                                AssignmentTargetKind::None => {
                                    self.get_flow_type_of_reference(node_id, Some(symbol), declared)
                                }
                            }
                        } else {
                            declared
                        }
                    }
                    None => self.intrinsics.error,
                }
            }
            Expression::ParenthesizedExpression(node) => {
                node.expression.map_or(self.intrinsics.error, |inner| self.check_expression(inner))
            }
            // `checkTemplateExpression` (`checker.go:7976`): spans check;
            // an all-literal template folds to the fresh string literal
            // (the evaluator's observable for string/number parts); else
            // `string` — with the const-context, element-access-argument,
            // and unfoldable-literal shapes declined to gaps
            // (`checker-notes-narrow.md` §24).
            Expression::TemplateExpression(node) => self.check_template_expression(node),
            Expression::BinaryExpression(node) => self.check_binary_expression(node),
            Expression::PropertyAccessExpression(node) => {
                self.check_property_access_expression(node)
            }
            Expression::CallExpression(node) => self.check_call_expression(node),
            Expression::ElementAccessExpression(node) => self.check_element_access_expression(node),
            // `checkObjectLiteral` (`checker.go:13144`). Distinct from the
            // `{ a: string }` *type* node in `crate::declared`, which prints
            // identically and is computed by unrelated code — see
            // [`crate::objects`].
            Expression::ObjectLiteralExpression(node) => self.check_object_literal(node),
            // `checkArrayLiteral` (`checker.go:8021`). The element union meets
            // the array type, both of which already existed — see
            // [`crate::array_literals`].
            Expression::ArrayLiteralExpression(node) => self.check_array_literal(node),
            // `checkAssertion` (`checker.go:12287`). Both spellings of the same
            // construct, and `const` is recognised before the type node is
            // resolved — see [`crate::assertions`].
            Expression::AsExpression(node) => {
                node.node_id.map_or(self.intrinsics.error, |id| self.check_assertion(id))
            }
            Expression::TypeAssertion(node) => {
                node.node_id.map_or(self.intrinsics.error, |id| self.check_assertion(id))
            }
            // `checkFunctionExpressionOrObjectLiteralMethod` (`checker.go:9077`).
            // Both kinds answer through the function's own symbol, which is the
            // same arm `getTypeOfFuncClassEnumModule` serves — see
            // [`Checker::get_type_of_function_expression`] for the one place that
            // is not safe, an unannotated parameter under a contextual type.
            Expression::FunctionExpression(node) => node
                .node_id
                .map_or(self.intrinsics.error, |id| self.get_type_of_function_expression(id)),
            Expression::ArrowFunction(node) => node
                .node_id
                .map_or(self.intrinsics.error, |id| self.get_type_of_function_expression(id)),
            // `checkTypeOfExpression` (`checker.go:10617`). The operand's type is
            // irrelevant to the answer — see below.
            Expression::TypeOfExpression(node) => self.check_type_of_expression(node),
            // `checkVoidExpression` (`checker.go:10633`): the operand checks
            // for its own lines; the expression is `undefined`.
            Expression::VoidExpression(node) => {
                let Some(operand) = node.expression else { return self.intrinsics.error };
                let checked = self.check_expression(operand);
                if checked == self.intrinsics.error {
                    return self.intrinsics.error;
                }
                self.intrinsics.undefined
            }
            // `checkDeleteExpression` (`checker.go:10570`): the operand
            // checks; the expression is `boolean`.
            Expression::DeleteExpression(node) => {
                let Some(operand) = node.expression else { return self.intrinsics.error };
                let checked = self.check_expression(operand);
                if checked == self.intrinsics.error {
                    return self.intrinsics.error;
                }
                self.intrinsics.boolean
            }
            // `checkPrefixUnaryExpression` (`checker.go:10855`).
            Expression::PrefixUnaryExpression(node) => self.check_prefix_unary_expression(node),
            // `checkNonNullAssertion` (`checker.go:12266`): the operand's
            // non-nullable remainder (`checker-notes-narrow.md` §26).
            Expression::NonNullExpression(node) => {
                let Some(operand) = node.expression else { return self.intrinsics.error };
                let checked = self.check_expression(operand);
                if checked == self.intrinsics.error {
                    return self.intrinsics.error;
                }
                self.get_non_nullable_type(checked)
            }
            // `checkPostfixUnaryExpression` (`checker.go:10909`).
            Expression::PostfixUnaryExpression(node) => self.check_postfix_unary_expression(node),
            // `checkConditionalExpression` (`checker.go:10934`).
            Expression::ConditionalExpression(node) => self.check_conditional_expression(node),
            // `resolveNewExpression` (`checker.go:8575`).
            Expression::NewExpression(node) => self.check_new_expression(node),
            // `checkYieldExpression` (`checker.go:10952`).
            Expression::YieldExpression(node) => self.check_yield_expression(node),
            // `checkAwaitExpression` (`checker.go:10845`).
            Expression::AwaitExpression(node) => self.check_await_expression(node),
            // `checkTaggedTemplateExpression` (`checker.go:10034`) — see
            // [`crate::calls`], which owns signature resolution.
            Expression::TaggedTemplateExpression(node) => {
                self.check_tagged_template_expression(node)
            }
            // `checkJsxElement` (`jsx.go:72`) → `getJsxElementTypeAt`
            // (`jsx.go:1275`): an element or fragment expression has the
            // declared type of the in-scope `JSX` namespace's `Element`
            // export. Sized at 759 forecast lines with the bar in
            // `checker-notes-jsx.md`; when `JSX` or `Element` is absent the
            // arm gaps — that refusal is the bar's registered falsifier.
            Expression::JsxElement(node) => self.check_jsx_element(node.node_id),
            Expression::JsxSelfClosingElement(node) => self.check_jsx_element(node.node_id),
            Expression::JsxFragment(node) => self.check_jsx_element(node.node_id),
            _ => self.intrinsics.error,
        }
    }

    /// `getJsxType(JsxNames.Element, location)` (`jsx.go:1275`, `:1295`),
    /// reduced to the resolving path: the `JSX` namespace in scope at the
    /// element, its `Element` export, that symbol's declared type. Every
    /// missing hop is a gap — `errorType` — never a substitute.
    fn check_jsx_element(&mut self, id: Option<tsr_ast::NodeId>) -> TypeId {
        let Some(id) = id else { return self.intrinsics.error };
        let Some(jsx) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            id,
            "JSX",
            tsr_binder::SymbolFlags::NAMESPACE,
        ) else {
            return self.intrinsics.error;
        };
        let jsx = self.binder.merged_symbol(jsx);
        let Some(&element) = self.binder.symbols().get(jsx).exports.get("Element") else {
            return self.intrinsics.error;
        };
        self.get_declared_type_of_symbol(element)
    }

    /// Ported from `Checker.checkTypeOfExpression` (`checker.go:10617`).
    ///
    /// # The operand's type does not reach the answer
    ///
    /// Upstream is two lines: check the operand, then return `typeofType`
    /// regardless of what came back. So `typeof` is the one expression form here
    /// whose result is **not** weakened by a gap in its operand —
    /// `typeof someUnportedThing` is still the full union, and answering
    /// `errorType` because the operand gapped would invent a gap upstream does not
    /// have. The operand is still checked, because that is what populates the
    /// operand's own line in the output.
    ///
    /// `typeofType` is built at `checker.go:1052` as the union of the sorted keys
    /// of `typeofNEFacts`, so the constituent order is alphabetical rather than
    /// the order a human would list them. 265 baseline lines record it exactly:
    ///
    /// ```text
    /// >typeof x : "bigint" | "boolean" | "function" | "number" | "object" | "string" | "symbol" | "undefined"
    /// ```
    fn check_type_of_expression(&mut self, node: &tsr_ast::TypeOfExpression<'_>) -> TypeId {
        if let Some(operand) = node.expression {
            self.check_expression(operand);
        }
        self.get_typeof_type()
    }

    /// `typeofType` (`checker.go:1052`).
    ///
    /// Rebuilt per call rather than cached on the checker: both the string
    /// literal types and the union are interned, so this is a lookup after the
    /// first call and needs no field on `Checker`.
    fn get_typeof_type(&mut self) -> TypeId {
        // `slices.Sorted(maps.Keys(typeofNEFacts))` — alphabetical, and the
        // union's constituent order follows the order the types are created in,
        // so this list must stay sorted.
        const NAMES: [&str; 8] =
            ["bigint", "boolean", "function", "number", "object", "string", "symbol", "undefined"];
        let types = NAMES
            .iter()
            .map(|name| {
                // `getStringLiteralType` yields the **regular** form; only a
                // string literal *expression* is fresh.
                self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    TypeData::StringLiteral((*name).to_string()),
                    false,
                )
            })
            .collect::<Vec<_>>();
        self.get_union_type(&types)
    }

    /// Ported from `Checker.checkPrefixUnaryExpression` (`checker.go:10855`).
    ///
    /// # A negated numeric literal is a literal, not `number`
    ///
    /// Upstream special-cases the operand being a numeric or bigint literal
    /// *before* it looks at the operator's general rule, so `-1` is the literal
    /// type `-1` and not `number`. The baselines record `>-1 : -1` and `>+1 : 1`,
    /// against `>-x : number` and `>-true : number` for every non-literal operand.
    /// Missing this case would be a plausible wrong line on every negative
    /// constant in the corpus.
    ///
    /// Not ported, each a gap: an operand this port cannot type (see
    /// [`Self::unary_result_type`] for why that is a gap rather than `number`),
    /// and `!` on an operand whose truthiness is not decidable here.
    fn check_prefix_unary_expression(
        &mut self,
        node: &tsr_ast::PrefixUnaryExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let Some(operand) = node.operand else { return error };
        let operand_type = self.check_expression(operand);
        let operator = node.operator.kind;

        // The literal special cases, which run before the operator's general
        // rule. `getFreshTypeOfLiteralType` — a unary expression is an
        // expression, so the literal it produces is fresh, exactly like the
        // `NumericLiteral` arm above.
        if let Expression::NumericLiteral(literal) = operand
            && matches!(operator, SyntaxKind::MinusToken | SyntaxKind::PlusToken)
        {
            let normalised = printing::normalise_number(literal.text);
            let text = if operator == SyntaxKind::MinusToken {
                negate_number_text(&normalised)
            } else {
                Some(normalised)
            };
            // An unparseable literal keeps `normalise_number`'s fallback of the
            // source text, which cannot be negated meaningfully; the scanner has
            // already reported it, so this is a gap rather than a guess.
            if let Some(text) = text {
                return self.store.intern_literal(
                    TypeFlags::NUMBER_LITERAL,
                    TypeData::NumberLiteral(text),
                    true,
                );
            }
            return error;
        }
        // The bigint half of the same special case. `-1n` is the literal `-1n`;
        // `+1n` is not a case at all, because unary `+` on a bigint is an error
        // upstream rather than a literal.
        if let Expression::BigIntLiteral(literal) = operand
            && operator == SyntaxKind::MinusToken
        {
            let digits = literal.text.trim_end_matches('n');
            return self.store.intern_literal(
                TypeFlags::BIG_INT_LITERAL,
                TypeData::BigIntLiteral(format!("-{digits}")),
                true,
            );
        }

        match operator {
            // `+` returns `numberType` unconditionally — upstream reports on a
            // bigint operand but still answers `number`, so there is no bigint
            // gap on this arm.
            SyntaxKind::PlusToken => self.intrinsics.number,
            SyntaxKind::MinusToken | SyntaxKind::TildeToken => self.unary_result_type(operand_type),
            SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken => {
                self.unary_result_type(operand_type)
            }
            SyntaxKind::ExclamationToken => self.negated_truthiness_type(operand_type),
            _ => error,
        }
    }

    /// Ported from `Checker.checkPostfixUnaryExpression` (`checker.go:10909`).
    ///
    /// `x++` and `x--` have no literal special case — upstream goes straight to
    /// `getUnaryResultType`, which is why `>i++ : number` even when `i` is the
    /// literal type `0`.
    fn check_postfix_unary_expression(
        &mut self,
        node: &tsr_ast::PostfixUnaryExpression<'_>,
    ) -> TypeId {
        let Some(operand) = node.operand else { return self.intrinsics.error };
        let operand_type = self.check_expression(operand);
        self.unary_result_type(operand_type)
    }

    /// Ported from `Checker.getUnaryResultType` (`checker.go:10923`).
    ///
    /// # The bigint arm is a gap, and the operand's type is why this can fail
    ///
    /// Upstream answers `number` for everything that is not bigint-like, and
    /// `bigint` or `number | bigint` when it is. The bigint arm needs
    /// `numberOrBigIntType` and `isTypeAssignableToKind` — an assignability
    /// question this port cannot ask — so a bigint-like operand is a gap.
    ///
    /// That is also why an operand this port could not type is a gap rather than
    /// `number`: the *only* thing the answer depends on is whether the operand is
    /// bigint-like, and an `errorType` operand is precisely the case where that
    /// is unknown. Answering `number` there would be right for most of the corpus
    /// and wrong for every bigint, which is the guess this discipline exists to
    /// prevent.
    fn unary_result_type(&mut self, operand: TypeId) -> TypeId {
        let flags = self.store.get(operand).flags;
        if operand == self.intrinsics.error || flags.intersects(TypeFlags::BIG_INT_LIKE) {
            return self.intrinsics.error;
        }
        // `any`/`unknown` answer `number`: `maybeTypeOfKind` is a FLAG test
        // (`checker.go:10923` reads `operandType`'s flags against
        // `TypeFlagsBigIntLike`), and neither carries the bigint bit, so
        // upstream falls straight to `numberType` —
        // `bitwiseNotOperatorWithAnyOtherType.types` records `~ANY1 : number`
        // throughout. The comment this replaces claimed `maybeTypeOfKind`
        // "answers yes for them" — an intuition falsified by its own anchor,
        // caught when the §9.4 initial-type change exposed 8 such lines.
        self.intrinsics.number
    }

    /// The `!` arm of `checkPrefixUnaryExpression` (`checker.go:10887`).
    ///
    /// Upstream calls `getTypeFacts(operandType, TypeFactsTruthy|TypeFactsFalsy)`
    /// and answers `false` when the operand can only be truthy, `true` when it
    /// can only be falsy, and `boolean` when it could be either. `>!x : boolean`
    /// is the common baseline line, but `!` on a literal is not `boolean` and
    /// answering `boolean` everywhere would be a wrong line on each one.
    ///
    /// # Only the decidable half of `getTypeFacts` is ported
    ///
    /// A unit type has one truthiness and the primitives have both, which is
    /// enough for the corpus shapes. Everything else — unions, objects,
    /// intersections, type parameters, `never` — is a gap. An object type is
    /// *always* truthy upstream and so would answer `false`, but that holds only
    /// once `TypeFacts` distinguishes an object from a possibly-`undefined` one,
    /// and guessing it here would be a wrong line on every optional value.
    fn negated_truthiness_type(&mut self, operand: TypeId) -> TypeId {
        if operand == self.intrinsics.error {
            return self.intrinsics.error;
        }
        let (flags, data) = {
            let t = self.store.get(operand);
            (t.flags, t.data.clone())
        };
        // Always falsy: `!null`, `!undefined`, `!void` are all `true`.
        if flags.intersects(TypeFlags::NULLABLE | TypeFlags::VOID) {
            return self.intrinsics.true_type;
        }
        let falsy = match data {
            TypeData::BooleanLiteral(value) => !value,
            TypeData::StringLiteral(text) => text.is_empty(),
            // The normalised text, so `0`, `0.0` and `0x0` all arrive as `"0"`,
            // and `0n` likewise for the bigint half.
            TypeData::NumberLiteral(text) | TypeData::BigIntLiteral(text) => text == "0",
            _ => {
                // Both truthiness values are possible for the unit-less
                // primitives, which is upstream's `Truthy|Falsy` and prints
                // `boolean`.
                return if flags.intersects(
                    TypeFlags::STRING
                        | TypeFlags::NUMBER
                        | TypeFlags::BIG_INT
                        | TypeFlags::BOOLEAN
                        | TypeFlags::ANY_OR_UNKNOWN,
                ) {
                    self.intrinsics.boolean
                } else {
                    self.intrinsics.error
                };
            }
        };
        if falsy { self.intrinsics.true_type } else { self.intrinsics.false_type }
    }

    /// Ported from `Checker.checkThisExpression` (`checker.go:12077`), reduced to
    /// the class case.
    ///
    /// **`this` inside a class is the class's `this` *type*, printed `this`** —
    /// not the class type printed `C`. Upstream models it as a type parameter
    /// whose constraint is the class (`checker.go:17334`), and the corpus records
    /// it that way: `>this : this`. Its members are the class's, which is what
    /// makes `this.x` work.
    ///
    /// Arrow functions are transparent to `this` and a plain `function` is not,
    /// which is the only part of upstream's container walk that changes an
    /// answer here. Every other container — a plain function, a module, the top
    /// level — is a gap: upstream answers `anyType` there through a signature's
    /// `this` parameter or a contextual type, and neither exists yet, so
    /// answering `any` would be a claim rather than a computation.
    fn check_this_expression(&mut self, node: NodeId) -> TypeId {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            // **Arm 1 shadows arm 2, and the order is upstream's.**
            // `tryGetThisTypeAtEx` (`checker.go:12146`) asks
            // `ast.IsFunctionLike(container)` *before* it asks
            // `ast.IsClassLike(container.Parent)`, so a **method** carrying a
            // `this` parameter answers the annotation and never reaches the
            // class's `thisType`. Testing class-ness first is the natural port
            // and it is backwards: it would print `this` where upstream prints
            // the written annotation. A method is function-like.
            //
            // Falling through when there is no `this` parameter is also
            // upstream's: `getThisTypeOfSignature` answers `nil`,
            // `getContextualThisParameterType` is unported and answers nothing
            // here, and the class arm below is what runs next.
            if let Some(this_type) = self.this_parameter_type(id) {
                return this_type;
            }
            match self.nodes.kind(id) {
                // An arrow function is transparent — it keeps the enclosing
                // `this` — which is the same as walking past any other node, so
                // it needs no arm of its own. It is named here because that
                // transparency is a rule and not an omission.
                //
                // Opaque: a plain function rebinds `this`, and what to is
                // upstream's signature machinery (`bd tsr-4sc.8`).
                SyntaxKind::FunctionDeclaration | SyntaxKind::FunctionExpression => {
                    return self.intrinsics.error;
                }
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression => {
                    let Some(symbol) = self.binder.symbol_of(id) else {
                        return self.intrinsics.error;
                    };
                    if let Some(&cached) = self.this_types.get(&symbol) {
                        return cached;
                    }
                    let this_type = self.store.new_named(
                        TypeFlags::TYPE_PARAMETER,
                        "this".to_string(),
                        Some(symbol),
                    );
                    self.this_types.insert(symbol, this_type);
                    return this_type;
                }
                _ => {}
            }
            current = self.nodes.parent(id);
        }
        self.intrinsics.error
    }

    /// The written annotation on a container's `this` parameter, if it has one.
    ///
    /// `ast.GetThisParameter` (`tryGetThisTypeAtEx`'s test, `checker.go:12146`)
    /// reads the container's **first** parameter and asks whether it is named
    /// `this`; the grammar allows it nowhere else.
    ///
    /// # An unannotated `this` parameter is deliberately not answered
    ///
    /// `function f(this) {}` would take its type from `getTypeOfSymbol`, which
    /// answers the implicit `any` — and `docs/architecture/checker-notes-rank.md`
    /// §6 records 21,685 gradient lines already banked on `any` with the computed
    /// and the defaulted not yet separated. Adding to that column for a parameter
    /// the source did not annotate is a claim, not a computation, so this returns
    /// `None` and the line stays a gap. Three such parameters exist in the corpus
    /// and `crates/tsr-conformance/examples/thisparam.rs` prints the count as a
    /// control on the exclusion.
    ///
    /// An **arrow function is not a `this` container** —
    /// `getThisContainer(node, includeArrowFunctions: false, …)`
    /// (`checker.go:12188`) — so it is absent from this match and stays
    /// transparent, which is the rule the caller's own comment already records.
    fn this_parameter_type(&mut self, container: NodeId) -> Option<TypeId> {
        // The map is a shared reference on the checker, so copying it out first
        // ends the borrow of `self` before `get_type_from_type_node` needs
        // `&mut self`.
        let map = self.node_map;
        let parameters = match map.get(container)? {
            Node::FunctionDeclaration(node) => node.parameters,
            Node::FunctionExpression(node) => node.parameters,
            Node::MethodDeclaration(node) => node.parameters,
            Node::GetAccessorDeclaration(node) => node.parameters,
            Node::SetAccessorDeclaration(node) => node.parameters,
            Node::ConstructorDeclaration(node) => node.parameters,
            _ => return None,
        };
        let first = parameters.first()?;
        if !matches!(first.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
        {
            return None;
        }
        let annotation = first.r#type?;
        // **An annotation this port cannot resolve falls through rather than
        // answering `errorType`.** `explicitThis(this: this, m: number)` in
        // `conformance/looseThisTypeInFunctions` is the case: the annotation is a
        // `ThisTypeNode`, which `getTypeFromTypeNode` has an arm for and this
        // port does not, and the class arm below answers `this` — **correctly**.
        // Returning the gap here instead turned 4 right lines into gaps and one
        // more in `compiler/unusedParametersThis`, measured, before this line
        // existed.
        //
        // Falling through is also upstream's own shape: `getThisTypeOfSignature`
        // answering `nil` is what sends `tryGetThisTypeAtEx` (`checker.go:12146`)
        // on to `ast.IsClassLike(container.Parent)`. "We could not read the
        // annotation" is this port's `nil`.
        let resolved = self.get_type_from_type_node(annotation);
        (resolved != self.intrinsics.error).then_some(resolved)
    }

    /// Ported from `Checker.checkSuperExpression` (`checker.go:7854`), reduced
    /// to the type it answers.
    ///
    /// # `super(...)` is the **static** side, and that is not about `static`
    ///
    /// Upstream's tail is `if ast.IsStatic(container) || isCallExpression`
    /// (`checker.go:7946`), where `isCallExpression` is *"this `super` is the
    /// callee of its own call"* (`:7855`). **A `super(...)` call answers the base
    /// **constructor** type — `typeof Base` — even inside an ordinary instance
    /// constructor**, because what it calls is the base constructor.
    ///
    /// This is measured, not reasoned. A first version of this arm split on
    /// `ast.IsStatic` alone, shipped nothing, and manufactured **257 wrong lines
    /// against 198 right** — of which ~151 were exactly this: `Base` where
    /// upstream prints `typeof Base`, `A` for `typeof A`, `C` for `typeof C`.
    /// The hypothesis at the time was that the container walk picked the wrong
    /// node; it did not. **The walk was right and the rule was wrong**, and only
    /// reading upstream's tail said so — the data structure had nothing to
    /// confess. See `docs/architecture/checker-notes-this.md` and `bd tsr-h1s`.
    ///
    /// # What is deliberately not answered
    ///
    /// - **An object-literal container.** Upstream assumes `any` there
    ///   (`checker.go:7917`), and `checker-notes-rank.md` §6 forbids banking on
    ///   `any`.
    /// - **A base this port cannot resolve.** `base_symbols_of`
    ///   (`crate::members`) resolves the heritage name in `SymbolFlags::TYPE`
    ///   meaning and answers `None` for `class C extends someExpression()` and
    ///   for a generic instantiation. Upstream prints `any` for many of those and
    ///   this gaps instead.
    /// - **`extends null`**, whose answer is the null-widening type
    ///   (`checker.go:7930`).
    fn check_super_expression(&mut self, node: NodeId) -> TypeId {
        let error = self.intrinsics.error;
        // `isCallExpression` (`checker.go:7855`): this `super` is its own call's
        // callee. Read before the walk, because it overrides the container's
        // static-ness rather than depending on it.
        let is_call = self.nodes.parent(node).is_some_and(|parent| {
            matches!(self.node_map.get(parent), Some(Node::CallExpression(call))
                if call.expression.and_then(|callee| callee.node_id()) == Some(node))
        });
        // `getSuperContainer(node, stopOnFunctions: true)` (`checker.go:7856`):
        // the nearest **member**, not the nearest class. An arrow is transparent,
        // so it is absent from this match for the same reason it is absent from
        // [`Checker::check_this_expression`]'s.
        let mut current = self.nodes.parent(node);
        let mut is_static = None;
        let mut class = None;
        while let Some(id) = current {
            match self.nodes.kind(id) {
                // Two different reasons, one answer. A plain function is where
                // `getSuperContainer(node, stopOnFunctions: true)` stops, so an
                // outer class is not reached; an object-literal container is
                // upstream's `any` (`checker.go:7917`), which
                // `checker-notes-rank.md` §6 forbids banking on. They are one arm
                // because clippy will not keep two that return the same thing,
                // and the distinction lives here rather than in the shape.
                SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ObjectLiteralExpression => return error,
                SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor => {
                    if is_static.is_none() {
                        is_static = Some(self.has_static_modifier(id));
                    }
                }
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression => {
                    class = Some(id);
                    break;
                }
                _ => {}
            }
            current = self.nodes.parent(id);
        }
        let (Some(class), Some(is_static)) = (class, is_static) else { return error };
        let Some(symbol) = self.binder.symbol_of(class) else { return error };
        let Some(bases) = self.base_symbols_of(symbol) else { return error };
        // Exactly one `extends` entry, which is the grammar for a class. Zero is
        // a base-less class — upstream's own error — and more than one cannot
        // arise; both gap rather than guessing which base `super` means.
        let [base] = bases[..] else { return error };
        if is_static || is_call {
            self.get_type_of_symbol(base)
        } else {
            self.get_declared_type_of_symbol(base)
        }
    }

    /// Whether a class member carries `static`. `ast.IsStatic` (`checker.go:7946`).
    fn has_static_modifier(&self, member: NodeId) -> bool {
        let modifiers = match self.node_map.get(member) {
            Some(Node::MethodDeclaration(node)) => node.modifiers,
            Some(Node::PropertyDeclaration(node)) => node.modifiers,
            Some(Node::GetAccessorDeclaration(node)) => node.modifiers,
            Some(Node::SetAccessorDeclaration(node)) => node.modifiers,
            // A constructor cannot be static.
            _ => return false,
        };
        modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::StaticKeyword)
        })
    }

    /// Ported from `Checker.checkConditionalExpression` (`checker.go:10934`).
    ///
    /// # The union is built at the wrong reduction, so the safe cases are fenced
    ///
    /// Upstream builds the branch union at **`UnionReductionSubtype`**
    /// (`checker.go:10940`), and this port only has `UnionReductionLiteral`
    /// ([`Checker::get_union_type`]) because subtype reduction needs an
    /// assignability question that does not exist here. The two reductions agree
    /// on primitives and unit types — `bob ? 1 : 2` is `1 | 2` under both, and
    /// `x ? 1 : n` is `number` under both, because literal reduction already
    /// removes a literal whose base primitive is present.
    ///
    /// They part company on object types, where subtype reduction collapses a
    /// constituent into a supertype it is assignable to. The baselines record
    /// exactly that: `>true ? a : b : { Foo?: Base; }` is one object type, not the
    /// two-member union this port would build. So an object-typed branch is a
    /// **gap**, fenced by [`Self::is_subtype_reduction_free`], rather than a
    /// plausible `A | B` that is wrong on every such line.
    ///
    /// The rejected alternative was to emit the union everywhere and accept the
    /// object case as a known divergence. It wins the moment `relater.rs` can
    /// answer assignability for object types, at which point the fence comes out
    /// and real subtype reduction goes in — not before, because a union that
    /// should have collapsed is a wrong line rather than a missing one.
    ///
    /// **How we would know this is wrong:** a baseline `>c ? a : b` line whose
    /// printed type is a union of two primitives that this port gaps, or a
    /// non-union answer where it prints a union.
    fn check_conditional_expression(
        &mut self,
        node: &tsr_ast::ConditionalExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        // Checked for its own line and its narrowing effects; the condition's
        // type does not reach the answer.
        if let Some(condition) = node.condition {
            self.check_expression(condition);
        }
        let (Some(when_true), Some(when_false)) = (node.when_true, node.when_false) else {
            return error;
        };
        let branches = [self.check_expression(when_true), self.check_expression(when_false)];
        // A gap in a branch is a gap in the conditional: `c ? 1 : unported` is
        // not `1`, and printing the known branch alone would be a wrong line.
        if branches.contains(&error) {
            return error;
        }
        // Two shapes need no reduction judgement (`checker-notes-assign.md`
        // §7): identical branches — the union of `[t, t]` is `t` under every
        // reduction, and references intern by `(symbol, args)` — and an
        // `any`/`unknown` branch, which absorbs the union under both
        // reductions. The freshness hop matters for the identity test:
        // `c ? 1 : 1` is the fresh literal twice, and `c ? x : 1` with
        // `x: 1` is the regular and the fresh spelling of one type, which
        // upstream's union regularises to one constituent.
        let regular = [
            self.get_regular_type_of_literal_type(branches[0]),
            self.get_regular_type_of_literal_type(branches[1]),
        ];
        let any = self.intrinsics.any;
        if regular[0] == regular[1]
            || branches.contains(&any)
            || branches.iter().all(|&branch| self.is_subtype_reduction_free(branch))
        {
            return self.get_union_type(&branches);
        }
        // The non-agnostic pairs run the decidability-gated `removeSubtypes`
        // (`checker-notes-assign.md` §9); an undecidable pair stays a gap.
        self.union_with_subtype_reduction(&branches).unwrap_or(error)
    }

    /// Whether `UnionReductionLiteral` and `UnionReductionSubtype` must agree for
    /// this type as a union constituent.
    ///
    /// True for the primitives and the unit types: subtype relationships among
    /// them are exactly the literal-to-base-primitive ones that literal reduction
    /// already handles. False for everything else — objects, type parameters,
    /// intersections, and the enum types, whose reduction is [`crate::unions`]'s
    /// question rather than this one's.
    ///
    /// A union is transparent: it is reduction-free when all of its constituents
    /// are, because `addTypesToUnion` flattens it before either reduction runs.
    pub(crate) fn is_subtype_reduction_free(&self, id: TypeId) -> bool {
        const SAFE: TypeFlags = TypeFlags::STRING
            .union(TypeFlags::NUMBER)
            .union(TypeFlags::BIG_INT)
            .union(TypeFlags::BOOLEAN)
            .union(TypeFlags::STRING_LITERAL)
            .union(TypeFlags::NUMBER_LITERAL)
            .union(TypeFlags::BIG_INT_LITERAL)
            .union(TypeFlags::BOOLEAN_LITERAL)
            .union(TypeFlags::NULL)
            .union(TypeFlags::UNDEFINED)
            .union(TypeFlags::VOID)
            .union(TypeFlags::NEVER);
        let t = self.store.get(id);
        if let TypeData::Union { types, symbol, .. } = &t.data {
            // A *named* union prints as its symbol, and `union_type_worker`
            // already refuses to nest one; keeping it out here makes the reason
            // local rather than relying on that.
            return symbol.is_none()
                && types.iter().all(|&constituent| self.is_subtype_reduction_free(constituent));
        }
        // An enum literal carries STRING_LITERAL or NUMBER_LITERAL as well, so
        // the flag test alone would let it through.
        !t.flags.intersects(TypeFlags::ENUM_LIKE) && SAFE.contains(t.flags)
    }

    /// The type of `new C()`.
    ///
    /// Ported from `Checker.resolveNewExpression` (`checker.go:8575`), reduced to
    /// the single shape whose answer does not need a construct signature.
    ///
    /// # Why this does not go through signatures at all
    ///
    /// Upstream takes the callee's apparent type, pulls its
    /// `SignatureKindConstruct` signatures and runs `resolveCall` over them; the
    /// result is that signature's return type. [`crate::signatures::Signature`]
    /// **has no construct flag** — `signatures.rs` says so where it refuses to
    /// fold `ConstructorTypeNode` into the function-type arm — so that route is
    /// closed.
    ///
    /// It is closed but not needed for the common case, because of a fact about
    /// classes rather than about signatures: a class's implicit construct
    /// signature returns the class's *instance* type, and a constructor cannot
    /// carry a return type annotation to make it return anything else. So for a
    /// class callee the answer is `getDeclaredTypeOfSymbol` on the class symbol,
    /// which this port already computes, and the signature is not on the path to
    /// it. 160 baseline lines record `>new C() : C`.
    ///
    /// This is a **reduction, not a shortcut**: it answers exactly the cases
    /// where the signature would have been redundant, and gaps every case where
    /// the signature actually carries information.
    ///
    /// # What gaps, and why each one has to
    ///
    /// - **A generic class.** `new C<T>()` needs `inferTypeArguments`, and the
    ///   uninstantiated instance type would print `C<T>` where upstream prints
    ///   the inferred `C<number>`.
    /// - **Explicit type arguments**, for the same reason, and matching
    ///   [`Self::check_call_expression`]'s rule.
    /// - **An abstract class.** Upstream reports and answers `errorType`
    ///   (`checker.go:8620`), so this is upstream's own answer rather than a
    ///   local gap.
    /// - **Any non-class callee.** `new Date()` prints `Date` upstream, but it
    ///   goes through a `DateConstructor` *interface* with a real construct
    ///   signature member; there is nothing about it this port can shortcut, and
    ///   48 baseline lines of `>new StringHashTable() : any` are a reminder that
    ///   the non-class cases do not all answer the obvious thing.
    fn check_new_expression(&mut self, node: &tsr_ast::NewExpression<'_>) -> TypeId {
        use crate::calls::counters::{COUNTERS, bump};

        let error = self.intrinsics.error;
        // The `new` funnel (`bd tsr-klm`). Counting only; no answer below
        // depends on it. This path had no counters at all, and `new` is the
        // half of the call row whose callee is *most often* typed — so the
        // uninstrumented half was also the most admitted one.
        bump(&COUNTERS.new_expressions);
        let Some(callee) = node.expression else { return error };
        let callee_type = self.check_expression(callee);
        // The callee's type is the class's *static* side, which
        // `getTypeOfFuncClassEnumModule` gives as an anonymous type carrying the
        // class symbol. Reaching the symbol through the type rather than through
        // the callee's syntax is what makes `new (C)()` and an aliased class
        // work the same way.
        let TypeData::Anonymous { symbol, .. } = self.store.get(callee_type).data else {
            // A constructor **interface** — `DateConstructor`, `ErrorConstructor`
            // — whose construct signatures live in its members rather than in
            // its symbol's declarations (`bd tsr-4sa`,
            // `docs/architecture/checker-notes-namedcallee.md`). Written type
            // arguments are not handled here: an interface's construct
            // signature that takes them is generic, and
            // `get_signature_of_named_type` declines a generic candidate.
            if node.type_arguments.is_empty() {
                if let Some(signature) = self.get_signature_of_named_type(
                    callee_type,
                    crate::signatures::SignatureKind::Construct,
                ) {
                    bump(&COUNTERS.new_resolved);
                    return signature.r#type;
                }
            }
            bump(&COUNTERS.new_callee_not_anonymous);
            return error;
        };
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::CLASS) {
            bump(&COUNTERS.new_callee_not_class);
            return error;
        }
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            bump(&COUNTERS.new_no_declaration);
            return error;
        };
        let (type_parameters, modifiers) = match self.node_map.get(declaration) {
            Some(Node::ClassDeclaration(class)) => (class.type_parameters, class.modifiers),
            Some(Node::ClassExpression(class)) => (class.type_parameters, class.modifiers),
            _ => {
                bump(&COUNTERS.new_declaration_not_class_like);
                return error;
            }
        };
        if !type_parameters.is_empty() {
            // `new C<string>()` — the caller wrote the type arguments, so the
            // instance type is `createTypeReference(C, [string])` and there is
            // nothing to infer. This is the *same* rule the call side already
            // applies (`crate::inference`: "the caller wrote the type
            // arguments, so there is nothing to infer and substitution is all
            // that is left"); the two halves of one construct answered
            // differently until `bd tsr-tgov`, and `checker-notes-callres.md`
            // §13.4 has the 826 lines that cost.
            //
            // Reached through `create_type_reference`, which is what
            // `crate::declared` calls for `C<string>` in *type* position, so
            // the instance type is identical **by interning** to the
            // annotation's — `let c: C<string> = new C<string>()` is one type,
            // and `tsr-4qx`'s instantiated members hang off it unchanged.
            // Re-read the written arguments from `self.node_map`, which carries
            // the checker's `'a`; the `node` parameter's lifetime is
            // independent of it and `get_type_from_type_node` needs `'a`. Same
            // move as [`Self::this_parameter_type`], and its comment explains
            // why the map is copied out first.
            let map = self.node_map;
            let written = match node.node_id.and_then(|id| map.get(id)) {
                Some(Node::NewExpression(from_map)) => from_map.type_arguments,
                _ => &[],
            };
            if written.len() == type_parameters.len() {
                let mut arguments = Vec::with_capacity(written.len());
                for argument in written {
                    let id = self.get_type_from_type_node(*argument);
                    // A gap in an argument gaps the whole `new`: `C<Unported>`
                    // is not `C<any>`, the rule the tuple and array arms use.
                    if id == error {
                        bump(&COUNTERS.new_type_parameters);
                        return error;
                    }
                    arguments.push(id);
                }
                bump(&COUNTERS.new_instantiated);
                return self.create_type_reference(symbol, arguments);
            }
            // A count that does not match the class's type parameters.
            // `checkTypeArguments` (`checker.go:9269`) fails the whole call,
            // and `fillMissingTypeArguments`' defaults are ported only for the
            // no-candidate case (`bd tsr-1uz`), so a shorter list that defaults
            // would make legal stays a gap rather than a guess.
            bump(&COUNTERS.new_type_parameters);
            return error;
        }
        // A non-generic class cannot take type arguments.
        if !node.type_arguments.is_empty() {
            bump(&COUNTERS.new_type_arguments);
            return error;
        }
        // `ast.HasModifier(valueDecl, ast.ModifierFlagsAbstract)` — upstream
        // reports "Cannot create an instance of an abstract class" and answers
        // `errorType`, so gapping here agrees with upstream rather than
        // diverging from it.
        if modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::AbstractKeyword)
        }) {
            bump(&COUNTERS.new_abstract);
            return error;
        }
        let declared = self.get_declared_type_of_symbol(symbol);
        bump(&COUNTERS.new_resolved);
        if declared == error {
            bump(&COUNTERS.new_resolved_error);
        }
        declared
    }

    /// The type of a `yield` expression.
    ///
    /// Ported from `Checker.checkYieldExpression` (`checker.go:10952`), reduced
    /// to the paths that reach `anyType` **before** any machinery this port
    /// lacks.
    ///
    /// # `any` here is a computed answer, not a gap wearing `any`
    ///
    /// This is the one place in this module that returns `anyType`, and it needs
    /// justifying against the `errorType`-not-`anyType` rule. The rule forbids
    /// answering `any` for a form we could not compute. It does not forbid
    /// answering `any` where **upstream's own computation returns `anyType`** —
    /// `signatures.rs` already does this for a declaration with no body. 430 of
    /// roughly 540 `>yield` baseline lines are `any`, and the two paths below
    /// return it unconditionally:
    ///
    /// - **No containing function** (`checker.go:10963`). `fn == nil` returns
    ///   `anyType` outright.
    /// - **A containing function that is not a generator**
    ///   (`checker.go:10967`). This return happens *before* the function reads
    ///   any return annotation or contextual type, so it cannot be perturbed by
    ///   the contextual typing this port does not have. That ordering is what
    ///   makes the answer safe rather than merely common.
    ///
    /// # Inside a real generator, only the uncontextualisable case answers
    ///
    /// For a generator with no return annotation, upstream ends at
    /// `getContextualIterationType(IterationTypeKindNext, fn)` falling back to
    /// `anyType` (`checker.go:11005`). This port has no contextual typing, so it
    /// would always take the fallback — right whenever the function has no
    /// contextual type, wrong when it has one.
    ///
    /// The fence is therefore on the **container's kind**, not on the yield: a
    /// function *declaration* and a class *method* cannot be contextually typed,
    /// while a function expression, an arrow and an object-literal method can.
    /// So the first two answer `any` and the rest gap.
    ///
    /// Gapped: `yield*` (needs `getIterationTypeOfIterable`), a generator with a
    /// return type annotation (needs
    /// `getIterationTypesOfGeneratorFunctionReturnType` — note this is *not*
    /// `any`, since `Generator<number>`'s next type is `unknown`), and any
    /// contextualisable container.
    /// Ported from `Checker.checkAwaitExpression` (`checker.go:10845`) —
    /// `checkAwaitedType` of the operand, reduced to the shapes decidable
    /// without the `then`-signature walk (`checker-notes-callres.md` §18):
    /// `any`/`unknown` pass through, a primitive is its own awaited type
    /// (nothing to carry a `then` member), and a reference to the **global**
    /// `Promise` unwraps to its argument, recursively. Everything else —
    /// unions, object types, type parameters, `PromiseLike`, user thenables —
    /// stays a gap with `getAwaitedTypeNoAlias` (`checker.go`) as the named
    /// owner. The grammar check and the "no effect" suggestion are
    /// diagnostics and out of scope here.
    fn check_await_expression(&mut self, node: &tsr_ast::AwaitExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let Some(operand) = node.expression else { return error };
        let operand_type = self.check_expression(operand);
        if operand_type == error {
            return error;
        }
        self.awaited_type_minimal(operand_type).unwrap_or(error)
    }

    /// The §18 slice of `getAwaitedTypeNoAlias`. `None` is a gap, never `any`.
    fn awaited_type_minimal(&mut self, id: TypeId) -> Option<TypeId> {
        let flags = self.store.get(id).flags;
        if flags.intersects(TypeFlags::ANY | TypeFlags::UNKNOWN) {
            return Some(id);
        }
        if flags.intersects(TypeFlags::PRIMITIVE) {
            return Some(id);
        }
        if let Some((target, arguments)) = self.type_reference_targets.get(&id)
            && arguments.len() == 1
            && Some(self.binder.merged_symbol(*target))
                == self.global_type_symbol("Promise").map(|s| self.binder.merged_symbol(s))
        {
            let argument = arguments[0];
            return self.awaited_type_minimal(argument);
        }
        None
    }

    fn check_yield_expression(&mut self, node: &tsr_ast::YieldExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let any = self.intrinsics.any;
        // Upstream checks the operand even when the yield is outside a
        // generator, "so its identifiers are resolved ... keeping diagnostics
        // stable regardless of traversal order". The operand's type does not
        // reach the answer on any path this port takes.
        if let Some(operand) = node.expression {
            self.check_expression(operand);
        }
        let Some(id) = node.node_id else { return error };
        let Some(container) = self.containing_function(id) else {
            // `fn == nil` — a `yield` at the top level.
            return any;
        };
        let (asterisk, annotation, contextualisable) = match self.node_map.get(container) {
            Some(Node::FunctionDeclaration(f)) => (f.asterisk_token, f.r#type, false),
            Some(Node::MethodDeclaration(f)) => (f.asterisk_token, f.r#type, false),
            Some(Node::FunctionExpression(f)) => (f.asterisk_token, f.r#type, true),
            // An arrow cannot be a generator at all, so it always takes the
            // not-a-generator arm below.
            Some(Node::ArrowFunction(f)) => (None, f.r#type, true),
            _ => return error,
        };
        if asterisk.is_none() {
            // Not a generator. Upstream returns `anyType` here before reading
            // anything else, so the container's contextual type is irrelevant.
            return any;
        }
        if node.asterisk_token.is_some() || annotation.is_some() || contextualisable {
            return error;
        }
        any
    }

    /// `ast.GetContainingFunction` (`utilities.go`).
    ///
    /// The nearest function-like ancestor. Shares its walk shape with
    /// [`Self::check_this_expression`] but not its rules: `this` treats an arrow
    /// as transparent, and this does not — an arrow is a function for the
    /// purpose of "which function contains this node", which is exactly why a
    /// `yield` inside an arrow inside a generator is not the generator's yield.
    fn containing_function(&self, node: NodeId) -> Option<NodeId> {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            if matches!(
                self.nodes.kind(id),
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::MethodDeclaration
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
                    | SyntaxKind::Constructor
            ) {
                return Some(id);
            }
            current = self.nodes.parent(id);
        }
        None
    }
}

/// Negate a number literal's already-normalised text.
///
/// `getNumberLiteralType(-jsnum.FromString(text))` (`checker.go:10864`) negates
/// the *value*, so the answer has to go back through the same normalisation the
/// positive literal did rather than gaining a `-` on the front. The two differ on
/// exactly one input: negative zero, which upstream prints `0` and the baselines
/// record four times as `>-0 : 0`. Prefixing the text would print `-0`.
///
/// `None` when the text is not a parseable number, which is
/// [`printing::normalise_number`]'s fallback for a literal the scanner has
/// already reported on.
fn negate_number_text(normalised: &str) -> Option<String> {
    let value: f64 = normalised.parse().ok()?;
    let negated = -value;
    // `-0.0 == 0.0` is true in IEEE 754, so this catches negative zero without
    // needing to inspect the sign bit, and turns it into the `0` upstream prints.
    if negated == 0.0 {
        return Some("0".to_string());
    }
    #[allow(clippy::float_cmp, reason = "exact integrality is the intended test")]
    let is_integral = negated == negated.trunc();
    Some(if is_integral && negated.abs() < 1e21 {
        format!("{negated:.0}")
    } else {
        format!("{negated}")
    })
}

impl Checker<'_, '_> {
    /// The assignment-target walk plus kind classification —
    /// `ast.GetAssignmentTarget` (`internal/ast/utilities.go:184`) and
    /// `getAssignmentTargetKind` (`internal/checker/utilities.go:90`). A `=`
    /// or logical-assignment binary and a for-in/for-of initializer position
    /// are DEFINITE; other assignment operators and `++`/`--` are COMPOUND.
    /// The walk climbs parentheses, array literals, spreads, non-null
    /// assertions, and the object-literal assignment shapes, so destructuring
    /// targets classify the same as direct ones.
    pub(crate) fn assignment_target_kind(&self, id: NodeId) -> AssignmentTargetKind {
        self.assignment_target(id).map_or(AssignmentTargetKind::None, |target| {
            match self.node_map.get(target) {
                Some(Node::BinaryExpression(binary)) => {
                    match binary.operator_token.map(|token| token.kind) {
                        Some(
                            SyntaxKind::EqualsToken
                            | SyntaxKind::AmpersandAmpersandEqualsToken
                            | SyntaxKind::BarBarEqualsToken
                            | SyntaxKind::QuestionQuestionEqualsToken,
                        ) => AssignmentTargetKind::Definite,
                        _ => AssignmentTargetKind::Compound,
                    }
                }
                Some(Node::PrefixUnaryExpression(_) | Node::PostfixUnaryExpression(_)) => {
                    AssignmentTargetKind::Compound
                }
                Some(Node::ForInOrOfStatement(_)) => AssignmentTargetKind::Definite,
                _ => AssignmentTargetKind::None,
            }
        })
    }

    /// The `BinaryExpression`, unary increment/decrement, or
    /// for-in/for-of statement that references `id` as an assignment target —
    /// `ast.GetAssignmentTarget` (`internal/ast/utilities.go:184`).
    fn assignment_target(&self, id: NodeId) -> Option<NodeId> {
        let mut current = id;
        loop {
            let parent = self.nodes.parent(current)?;
            match self.node_map.get(parent)? {
                Node::BinaryExpression(binary) => {
                    let is_assignment = binary.operator_token.is_some_and(|token| {
                        matches!(
                            token.kind,
                            SyntaxKind::EqualsToken
                                | SyntaxKind::PlusEqualsToken
                                | SyntaxKind::MinusEqualsToken
                                | SyntaxKind::AsteriskEqualsToken
                                | SyntaxKind::AsteriskAsteriskEqualsToken
                                | SyntaxKind::SlashEqualsToken
                                | SyntaxKind::PercentEqualsToken
                                | SyntaxKind::LessThanLessThanEqualsToken
                                | SyntaxKind::GreaterThanGreaterThanEqualsToken
                                | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                                | SyntaxKind::AmpersandEqualsToken
                                | SyntaxKind::BarEqualsToken
                                | SyntaxKind::CaretEqualsToken
                                | SyntaxKind::AmpersandAmpersandEqualsToken
                                | SyntaxKind::BarBarEqualsToken
                                | SyntaxKind::QuestionQuestionEqualsToken
                        )
                    });
                    let left = binary.left.and_then(|l| Node::from(l).node_id());
                    return (is_assignment && left == Some(current)).then_some(parent);
                }
                Node::PrefixUnaryExpression(unary) => {
                    return matches!(
                        unary.operator.kind,
                        SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                    )
                    .then_some(parent);
                }
                Node::PostfixUnaryExpression(unary) => {
                    return matches!(
                        unary.operator.kind,
                        SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                    )
                    .then_some(parent);
                }
                Node::ForInOrOfStatement(statement) => {
                    let initializer = statement.initializer.and_then(|i| Node::from(i).node_id());
                    return (initializer == Some(current)).then_some(parent);
                }
                Node::ParenthesizedExpression(_)
                | Node::ArrayLiteralExpression(_)
                | Node::SpreadElement(_)
                | Node::NonNullExpression(_) => current = parent,
                // The object-literal assignment shapes hop to the literal
                // itself, whose parent the next iteration classifies.
                Node::SpreadAssignment(_) => current = self.nodes.parent(parent)?,
                Node::ShorthandPropertyAssignment(shorthand) => {
                    if Node::from(shorthand.name).node_id() != Some(current) {
                        return None;
                    }
                    current = self.nodes.parent(parent)?;
                }
                Node::PropertyAssignment(property) => {
                    if Node::from(property.name).node_id() == Some(current) {
                        return None;
                    }
                    current = self.nodes.parent(parent)?;
                }
                _ => return None,
            }
        }
    }

    /// `isInCompoundLikeAssignment` (`internal/checker/utilities.go:118`): a
    /// definite `=` whose right side (parentheses skipped) is a
    /// shift-or-higher binary — `x = x + 1` reads its target like `x += 1`.
    fn is_in_compound_like_assignment(&self, id: NodeId) -> bool {
        let Some(target) = self.assignment_target(id) else { return false };
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(target) else {
            return false;
        };
        if binary.operator_token.map(|token| token.kind) != Some(SyntaxKind::EqualsToken) {
            return false;
        }
        let mut right = binary.right;
        while let Some(Expression::ParenthesizedExpression(inner)) = right {
            right = inner.expression;
        }
        let Some(Expression::BinaryExpression(inner)) = right else { return false };
        // `isShiftOperatorOrHigher`: shift, additive, multiplicative,
        // exponentiation.
        inner.operator_token.is_some_and(|token| {
            matches!(
                token.kind,
                SyntaxKind::LessThanLessThanToken
                    | SyntaxKind::GreaterThanGreaterThanToken
                    | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                    | SyntaxKind::PlusToken
                    | SyntaxKind::MinusToken
                    | SyntaxKind::AsteriskToken
                    | SyntaxKind::SlashToken
                    | SyntaxKind::PercentToken
                    | SyntaxKind::AsteriskAsteriskToken
            )
        })
    }

    /// `getBaseTypeOfLiteralType` (`checker.go`, the literal arms): a literal
    /// widens to its base primitive; a union maps constituents; everything
    /// else — including enum-like, whose base walk lives elsewhere — returns
    /// unchanged, which preserves current behaviour for the shapes §10's bar
    /// did not size.
    pub(crate) fn get_base_type_of_literal_type(&mut self, id: TypeId) -> TypeId {
        let flags = self.store.get(id).flags;
        if flags.intersects(TypeFlags::ENUM_LIKE) {
            return id;
        }
        if flags.intersects(TypeFlags::STRING_LITERAL) {
            return self.intrinsics.string;
        }
        if flags.intersects(TypeFlags::NUMBER_LITERAL) {
            return self.intrinsics.number;
        }
        if flags.intersects(TypeFlags::BIG_INT_LITERAL) {
            return self.intrinsics.bigint;
        }
        if flags.intersects(TypeFlags::BOOLEAN_LITERAL) {
            return self.intrinsics.boolean;
        }
        if flags.intersects(TypeFlags::UNION) {
            if let TypeData::Union { types, .. } = &self.store.get(id).data {
                let constituents = types.clone();
                let mapped: Vec<TypeId> = constituents
                    .into_iter()
                    .map(|constituent| self.get_base_type_of_literal_type(constituent))
                    .collect();
                return self.get_union_type(&mapped);
            }
        }
        id
    }
}
