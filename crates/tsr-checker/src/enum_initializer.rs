//! TS18033 — `Type '{0}' is not assignable to type '{1}' as required for
//! computed enum member values.`
//!
//! `computeConstantEnumMemberValue`'s default arm (`checker.go:24019`): an
//! initializer of a non-const, non-ambient enum that the constant evaluator
//! does not fold is checked with `checkTypeAssignableTo(checkExpression(init),
//! numberType)`.
//!
//! "Does not fold" is the checker's evaluator answering `nil`
//! ([`Checker::evaluate_constant`], ported below: `evaluator.NewEvaluator`
//! with `evaluateEntity`, `checker.go:24024`). It replaced a syntactic
//! over-approximation (`docs/parity/notes/r4-templates.md` §3).

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
        if self.evaluate_constant(at, node).is_some() {
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

    /// TS1061 — `Enum member must have initializer.`
    ///
    /// `computeEnumMemberValue` (`checker.go:23958`) for a member without an
    /// initializer: in an ambient non-`const` enum it is computed and takes no
    /// value (`checker.go:23973`); otherwise it takes `autoValue`, which is `0`
    /// for the declaration's first member and the previous member's value
    /// plus one when that value is a number — and `nil` (this report, at the
    /// name) when it is not. `docs/parity/notes/r4-templates.md` §5.
    pub(crate) fn check_enum_member_auto_value(&mut self, node: NodeId) {
        let Some(Node::EnumMember(member)) = self.node_map.get(node) else { return };
        if member.initializer.is_some() {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        let Some(Node::EnumDeclaration(declaration)) = self.node_map.get(parent) else { return };
        let is_const = declaration.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::ConstKeyword)
        });
        if !is_const && self.is_ambient_declaration(parent) {
            return;
        }
        let Some(index) = declaration.members.iter().position(|m| m.node_id == Some(node)) else {
            return;
        };
        let Some(previous) = index.checked_sub(1).and_then(|i| declaration.members[i].node_id)
        else {
            return;
        };
        if matches!(self.enum_member_value_of(previous, 0), Some(EnumConstant::Number(_))) {
            return;
        }
        let Some(at) = member.name.node_id() else { return };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(file, Diagnostic::new(&messages::ENUM_MEMBER_MUST_HAVE_INITIALIZER, span));
    }

    /// TS18055 / TS18056 — the `isolatedModules` arms of
    /// `computeConstantEnumMemberValue` (`checker.go:24009`) and
    /// `computeEnumMemberValue` (`checker.go:23985`). A member whose
    /// initializer evaluates to a string not built from string syntax
    /// (`IsSyntacticallyString`) reports `'{0}' has a string type, but must
    /// have syntactically recognizable string syntax…` at the initializer; an
    /// initializer-less member after an initialized one whose value is not a
    /// number or was resolved through another file reports TS18056 at its
    /// name. `docs/parity/notes/r4-templates.md` §5.
    pub(crate) fn check_enum_member_isolated_modules(&mut self, node: NodeId) {
        if !self.isolated_modules {
            return;
        }
        let Some(Node::EnumMember(member)) = self.node_map.get(node) else { return };
        let Some(parent) = self.nodes.parent(node) else { return };
        let Some(Node::EnumDeclaration(declaration)) = self.node_map.get(parent) else { return };
        if let Some(initializer) = member.initializer.and_then(|e| e.node_id()) {
            let result = self.evaluate_constant_result(initializer, node);
            if !matches!(result.value, Some(EnumConstant::String(_))) || result.syntactically_string
            {
                return;
            }
            let enum_name = declaration.name.map_or("", |name| name.text);
            let member_name = self
                .binder
                .symbol_of(node)
                .map(|symbol| self.binder.symbols().get(symbol).name.to_string())
                .unwrap_or_default();
            let Some(file) = self.source_file_of_for_diagnostics(initializer) else { return };
            let span = self.error_span(initializer);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_HAS_A_STRING_TYPE_BUT_MUST_HAVE_SYNTACTICALLY_RECOGNIZABLE_STRING_SYNTAX_WHEN_ISOLATEDMODULES_IS_ENABLED,
                    span,
                    [format!("{enum_name}.{member_name}")],
                ),
            );
            return;
        }
        let is_const = declaration.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::ConstKeyword)
        });
        if !is_const && self.is_ambient_declaration(parent) {
            return;
        }
        let Some(index) = declaration.members.iter().position(|m| m.node_id == Some(node)) else {
            return;
        };
        let Some(previous) = index.checked_sub(1).and_then(|i| declaration.members[i].node_id)
        else {
            return;
        };
        // `autoValue == nil` is TS1061's arm, which returns first.
        if !matches!(self.enum_member_value_of(previous, 0), Some(EnumConstant::Number(_))) {
            return;
        }
        let Some(Node::EnumMember(previous_member)) = self.node_map.get(previous) else { return };
        let Some(previous_initializer) = previous_member.initializer.and_then(|e| e.node_id())
        else {
            return;
        };
        let result = self.evaluate_constant_result(previous_initializer, previous);
        if matches!(result.value, Some(EnumConstant::Number(_))) && !result.resolved_other_files {
            return;
        }
        let Some(at) = member.name.node_id() else { return };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::new(
                &messages::ENUM_MEMBER_FOLLOWING_A_NON_LITERAL_NUMERIC_MEMBER_MUST_HAVE_AN_INITIALIZER_WHEN_ISOLATEDMODULES_IS_ENABLED,
                span,
            ),
        );
    }

    /// `enumMemberLinks.value` of one member declaration
    /// (`computeEnumMemberValue`'s result): the published declared literal
    /// type's value for a member the binder named, and for a member it
    /// could not (a non-literal computed name, which this port's enum type
    /// skips) the same computation done here — its initializer evaluated
    /// with it as `location`, or the auto value from its predecessor.
    fn enum_member_value_of(&mut self, member: NodeId, depth: u32) -> Option<EnumConstant> {
        if depth > EVALUATE_DEPTH_LIMIT {
            return None;
        }
        let Some(Node::EnumMember(node)) = self.node_map.get(member) else { return None };
        if let Some(symbol) = self.binder.symbol_of(member)
            && !(matches!(node.name, tsr_ast::PropertyName::ComputedPropertyName(_))
                && self.binder.symbols().get(symbol).name == "__computed")
        {
            return self.enum_member_constant(self.binder.merged_symbol(symbol));
        }
        if let Some(initializer) = node.initializer.and_then(|e| e.node_id()) {
            return self.evaluate_constant(initializer, member);
        }
        let parent = self.nodes.parent(member)?;
        let Some(Node::EnumDeclaration(declaration)) = self.node_map.get(parent) else {
            return None;
        };
        let is_const = declaration.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::ConstKeyword)
        });
        if !is_const && self.is_ambient_declaration(parent) {
            return None;
        }
        let index = declaration.members.iter().position(|m| m.node_id == Some(member))?;
        let Some(previous) = index.checked_sub(1).and_then(|i| declaration.members[i].node_id)
        else {
            return Some(EnumConstant::Number(0.0));
        };
        match self.enum_member_value_of(previous, depth + 1)? {
            EnumConstant::Number(value) => Some(EnumConstant::Number(value + 1.0)),
            EnumConstant::String(_) => None,
        }
    }

    /// TS2477 / TS2478 — `computeConstantEnumMemberValue`'s const arm
    /// (`checker.go:24001`): a `const` enum member whose initializer evaluates
    /// to a non-finite number reports at the initializer, `NaN` with its own
    /// message. Evaluated by [`Checker::evaluate_constant`] with the member
    /// as `location`.
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
        let Some(EnumConstant::Number(value)) = self.evaluate_constant(at, node) else { return };
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

    /// TS2565 / TS2651 — `evaluateEnumMember` (`checker.go:24077`), reached
    /// when `computeEnumMemberValues` evaluates an initializer with the member
    /// as `location`: a member reading itself is `Property '{0}' is used
    /// before being assigned.`; one declared after the usage
    /// (`isBlockScopedNameDeclaredBeforeUse`, `checker.go:1922`; an ambient
    /// usage is always before) is `A member initializer in a enum declaration
    /// cannot reference members declared after it, including members defined
    /// in other enums.` Both at the reference, in the evaluator's visit order.
    /// `docs/parity/notes/r4-templates.md` §4.
    pub(crate) fn check_enum_member_forward_references(&mut self, node: NodeId) {
        let Some(Node::EnumMember(member)) = self.node_map.get(node) else { return };
        let Some(initializer) = member.initializer.and_then(|initializer| initializer.node_id())
        else {
            return;
        };
        let mut sink = EvaluationSink { reports: Some(Vec::new()), flags: false };
        self.evaluate_enum_constant(initializer, node, 0, &mut sink);
        for report in sink.reports.unwrap_or_default() {
            let (at, diagnostic) = match report {
                EnumMemberReport::UsedBeforeAssigned(at, symbol) => {
                    // `symbolToString(symbol)` of an enum member: its name
                    // (the baseline spells `Property 'B'` for `B = E.B`).
                    let name = self.binder.symbols().get(symbol).name.to_string();
                    let span = self.error_span(at);
                    (
                        at,
                        Diagnostic::with_args(
                            &messages::PROPERTY_0_IS_USED_BEFORE_BEING_ASSIGNED,
                            span,
                            [name],
                        ),
                    )
                }
                EnumMemberReport::DeclaredAfter(at) => {
                    let span = self.error_span(at);
                    (
                        at,
                        Diagnostic::new(
                            &messages::A_MEMBER_INITIALIZER_IN_A_ENUM_DECLARATION_CANNOT_REFERENCE_MEMBERS_DECLARED_AFTER_IT_INCLUDING_MEMBERS_DEFINED_IN_OTHER_ENUMS,
                            span,
                        ),
                    )
                }
            };
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            self.report(file, diagnostic);
        }
    }
}

