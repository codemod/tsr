//! Building a type node from an expression, without a checker.
//!
//! # What upstream does here, and why none of it is portable
//!
//! This module stands in for `EmitResolver.CreateTypeOfDeclaration`
//! (`internal/printer/emitresolver.go:122`), whose implementation is the checker's
//! node builder: it computes the expression's `*Type` and then serializes that
//! type back into syntax. Nothing about that is reachable before Phase 4, and
//! [ADR-0021](../../../docs/adr/0021-isolated-declarations-is-not-a-port.md) is
//! the decision not to wait for it.
//!
//! So this goes the other way round: **from syntax to syntax, never through a
//! type.** `{ a: 1 }` becomes `{ a: number; }` by rewriting the literal's shape,
//! not by inferring `{ a: number }` and printing it.
//!
//! # The scope is not a judgement call
//!
//! The obvious risk in a syntactic type builder is that it quietly handles a
//! little more or a little less than the analysis admits, and the gap becomes a
//! wrong `.d.ts` instead of an error. So the scope is pinned to something
//! external: **this module handles exactly the expressions
//! [`tsr_dts::rules`]`::infer` returns `Ok` for.**
//!
//! | `infer` says | here |
//! |---|---|
//! | `Ok` — the type is apparent | a type node is produced |
//! | `Generic`/`Reported` — inference required | `None`, and the caller emits `any` |
//!
//! `tests/analysis_agreement.rs` asserts both directions over a table of
//! constructs. A construct the analysis accepts and this module refuses is a
//! silent `any` in the output; one the analysis rejects and this module accepts is
//! a guess presented as fact. Both are bugs, and neither is visible from inside
//! either crate alone.
//!
//! # Freshness, which is the whole difference between `const` and `let`
//!
//! `const x = 1` emits `1` and `let x = 1` emits `number`. Upstream distinguishes
//! them by literal-type freshness in the checker; here it is
//! [`Freshness`](crate::Freshness), threaded down through every recursion.
//! `as const` re-enters the const context mid-expression, exactly as
//! `tsr_dts::rules::infer` does.

use tsr_ast::{
    Expression, ModifierLike, Node, NodeFlags, PropertySignatureDeclaration, StringLiteral,
    SyntaxKind, TokenFlags, TypeElement, TypeNode, TypeOperatorNode, TypeReferenceNode,
};
use tsr_core::Span;

use crate::{Freshness, factory::Factory};

/// Ported from `ast.IsPrimitiveLiteralValue` (`internal/ast/utilities.go`), which
/// upstream calls with `includeBigInt = true` from `ensureNoInitializer`.
///
/// This decides whether a `const` declaration emits `= value` rather than a type,
/// so it is the difference between `declare const one = 1` and
/// `declare const one: number`.
#[must_use]
pub fn is_primitive_literal_value(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::NoSubstitutionTemplateLiteral(_) => true,
        Expression::KeywordExpression(keyword) => {
            matches!(keyword.kind, SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword)
        }
        // `-1` is a primitive literal value; `!x` and `+"a"` are not.
        Expression::PrefixUnaryExpression(unary) => {
            matches!(unary.operator.kind, SyntaxKind::MinusToken | SyntaxKind::PlusToken)
                && unary.operand.as_ref().is_some_and(|operand| {
                    matches!(operand, Expression::NumericLiteral(_) | Expression::BigIntLiteral(_))
                })
        }
        _ => false,
    }
}

