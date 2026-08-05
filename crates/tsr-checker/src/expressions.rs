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

        if let Some(id) = node.node_id() {
            self.node_types.insert(id, computed);
        }
        computed
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
                            self.get_flow_type_of_reference(node_id, symbol, declared)
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
            // `checkPrefixUnaryExpression` (`checker.go:10855`).
            Expression::PrefixUnaryExpression(node) => self.check_prefix_unary_expression(node),
            // `checkPostfixUnaryExpression` (`checker.go:10909`).
            Expression::PostfixUnaryExpression(node) => self.check_postfix_unary_expression(node),
            _ => self.intrinsics.error,
        }
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
        if operand == self.intrinsics.error
            || flags.intersects(TypeFlags::BIG_INT_LIKE)
            // `any` and `unknown` could each be a bigint at runtime, and
            // `maybeTypeOfKind` answers yes for them.
            || flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        {
            return self.intrinsics.error;
        }
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
