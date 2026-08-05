# 0040 — Diagnostics come from a check traversal, and assignability gets a reporting twin

**Status:** accepted, 2026-08-05. **Nothing is built.** This records how
diagnostics will enter this port and what bounds them; the work is `bd tsr-6re`
and `bd tsr-8yu`. Upstream references are `vendor/typescript-go` @ `5b1047d10`,
each `grep -n`-verified.

## The forcing constraint

`diagnostics` has read **80/5,488 (1.46%)** across cycles 8, 9 and 10, unmoved by
a `checker_types` gradient that went 36% → 60.9% in the same period. The reason is
structural: `tsr-checker` emits **no diagnostics at all** — `grep` for
`Diagnostic` across `crates/tsr-checker/src` returns two hits, both test helpers
reading *parser* output.

The suite's rule is **exact multiset equality** on `(file, line, column, code)`,
so an invented diagnostic fails a case exactly as a missing one does.

Measured over the 7,025 diagnostic-bearing baselines:

| | cases | share |
|---|---:|---|
| reachable with scanner + parser + binder only | 1,063 | 15.13% |
| **+ TS2322** | **1,599** | **22.76%** |
| + assignability family (2322/2345/2739/2740/2741) | 1,808 | 25.74% |
| cases whose *only* code is TS2322 | **516** | |

**TS2322 alone is +536 cases — 6.7× the entire current pass count.** It is the
largest single reachable item, and `Checker::is_type_assignable_to`
(`crates/tsr-checker/src/relater.rs:151`) already exists.

## What upstream does

### Diagnostics are produced *inside* the checker, not by a consumer afterwards

`c.error(location, message, args...)` (`checker.go:13999`) builds a diagnostic and
calls `c.addDiagnostic`, appending to `c.diagnostics`, an
`ast.DiagnosticsCollection` **field on the Checker** (`checker.go:661`). The
public entry `GetDiagnostics` (`checker.go:13951`) *drains* that collection; it
produces nothing itself.

### But reporting is a **second traversal**, not a hook on the type query

`getDiagnostics` calls `checkSourceFile` (`checker.go:2196`) **first**, then reads
the collection. `checkSourceFile` runs grammar checks, walks statements via
`checkSourceElements` → `checkSourceElement` (`:2241`), runs deferred nodes, and
produces deferred diagnostics. Errors are an eager side effect of that walk.

That walk is a *different entry point* from the one the `.types` baseline uses.
The baseline writer goes through `getTypeOfNode` (`checker.go:31927`) — a query.
Two roads into one engine:

| entry | purpose | our equivalent |
|---|---|---|
| `checkSourceFile` → `checkSourceElement` | reports diagnostics | **does not exist** — `grep` for `check_source_file`/`check_source_element` in `crates/tsr-checker/src` returns 0 |
| `getTypeOfNode` | answers "what type is this node" | `types_producer::type_at_location` |

**This port has built the query road and none of the traversal road.** That is the
single fact that explains a flat `diagnostics` number beside a rising
`checker_types` one, and it is why no amount of `.types` work can move the suite.

### The relation has a silent form and a reporting form, sharing one engine

```
relater.go:150  isTypeAssignableTo(source, target) bool
relater.go:340  checkTypeAssignableTo(source, target, errorNode, headMessage) bool
relater.go:426  checkTypeAssignableToAndOptionallyElaborate(source, target, errorNode, expr, …)
```

Both funnel into the same relation walk; the difference is whether `errorNode` is
non-nil. **Upstream does not call the boolean and format a message at the call
site.** It passes the error node *into* the relation.

### The message is an output of the walk, not of the call site

`diagnostics.Type_0_is_not_assignable_to_type_1` appears at **exactly one place in
the entire checker package** — `relater.go:4796` — at the bottom of a `switch`
that first tries several more specific messages: the comparable relation, "two
different types with this name exist but they are unrelated" when
`sourceType == targetType`, an `exactOptionalPropertyTypes` variant, and a
"Did you mean" suggestion for a string literal against a union. Above it sits
`r.errorChain`, the elaboration ("Types of property 'a' are incompatible…").

So the **code, the text and the position** of TS2322 are all products of the
structural walk. A boolean cannot yield them.

For the motivating case, `const x: number = "hello"`, the site is
`checkVariableLikeDeclaration` at `checker.go:5899`:

```go
initializerType := c.checkExpressionCached(initializer)
c.checkTypeAssignableToAndOptionallyElaborate(initializerType, t, node, initializer, nil, nil)
```