/// Stands in for `EmitResolver.CreateLiteralConstValue`.
///
/// Upstream produces the literal from the *value* the checker computed, which is
/// why `0x1` comes back as `1` and `` `1` `` comes back as `"1"` in
/// `compiler/isolatedDeclarationsLiterals`'s baseline. Two normalisations are
/// therefore not optional and are done syntactically here:
///
/// - **A string-valued literal becomes a `StringLiteral`.** A template with no
///   substitutions has the same value as the equivalent string, and the baseline
///   writes the string form.
/// - **A numeric literal is re-spelled in decimal.** `0o1`, `0x1` and `1_0` all
///   have decimal values, and that is what the baseline carries.
///
/// A `+1` loses its sign, because `+1` and `1` are the same value; a `-1` keeps
/// it, and is reproduced as the original prefix expression.
pub(crate) fn literal_const_value<'a>(
    factory: &mut Factory<'a, '_>,
    expression: &Expression<'a>,
) -> Option<Expression<'a>> {
    let span = factory.span_of(expression.node_id());
    match expression {
        Expression::NumericLiteral(literal) => Some(decimal_literal(factory, literal.text, span)),
        Expression::NoSubstitutionTemplateLiteral(template) => {
            Some(string_literal(factory, template.text, span))
        }
        Expression::StringLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::KeywordExpression(_) => Some(*expression),
        Expression::PrefixUnaryExpression(unary) => {
            let operand = unary.operand.as_ref()?;
            if unary.operator.kind == SyntaxKind::PlusToken {
                // `+1` is `1`. Upstream never round-trips the plus, because it is
                // rebuilding from a value rather than from the text.
                return literal_const_value(factory, operand);
            }
            Some(*expression)
        }
        _ => None,
    }
}

/// The type of an expression, when syntax alone determines it.
///
/// `None` means inference was required. The caller — `ensureType`, ported in
/// [`crate::transform`] — turns that into `any`, exactly as upstream does when its
/// node builder returns nothing.
pub(crate) fn type_of_expression<'a>(
    factory: &mut Factory<'a, '_>,
    expression: &Expression<'a>,
    freshness: Freshness,
) -> Option<TypeNode<'a>> {
    let span = factory.span_of(expression.node_id());
    match expression {
        // A literal in a const context is its own type; widened, it is the base
        // primitive. This is the one place `Freshness` does real work, and every
        // other arm only threads it.
        Expression::NumericLiteral(_) | Expression::PrefixUnaryExpression(_) => {
            widen_or_literal(factory, expression, freshness, SyntaxKind::NumberKeyword, span)
        }
        Expression::BigIntLiteral(_) => {
            widen_or_literal(factory, expression, freshness, SyntaxKind::BigIntKeyword, span)
        }
        Expression::StringLiteral(_) => {
            widen_or_literal(factory, expression, freshness, SyntaxKind::StringKeyword, span)
        }
        Expression::NoSubstitutionTemplateLiteral(template) => {
            if freshness == Freshness::Widening {
                return Some(factory.keyword_type(SyntaxKind::StringKeyword, span));
            }
            // A no-substitution template's *type* is the string literal type, and
            // upstream writes it in string form.
            let literal = string_literal(factory, template.text, span);
            Some(literal_type(factory, Node::from(literal), span))
        }
        Expression::KeywordExpression(keyword) => match keyword.kind {
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword => {
                widen_or_literal(factory, expression, freshness, SyntaxKind::BooleanKeyword, span)
            }
            // `null` widens to `any`, in both contexts. `compiler/constDeclarations`
            // writes `const c3 = 0, c4: string, c5 = null` and its baseline reads
            // `declare const c3 = 0, c4: string, c5: any` — so `null` is neither a
            // literal-const value (it is not in `IsPrimitiveLiteralValue`) nor a
            // literal type here.
            //
            // This is the corpus's default, which is `strictNullChecks` off. Under
            // `strict`, `null` stays `null`, and this port has no compiler options
            // to consult — the option plumbing arrives with the `Program`
            // (`bd tsr-49v.6`). Emitting `any` is the answer that matches every
            // baseline currently in the target.
            SyntaxKind::NullKeyword => Some(factory.keyword_type(SyntaxKind::AnyKeyword, span)),
            _ => None,
        },
        // A regular expression's type is the global `RegExp`, which is a name
        // rather than a keyword. Nothing here checks that `RegExp` resolves — the
        // same assumption upstream's `globalRegExpType` makes.
        Expression::RegularExpressionLiteral(_) => {
            let name = factory.identifier("RegExp", span);
            Some(TypeNode::TypeReferenceNode(factory.alloc(
                TypeReferenceNode::new(Some(tsr_ast::EntityName::Identifier(name)), &[]),
                SyntaxKind::TypeReference,
                span,
                NodeFlags::empty(),
            )))
        }
        // A template with substitutions widens to `string`. In a const context it
        // would need its substitutions' types, which is inference — and
        // `tsr_dts::rules::infer` reports it, so this arm is never reached with
        // `Freshness::Const` for a case in the emitter's target.
        Expression::TemplateExpression(_) => match freshness {
            Freshness::Widening => Some(factory.keyword_type(SyntaxKind::StringKeyword, span)),
            Freshness::Const => None,
        },
        Expression::ParenthesizedExpression(inner) => {
            type_of_expression(factory, inner.expression.as_ref()?, freshness)
        }
        // `x as const` enters a const context; `x as T` states `T` outright, and
        // the annotation is reused rather than rebuilt.
        Expression::AsExpression(as_expression) => {
            if is_const_assertion(as_expression.r#type.as_ref()) {
                type_of_expression(factory, as_expression.expression.as_ref()?, Freshness::Const)
            } else {
                as_expression.r#type
            }
        }
        Expression::TypeAssertion(assertion) => assertion.r#type,
        // `satisfies` constrains without naming the type, so the operand still
        // has to produce one.
        Expression::SatisfiesExpression(satisfies) => {
            type_of_expression(factory, satisfies.expression.as_ref()?, freshness)
        }
        Expression::ObjectLiteralExpression(object) => {
            object_literal_type(factory, object, freshness, span)
        }
        // Only a const array is inferable — `tsr_dts` reports `TS9017` for a
        // mutable one — and a const array is a `readonly` tuple of its elements'
        // types, never `T[]`.
        Expression::ArrayLiteralExpression(array) => {
            if freshness == Freshness::Widening {
                return None;
            }
            let mut elements = Vec::with_capacity(array.elements.len());
            for element in array.elements {
                elements.push(type_of_expression(factory, element, Freshness::Const)?);
            }
            let elements = factory.slice(&elements);
            let tuple = TypeNode::TupleTypeNode(factory.alloc(
                tsr_ast::TupleTypeNode::new(elements),
                SyntaxKind::TupleType,
                span,
                NodeFlags::empty(),
            ));
            Some(readonly_operator(factory, tuple, span))
        }
        Expression::ArrowFunction(arrow) => function_type(
            factory,
            arrow.type_parameters,
            arrow.parameters,
            arrow.r#type.as_ref(),
            span,
        ),
        Expression::FunctionExpression(function) => function_type(
            factory,
            function.type_parameters,
            function.parameters,
            function.r#type.as_ref(),
            span,
        ),
        _ => None,
    }
}

