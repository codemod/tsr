//! The checker.
//!
//! Ported from `internal/checker/checker.go`. This is the first vertical slice:
//! literal and keyword expressions, and the freshness rule that decides whether
//! `const x = "a"` is `"a"` or `string`. See the crate docs for exactly what
//! exists.

use rustc_hash::FxHashMap;
use tsr_ast::{Expression, Node, NodeFlags, NodeId, NodeMap, NodeTable, SyntaxKind, TypeNode};
use tsr_binder::{BindResult, SymbolFlags, SymbolId};

use crate::{
    flags::TypeFlags,
    intrinsics::Intrinsics,
    printing,
    resolution::{PropertyName, Resolutions},
    types::{TypeData, TypeId, TypeStore},
};

/// Computes types.
///
/// **Every method takes `&mut self` and returns [`TypeId`].** No method hands out
/// a reference into checker state, which is what makes lazy memoisation ordinary
/// safe Rust here — see
/// [ADR-0013](../../../docs/adr/0013-checker-memoisation.md). Read a type's
/// contents with [`Checker::type_of`].
pub struct Checker<'a, 'n> {
    store: TypeStore,
    intrinsics: Intrinsics,
    nodes: &'n NodeTable,
    /// The way back from a `NodeId` to the typed node
    /// ([ADR-0033](../../../docs/adr/0033-the-parser-fills-the-node-map.md)).
    /// A symbol names its declaration by id; the annotation and initialiser live
    /// in the node.
    node_map: &'n NodeMap<'a>,
    binder: &'a BindResult<'a>,
    /// `expression node -> its type`, the memo upstream keeps in `nodeLinks`.
    node_types: FxHashMap<NodeId, TypeId>,
    /// How many times the *worker* has run, as opposed to the memo answering.
    ///
    /// Instrumentation, not state: it exists because the memo is otherwise
    /// unobservable. Interning already makes a repeated literal produce no new
    /// type, so a test that counts types cannot tell a working memo from a
    /// missing one — a no-op test, and this counter is what makes the memo test
    /// actually bite.
    computations: usize,
    /// Fresh literal type -> its widened (regular) form.
    ///
    /// Upstream keeps this as `regularType` on the literal type itself
    /// (`types.go`, `LiteralType.regularType`). A side table here, for the same
    /// reason the binder uses one: the stored type is immutable once created.
    regular_types: FxHashMap<TypeId, TypeId>,
    /// `symbol -> its type`, upstream's `valueSymbolLinks[symbol].resolvedType`.
    symbol_types: FxHashMap<SymbolId, TypeId>,
    /// `(generic symbol, type arguments) -> the instantiated reference`,
    /// upstream's `d.instantiations` keyed by `getTypeListKey`
    /// (`checker.go:17342`).
    ///
    /// Identity matters even though nothing yet looks inside one of these types:
    /// `C<number>` written twice must be one type, or the first relation check
    /// written will compare two handles that should have been equal.
    instantiations: FxHashMap<(SymbolId, Vec<TypeId>), TypeId>,
    /// A class symbol to its `this` type, upstream's `d.thisType`
    /// (`checker.go:17334`). One per class, so `this` has a stable identity
    /// inside one.
    this_types: FxHashMap<SymbolId, TypeId>,
    /// `symbol -> the type it *declares*`, upstream's
    /// `declaredTypeLinks[symbol].declaredType`.
    ///
    /// Separate from [`Checker::symbol_types`] because they are different
    /// questions about the same symbol: a class `C` declares the instance type
    /// `C` and *has* the type `typeof C`. Merging them would answer one with the
    /// other.
    declared_types: FxHashMap<SymbolId, TypeId>,
    /// In-progress resolutions, for circularity detection.
    resolutions: Resolutions<SymbolId>,
}

impl<'a, 'n> Checker<'a, 'n> {
    /// Create a checker over one bound file.
    #[must_use]
    pub fn new(
        binder: &'a BindResult<'a>,
        nodes: &'n NodeTable,
        node_map: &'n NodeMap<'a>,
    ) -> Self {
        let mut store = TypeStore::new();
        let intrinsics = Intrinsics::create(&mut store);
        Self {
            store,
            intrinsics,
            nodes,
            node_map,
            binder,
            computations: 0,
            node_types: FxHashMap::default(),
            regular_types: FxHashMap::default(),
            symbol_types: FxHashMap::default(),
            declared_types: FxHashMap::default(),
            this_types: FxHashMap::default(),
            instantiations: FxHashMap::default(),
            resolutions: Resolutions::new(),
        }
    }

    /// The well-known types.
    #[must_use]
    pub fn intrinsics(&self) -> &Intrinsics {
        &self.intrinsics
    }

    /// Read a type's contents.
    ///
    /// The only way in: [`TypeId`] is a handle and the store owns the type.
    #[must_use]
    pub fn type_of(&self, id: TypeId) -> &crate::types::Type {
        self.store.get(id)
    }

