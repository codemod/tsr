//! Folding enum member values.
//!
//! Stands in for `EmitResolver.GetEnumMemberValue`
//! (`internal/printer/emitresolver.go:111`), which upstream answers from the
//! checker's evaluator (`internal/evaluator`).
//!
//! # Why a `.d.ts` cannot just keep the initializer as written
//!
//! `transformEnumDeclaration` (`transform.go:2262`) rewrites **every** member's
//! initializer to the constant the checker computed, and drops it entirely when
//! there is no constant. The comment upstream reads "Rewrite enum values to their
//! constants, if available", and three consequences follow that are visible in
//! any baseline:
//!
//! | source | `.d.ts` |
//! |---|---|
//! | `enum E { A, B }` | `enum E { A = 0, B = 1 }` |
//! | `enum F { A = 1, B = 2, C = A \| B }` | `enum F { A = 1, B = 2, C = 3 }` |
//! | `enum G { A = f() }` | `enum G { A }` |
//!
//! The first is the one that matters most, because it is every unannotated enum in
//! the corpus: an auto-numbered member emits its ordinal. Keeping the source form
//! is not a formatting difference — `enum E { A }` and `enum E { A = 0 }` are the
//! same type but not the same text, and this gate compares text.
//!
//! # What this evaluator covers, and why that is the right amount
//!
//! Exactly the constant-expression grammar `tsr_dts::rules`'s enum fold already
//! recognises, because that fold is what decides whether a member is reported as
//! `TS9020` and therefore whether the case is in this emitter's target at all. A
//! member this evaluator cannot fold is one of two things: reaching outside the
//! enum (reported, so the case is excluded) or genuinely non-constant (`f()`),
//! which emits with no value — the same output upstream produces.
//!
//! Numbers are `f64` throughout, as JavaScript's are. The bitwise operators
//! therefore truncate to `i32` the way `ToInt32` does, which is the one place a
//! naive implementation silently disagrees on large values.

use tsr_ast::{EnumMember, Expression, MemberName, SyntaxKind};

/// A folded enum member value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EnumValue {
    /// A numeric value, in JavaScript's only number type.
    Number(f64),
    /// A string value, from a string-valued member.
    String(String),
}