/// `evaluator.Result.Value` for the two kinds an enum member can hold
/// (`evaluator.go:11`). The `IsSyntacticallyString`, `ResolvedOtherFiles`
/// and `HasExternalReferences` bits are not carried: their only consumers
/// are the `isolatedModules` reports (TS18055, TS18056), which this port
/// does not make from the value computation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EnumConstant {
    Number(f64),
    String(String),
}

impl EnumConstant {
    /// `evaluator.AnyToString` (`evaluator.go:137`).
    fn render(&self) -> String {
        match self {
            EnumConstant::Number(value) => tsr_core::jsnum::format_number(*value),
            EnumConstant::String(value) => value.clone(),
        }
    }
}

/// `evaluator.Result` (`evaluator.go:11`), with the value narrowed to the
/// kinds an enum member holds.
#[derive(Clone, Debug, Default)]
pub(crate) struct Evaluation {
    pub(crate) value: Option<EnumConstant>,
    pub(crate) syntactically_string: bool,
    pub(crate) resolved_other_files: bool,
    pub(crate) has_external_references: bool,
}

impl Evaluation {
    fn value(value: EnumConstant) -> Self {
        Evaluation { value: Some(value), ..Evaluation::default() }
    }

    fn syntactic_string(text: String) -> Self {
        Evaluation {
            value: Some(EnumConstant::String(text)),
            syntactically_string: true,
            ..Evaluation::default()
        }
    }
}

