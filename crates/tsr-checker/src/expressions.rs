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
            _ => self.intrinsics.error,
        }
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
