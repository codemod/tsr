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

## ~~Blocked~~ RESOLVED 2026-08-05: non-identifier string property names

> **Superseded.** `printing::quote` was made `pub(crate)` in `891371a` and the
> arm landed in `73396dd`. The reasoning below is kept because the *argument*
> for not duplicating the escape table outlived the blocker, and because a
> resolved blocker deleted is a blocker rediscovered.

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

**Corrected 2026-08-05.** The first two rows of this table are now done; the
sizes below are re-measured rather than estimated.

| Gap | Size | Status / what it needs |
|---|---|---|
| Non-identifier string names | ~45 lines | **DONE** `73396dd` |
| Numeric names | ~130 lines | **DONE** `724da91` |
| Shorthand `{ a }` | ~89 lines | **DONE** `c707889` |
| Member *symbol* types | 8,549 lines | **DONE** `f6ffe77` — the big one, and it was never the lookup |
| Arrow-valued members `{ f: () => 1 }` | 630 lines | the arrow's own return inference gaps; not an object-literal item |
| Methods `{ m() {} }` | ~3 lines | `checkObjectLiteralMethod`, and a signature this port can print |
| Spread `{ ...x }` | not measured | `getSpreadType` |
| Accessors | not measured | `getTypeOfAccessors`, unported |
| Computed names `{ [k]: v }` | not measured | `isTypeUsableAsPropertyName` |

