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
    /// `checkTemplateExpression`).
    pub(crate) fn evaluate_constant(
        &mut self,
        expr: NodeId,
        location: NodeId,
    ) -> Option<EnumConstant> {
        self.evaluate_enum_constant(expr, location, 0)
    }

    /// The checker's evaluator (`c.evaluate`, built by
    /// `evaluator.NewEvaluator(c.evaluateEntity, OEKParentheses)`,
    /// `evaluator.go:24`), arm for arm.
    fn evaluate_enum_constant(
        &mut self,
        expr: NodeId,
        location: NodeId,
        depth: u32,
    ) -> Option<EnumConstant> {
        if depth > EVALUATE_DEPTH_LIMIT {
            return None;
        }
        let mut expr = expr;
        while let Some(Node::ParenthesizedExpression(wrapper)) = self.node_map.get(expr) {
            expr = wrapper.expression?.node_id()?;
        }
        match self.node_map.get(expr)? {
            Node::PrefixUnaryExpression(unary) => {
                let operand = unary.operand?.node_id()?;
                let EnumConstant::Number(value) =
                    self.evaluate_enum_constant(operand, location, depth + 1)?
                else {
                    return None;
                };
                match unary.operator.kind {
                    SyntaxKind::PlusToken => Some(EnumConstant::Number(value)),
                    SyntaxKind::MinusToken => Some(EnumConstant::Number(-value)),
                    SyntaxKind::TildeToken => {
                        Some(EnumConstant::Number(f64::from(!to_int32(value))))
                    }
                    _ => None,
                }
            }
            Node::BinaryExpression(binary) => {
                // Both operands are evaluated whatever the operator: an enum
                // member read on either side is an evaluation (and TS2651's
                // visit, `collect_enum_forward_references`).
                let left = binary
                    .left
                    .and_then(|e| e.node_id())
                    .and_then(|at| self.evaluate_enum_constant(at, location, depth + 1));
                let right = binary
                    .right
                    .and_then(|e| e.node_id())
                    .and_then(|at| self.evaluate_enum_constant(at, location, depth + 1));
                let operator = binary.operator_token?.kind;
                match (left?, right?) {
                    (EnumConstant::Number(a), EnumConstant::Number(b)) => {
                        let value = match operator {
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
                            _ => return None,
                        };
                        Some(EnumConstant::Number(value))
                    }
                    (left, right) if operator == SyntaxKind::PlusToken => {
                        Some(EnumConstant::String(left.render() + &right.render()))
                    }
                    _ => None,
                }
            }
            Node::StringLiteral(literal) => Some(EnumConstant::String(literal.text.to_string())),
            Node::NoSubstitutionTemplateLiteral(literal) => {
                Some(EnumConstant::String(literal.text.to_string()))
            }
            // `evaluateTemplateExpression` (`evaluator.go:118`).
            Node::TemplateExpression(template) => {
                let mut text = template.head?.text.to_string();
                for span in template.template_spans {
                    let inner = span.expression?.node_id()?;
                    let value = self.evaluate_enum_constant(inner, location, depth + 1)?;
                    text.push_str(&value.render());
                    text.push_str(match span.literal? {
                        tsr_ast::TemplateMiddleOrTail::TemplateMiddle(part) => part.text,
                        tsr_ast::TemplateMiddleOrTail::TemplateTail(part) => part.text,
                    });
                }
                Some(EnumConstant::String(text))
            }
            Node::NumericLiteral(literal) => {
                Some(EnumConstant::Number(tsr_core::jsnum::numeric_value(literal.text)))
            }
            Node::Identifier(_) => self.evaluate_enum_entity(expr, location, depth),
            Node::PropertyAccessExpression(access) => {
                let root = access.expression?.node_id()?;
                if !self.is_entity_name_expression(root) {
                    return None;
                }
                self.evaluate_enum_entity(expr, location, depth)
            }
            Node::ElementAccessExpression(access) => {
                let root = access.expression?.node_id()?;
                if !self.is_entity_name_expression(root) {
                    return None;
                }
                self.evaluate_enum_entity(expr, location, depth)
            }
            _ => None,
        }
    }

    /// `Checker.evaluateEntity` (`checker.go:24024`).
    fn evaluate_enum_entity(
        &mut self,
        expr: NodeId,
        location: NodeId,
        depth: u32,
    ) -> Option<EnumConstant> {
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
            return self.evaluate_enum_member_reference(expr, member, location);
        }
        let symbol = self.resolve_entity_name_expression_value(expr, SymbolFlags::VALUE)?;
        if let Some(Node::Identifier(identifier)) = self.node_map.get(expr)
            && matches!(identifier.text, "Infinity" | "NaN")
            && self.binder.global(identifier.text) == Some(symbol)
        {
            return Some(EnumConstant::Number(tsr_core::jsnum::numeric_value(identifier.text)));
        }
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(SymbolFlags::ENUM_MEMBER) {
            return self.evaluate_enum_member_reference(expr, symbol, location);
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
            return self.evaluate_enum_constant(initializer, declaration, depth + 1);
        }
        None
    }

    /// `Checker.evaluateEnumMember` (`checker.go:24077`), value only: the
    /// TS2565/TS2651 reports it makes are
    /// [`Checker::check_enum_member_forward_references`]'s, issued from the
    /// member check rather than from the declared-type computation.
    fn evaluate_enum_member_reference(
        &mut self,
        _expr: NodeId,
        symbol: SymbolId,
        location: NodeId,
    ) -> Option<EnumConstant> {
        let symbol = self.binder.merged_symbol(symbol);
        let declaration = self.binder.symbols().get(symbol).value_declaration?;
        if declaration == location {
            return None;
        }
        if !self.enum_evaluation_declared_before_use(declaration, location) {
            return Some(EnumConstant::Number(0.0));
        }
        self.enum_member_constant(symbol)
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