/// Fold every member of an enum, in declaration order.
///
/// `None` for a member means "no constant value", which emits with no
/// initializer. Auto-numbering continues from the previous *numeric* value, and
/// stops — as TypeScript does — once a member has no numeric predecessor.
pub(crate) fn fold_members(members: &[&EnumMember<'_>], enum_name: &str) -> Vec<Option<EnumValue>> {
    let mut folded: Vec<(String, Option<EnumValue>)> = Vec::with_capacity(members.len());
    let mut auto = Some(0.0_f64);

    for member in members {
        let key = member_key(&member.name);
        let value = match &member.initializer {
            None => {
                let next = auto.map(EnumValue::Number);
                auto = auto.map(|value| value + 1.0);
                next
            }
            Some(initializer) => {
                let value = evaluate(initializer, enum_name, &folded);
                auto = match &value {
                    Some(EnumValue::Number(number)) => Some(number + 1.0),
                    // A string member, or one with no constant value, ends
                    // auto-numbering: TypeScript requires an initializer on
                    // whatever follows.
                    _ => None,
                };
                value
            }
        };
        folded.push((key, value));
    }
    folded.into_iter().map(|(_, value)| value).collect()
}

/// The name a member is looked up by, for sibling references.
fn member_key(name: &tsr_ast::PropertyName<'_>) -> String {
    match name {
        tsr_ast::PropertyName::Identifier(identifier) => identifier.text.to_string(),
        tsr_ast::PropertyName::StringLiteral(literal) => literal.text.to_string(),
        tsr_ast::PropertyName::NumericLiteral(literal) => literal.text.to_string(),
        _ => String::new(),
    }
}

fn evaluate(
    expression: &Expression<'_>,
    enum_name: &str,
    siblings: &[(String, Option<EnumValue>)],
) -> Option<EnumValue> {
    let sibling = |name: &str| {
        siblings
            .iter()
            .find(|(candidate, _)| candidate == name)
            .and_then(|(_, value)| value.clone())
    };

    match expression {
        Expression::NumericLiteral(literal) => {
            Some(EnumValue::Number(crate::type_builder::numeric_value(literal.text)))
        }
        Expression::StringLiteral(literal) => Some(EnumValue::String(literal.text.to_string())),
        Expression::NoSubstitutionTemplateLiteral(literal) => {
            Some(EnumValue::String(literal.text.to_string()))
        }
        Expression::Identifier(identifier) => sibling(identifier.text),
        // `E.Member` and `E["Member"]` stay inside the enum; any other qualifier
        // reaches outside it, which is `TS9020` and so out of the target.
        Expression::PropertyAccessExpression(access) => match (&access.expression, &access.name) {
            (Some(Expression::Identifier(target)), Some(MemberName::Identifier(name)))
                if target.text == enum_name =>
            {
                sibling(name.text)
            }
            _ => None,
        },
        Expression::ElementAccessExpression(access) => {
            match (&access.expression, &access.argument_expression) {
                (
                    Some(Expression::Identifier(target)),
                    Some(Expression::StringLiteral(argument)),
                ) if target.text == enum_name => sibling(argument.text),
                _ => None,
            }
        }
        Expression::ParenthesizedExpression(inner) => {
            evaluate(inner.expression.as_ref()?, enum_name, siblings)
        }
        Expression::PrefixUnaryExpression(unary) => {
            let EnumValue::Number(operand) =
                evaluate(unary.operand.as_ref()?, enum_name, siblings)?
            else {
                return None;
            };
            Some(EnumValue::Number(match unary.operator.kind {
                SyntaxKind::PlusToken => operand,
                SyntaxKind::MinusToken => -operand,
                SyntaxKind::TildeToken => f64::from(!to_int32(operand)),
                _ => return None,
            }))
        }
        Expression::BinaryExpression(binary) => {
            let operator = binary.operator_token?.kind;
            let left = evaluate(binary.left.as_ref()?, enum_name, siblings)?;
            let right = evaluate(binary.right.as_ref()?, enum_name, siblings)?;
            match (left, right) {
                (EnumValue::Number(left), EnumValue::Number(right)) => {
                    Some(EnumValue::Number(match operator {
                        SyntaxKind::BarToken => f64::from(to_int32(left) | to_int32(right)),
                        SyntaxKind::AmpersandToken => f64::from(to_int32(left) & to_int32(right)),
                        SyntaxKind::CaretToken => f64::from(to_int32(left) ^ to_int32(right)),
                        SyntaxKind::LessThanLessThanToken => {
                            f64::from(to_int32(left) << (to_uint32(right) & 31))
                        }
                        SyntaxKind::GreaterThanGreaterThanToken => {
                            f64::from(to_int32(left) >> (to_uint32(right) & 31))
                        }
                        SyntaxKind::GreaterThanGreaterThanGreaterThanToken => {
                            f64::from(to_uint32(left) >> (to_uint32(right) & 31))
                        }
                        SyntaxKind::PlusToken => left + right,
                        SyntaxKind::MinusToken => left - right,
                        SyntaxKind::AsteriskToken => left * right,
                        SyntaxKind::SlashToken => left / right,
                        SyntaxKind::PercentToken => left % right,
                        SyntaxKind::AsteriskAsteriskToken => left.powf(right),
                        _ => return None,
                    }))
                }
                // Only `+` is defined on strings, and only string-to-string.
                (EnumValue::String(left), EnumValue::String(right))
                    if operator == SyntaxKind::PlusToken =>
                {
                    Some(EnumValue::String(format!("{left}{right}")))
                }
                _ => None,
            }
        }
        // A call, a template with substitutions, anything else: no constant value.
        // Upstream emits the member with no initializer at all, which is legal.
        _ => None,
    }
}

/// `ToInt32`, which is what JavaScript's bitwise operators do to their operands.
#[allow(clippy::cast_possible_wrap)]
fn to_int32(value: f64) -> i32 {
    to_uint32(value) as i32
}

/// `ToUint32`: truncate towards zero, then reduce modulo 2^32.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_uint32(value: f64) -> u32 {
    #[allow(clippy::float_cmp)]
    if !value.is_finite() || value == 0.0 {
        return 0;
    }
    let truncated = value.trunc();
    let wrapped = truncated.rem_euclid(4_294_967_296.0);
    wrapped as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_uint32_wraps_the_way_javascript_does() {
        assert_eq!(to_uint32(1.0), 1);
        assert_eq!(to_uint32(-1.0), u32::MAX);
        assert_eq!(to_uint32(4_294_967_296.0), 0);
        assert_eq!(to_uint32(f64::NAN), 0);
        assert_eq!(to_uint32(f64::INFINITY), 0);
        assert_eq!(to_int32(-1.0), -1);
    }
}