/// What one evaluation collects besides its value: the `evaluateEnumMember`
/// reports (when the member check asks), and whether member reads need their
/// own `Result` bits (only the `isolatedModules` reports read them).
pub(crate) struct EvaluationSink {
    reports: Option<Vec<EnumMemberReport>>,
    flags: bool,
}

/// A report `evaluateEnumMember` (`checker.go:24077`) makes while evaluating.
#[derive(Clone, Copy, Debug)]
pub(crate) enum EnumMemberReport {
    /// TS2565 at the reference: the member reads itself (or has no value
    /// declaration).
    UsedBeforeAssigned(NodeId, SymbolId),
    /// TS2651 at the reference: the member is declared after the usage.
    DeclaredAfter(NodeId),
}

/// `jsnum.Number.toInt32` (`jsnum.go:52`): truncate, wrap modulo 2^32;
/// non-finite values map to zero.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "ECMAScript ToInt32 truncates, wraps modulo 2^32, and reinterprets the sign bit"
)]
fn to_int32(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.trunc().rem_euclid(4_294_967_296.0) as u32 as i32
}

/// The masked shift count of `jsnum`'s shift operators (`ToUint32(b) & 31`).
#[expect(clippy::cast_sign_loss, reason = "ECMAScript ToUint32 reinterprets the sign bit")]
fn shift_count(value: f64) -> u32 {
    (to_int32(value) as u32) & 31
}

