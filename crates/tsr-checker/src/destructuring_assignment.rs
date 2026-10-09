//! The relation half of `checkDestructuringAssignment` (`checker.go:12552`):
//! every leaf target of an array or object literal on the left of `=` (or of
//! a `for…of` head) is related to the slice of the source it receives, by
//! `checkReferenceAssignment` (`checker.go:12704`) through
//! `checkTypeAssignableToAndOptionallyElaborate(sourceType, targetType,
//! target, target)`.
//!
//! `reference_target.rs` ports the same walk's `checkReferenceExpression`
//! half, which reads no types. This module walks the literal top-down with
//! the source type, as native does:
//!
//! - `checkObjectLiteralDestructuringPropertyAssignment` (`checker.go:12597`):
//!   a named property's slice is `getIndexedAccessTypeEx(source, name,
//!   ExpressionPosition | AllowMissing-if-defaulted)`; a last rest is
//!   `getRestType(source, siblingNames)`;
//! - `checkArrayLiteralDestructuringElementAssignment` (`checker.go:12663`):
//!   positional access on an array-like source, else the iterated element
//!   (`IterationUseDestructuring | PossiblyOutOfBounds`); a last rest is the
//!   tuple slice when every constituent is a tuple, else an array of the
//!   in-bounds element type;
//! - a defaulted target (`x = d`, or a shorthand's `= d`) strips `undefined`
//!   under `strictNullChecks` before recursing into its left.
//!
//! Each slice is read through the binding-pattern queries of `destructure.rs`
//! (`destructuring_property_lookup`, `concrete_rest_type`,
//! `binding_rest_tuple_slice`), which answer `errorType` where this port
//! cannot compute the slice; such a target, and every target below it, is
//! declined rather than related. The target type is
//! [`Checker::assignment_target_type`], the reader (and declines) the `=`
//! arm uses.
//!
//! `getFlowTypeOfDestructuring` (`checker.go:17849`) is not ported: native
//! narrows each slice through a synthetic `R["name"]` element access whose
//! flow node is the right operand `R`'s (`getSyntheticElementAccess`,
//! `:17857`), and this port's flow walker matches real nodes only
//! (`flow.rs`, main's). The synthetic reference exists only when `R` carries
//! a flow node, and its flow type differs from the declared slice only where
//! a flow node matches it — a condition on or an assignment to `R.name` /
//! `R["name"]` (`isMatchingReference`), every one of which is an access on
//! `R` written in `R`'s scope. [`DestructuringNarrowing`] therefore scans
//! that scope once per assignment and declines exactly the first-level
//! names it finds accessed (every name when `R` is not a plain identifier,
//! or when an element access on `R` has a non-literal key). Deeper slices
//! inherit their first-level ancestor's answer: `R.a.b` contains `R.a`.
//!
//! No cache or side table. Traversals: the literal's own elements, and for a
//! right operand with a flow node one scan of its declaration's enclosing
//! function (or file), per destructuring assignment.
//! `docs/parity/notes/r5-ts2322.md` §2.1.

use tsr_ast::{Expression, Node, NodeId, ObjectLiteralElementLike, PropertyName, SyntaxKind};

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::flow::TypeFacts;
use crate::types::{TypeData, TypeId};

impl Checker<'_, '_> {
    /// The `=` arm of `checkBinaryLikeExpression` (`checker.go:12338`): a
    /// left operand that is an object or array literal is checked by
    /// `checkDestructuringAssignment(left, checkExpression(right))`.
    pub(crate) fn check_destructuring_assignment_relations(&mut self, node: NodeId) {
        if self.in_js_file(node) || self.file_has_parse_errors {
            return;
        }
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else { return };
        if binary.operator_token.is_none_or(|t| t.kind != SyntaxKind::EqualsToken) {
            return;
        }
        let (Some(left), Some(right)) = (binary.left, binary.right) else { return };
        let Some(left) = left.node_id() else { return };
        if !matches!(
            self.nodes.kind(left),
            SyntaxKind::ObjectLiteralExpression | SyntaxKind::ArrayLiteralExpression
        ) {
            return;
        }
        // A nested `[a] = d` inside an outer target is also checked on its
        // own, against its default: native's walk calls
        // `checkBinaryExpression(target)` on it before recursing into its
        // left with the outer slice, and this port's traversal visits it as
        // its own node.
        let source = self.check_expression(right);
        let narrowing = match right.node_id() {
            Some(right) if self.binder.flow_of(right).is_some() => {
                self.destructuring_narrowing(right)
            }
            _ => DestructuringNarrowing::None,
        };
        self.check_destructuring_assignment_target(left, source, &narrowing);
    }

