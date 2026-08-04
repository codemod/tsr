# ADR-0026 — `class implements X` names nothing, a class expression's name is its own, and a second `export default` is TS2528

Status: accepted
Date: 2026-08-05
Upstream pinned at `5b1047d10`.
Follows [ADR-0025](0025-an-export-specifier-is-an-export.md).

## Context

15 of the 34 TS2300 over-reports left after ADR-0025 were attributed to
`ClassDeclaration` (7) or `ClassExpression` (8). Unlike every previous round, they
were **not one defect**. At 34 total the per-construct buckets had stopped being
informative — a point ADR-0025 anticipated — so the cases were read individually
instead, and they split three ways with no shared cause.

### `class implements number {}` parsed as a class named `implements` (6)

`compiler/classImplementsPrimitive` and `conformance/classExtendingPrimitive`
contain:

```ts
const C4 = class implements number {}
const C5 = class implements string {}
```

`implements` is a *future reserved* word, so it is a legal binding identifier
outside strict mode, and `class implements … ` is genuinely ambiguous between a
nameless class expression whose heritage clause starts at `implements`, and a class
*named* `implements`. Upstream resolves it in
`parseNameOfClassDeclarationOrExpression` (`internal/parser/parser.go:1791-1800`) by
looking one token past `implements`: an identifier or keyword there means a heritage
clause (`isImplementsClause`, `:1806`). We had no such check, so both class
expressions above were named `implements` — and two declarations of `implements` in
one file are a duplicate identifier.

**This one is a parser defect, not a binder defect.** It surfaced in a binder
diagnostic because a wrong name is what the binder is handed.

### A named class expression leaked its name into the enclosing scope (4)

```ts
const C9 = class C implements boolean { }   // collided with `class C` above
```

`bindClassLikeDeclaration` splits on the **kind**, not on whether there is a name
(`internal/binder/binder.go:942-951`): a `ClassDeclaration` goes through
`bindBlockScopedDeclaration`, a `ClassExpression` *always* through
`bindAnonymousDeclaration`, which uses the written name as the symbol's name and
files the symbol in **no table**. The name of a named class expression is visible
only inside it.

The comment at our `anonymous_declaration` asserted the opposite — "a *named* class
expression declares its name into the enclosing block, as upstream's
`bindClassLikeDeclaration` does" — and the `FunctionExpression` arm three lines
below it already had the correct rule, with the correct explanation. So the code
contained both the right answer and a confident statement of the wrong one.

### A second `export default` is TS2528, not TS2300 (4)

`compiler/exportDefaultTypeClassAndValue`,
`compiler/jsFileCompilationBindMultipleDefaultExports`,
`conformance/multipleExportDefault3`, `conformance/multipleExportDefault4`. This is
the `binder.go:224-244` block that `bd tsr-y4u.18` listed as unported and
`bd tsr-y4u.23` tracks: a duplicate under the name `default` is
`A_module_cannot_have_multiple_default_exports`, checked *after* the enum and
block-scoped message choices and overriding both, and carrying no name argument.

## Decision

Port all three. Each is small, and they are independent — but they are committed
together because they are one bucket of the same issue and were measured as a
sequence.

### The one inference

Upstream reaches the TS2528 message through two conditions
(`binder.go:229`, `:238`): `isDefaultExport`, and separately an `ExportAssignment`
that is not `export =` — the second exists because `export default { }` carries no
`default` *modifier* to test. `declare_into` has neither the node nor
`isDefaultExport`, only the resolved name.

Both conditions are implemented here as `name == INTERNAL_DEFAULT`. This is an
equivalence, not a shortcut, and the argument is:

- `getDeclarationName` maps a non-`export =` export assignment to
  `InternalSymbolNameDefault` (`binder.go:302-304`), which covers the second
  condition exactly.
- `declareSymbolEx` names the export half of any default export `default`
  (`binder.go:158`), which covers the first.
- Nothing else in the language can be filed under that name — `default` is not a
  spellable binding — so the two upstream branches and this one test admit the same
  set of nodes.