/// `jsnum.Number.Exponentiate` (`jsnum.go`): JavaScript's `**`, which differs
/// from IEEE `pow` where the exponent is `NaN` or the base is `±1` with an
/// infinite exponent — both `NaN`.
#[expect(clippy::float_cmp, reason = "`±1` is an exact case of the ECMAScript algorithm")]
fn exponentiate(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() || (base.abs() == 1.0 && exponent.is_infinite()) {
        return f64::NAN;
    }
    base.powf(exponent)
}

/// Recursion bound for [`Checker::evaluate_enum_constant`]: a constant
/// variable chain across files has no before-use order to stop it.
const EVALUATE_DEPTH_LIMIT: u32 = 64;

impl Checker<'_, '_> {
    /// `computeConstantEnumMemberValue`'s `c.evaluate(initializer, member)`
    /// (`checker.go:23997`): the value an enum member's initializer folds
    /// to, or `None` for a computed member. Called by
    /// [`Checker::get_declared_type_of_enum`]'s member loop, which is this
    /// port's `computeEnumMemberValues` (`checker.go:23938`).
    ///
    /// Port record (`docs/conventions.md`, checker ports): no cache, side
    /// table or mapper of its own. Upstream memoises each member's value on
    /// `enumMemberLinks.value`; this port's single owner of that value is the
    /// member's declared literal type (`declared_types`, published by the
    /// enum's member loop), which [`Checker::enum_member_constant`] reads
    /// back. `docs/parity/notes/r4-templates.md` §1.
    pub(crate) fn evaluate_enum_initializer(
        &mut self,
        initializer: NodeId,
        member: NodeId,
    ) -> Option<EnumConstant> {
        self.evaluate_constant(initializer, member)
    }

    /// `c.evaluate(expr, location)` (`checker.go:931`): the checker's
    /// constant evaluator, entry for every consumer (enum member values,
    /// `checkTemplateExpression`). The value only; the `Result` bits are
    /// [`Checker::evaluate_constant_result`]'s.
    pub(crate) fn evaluate_constant(
        &mut self,
        expr: NodeId,
        location: NodeId,
    ) -> Option<EnumConstant> {
        let mut sink = EvaluationSink { reports: None, flags: false };
        self.evaluate_enum_constant(expr, location, 0, &mut sink).value
    }

    /// `c.evaluate(expr, location)` with the whole `evaluator.Result`: an
    /// enum member read carries that member's own bits, which are
    /// recomputed from its initializer (`enumMemberLinks.value` holds them
    /// upstream; this port publishes only the value, §1).
    fn evaluate_constant_result(&mut self, expr: NodeId, location: NodeId) -> Evaluation {
        let mut sink = EvaluationSink { reports: None, flags: true };
        self.evaluate_enum_constant(expr, location, 0, &mut sink)
    }