    /// `checkForOfStatement` (`checker.go:4050`): a literal left-hand side is
    /// `checkDestructuringAssignment(varExpr, iteratedType)`, the iterated
    /// type being `checkRightHandSideOfForOf`'s (async-first for `for await`).
    pub(crate) fn check_for_of_destructuring_relations(&mut self, node: NodeId) {
        if self.in_js_file(node) || self.file_has_parse_errors {
            return;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        if statement.kind.kind != SyntaxKind::ForOfStatement {
            return;
        }
        let (Some(initializer), Some(expression)) = (statement.initializer, statement.expression)
        else {
            return;
        };
        let Some(left) = initializer.node_id() else { return };
        if !matches!(
            self.nodes.kind(left),
            SyntaxKind::ObjectLiteralExpression | SyntaxKind::ArrayLiteralExpression
        ) {
            return;
        }
        let is_async = statement.await_modifier.is_some();
        let Some(source) = self.for_of_statement_element_type(expression, is_async) else {
            return;
        };
        // `getParentElementAccess` (`checker.go:17882`) has no arm for a
        // `for…of` head: no synthetic access, no narrowing.
        self.check_destructuring_assignment_target(left, source, &DestructuringNarrowing::None);
    }

    /// `checkDestructuringAssignment(node, sourceType)` for one target.
    fn check_destructuring_assignment_target(
        &mut self,
        node: NodeId,
        mut source: TypeId,
        narrowing: &DestructuringNarrowing,
    ) {
        if self.is_gap(source) || source == self.intrinsics.any {
            return;
        }
        let mut target = node;
        if let Some(Node::ShorthandPropertyAssignment(shorthand)) = self.node_map.get(node) {
            if let Some(initializer) = shorthand.object_assignment_initializer {
                // "If a default value of a non-undefined type is specified,
                // remove undefined from the final type."
                let default = self.check_expression(initializer);
                if self.strict_null_checks
                    && !self.get_type_facts(default).contains(TypeFacts::IS_UNDEFINED)
                {
                    source = self.get_type_with_facts(source, TypeFacts::NE_UNDEFINED);
                }
                // `checkBinaryLikeExpression(name, equalsToken, initializer)`:
                // the default is itself assigned to the name, an ordinary
                // `checkAssignmentOperator` relation at the name.
                if let (Some(name), Some(default_node)) =
                    (shorthand.name.node_id(), initializer.node_id())
                    && let Some(target_type) = self.assignment_target_type(name)
                {
                    self.report_assignability_failure(name, default_node, default, target_type);
                }
            }
            let Some(name) = shorthand.name.node_id() else { return };
            target = name;
        }
        if let Some(Node::BinaryExpression(binary)) = self.node_map.get(target)
            && binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken)
        {
            let Some(left) = binary.left.and_then(|left| left.node_id()) else { return };
            target = left;
            if self.strict_null_checks {
                source = self.get_type_with_facts(source, TypeFacts::NE_UNDEFINED);
            }
        }
        match self.nodes.kind(target) {
            SyntaxKind::ObjectLiteralExpression => {
                self.check_object_literal_assignment_relations(target, source, narrowing);
            }
            SyntaxKind::ArrayLiteralExpression => {
                self.check_array_literal_assignment_relations(target, source, narrowing);
            }
            _ => self.check_reference_assignment_relation(target, source),
        }
    }

