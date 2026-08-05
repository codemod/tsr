# Checker notes — expression forms and object literals

Working notes from the expression-forms workstream. Kept in a separate file
because four agents appending to `docs/architecture/checker.md` in one checkout
caused three sweeps and one near-loss of committed content; the lead will fold
these into `checker.md` once at the end of the cycle.

Earlier sections of this workstream — the unary operators, `typeof`, conditional
expressions, `new C()`, `yield`, and shorthand object-literal properties — are
already in `checker.md`. This file starts from the numeric-property-name slice.

## Numeric property names print their value, not their spelling (2026-08-05)

`{ 0: 1 }` is `{ 0: number; }`. Numeric names are not an exotic spelling: the
baselines carry 63 lines of `{ 0: number; }`, 30 of `{ 1: string; }`, and 24 of
`{ 1: number; }`, because a numeric-keyed object literal is what array-like data
looks like before anyone reaches for a tuple.

The one decision is which text to print. The name goes through
`printing::normalise_number` — the same function the numeric *literal type* uses
— rather than through the source text. Writing the source text through unchanged
is the obvious shortcut and is wrong in both directions: `{ 1.0: x }` must print
`1`, and `{ 1e3: x }` must print `1000`. Sharing the function is also what stops
`1e3` printing one way as a property name and another as a type in the same
baseline.

**How we would know this is wrong:** a baseline object type whose numeric key is
printed in a spelling that differs from how the same numeral prints as a literal
type.

## Blocked: non-identifier string property names need `printing::quote`

`{ "a-b": 1 }` should print `{ "a-b": number; }` and currently gaps. It is worth
having — `{ "resolution-mode": string; }` is 24 baseline lines, `{ "a b": number; }`
8, `{ "a-b": string; }` 6, plus `{ "@ns/dep": string; }` at 10.

It is **not** implemented, and the reason is deliberate rather than incidental.
Printing the name requires the same quoting and escaping a string literal type
gets, which lives in `printing::quote`. That function is private to
`crates/tsr-checker/src/printing.rs`, and its escape table is **deliberately
incomplete**: it covers `\`, `"`, the C0 controls and the common escapes, stops
short of the full table, and emits anything else raw so that a miss shows up as a
baseline mismatch rather than as silent corruption (`bd tsr-4sc.1`).

Duplicating that table in `objects.rs` would create two escape tables that have
to be corrected together when `bd tsr-4sc.1` lands, and nothing would fail if
only one of them were. That is precisely the drift `render_object_type` was
written to prevent — its own doc comment records that the two object-type
renderers were separate in draft, "which is exactly how a port ends up with
`{ a: string }` in one position and `{ a: string; }` in another".

**What unblocks it:** making `printing::quote` `pub(crate)`. That is a
one-word change in a file this workstream does not own, so it is reported rather
than made. Once it is visible, the property-name arm is three lines: if the
string is not identifier text, print `quote(text)`.

## Remaining object-literal gaps, ranked

Measured against the baselines, for whoever picks this up next:

| Gap | Rough size | What it needs |
|---|---|---|
| Non-identifier string names | ~45 lines | `printing::quote` made `pub(crate)` — see above |
| Methods `{ m() {} }` | ~3 lines | `checkObjectLiteralMethod`, and a signature this port can print |
| Spread `{ ...x }` | not measured | `getSpreadType` |
| Accessors | not measured | `getTypeOfAccessors`, unported |
| Computed names `{ [k]: v }` | not measured | `isTypeUsableAsPropertyName` |

Only the first is worth a slice on its own, and only once it is unblocked.

## Tagged templates are a call, so they reuse the call's resolution (2026-08-05)

``tag`a${b}c` `` — `checkTaggedTemplateExpression` (`checker.go:10034`) is three
lines once the grammar checks are set aside: resolve the tag's signature through
**`getResolvedSignature`** and return its return type. That is the *same* entry
point a call expression uses, because a tagged template **is** a call whose
arguments are the template strings array and the substitutions.

So the port shares `resolve_call_signature` rather than growing a second
resolution path, and inherits its restriction to a callee with exactly one call
signature for exactly the same reason: choosing among several needs
assignability. Writing a separate resolver would have been the natural shape —
the syntax looks nothing like a call — and would have drifted from the call path
the first time either changed.

The answer comes from the **tag**, never from the template. A port could
plausibly answer `string` here, since that is what most tags return and what the
untagged template answers; the test uses a tag returning a class type to make
the difference visible.

**A template with substitutions still gets a real answer.** The template itself
is a `TemplateExpression`, which is unported and correctly stays a gap on its own
line, but the tagged template does not inherit that — the tag's signature is what
decides. This is the second form in this workstream where a gap in a sub-
expression does *not* propagate (`typeof` was the first), and both times the
reason is upstream's own structure rather than a convenience.

### What stays gapped, and why the 289 lines will not all close

A **generic tag** gaps, and this is the big one: `String.raw` and essentially
every typed template helper is generic, so its return type depends on inference
over the template strings array and each substitution. Also gapped: explicit type
arguments, and a tag whose type has anything other than exactly one call
signature.

The optional-chain guard is **unobservable** — the grammar prohibits
``tag?.`x` `` ("Tagged template expressions are not permitted in an optional
chain") and this port's parser rejects it outright — so it is documented, not
tested, on the same rule as the `{ a = 1 }` guard in `objects.rs`.

### Prediction, recorded before measurement

Of the 289 gap lines this form carries, I expect **roughly 100 to close**, with a
plausible range of 40–110, concentrated in the `string` answer row (the baseline
split over tagged-template lines is 108 `string`, 37 `any`, 4
`TemplateStringsArray`). The gap between 289 and ~100 is generic tags and
overloaded tags, neither of which this closes. Attribute to the commit pair
**724da91 → this commit**.

If the measured movement is far above ~110, something is answering that should
be gapping — most likely a generic tag slipping through — and the fence needs
checking rather than celebrating.
