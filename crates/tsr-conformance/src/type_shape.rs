//! Classifying the **shape of an answer** in a `.types` baseline.
//!
//! `bd tsr-4sc.6` asks a question the case rate cannot answer: of the assertion
//! lines we get wrong, what *kind* of type was upstream's answer? That is what
//! ranks the checker's remaining work — see
//! [checker.md](../../../docs/architecture/checker.md).
//!
//! # This is a bucket of the answer, not of the feature
//!
//! `f()` → `string` is an *intrinsic* answer that needs full call resolution.
//! So a bucket is an upper bound on what a feature could win and never an
//! estimate of the work; the histogram in `examples/types_shapes.rs` exists
//! precisely because the buckets alone cannot rank anything.
//!
//! # Why the classification is syntactic
//!
//! A baseline records the *printed* type and nothing else — there is no type
//! object to ask. So this reads the printed form, at the precedence TypeScript's
//! own printer emits: a top-level `=>` before any top-level `|` is a signature
//! returning a union, because a function type extends as far right as it can and
//! upstream parenthesises it when it does not.

/// The shape of a printed type, as the ranking table buckets them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Shape {
    /// `string`, `number`, `any`, `void`, …
    Intrinsic,
    /// `"a"`, `1`, `true`, `1n`, a template literal type.
    Literal,
    /// `A | B`.
    Union,
    /// `A & B`.
    Intersection,
    /// `() => void`, `new () => C`, `<T>(x: T) => T`.
    Signature,
    /// `{ a: string; }`.
    ObjectLiteral,
    /// `typeof x`, `typeof import("./m")`.
    Typeof,
    /// `string[]`, `A[][]`.
    Array,
    /// `Array<string>`, `M.I<T>`.
    GenericReference,
    /// `C`, `M.I` — a bare name.
    NamedReference,
    /// Everything else: indexed access, conditional, mapped, tuple, `this`, …
    Other,
}

impl Shape {
    /// A stable short label for reports.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Shape::Intrinsic => "intrinsic",
            Shape::Literal => "literal",
            Shape::Union => "union",
            Shape::Intersection => "intersection",
            Shape::Signature => "function/signature",
            Shape::ObjectLiteral => "object literal",
            Shape::Typeof => "typeof",
            Shape::Array => "array",
            Shape::GenericReference => "generic reference",
            Shape::NamedReference => "named reference",
            Shape::Other => "other",
        }
    }

    /// Every shape, in report order.
    #[must_use]
    pub fn all() -> [Shape; 11] {
        [
            Shape::Intrinsic,
            Shape::Literal,
            Shape::NamedReference,
            Shape::Signature,
            Shape::ObjectLiteral,
            Shape::Typeof,
            Shape::Union,
            Shape::GenericReference,
            Shape::Array,
            Shape::Intersection,
            Shape::Other,
        ]
    }
}

/// The names TypeScript prints for its intrinsic types.
///
/// `boolean` is here although upstream models it as a union of two literal
/// types: it *prints* as an intrinsic name and this classifies printed forms.
/// That is also why the union work (`bd tsr-4sc.9`) reaches beyond its own
/// bucket — see [checker.md](../../../docs/architecture/checker.md).
const INTRINSICS: &[&str] = &[
    "any",
    "unknown",
    "never",
    "void",
    "undefined",
    "null",
    "string",
    "number",
    "bigint",
    "boolean",
    "symbol",
    "unique symbol",
    "object",
];

/// Bucket a printed type.
#[must_use]
pub fn classify(printed: &str) -> Shape {
    let text = printed.trim();
    if text.is_empty() {
        return Shape::Other;
    }

    // Operators first, at printer precedence. A parenthesised whole is unwrapped
    // only after this, so `(A | B)[]` stays an array rather than becoming a union.
    if let Some(shape) = operator_shape(text) {
        return shape;
    }

    if text.ends_with("[]") {
        return Shape::Array;
    }
    if text.starts_with('{') {
        return Shape::ObjectLiteral;
    }
    if text.starts_with('[') {
        return Shape::Other; // a tuple
    }
    if let Some(inner) = unwrap_parens(text) {
        return classify(inner);
    }
    if text.starts_with("typeof ") {
        return Shape::Typeof;
    }
    if text.starts_with("keyof ")
        || text.starts_with("readonly ")
        || text.starts_with("infer ")
        || text == "this"
    {
        return Shape::Other;
    }
    if INTRINSICS.contains(&text) {
        return Shape::Intrinsic;
    }
    if is_literal(text) {
        return Shape::Literal;
    }
    if let Some(head) = text.strip_suffix('>')
        && let Some(open) = top_level_angle(head)
        && is_qualified_name(&head[..open])
    {
        return Shape::GenericReference;
    }
    if is_qualified_name(text) {
        return Shape::NamedReference;
    }
    Shape::Other
}

