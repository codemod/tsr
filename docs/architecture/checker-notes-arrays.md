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

## Quoted property names, and why the escape table is shared (2026-08-05)

`{ "a-b": 1 }` is `{ "a-b": number; }`. Now unblocked — `printing::quote` was
made `pub(crate)`, so there is one escape table rather than two.

The rule is upstream's: a string-named property prints **unquoted when the name
is identifier text and re-quoted otherwise**, and the *source spelling is
discarded either way*. So `{ "a": 1 }` loses its quotes and `{ "a-b": 1 }` keeps
them. Echoing the source spelling passes the first case in most fixtures and is
wrong in both directions, which is why the test asserts the pair rather than
either alone.

### Why duplicating `quote` would have been worse than it looks

The argument inverts the obvious one. `printing::quote`'s escape table is
**deliberately incomplete** (`bd tsr-4sc.1`): it handles the common escapes and
the C0 controls and emits anything else raw, so a miss surfaces as a baseline
mismatch rather than as silent corruption. That incompleteness is exactly what
makes a *single* copy safe — and exactly what would make a divergence between
*two* copies invisible, since both would have to be corrected together when
tsr-4sc.1 lands and nothing would fail if only one were.

The parser unescapes and `quote` re-escapes, which is the same round trip
`tests/types.rs` already pins for a string literal *type*. Sharing the function
is what stops a name and a literal escaping differently in the same baseline.

## Fixtures that exercise without discriminating

The recurring failure of this workstream, now at five instances. A test can run
the right code and still not distinguish the right implementation from a wrong
one. Reading the test never reveals this; only mutating the code does.

1. **`typeof` constituent order.** Scrambling the source list stayed green — the
   order is guaranteed by `unions.rs`'s `compare_types` sort, not by this code.
   Rewritten to pin the *set*.
2. **Arrow transparency in `GetContainingFunction`.** Removing `ArrowFunction`
   from the walk stayed green, because with the arrow transparent the walk
   reached a generator that *also* answers `any`. Fixed by annotating the
   enclosing generator so the two readings diverge.
3. **The `{ a = 1 }` guard.** Unobservable: `check_binary_expression` gaps the
   whole assignment first. Proved by a positive probe — making the arm answer
   `never` and watching the result stay `error` — then documented, not tested.
4. **Numeric name normalisation.** Swapping `normalise_number` for the raw source
   text reddened only *one* of two tests, because `{ 0: 1 }` has no spelling to
   normalise. A single obvious fixture would have looked verified.
5. **The quoting mutations themselves.** Both first attempts *failed to apply* —
   `cargo fmt` had reformatted the arm onto one line, so the anchor no longer
   matched. The tests then "passed", which would have been recorded as verified
   had `grep -c` not reported `0`.

The fifth is the one to internalise: **a mutation that does not apply is
indistinguishable from a mutation that does not matter**, and both look like a
green test run. Confirming the edit landed is not bureaucracy, it is the only
thing separating those two cases. Every mutation in this workstream is
`grep -c`-confirmed before the test is run, and that check has now caught two
silent no-ops that would otherwise have been reported as passing verification.

The general rule: design the fixture to *discriminate*, then prove it does by
breaking the code. A fixture that merely exercises the path is a decoration with
a test's name on it.

## The element-access row was an `any` receiver all along (2026-08-05)

The largest single row on the board — 12,376 gap lines, 17.3% of everything this
port answers `error` on where upstream answers an intrinsic. It had been assigned
once before on the belief that index signatures were unported. They are not, and
the row is not downstream of them.

### The measurement, before the work

Element-access lines in the `.types` baselines, by index shape:

| index | lines | answer `any` |
|---|---|---|
| `a[0]` numeric | 11,278 | 10,737 (95%) |
| `a[k]` identifier | 1,120 | 574 (51%) |
| `a["k"]` string | 507 | 52 (10%) |
| **total** | **12,905** | **11,363 (88%)** |

88% of the row answers `any`, because **the receiver is `any`** and this port
answered `error`. The canonical baseline is `conformance/anyPropertyAccess.types`.

### The numeric skew nearly produced a confident wrong answer

Read as an index-shape histogram, "79% numeric" says *arrays and tuples*, which
points at instantiated generic members — a real blocker (`bd tsr-el3.2`, the same
one that leaves `C<number>` memberless) that would have made this row a
workstream and closed it for the cycle. But of those 11,278 numeric lines, 10,737
answer `any`, so they are `any` receivers, not arrays. Array receivers are a real
residue, not the row.

