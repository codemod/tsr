# ADR-0024 — Static members are exports; interfaces, type aliases, enums and functions are block-scoped

Status: accepted
Date: 2026-08-05
Upstream pinned at `5b1047d10`.
Follows [ADR-0023](0023-the-symbol-table-comes-from-the-container.md).

## Context

ADR-0023 fixed one symbol-table defect and left 190 TS2300 over-reports bucketed by
declaring construct (`bd tsr-y4u.19`). Two buckets were the same *kind* of defect
again — a declaration filed in the wrong table — and between them accounted for 97
of the 190.

### Static and instance members shared a table (34 over-reports)

Upstream's `declareClassMember` (`internal/binder/binder.go:414-419`) splits on
`ast.IsStatic`: a static class member goes to `GetExports(container.Symbol())`, an
instance member to `GetMembers(...)`. We had one answer for both.

The visible symptom was 34 accessor over-reports — 20 `GetAccessor`, 14
`SetAccessor` — from cases like `compiler/useBeforeDeclaration_classDecorators.1`,
which declares `static get x` on line 15 and `get x` on line 21. They collided
because they were in one table, and `GetAccessorExcludes` legitimately collides
with another accessor of the same kind.

**The accessors were the only ones that said anything.** `static m()`/`m()` and
`static p`/`p` merged into a single symbol too, silently, because the
`excludes()` corrections in ADR-0023 had just made `MethodExcludes` and
`PropertyExcludes` permissive enough not to complain. The probe
(`examples/ts2300_probe.rs`) shows it directly: before this change, `class C { static
m() {} m() {} }` produced one symbol with two declarations.

This is worth more than the 34 diagnostics. A static and an instance member of the
same name are different things with different types, and the checker would have
asked one symbol about both.

### Block-scoped declarations were filed in the enclosing function (63 over-reports)

Upstream has two ways into a locals table, fixed per declaration kind:

- `declareSymbolAndAddToSymbolTable` → `GetLocals(b.container)` (`binder.go:444`)
- `bindBlockScopedDeclaration` → `GetLocals(b.blockScopeContainer)` (`binder.go:1249`)

Six kinds take the second: block-scoped variables (`binder.go:1171`), classes
(`:944`), interfaces (`:681`), type aliases (`:693`), enums both `const` and regular
(`:1158`, `:1160`), and function declarations (`:1216`). `locals_owner` had the
first two.

So an interface, type alias, enum or function declared inside a nested block was
filed in the enclosing **function**. This only diverges inside a nested block —
`self.block` and `self.container` are the same node at the top of a function,
because both are set for every `IS_CONTAINER` — which is why it took
`compiler/narrowingOfQualifiedNames`, with `type A` in an `if` block and another
`type A` in a `for` block nested inside it, to surface.

## Decision

Port both. `is_static` (upstream `ast.IsStatic`,
`internal/ast/utilities.go:1048`) selects `Destination::Exports` for a static class
member; `locals_owner` gains the four missing block-scoped flags.

Both are written as extensions of the existing per-kind model rather than as the
container-keyed refactor ADR-0023 deferred (`bd tsr-y4u.22`). **This is the point
at which that deferral should be reconsidered.** ADR-0023 named its own falsifier —
"if a second declaration kind turns out to need the same treatment, the
special-case model has failed" — and the static split is a second special case
against the parent node. The counter-argument, and the reason the refactor is still
deferred: the static split is not the same *shape* of problem. It is not "the table
depends on the container" but "the table depends on a modifier on the declaration
itself", which is a per-declaration fact that a container-keyed dispatch would
still have to consult. The refactor would subsume the type-parameter case and not
this one.

## Consequences

- `classify` now consults the parent for two node kinds and a modifier for a third.
  The model is visibly straining; the next addition should be the refactor.
- `is_static` lists only the kinds that can appear in a class body, and returns
  `true` for `ClassStaticBlockDeclaration` without a modifier check, as upstream
  does. `static` is not valid elsewhere; the parser recovers from it, and honouring
  it on an interface member would move the member into a table upstream never puts
  it in — so the split is additionally guarded on the container being a class,
  which is the only branch of upstream's switch that consults `IsStatic`
  (`binder.go:435`).
- Function declarations are now block-scoped **unconditionally**, matching
  `bindFunctionDeclaration`, which calls `bindBlockScopedDeclaration` with no
  dialect or strict-mode test. `if (x) { function f() {} }` no longer declares `f`
  in the enclosing function.
- One existing test, `a_real_declaration_beats_an_expando_of_the_same_name`, was
  asserting the *old* behaviour, and its own comment said so: "the static member is
  in `members` here rather than `exports`". It now asserts the invariant it was
  named for, and does so more strongly — with `static x` correctly in `exports`, the
  expando `C.x = 2` lands in the same table and has to lose, where before the two
  could not have met.

## How we would know this was wrong

**Neither conformance suite could see either defect.** `binder_symbols` stayed at
8,278/8,449 through both changes, and through all of ADR-0023 — its `.symbols`
baselines do not distinguish a static member from an instance one, nor a block's
locals from its function's. Treating 97.98% as evidence that the symbol tables are
right is unsound, and this is the second session in which the only instrument that
could see a table defect was a diagnostic it happened to produce.

So the falsifier is not a suite number. It is:

- Two unit tests added with these changes, both verified to **fail** when the
  binder change is reverted (`a_static_and_an_instance_member_of_the_same_name_are_different_symbols`,
  `block_scoped_declarations_go_in_the_block_not_the_function`). A test that passes
  either way would have recorded nothing.
- The probe, for the cases the suites cannot reach.

If a future change to `locals_owner` or `classify` moves `binder_symbols` by zero,
that is not evidence of safety.

## Measured effect

TS2300 over-reports, by construct (`examples/ts2300_constructs.rs`):

| | over-reports | `diagnostics` | `binder_symbols` |
|---|---|---|---|
| after ADR-0023 | 190 | 77/5,488 | 8,278/8,449 |
| static/instance split | 154 | 78/5,488 | 8,278/8,449 |
| block-scoped kinds | **92** | 78/5,488 | 8,278/8,449 |

The static split is the first change in either ADR to *gain* a case on
`diagnostics`. The block-scoped fix removed 62 over-reports and gained none, for
the reason ADR-0023 sets out: those cases also miss checker diagnostics, and a case
fails once.

`GetAccessor` and `SetAccessor` went to zero — all 34. `TypeAliasDeclaration` went
63 → 3. Measured on the other classifier, total TS2300 divergences are 286 → 190,
of which 87 are now "upstream reports no TS2300 in this file" (from 185).

Across both ADRs, in one session: **4,723 → 190** total TS2300 divergences, and
4,464 → 87 genuine over-reports, with `binder_symbols` unmoved throughout.

The remaining 92 are dominated by `ExportSpecifier` (26), `ImportSpecifier` (20)
and `ImportClause` (5) — aliases, a different mechanism — plus 15 class
declarations/expressions and 8 module declarations. `bd tsr-y4u.19`.
