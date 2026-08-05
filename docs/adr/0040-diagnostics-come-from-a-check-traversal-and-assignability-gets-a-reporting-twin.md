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