/// The shape implied by the leftmost top-level operator, if any.
///
/// `=>` binds loosest to its *right*, so `() => A | B` is a signature returning
/// a union and upstream writes `(() => A) | B` when it means the other thing.
/// Comparing positions is therefore enough, and it is why this is one scan
/// rather than three independent `contains` checks.
fn operator_shape(text: &str) -> Option<Shape> {
    let mut arrow = None;
    let mut union = None;
    let mut intersection = None;
    for (index, depth) in top_level_positions(text) {
        if depth != 0 {
            continue;
        }
        // Byte slices, not `&text[index..]`: the scan yields every byte index and
        // the corpus is full of non-ASCII identifiers, so slicing as `str` panics
        // mid-character. Found by running this over the corpus.
        let rest = &text.as_bytes()[index..];
        if arrow.is_none() && rest.starts_with(b"=>") {
            arrow = Some(index);
        } else if union.is_none() && rest.starts_with(b" | ") {
            union = Some(index);
        } else if intersection.is_none() && rest.starts_with(b" & ") {
            intersection = Some(index);
        }
    }
    let mut candidates =
        [(arrow, Shape::Signature), (union, Shape::Union), (intersection, Shape::Intersection)]
            .into_iter()
            .filter_map(|(position, shape)| position.map(|p| (p, shape)))
            .collect::<Vec<_>>();
    candidates.sort_by_key(|(position, _)| *position);
    candidates.first().map(|(_, shape)| *shape)
}

/// Every byte index of `text` paired with the bracket depth *at* that index,
/// with string and template literals skipped whole.
fn top_level_positions(text: &str) -> Vec<(usize, i32)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'"' | b'\'' | b'`' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                index += 1;
                continue;
            }
            b'(' | b'[' | b'{' | b'<' => {
                out.push((index, depth));
                depth += 1;
            }
            // `=>` closes nothing; a lone `>` closes an angle bracket, so the
            // guard has to come before the closer arm it would otherwise join.
            b'>' if index > 0 && bytes[index - 1] == b'=' => out.push((index, depth)),
            b')' | b']' | b'}' | b'>' => {
                depth -= 1;
                out.push((index, depth));
            }
            _ => out.push((index, depth)),
        }
        index += 1;
    }
    out
}

/// `(A)` → `A`, but only when the parens wrap the whole thing.
fn unwrap_parens(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    // Depth never dipping below zero is what distinguishes `(A)` from `(A)(B)`,
    // whose inner text is balanced overall but closes the opening paren early.
    top_level_positions(inner).iter().all(|&(_, depth)| depth >= 0).then_some(inner)
}

/// The index of the `<` opening a top-level type-argument list at the end.
fn top_level_angle(head: &str) -> Option<usize> {
    top_level_positions(head)
        .into_iter()
        .find(|&(index, depth)| depth == 0 && head.as_bytes()[index] == b'<')
        .map(|(index, _)| index)
}

/// A string, number, bigint, boolean or template literal type.
fn is_literal(text: &str) -> bool {
    if text == "true" || text == "false" {
        return true;
    }
    let first = text.as_bytes()[0];
    if first == b'"' || first == b'\'' || first == b'`' {
        return true;
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    let digits = digits.strip_suffix('n').unwrap_or(digits);
    !digits.is_empty()
        && digits.as_bytes()[0].is_ascii_digit()
        && digits.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'+' || b == b'-')
}

/// `C`, `M.I`, `M.N.I` — an identifier or a dotted chain of them.
fn is_qualified_name(text: &str) -> bool {
    !text.is_empty()
        && text.split('.').all(|part| {
            let mut bytes = part.bytes();
            bytes.next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_' || b == b'$')
                && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
        })
}

