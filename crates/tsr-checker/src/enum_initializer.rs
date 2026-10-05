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
use tsr_binder::SymbolFlags;
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
}