/// A literal type in a const context, the base primitive when widening.
fn widen_or_literal<'a>(
    factory: &mut Factory<'a, '_>,
    expression: &Expression<'a>,
    freshness: Freshness,
    widened: SyntaxKind,
    span: Span,
) -> Option<TypeNode<'a>> {
    match freshness {
        Freshness::Widening => Some(factory.keyword_type(widened, span)),
        Freshness::Const => {
            // `+1`'s literal type is `1`: the plus is not part of the type, and
            // writing `+1` as a type does not parse.
            let literal = literal_const_value(factory, expression)?;
            Some(literal_type(factory, Node::from(literal), span))
        }
    }
}

/// The type of an object literal: a type literal with one member per property.
///
/// In a const context every member is `readonly` and every value keeps its
/// literal type; widened, neither happens. `compiler/isolatedDeclarationsLiterals`
/// pairs the two spellings of the same object and its baseline shows exactly this
/// difference.
fn object_literal_type<'a>(
    factory: &mut Factory<'a, '_>,
    object: &tsr_ast::ObjectLiteralExpression<'a>,
    freshness: Freshness,
    span: Span,
) -> Option<TypeNode<'a>> {
    use tsr_ast::ObjectLiteralElementLike as Member;

    let mut members: Vec<TypeElement<'a>> = Vec::with_capacity(object.properties.len());
    for property in object.properties {
        let member = match property {
            Member::PropertyAssignment(assignment) => {
                let value = assignment.initializer.as_ref()?;
                let r#type = type_of_expression(factory, value, freshness)?;
                property_signature(factory, assignment.name, Some(r#type), freshness, span)
            }
            // A method's type needs its return annotation. Without one this is
            // `TS9008` and the case is not in the target; with one, a const context
            // wants `readonly m: () => T` and a widening one wants `m(): T`.
            Member::MethodDeclaration(method) => {
                let return_type = method.r#type?;
                match freshness {
                    Freshness::Const => {
                        let function = function_type(
                            factory,
                            method.type_parameters,
                            method.parameters,
                            Some(&return_type),
                            span,
                        )?;
                        property_signature(factory, method.name, Some(function), freshness, span)
                    }
                    Freshness::Widening => TypeElement::MethodSignatureDeclaration(factory.alloc(
                        tsr_ast::MethodSignatureDeclaration::new(
                            &[],
                            method.name,
                            None,
                            method.type_parameters,
                            method.parameters,
                            Some(return_type),
                            None,
                        ),
                        SyntaxKind::MethodSignature,
                        span,
                        NodeFlags::empty(),
                    )),
                }
            }
            // Shorthand (`TS9016`), spread (`TS9015`) and accessors in an object
            // literal are all reported by the analysis, so refusing here keeps the
            // two in agreement rather than inventing a shape.
            _ => return None,
        };
        members.push(member);
    }

    let members = factory.slice(&members);
    Some(TypeNode::TypeLiteralNode(factory.alloc(
        tsr_ast::TypeLiteralNode::new(members),
        SyntaxKind::TypeLiteral,
        span,
        NodeFlags::empty(),
    )))
}

fn property_signature<'a>(
    factory: &mut Factory<'a, '_>,
    name: tsr_ast::PropertyName<'a>,
    r#type: Option<TypeNode<'a>>,
    freshness: Freshness,
    span: Span,
) -> TypeElement<'a> {
    let modifiers: &'a [ModifierLike<'a>] = match freshness {
        Freshness::Const => {
            let readonly = factory.modifier(SyntaxKind::ReadonlyKeyword, span);
            factory.slice(&[readonly])
        }
        Freshness::Widening => &[],
    };
    TypeElement::PropertySignatureDeclaration(factory.alloc(
        PropertySignatureDeclaration::new(modifiers, name, None, r#type, None),
        SyntaxKind::PropertySignature,
        span,
        NodeFlags::empty(),
    ))
}