Nothing left here is worth a slice on its own. The 630 arrow-valued members are
the largest remaining number and they belong to whoever owns return inference,
not to `objects.rs`.

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
points at instantiated generic members — a real blocker (`bd tsr-4qx`, the same
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
  `bd tsr-4qx`. A workstream, and not in these files.
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

## What the `any`-receiver measurement actually said (2026-08-05)

Predicted 6,000–15,000 lines, most likely ~10,000. **Delivered 950** — right at
the pre-registered "under ~1,000" falsifier boundary. Both halves, measured as
single-commit pairs (`4fdcba5..4f060b9` and `eecb7aa..9700087`):

| row | gap | wrong |
|---|---|---|
| ElementAccessExpression | 12,372 → 12,316 (**−56**) | 67 → 81 (+14) |
| PropertyAccessExpression | 8,143 → 7,249 (**−894**) | 1,040 → 1,595 (+555) |

The row diagnosed at **88% `any`** moved 56 lines; the row at **23% `any`** moved
894. The inversion is the finding.

### 88% of the element-access row is one file, and it is a flow item

`compiler/largeControlFlowGraph.types` holds **9,999 of the 11,363**
`any`-answering element-access lines. It opens:

```text
// The control flow graph for the following statement block is 10000 nodes deep.
const data = [];
```

followed by ~10,000 repetitions of `>data[0] : any` with `>data : any` at each
access site. `const data = []` is an **evolving array**: upstream's
`autoArrayType`, `addEvolvingArrayElementType` and `finalizeEvolvingArrayType`
make the flow type `any` at each site, and `crate::flow` documents that machinery
as unported (flow.rs:293–297).

So the receiver is `any` upstream and is *not* `any` here, and the access
correctly propagates our gap. The arm was never going to reach it. **The
element-access row is a flow item, not an access item** — and it is unreachable
for the case metric too, since that file cannot pass without evolving arrays.

### The diagnosis was wrong at a level below the one it corrected

The first attempt bucketed by index *syntax* (79% numeric) and pointed at array
receivers — a real blocker, `bd tsr-4qx`, and the wrong one. Bucketing by
upstream's *answer* (88% `any`) corrected that and pointed here. Both were
measuring the wrong population.

**Three levels, and they are not interchangeable:**

1. the syntax of the question — what the expression looks like;
2. what **upstream** answers — the baseline's type column;
3. what **our port** fails on — the join of our gap set against (2).

Level 2 beats level 1 and is still not level 3. "88% of element-access lines
answer `any`" is true and says nothing about whether our port can reach those
receivers. Only level 3 predicts movement, and computing it needs the conformance
instrument rather than a grep over baselines.

### The +555 wrong lines: cause, owner, and the cost accepted

Not a leak through the identity guard — the guard held. The arm is faithful
given its input; the *input* is wrong.

This port has no contextual typing, so an unannotated parameter is the implicit
`any` here where upstream infers a real type —
`getContextuallyTypedParameterType` (`checker.go:29458`) via
`assignContextualParameterTypes` (`checker.go:10349`). Probe:

```text
const y = x => x.foo;      // this port: x.foo : any
arr.map(x => x.foo)        // upstream:  x typed from context, x.foo is real
```

Before the arm these were gaps; now they are claims. **Kept deliberately**, on
three grounds: the arm matches upstream given an `any` receiver; the lines cost
no gradient and no cases, because the receiver's own line was already wrong so
every affected case was already failing; and reverting would surrender 894 real
lines to avoid 555 that cost nothing measurable.

**The cost that is real is diagnostic separability.** A gap is an honest "don't
know" the instrument can bucket by cause; a wrong answer is a claim that looks
like a result. Someone reading a property-access histogram will see 555 wrong
lines with no way to learn from the instrument that they belong to contextual
typing. That is why the owner is named at the arm in `members.rs` and here:
closing contextual typing is what removes them.

## Export markers: a symbol with no useful flags (2026-08-05)

`export var x = 1; var q = x;` — **one file, no imports** — answered `errorType`
for the reference to `x`. Measured at **3,144 lines across 463 cases**: not one
file and not one shape, just every reference to every exported name.

### Why the symbol has no flags to dispatch on

When a declaration is exported from a module, the binder declares it **twice**:
the real symbol into the module's `exports` with its own flags, and a **marker**
into the file's `locals` carrying `SymbolFlags::EXPORT_VALUE` and nothing else
(`binder.rs:3086`–`3098`). That is upstream's design, not a local quirk — it
keeps an unqualified reference resolvable while the export table stays the
authority on what was exported.

A reference inside the module resolves to the marker. Its flags carry no
`VARIABLE`, `FUNCTION` or `CLASS` bit, so every arm of `get_type_of_symbol`
missed and the fallthrough answered `errorType`.

### Two attempts, and why the second is the port

**First attempt — dispatch on the declaration's kind.** The marker shares its
declaration node with the export symbol, so matching on that node's kind looks
like it should reproduce what the export symbol's flags would have selected.

It half-worked, and the half that worked was misleading. `export function f`
answered correctly while `export var x` did not, because
`get_type_of_variable_or_parameter_or_property` reads **`value_declaration`**,
which a marker never has: `SymbolFlags::VALUE` does not include `EXPORT_VALUE`
(`symbol.rs:106`), so the binder's `if flags.intersects(VALUE)` test never fires
and the field stays `None`. The function path uses `declarations` instead, so it
alone appeared to succeed — a partial result that looked like a working arm.

`export class C` could not be fixed that way at all. A class *value* reference is
the static side, `typeof C`, and nothing local to a marker can know that: the
marker carries no `CLASS` flag, and the declaration kind alone does not say
whether the reference is to the constructor or the instance.

**Second attempt — reconstruct the link.** Upstream reads
`symbol.ExportSymbol` (`getExportSymbolOfValueSymbolIfExported`,
`checker.go:14383`). `tsr_binder::Symbol` has no such field, so the link is
rebuilt from what the binder does record: walk from the marker's declaration to
the enclosing `SourceFile`, take that file's own symbol — which *is* the module
symbol (`binder.rs:2578`) — and look the name up in its `exports`.

Then hand the export symbol to `get_type_of_symbol` and let the ordinary
dispatch answer it. Every exported form is served by the arm that already knows
how, rather than by a second copy of that knowledge. `export class C` now gives
`C : typeof C` and `C.s : string`.

Walking to the `SourceFile` is what makes it exact rather than a name search:
the lookup is scoped to the module that declared the marker and cannot collide
with a same-named export elsewhere in the program. A mutation replacing it with
a `globals` lookup reddens four of the six tests.

**Adding the field to the binder is the faithful port** and is the better fix
when someone owns that crate. This reaches the same symbol without reshaping a
type every consumer of the binder shares.

**How this would be shown wrong:** an exported declaration whose export symbol's
flags disagree with its declaration kind. Merged declarations are where to look —
`export interface I {}` beside `export const I = 1` — and the arm answers
`errorType` rather than guessing when the lookup misses.

## Object-literal member symbols: the lookup was never the problem (2026-08-05)

`var o = { a: 1 }; o.a` answered `errorType` — while the literal's own type
printed `{ a: number; }` correctly. **8,549 lines across 1,571 cases.**

The row is named for the declaration kind, and the name points at the wrong
step. `objects.rs` gives the literal an `__object` symbol as its members table,
so `get_property_of_type` **finds** `a` without trouble. What failed was the step
after: `get_type_of_symbol` on a `PROPERTY` whose declaration is a
`PropertyAssignment`, which the variable/parameter/property worker had no arm
for. Two arms in one match, `checker.go:16611` and `:16613`.

### Concentration, checked before predicting

Top ten cases are **24.9%** of the row and the largest single case is 11.7%,
spread over 1,571 cases. That check has changed the answer three times this
cycle — the `+=` row was 96% one file — so it is now run before any prediction.

By initialiser kind: NumericLiteral 2,500, StringLiteral 1,938, AsExpression
1,069, ArrowFunction 630, ObjectLiteralExpression 483, ShorthandPropertyAssignment
409, Identifier 345, booleans 336, ArrayLiteralExpression 156.

### Sharing the call is what pins the widening

Both arms route to `check_expression_for_mutable_location` — **the same function
`objects.rs` calls to build the literal's printed member text**. That is a
deliberate choice over writing the equivalent rule twice, and it buys two things.

The member symbol's type and the literal's printed text cannot drift, because
there is one rule rather than two that agree today. And the widening boundary
comes for free: `var o = { a: 1 }` gives the member `number`, not `1`, because
freshness stops at the property boundary.

That second point is the failure mode this arm existed to avoid. Written with a
plain `check_expression`, every numeric member would print `1` where upstream
prints `number` — **2,500 gaps converted into 2,500 wrong answers**, which this
cycle established is a worse trade than leaving them. The mutation that swaps
the widened call for the raw one reddens three tests, so the boundary is pinned
rather than assumed.

`const o = { a: 1 }` also gives `number`: `as const` is what would keep `1`, and
it is unported. The widening is a property of the property boundary, not of how
the object was bound.

## State at handoff (2026-08-05)

Written because this file is the handoff, and a handoff that only records
successes is the same failure as a doc that outlives its truth.

### Landed in this workstream

Expression forms in `expressions.rs` and `calls.rs`: `typeof`, prefix and postfix
unary, conditional, `new C()`, `yield`, tagged templates — 7 of the 8 assigned,
~8,181 lines. Object literals in `objects.rs`: shorthand, numeric names, quoted
names. Accesses: an `any` receiver in `indexed.rs` and `members.rs`. Symbols in
`symbols.rs`: export markers, and object-literal member symbol types.

### Outstanding predictions — SCORED 2026-08-05 (cycle 10)

Each is a verified single-commit window (`git log --oneline X^..X` → 1). All six
runs were done by the lead, serialised, each in its own
`git worktree add --detach` with its own submodule checkout, so no teammate's
edits are inside any measurement.

| Pair | Item | Predicted | **Measured** | Cases | Verdict |
|---|---|---|---:|---:|---|
| `1bc1f97^..1bc1f97` | tagged templates | ~100 of 289, range 40–110 | **+135** | +6 | miss, 1.23× over the stated ceiling |
| `af7f12a^..af7f12a` | export markers | 3,000–6,000, ~4,000 | **+1,134** | +11 | **miss, 3.5× high** |
| `f6ffe77^..f6ffe77` | member symbol types | 6,000–11,000, ~8,000 | **+6,605** | +109 | **hit** (in range, 1.21× low) |

Raw figures, assertion lines out of 478,954: 273,367 → 273,502; 278,387 →
279,521; 282,021 → 288,626.

**The scoreboard for this project's predictions is now 4 hits, 7 misses**, after
two further exact hits landed later the same day in `f40cb78` — the label arm
re-derived with libs at **594 predicted / 594 measured**, and the binding-element
property name at **428 predicted / 428 measured**.

Those two matter out of proportion to their size, because they are the first
predictions here made from a *repaired method* rather than from a fresh count.
The label arm's earlier 209→597 miss diagnosed its own cause — an attribution
order that hides subordinate arms under the dominant one — and left the
multiplier unknown; sizing from the flattened run instead answers it directly.
Predicted twice, exact twice. See `docs/conventions.md`, "Size a positional arm
from the flattened run, never from the attributed one".

The correlation below therefore still holds at 11 for 11: every hit was
cross-checked against the instrument before it was quoted.

#### Tagged templates: the pre-registered falsifier is mildly triggered

The prediction carried one: *"If the measured movement is far above ~110,
something is answering that should be gapping — most likely a generic tag
slipping through, and the fence needs checking rather than celebrating."* +135 is
above 110 and is not *far* above it, so this is a flag rather than an alarm — but
it is the author's own stated condition and it should be checked rather than
rounded away. The specific thing to check is whether a generic tag is being
answered; the fence is the single-call-signature restriction inherited from
`resolve_call_signature`.

#### Export markers: the miss is the "counted where the form appears" failure

The population for this arm was **measured** in this same file at *"3,144 lines
across 463 cases"*. The prediction's range was **3,000–6,000** — that is, its
floor was the whole measured population and its ceiling was **twice** it. A
prediction whose range starts at 100% conversion has no room to be right, and
1,134 is 36% of the population, which is an unremarkable conversion rate.

This is `docs/conventions.md`'s *"count the lines a failure blocks, not the lines
where the form appears"*, arriving from a direction that section does not yet
cover: **the count here was already of blocked lines, and the error was assuming
they would all convert.** A measured population is a *ceiling*, and quoting a
range whose floor equals the ceiling converts a good measurement back into a
guess. The corrective is one line: state the expected conversion *rate*
separately from the population, and say what would make it low.

#### Member symbol types: the one that hit, and why

Predicted 6,000–11,000, delivered 6,605. It is the second hit in nine, and it
shares the property of the first: **the author counted assertion lines directly
rather than source occurrences, and checked the count against the instrument's
own histogram before quoting it** — 8,549 lines across 1,571 cases, enumerated by
initialiser kind (NumericLiteral 2,500, StringLiteral 1,938, AsExpression 1,069,
ArrowFunction 630, …) rather than by summing the two visible histogram rows. The
enumeration is what makes it a prediction; `docs/conventions.md` records the same
bucket being sized at 4,300 by adding its two visible rows, a 2× under-count.

Both hits so far were cross-checked against the instrument before quoting. All
seven misses arrived with a mechanism story and no cross-check. That correlation
is now 9 for 9.

#### This is the slice `docs/conventions.md`'s level-4 section is about

The 6,605 lines and 109 cases here are the *same event* as the "6,610 lines, 109
cases, 6.9% against a predicted 10–25%" recorded under *"A fourth level, and it
is the one that predicts cases"*. The two line counts differ by 5 and the case
counts are identical, which is a reassuring independent re-derivation. **61 lines
per case flipped** — the broad-and-shallow signature. Noted here because that
section cites the number without naming the commit, so a reader could not
otherwise connect the rule to the slice that bought it.

#### A trap this measurement walked into, recorded because it nearly landed

`crates/tsr-conformance/snapshots/*.snap` is **checked in**, so a worktree at any
commit already has a snapshot file on disk before anything runs. Reading it gives
a number that looks exactly like a fresh measurement. The first read of
`f6ffe77`'s snapshot returned `273502/478954`, which is `1bc1f97`'s figure —
snapshots had not been regenerated between those commits — and it would have been
recorded as "this slice moved nothing".

What caught it was `git status --porcelain` on the snapshot path: five of the six
worktrees showed ` M` and one showed clean, and only the modified ones had
actually been written by a run. **A snapshot file's existence is not evidence
that a run produced it.** Same family as the corpus rule already in `bd`
memory — *a missing baseline file is only evidence when some other baseline
proves the case was run* — and it is the inverse: a *present* file is only
evidence when its mtime or git status proves the run wrote it.

### Blocked, with the blocker named

- **ES imports / cross-file aliases** (~869 lines). `Checker::new` takes only
  `(binder, nodes, node_map)`; the checker has no path or module awareness and
  `BindResult` exposes no specifier→file map. Needs module resolution plumbed
  into the checker — a `Checker::new` signature change, not a `symbols.rs` arm.
  **`enums`' same-file scoping in `964ec88` was right**; it named the wrong
  blocker (ADR-0034 globals, which do work) and reached the right conclusion.
- **Array and tuple receivers** carrying no members. `create_type_reference`
  (`declared.rs:489`) builds with `symbol: None`. `bd tsr-4qx`.
- **`TemplateExpression`** (1,036 lines). Needs a constant evaluator plus
  const-context detection plus contextual typing — a workstream, not a slice.
- **Contextual typing** owns the 555 wrong property-access lines this workstream
  created. Named at the arm in `members.rs`.

### The thing most likely to be rediscovered

`get_type_of_symbol`'s dispatch is where three separate rows turned out to live:
export markers, object-literal member symbols, and the alias arm. Each was a
missing `match` arm rather than missing machinery, and in each case the row's
*name* pointed at the wrong step — the lookup already worked. Anyone ranking a
row that mentions a symbol flag or a declaration kind should check that dispatch
before believing the row is about what it says it is about.

## The empty literal under non-strict options: `undefined[]` (2026-08-07, ninth session)

**Found by the fresh wrong-bucket split** (`wrongflip.rs` at `0a609cc`, wrong
35,859): the W2 board's third row is the exact substitution **upstream
`undefined[]` / ours `never[]` — 212 ROOT lines, 76 cases, 7 finishes**, head
cases `externalModuleImmutableBindings` (16), `typedArrays` (11),
`contextualTyping` (10). Diffuse, which is what makes a 212-line row worth an
arm.

**The mechanism is one branch this module documents itself skipping.** The
module doc above says *"`checker.go:8098` — `implicitNeverType` under
`strictNullChecks` and `undefinedWideningType` without it"* and then assumes
the option on. That assumption predates `set_strict_null_checks`
(`checker.rs:478`) — the flag has been per-case real since the `856972a` build,
and the empty-literal arm never started reading it.

**Attribution from the baseline, not the row name** (`typedArrays.types:177`):

```
    var typedArrays = [];
>typedArrays : any[]        <- the DECLARATION widening, unported (bd tsr-mli), stays wrong
>[] : undefined[]           <- the literal, this arm, converts
```

**Sizing.** Converts: the 212 measured exact-substitution lines, plus an
unsized composite share (corpus-wide `: undefined[]` appears on 384 lines in
138 baselines; the difference is composites, gaps, and declaration lines like
the `any[]` above). At-risk: **structurally zero** — a baseline line records
`undefined[]` only where the case's options resolve `strictNullChecks` off, and
in exactly those cases upstream *cannot* record `never[]` from this arm; strict
cases take the unchanged branch. Would-wrong: lines where the `undefined`
element then feeds a mechanism that diverges under the new constituent —
unions drop nothing here (`undefined` is kept even non-strict in this port's
constructor per `unions.rs:414` — NOTE: that arm drops nullable-only unions
when non-strict, which is a single `undefined` element... **checked before
registering**: `get_union_type` of one element returns the element unreduced,
so the dropping arm is not reached).

**Bar, registered before any code:**

1. net ≥ **+150** (~70% of the exact row; the composite share is upside, not
   floor).
2. own new wrong ≤ **10**; global Δwrong may include downstream re-exposure but
   `RIGHT→WRONG` in **strict** cases must be **0** — that is the falsifier: a
   strict-case loss means the flag read is wrong, not a trade.
3. cases regressed == **0**.
4. lost (RIGHT→GAP) == **0** — the arm changes which type it answers, never
   whether it answers.

Falsifier bis: if conversions land far *below* 150, the 212 lines are not at
the literal's own node (wrongflip ROOT classification wrong for this row) —
re-read the dump before touching the arm.

## §5 The identical-constituent union — `E | E` needs no origin machinery

`enumLiteralsSubtypeReduction` (513 wrong lines, want `E[]` got `error[]`)
decodes in `union_type_worker`'s named-union decline (`unions.rs`): the
512-member enum's declared type is a NAMED union, and a union containing
one expands its members, so the decline answers `error` to avoid printing
`E.E0 | E.E1 | …` where upstream writes `E` (the missing denormalised
`origin`, `checker.go:25705`). But `[E.E0, E.E1]`'s element union is
`union(E, E)` — every input is the SAME type, and `T | T = T` for any `T`
by identity, named or not. The fast path answers before expansion ever
happens.

**The bar**: the case's 513 flip to `E[]`. Falsifiers: (a) any change
outside identical-input unions — the path must trigger only when every
input `TypeId` is equal; (b) the named-union decline for genuinely mixed
inputs must stay (its falsifier population: `E | string` prints).

**§5 score — LANDED.** **+526 WRONG→RIGHT / 2 GAP→WRONG** — the 2 are
`nullIsSubtypeOfEverythingButUndefined`'s `union(null, null)` under
non-strict, where the empties-set decline used to answer `error` and
identity now answers `null` against a want of widened `any`: the
null-widening intrinsic gap (`with_module_host`'s recorded divergence),
surfaced one step closer, not a new mechanism. `checker_types` right
368,720 → **369,246**, wrong 23,212 → **22,688**. The case's residual line
is the return aggregate's fall-through `| undefined`, a different (priced)
mechanism.

## §6 Array spreads contribute their element type

`SpreadElement` (402 TERMINAL lines): `[a, ...xs]` gaps whole today. The
ported slice: a spread whose operand types as `Array<T>` (the
`type_reference_targets` unwrap the `await` build introduced) contributes
`T` to the element union — `getSpreadElementType`'s array half. Tuple
spreads, iterables, and strings stay declines (error whole, the status
quo). Falsifier: contextual/tuple positions where upstream spreads
per-element — those want tuples, and an `Array<T>` answer there is wrong;
counted by the measure.

**§6 score — LANDED.** **+85 GAP→RIGHT / 22 GAP→WRONG** — the 22 are the
falsifier's own population (contextual and tuple spread positions wanting
per-element results), measured and accepted. `checker_types` right
372,144 → **372,229**.

## §6.1 The spread's own line

The TERMINAL row survived §6 whole because it counts the `SpreadElement`
node's OWN assertion line — `...xs : number[]` prints the operand's type
verbatim (`checkSpreadExpression`, `checker.go` — `checkExpression(node.
Expression())` is the whole of it), and `check_expression` had no arm.
Falsifier: none beyond the measure; the rule has no alternative.

**§6.1 REFUSED at +23/388.** The spread's own line converts, but every
CALL whose argument list contains a now-typing spread stops gapping and
answers through selection machinery that cannot see spread arity — 388
confident wrongs across decorator/binding/argument cases against 23
gains. The arm reverts whole; the row stays TERMINAL with this number
recorded. The prerequisite is spread-aware call resolution, not the
expression arm.

**§6.1 postscript — the study the refusal mandated.** The corpus holds
**zero** gap lines whose own text is a `...` spread with a type assertion:
the 402-line TERMINAL row is entirely ROOT-CAUSE attribution — calls and
literals whose failure traces TO a SpreadElement, never the spread's own
printed line. The expression arm therefore had no population of its own to
convert (+23 were coincidental), and the 388 wrongs were its whole effect.
Confirms TASK item 8: the consumer (spread-aware call resolution) is the
only road into this row.

## §7 Literal-free element unions are context-independent

The contextual refusal's mechanism-level re-open, for one provable slice:
`isLiteralOfContextualType` (the refusal's engine) preserves LITERAL
types under a contextual type — an element union with **no fresh-literal
constituent** cannot differ between contextual and uncontextual positions,
so the §13 admission extends: two-object arrays whose elements carry no
FRESHABLE literal admit REGARDLESS of position.
`generatedContextualTyping`'s 900 lines are the head (class-instance
elements under annotated targets; the wants equal the uncontextual
answers throughout). Falsifiers: (a) an admitted line whose want differs
from the uncontextual union — the context-independence claim is then
WRONG for that shape and the slice narrows; (b) the §13-admitted
population must not move.