Note the split: the **error node is `node`** — the declaration — while the
elaboration expression is `initializer`. That is why upstream reports at the
declaration name, and getting it wrong fails every case while looking correct.

## The decisions

1. **Diagnostics are produced inside `tsr-checker`, appended to a collection on
   the `Checker`, and drained by the consumer.** Not synthesised by a consumer
   from types it queries.
2. **A `check_source_file` traversal is built as a second entry point**, distinct
   from `type_at_location`. It is new machinery, not a hook.
3. **`is_type_assignable_to` gets a reporting twin taking an error node**, rather
   than a formatter wrapped around the boolean.
4. **Emission is bounded to the domains where a `false` from our relater is
   trustworthy** — and this is the one deviation from upstream with no
   counterpart there.

### Why (4) exists, and why it is ours rather than upstream's

Upstream's relater is complete, so `false` means *not related*. Ours is not:
[ADR-0037](0037-object-types-are-compared-structurally.md) records that
`is_type_assignable_to` answers `false` for two distinct object types because
structural comparison is **narrow**, not because they are unrelated.

`docs/conventions.md` states the consequence — *a conservative `false` is safe for
one kind of consumer and unsafe for the other*. **A diagnostic emitter is the
second kind**: it acts on the negative. Overload selection was the first such
consumer in this checker, and the containment invented for it is `SELECTABLE`
(`crates/tsr-checker/src/calls.rs:75`): `STRING | STRING_LITERAL | NUMBER |
NUMBER_LITERAL | BIG_INT | BIG_INT_LITERAL | BOOLEAN | BOOLEAN_LITERAL | VOID |
UNDEFINED | NULL | NEVER` — the domains `isSimpleTypeRelatedTo` decides on flags
alone.

`const x: number = "hello"` is `STRING_LITERAL → NUMBER`: squarely inside.

## Alternatives, taken seriously

**Emit from a consumer that walks the AST after checking, calling
`is_type_assignable_to`.** This is the shape the question "is it the checker or an
aftermath?" naturally suggests, and it is the cheapest thing to build. **Rejected
on the message, not on taste.** With only a boolean, a consumer can emit the
generic TS2322 and nothing else — so it is wrong on every comparison where
upstream's `relater.go:4796` switch picks a more specific message or attaches an
elaboration chain. Under exact-multiset equality a *wrong* diagnostic costs the
same as a missing one, and it is worse than missing because it is a false positive
that `examples/over_reports.rs` exists to hunt and that is invisible today while
almost everything fails for too few.
*What would make it win:* if, restricted to the `SELECTABLE` domain, upstream's
switch provably always falls through to the generic message and never builds a
chain. That is plausible for primitives and it is **not checked**. If it holds,
the consumer shape is adequate for the first slice and decision (3) is
over-engineering for it — see the falsifiers.

**Emit on every `false` the current relater returns.** Rejected: it manufactures a
false positive for every object-to-object comparison the relater is too narrow
for, and those failures land in cases that would otherwise pass.

**Wait for a complete relater, then port upstream's reporting wholesale.** Not
rejected on principle — it is what makes decision (4)'s bound unnecessary, and it
is the clean end state. Rejected as the *next* step because it is a multi-thousand
line workstream and the bounded emitter is reachable now.

## Consequences accepted

- **We will under-report by construction, and under-reporting fails cases too.**
  A bounded emitter only converts a case where *every* diagnostic that case
  expects is inside the bound. That is why the deliverable is not "536 cases" —
  see below.
- A second traversal is real machinery with its own ordering hazards, and it will
  duplicate node visits the query path already makes.
- `SELECTABLE` becomes load-bearing for a second consumer, which raises the cost
  of widening it (`bd tsr-6v7`) rather than lowering it.

## How we would know this is wrong

- **The sizing falsifier, and it is the first thing to measure.** Of the 516
  TS2322-only cases, how many are `SELECTABLE`-shaped rather than
  object-to-object? **This is not measured and must not be guessed** — the
  kind-1/kind-2 section of `docs/conventions.md` records two defensible proxies
  bracketing one such answer by **31×**. If most of the 516 are object-to-object,
  the real item is the complete relater and this ADR's bounded emitter converts
  almost nothing.
- **The message falsifier.** If upstream's `relater.go:4796` switch selects a
  non-generic message, or `errorChain` is non-empty, for primitive-to-primitive
  comparisons, then the reporting twin is doing work the call site cannot — and
  decision (3) is confirmed. If it never does, decision (3) is unnecessary for
  the first slice and the simpler consumer shape should be taken instead.