/// `(params) => T`, reusing the parameter nodes as written.
///
/// The parameters are reused rather than rebuilt because they are already
/// annotated — an unannotated one is `TS9011` and the case is not in the target.
fn function_type<'a>(
    factory: &mut Factory<'a, '_>,
    type_parameters: &'a [&'a tsr_ast::TypeParameterDeclaration<'a>],
    parameters: &'a [&'a tsr_ast::ParameterDeclaration<'a>],
    return_type: Option<&TypeNode<'a>>,
    span: Span,
) -> Option<TypeNode<'a>> {
    let return_type = *return_type?;
    Some(TypeNode::FunctionTypeNode(factory.alloc(
        tsr_ast::FunctionTypeNode::new(type_parameters, parameters, Some(return_type), &[], None),
        SyntaxKind::FunctionType,
        span,
        NodeFlags::empty(),
    )))
}

fn literal_type<'a>(factory: &mut Factory<'a, '_>, literal: Node<'a>, span: Span) -> TypeNode<'a> {
    TypeNode::LiteralTypeNode(factory.alloc(
        tsr_ast::LiteralTypeNode::new(Some(literal)),
        SyntaxKind::LiteralType,
        span,
        NodeFlags::empty(),
    ))
}

fn readonly_operator<'a>(
    factory: &mut Factory<'a, '_>,
    inner: TypeNode<'a>,
    span: Span,
) -> TypeNode<'a> {
    let operator = factory.token(SyntaxKind::ReadonlyKeyword, span);
    TypeNode::TypeOperatorNode(factory.alloc(
        TypeOperatorNode::new(operator, Some(inner)),
        SyntaxKind::TypeOperator,
        span,
        NodeFlags::empty(),
    ))
}

fn string_literal<'a>(factory: &mut Factory<'a, '_>, text: &str, span: Span) -> Expression<'a> {
    let text = factory.alloc_str(text);
    Expression::StringLiteral(factory.alloc(
        StringLiteral::new(text, TokenFlags::empty()),
        SyntaxKind::StringLiteral,
        span,
        NodeFlags::empty(),
    ))
}

