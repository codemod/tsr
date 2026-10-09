# ADR-0052: A print-time decision is recorded as a plan at the mint and made by the site renderer; the baked text stays the site-free print

- **Status:** Accepted. Built in `crates/tsr-checker/src/objects.rs`
  (`PrintedSlot::of_declaration`, `reuse_at_print`) and
  `crates/tsr-checker/src/printing.rs` (`deferred_member_text_at`,
  `deferred_conditional_text`, `conditional_type_text`). Their producers and
  dispatch are diffs in other lanes' files:
  `docs/parity/notes/r6-lazytext-spread-members.diff` (spreads.rs) and
  `docs/parity/notes/r6-lazytext-conditional-text.diff` (declared.rs,
  checker.rs).
- **Date:** 2026-10-09
- **Issue:** `bd tsr-2zk.1138` (`.1120`, `.1135`), lane r6-lazytext.
  Measurements: [`docs/parity/notes/r6-lazytext.md`](../parity/notes/r6-lazytext.md).
- **Pinned upstream:** `vendor/typescript-go` @ `5b1047d`.
- **Relates to:** [ADR-0050](0050-mapped-symbol-types-are-read-lazily-and-mapped-prints-truncate.md)
  (its alternative 1), [ADR-0043](0043-a-type-must-be-renderable-differently-at-different-sites.md).

## The forcing constraint

Native decides how to print a type inside `typeToTypeNode`, when something
prints it. The port computes a type's text when the type is minted
(`TypeData::Named` and the other text-carrying variants, `types.rs`), and
the checker reads that text through `&self` (`Checker::type_to_string`) and
through store-level composite printers (`unions.rs`, `intersections.rs`).
Any decision native makes at print that this port bakes at the mint runs
for every minted type, printed or not.

Two measured, lossless parity diffs were held for that reason alone:

- `r6-nodereuse-property-slot-spreads.diff`: +19 types, 0 lost; domain-model
  Ir +0.12%. `addPropertyToElementList`'s `serializeTypeForDeclaration`
  reuse (`nodebuilderimpl.go:2486`, `:2181`) ran for 1,248 spread members on
  domain-model at their mint, with about one `resolve_name` per entity name.
- `r5-mapped6-conditional-typed-print.diff`: +5 types; domain-model Ir
  +0.153%. `conditionalTypeToTypeNode`'s branch reads
  (`nodebuilderimpl.go:2916`) ran at every deferred conditional's mint.

## The decision

**The baked text stays exactly what it was: the type's site-free print.**
Nothing is made lazy in the store, and every `&self` or store-level reader
keeps reading the same text it read before.

**A decision native makes at print is recorded at the mint as a plan, never
as its result, and the site renderer (`Checker::type_to_string_at`) makes it
when it prints.** A plan holds the inputs the decision needs and nothing
computed from them:

| plan | where it lives (owner) | key | made by |
|---|---|---|---|
| spread member reuse | `PrintedSlot`'s second field, in the property held by its image's `anonymous_properties` entry | the property's declaration (`origin`) and the displayed type | `deferred_member_text_at`, which asks `reused_property_type_text(origin, displayed, None)` |
| deferred conditional's typed parts | the existing mint captures, `conditional_inference_nodes` / `mapped_conditionals` (private Checker tables keyed by the minted `TypeId`) | the conditional node, the flattened alias bindings and the mapped-template flag (`type_literal_key`'s identity) | `deferred_conditional_text`, under the captured frames |

