//! TS18033 — `Type '{0}' is not assignable to type '{1}' as required for
//! computed enum member values.`
//!
//! `computeConstantEnumMemberValue`'s default arm (`checker.go:24019`): an
//! initializer of a non-const, non-ambient enum that the constant evaluator
//! does not fold is checked with `checkTypeAssignableTo(checkExpression(init),
//! numberType)`.
//!
//! This port has no symbol-aware evaluator (`enum_member_name.rs`, §819), so
//! "does not fold" is decided by [`Checker::enum_initializer_may_evaluate`], a
//! syntactic over-approximation of `evaluator.NewEvaluator` with the checker's
//! `evaluateEntity` (`checker.go:24024`): anything it cannot rule out is
//! declined, so the rule reports only where upstream's evaluator answers
//! `nil`. No cache, side table or traversal beyond the initializer's spine.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::relater::{Relation, Ternary};

impl Checker<'_, '_> {
    /// TS18033 for one enum member. `ambient` is the enclosing context's
    /// (`member.Parent.Flags&NodeFlagsAmbient`); a `const` enum takes TS2474
    /// at the same switch (`checker.go:24014`), so both are excluded here.
    pub(crate) fn check_computed_enum_member_initializer(&mut self, node: NodeId, ambient: bool) {
        if ambient {
            return;
        }
        let Some(Node::EnumMember(member)) = self.node_map.get(node) else { return };
        let Some(initializer) = member.initializer else { return };
        let Some(at) = initializer.node_id() else { return };
        let Some(parent) = self.nodes.parent(node) else { return };
        let Some(Node::EnumDeclaration(declaration)) = self.node_map.get(parent) else { return };
        if declaration.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if matches!(token.kind, SyntaxKind::ConstKeyword | SyntaxKind::DeclareKeyword))
        }) {
            return;
        }
        if self.enum_initializer_may_evaluate(at, 0) {
            return;
        }
        let source = self.check_expression(initializer);
        let number = self.intrinsics.number;
        if !self.assignability_pair_is_reportable(source, number)
            || self.relate_ternary(source, number, Relation::Assignable) != Ternary::NotRelated
        {
            return;
        }
        let printed = self.type_to_string(source);
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1_AS_REQUIRED_FOR_COMPUTED_ENUM_MEMBER_VALUES,
                span,
                [printed, "number".to_string()],
            ),
        );
    }

    /// Could `evaluate` (`evaluator.go:24`, skipping parentheses only) answer
    /// a value for this expression? `true` wherever it might; `false` only for
    /// shapes whose result is `nil` whatever their symbols resolve to, plus
    /// identifiers that resolve to neither an enum member, a constant
    /// variable `evaluateEntity` reads, nor the global `Infinity`/`NaN`.
    fn enum_initializer_may_evaluate(&self, node: NodeId, depth: u32) -> bool {
        if depth > 64 {
            return true;
        }
        let mut node = node;
        while let Some(Node::ParenthesizedExpression(wrapper)) = self.node_map.get(node) {
            let Some(inner) = wrapper.expression.and_then(|e| e.node_id()) else { return true };
            node = inner;
        }
        match self.node_map.get(node) {
            // Literals fold. Property and element accesses on an entity name go
            // to `evaluateEntity` (`ast.IsEntityNameExpression`), whose
            // enum-member reads this port cannot decide without symbols.
            Some(
                Node::StringLiteral(_)
                | Node::NoSubstitutionTemplateLiteral(_)
                | Node::NumericLiteral(_)
                | Node::PropertyAccessExpression(_)
                | Node::ElementAccessExpression(_),
            ) => true,
            Some(Node::PrefixUnaryExpression(unary)) => {
                matches!(
                    unary.operator.kind,
                    SyntaxKind::PlusToken | SyntaxKind::MinusToken | SyntaxKind::TildeToken
                ) && unary
                    .operand
                    .and_then(|e| e.node_id())
                    .is_none_or(|operand| self.enum_initializer_may_evaluate(operand, depth + 1))
            }
            Some(Node::BinaryExpression(binary)) => {
                let Some(token) = binary.operator_token else { return true };
                let folds = matches!(
                    token.kind,
                    SyntaxKind::BarToken
                        | SyntaxKind::AmpersandToken
                        | SyntaxKind::GreaterThanGreaterThanToken
                        | SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                        | SyntaxKind::LessThanLessThanToken
                        | SyntaxKind::CaretToken
                        | SyntaxKind::AsteriskToken
                        | SyntaxKind::SlashToken
                        | SyntaxKind::PlusToken
                        | SyntaxKind::MinusToken
                        | SyntaxKind::PercentToken
                        | SyntaxKind::AsteriskAsteriskToken
                );
                folds
                    && [binary.left, binary.right].into_iter().all(|side| {
                        side.and_then(|e| e.node_id())
                            .is_none_or(|side| self.enum_initializer_may_evaluate(side, depth + 1))
                    })
            }
            Some(Node::TemplateExpression(template)) => {
                template.template_spans.iter().all(|span| {
                    span.expression
                        .and_then(|e| e.node_id())
                        .is_none_or(|inner| self.enum_initializer_may_evaluate(inner, depth + 1))
                })
            }
            Some(Node::Identifier(identifier)) => {
                let Some(symbol) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    node,
                    identifier.text,
                    SymbolFlags::VALUE,
                ) else {
                    return false;
                };
                if matches!(identifier.text, "Infinity" | "NaN")
                    && self.binder.globals().get(identifier.text) == Some(&symbol)
                {
                    return true;
                }
                let entry = self.binder.symbols().get(symbol);
                // `resolveEntityName` follows an import alias to its target,
                // which this binder lookup does not; an alias may name a
                // constant variable or enum member elsewhere.
                if entry.flags.intersects(SymbolFlags::ENUM_MEMBER | SymbolFlags::ALIAS) {
                    return true;
                }
                entry.flags.intersects(SymbolFlags::VARIABLE)
                    && entry.value_declaration.is_some_and(|declaration| {
                        matches!(
                            self.node_map.get(declaration),
                            Some(Node::VariableDeclaration(variable))
                                if variable.r#type.is_none() && variable.initializer.is_some()
                        ) && self
                            .combined_node_flags(declaration)
                            .intersects(tsr_ast::NodeFlags::CONSTANT)
                    })
            }
            _ => false,
        }
    }

    /// TS2477 / TS2478 — `computeConstantEnumMemberValue`'s const arm
    /// (`checker.go:24001`): a `const` enum member whose initializer evaluates
    /// to a non-finite number reports at the initializer, `NaN` with its own
    /// message. Evaluated by [`Checker::const_enum_numeric_value`]; an
    /// initializer it cannot fold declines (it may still evaluate upstream
    /// through an enum member or constant).
    pub(crate) fn check_const_enum_member_value(&mut self, node: NodeId) {
        let Some(Node::EnumMember(member)) = self.node_map.get(node) else { return };
        let Some(at) = member.initializer.and_then(|initializer| initializer.node_id()) else {
            return;
        };
        let Some(parent) = self.nodes.parent(node) else { return };
        let Some(Node::EnumDeclaration(declaration)) = self.node_map.get(parent) else { return };
        if !declaration.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::ConstKeyword)
        }) {
            return;
        }
        let Some(value) = self.const_enum_numeric_value(at, 0) else { return };
        let message = if value.is_nan() {
            &messages::CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_DISALLOWED_VALUE_NAN
        } else if value.is_infinite() {
            &messages::CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_A_NON_FINITE_VALUE
        } else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(file, Diagnostic::new(message, span));
    }

    /// `evaluate` (`evaluator.go`) restricted to the numeric arms that can
    /// produce a non-finite value: numeric literals, the global `Infinity`
    /// and `NaN` (`evaluateEntity`'s first arm, `checker.go:24032`), unary
    /// `+`/`-`, and the arithmetic binary operators. Any other shape —
    /// enum members, constants, strings, bitwise operators (always finite)
    /// — answers `None`.
    fn const_enum_numeric_value(&self, node: NodeId, depth: u32) -> Option<f64> {
        if depth > 64 {
            return None;
        }
        match self.node_map.get(node)? {
            Node::ParenthesizedExpression(wrapper) => {
                self.const_enum_numeric_value(wrapper.expression?.node_id()?, depth + 1)
            }
            Node::NumericLiteral(literal) => Some(tsr_core::jsnum::numeric_value(literal.text)),
            Node::Identifier(identifier) if matches!(identifier.text, "Infinity" | "NaN") => {
                let symbol = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    node,
                    identifier.text,
                    SymbolFlags::VALUE,
                )?;
                (self.binder.globals().get(identifier.text) == Some(&symbol))
                    .then(|| if identifier.text == "NaN" { f64::NAN } else { f64::INFINITY })
            }
            Node::PrefixUnaryExpression(unary) => {
                let operand =
                    self.const_enum_numeric_value(unary.operand?.node_id()?, depth + 1)?;
                match unary.operator.kind {
                    SyntaxKind::PlusToken => Some(operand),
                    SyntaxKind::MinusToken => Some(-operand),
                    _ => None,
                }
            }
            Node::BinaryExpression(binary) => {
                let operator = binary.operator_token?.kind;
                let left = self.const_enum_numeric_value(binary.left?.node_id()?, depth + 1)?;
                let right = self.const_enum_numeric_value(binary.right?.node_id()?, depth + 1)?;
                match operator {
                    SyntaxKind::PlusToken => Some(left + right),
                    SyntaxKind::MinusToken => Some(left - right),
                    SyntaxKind::AsteriskToken => Some(left * right),
                    SyntaxKind::SlashToken => Some(left / right),
                    SyntaxKind::PercentToken => Some(left % right),
                    SyntaxKind::AsteriskAsteriskToken => Some(left.powf(right)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// TS2651 — `A member initializer in a enum declaration cannot reference
    /// members declared after it, including members defined in other enums.`
    ///
    /// `evaluateEnumMember` (`checker.go:24077`), reached when
    /// `computeEnumMemberValues` evaluates an initializer with the member as
    /// `location`. The evaluator (`evaluator.go`) visits a prefix operand,
    /// both binary operands whatever the operator, and hands identifiers and
    /// entity-name accesses to `evaluateEntity` (`checker.go:24024`); an
    /// enum member it resolves that is declared after `location` in the same
    /// file (`isBlockScopedNameDeclaredBeforeUse`, `checker.go:1922`) is
    /// reported at the reference. `docs/parity/notes/misc-checks.md` §15.
    pub(crate) fn check_enum_member_forward_references(&mut self, node: NodeId, ambient: bool) {
        // `isInAmbientOrTypeNode(usage)` makes every ambient use legal.
        if ambient {
            return;
        }
        let Some(Node::EnumMember(member)) = self.node_map.get(node) else { return };
        let Some(initializer) = member.initializer.and_then(|initializer| initializer.node_id())
        else {
            return;
        };
        if let Some(parent) = self.nodes.parent(node)
            && let Some(Node::EnumDeclaration(declaration)) = self.node_map.get(parent)
            && declaration.modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(token)
                    if token.kind == SyntaxKind::DeclareKeyword)
            })
        {
            return;
        }
        let mut offenders = Vec::new();
        self.collect_enum_forward_references(initializer, node, 0, &mut offenders);
        for at in offenders {
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::new(
                    &messages::A_MEMBER_INITIALIZER_IN_A_ENUM_DECLARATION_CANNOT_REFERENCE_MEMBERS_DECLARED_AFTER_IT_INCLUDING_MEMBERS_DEFINED_IN_OTHER_ENUMS,
                    span,
                ),
            );
        }
    }

    /// The evaluator's visit order over `expr`. A template's spans are
    /// visited only while earlier spans have values, which needs values;
    /// only the first span (always visited) is followed.
    fn collect_enum_forward_references(
        &self,
        expr: NodeId,
        location: NodeId,
        depth: u32,
        offenders: &mut Vec<NodeId>,
    ) {
        if depth > 64 {
            return;
        }
        match self.node_map.get(expr) {
            Some(Node::ParenthesizedExpression(wrapper)) => {
                if let Some(inner) = wrapper.expression.and_then(|e| e.node_id()) {
                    self.collect_enum_forward_references(inner, location, depth + 1, offenders);
                }
            }
            Some(Node::PrefixUnaryExpression(unary)) => {
                if let Some(operand) = unary.operand.and_then(|e| e.node_id()) {
                    self.collect_enum_forward_references(operand, location, depth + 1, offenders);
                }
            }
            Some(Node::BinaryExpression(binary)) => {
                for side in [binary.left, binary.right] {
                    if let Some(side) = side.and_then(|e| e.node_id()) {
                        self.collect_enum_forward_references(side, location, depth + 1, offenders);
                    }
                }
            }
            Some(Node::TemplateExpression(template)) => {
                if let Some(first) =
                    template.template_spans.first().and_then(|span| span.expression)
                    && let Some(first) = first.node_id()
                {
                    self.collect_enum_forward_references(first, location, depth + 1, offenders);
                }
            }
            Some(
                Node::Identifier(_)
                | Node::PropertyAccessExpression(_)
                | Node::ElementAccessExpression(_),
            ) => {
                if let Some(member) = self.evaluated_enum_member(expr)
                    && self.enum_member_declared_after(member, location)
                {
                    offenders.push(expr);
                }
            }
            _ => {}
        }
    }

    /// The enum member `evaluateEntity` resolves `expr` to: an identifier
    /// naming one, `E.m` or `E["m"]` on an identifier naming the enum.
    /// Aliases and longer entity names decline (`resolveEntityName` follows
    /// them; this binder lookup does not).
    fn evaluated_enum_member(&self, expr: NodeId) -> Option<SymbolId> {
        let resolve = |at: NodeId, text: &str| {
            self.binder.resolve_name(self.nodes, self.node_map, at, text, SymbolFlags::VALUE)
        };
        let symbol = match self.node_map.get(expr)? {
            Node::Identifier(identifier) => resolve(expr, identifier.text)?,
            Node::PropertyAccessExpression(access) => {
                let tsr_ast::MemberName::Identifier(name) = access.name? else { return None };
                let root = access.expression?.node_id()?;
                let Some(Node::Identifier(root_name)) = self.node_map.get(root) else {
                    return None;
                };
                let enumeration = self.binder.merged_symbol(resolve(root, root_name.text)?);
                let record = self.binder.symbols().get(enumeration);
                if !record.flags.intersects(SymbolFlags::ENUM) {
                    return None;
                }
                *record.exports.get(name.text)?
            }
            Node::ElementAccessExpression(access) => {
                let name = match access.argument_expression? {
                    tsr_ast::Expression::StringLiteral(literal) => literal.text,
                    tsr_ast::Expression::NoSubstitutionTemplateLiteral(literal) => literal.text,
                    _ => return None,
                };
                let root = access.expression?.node_id()?;
                let Some(Node::Identifier(root_name)) = self.node_map.get(root) else {
                    return None;
                };
                let enumeration = self.binder.merged_symbol(resolve(root, root_name.text)?);
                let record = self.binder.symbols().get(enumeration);
                if !record.flags.intersects(SymbolFlags::ENUM) {
                    return None;
                }
                *record.exports.get(name)?
            }
            _ => return None,
        };
        let symbol = self.binder.merged_symbol(symbol);
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::ENUM_MEMBER)
            .then_some(symbol)
    }

    /// `!isBlockScopedNameDeclaredBeforeUse(declaration, location)` for an
    /// enum member declaration: same file and starting after `location`.
    /// A self-reference (`declaration == location`) is TS2565's arm, not
    /// this one.
    fn enum_member_declared_after(&self, member: SymbolId, location: NodeId) -> bool {
        let Some(declaration) = self.binder.symbols().get(member).value_declaration else {
            return false;
        };
        if declaration == location {
            return false;
        }
        if self.source_file_of_for_diagnostics(declaration)
            != self.source_file_of_for_diagnostics(location)
        {
            return false;
        }
        self.nodes.span(declaration).start > self.nodes.span(location).start
    }
}