/// A numeric literal re-spelled in decimal.
///
/// `0x1`, `0o1`, `0b1` and `1_0` all print as themselves if the source text is
/// reused, and upstream's baselines carry the decimal value, because upstream is
/// rebuilding the literal from a computed `jsnum.Number` rather than from text.
///
/// **The known gap**, stated rather than left to be discovered: the formatting
/// here is Rust's shortest round-trip for `f64`, which agrees with JavaScript's
/// `Number::toString` on integers and ordinary decimals but not on the exponent
/// forms JavaScript switches to at `1e21` and `1e-7`. No corpus baseline in the
/// emitter's target exercises those, and a literal that large in a `.d.ts` would
/// be a curiosity; the alternative is porting `jsnum`, which is Phase 4's.
fn decimal_literal<'a>(factory: &mut Factory<'a, '_>, text: &str, span: Span) -> Expression<'a> {
    let value = numeric_value(text);
    let formatted = format_number(value);
    let text = factory.alloc_str(&formatted);
    Expression::NumericLiteral(factory.alloc(
        tsr_ast::NumericLiteral::new(text, TokenFlags::empty()),
        SyntaxKind::NumericLiteral,
        span,
        NodeFlags::empty(),
    ))
}

/// The value of a numeric literal as written.
///
/// Handles the four radix prefixes, `_` separators, and legacy octal (`0755`),
/// which is still a numeric literal in a non-strict file.
pub(crate) fn numeric_value(text: &str) -> f64 {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    let lower = cleaned.to_ascii_lowercase();
    #[allow(clippy::cast_precision_loss)]
    let radix_parse = |digits: &str, radix: u32| -> f64 {
        u128::from_str_radix(digits, radix).map_or(f64::NAN, |value| value as f64)
    };
    if let Some(digits) = lower.strip_prefix("0x") {
        return radix_parse(digits, 16);
    }
    if let Some(digits) = lower.strip_prefix("0o") {
        return radix_parse(digits, 8);
    }
    if let Some(digits) = lower.strip_prefix("0b") {
        return radix_parse(digits, 2);
    }
    if cleaned.len() > 1
        && cleaned.starts_with('0')
        && cleaned.bytes().all(|b| b.is_ascii_digit())
        && !cleaned.contains(['8', '9'])
    {
        return radix_parse(&cleaned[1..], 8);
    }
    cleaned.parse::<f64>().unwrap_or(f64::NAN)
}

/// Format a number the way a `.d.ts` writes it.
pub(crate) fn format_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() };
    }
    #[allow(clippy::float_cmp, clippy::cast_possible_truncation)]
    if value.fract() == 0.0 && value.abs() < 1e21 {
        // `{}` on an integral f64 prints a trailing `.0`; a `.d.ts` never does.
        format!("{}", value as i128)
    } else {
        format!("{value}")
    }
}

/// Whether a type node is the `const` of `x as const`.
///
/// The parser gives a const assertion a `TypeReferenceNode` with **no name**
/// (`crates/tsr-parser/src/types.rs:403`), because `const` is a keyword and never
/// becomes an identifier. `tsr_dts::rules` documents the same trap and matches
/// both spellings; so does this, and the two must not drift.
fn is_const_assertion(node: Option<&TypeNode<'_>>) -> bool {
    let Some(TypeNode::TypeReferenceNode(reference)) = node else { return false };
    match &reference.type_name {
        None => true,
        Some(tsr_ast::EntityName::Identifier(identifier)) => identifier.text == "const",
        Some(tsr_ast::EntityName::QualifiedName(_)) => false,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn numeric_literals_are_read_in_every_radix() {
        // `isolatedDeclarationsLiterals` writes `0o1` and `0x1` and its baseline
        // carries `1` for both, so the radix prefixes are not decoration.
        assert_eq!(numeric_value("0x1"), 1.0);
        assert_eq!(numeric_value("0o10"), 8.0);
        assert_eq!(numeric_value("0b101"), 5.0);
        assert_eq!(numeric_value("1_000"), 1000.0);
        assert_eq!(numeric_value("0755"), 493.0);
        // A leading zero with an `8` in it is decimal, not octal.
        assert_eq!(numeric_value("089"), 89.0);
        assert_eq!(numeric_value("1.5"), 1.5);
    }

    #[test]
    fn integral_values_lose_the_rust_trailing_zero() {
        assert_eq!(format_number(1.0), "1");
        assert_eq!(format_number(-1.0), "-1");
        assert_eq!(format_number(1.5), "1.5");
    }
}
