# Primitive literal value identity

TypeScript's checker stores primitive literal **values**, not their source
spellings. This matters at the checker boundary because the scanner deliberately
preserves enough spelling for diagnostics and emit: `0xC0Bn` reaches the checker
with that text, but its literal type is `3083n`.

## Native anchors

The pinned native path is:

- `checker.go:7753` calls `jsnum.ParsePseudoBigInt(node.Text())` when checking a
  bigint expression;
- `checker.go:10871` constructs the same value with its negative bit for unary
  minus;
- `internal/jsnum/pseudobigint.go:45-69` converts binary, octal and hexadecimal
  spellings to an arbitrary-precision decimal identity;
- `checker.go:25331-25344` interns that identity, and
  `nodebuilderimpl.go:3304-3305` prints its canonical decimal spelling.

`printing::normalise_bigint` owns the equivalent Rust boundary. It uses decimal
digit multiply/add, so literals larger than `u128` remain exact without imposing
a general bigint representation on the checker. Separators, radix and leading
zeros disappear; negative zero canonicalises to zero. Malformed scanner-recovery
text retains its spelling rather than acquiring an invented value.

Numeric literals have ECMAScript `Number` (`f64`) identity already. Rust and
ECMAScript agree on shortest round-tripping digits but choose scientific notation
at different thresholds. `normalise_number` therefore changes only presentation:
ECMAScript uses scientific notation at magnitudes at least `1e21` and non-zero
magnitudes below `1e-6`. Boundary controls distinguish both sides, including the
native `castExpressionParentheses` value `1.2e35`.

The arbitrary-precision control is independently derived at the u128 boundary:
`0x100000000000000000000000000000000` is 16^32 = 2^128, or
`340282366920938463463374607431768211456` in decimal — exactly one greater than
`u128::MAX`. The conformance fixture `parseBigInt` independently checks
expression, unary-minus, separator, leading-zero and radix spellings.

## Measurement

Against the same-checkout baseline at `8f8f4e1a` (474,244 aligned rows; 458,022
RIGHT), the full scorepair moves 112 WRONG rows to RIGHT and loses zero previously
RIGHT rows. The resulting score is 458,134 RIGHT, 2,445 GAP and 13,665 WRONG.
There are no added or removed rows and no changed payloads among rows remaining
WRONG. The gains are 63 `parseBigInt`, 23 `numberLiteralsWithLeadingZeros`, 15
`binaryIntegerLiteralES6`, 3 each `enumInitializersWithExponents` and
`fakeInfinity1`, 2 each `bigintWithLib` and `castExpressionParentheses`, and 1
`octalIntegerLiteralES6`.