The lesson is that a histogram over *syntax* suggested one cause and a histogram
over *answers* revealed another. Counting the answer column is what separated
them, and it cost one extra command.

`indexed.rs` itself was already complete: object lookup, string index signatures
and numeric index signatures all verified by probe before any code was written.

### Everything else in the row, ranked

- **Array and tuple receivers carry no members.** `a[0]`, `a.length` and
  `a["length"]` all gap because `create_type_reference` (`declared.rs:489`)
  builds the type with `symbol: None`. Blocked on instantiated generic members,
  `bd tsr-el3.2`. A workstream, and not in these files.
- **Numeric property names in *type* position.** `var o: { 0: string }` gaps as
  an annotation, so `o[0]` never gets a chance. This is the same gap closed on
  the object-literal *expression* side in `724da91`; the type-literal side is in
  `declared.rs`.
- **Unannotated variables never reach the arm**, and the cause is flow, not
  indexing: `declare let a` narrows to `undefined` at a use before assignment,
  and `var a` needs `autoType` and the evolving-array machinery that
  `crate::flow` does not port. Implicit-any *parameters* do reach it, which is
  the shape that matters — most corpus receivers are `any` that way.

### The guard that cannot be tested, and why it stays

The arm tests `object_type == self.intrinsics.any` by **identity**, not by
`TypeFlags::ANY`, because `errorType` also carries `ANY` in this port and a flag
test would turn every gap into a confident `any` — the most dangerous way this
row could produce a large number.

That identity test is **unobservable today**: the `error` guard at the top of
`check_element_access_expression` already returned, so swapping identity for a
flag test leaves every test green. Verified by mutation rather than assumed. It
stays as defence in depth against a future edit reordering that guard, and it is
documented instead of tested — the same rule as the `{ a = 1 }` guard.

This is the sixth instance of the exercises-versus-discriminates problem in this
workstream, and the first where the overclaiming comment was one I had written
myself: the test said it pinned the identity-versus-flag distinction, and it
never could. The comment now says what the test actually establishes.

## The property-access half of the same cause (2026-08-05)

`a.b` on an `any` receiver, ported in the same cycle as `a[i]` and for the same
reason: `conformance/anyPropertyAccess.types` records both spellings failing in
the same files, so under a whole-line case gate fixing one alone would move the
gradient while the case count barely budged, and we would learn nothing about
which half mattered.

Property-access lines in the baselines: **27,140 total, 6,329 answer `any`**,
against a gap row of 8,154. So an `any` receiver is most of what that row is,
even though it is a much smaller *share* of all property accesses (23%) than it
is of element accesses (88%) — most property accesses already resolve.

### Upstream guards `errorType` inside this very branch

The precise anchor is worth having, because it settles the design question
rather than leaving it to local judgement. `isAnyLike` (`checker.go:11266`)
guards a branch at `checker.go:11314`, and *inside* that branch, before the
return, sits:

```go
if c.isErrorType(apparentType) {
    return c.errorType
}
return apparentType
```

(`checker.go:11318`). Upstream considers `errorType` any-like and then
explicitly refuses to answer `any` for it. This port gets the same result by
testing **identity** against `intrinsics.any`: `errorType` is a different type
carrying the same `ANY` flag, so it cannot match.

**A `TypeFlags::ANY` test instead would answer `any` for every gap in the
corpus** — and it would look like an enormous win in the measurement. It is the
single most dangerous edit available in either of these two functions, which is
why both carry the reasoning inline rather than a bare comparison.

### The same guard is load-bearing in one file and decoration in the other

Only mutation could tell them apart, and it is a neat illustration of why the
rule is worth the cost:

- In `members.rs` the identity test **is** load-bearing. Swapping it for a flag
  test reddens `a_property_access_on_an_untypeable_receiver_is_still_a_gap`,
  because nothing else stands between `errorType` and the `any` answer.
- In `indexed.rs` the same swap leaves every test **green**, because the
  `object_type == error` guard at the top of the function already returned. The
  test there is documented as defence in depth rather than claimed as coverage.

Identical code, identical intent, opposite testability — decided by what happens
to sit above it. Reading either function would not reveal which was which.