    /// `checkReferenceAssignment` (`checker.go:12704`): only a valid
    /// reference (`checkReferenceExpression`) is related, at the target.
    fn check_reference_assignment_relation(&mut self, target: NodeId, source: TypeId) {
        if !self.is_assignable_reference(target) {
            return;
        }
        let Some(target_type) = self.assignment_target_type(target) else { return };
        self.report_assignability_failure(target, target, source, target_type);
    }

    /// `checkObjectLiteralAssignment` (`checker.go:12585`).
    fn check_object_literal_assignment_relations(
        &mut self,
        node: NodeId,
        source: TypeId,
        narrowing: &DestructuringNarrowing,
    ) {
        let Some(Node::ObjectLiteralExpression(object)) = self.node_map.get(node) else { return };
        let properties = object.properties;
        let count = properties.len();
        for (index, property) in properties.iter().enumerate() {
            let Some(id) = property.node_id() else { continue };
            match property {
                ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    let Some(initializer) = assignment.initializer.and_then(|e| e.node_id()) else {
                        continue;
                    };
                    let defaulted = self.is_default_assignment(initializer);
                    let Some(slice) = self.destructuring_slice_of_name(
                        source,
                        &assignment.name,
                        defaulted,
                        narrowing,
                    ) else {
                        continue;
                    };
                    self.check_destructuring_assignment_target(
                        initializer,
                        slice,
                        &DestructuringNarrowing::None,
                    );
                }
                ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    let defaulted = shorthand.object_assignment_initializer.is_some();
                    let Some(element) = self.destructuring_slice_of_name(
                        source,
                        &shorthand.name,
                        defaulted,
                        narrowing,
                    ) else {
                        continue;
                    };
                    self.check_destructuring_assignment_target(
                        id,
                        element,
                        &DestructuringNarrowing::None,
                    );
                }
                // A rest that is not last reports TS2462 and returns.
                ObjectLiteralElementLike::SpreadAssignment(spread) if index + 1 == count => {
                    let Some(expression) = spread.expression.and_then(|e| e.node_id()) else {
                        continue;
                    };
                    let Some(names) = self.object_literal_sibling_names(properties) else {
                        continue;
                    };
                    let rest = self.concrete_rest_type(source, &names);
                    if rest == self.intrinsics.error {
                        continue;
                    }
                    self.check_destructuring_assignment_target(
                        expression,
                        rest,
                        &DestructuringNarrowing::None,
                    );
                }
                _ => {}
            }
        }
    }

    /// The names `getRestType` subtracts: every non-rest property's name.
    /// A computed name declines the rest.
    fn object_literal_sibling_names(
        &mut self,
        properties: &[ObjectLiteralElementLike<'_>],
    ) -> Option<Vec<String>> {
        let mut names = Vec::new();
        for property in properties {
            match property {
                ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    names.push(self.literal_property_name_text(&assignment.name)?.0);
                }
                ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    names.push(self.literal_property_name_text(&shorthand.name)?.0);
                }
                ObjectLiteralElementLike::SpreadAssignment(_) => {}
                _ => return None,
            }
        }
        Some(names)
    }

    /// `getLiteralTypeFromPropertyName` of a written name, as the
    /// `(name, numeric)` pair `destructuring_property_lookup` reads. A
    /// computed name is its checked type when that is a literal.
    fn literal_property_name_text(&mut self, name: &PropertyName<'_>) -> Option<(String, bool)> {
        match name {
            PropertyName::Identifier(identifier) => Some((identifier.text.to_string(), false)),
            PropertyName::StringLiteral(literal) => Some((literal.text.to_string(), false)),
            PropertyName::NumericLiteral(literal) => {
                Some((crate::printing::normalise_number(literal.text), true))
            }
            PropertyName::ComputedPropertyName(computed) => {
                let key = self.check_expression(computed.expression?);
                let numeric = self.store.get(key).flags.intersects(TypeFlags::NUMBER_LITERAL);
                Some((self.property_name_from_index(key)?, numeric))
            }
            _ => None,
        }
    }

    /// The slice of `source` a `name: target` property receives; `None` when
    /// it cannot be computed or its flow type may differ (`narrowing`).
    fn destructuring_slice_of_name(
        &mut self,
        source: TypeId,
        name: &PropertyName<'_>,
        defaulted: bool,
        narrowing: &DestructuringNarrowing,
    ) -> Option<TypeId> {
        let (text, numeric) = self.literal_property_name_text(name)?;
        if narrowing.may_narrow(&text) {
            return None;
        }
        let element = self.destructuring_property_lookup(source, &text, numeric, defaulted);
        (element != self.intrinsics.error).then_some(element)
    }

    /// `hasDefaultValue` for an element written `target = default`.
    fn is_default_assignment(&self, node: NodeId) -> bool {
        matches!(
            self.node_map.get(node),
            Some(Node::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken)
        )
    }

    /// `checkArrayLiteralAssignment` (`checker.go:12648`) and its per-element
    /// `checkArrayLiteralDestructuringElementAssignment`.
    fn check_array_literal_assignment_relations(
        &mut self,
        node: NodeId,
        source: TypeId,
        narrowing: &DestructuringNarrowing,
    ) {
        let Some(Node::ArrayLiteralExpression(array)) = self.node_map.get(node) else { return };
        let elements = array.elements;
        let count = elements.len();
        let array_like = self.binding_parent_is_array_like(source);
        for (index, &element) in elements.iter().enumerate() {
            let Some(id) = element.node_id() else { continue };
            match element {
                Expression::OmittedExpression(_) => {}
                Expression::SpreadElement(spread) => {
                    if index + 1 != count {
                        continue;
                    }
                    let Some(rest) = spread.expression.and_then(|e| e.node_id()) else {
                        continue;
                    };
                    // A rest with an initializer is TS1186, not related.
                    if self.is_default_assignment(rest) {
                        continue;
                    }
                    let Some(rest_type) = self.array_destructuring_rest_type(source, index) else {
                        continue;
                    };
                    self.check_destructuring_assignment_target(
                        rest,
                        rest_type,
                        &DestructuringNarrowing::None,
                    );
                }
                _ => {
                    let element_type = match array_like {
                        Some(true) => {
                            if narrowing.may_narrow(&index.to_string()) {
                                continue;
                            }
                            let index_type = self.store.intern_literal(
                                TypeFlags::NUMBER_LITERAL,
                                TypeData::NumberLiteral(index.to_string()),
                                false,
                            );
                            let apparent = self.apparent_type(source);
                            let Some(element_type) = self.resolved_indexed_access_type(
                                apparent,
                                index_type,
                                self.no_unchecked_indexed_access,
                            ) else {
                                continue;
                            };
                            if self.is_default_assignment(id) {
                                self.get_type_with_facts(element_type, TypeFacts::NE_UNDEFINED)
                            } else {
                                element_type
                            }
                        }
                        Some(false) => {
                            // `IterationUseDestructuring | PossiblyOutOfBounds`:
                            // the iterated element, with `undefined` under
                            // `noUncheckedIndexedAccess`.
                            let Some(element_type) = self.for_of_element_type(source) else {
                                continue;
                            };
                            if self.no_unchecked_indexed_access {
                                let undefined = self.intrinsics.undefined;
                                self.get_union_type(&[element_type, undefined])
                            } else {
                                element_type
                            }
                        }
                        None => continue,
                    };
                    self.check_destructuring_assignment_target(
                        id,
                        element_type,
                        &DestructuringNarrowing::None,
                    );
                }
            }
        }
    }

    /// A last rest element's source: `sliceTupleType` over every constituent
    /// when all are tuples, else `createArrayType` of the in-bounds element
    /// type (`IterationUseDestructuring`).
    fn array_destructuring_rest_type(&mut self, source: TypeId, index: usize) -> Option<TypeId> {
        if self.every_constituent_is_tuple(source) {
            return self.binding_rest_tuple_slice(source, index);
        }
        let element = self.for_of_element_type(source)?;
        if self.is_gap(element) {
            return None;
        }
        let array = self.global_type_symbol("Array")?;
        Some(self.create_type_reference(array, vec![element]))
    }

    /// `everyType(sourceType, isTupleType)`, through an alias body as
    /// `binding_rest_tuple_slice` reads it.
    fn every_constituent_is_tuple(&mut self, source: TypeId) -> bool {
        let source = self.binding_type_alias_body(source);
        let parts = match &self.store.get(source).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![source],
        };
        parts.iter().all(|part| {
            self.tuple_element_lists.contains_key(part)
                || self.variadic_tuple_elements.contains_key(part)
        })
    }

    /// Which first-level slices of a destructuring assignment whose right
    /// operand `right` carries a flow node may be narrowed by
    /// `getFlowTypeOfDestructuring`: the names accessed on `right` anywhere in
    /// its declaration's enclosing function (or file). See the module doc.
    fn destructuring_narrowing(&mut self, right: NodeId) -> DestructuringNarrowing {
        let Some(Node::Identifier(identifier)) = self.node_map.get(right) else {
            return DestructuringNarrowing::All;
        };
        let text = identifier.text;
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            right,
            text,
            tsr_binder::SymbolFlags::VALUE,
        ) else {
            return DestructuringNarrowing::All;
        };
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return DestructuringNarrowing::All;
        };
        let mut scope = declaration;
        while let Some(parent) = self.nodes.parent(scope) {
            scope = parent;
            if matches!(
                self.nodes.kind(scope),
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::MethodDeclaration
                    | SyntaxKind::Constructor
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
                    | SyntaxKind::ClassStaticBlockDeclaration
                    | SyntaxKind::SourceFile
            ) {
                break;
            }
        }
        let mut names = Vec::new();
        let mut stack = vec![scope];
        while let Some(node) = stack.pop() {
            let Some(current) = self.node_map.get(node) else { continue };
            tsr_ast::for_each_child_id(current, |child| stack.push(child));
            let (receiver, name) = match current {
                Node::PropertyAccessExpression(access) => {
                    let name = match access.name {
                        Some(tsr_ast::MemberName::Identifier(name)) => Some(name.text.to_string()),
                        _ => None,
                    };
                    (access.expression, name)
                }
                Node::ElementAccessExpression(access) => {
                    let name = match access.argument_expression {
                        Some(Expression::StringLiteral(literal)) => Some(literal.text.to_string()),
                        Some(Expression::NumericLiteral(literal)) => {
                            Some(crate::printing::normalise_number(literal.text))
                        }
                        Some(Expression::NoSubstitutionTemplateLiteral(literal)) => {
                            Some(literal.text.to_string())
                        }
                        _ => None,
                    };
                    (access.expression, name)
                }
                _ => continue,
            };
            let mut receiver = receiver.and_then(|receiver| receiver.node_id());
            while let Some(Node::ParenthesizedExpression(inner)) =
                receiver.and_then(|id| self.node_map.get(id))
            {
                receiver = inner.expression.and_then(|inner| inner.node_id());
            }
            let Some(receiver) = receiver else { continue };
            let Some(Node::Identifier(candidate)) = self.node_map.get(receiver) else { continue };
            if candidate.text != text
                || self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    receiver,
                    candidate.text,
                    tsr_binder::SymbolFlags::VALUE,
                ) != Some(symbol)
            {
                continue;
            }
            match name {
                Some(name) => names.push(name),
                None => return DestructuringNarrowing::All,
            }
        }
        DestructuringNarrowing::Names(names)
    }
}

/// Which first-level slices `getFlowTypeOfDestructuring` may narrow (see the
/// module doc): none, the listed property names, or all.
pub(crate) enum DestructuringNarrowing {
    None,
    Names(Vec<String>),
    All,
}

impl DestructuringNarrowing {
    fn may_narrow(&self, name: &str) -> bool {
        match self {
            Self::None => false,
            Self::Names(names) => names.iter().any(|accessed| accessed == name),
            Self::All => true,
        }
    }
}