    /// Render a type as a `.types` baseline would print it.
    #[must_use]
    pub fn type_to_string(&self, id: TypeId) -> String {
        printing::type_to_string(self.store.get(id))
    }

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
            // Ported from `Checker.checkIdentifier`, reduced to its first step:
            // resolve the name and take the symbol's type.
            //
            // **No control-flow narrowing.** Upstream reaches
            // `getNarrowedTypeOfSymbol` / `getFlowTypeOfReference` here, so a
            // reference to `let x = "a"` narrows to `"a"` at a point where the
            // assignment dominates. This returns the *declared* type, `string`.
            // The binder builds the flow graph already; nothing reads it yet
            // (`bd tsr-4sc`). Until it does, reference lines in a `.types`
            // baseline will disagree wherever narrowing applies.
            Expression::Identifier(node) => {
                let Some(id) = node.node_id else { return self.intrinsics.error };
                match self.binder.resolve(self.nodes, id, node.text) {
                    Some(symbol) => self.get_type_of_symbol(symbol),
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

    /// Ported from `Checker.checkPropertyAccessExpression` into
    /// `checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11244`,
    /// `:11258`), reduced to the lookup.
    ///
    /// Upstream takes the receiver's **apparent** type first, which is what makes
    /// `"a".length` work: a primitive's apparent type is its wrapper interface
    /// from `lib.d.ts`. There are no lib files (`bd tsr-9or.1`), so a primitive
    /// receiver has no members here and answers `errorType` — a gap the histogram
    /// attributes to lib rather than to this function.
    ///
    /// Not ported: optional chains, private identifiers, `super`, index
    /// signatures, and **inherited members** — a base class's properties are not
    /// in the derived symbol's table, so `class C extends B {}` finds nothing of
    /// `B`'s. All answer `errorType`.
    pub fn check_property_access_expression(
        &mut self,
        node: &tsr_ast::PropertyAccessExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let (Some(receiver), Some(tsr_ast::MemberName::Identifier(name))) =
            (node.expression, node.name)
        else {
            return error;
        };
        let receiver_type = self.check_expression(receiver);
        // No explicit test for an `errorType` receiver: it is an intrinsic and
        // never carries a members table, so the lookup below misses and answers
        // `errorType` anyway. An earlier draft guarded it and no mutation could
        // make the guard observable, so it was removed rather than kept as
        // decoration.
        match self.get_property_of_type(receiver_type, name.text) {
            Some(property) => self.get_type_of_symbol(property),
            None => error,
        }
    }

    /// Ported from `Checker.getPropertyOfType` (`checker.go`), reduced to a
    /// members-table lookup.
    ///
    /// Upstream resolves the type's structure first — base types, index
    /// signatures, mapped and intersection members. This asks the one table the
    /// binder already built, which is why inherited and index-signature
    /// properties are misses rather than answers.
    #[must_use]
    pub fn get_property_of_type(&mut self, id: TypeId, name: &str) -> Option<SymbolId> {
        let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data else {
            return None;
        };
        self.binder.symbols().get(owner).members.get(name).copied()
    }

    /// Ported from `Checker.checkBinaryExpression` / `checkBinaryLikeExpression`
    /// (`checker.go:12331`, `:12336`).
    ///
    /// # What is here and what is not
    ///
    /// Upstream's worker is a dozen unrelated rules behind one node kind, and
    /// they do not become available at the same time. Ported: assignment, the
    /// arithmetic/bitwise/shift family, `+`, the relational and equality
    /// families, `in`, `instanceof` and the comma operator. Not ported: the
    /// logical operators, which compute a *union* of the operands
    /// (`bd tsr-4sc.9`), and destructuring assignment, whose left-hand side is
    /// an object or array literal pattern (`bd tsr-4sc.13`). Both yield
    /// `errorType`.
    ///
    /// Nothing here reports a diagnostic: upstream's arms are mostly error
    /// reporting, and the result type is computed independently of it. That is
    /// why this is a small function against a large one, rather than an
    /// abbreviation of it. `bd tsr-5e7.6`.
    fn check_binary_expression(&mut self, node: &tsr_ast::BinaryExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let (Some(left), Some(operator_token), Some(right)) =
            (node.left, node.operator_token, node.right)
        else {
            return error;
        };
        let operator = operator_token.kind;

        // `[a, b] = c` is a destructuring assignment, and upstream leaves this
        // function before checking either operand as an expression
        // (`checker.go:12338`). Taking the right-hand type would be *nearly*
        // right and would silently mistype the pattern itself.
        //
        // **This test is unobservable today**, and that is stated rather than
        // covered by a test that would not bite: an array or object literal is
        // itself unported, so both paths reach `errorType` and deleting the test
        // turns nothing red. It becomes load-bearing the moment those literals
        // are checked, at which point the fall-through would answer the
        // right-hand type for a pattern. Same reasoning as the symbol-flags test
        // in `get_type_of_symbol`.
        if operator == SyntaxKind::EqualsToken
            && matches!(
                left,
                Expression::ObjectLiteralExpression(_) | Expression::ArrayLiteralExpression(_)
            )
        {
            return error;
        }

        let left_type = self.check_expression(left);
        let right_type = self.check_expression(right);

        match operator {
            // Assignment yields the right-hand type — including its *freshness*,
            // which is why `x = "a"` is `"a"` and not `string`. The rest of
            // upstream's arm is `checkAssignmentOperator`, which reports rather
            // than computes.
            SyntaxKind::EqualsToken | SyntaxKind::CommaToken => right_type,

            SyntaxKind::AsteriskToken
            | SyntaxKind::AsteriskAsteriskToken
            | SyntaxKind::SlashToken
            | SyntaxKind::PercentToken
            | SyntaxKind::MinusToken
            | SyntaxKind::LessThanLessThanToken
            | SyntaxKind::GreaterThanGreaterThanToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
            | SyntaxKind::BarToken
            | SyntaxKind::CaretToken
            | SyntaxKind::AmpersandToken
            | SyntaxKind::AsteriskEqualsToken
            | SyntaxKind::AsteriskAsteriskEqualsToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::PercentEqualsToken
            | SyntaxKind::MinusEqualsToken
            | SyntaxKind::LessThanLessThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
            | SyntaxKind::BarEqualsToken
            | SyntaxKind::CaretEqualsToken
            | SyntaxKind::AmpersandEqualsToken => {
                self.check_arithmetic_operation(left_type, right_type)
            }

            SyntaxKind::PlusToken | SyntaxKind::PlusEqualsToken => {
                self.check_addition(left_type, right_type)
            }

            // Every comparison is `boolean` **whatever the operands are** —
            // upstream computes the operand types only to report on them and
            // returns `booleanType` unconditionally (`checker.go:12460`, `:12474`,
            // and `checkInExpression`). So these answer even where an operand is
            // a gap, and that is a computed answer rather than a guess.
            SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::LessThanEqualsToken
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::EqualsEqualsToken
            | SyntaxKind::ExclamationEqualsToken
            | SyntaxKind::EqualsEqualsEqualsToken
            | SyntaxKind::ExclamationEqualsEqualsToken
            | SyntaxKind::InKeyword
            // `instanceof` reaches `checkInstanceOfExpression`
            // (`checker.go:12492`), which consults `Symbol.hasInstance` and then
            // returns `booleanType` as well. The result does not depend on that
            // lookup, so it joins the arm rather than getting one of its own.
            | SyntaxKind::InstanceOfKeyword => self.intrinsics.boolean,

            // The logical operators build a union of the operands, and unions do
            // not exist yet (`bd tsr-4sc.9`). `errorType`, not the left type,
            // which would be right only when the left operand is never falsy.
            _ => error,
        }
    }

    /// The arithmetic, bitwise and shift arm of `checkBinaryLikeExpression`
    /// (`checker.go:12358`).
    ///
    /// Upstream: `number` when both operands are any-like or neither is
    /// bigint-like, `bigint` when both are bigint-like, and `errorType`
    /// otherwise.
    ///
    /// # A deliberate deviation: a gap in is a gap out
    ///
    /// `errorType` carries `TypeFlagsAny`, so upstream's first test — *"if both
    /// are any or unknown, assume the operation resolves to `number`"* — accepts
    /// it and answers `number`. In upstream that is sound: `errorType` appears
    /// only where a real error was already reported, and the operand genuinely
    /// could be anything.
    ///
    /// In this port `errorType` also means **an unported form**, and there the
    /// same rule would convert a gap into a claim: `someUnportedThing * 2` would
    /// read `number` whether or not the operand is a `bigint`, and the
    /// `checker_types` histogram could no longer tell the two apart. So an
    /// `errorType` operand propagates. Upstream itself does exactly this in the
    /// `+` arm (`checker.go:12452`), which is the precedent.
    ///
    /// **How this would be shown wrong:** when the unported expression forms
    /// land, the operands stop being `errorType` and the deviation stops
    /// applying to anything. If a measurable population of lines still reaches
    /// here with an `errorType` operand at that point, this should return
    /// `number` as upstream does — the honest reading of that would be that the
    /// gap is not the operand's form but something else.
    fn check_arithmetic_operation(&mut self, left: TypeId, right: TypeId) -> TypeId {
        if self.is_error(left) || self.is_error(right) {
            return self.intrinsics.error;
        }
        let left_flags = self.store.get(left).flags;
        let right_flags = self.store.get(right).flags;
        if left_flags.intersects(TypeFlags::BIG_INT_LIKE)
            || right_flags.intersects(TypeFlags::BIG_INT_LIKE)
        {
            if left_flags.intersects(TypeFlags::BIG_INT_LIKE)
                && right_flags.intersects(TypeFlags::BIG_INT_LIKE)
            {
                return self.intrinsics.bigint;
            }
            // Mixing `bigint` with anything else is an error, and upstream's
            // answer for it is `errorType` rather than a best guess.
            return self.intrinsics.error;
        }
        self.intrinsics.number
    }

    /// The `+` arm of `checkBinaryLikeExpression` (`checker.go:12403`).
    ///
    /// Upstream's order is load-bearing and is kept: both number-like → `number`,
    /// both bigint-like → `bigint`, *either* string-like → `string`, either any
    /// → `any` unless either is `errorType`, in which case `errorType`.
    ///
    /// **The order is upstream's and is currently unobservable here**, which is
    /// stated rather than dressed up as a test: with only primitive types, a
    /// type is string-like or number-like and never both, so swapping the two
    /// arms turns nothing red. It starts to matter with the types that are
    /// assignable to both kinds — enums, and unions of them — and it is kept in
    /// upstream's order so that it is already right when they arrive.
    fn check_addition(&mut self, left: TypeId, right: TypeId) -> TypeId {
        if self.is_error(left) || self.is_error(right) {
            return self.intrinsics.error;
        }
        let left_flags = self.store.get(left).flags;
        let right_flags = self.store.get(right).flags;
        let both = |kind: TypeFlags| left_flags.intersects(kind) && right_flags.intersects(kind);
        if both(TypeFlags::NUMBER_LIKE) {
            return self.intrinsics.number;
        }
        if both(TypeFlags::BIG_INT_LIKE) {
            return self.intrinsics.bigint;
        }
        if left_flags.intersects(TypeFlags::STRING_LIKE)
            || right_flags.intersects(TypeFlags::STRING_LIKE)
        {
            return self.intrinsics.string;
        }
        if left_flags.intersects(TypeFlags::ANY) || right_flags.intersects(TypeFlags::ANY) {
            return self.intrinsics.any;
        }
        // Upstream reports and answers `any` here; without diagnostics the honest
        // answer is that nothing was computed.
        self.intrinsics.error
    }

    /// Whether a type is `errorType` itself, by identity.
    ///
    /// Not a flag test: `errorType` and `anyType` share `TypeFlagsAny` and are
    /// distinguished only by identity, which is the whole point of them being
    /// separate types (`checker.go:979`).
    fn is_error(&self, id: TypeId) -> bool {
        id == self.intrinsics.error
    }

    /// The type of a symbol.
    ///
    /// Ported from `Checker.getTypeOfSymbol` (`checker.go:16493`). Upstream
    /// dispatches on nine symbol shapes; this slice ports the
    /// variable/parameter/property one and returns `errorType` for the rest.
    ///
    /// **`errorType`, not `anyType`.** Both print `any`, and only one of them is
    /// a claim that the answer *is* `any`. Every unported shape must be
    /// distinguishable from a computed answer or the conformance suite cannot
    /// tell a gap from a result.
    pub fn get_type_of_symbol(&mut self, symbol: SymbolId) -> TypeId {
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(SymbolFlags::VARIABLE | SymbolFlags::PROPERTY) {
            return self.get_type_of_variable_or_parameter_or_property(symbol);
        }
        // Unported: accessors, functions, classes, enums, enum members, modules,
        // aliases, and the four `CheckFlags` shapes upstream tests first
        // (deferred, instantiated, mapped, reverse-mapped).
        //
        // **The flags test above is currently unobservable**, and that is stated
        // rather than hidden behind a test that would not bite: for every symbol
        // shape this slice reaches, removing the test changes nothing, because a
        // non-variable symbol's declaration kind is rejected by the worker's
        // match anyway and lands on the same `errorType`. It is kept because it
        // is upstream's dispatch (`checker.go:16509`) and it starts to matter the
        // moment `getTypeOfFuncClassEnumModule` exists. Mutating it to `if true`
        // turns no test red today; mutating either `errorType` below does.
        self.intrinsics.error
    }

    /// Ported from `Checker.getTypeOfVariableOrParameterOrProperty`
    /// (`checker.go:16544`).
    ///
    /// The memo is ADR-0013's read-drop-recurse-write: the lookup's borrow ends
    /// before the recursion, because `TypeId` is `Copy` and nothing borrowed from
    /// `self` survives into it.
    fn get_type_of_variable_or_parameter_or_property(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.symbol_types.get(&symbol) {
            return cached;
        }
        let computed = self.get_type_of_variable_or_parameter_or_property_worker(symbol);
        self.symbol_types.insert(symbol, computed);
        computed
    }

    /// Ported from `Checker.getTypeOfVariableOrParameterOrPropertyWorker`
    /// (`checker.go:16578`).
    fn get_type_of_variable_or_parameter_or_property_worker(&mut self, symbol: SymbolId) -> TypeId {
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return self.intrinsics.error;
        };

        // The circularity guard wraps the *whole* computation, so a type that
        // reaches itself through any depth of indirection is caught. Nothing
        // between here and `pop` may return early, or the stack unbalances.
        if !self.resolutions.push(symbol, PropertyName::Type) {
            return self.report_circularity_error(declaration);
        }

        let kind = self.nodes.kind(declaration);
        let result = match kind {
            SyntaxKind::VariableDeclaration
            | SyntaxKind::Parameter
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::PropertySignature => {
                self.get_widened_type_for_variable_like_declaration(declaration)
            }
            // Unported: property assignments, shorthand, methods, export
            // assignments, binary/call assignment declarations, JSX attributes
            // and enum members.
            _ => self.intrinsics.error,
        };

        if !self.resolutions.pop() {
            // A cycle closed *below* this frame, so the answer computed above was
            // built on a partial one and must not be kept.
            return self.report_circularity_error(declaration);
        }
        result
    }

    /// Ported from `Checker.reportCircularityError` (`checker.go:18822`),
    /// without the diagnostics — the checker has none yet (`bd tsr-5e7.6`).
    ///
    /// The **return type differs by cause**, which is easy to get wrong because
    /// the two print identically:
    ///
    /// - a self-referencing *type annotation* yields `errorType`;
    /// - a self-referencing *initialiser* yields `anyType`.
    ///
    /// `bd tsr-4sc.2`'s issue text said "errorType" flatly. It is not.
    fn report_circularity_error(&mut self, declaration: NodeId) -> TypeId {
        if self.type_annotation_of(declaration).is_some() {
            return self.intrinsics.error;
        }
        self.intrinsics.any
    }

    /// Ported from `Checker.getWidenedTypeForVariableLikeDeclaration`, which is
    /// `widenTypeForVariableLikeDeclaration(getTypeForVariableLikeDeclaration(..))`
    /// (`checker.go:16610`, `:18242`).
    fn get_widened_type_for_variable_like_declaration(&mut self, declaration: NodeId) -> TypeId {
        match self.get_type_for_variable_like_declaration(declaration) {
            Some(id) => id,
            // Upstream returns `anyType` for a declaration with neither an
            // annotation nor an initialiser (`checker.go:18264`) — a genuine
            // answer, the implicit any, not a gap. So `anyType` is right here
            // where `errorType` is right for an unported form.
            None => self.intrinsics.any,
        }
    }

    /// Ported from `Checker.getTypeForVariableLikeDeclaration`
    /// (`checker.go:16652`), restricted to the two paths this slice covers.
    ///
    /// `None` means "nothing could be inferred", which upstream signals with a
    /// nil `*Type` and turns into the implicit `any` one level up.
    fn get_type_for_variable_like_declaration(&mut self, declaration: NodeId) -> Option<TypeId> {
        // An annotation wins over an initialiser, always.
        if let Some(annotation) = self.type_annotation_of(declaration) {
            return Some(self.get_type_from_type_node(annotation));
        }
        let initializer = self.initializer_of(declaration)?;
        let initializer_type = self.check_expression(initializer);
        Some(self.get_widened_literal_type_for_initializer(declaration, initializer_type))
    }

    /// Ported from `Checker.getWidenedLiteralTypeForInitializer`
    /// (`checker.go:16897`).
    ///
    /// This is the rule behind the most frequently surprising line in a `.types`
    /// baseline: `const x = "a"` is `"a"` and `let x = "a"` is `string`, from the
    /// same initialiser expression. A `const` keeps the literal; anything else
    /// widens it.
    fn get_widened_literal_type_for_initializer(
        &mut self,
        declaration: NodeId,
        id: TypeId,
    ) -> TypeId {
        if self.combined_node_flags(declaration).intersects(NodeFlags::CONSTANT) {
            return id;
        }
        self.get_widened_literal_type(id)
    }

    /// Ported from `ast.GetCombinedNodeFlags` / `getCombinedFlags`
    /// (`internal/ast/utilities.go:1180`).
    ///
    /// `const` is not a flag on the declaration: it is on the enclosing
    /// `VariableDeclarationList`, so answering "is this a const?" means walking
    /// up. This is the first place the port needs `NodeTable::parent`, and the
    /// reason [ADR-0033](../../../docs/adr/0033-the-parser-fills-the-node-map.md)
    /// insisted the lookup answer parent ids rather than only declarations.
    fn combined_node_flags(&self, declaration: NodeId) -> NodeFlags {
        let mut flags = self.nodes.flags(declaration);
        let mut node = declaration;
        if self.nodes.kind(node) == SyntaxKind::VariableDeclaration {
            let Some(parent) = self.nodes.parent(node) else { return flags };
            node = parent;
        }
        if self.nodes.kind(node) == SyntaxKind::VariableDeclarationList {
            flags |= self.nodes.flags(node);
            let Some(parent) = self.nodes.parent(node) else { return flags };
            node = parent;
        }
        if self.nodes.kind(node) == SyntaxKind::VariableStatement {
            flags |= self.nodes.flags(node);
        }
        flags
    }

    /// The type annotation of a declaration, if it has one.
    fn type_annotation_of(&self, declaration: NodeId) -> Option<TypeNode<'a>> {
        match self.node_map.get(declaration)? {
            Node::VariableDeclaration(node) => node.r#type,
            Node::ParameterDeclaration(node) => node.r#type,
            Node::PropertyDeclaration(node) => node.r#type,
            Node::PropertySignatureDeclaration(node) => node.r#type,
            _ => None,
        }
    }