    /// The checker's evaluator (`c.evaluate`, built by
    /// `evaluator.NewEvaluator(c.evaluateEntity, OEKParentheses)`,
    /// `evaluator.go:24`), arm for arm.
    fn evaluate_enum_constant(
        &mut self,
        expr: NodeId,
        location: NodeId,
        depth: u32,
        sink: &mut EvaluationSink,
    ) -> Evaluation {
        let none = Evaluation::default();
        if depth > EVALUATE_DEPTH_LIMIT {
            return none;
        }
        let mut expr = expr;
        while let Some(Node::ParenthesizedExpression(wrapper)) = self.node_map.get(expr) {
            let Some(inner) = wrapper.expression.and_then(|e| e.node_id()) else { return none };
            expr = inner;
        }
        let Some(node) = self.node_map.get(expr) else { return none };
        match node {
            Node::PrefixUnaryExpression(unary) => {
                let Some(operand) = unary.operand.and_then(|e| e.node_id()) else { return none };
                let result = self.evaluate_enum_constant(operand, location, depth + 1, sink);
                let flags = Evaluation {
                    value: None,
                    syntactically_string: false,
                    resolved_other_files: result.resolved_other_files,
                    has_external_references: result.has_external_references,
                };
                let Some(EnumConstant::Number(value)) = result.value else { return flags };
                let value = match unary.operator.kind {
                    SyntaxKind::PlusToken => value,
                    SyntaxKind::MinusToken => -value,
                    SyntaxKind::TildeToken => f64::from(!to_int32(value)),
                    _ => return flags,
                };
                Evaluation { value: Some(EnumConstant::Number(value)), ..flags }
            }
            Node::BinaryExpression(binary) => {
                // Both operands are evaluated whatever the operator: an enum
                // member read on either side is an evaluation (and a TS2651
                // visit).
                let left = match binary.left.and_then(|e| e.node_id()) {
                    Some(at) => self.evaluate_enum_constant(at, location, depth + 1, sink),
                    None => Evaluation::default(),
                };
                let right = match binary.right.and_then(|e| e.node_id()) {
                    Some(at) => self.evaluate_enum_constant(at, location, depth + 1, sink),
                    None => Evaluation::default(),
                };
                let Some(operator) = binary.operator_token.map(|token| token.kind) else {
                    return none;
                };
                let flags = Evaluation {
                    value: None,
                    syntactically_string: (left.syntactically_string || right.syntactically_string)
                        && operator == SyntaxKind::PlusToken,
                    resolved_other_files: left.resolved_other_files || right.resolved_other_files,
                    has_external_references: left.has_external_references
                        || right.has_external_references,
                };
                let (Some(left), Some(right)) = (left.value, right.value) else { return flags };
                let value = match (left, right) {
                    (EnumConstant::Number(a), EnumConstant::Number(b)) => {
                        EnumConstant::Number(match operator {
                            SyntaxKind::BarToken => f64::from(to_int32(a) | to_int32(b)),
                            SyntaxKind::AmpersandToken => f64::from(to_int32(a) & to_int32(b)),
                            SyntaxKind::GreaterThanGreaterThanToken => {
                                f64::from(to_int32(a) >> shift_count(b))
                            }
                            SyntaxKind::GreaterThanGreaterThanGreaterThanToken => {
                                #[expect(
                                    clippy::cast_sign_loss,
                                    reason = "ECMAScript ToUint32 reinterprets the sign bit"
                                )]
                                let unsigned = to_int32(a) as u32;
                                f64::from(unsigned >> shift_count(b))
                            }
                            SyntaxKind::LessThanLessThanToken => {
                                f64::from(to_int32(a).wrapping_shl(shift_count(b)))
                            }
                            SyntaxKind::CaretToken => f64::from(to_int32(a) ^ to_int32(b)),
                            SyntaxKind::AsteriskToken => a * b,
                            SyntaxKind::SlashToken => a / b,
                            SyntaxKind::PlusToken => a + b,
                            SyntaxKind::MinusToken => a - b,
                            SyntaxKind::PercentToken => a % b,
                            SyntaxKind::AsteriskAsteriskToken => exponentiate(a, b),
                            _ => return flags,
                        })
                    }
                    (left, right) if operator == SyntaxKind::PlusToken => {
                        EnumConstant::String(left.render() + &right.render())
                    }
                    _ => return flags,
                };
                Evaluation { value: Some(value), ..flags }
            }
            Node::StringLiteral(literal) => Evaluation::syntactic_string(literal.text.to_string()),
            Node::NoSubstitutionTemplateLiteral(literal) => {
                Evaluation::syntactic_string(literal.text.to_string())
            }
            // `evaluateTemplateExpression` (`evaluator.go:118`).
            Node::TemplateExpression(template) => {
                let unfolded = Evaluation { syntactically_string: true, ..Evaluation::default() };
                let Some(head) = template.head else { return unfolded };
                let mut text = head.text.to_string();
                let mut resolved_other_files = false;
                let mut has_external_references = false;
                for span in template.template_spans {
                    let Some(inner) = span.expression.and_then(|e| e.node_id()) else {
                        return unfolded;
                    };
                    let result = self.evaluate_enum_constant(inner, location, depth + 1, sink);
                    let Some(value) = result.value else { return unfolded };
                    text.push_str(&value.render());
                    let Some(literal) = span.literal else { return unfolded };
                    text.push_str(match literal {
                        tsr_ast::TemplateMiddleOrTail::TemplateMiddle(part) => part.text,
                        tsr_ast::TemplateMiddleOrTail::TemplateTail(part) => part.text,
                    });
                    resolved_other_files |= result.resolved_other_files;
                    has_external_references |= result.has_external_references;
                }
                Evaluation {
                    value: Some(EnumConstant::String(text)),
                    syntactically_string: true,
                    resolved_other_files,
                    has_external_references,
                }
            }
            Node::NumericLiteral(literal) => Evaluation::value(EnumConstant::Number(
                tsr_core::jsnum::numeric_value(literal.text),
            )),
            Node::Identifier(_) => self.evaluate_enum_entity(expr, location, depth, sink),
            Node::PropertyAccessExpression(tsr_ast::PropertyAccessExpression {
                expression,
                ..
            })
            | Node::ElementAccessExpression(tsr_ast::ElementAccessExpression {
                expression, ..
            }) => {
                if !expression
                    .and_then(|e| e.node_id())
                    .is_some_and(|root| self.is_entity_name_expression(root))
                {
                    return none;
                }
                self.evaluate_enum_entity(expr, location, depth, sink)
            }
            _ => none,
        }
    }

    /// `Checker.evaluateEntity` (`checker.go:24024`).
    fn evaluate_enum_entity(
        &mut self,
        expr: NodeId,
        location: NodeId,
        depth: u32,
        sink: &mut EvaluationSink,
    ) -> Evaluation {
        self.evaluate_enum_entity_inner(expr, location, depth, sink).unwrap_or_default()
    }

    fn evaluate_enum_entity_inner(
        &mut self,
        expr: NodeId,
        location: NodeId,
        depth: u32,
        sink: &mut EvaluationSink,
    ) -> Option<Evaluation> {
        if let Some(Node::ElementAccessExpression(access)) = self.node_map.get(expr) {
            let name = match access.argument_expression? {
                tsr_ast::Expression::StringLiteral(literal) => literal.text,
                tsr_ast::Expression::NoSubstitutionTemplateLiteral(literal) => literal.text,
                _ => return None,
            };
            let root = access.expression?.node_id()?;
            let root_symbol =
                self.resolve_entity_name_expression_value(root, SymbolFlags::VALUE)?;
            if !self.binder.symbols().get(root_symbol).flags.intersects(SymbolFlags::ENUM) {
                return None;
            }
            let member = *self.binder.symbols().get(root_symbol).exports.get(name)?;
            return Some(self.evaluate_enum_member_reference(expr, member, location, depth, sink));
        }
        let symbol = self.resolve_entity_name_expression_value(expr, SymbolFlags::VALUE)?;
        if let Some(Node::Identifier(identifier)) = self.node_map.get(expr)
            && matches!(identifier.text, "Infinity" | "NaN")
            && self.binder.global(identifier.text) == Some(symbol)
        {
            return Some(Evaluation::value(EnumConstant::Number(tsr_core::jsnum::numeric_value(
                identifier.text,
            ))));
        }
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(SymbolFlags::ENUM_MEMBER) {
            return Some(self.evaluate_enum_member_reference(expr, symbol, location, depth, sink));
        }
        if self.is_constant_variable(symbol) {
            let declaration = self.binder.symbols().get(symbol).value_declaration?;
            let Some(Node::VariableDeclaration(variable)) = self.node_map.get(declaration) else {
                return None;
            };
            if variable.r#type.is_some()
                || declaration == location
                || !self.enum_evaluation_declared_before_use(declaration, location)
            {
                return None;
            }
            let initializer = variable.initializer?.node_id()?;
            let result = self.evaluate_enum_constant(initializer, declaration, depth + 1, sink);
            if self.source_file_of_for_diagnostics(location)
                != self.source_file_of_for_diagnostics(declaration)
            {
                return Some(Evaluation {
                    value: result.value,
                    syntactically_string: false,
                    resolved_other_files: true,
                    has_external_references: true,
                });
            }
            return Some(Evaluation { has_external_references: true, ..result });
        }
        None
    }

    /// `Checker.evaluateEnumMember` (`checker.go:24077`). Its TS2565/TS2651
    /// reports are collected into the sink when the caller asks
    /// ([`Checker::check_enum_member_forward_references`], the member check);
    /// the declared-type computation evaluates with no sink, so each report
    /// is made once, by the member's own check.
    fn evaluate_enum_member_reference(
        &mut self,
        expr: NodeId,
        symbol: SymbolId,
        location: NodeId,
        depth: u32,
        sink: &mut EvaluationSink,
    ) -> Evaluation {
        let symbol = self.binder.merged_symbol(symbol);
        let declaration = self.binder.symbols().get(symbol).value_declaration;
        let Some(declaration) = declaration.filter(|&declaration| declaration != location) else {
            if let Some(reports) = &mut sink.reports {
                reports.push(EnumMemberReport::UsedBeforeAssigned(expr, symbol));
            }
            return Evaluation::default();
        };
        if !self.enum_evaluation_declared_before_use(declaration, location) {
            if let Some(reports) = &mut sink.reports {
                reports.push(EnumMemberReport::DeclaredAfter(expr));
            }
            return Evaluation::value(EnumConstant::Number(0.0));
        }
        let value = self.enum_member_constant(symbol);
        // The member's own `Result` bits, recomputed from its initializer
        // (an auto-valued member's are all false).
        let mut result = Evaluation { value, ..Evaluation::default() };
        if sink.flags
            && let Some(Node::EnumMember(member)) = self.node_map.get(declaration)
            && let Some(initializer) = member.initializer.and_then(|e| e.node_id())
        {
            let mut own = EvaluationSink { reports: None, flags: true };
            let bits = self.evaluate_enum_constant(initializer, declaration, depth + 1, &mut own);
            result.syntactically_string = bits.syntactically_string;
            result.resolved_other_files = bits.resolved_other_files;
            result.has_external_references = bits.has_external_references;
        }
        if self.nodes.parent(location) != self.nodes.parent(declaration) {
            result.has_external_references = true;
        }
        result
    }

    /// `Checker.getEnumMemberValue` (`checker.go:23930`) for a member symbol:
    /// its enum's values are computed (`computeEnumMemberValues`) and the
    /// member's is read back from its declared literal type.
    ///
    /// Publication: a member's declared type is published by its enum's
    /// member loop as soon as its value is folded. Upstream sets
    /// `NodeCheckFlagsEnumValuesComputed` *before* the loop, so a re-entrant
    /// read of a member not yet folded answers the zero `Result` (`nil`).
    /// The same state here is the enum's `DeclaredType` resolution frame,
    /// which [`Checker::get_declared_type_of_enum`] holds for the loop: while
    /// it is on the stack the enum is not forced again and an unpublished
    /// member answers `None`. Upstream's flag is per declaration and this
    /// port computes a merged enum's declarations together, so a member of
    /// a *later* declaration of the enum being computed is also `None`
    /// (upstream would compute that declaration's values first).
    fn enum_member_constant(&mut self, symbol: SymbolId) -> Option<EnumConstant> {
        let ty = if let Some(&ty) = self.declared_types.get(&symbol) {
            ty
        } else {
            let parent = self.binder.symbols().get(symbol).parent?;
            let parent = self.binder.merged_symbol(parent);
            if self.resolutions.on_stack(parent, crate::resolution::PropertyName::DeclaredType) {
                return None;
            }
            self.get_declared_type_of_symbol(parent);
            *self.declared_types.get(&symbol)?
        };
        let regular = self.enum_member_regular.get(&ty).copied().unwrap_or(ty);
        let crate::types::TypeData::EnumLiteral { value, .. } = &self.store.get(regular).data
        else {
            return None;
        };
        match value {
            crate::types::EnumLiteralValue::String(text) => {
                Some(EnumConstant::String(text.clone()))
            }
            crate::types::EnumLiteralValue::Number(text) => {
                Some(EnumConstant::Number(text.parse::<f64>().ok()?))
            }
        }
    }

    /// `resolveEntityName(expr, meaning, ignoreErrors=true)`
    /// (`checker.go:15772`) for an entity-name *expression*: an identifier
    /// resolves through the scope walk, `a.b` resolves `a` as a namespace
    /// and reads `b` from its exports (`resolveQualifiedName`,
    /// `checker.go:15828`), and the result is followed along its alias
    /// chain until it carries `meaning`. Not ported: `getExportsOfSymbol`'s
    /// `export *` re-exports and the `CommonJS` `require` redirect — each a
    /// `None`, never a different symbol.
    fn resolve_entity_name_expression_value(
        &mut self,
        expr: NodeId,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        let symbol = match self.node_map.get(expr)? {
            Node::Identifier(identifier) => {
                self.resolve_name_with_export_alias(expr, identifier.text, meaning)?
            }
            Node::PropertyAccessExpression(access) => {
                let tsr_ast::MemberName::Identifier(name) = access.name? else { return None };
                let left = access.expression?.node_id()?;
                let namespace =
                    self.resolve_entity_name_expression_value(left, SymbolFlags::NAMESPACE)?;
                let namespace = self.resolve_external_module_symbol(namespace);
                let namespace = self.binder.merged_symbol(namespace);
                *self.binder.symbols().get(namespace).exports.get(name.text)?
            }
            _ => return None,
        };
        let symbol = self.binder.merged_symbol(symbol);
        let symbol = if self.binder.symbols().get(symbol).flags.intersects(meaning) {
            symbol
        } else {
            self.resolve_alias_fully(symbol)
        };
        let symbol = self.binder.merged_symbol(symbol);
        self.binder.symbols().get(symbol).flags.intersects(meaning).then_some(symbol)
    }

    /// `isBlockScopedNameDeclaredBeforeUse(declaration, usage)`
    /// (`checker.go:1922`) for the two declarations the evaluator asks about
    /// — an enum member or a constant variable — with an enum member or a
    /// variable declaration as `usage`. Ported arms: another file, an
    /// ambient usage, declared earlier (a variable used inside its own
    /// declaration is not), and the deferred use inside a function between
    /// the usage and the declaration's container
    /// (`isUsedInFunctionOrInstanceProperty`'s function arm). Not ported:
    /// that function's class-property arms, which no enum member or
    /// variable-declaration usage can sit under without a function between.
    fn enum_evaluation_declared_before_use(&self, declaration: NodeId, usage: NodeId) -> bool {
        if self.source_file_of_for_diagnostics(declaration)
            != self.source_file_of_for_diagnostics(usage)
        {
            return true;
        }
        if self.is_ambient_declaration(usage) {
            return true;
        }
        if self.nodes.span(declaration).start <= self.nodes.span(usage).start {
            return !(self.nodes.kind(declaration) == SyntaxKind::VariableDeclaration
                && self.nodes.ancestors(usage).any(|ancestor| ancestor == declaration));
        }
        let declaration_ancestors: Vec<NodeId> = self.nodes.ancestors(declaration).collect();
        std::iter::once(usage)
            .chain(self.nodes.ancestors(usage))
            .take_while(|current| !declaration_ancestors.contains(current))
            .any(|current| {
                self.is_function_like_or_static_block(current)
                    && self.nodes.kind(current) != SyntaxKind::ClassStaticBlockDeclaration
            })
    }
}