The alternative was threading an `is_default_export` flag through all eight
`declare_into` call sites. Rejected as noise for no gain, given the equivalence
above; if it ever stops holding, the flag is the fix.

## Consequences

- **The related-info chain is not ported.** Upstream attaches
  `Another_export_default_is_here` / `and_here` /
  `The_first_export_default_is_here` (`binder.go:265-275`). `Diagnostic` carries no
  related information at all yet, and the `diagnostics` suite compares codes and
  positions only, so there is nothing to compare it against. Recorded on
  `bd tsr-y4u.23` rather than left implicit — this ADR does not claim the block is
  fully ported.
- `messageNeedsName` is now tracked as its own variable, matching
  `binder.go:222`. It was previously derived from `enum_conflict`, which was correct
  only while the enum message was the sole nameless one — TS2528 is the second.
- `is_implements_clause` is the first lookahead in the class parser. It costs one
  token scan and only when the cursor is on `implements`.
- **`class implements {}` still parses as a class named `implements`.** That is the
  other half of the ambiguity and upstream keeps it; the parser test asserts both
  directions, because a fix that only ever answered "heritage clause" would pass a
  one-sided test.

### The denominators moved

`binder_symbols` went from 8,449 cases to **8,451**, and `printer_round_trip` from
11,726 to **11,728**. This is the parser fix: two units that previously failed to
parse now parse, so they entered suites that had been skipping them. Both numerators
moved by the same +2, so the *rate* is unchanged at 98.02% and 99.52%.

Worth stating explicitly because a moving denominator is the failure mode that has
bitten this project repeatedly (see `bd` notes on three suites with three different
versions of it): a rate that holds steady while the denominator grows is not the same
result as a rate that holds steady, and neither is a regression here — but only
because the numerators moved with it.

## How we would know this was wrong

Three tests, each verified to **fail** with its fix reverted:

- `class_implements_is_a_heritage_clause_not_a_name` (`tsr-parser`), asserting both
  directions of the ambiguity.
- `a_named_class_expressions_name_is_visible_only_inside_it` (`tsr-binder`).
- `a_second_export_default_is_not_a_duplicate_identifier` (`tsr-binder`), asserting
  the code is 2528, that there is one diagnostic per declaration, and that the
  message takes no argument.

The TS2528 equivalence argument is the weakest link: it is a claim about what can be
named `default`, and it is the thing to re-check if a case ever reports TS2528 where
upstream reports a duplicate identifier.

## Measured effect

| | over-reports | `diagnostics` | `binder_symbols` | `parser_typescript` |
|---|---|---|---|---|
| after ADR-0025 | 34 | 79/5,488 | 8,282/8,449 | 4,999/5,031 |
| `implements` is a clause | 29 | 79/5,488 | 8,284/8,451 | **5,000**/5,031 |
| class expression names + TS2528 | **14** | **80**/5,488 | 8,284/8,451 | 5,000/5,031 |

`ClassDeclaration` 7 → 0, `ClassExpression` 8 → 0, and `ExportAssignment` (2) and
`FunctionDeclaration` (2) went to zero with them — they were the other declarations
involved in the same default-export collisions.

Perf by the method [`performance.md`](../architecture/performance.md) prescribes,
three readings of each, same session, at load 0.11:

| | parse+bind `checker.ts` | RSS |
|---|---|---|
| baseline (`a8c6b97`) | 26.03 / 25.88 / 25.62 ms | 15,052 / 15,052 / 14,848 KiB |
| with these changes | 25.53 / 25.77 / 25.47 ms | 15,048 / 14,848 / 14,836 KiB |

Neutral to slightly faster on both; the RSS ranges overlap, as expected given the
1.7% spread documented for that measurement.

## What is left

14 over-reports, and they are 7 cases seen twice — `ModuleDeclaration` (5) and
`VariableDeclaration` (5) are the same five `export =` cases counted at two
positions each, plus `compiler/intTypeCheck` twice and two singletons. `bd
tsr-y4u.19`. From 4,723 at the start of the session to 14, none of which is a class
or an alias.