- **The position falsifier.** If our emitter reports at the initialiser rather
  than the declaration name, every affected case fails on column while the types
  are right. `checker.go:5899` passing `node` as the error node and `initializer`
  as the elaboration expression is the discriminating detail.
- If `diagnostics` moves and `over_reports.rs`'s buckets grow at the same time,
  the bound is leaking and decision (4) is not being enforced where it is claimed.

---

## Measured, 2026-08-05 — two of the three falsifiers fired

Recorded here rather than by editing the text above, per
`docs/conventions.md`: the wrong turns are the useful part of the archive. The
instrument is `crates/tsr-conformance/examples/assignability_shape.rs`; the full
working is `docs/architecture/checker-notes-diag.md`. **No emitter was built, and
these numbers are why.**

### Correction: the sizing table above is over the wrong denominator

The `7,025` / `516` / `+536` figures are counts over the **7,027 `*.errors.txt`
files**, of which **1,180** are configuration variants (`case(target=es5)…`) that
the `diagnostics` suite excludes by construction
(`CaseEntry::has_varied_errors`, `crates/tsr-conformance/src/corpus.rs:163`).
Over the suite's own **5,488** judged cases:

| | as written above | measured |
|---|---:|---:|
| scanner + parser + binder only | 1,063 / 15.13% | **765 / 13.94%** |
| + TS2322 | 1,599 / 22.76% | **1,254 / 22.85%** |
| + assignability family | 1,808 / 25.74% | **1,445 / 26.33%** |
| TS2322 gains | +536 | **+489** |
| cases whose only code is TS2322 | 516 | **478** |

The shares are nearly identical and the counts are not, which is why this went
unnoticed. `+489` remains the largest single reachable item.

### Falsifier 1 (sizing) fired, and it fired against decision (4)

Of the **478** judged TS2322-only cases, **89** have every TS2322
primitive-to-primitive, 25 are mixed, and **364 (76.2%) have none**. This ADR
wrote the consequence in advance — *"If most of the 516 are object-to-object, the
real item is the complete relater and this ADR's bounded emitter converts almost
nothing"* — and that is the measured outcome.

89 is an **upper bound** (it classifies upstream's printed type names, not ours).
The bucket that decides the *first slice* — cases whose every TS2322 sits on a
`VariableDeclaration` name, the one call site this ADR names at
`checker.go:5899` — is **15**, or 0.27% of the suite.

### Falsifier 2 (message) fired, and it fired against decision (3)

Within the `SELECTABLE` bound, upstream's `relater.go:4780`–`4797` switch
*provably always* falls through to the generic message, for a reason this ADR
missed: **each arm above the fallthrough carries a different diagnostic code** —
2678 comparable (`diagnostics_generated.go:1629`), 2719 two-different-types
(`:1707`), 2375 exactOptionalPropertyTypes (`:1103`), 2820 did-you-mean
(`:1907`). Filtering on 2322 *is* selecting the generic branch. Empirically:
**0 of 2,888** TS2322 header texts in the corpus are anything else.

So decision (3), the reporting twin, is **over-engineering for the bounded first
slice** — exactly as this ADR said it would be if the falsifier fired. It remains
correct for the general case. The one genuine walk output inside the bound is the
literal generalisation in `reportRelationError` (`Type 'string'`, not
`Type '"hello"'`), which is one `if`, and which the suite does not compare anyway
because it compares `(file, line, column, code)` and not text.

### Falsifier 3 (position): mechanism confirmed, distribution fatal

`GetErrorRangeForNode`
(`vendor/typescript-go/internal/scanner/scanner.go:2588`) maps
`ast.KindVariableDeclaration` to `ast.GetNameOfDeclaration(node)`, so passing the
declaration reports at the name, as this ADR says. But the 137 convertible
diagnostics land on **28 distinct anchors**, and the modal one is
`BinaryExpression / Identifier` (28) — assignment checking — not
`VariableDeclaration / Identifier` (18). `PropertyAssignment` (21) and
`ArrayLiteralExpression` (15) arrive through `elaborateObjectLiteral` /
`elaborateArrayLiteral` and never reach `checkTypeRelatedToEx` at all. The
relation walk is shared; the **positions are not**, and positions are what the
suite compares.

### What this ADR still gets right

Decisions (1) and (2) are untouched: diagnostics are produced inside the checker
and a `check_source_file` traversal is the machinery this port lacks. Decision (4)
— bounding emission to where a `false` is trustworthy — is untouched as a *rule*;
what changed is that the bound turns out to contain 89 cases rather than most of
516, which makes the bounded emitter a poor next step rather than a wrong one.
The clean end state this ADR names, a complete relater, is where 364 of the 478
live.
