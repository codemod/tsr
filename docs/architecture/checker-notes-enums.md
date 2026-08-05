# Checker notes — `getTypeOfSymbol`'s remaining arms

Working notes for the enum-member, alias and accessor slices of
`Checker.getTypeOfSymbol` (`bd tsr-4sc.9`). Kept as a **separate file** rather
than appended to [`checker.md`](checker.md): four agents share one checkout, and
a shared markdown file is swept by whoever commits second no matter how
carefully either stages — `git commit -- <paths>` commits the working-tree state
of a path, so path-scoping protects you from a neighbour's edits to *other*
files and does nothing when you are both editing the same one. The lead folds
these notes into `checker.md` at cycle end.

The enum-member and alias sections already live in `checker.md` (under
"An enum member's type is read, not built" and "Aliases: the same-file slice").
This file starts at accessors.

## Accessors, and the one place `any` is the right answer (`bd tsr-4sc.9`)

`getTypeOfSymbol`'s **first** flags branch — `checker.go:16506` — is accessors,
and it answered `errorType` until now. With this, every `SymbolFlags` shape
upstream dispatches on is answered; what remains unported in that function is the
four `CheckFlags` shapes it tests *before* any of them (deferred, instantiated,
mapped, reverse-mapped).

### The prediction, stated before building

Per [`conventions.md`](../conventions.md)'s "Predict which histogram row you move,
and by how much, before measuring":

- **Row:** `declaration name, symbol has no type: SymbolFlags(GET_ACCESSOR)` and
  its `SET_ACCESSOR` twin — an accessor's own `>x : T` line.
- **Magnitude:** upper bound **~871 lines**. Counted over the corpus at
  `2c107a3`: 1,068 `get` and 804 `set` declarations, of which 352 getters carry a
  return annotation and 519 setters carry a parameter annotation. Those 871 are
  what the annotation arms answer.
- **Not claimed:** unannotated getters with bodies (the largest remaining group,
  ~716 getters) gap, so a large share of the 1,872 declarations stays red.
- **Commit pair that would show it:** this commit against the next
  `checker_types.snap` refresh.

If the next measurement moves that row by far less than ~871, the prediction was
wrong and the likely cause is that accessor *declaration names* are not what the
histogram row counts — in which case the arm still helps property access through
`crate::members`, but the row named above is the wrong instrument.

### Upstream's four sources, and which two are here

`getTypeOfAccessors` (`checker.go:18511`) tries, strictly in order:

1. the getter's return annotation — `checker.go:18522` — **ported**
2. else the setter's **parameter** annotation — `checker.go:18524` — **ported**
3. else an auto-accessor property's annotation — `checker.go:18527` — gapped
4. else the getter's inferred body return type — `checker.go:18531` — gapped
5. else `anyType` with an implicit-any diagnostic — `checker.go:18545` — **ported**

The order is the entire content of the function. It is observable only when a
getter and a setter carry *different* annotations, which is why the test uses
`get x(): number` beside `set x(v: string)`; with matching types no ordering bug
could be seen. Swapping the two arms turns that assertion red — verified, not
assumed.

**A setter's annotation is on its parameter, not on the setter.**
`getEffectiveSetAccessorTypeAnnotationNode` (`checker.go:20118`) reads the first
parameter's type. Reading the setter node's own `r#type` slot compiles, type-checks
and silently answers `None` for every setter in existence, falling through to the
`any` arm. Mutating the code to do exactly that prints `any` where `string`
belongs — that is how the trap was confirmed rather than argued.

### Why the fallback is `anyType` here and `errorType` everywhere else

This module's standing rule is that an unported form answers `errorType`, never
`anyType`, so a gap stays separable from a computed answer. Case 5 **inverts**
it: an accessor with no annotation anywhere and no getter body is implicitly
`any`, upstream *computes* that and reports a diagnostic alongside it. Answering
`errorType` would mark a line this port gets right as missing.

That is only safe because case 4 is separated out **first**. A getter with a body
would be inferred upstream — `accessorBodyInTypeContext.types` records
`get foo() { return 0 }` as `>foo : number` — so it must gap rather than fall
into the `any` arm. Without that ordering, every inferable accessor in the corpus
would produce a plausible, wrong, indistinguishable-from-computed `any`: the same
failure the `errorType`-not-`anyType` rule exists to prevent, arrived at from the
opposite direction.

**How I would know this was wrong.** If the next measurement shows the accessor
rows moving but the *wrong-answer* differential rising by a similar amount, the
`any` arm is claiming lines it should be gapping, and the fix is to narrow case 5
to the declarations that provably have no body and no annotation rather than to
treat it as the default.

**What would have to change for case 4 to land.** `getReturnTypeFromBody` exists
in this port as `crate::signatures`' inference path, but it is private to that
module. Reaching it needs one `pub(crate)` entry point there — an edit outside
this slice's file, so it was left alone rather than reached for. That single
change is the whole of case 4, and it is worth roughly the ~716 unannotated
getters.