**What is cached: nothing new.** No print result is stored. Each print asks
again, as each native `typeToTypeNode` does, and reuses only the ordinary
caches the decision already reads (type-node types, symbol types, the
visitor's own memo).

**What is invalidated: nothing, by construction.** A plan is written once,
complete, when its owner is minted, and is never mutated. A producer that
changes a property's type writes a new `PrintedSlot`, which carries no plan;
widening and regularization clone an unchanged property and keep its plan,
so a widened image asks the same reuse. The conditional captures are the
mint's own, unchanged.

**Publication states.** A plan is absent (the type prints its baked text) or
complete. There is no provisional state: a re-entrant print of the same
identity is caught by the renderer's visiting set (`rendering_composites`)
and keeps the baked text (an on-demand accessor slot still elides to `any`,
as createAnonymousTypeNode's visited check does).

**Receiver and alias context.** The spread reuse is asked site-free, as the
mint asked it: with the site, the visitor declines names the site cannot
reach, which native's typeToString still writes as declared
(`declarationEmitComputedPropertyNameSymbol1`). The conditional's parts are
read under the mint's captured frames with the print site's frames set
aside, and printed site-free. An alias name or a union origin still wins
over a conditional's typed print, because the dispatch sits where the
renderer used to read the baked text, after those arms.

**Expensive work boundary.** The reuse visitor and the four part reads run
only inside the site renderer. The renderer is also called during checking
on domain-model (`type_to_string_at_worker`, 8.5 M Ir inclusive), so the
work is not zero there: the stacked items measure +0.0075% Ir, inside the
same binary's run-to-run spread (about 0.05%).

## Alternatives

1. **Lazy baked text, forced on first read (ADR-0050 alternative 1).**
   Native's model. Rejected now: every reader must be able to force, which
   makes `Checker::type_to_string` and the store-level composite printers
   `&mut`, a refactor across five owners' files; two semantic tests read
   text (`text == "{}"`, `unions.rs`, `relater.rs`); and for its one named
   target, the mapped member print, the measured ceiling is about −0.03%
   on domain-model, because the check reads those members anyway
   (r6-lazytext.md §3). It wins once the printers are `&mut` for another
   reason, or on a project that mints resolved mapped objects whose
   members the check never reads.
2. **Cache the print-time result per `(TypeId, site)`.** Rejected: native
   keeps no such cache, a site-dependent result needs the site (or its
   scope chain) in the key, and nothing measured prints the same planned
   type often enough to pay for the table.
3. **Keep deciding at the mint (the held diffs).** Rejected: +0.12% and
   +0.153% domain-model Ir for no change in any printed line.
4. **Decide at print with the site for the spread reuse.** Measured: 4 of
   the 19 lines stay WRONG (`[Foo.sym]` read from a file that does not
   import `Foo`). Wins if the visitor learns to write an inaccessible
   name as declared, as native's tracker does.

## Consequences

- +23 type lines (19 spread members, 4 conditionals), 0 lost on either
  dump, at Ir within noise.
- A **site-free reader** (a diagnostic message, a composite's baked text)
  still prints the undecided form: the unreused spread member, the written
  conditional. That was the base's print; no diagnostic in the corpus
  changed under the held diffs either.
- A planned type nested in a text that is **baked by its own mint** is not
  re-printed: `mappedTypeAsClauses` 0:106 holds a conditional inside a
  generic mapped type's `as` clause under `keyof`, both baked at their mint.
- A deferred conditional printed after its resolution completed can read
  parts the mint could not: `circularTypeArgumentsLocalAndOuterNoCrash1`
  0:1 and `recursiveConditionalTypes` 0:8 print a different wrong text
  (native prints the circularity result for both).
- Each print pays for the decision again.

## How we would know we were wrong

- A corpus line where native prints a planned decision through a reader
  this port answers site-free (a diagnostic message naming a spread
  object or a deferred conditional). Then that reader needs the site
  renderer, or alternative 1.
- A print-time read that differs from the read the mint would have made,
  beyond the circularity lines above. That means a part's evaluation
  depends on state the plan did not capture, and the plan's key is
  incomplete.
- A whole-project profile where `deferred_member_text_at` or
  `deferred_conditional_text` shows above noise. Then alternative 2's
  cache, keyed on what the decision actually reads, is due.
- Falsifier tests (in the diffs):
  `print_time_spread_members::a_spread_member_reuses_its_declaration_when_printed` and
  `print_time_conditional_text::a_deferred_conditional_prints_its_typed_branches_when_printed`.