/// Each test names the case it exists for, and every one was checked against a
/// deliberately weakened classifier rather than assumed to bite; the mutations
/// are recorded in `docs/architecture/checker-oracle.md`.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plain_shapes() {
        assert_eq!(classify("string"), Shape::Intrinsic);
        assert_eq!(classify("boolean"), Shape::Intrinsic);
        assert_eq!(classify("unique symbol"), Shape::Intrinsic);
        assert_eq!(classify(r#""a""#), Shape::Literal);
        assert_eq!(classify("1"), Shape::Literal);
        assert_eq!(classify("-1"), Shape::Literal);
        assert_eq!(classify("1.5e21"), Shape::Literal);
        assert_eq!(classify("123n"), Shape::Literal);
        assert_eq!(classify("true"), Shape::Literal);
        assert_eq!(classify("C"), Shape::NamedReference);
        assert_eq!(classify("M.I"), Shape::NamedReference);
        assert_eq!(classify("Array<string>"), Shape::GenericReference);
        assert_eq!(classify("string[]"), Shape::Array);
        assert_eq!(classify("typeof x"), Shape::Typeof);
        assert_eq!(classify(r#"typeof import("./m")"#), Shape::Typeof);
        assert_eq!(classify("{ a: string; }"), Shape::ObjectLiteral);
        assert_eq!(classify("[string, number]"), Shape::Other);
    }

    #[test]
    fn a_function_type_returning_a_union_is_a_signature() {
        // The case that makes this a precedence scan rather than three
        // `contains` checks. `() => void | number` is a function returning a
        // union — upstream writes `(() => void) | number` when it means a union
        // of a function — so the leftmost top-level operator decides.
        assert_eq!(classify("() => void | number"), Shape::Signature);
        assert_eq!(classify("(() => void) | number"), Shape::Union);
        assert_eq!(classify("<T>(x: T) => T"), Shape::Signature);
        assert_eq!(classify("new () => C"), Shape::Signature);
    }

    #[test]
    fn an_operator_inside_brackets_is_not_top_level() {
        // Without depth tracking every one of these reads as a union, and the
        // three largest buckets bleed into it.
        assert_eq!(classify("Array<string | number>"), Shape::GenericReference);
        assert_eq!(classify("{ a: string | number; }"), Shape::ObjectLiteral);
        assert_eq!(classify("(A | B)[]"), Shape::Array);
        assert_eq!(classify(r#""a" | "b""#), Shape::Union);
        assert_eq!(classify("A & B"), Shape::Intersection);
    }

    #[test]
    fn a_separator_inside_a_string_literal_is_not_an_operator() {
        // Literal types are 18.86% of the corpus and they can contain anything.
        assert_eq!(classify(r#"" | ""#), Shape::Literal);
        assert_eq!(classify(r#""=>""#), Shape::Literal);
        assert_eq!(classify(r#""a" | " & ""#), Shape::Union);
    }

    #[test]
    fn an_indexed_access_is_not_an_array() {
        // `A["k"]` ends in `]` but is an indexed access. The distinction is the
        // *pair* `[]`, not the closing bracket: an earlier draft also checked
        // that the head was bracket-balanced, and that guard was removed because
        // no printed type can end in `[]` with an unbalanced head — TypeScript
        // closes object, tuple and type-argument lists before any `[]` suffix.
        assert_eq!(classify(r#"A["k"]"#), Shape::Other);
        assert_eq!(classify("A[][]"), Shape::Array);
    }

    #[test]
    fn a_wrapped_type_is_classified_by_its_contents() {
        assert_eq!(classify("(string)"), Shape::Intrinsic);
        // But only when the parens wrap the *whole* thing: this is a signature
        // whose parameter list merely starts at byte zero.
        assert_eq!(classify("(a: string) => void"), Shape::Signature);
    }

    #[test]
    fn what_is_not_a_name() {
        assert_eq!(classify("A extends B ? C : D"), Shape::Other);
        assert_eq!(classify("keyof C"), Shape::Other);
        assert_eq!(classify("this"), Shape::Other);
        assert_eq!(classify(""), Shape::Other);
    }
}
