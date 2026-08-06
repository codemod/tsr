# Checker notes — type predicates

`function f(x: unknown): x is string`, and the `asserts` forms. `bd tsr-rkdc`.

This page is the reasoning behind the arm that gives `getTypeFromTypeNode` a
`TypePredicateNode` case and gives a signature's printed return position the
predicate instead of the type. It is written **before** the code, because §2
below is a pre-registered keep/revert bar and a bar written afterwards is not a
bar.

---

## 1. Two mechanisms wear the same name, and only one of them is this item

A type predicate is used for two things, and they are not one piece of work:

1. **Printing.** A signature whose return annotation is a predicate prints
   `(x: unknown) => x is string`, not `(x: unknown) => boolean`. Upstream does
   this in the node builder — `serializeInferredReturnTypeForSignature`
   (`nodebuilderimpl.go:1745`) asks `getTypePredicateOfSignature`
   (`relater.go:2016`) and, when there is one, emits a `TypePredicateNode`
   through `typePredicateToTypePredicateNodeHelper` (`nodebuilderimpl.go:1765`)
   in the slot `typeToTypeNode(returnType)` would have filled.
2. **Narrowing.** `if (isString(x)) { … }` narrows `x` at the call site, which
   is `narrowTypeByTypePredicate` and belongs to the flow walk.

**The 754 lines `depend.rs` reports on the `TypePredicate` root are the first
mechanism, and the walk establishes it by construction rather than by
inspection.** `depend.rs` follows *declaration* edges: a line's identifier goes
to its symbol's declaration, the declaration's name goes to its annotation, and
the root is the first node in that chain that gaps with no gapping dependency of
its own. A narrowing failure has no such chain — the guarded reference's
declaration types fine, so its root is never the predicate node — and a
narrowing failure produces a **wrong** line (the declared type, printed
confidently) rather than a gap. This root is a gap root. Independently,
`predgap.rs` reports the root's parent kind for all 685 lines it classifies:

```
  613  FunctionDeclaration
   37  MethodDeclaration
   34  MethodSignature
    1  GetAccessor
```

Every one is a *declaration whose return annotation is the predicate*. Not one
is a call site. The item is printing.

### The two halves of the printing arm

- `getTypeFromTypeNode`'s `ast.KindTypePredicate` case (`checker.go:22858`)
  answers **`voidType` under an `asserts` modifier and `booleanType`
  otherwise**. That is what stops the *signature* gapping: today
  `return_type_of` calls `get_type_from_type_node` on the annotation, gets
  `errorType`, and `get_signature_from_declaration` answers `None` for the whole
  declaration under its all-or-nothing rule.
- The printed return position takes the predicate text instead. Rendered from
  `createTypePredicateFromTypePredicateNode` (`relater.go:2084`) — the written
  parameter name, the `asserts` flag, and the predicate's type resolved through
  `getTypeFromTypeNode` — and emitted by the printer's `emitTypePredicate`
  (`printer.go:1869`).

Both halves are visible in one corpus line, and the pair is what makes the
distinction testable rather than asserted:

```text
>isFunction : (x: any) => x is Function        the declaration — the predicate
>isFunction(x) : boolean                       a call to it — the type
```

`conformance/assertionTypePredicates1` records the `asserts` counterpart,
`>assertDefined(x) : void`.

### What is *not* built, and is refused rather than approximated

- **Inferred predicates.** `getTypePredicateFromBody` (`checker.go:20535`)
  gives an *unannotated* function whose body refines a parameter a predicate it
  never wrote. That is a different input (no `TypePredicateNode` exists) and a
  different mechanism; nothing here touches it, so it neither improves nor
  regresses.
- **Narrowing by a predicate at a call site.** Above.
- **Composite predicates** over union/intersection signatures
  (`getUnionOrIntersectionTypePredicate`, `relater.go:2049`) and
  `instantiateTypePredicate`'s mapper path. Both need signature machinery this
  arm does not touch.

None of the five *written* sub-forms is refused, and that is a finding rather
than a convenience: `x is T`, `this is T`, `asserts x`, `asserts x is T` and
`asserts this is T` all print from the same three fields of the same node, so
there is no sub-form that would have to be approximated. `predgap.rs` counts
them:

```
  588  x is T
   42  asserts x is T
   31  this is T
   23  asserts x
    1  asserts this is T
```

---

## 2. The counterfactual, and the bar it licenses

### 2.1 The counterfactual — `crates/tsr-conformance/examples/predgap.rs`

754 is a **ceiling**, and `docs/conventions.md` requires the *match* test. So
the probe computes the line the arm would print and compares it to the baseline
string for string.

The forecast is not re-implemented — re-implementing the signature printer
would forecast the probe's spelling rather than the compiler's, which is the
error `newgen.rs` was written to avoid. It is taken from the compiler by
**de-predication**: every `TypePredicateNode` span in the case's own units is
replaced with a fresh unresolved type name, the case is re-checked, and the port
prints the whole line for real — an unresolved type reference prints the name
that was written (`d356450`, `bd tsr-eep`), so the marker survives into the
output. Substituting the arm's two answers back for the marker gives the exact
string the arm would print, with **every other part of the line produced by the
code that will produce it after the build**.

Measured at `87adff4`:

```
classified (gap line whose root is a TypePredicateNode)   685

  545   CONVERTS — forecast matches the baseline exactly
   61   SKIPPED — the de-predicated run does not line up (C4)
   32   MISS — forecast differs
   27   still gaps once de-predicated — blocked elsewhere
   20   the root predicate's own type gaps

  C1 classified-but-not-gap:          0   (expect 0)
  C2 verdicts sum 685 vs classified 685
  C3 converting without a marker:     0   (expect 0)
  C4 cases skipped for misalignment: 17 cases / 61 lines
  C5 classified lines wanting `any`:  0   (expect 0)
```

**685 and not 754** because this probe walks the chain itself and drops lines
`depend.rs` bins differently at the margin; the gap between the two figures is
not claimed as anything and is not counted as work.

C5 is pinned to the upstream construct rather than to a summary of it — the
rule this document's parent learned from `removeSubtypes`. `getTypeFromTypeNode`
answers `boolean`/`void` for a predicate and never `errorType`, so no classified
line can be wanting `any` *for the predicate's own sake*. Its value is fixed by
`checker.go:22858` and not by my reading of the row. It reads 0.

**The probe's first run was wrong and its own control said so.** It substituted
the predicate text at every marker and reported 204 misses, whose head was
`want isFunction(x) : boolean` against `got isFunction(x) : x is Function`. That
is the *type* half of the arm arriving in a call-result position, and the
correction — substitute the predicate only after `") => "` or `"): "`, the two
return-position spellings this port has — moved 172 lines from MISS to CONVERTS.
Recorded because the number 545 would otherwise look like it came from a probe
that was right the first time.

### 2.2 What the 32 misses are

Read before the bar was written, because a miss population is where the
mechanism boundary shows:

| lines | miss | owner |
|---:|---|---|
| ~10 | `<T_1>` where we print `<T>` | type-parameter renaming, not ported |
| ~8 | `value is ("foo" \| "bar")`, `value is (string & Tag)`, `a is string \| null` written-order unions | `bd tsr-5o2` — written-node reuse in signature prints |
| ~4 | `obj is PartialUser` where we print `obj is Partial<User>`; `result is FAILURE` vs `"FAILURE"` | alias naming |
| ~5 | `kind: "A" \| "A"` — a *parameter* type we print as `"A"` | overload/union reduction, not the predicate |
| 2 | `value is never` where the baseline says `(string & Tag2)` | intersection reduction |
| 1 | `=> I is any` where the baseline says `=> I` | unexplained; one line, filed |

**Every one of them is a family this port already owns somewhere else**, and
none is a rule specific to predicates. That is the observation the falsifier
below is built on.

### 2.3 The registered bar

Registered at `87adff4`, before any checker code exists. Scored in §3.

- **Leg 1 — net floor.** Keep only if the measured net is **≥ +400 lines**.
  Derived from the counterfactual's **545** exact matches with a ~25% discount,
  *not* from the 754 row and not from the 685 classified. What input makes it
  non-zero: every classified gap line that starts printing; the denominator is
  545 and it is not empty.
- **Leg 2 — lost.** Keep only if **lost ≤ 5 lines**. Every line the arm touches
  directly is a gap today — `get_type_from_type_node` answers `errorType` for
  every predicate node in the corpus — so the arm cannot lose one by its own
  action. A loss can only come from the cascade: a signature that used to gap
  now resolves, and some *other* line that was right under the gap goes wrong.
  That input exists (the 545 sit in 130-odd cases with ~36,000 right lines
  between them), so the leg has a denominator; a reading above 5 means the
  cascade is doing something this page did not model, which is a bug and not a
  trade. This is the absolute-zero shape and not a ratio, for the reason
  `docs/conventions.md` gives: here a loss is evidence of a wrong rule.
- **Leg 3 — case regression.** Keep only if **cases regressed < cases
  finished**, on `casedelta.rs` joined over a `git stash`.
- **Leg 4 — gap→wrong.** Keep only if **gained ≥ 4 × new wrong**, with new
  wrong from `wrongdelta.rs` at both ends (`comm -13` on sorted output).
  Forecast: 32 forecast misses plus at most the 61 C4-unforecast lines = ≤ 93
  against 545, i.e. ≥ 5.9×. A 4× bar has room for the unforecast and fires if
  the arm mints wrong lines outside the families §2.2 names.

**Falsifier.** *The misses are other people's families, so the arm should render
the predicate's type from the **computed** type (`typeToTypeNode`, upstream's
own choice) and not from the written node.* This is wrong if the residual new
wrong lines are dominated by a disagreement that only appears **after `is`** —
a written-versus-computed difference that this port renders correctly in every
other annotation position. If that is what the residual says, the boundary is in
the wrong place and the predicate's type must come from the written node
instead, which is a different build.

**A leg that could not fire was replaced before the run.** The first draft of
leg 2 was a `gained ÷ lost ≥ 3.0` ratio copied from the earlier refusals. It
has the empty denominator `docs/conventions.md` names — the row is entirely gap
today, so the ratio's divisor is structurally zero and the leg would have
"passed" by being vacuous. The absolute form above is the replacement.

---

## 3. Scoring against the bar

*(Filled in after the measurement; see the session log in `STATUS.md`.)*