    /// The initialiser of a declaration, if it has one.
    fn initializer_of(&self, declaration: NodeId) -> Option<Expression<'a>> {
        match self.node_map.get(declaration)? {
            Node::VariableDeclaration(node) => node.initializer,
            Node::ParameterDeclaration(node) => node.initializer,
            Node::PropertyDeclaration(node) => node.initializer,
            _ => None,
        }
    }

    /// The type a type node denotes.
    ///
    /// Ported from `Checker.getTypeFromTypeNodeWorker` (`checker.go:22811`),
    /// restricted to the forms this slice covers: the keyword types, literal
    /// types, and parenthesised types. Everything else — type references, arrays,
    /// unions, intersections, conditionals, mapped types, and the rest — yields
    /// `errorType`, because none of those type *shapes* exists yet.
    pub fn get_type_from_type_node(&mut self, node: TypeNode<'a>) -> TypeId {
        match node {
            TypeNode::KeywordTypeNode(keyword) => match keyword.kind {
                SyntaxKind::AnyKeyword => self.intrinsics.any,
                SyntaxKind::UnknownKeyword => self.intrinsics.unknown,
                SyntaxKind::StringKeyword => self.intrinsics.string,
                SyntaxKind::NumberKeyword => self.intrinsics.number,
                SyntaxKind::BigIntKeyword => self.intrinsics.bigint,
                SyntaxKind::BooleanKeyword => self.intrinsics.boolean,
                SyntaxKind::SymbolKeyword => self.intrinsics.es_symbol,
                SyntaxKind::VoidKeyword => self.intrinsics.void,
                SyntaxKind::UndefinedKeyword => self.intrinsics.undefined,
                SyntaxKind::NeverKeyword => self.intrinsics.never,
                SyntaxKind::ObjectKeyword => self.intrinsics.non_primitive,
                _ => self.intrinsics.error,
            },
            // `Checker.getTypeFromLiteralTypeNode`: the literal's type, made
            // **regular**. A literal in a type position is not fresh, which is
            // what keeps `let x: "a"` from widening to `string`.
            TypeNode::LiteralTypeNode(literal) => {
                // `literal` is a `Node`, not an `Expression`: a literal type's
                // payload can be `null`, or a prefixed `-1`, which are not the
                // same alias. Anything that is not an expression we can type is
                // an unported form.
                let Some(node) = literal.literal else { return self.intrinsics.error };
                let Ok(expression) = Expression::try_from(node) else {
                    return self.intrinsics.error;
                };
                let id = self.check_expression(expression);
                self.get_regular_type_of_literal_type(id)
            }
            TypeNode::ParenthesizedTypeNode(node) => node
                .r#type
                .map_or(self.intrinsics.error, |inner| self.get_type_from_type_node(inner)),
            TypeNode::TypeReferenceNode(node) => self.get_type_from_type_reference(node),
            TypeNode::TypeLiteralNode(node) => self.get_type_from_type_literal(node),
            _ => self.intrinsics.error,
        }
    }

    /// Ported from `Checker.getTypeFromTypeReference` into
    /// `getTypeReferenceType` (`checker.go:23146`).
    ///
    /// Resolves the name and asks the symbol what type it declares. Two things
    /// are deliberately left as gaps rather than approximated:
    ///
    /// - **A qualified name** (`M.I`) needs `resolveEntityName` walking module
    ///   exports, which the binder does not expose yet.
    /// - **Type arguments** (`C<number>`) need instantiation, the machinery
    ///   upstream guards with a depth of 100 and a count of 5 million
    ///   (`checker.go:22111`, `bd tsr-el3.2`). Half of it — substituting names
    ///   without the guards — is exactly the kind of port that works on the
    ///   corpus and hangs on a real program.
    fn get_type_from_type_reference(&mut self, node: &tsr_ast::TypeReferenceNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        let Some(tsr_ast::EntityName::Identifier(name)) = node.type_name else {
            return error;
        };
        let Some(id) = name.node_id else { return error };
        let Some(symbol) = self.binder.resolve(self.nodes, id, name.text) else {
            return error;
        };
        let parameters = self.local_type_parameters_of(symbol).len();
        if parameters == 0 {
            // `checkNoTypeArguments` (`checker.go:23157`): arguments on a type
            // that takes none is an error, and answering the bare declared type
            // would quietly drop them.
            if !node.type_arguments.is_empty() {
                return error;
            }
            let declared = self.get_declared_type_of_symbol(symbol);
            return self.get_regular_type_of_literal_type(declared);
        }
        self.get_instantiated_type_reference(node, symbol, parameters)
    }

    /// Ported from `Checker.getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode`
    /// (`checker.go`), for the type-literal half only.
    ///
    /// # An anonymous object type, printed structurally
    ///
    /// `{ a: string }` has no symbol to be named by, so unlike a class or an
    /// interface it prints its members: `{ a: string; }`, with the trailing
    /// semicolon and the surrounding spaces upstream\'s printer emits, and `{}`
    /// when there are none. That form is not a style choice — it is compared
    /// character for character against 26,686 corpus lines.
    ///
    /// # Any member this port cannot render makes the whole type a gap
    ///
    /// Methods, call and construct signatures, index signatures, accessors and
    /// computed names all remain unported. A literal containing one answers
    /// `errorType` rather than printing the members it *does* understand: a
    /// partial object type is a wrong answer that looks like a right one, and
    /// it would score as a mismatch either way. The same applies to a member
    /// whose own type is a gap.
    fn get_type_from_type_literal(&mut self, node: &tsr_ast::TypeLiteralNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        let mut printed = String::new();
        for member in node.members {
            let tsr_ast::TypeElement::PropertySignatureDeclaration(property) = member else {
                return error;
            };
            let tsr_ast::PropertyName::Identifier(name) = property.name else {
                return error;
            };
            let Some(annotation) = property.r#type else { return error };
            let member_type = self.get_type_from_type_node(annotation);
            if member_type == error {
                return error;
            }
            // `?` on a property signature; `!` cannot appear on one, so the
            // token\'s presence is enough to distinguish it.
            let optional =
                property.postfix_token.is_some_and(|token| token.kind == SyntaxKind::QuestionToken);
            let readonly = property.modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(m) if m.kind == SyntaxKind::ReadonlyKeyword)
            });
            printed.push_str(if readonly { "readonly " } else { "" });
            printed.push_str(name.text);
            printed.push_str(if optional { "?: " } else { ": " });
            printed.push_str(&self.type_to_string(member_type));
            printed.push_str("; ");
        }
        let printed = if printed.is_empty() { "{}".to_string() } else { format!("{{ {printed}}}") };
        // The binder gives a type literal its own anonymous `__type` symbol,
        // whose members table is where a property access on this type looks.
        let members = node.node_id.and_then(|id| self.binder.symbol_of(id));
        self.store.new_named(TypeFlags::OBJECT, printed, members)
    }

    /// A reference to a generic type: `C<number>`, `Tree<T>`.
    ///
    /// Ported from `getTypeFromClassOrInterfaceReference` and
    /// `getTypeFromTypeAliasReference` (`checker.go:23168`, `:23222`), reduced to
    /// what a printed line needs — the target and its arguments — and interned
    /// on that pair so `C<number>` written twice is one type.
    ///
    /// # There is no substitution here, and therefore no depth limit
    ///
    /// Upstream instantiates by *substituting* the arguments through the target's
    /// members, and guards that with an instantiation depth of 100 and a count of
    /// 5,000,000 (`checker.go:22111`) because self-referential generics generate
    /// new type identities forever. This port has no members to substitute into
    /// (see `getDeclaredTypeOfClassOrInterface`), so the recursion those limits
    /// exist to stop does not happen yet: the only recursion is over the *source*
    /// nesting of the argument type nodes, which is finite in a parsed file.
    ///
    /// **The limits are therefore not ported here, deliberately, and they are not
    /// optional later.** `bd tsr-el3.2` records that they belong with
    /// `instantiateType` — the code that actually recurses — and porting them now
    /// would be a guard around a loop that does not exist, which reads as
    /// coverage and provides none.
    ///
    /// # Arity
    ///
    /// Upstream reports and answers `errorType` when the count is outside
    /// `[minTypeArgumentCount, len(typeParameters)]` (`checker.go:23189`). Fewer
    /// arguments than parameters *within* that range is legal and fills from the
    /// parameters\' defaults (`fillMissingTypeArguments`), and a default may
    /// reference an earlier parameter — which is substitution, so that case is a
    /// gap rather than a guess.
    fn get_instantiated_type_reference(
        &mut self,
        node: &tsr_ast::TypeReferenceNode<'a>,
        symbol: SymbolId,
        parameters: usize,
    ) -> TypeId {
        let error = self.intrinsics.error;
        if node.type_arguments.len() != parameters {
            return error;
        }
        let mut arguments = Vec::with_capacity(parameters);
        for argument in node.type_arguments {
            let resolved = self.get_type_from_type_node(*argument);
            // A gap in an argument is a gap in the reference: `C<Unported>` is
            // not `C<any>`, and printing it as though the argument were known
            // would be a wrong line rather than a missing one.
            if resolved == error {
                return error;
            }
            arguments.push(resolved);
        }
        if let Some(&cached) = self.instantiations.get(&(symbol, arguments.clone())) {
            return cached;
        }
        let name = self.binder.symbols().get(symbol).name.to_string();
        let printed = arguments
            .iter()
            .map(|&argument| self.type_to_string(argument))
            .collect::<Vec<_>>()
            .join(", ");
        // `OBJECT` even when the target is a type alias, where upstream\'s
        // instantiated type carries the flags of the alias\'s *body*. The flags
        // are consulted by the arithmetic and `+` arms, and claiming
        // `Alias<number>` is string- or number-like would be worse than claiming
        // it is an object: `object` is the one answer those arms treat as
        // neither.
        // No members: see `TypeData::Named`. `C<number>`'s properties would be
        // `C`'s uninstantiated ones, so `c.a` would answer `T` where upstream
        // answers `number`.
        //
        // **This is blunter than upstream and it costs answers.** A member whose
        // type does not mention a type parameter — `class C<T> { a: string }` —
        // is the same before and after instantiation, and upstream answers it;
        // this gaps it. The correct rule is `couldContainTypeVariables`, which
        // arrives with real instantiation. There is deliberately **no test
        // pinning the current answer**, because the current answer is the worse
        // of the two and a test would cement it — and the obvious test cannot
        // tell the two apart anyway while `bd tsr-y4u.21` keeps a class's type
        // parameters out of every scope.
        let id = self.store.new_named(TypeFlags::OBJECT, format!("{name}<{printed}>"), None);
        self.instantiations.insert((symbol, arguments), id);
        id
    }

    /// The type a *type* symbol declares.
    ///
    /// Ported from `Checker.getDeclaredTypeOfSymbol` / `tryGetDeclaredTypeOfSymbol`
    /// (`checker.go:23670`), in upstream's dispatch order. Enum members and
    /// aliases (`import X = ...`) are unported and answer `errorType`.
    ///
    /// **This is not `getTypeOfSymbol`.** A class `C` *declares* the instance
    /// type `C` and *has* the type `typeof C`; asking the wrong one is how a
    /// baseline line ends up plausible and wrong.
    pub fn get_declared_type_of_symbol(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.declared_types.get(&symbol) {
            return cached;
        }
        let flags = self.binder.symbols().get(symbol).flags;
        let computed = if flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
            self.get_declared_type_of_class_or_interface(symbol)
        } else if flags.contains(SymbolFlags::TYPE_PARAMETER) {
            self.new_named_type(symbol, TypeFlags::TYPE_PARAMETER, false)
        } else if flags.contains(SymbolFlags::TYPE_ALIAS) {
            self.get_declared_type_of_type_alias(symbol)
        } else if flags.intersects(SymbolFlags::ENUM) {
            // **A divergence, and a visible one.** Upstream's declared type of an
            // enum is the *union of its members\' literal types*
            // (`checker.go:23874`), which happens to print as the enum\'s name.
            // Without unions (`bd tsr-4sc.9`) this is a named type that prints
            // the same string and has none of the behaviour: it cannot be
            // narrowed to a member, and `E.A` is not assignable to it because
            // nothing is assignable to anything yet. It is here because the
            // printed line is right and the alternative is a gap on every enum;
            // it must be replaced, not extended, when unions land.
            self.new_named_type(symbol, TypeFlags::ENUM, false)
        } else {
            self.intrinsics.error
        };
        self.declared_types.insert(symbol, computed);
        computed
    }

    /// Ported from `Checker.getDeclaredTypeOfClassOrInterface`
    /// (`checker.go:17319`).
    ///
    /// Upstream builds an object type with members, base types and a `this`
    /// type. This builds the *identity* and the printed form only: one type per
    /// symbol, printing `C` or `C<T>`. Members are `bd tsr-4sc.7`\'s second
    /// slice and nothing here depends on them, because no relation is computed
    /// yet — a type this port cannot look inside is still the right answer to
    /// "what type is this".
    fn get_declared_type_of_class_or_interface(&mut self, symbol: SymbolId) -> TypeId {
        self.new_named_type(symbol, TypeFlags::OBJECT, true)
    }

    /// Ported from `Checker.getDeclaredTypeOfTypeAlias` (`checker.go:23837`).
    ///
    /// A type alias is **transparent**: `type T = number` declares `number`, and
    /// upstream\'s baselines print it that way — `var x: T` reads `>x : number`
    /// (`conformance/typeAliases.types`). The alias name survives in the printed
    /// form only for *generic* aliases, which are a gap here.
    ///
    /// The circularity guard is upstream\'s and is not optional: `type T = T`
    /// resolves through this function forever without it, and the corpus
    /// contains such cases deliberately.
    fn get_declared_type_of_type_alias(&mut self, symbol: SymbolId) -> TypeId {
        let error = self.intrinsics.error;
        // A generic alias keeps its own name in the printed form:
        // `type Tree<T> = T | { left: Tree<T> }` records `>Tree : Tree<T>`
        // (`conformance/genericTypeAliases.types`), because upstream\'s declared
        // type for one is the body instantiated with the alias\'s own parameters
        // and carrying it as an alias symbol. The body is not expanded here —
        // which is also why a self-referential alias like `Tree` terminates
        // rather than needing the guard below.
        let parameters = self.local_type_parameter_names_of(symbol);
        if !parameters.is_empty() {
            let name = self.binder.symbols().get(symbol).name.to_string();
            return self.store.new_named(
                TypeFlags::OBJECT,
                format!("{name}<{}>", parameters.join(", ")),
                None,
            );
        }
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return error;
        };
        let Some(annotation) = self.node_map.get(declaration).and_then(|node| node.type_id())
        else {
            return error;
        };
        let Some(type_node) =
            self.node_map.get(annotation).and_then(|n| TypeNode::try_from(n).ok())
        else {
            return error;
        };
        if !self.resolutions.push(symbol, PropertyName::DeclaredType) {
            return error;
        }
        let resolved = self.get_type_from_type_node(type_node);
        if !self.resolutions.pop() {
            // A cycle closed below this frame, so the answer above was built on a
            // partial one. Upstream reports "Type alias 0 circularly references
            // itself" and answers `errorType`; the diagnostic is `bd tsr-5e7.6`.
            return error;
        }
        resolved
    }

    /// A named type for `symbol`, printed as `C` or `C<T, U>`.
    ///
    /// `with_type_parameters` is upstream\'s distinction between a type that can
    /// be generic and one that cannot: an enum or a type parameter never carries
    /// type parameters of its own, and asking for them would print `E<T>` for an
    /// enum declared inside a generic class.
    fn new_named_type(
        &mut self,
        symbol: SymbolId,
        flags: TypeFlags,
        with_type_parameters: bool,
    ) -> TypeId {
        let name = self.binder.symbols().get(symbol).name.to_string();
        let printed = if with_type_parameters {
            let parameters = self.local_type_parameter_names_of(symbol);
            if parameters.is_empty() { name } else { format!("{name}<{}>", parameters.join(", ")) }
        } else {
            name
        };
        // A class or interface owns its members; a type parameter and an enum do
        // not, and pointing them at a members table they do not have would be a
        // lookup that silently succeeds against the wrong symbol.
        let members = with_type_parameters.then_some(symbol);
        self.store.new_named(flags, printed, members)
    }

    /// The type parameters declared *on* a symbol\'s own declaration.
    ///
    /// Ported from `getLocalTypeParametersOfClassOrInterfaceOrTypeAlias`
    /// (`checker.go`), without the merging across declarations: a symbol with two
    /// declarations takes the first, which is where upstream would find the same
    /// list in every case this slice reaches.
    fn local_type_parameters_of(
        &self,
        symbol: SymbolId,
    ) -> &'a [&'a tsr_ast::TypeParameterDeclaration<'a>] {
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return &[];
        };
        match self.node_map.get(declaration) {
            Some(Node::ClassDeclaration(node)) => node.type_parameters,
            Some(Node::ClassExpression(node)) => node.type_parameters,
            Some(Node::InterfaceDeclaration(node)) => node.type_parameters,
            Some(Node::TypeAliasDeclaration(node)) => node.type_parameters,
            _ => &[],
        }
    }

    /// The names of those type parameters, in order.
    fn local_type_parameter_names_of(&self, symbol: SymbolId) -> Vec<String> {
        self.local_type_parameters_of(symbol)
            .iter()
            .map(|parameter| {
                parameter.name.map_or_else(|| "?".to_string(), |name| name.text.to_string())
            })
            .collect()
    }

    /// The regular (non-fresh) form of a literal type.
    ///
    /// Ported from `Checker.getRegularTypeOfLiteralType`. A no-op for anything
    /// that is not a fresh literal.
    pub fn get_regular_type_of_literal_type(&mut self, id: TypeId) -> TypeId {
        let ty = self.store.get(id);
        if !ty.fresh {
            return id;
        }
        let (flags, data) = (ty.flags, ty.data.clone());
        self.store.intern_literal(flags, data, false)
    }

    /// The fresh form of a literal type.
    ///
    /// Ported from `Checker.getFreshTypeOfLiteralType`.
    pub fn get_fresh_type_of_literal_type(&mut self, id: TypeId) -> TypeId {
        let ty = self.store.get(id);
        if ty.fresh || !ty.flags.intersects(TypeFlags::FRESHABLE) {
            return id;
        }
        let (flags, data) = (ty.flags, ty.data.clone());
        self.store.intern_literal(flags, data, true)
    }

    /// The widened form of a literal type.
    ///
    /// Ported from `Checker.getWidenedLiteralType` (`checker.go:25487`).
    /// **Widens only a *fresh* literal**, which is the whole reason freshness is
    /// tracked: `let x = "a"` widens because the expression `"a"` is fresh, while
    /// `let x: "a"` does not, because a literal type node is regular.
    pub fn get_widened_literal_type(&mut self, id: TypeId) -> TypeId {
        if let Some(&cached) = self.regular_types.get(&id) {
            return cached;
        }
        let ty = self.store.get(id);
        let flags = ty.flags;
        let widened = if !ty.fresh {
            id
        } else if flags.contains(TypeFlags::STRING_LITERAL) {
            self.intrinsics.string
        } else if flags.contains(TypeFlags::NUMBER_LITERAL) {
            self.intrinsics.number
        } else if flags.contains(TypeFlags::BIG_INT_LITERAL) {
            self.intrinsics.bigint
        } else if flags.contains(TypeFlags::BOOLEAN_LITERAL) {
            self.intrinsics.boolean
        } else {
            id
        };
        self.regular_types.insert(id, widened);
        widened
    }

    /// How many times an expression type was actually computed rather than
    /// served from the memo. See [`Checker::computations`]'s field docs.
    #[must_use]
    pub fn computations(&self) -> usize {
        self.computations
    }

    /// How many types exist. For tests that assert interning actually interns.
    #[must_use]
    pub fn type_count(&self) -> usize {
        self.store.len()
    }

    /// The node table this checker reads.
    #[must_use]
    pub fn nodes(&self) -> &NodeTable {
        self.nodes
    }
}
